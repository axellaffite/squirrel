mod common;

use sqlx::PgPool;
use squirrel_core::NodeName;
use squirrel_db::nodes::{self, NodeChange};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::uploads::{self, NewUpload};
use squirrel_db::{DbError, quota, trash};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_owner_lock_does_not_block_rows_referencing_the_user(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let mut holder = pool.begin().await.unwrap();
    quota::lock_owner(&mut holder, alice.id).await.unwrap();

    let mut other = pool.acquire().await.unwrap();
    sqlx::query("SET statement_timeout = '1s'")
        .execute(&mut *other)
        .await
        .unwrap();
    let now = OffsetDateTime::now_utc();
    let insert = sqlx::query(
        "INSERT INTO sessions (id, token_hash, user_id, csrf_token, expires_at, absolute_expires_at) \
         VALUES (gen_random_uuid(), 'lock-test', $1, 'csrf', $2, $2)",
    )
    .bind(alice.id)
    .bind(now + Duration::hours(1))
    .execute(&mut *other)
    .await;
    assert!(insert.is_ok(), "session insert blocked: {insert:?}");
    holder.rollback().await.unwrap();
    other.close().await.unwrap();

    // The repository path works too once the lock is released.
    sessions::create(
        &pool,
        &NewSession {
            user_id: alice.id,
            token_hash: b"after",
            csrf_token: "csrf",
            user_agent: None,
            expires_at: now + Duration::hours(1),
            absolute_expires_at: now + Duration::hours(2),
        },
    )
    .await
    .unwrap();
}

fn move_into(target: Uuid) -> NodeChange {
    NodeChange {
        name: None,
        parent_id: Some(target),
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn concurrent_crossed_moves_cannot_create_a_cycle(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    for round in 0..10 {
        let a = common::raw_folder(&pool, alice.id, files.id, &format!("a{round}")).await;
        let b = common::raw_folder(&pool, alice.id, files.id, &format!("b{round}")).await;
        let (into_b, into_a) = (move_into(b), move_into(a));
        let (ab, ba) = tokio::join!(
            nodes::update(&pool, alice.id, a, &into_b),
            nodes::update(&pool, alice.id, b, &into_a),
        );
        let failures = [&ab, &ba]
            .into_iter()
            .filter(|r| matches!(r, Err(DbError::MoveIntoSelf)))
            .count();
        assert_eq!(failures, 1, "round {round}: {ab:?} / {ba:?}");
        assert_eq!(
            nodes::path_to(&pool, alice.id, a).await.unwrap()[0].id,
            files.id
        );
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn parallel_upload_sessions_never_exceed_the_quota(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    common::set_quota(&pool, alice.id, Some(100)).await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let (pool, owner, parent) = (pool.clone(), alice.id, files.id);
        tasks.spawn(async move {
            let name = NodeName::parse("big.bin").unwrap();
            let sha = common::hash('f');
            let new = NewUpload {
                owner_id: owner,
                parent_id: parent,
                name: &name,
                sha256: &sha,
                size: 30,
                chunk_size: 10,
                chunk_count: 3,
                expires_at: OffsetDateTime::now_utc() + Duration::hours(1),
                max_open: 100,
            };
            uploads::create(&pool, &new).await
        });
    }
    let results = tasks.join_all().await;
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 3);
    assert!(
        results
            .iter()
            .all(|r| r.is_ok() || matches!(r, Err(DbError::QuotaExceeded)))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trash_and_restore_races_leave_a_consistent_tree(pool: PgPool) {
    let alice = common::admin(&pool).await;
    let files = common::root(&pool, alice.id, "Files").await;
    let dir = common::raw_folder(&pool, alice.id, files.id, "dir").await;
    let inner = common::raw_file(&pool, alice.id, dir, "a.txt", 'a', 1).await;
    trash::trash(&pool, alice.id, dir).await.unwrap();

    for _ in 0..10 {
        let (restored, trashed) = tokio::join!(
            trash::restore(&pool, alice.id, dir),
            trash::trash(&pool, alice.id, dir),
        );
        let live = nodes::find_live(&pool, alice.id, dir)
            .await
            .unwrap()
            .is_some();
        let inner_live = nodes::find_live(&pool, alice.id, inner)
            .await
            .unwrap()
            .is_some();
        assert_eq!(live, inner_live, "the batch moves as a whole");
        let entries = trash::list(&pool, alice.id, None, 10).await.unwrap().items;
        assert_eq!(entries.len(), usize::from(!live));
        // Restore first then trash again, or trash refused (not live) then restore.
        match (&restored, &trashed) {
            (Ok(_), Ok(())) => assert!(!live),
            (Ok(_), Err(_)) => assert!(live),
            other => panic!("unexpected outcome {other:?}"),
        }
        if live {
            trash::trash(&pool, alice.id, dir).await.unwrap();
        }
    }
}
