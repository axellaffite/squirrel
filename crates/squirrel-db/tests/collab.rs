mod common;

use common::{admin, hash, raw_file, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::collab;

async fn ref_count(pool: &PgPool, fill: char) -> i64 {
    sqlx::query_scalar("SELECT ref_count FROM blobs WHERE sha256 = $1")
        .bind(hash(fill).as_str())
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn point_at(pool: &PgPool, node: uuid::Uuid, fill: char) {
    sqlx::query("UPDATE nodes SET blob_sha256 = $2 WHERE id = $1")
        .bind(node)
        .bind(hash(fill).as_str())
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn start_keeps_the_epoch_for_the_same_blob_and_renews_it_otherwise(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let note = raw_file(&pool, alice.id, files, "note.txt", 'a', 3).await;
    raw_file(&pool, alice.id, files, "other.txt", 'b', 3).await;

    let first = collab::start(&pool, note, hash('a').as_str(), b"s1")
        .await
        .unwrap()
        .unwrap();
    let again = collab::start(&pool, note, hash('a').as_str(), b"other")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.epoch, first.epoch);
    assert_eq!(again.state, b"s1");

    point_at(&pool, note, 'b').await;
    let changed = collab::start(&pool, note, hash('b').as_str(), b"s2")
        .await
        .unwrap()
        .unwrap();
    assert_ne!(changed.epoch, first.epoch);
    assert_eq!(changed.state, b"s2");
    assert_eq!(
        collab::find(&pool, note).await.unwrap().unwrap().epoch,
        changed.epoch
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn start_from_a_blob_the_file_left_writes_nothing(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let note = raw_file(&pool, alice.id, files, "note.txt", 'a', 3).await;
    raw_file(&pool, alice.id, files, "other.txt", 'b', 3).await;
    let saved = collab::start(&pool, note, hash('a').as_str(), b"s1")
        .await
        .unwrap()
        .unwrap();
    // A save moved the file to blob 'b' and its state with it.
    point_at(&pool, note, 'b').await;
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        collab::save_state(&mut conn, note, saved.epoch, hash('b').as_str(), b"s2")
            .await
            .unwrap()
    );

    // An opener that read the file before the save rebuilds from blob 'a'.
    let late = collab::start(&pool, note, hash('a').as_str(), b"old")
        .await
        .unwrap();
    assert!(late.is_none());
    let stored = collab::find(&pool, note).await.unwrap().unwrap();
    assert_eq!((stored.epoch, stored.state), (saved.epoch, b"s2".to_vec()));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn save_state_needs_the_current_epoch(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let note = raw_file(&pool, alice.id, files, "note.txt", 'a', 3).await;
    let doc = collab::start(&pool, note, hash('a').as_str(), b"s1")
        .await
        .unwrap()
        .unwrap();

    let mut conn = pool.acquire().await.unwrap();
    assert!(
        collab::save_state(&mut conn, note, doc.epoch, hash('c').as_str(), b"s3")
            .await
            .unwrap()
    );
    assert!(
        !collab::save_state(
            &mut conn,
            note,
            uuid::Uuid::now_v7(),
            hash('d').as_str(),
            b"x"
        )
        .await
        .unwrap()
    );
    let stored = collab::find(&pool, note).await.unwrap().unwrap();
    assert_eq!(
        (stored.blob_sha256, stored.state),
        (hash('c').as_str().to_owned(), b"s3".to_vec())
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn replace_blob_moves_the_reference_only_from_the_expected_blob(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let note = raw_file(&pool, alice.id, files, "note.txt", 'a', 3).await;
    // A second file makes blob 'b' exist.
    raw_file(&pool, alice.id, files, "other.txt", 'b', 5).await;

    let mut conn = pool.acquire().await.unwrap();
    let stale = collab::replace_blob(&mut conn, alice.id, note, hash('c').as_str(), &hash('b'), 5)
        .await
        .unwrap();
    assert!(stale.is_none());

    let node = collab::replace_blob(&mut conn, alice.id, note, hash('a').as_str(), &hash('b'), 5)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (node.blob_sha256.as_deref(), node.size),
        (Some(hash('b').as_str()), 5)
    );
    assert_eq!(ref_count(&pool, 'a').await, 0);
    assert_eq!(ref_count(&pool, 'b').await, 2);

    raw_trash(&pool, note).await;
    let trashed =
        collab::replace_blob(&mut conn, alice.id, note, hash('b').as_str(), &hash('a'), 3)
            .await
            .unwrap();
    assert!(trashed.is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_the_file_deletes_its_state(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let note = raw_file(&pool, alice.id, files, "note.txt", 'a', 3).await;
    collab::start(&pool, note, hash('a').as_str(), b"s1")
        .await
        .unwrap()
        .unwrap();
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(note)
        .execute(&pool)
        .await
        .unwrap();
    assert!(collab::find(&pool, note).await.unwrap().is_none());
}
