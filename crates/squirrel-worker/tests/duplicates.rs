mod common;

use std::time::Duration;

use common::{Fixture, admin, invited, job, queued, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_media::testutil::scene_jpeg;
use squirrel_worker::duplicates_job::group_owner;
use squirrel_worker::{backfill, metadata, process, thumbnails, visual_hash_job};
use uuid::Uuid;

const BUDGET: Duration = Duration::from_secs(30);

/// Stores `bytes` as one of `owner`'s photos and runs its media steps.
async fn photo(fx: &Fixture, owner: Uuid, name: &str, bytes: &[u8]) -> Uuid {
    let parent = root(fx.pool(), owner, "Photos").await;
    let node = fx.store_file(owner, parent, name, bytes).await;
    let sha = node.blob_sha256.clone().unwrap();
    let ctx = &fx.ctx;
    process::run(ctx, job("process_media", json!({"node_id": node.id})))
        .await
        .unwrap();
    thumbnails::run(ctx, job("thumbnails", json!({"sha256": sha})))
        .await
        .unwrap();
    metadata::run(ctx, job("extract_metadata", json!({"sha256": sha})))
        .await
        .unwrap();
    node.id
}

/// `owner`'s groups as sorted member lists, with whether each is resolved.
async fn groups(pool: &PgPool, owner: Uuid) -> Vec<(Vec<Uuid>, bool)> {
    let rows: Vec<(Vec<Uuid>, bool)> = sqlx::query_as(
        "SELECT array_agg(m.node_id ORDER BY m.node_id), g.resolved_at IS NOT NULL \
           FROM duplicate_groups g JOIN duplicate_members m ON m.group_id = g.id \
          WHERE g.owner_id = $1 GROUP BY g.id ORDER BY g.id",
    )
    .bind(owner)
    .fetch_all(pool)
    .await
    .unwrap();
    rows
}

fn sorted(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids
}

async fn hidden(pool: &PgPool, node: Uuid) -> bool {
    sqlx::query_scalar("SELECT hidden_at IS NOT NULL FROM assets WHERE node_id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Hides every member of `owner`'s only group but `keep`, as a "keep" does.
async fn resolve(pool: &PgPool, owner: Uuid, keep: Uuid) {
    sqlx::query(
        "UPDATE assets SET hidden_at = now(), hidden_reason = 'duplicate' \
          WHERE owner_id = $1 AND node_id <> $2 \
            AND node_id IN (SELECT node_id FROM duplicate_members)",
    )
    .bind(owner)
    .bind(keep)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE duplicate_groups SET resolved_at = now(), kept_node_id = $2 WHERE owner_id = $1",
    )
    .bind(owner)
    .bind(keep)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn copies_of_one_photo_are_grouped_and_other_photos_are_not(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let big = photo(&fx, alice.id, "big.jpg", &scene_jpeg(1, 1600, 1200, 92)).await;
    let small = photo(&fx, alice.id, "small.jpg", &scene_jpeg(1, 800, 600, 60)).await;
    photo(&fx, alice.id, "other.jpg", &scene_jpeg(2, 1600, 1200, 92)).await;
    let jobs = queued(fx.pool()).await;
    assert!(
        jobs.iter()
            .any(|(kind, _, payload)| kind == "group_duplicates"
                && payload["owner_id"] == json!(alice.id)),
        "{jobs:?}"
    );

    let run = group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert_eq!((run.checked, run.groups_created), (3, 1));
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![big, small]), false)]
    );
    // Nothing is hidden without the user's decision.
    assert!(!hidden(fx.pool(), big).await && !hidden(fx.pool(), small).await);
    // Looked at once: a second run does nothing.
    let again = group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert_eq!(again.checked, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn groups_never_mix_users(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let bob = invited(fx.pool(), &alice, "bob").await;
    let mine = photo(&fx, alice.id, "a.jpg", &scene_jpeg(4, 1200, 900, 90)).await;
    let theirs = photo(&fx, bob.id, "b.jpg", &scene_jpeg(4, 1000, 750, 70)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    group_owner(fx.pool(), bob.id, BUDGET).await.unwrap();
    assert_eq!(groups(fx.pool(), alice.id).await, vec![]);
    assert_eq!(groups(fx.pool(), bob.id).await, vec![]);
    // Each user's own second copy groups with their own photo only.
    let mine2 = photo(&fx, alice.id, "a2.jpg", &scene_jpeg(4, 900, 675, 65)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![mine, mine2]), false)]
    );
    assert_eq!(groups(fx.pool(), bob.id).await, vec![]);
    let _ = theirs;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn photos_more_than_a_day_apart_are_not_grouped(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let first = photo(&fx, alice.id, "a.jpg", &scene_jpeg(5, 1200, 900, 90)).await;
    let later = photo(&fx, alice.id, "b.jpg", &scene_jpeg(5, 1000, 750, 70)).await;
    sqlx::query("UPDATE assets SET taken_at = taken_at + interval '25 hours' WHERE node_id = $1")
        .bind(later)
        .execute(fx.pool())
        .await
        .unwrap();
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert_eq!(groups(fx.pool(), alice.id).await, vec![]);
    let _ = first;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_new_copy_reopens_a_resolved_group_and_stays_visible(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(6, 1200, 900, 90)).await;
    let b = photo(&fx, alice.id, "b.jpg", &scene_jpeg(6, 1000, 750, 70)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    resolve(fx.pool(), alice.id, a).await;
    assert!(hidden(fx.pool(), b).await);

    let c = photo(&fx, alice.id, "c.jpg", &scene_jpeg(6, 800, 600, 60)).await;
    let run = group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert_eq!((run.joined, run.groups_created), (1, 0));
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![a, b, c]), false)],
        "reopened"
    );
    let kept: Option<Uuid> = sqlx::query_scalar("SELECT kept_node_id FROM duplicate_groups")
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(kept, None, "open again: no photo counts as kept");
    assert!(
        !hidden(fx.pool(), c).await,
        "the new copy waits for a decision"
    );
    assert!(hidden(fx.pool(), b).await, "hidden photos stay hidden");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn trashed_and_deleted_photos_leave_their_group(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(7, 1200, 900, 90)).await;
    let b = photo(&fx, alice.id, "b.jpg", &scene_jpeg(7, 1000, 750, 70)).await;
    let c = photo(&fx, alice.id, "c.jpg", &scene_jpeg(7, 800, 600, 60)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    resolve(fx.pool(), alice.id, a).await;

    // A hidden photo trashed leaves the group and is no longer hidden.
    squirrel_db::trash::trash(fx.pool(), alice.id, c)
        .await
        .unwrap();
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![a, b]), true)]
    );
    assert!(!hidden(fx.pool(), c).await);

    // Deleting the kept photo leaves the hidden one hidden, in its group.
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(a)
        .execute(fx.pool())
        .await
        .unwrap();
    assert_eq!(groups(fx.pool(), alice.id).await, vec![(vec![b], true)]);
    assert!(hidden(fx.pool(), b).await);
    let kept: Option<Uuid> = sqlx::query_scalar("SELECT kept_node_id FROM duplicate_groups")
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(kept, None);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_group_left_with_one_visible_photo_is_deleted(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(8, 1200, 900, 90)).await;
    let b = photo(&fx, alice.id, "b.jpg", &scene_jpeg(8, 1000, 750, 70)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    squirrel_db::trash::trash(fx.pool(), alice.id, b)
        .await
        .unwrap();
    assert_eq!(groups(fx.pool(), alice.id).await, vec![]);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM duplicate_groups")
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);
    let _ = a;
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn photos_hashed_before_get_a_hash_from_the_backfill(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(9, 1200, 900, 90)).await;
    let sha: String = sqlx::query_scalar("SELECT sha256 FROM assets WHERE node_id = $1")
        .bind(a)
        .fetch_one(fx.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE media SET phash = NULL")
        .execute(fx.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();

    backfill::run(&fx.ctx, job("backfill", json!({})))
        .await
        .unwrap();
    let jobs = queued(fx.pool()).await;
    assert!(
        jobs.iter()
            .any(|(kind, _, payload)| kind == "visual_hash" && payload["sha256"] == json!(sha)),
        "{jobs:?}"
    );
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    visual_hash_job::run(&fx.ctx, job("visual_hash", json!({"sha256": sha})))
        .await
        .unwrap();
    let phash: Option<i64> = sqlx::query_scalar("SELECT phash FROM media WHERE sha256 = $1")
        .bind(&sha)
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert!(phash.is_some());
    let jobs = queued(fx.pool()).await;
    assert!(
        jobs.iter().any(|(kind, _, _)| kind == "group_duplicates"),
        "{jobs:?}"
    );
}

/// Trashes `node` in a transaction left open (the caller commits it).
async fn trash_uncommitted(
    pool: &PgPool,
    node: Uuid,
) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(node)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

/// Waits until some session of this database waits for a lock (a request
/// blocked behind an open transaction), so the race is exercised, not skipped.
async fn until_blocked(pool: &PgPool) {
    for _ in 0..600 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity \
              WHERE datname = current_database() AND wait_event_type = 'Lock'",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("nothing ever waited for the open transaction");
}

async fn only_group(pool: &PgPool, owner: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT id FROM duplicate_groups WHERE owner_id = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn keeping_while_a_photo_is_trashed_never_hides_the_trashed_photo(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(12, 1200, 900, 90)).await;
    let b = photo(&fx, alice.id, "b.jpg", &scene_jpeg(12, 1000, 750, 70)).await;
    let c = photo(&fx, alice.id, "c.jpg", &scene_jpeg(12, 800, 600, 60)).await;
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    let group = only_group(fx.pool(), alice.id).await;

    let trash = trash_uncommitted(fx.pool(), b).await;
    let pool = fx.pool().clone();
    let keep = tokio::spawn(async move {
        squirrel_db::duplicates::review::keep(&pool, alice.id, group, a).await
    });
    until_blocked(fx.pool()).await;
    trash.commit().await.unwrap();
    let kept = keep.await.unwrap().unwrap();
    assert!(matches!(kept, squirrel_db::people::Edit::Done(_)));

    assert!(
        !hidden(fx.pool(), b).await,
        "the trashed photo left the group first"
    );
    assert!(hidden(fx.pool(), c).await);
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![a, c]), true)]
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_trashed_while_grouping_is_not_grouped(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    photo(&fx, alice.id, "a.jpg", &scene_jpeg(13, 1200, 900, 90)).await;
    let b = photo(&fx, alice.id, "b.jpg", &scene_jpeg(13, 1000, 750, 70)).await;

    let trash = trash_uncommitted(fx.pool(), b).await;
    let pool = fx.pool().clone();
    let run = tokio::spawn(async move { group_owner(&pool, alice.id, BUDGET).await });
    until_blocked(fx.pool()).await;
    trash.commit().await.unwrap();
    run.await.unwrap().unwrap();
    assert_eq!(groups(fx.pool(), alice.id).await, vec![]);
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM duplicate_members")
        .fetch_one(fx.pool())
        .await
        .unwrap();
    assert_eq!(members, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_hidden_photo_in_no_group_is_shown_again(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let a = photo(&fx, alice.id, "a.jpg", &scene_jpeg(14, 1200, 900, 90)).await;
    sqlx::query("UPDATE assets SET hidden_at = now(), hidden_reason = 'duplicate'")
        .execute(fx.pool())
        .await
        .unwrap();
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    assert!(!hidden(fx.pool(), a).await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_a_user_deletes_their_groups(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let bob = invited(fx.pool(), &alice, "bob").await;
    let a = photo(&fx, bob.id, "a.jpg", &scene_jpeg(15, 1200, 900, 90)).await;
    photo(&fx, bob.id, "b.jpg", &scene_jpeg(15, 1000, 750, 70)).await;
    photo(&fx, bob.id, "c.jpg", &scene_jpeg(15, 800, 600, 60)).await;
    let mine = photo(&fx, alice.id, "m.jpg", &scene_jpeg(15, 1200, 900, 90)).await;
    let mine2 = photo(&fx, alice.id, "m2.jpg", &scene_jpeg(15, 900, 675, 70)).await;
    group_owner(fx.pool(), bob.id, BUDGET).await.unwrap();
    group_owner(fx.pool(), alice.id, BUDGET).await.unwrap();
    resolve(fx.pool(), bob.id, a).await;

    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(bob.id)
        .execute(fx.pool())
        .await
        .unwrap();
    let left: Vec<Uuid> = sqlx::query_scalar("SELECT owner_id FROM duplicate_groups")
        .fetch_all(fx.pool())
        .await
        .unwrap();
    assert_eq!(left, vec![alice.id]);
    assert_eq!(
        groups(fx.pool(), alice.id).await,
        vec![(sorted(vec![mine, mine2]), false)]
    );
}
