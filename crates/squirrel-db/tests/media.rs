mod common;

use common::{admin, hash, invited, raw_file, root};
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::assets::{self, AssetKind, NewAsset, TakenSource};
use squirrel_db::media::{self, MediaState, MetadataUpdate, Step};
use squirrel_db::nodes;
use time::macros::datetime;
use uuid::Uuid;

async fn states(pool: &PgPool, sha: &str) -> (MediaState, MediaState, MediaState) {
    let row = media::find(pool, sha).await.unwrap().unwrap();
    (row.metadata_state(), row.thumbs_state(), row.text_state())
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ensure_sets_the_initial_states_of_each_kind_once(pool: PgPool) {
    use MediaState::{Pending, Unsupported};
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    let cases = [
        ('a', MediaKind::Image, (Pending, Pending, Unsupported)),
        ('b', MediaKind::Pdf, (Unsupported, Pending, Pending)),
        ('c', MediaKind::Text, (Unsupported, Unsupported, Pending)),
    ];
    for (fill, kind, expected) in cases {
        raw_file(&pool, alice.id, files, &format!("{fill}.bin"), fill, 3).await;
        let sha = hash(fill);
        let mut conn = pool.acquire().await.unwrap();
        let row = media::ensure(&mut conn, sha.as_str(), kind).await.unwrap();
        assert_eq!(row.kind(), kind);
        assert_eq!(states(&pool, sha.as_str()).await, expected, "{kind:?}");
    }
    let sha = hash('a');
    media::set_state(
        &pool,
        sha.as_str(),
        Step::Thumbs,
        MediaState::Failed,
        Some("bad"),
    )
    .await
    .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let again = media::ensure(&mut conn, sha.as_str(), MediaKind::Image)
        .await
        .unwrap();
    assert_eq!(again.thumbs_state(), MediaState::Failed, "unchanged");
    assert_eq!(again.last_error.as_deref(), Some("bad"));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn metadata_and_thumbnails_are_recorded(pool: PgPool) {
    let alice = admin(&pool).await;
    let files = root(&pool, alice.id, "Files").await.id;
    raw_file(&pool, alice.id, files, "a.jpg", 'a', 3).await;
    let sha = hash('a');
    let mut conn = pool.acquire().await.unwrap();
    media::ensure(&mut conn, sha.as_str(), MediaKind::Image)
        .await
        .unwrap();
    let update = MetadataUpdate {
        exif_taken_at: Some(datetime!(2024-07-14 18:30:05)),
        exif_offset_min: Some(120),
        latitude: Some(48.85),
        longitude: Some(2.35),
        camera_make: Some("Google".into()),
        width: Some(32),
        height: Some(64),
        ..MetadataUpdate::default()
    };
    media::set_metadata(&pool, sha.as_str(), &update)
        .await
        .unwrap();
    let row = media::find(&pool, sha.as_str()).await.unwrap().unwrap();
    assert_eq!(row.metadata_state(), MediaState::Ready);
    assert_eq!(row.exif_taken_at, Some(datetime!(2024-07-14 18:30:05)));
    assert_eq!((row.width, row.height), (Some(32), Some(64)));
    assert!(
        media::thumbnails_for(&pool, &[sha.as_str().to_owned()])
            .await
            .unwrap()
            .is_empty()
    );

    media::set_thumbnails(&pool, sha.as_str(), 1, 30, 60, "LKO2?U%2Tw=w")
        .await
        .unwrap();
    let row = media::find(&pool, sha.as_str()).await.unwrap().unwrap();
    assert_eq!(row.thumbs_state(), MediaState::Ready);
    assert_eq!(
        (row.width, row.height, row.thumbs_version),
        (Some(30), Some(60), 1)
    );
    let thumbs = media::thumbnails_for(&pool, &[sha.as_str().to_owned(), "x".repeat(64)])
        .await
        .unwrap();
    assert_eq!(thumbs.len(), 1);
    assert_eq!(
        thumbs[sha.as_str()].blurhash.as_deref(),
        Some("LKO2?U%2Tw=w")
    );
}

fn asset(node_id: Uuid, owner_id: Uuid, fill: char) -> NewAsset {
    NewAsset {
        node_id,
        owner_id,
        sha256: hash(fill).as_str().to_owned(),
        kind: AssetKind::Photo,
        taken_at: datetime!(2026-11-01 10:00),
        taken_offset_min: Some(0),
        taken_source: TakenSource::Upload,
    }
}

async fn image_with_media(pool: &PgPool, owner: Uuid, name: &str, fill: char) -> Uuid {
    let files = root(pool, owner, "Files").await.id;
    let id = raw_file(pool, owner, files, name, fill, 3).await;
    let mut conn = pool.acquire().await.unwrap();
    media::ensure(&mut conn, hash(fill).as_str(), MediaKind::Image)
        .await
        .unwrap();
    id
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn assets_are_created_then_updated(pool: PgPool) {
    let alice = admin(&pool).await;
    let id = image_with_media(&pool, alice.id, "a.jpg", 'a').await;
    let mut conn = pool.acquire().await.unwrap();
    assets::upsert(&mut conn, &asset(id, alice.id, 'a'))
        .await
        .unwrap();
    let updated = NewAsset {
        kind: AssetKind::Screenshot,
        taken_at: datetime!(2024-07-14 18:30:05),
        taken_offset_min: Some(120),
        taken_source: TakenSource::Exif,
        ..asset(id, alice.id, 'a')
    };
    assets::upsert(&mut conn, &updated).await.unwrap();
    let row = assets::find(&pool, id).await.unwrap().unwrap();
    assert_eq!(
        (
            row.kind.as_str(),
            row.taken_at,
            row.taken_offset_min,
            row.taken_source.as_str()
        ),
        (
            "screenshot",
            datetime!(2024-07-14 18:30:05),
            Some(120),
            "exif"
        )
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn assets_follow_their_node_and_owner(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let id = image_with_media(&pool, alice.id, "a.jpg", 'a').await;
    let mut conn = pool.acquire().await.unwrap();
    // Another owner's node: nothing is written, and the database refuses it too.
    assert!(
        !assets::upsert(&mut conn, &asset(id, bob.id, 'a'))
            .await
            .unwrap()
    );
    let err = sqlx::query(
        "INSERT INTO assets (node_id, owner_id, sha256, kind, taken_at, taken_source) \
         VALUES ($1, $2, $3, 'photo', now(), 'upload')",
    )
    .bind(id)
    .bind(bob.id)
    .bind(hash('a').as_str())
    .execute(&pool)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("assets_node_fkey"), "{err}");

    assert!(
        assets::upsert(&mut conn, &asset(id, alice.id, 'a'))
            .await
            .unwrap()
    );
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        assets::find(&pool, id).await.unwrap().is_some(),
        "trash keeps it"
    );
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        assets::find(&pool, id).await.unwrap().is_none(),
        "deletion cascades"
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn deleting_the_blob_deletes_its_media(pool: PgPool) {
    let alice = admin(&pool).await;
    let id = image_with_media(&pool, alice.id, "a.jpg", 'a').await;
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM blobs WHERE sha256 = $1")
        .bind(hash('a').as_str())
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        media::find(&pool, hash('a').as_str())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn worker_lookups_span_owners_and_the_trash(pool: PgPool) {
    let alice = admin(&pool).await;
    let bob = invited(&pool, &alice, "bob").await;
    let mine = image_with_media(&pool, alice.id, "a.jpg", 'a').await;
    let theirs = image_with_media(&pool, bob.id, "same.jpg", 'a').await;
    sqlx::query("UPDATE nodes SET deleted_at = now(), trash_root = id WHERE id = $1")
        .bind(theirs)
        .execute(&pool)
        .await
        .unwrap();

    let nodes = assets::for_blob(&pool, hash('a').as_str()).await.unwrap();
    let mut found: Vec<(Uuid, Uuid, &str)> = nodes
        .iter()
        .map(|n| (n.node_id, n.owner_id, n.name.as_str()))
        .collect();
    found.sort();
    let mut expected = vec![(mine, alice.id, "a.jpg"), (theirs, bob.id, "same.jpg")];
    expected.sort();
    assert_eq!(found, expected);
    assert_eq!(nodes[0].mime.as_deref(), Some("text/plain"));

    let trashed = nodes::find_any(&pool, theirs).await.unwrap().unwrap();
    assert!(trashed.deleted_at.is_some());
    assert!(
        nodes::find_any(&pool, Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_missing_node_does_not_spoil_the_transaction(pool: PgPool) {
    let alice = admin(&pool).await;
    let gone = image_with_media(&pool, alice.id, "gone.jpg", 'a').await;
    let kept = {
        let files = root(&pool, alice.id, "Files").await.id;
        raw_file(&pool, alice.id, files, "kept.jpg", 'a', 3).await
    };
    sqlx::query("DELETE FROM nodes WHERE id = $1")
        .bind(gone)
        .execute(&pool)
        .await
        .unwrap();

    // As `extract_metadata` does with a node list read before the deletion.
    let mut tx = pool.begin().await.unwrap();
    assert!(
        !assets::upsert(&mut tx, &asset(gone, alice.id, 'a'))
            .await
            .unwrap()
    );
    assert!(
        assets::upsert(&mut tx, &asset(kept, alice.id, 'a'))
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert!(assets::find(&pool, kept).await.unwrap().is_some());
}
