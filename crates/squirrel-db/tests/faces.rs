mod common;

use common::{admin, invited, photo, photo_of_blob, raw_trash, root};
use sqlx::PgPool;
use squirrel_db::faces::{self, NewFace};
use squirrel_db::ml_settings;
use time::macros::datetime;
use uuid::Uuid;

const MODEL: &str = "yunet-sface-2021dec";

fn face(x: f32) -> NewFace {
    let mut embedding = vec![0.0; 128];
    embedding[0] = 1.0;
    NewFace {
        bbox: [x, 0.2, 0.2, 0.3],
        landmarks: [x; 10],
        score: 0.9,
        embedding,
    }
}

async fn sha(pool: &PgPool, node: Uuid) -> String {
    sqlx::query_scalar("SELECT blob_sha256 FROM nodes WHERE id = $1")
        .bind(node)
        .fetch_one(pool)
        .await
        .unwrap()
}

struct Two {
    alice: Uuid,
    bob: Uuid,
    sha: String,
    alice_node: Uuid,
}

/// Alice and Bob each hold a live photo of one blob.
async fn shared_blob(pool: &PgPool) -> Two {
    let alice = admin(pool).await;
    let bob = invited(pool, &alice, "bob").await;
    let a_root = root(pool, alice.id, "Photos").await.id;
    let b_root = root(pool, bob.id, "Photos").await.id;
    let when = datetime!(2024-01-01 10:00);
    let alice_node = photo(pool, alice.id, a_root, "a.jpg", when).await;
    let sha = sha(pool, alice_node).await;
    photo_of_blob(pool, bob.id, b_root, "b.jpg", &sha, when).await;
    Two {
        alice: alice.id,
        bob: bob.id,
        sha,
        alice_node,
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn scans_are_per_owner_and_only_for_owners_with_faces_on(pool: PgPool) {
    let t = shared_blob(&pool).await;
    ml_settings::set_faces_enabled(&pool, t.bob, false)
        .await
        .unwrap();
    assert_eq!(
        faces::owners_to_scan(&pool, &t.sha, MODEL).await.unwrap(),
        [t.alice]
    );
    assert!(faces::wanted(&pool, &t.sha).await.unwrap());
    assert!(
        faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[face(0.1), face(0.5)])
            .await
            .unwrap()
    );
    assert!(
        !faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[face(0.1)])
            .await
            .unwrap(),
        "a retry writes nothing"
    );
    assert!(
        !faces::store_scan(&pool, t.bob, &t.sha, MODEL, &[face(0.1)])
            .await
            .unwrap(),
        "faces off"
    );
    assert_eq!(
        faces::faces_of(&pool, t.alice, &t.sha).await.unwrap().len(),
        2
    );
    assert!(
        faces::faces_of(&pool, t.bob, &t.sha)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        faces::owners_to_scan(&pool, &t.sha, MODEL)
            .await
            .unwrap()
            .is_empty()
    );
    ml_settings::set_faces_enabled(&pool, t.alice, false)
        .await
        .unwrap();
    assert!(!faces::wanted(&pool, &t.sha).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_without_faces_still_records_its_scan(pool: PgPool) {
    let t = shared_blob(&pool).await;
    assert!(
        faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[])
            .await
            .unwrap()
    );
    assert_eq!(
        faces::scan_count(&pool, t.alice, &t.sha).await.unwrap(),
        Some(0)
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn faces_go_with_the_owners_last_copy_of_the_photo(pool: PgPool) {
    let t = shared_blob(&pool).await;
    let photos: Uuid = sqlx::query_scalar("SELECT parent_id FROM nodes WHERE id = $1")
        .bind(t.alice_node)
        .fetch_one(&pool)
        .await
        .unwrap();
    let copy = photo_of_blob(
        &pool,
        t.alice,
        photos,
        "copy.jpg",
        &t.sha,
        datetime!(2024-01-01 10:00),
    )
    .await;
    faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[face(0.1)])
        .await
        .unwrap();
    faces::store_scan(&pool, t.bob, &t.sha, MODEL, &[face(0.1)])
        .await
        .unwrap();
    // Trashed photos keep theirs; deleting one of two copies keeps them too.
    raw_trash(&pool, copy).await;
    delete_asset(&pool, copy).await;
    assert_eq!(
        faces::faces_of(&pool, t.alice, &t.sha).await.unwrap().len(),
        1
    );
    delete_asset(&pool, t.alice_node).await;
    assert!(
        faces::faces_of(&pool, t.alice, &t.sha)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        faces::scan_count(&pool, t.alice, &t.sha).await.unwrap(),
        None
    );
    assert_eq!(
        faces::faces_of(&pool, t.bob, &t.sha).await.unwrap().len(),
        1,
        "Bob's stay"
    );
}

async fn delete_asset(pool: &PgPool, node: Uuid) {
    sqlx::query("DELETE FROM assets WHERE node_id = $1")
        .bind(node)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn people_and_faces_never_cross_owners(pool: PgPool) {
    let t = shared_blob(&pool).await;
    faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[face(0.1)])
        .await
        .unwrap();
    let alice_face = faces::faces_of(&pool, t.alice, &t.sha).await.unwrap()[0].id;
    let bob_person = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(bob_person)
        .bind(t.bob)
        .execute(&pool)
        .await
        .unwrap();
    let cross = sqlx::query("UPDATE faces SET person_id = $1 WHERE id = $2")
        .bind(bob_person)
        .bind(alice_face)
        .execute(&pool)
        .await;
    assert!(cross.is_err(), "Alice's face cannot belong to Bob's person");
    let alice_person = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id) VALUES ($1, $2)")
        .bind(alice_person)
        .bind(t.alice)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE faces SET person_id = $1 WHERE id = $2")
        .bind(alice_person)
        .bind(alice_face)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE people SET cover_face_id = $1 WHERE id = $2")
        .bind(alice_face)
        .bind(alice_person)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM faces WHERE id = $1")
        .bind(alice_face)
        .execute(&pool)
        .await
        .unwrap();
    let cover: Option<Uuid> = sqlx::query_scalar("SELECT cover_face_id FROM people WHERE id = $1")
        .bind(alice_person)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cover, None, "the cover is cleared, the person stays");
    let counts = faces::counts(&pool, t.alice).await.unwrap();
    assert_eq!((counts.faces, counts.people), (0, 1));
}

/// The photo was deleted while its faces were being computed: nothing is
/// stored for that owner.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_scan_of_a_photo_deleted_meanwhile_is_not_stored(pool: PgPool) {
    let t = shared_blob(&pool).await;
    delete_asset(&pool, t.alice_node).await;
    assert!(
        !faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[face(0.1)])
            .await
            .unwrap()
    );
    assert_eq!(
        faces::scan_count(&pool, t.alice, &t.sha).await.unwrap(),
        None
    );
    assert!(
        faces::store_scan(&pool, t.bob, &t.sha, MODEL, &[face(0.1)])
            .await
            .unwrap(),
        "Bob still has his copy"
    );
}

async fn restore_due(pool: &PgPool, owner: Uuid, node: Uuid) -> Vec<String> {
    let mut tx = pool.begin().await.unwrap();
    let restored = squirrel_db::trash::restore_in(&mut tx, owner, node, MODEL)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(restored.node.id, node);
    restored.faces_due
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_photo_restored_without_a_scan_is_due_for_faces(pool: PgPool) {
    let t = shared_blob(&pool).await;
    // Trashed while the blob was analysed: Alice's copy got no scan.
    raw_trash(&pool, t.alice_node).await;
    assert_eq!(
        restore_due(&pool, t.alice, t.alice_node).await,
        vec![t.sha.clone()]
    );

    faces::store_scan(&pool, t.alice, &t.sha, MODEL, &[])
        .await
        .unwrap();
    raw_trash(&pool, t.alice_node).await;
    assert!(
        restore_due(&pool, t.alice, t.alice_node).await.is_empty(),
        "scanned already"
    );

    ml_settings::set_faces_enabled(&pool, t.alice, false)
        .await
        .unwrap();
    raw_trash(&pool, t.alice_node).await;
    assert!(
        restore_due(&pool, t.alice, t.alice_node).await.is_empty(),
        "faces off"
    );
}
