mod common;

use common::{admin, hash, raw_file, root};
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::media::{self, MediaState};
use squirrel_db::tags::{self, TagSource};
use squirrel_db::{clip, ml_settings};

const MODEL: &str = "openai-vit-b32-224";

/// An image blob with its media row, filled with `fill`.
async fn image(pool: &PgPool, fill: char) -> String {
    file(pool, fill, MediaKind::Image).await
}

/// A blob of `kind` in the admin's `Files` root (both created on first use).
async fn file(pool: &PgPool, fill: char, kind: MediaKind) -> String {
    let existing: Option<uuid::Uuid> = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
        .fetch_optional(pool)
        .await
        .unwrap();
    let owner = match existing {
        Some(id) => id,
        None => admin(pool).await.id,
    };
    let files: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM nodes WHERE owner_id = $1 AND name = 'Files'")
            .bind(owner)
            .fetch_optional(pool)
            .await
            .unwrap();
    let files = match files {
        Some(id) => id,
        None => root(pool, owner, "Files").await.id,
    };
    raw_file(pool, owner, files, &format!("{fill}.bin"), fill, 3).await;
    let sha = hash(fill).as_str().to_owned();
    let mut conn = pool.acquire().await.unwrap();
    media::ensure(&mut conn, &sha, kind).await.unwrap();
    sha
}

fn vector(seed: f32) -> Vec<f32> {
    (0..512).map(|i| ((i as f32) * seed).sin() / 16.0).collect()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn embeddings_round_trip_at_half_precision(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    let values = vector(0.1);
    clip::store_embedding(&pool, &sha, MODEL, &values)
        .await
        .unwrap();
    let stored = clip::embedding(&pool, &sha, MODEL).await.unwrap().unwrap();
    assert_eq!(stored.values.len(), 512);
    assert_eq!(stored.tagged_labels_version, None);
    for (a, b) in values.iter().zip(&stored.values) {
        assert!((a - b).abs() < 1e-3, "{a} {b}");
    }
    assert!(
        clip::embedding(&pool, &sha, "other")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_wrong_dimension_is_refused(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    let short = vec![0.1; 511];
    assert!(
        clip::store_embedding(&pool, &sha, MODEL, &short)
            .await
            .is_err()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn re_embedding_clears_the_tagged_version(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    clip::store_embedding(&pool, &sha, MODEL, &vector(0.1))
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    clip::mark_tagged(&mut conn, &sha, MODEL, 1).await.unwrap();
    let stored = clip::embedding(&pool, &sha, MODEL).await.unwrap().unwrap();
    assert_eq!(stored.tagged_labels_version, Some(1));
    clip::store_embedding(&pool, &sha, MODEL, &vector(0.2))
        .await
        .unwrap();
    let stored = clip::embedding(&pool, &sha, MODEL).await.unwrap().unwrap();
    assert_eq!(stored.tagged_labels_version, None);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn tags_replace_only_their_source_and_refuse_bad_slugs(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    let mut conn = pool.acquire().await.unwrap();
    let first = [("dog".to_owned(), 0.4), ("beach".to_owned(), 0.3)];
    tags::replace_tags(&mut conn, &sha, TagSource::Clip, &first, 1)
        .await
        .unwrap();
    let second = [("snow".to_owned(), 0.5)];
    tags::replace_tags(&mut conn, &sha, TagSource::Clip, &second, 2)
        .await
        .unwrap();
    assert_eq!(
        tags::tags_of(&pool, &sha).await.unwrap(),
        [("snow".to_owned(), 0.5)]
    );
    for bad in ["Dog", "1dog", "dog-cat", ""] {
        let result = tags::replace_tags(
            &mut conn,
            &sha,
            TagSource::Clip,
            &[(bad.to_owned(), 0.1)],
            1,
        )
        .await;
        assert!(result.is_err(), "{bad:?}");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn embeddings_and_tags_go_with_their_media_row(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    clip::store_embedding(&pool, &sha, MODEL, &vector(0.1))
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    tags::replace_tags(
        &mut conn,
        &sha,
        TagSource::Clip,
        &[("dog".to_owned(), 0.4)],
        1,
    )
    .await
    .unwrap();
    sqlx::query("DELETE FROM media WHERE sha256 = $1")
        .bind(&sha)
        .execute(&pool)
        .await
        .unwrap();
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM clip_embeddings) + (SELECT count(*) FROM asset_tags)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn only_images_are_analysed(pool: PgPool) {
    let pdf = file(&pool, 'a', MediaKind::Pdf).await;
    let pdf = media::find(&pool, &pdf).await.unwrap().unwrap();
    assert_eq!(pdf.clip_state(), MediaState::Unsupported);
    assert_eq!(pdf.faces_state(), MediaState::Unsupported);
    let img = image(&pool, 'b').await;
    let row = media::find(&pool, &img).await.unwrap().unwrap();
    assert_eq!(row.clip_state(), MediaState::Pending);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn label_embeddings_round_trip_per_model_and_version(pool: PgPool) {
    let bank = vec![
        ("beach".to_owned(), vector(0.3)),
        ("dog".to_owned(), vector(0.4)),
    ];
    clip::store_label_embeddings(&pool, MODEL, 1, &bank)
        .await
        .unwrap();
    clip::store_label_embeddings(&pool, MODEL, 1, &bank[..1])
        .await
        .unwrap();
    let read = clip::label_embeddings(&pool, MODEL, 1).await.unwrap();
    assert_eq!(read.len(), 1, "replaced as a whole");
    assert_eq!(read[0].0, "beach");
    assert!(
        clip::label_embeddings(&pool, MODEL, 2)
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn faces_are_on_by_default(pool: PgPool) {
    let alice = admin(&pool).await;
    assert!(ml_settings::faces_enabled(&pool, alice.id).await.unwrap());
    ml_settings::set_faces_enabled(&pool, alice.id, false)
        .await
        .unwrap();
    assert!(!ml_settings::faces_enabled(&pool, alice.id).await.unwrap());
    ml_settings::set_faces_enabled(&pool, alice.id, true)
        .await
        .unwrap();
    assert!(ml_settings::faces_enabled(&pool, alice.id).await.unwrap());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn ml_is_ready_when_any_worker_host_says_so(pool: PgPool) {
    use squirrel_db::ml_status::{self, MlState};
    let none = ml_status::current(&pool).await.unwrap();
    assert_eq!(none.state, MlState::Unavailable);
    assert_eq!(none.reason.as_deref(), Some(ml_status::NO_REPORT));
    ml_status::report(
        &pool,
        "nas",
        MlState::Unavailable,
        Some("models not installed"),
    )
    .await
    .unwrap();
    let off = ml_status::current(&pool).await.unwrap();
    assert_eq!(off.reason.as_deref(), Some("models not installed"));
    ml_status::report(&pool, "gpu-box", MlState::Ready, None)
        .await
        .unwrap();
    ml_status::report(&pool, "nas", MlState::Unavailable, Some("x"))
        .await
        .unwrap();
    assert_eq!(
        ml_status::current(&pool).await.unwrap().state,
        MlState::Ready
    );
    ml_status::report(&pool, "gpu-box", MlState::Unavailable, Some("child failed"))
        .await
        .unwrap();
    assert_eq!(
        ml_status::current(&pool).await.unwrap().state,
        MlState::Unavailable
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_new_preview_drops_the_embedding_and_resets_clip(pool: PgPool) {
    let sha = image(&pool, 'a').await;
    clip::store_embedding(&pool, &sha, MODEL, &vector(0.1))
        .await
        .unwrap();
    media::set_state(
        &pool,
        &sha,
        media::Step::Clip,
        MediaState::Unsupported,
        Some("bad"),
    )
    .await
    .unwrap();
    assert!(
        media::set_thumbnails(&pool, &sha, 2, 40, 30, "LKO2?U%2Tw=w")
            .await
            .unwrap()
    );
    assert!(clip::embedding(&pool, &sha, MODEL).await.unwrap().is_none());
    let row = media::find(&pool, &sha).await.unwrap().unwrap();
    assert_eq!(row.clip_state(), MediaState::Pending);
}
