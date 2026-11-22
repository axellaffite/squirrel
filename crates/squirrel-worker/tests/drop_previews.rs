mod common;

use common::{Fixture, admin, job, queued, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::media;
use squirrel_storage::Variant;
use squirrel_worker::drop_previews;
use uuid::Uuid;

/// A stored file `name` of `kind` with both stored variants, as before
/// previews were rendered on request.
async fn stored(fx: &Fixture, owner: Uuid, name: &str, kind: MediaKind) -> ContentHash {
    let parent = root(fx.pool(), owner, "Files").await;
    let node = fx.store_file(owner, parent, name, name.as_bytes()).await;
    let hash = ContentHash::parse(&node.blob_sha256.unwrap()).unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, hash.as_str(), kind).await.unwrap();
    let derived = fx.ctx.storage.derived();
    let scratch = derived.scratch().unwrap();
    std::fs::write(scratch.path().join("thumb.webp"), b"thumb").unwrap();
    std::fs::write(scratch.path().join("preview.webp"), b"preview").unwrap();
    derived.install(&hash, 1, &scratch).unwrap();
    hash
}

fn has(fx: &Fixture, hash: &ContentHash, variant: Variant) -> bool {
    fx.ctx.storage.derived().read(hash, variant, 1).is_ok()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn photo_previews_are_dropped_page_by_page_and_pdfs_keep_theirs(pool: PgPool) {
    let fx = Fixture::new(pool);
    let owner = admin(fx.pool()).await.id;
    let mut photos = [
        stored(&fx, owner, "a.jpg", MediaKind::Image).await,
        stored(&fx, owner, "b.jpg", MediaKind::Image).await,
    ];
    photos.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let pdf = stored(&fx, owner, "c.pdf", MediaKind::Pdf).await;

    let start = job("drop_previews", json!({ "after": null }));
    drop_previews::run_page(&fx.ctx, start, 1).await.unwrap();
    assert!(!has(&fx, &photos[0], Variant::Preview));
    assert!(has(&fx, &photos[1], Variant::Preview));
    let next = queued(fx.pool()).await;
    assert_eq!(next.len(), 1);
    let (kind, _, payload) = &next[0];
    assert_eq!(kind, "drop_previews");
    assert_eq!(payload["after"], photos[0].as_str());

    let page = job("drop_previews", payload.clone());
    drop_previews::run_page(&fx.ctx, page, 1).await.unwrap();
    assert!(!has(&fx, &photos[1], Variant::Preview));
    // Idempotent: running everything again finds nothing left to delete.
    let again = job("drop_previews", json!({}));
    drop_previews::run_page(&fx.ctx, again, 500).await.unwrap();
    assert!(has(&fx, &pdf, Variant::Preview), "PDF previews stay stored");
    for hash in photos.iter().chain([&pdf]) {
        assert!(has(&fx, hash, Variant::Thumb), "thumbnails stay");
    }
}
