mod common;

use std::time::Duration;

use common::{Fixture, admin, job, pdfium_dir, queued_kinds, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::documents;
use squirrel_db::media::{self, MediaRecord, MediaState};
use squirrel_db::nodes::NodeRecord;
use squirrel_jobs::JobContext;
use squirrel_media::testutil::{
    ExifSpec, encrypted_pdf, jpeg_with_exif, pdf_with_pages, pdf_with_text,
};
use squirrel_worker::exec::{Executor, SubprocessConfig};
use squirrel_worker::{backfill, text, thumbnails};

/// Stores `bytes` as `name` in Alice's Documents and creates its media row.
async fn stored(fx: &Fixture, name: &str, bytes: &[u8]) -> (NodeRecord, String) {
    let alice = match squirrel_db::users::find_by_username(
        fx.pool(),
        &squirrel_core::Username::parse("alice").unwrap(),
    )
    .await
    .unwrap()
    {
        Some(user) => user,
        None => admin(fx.pool()).await,
    };
    let parent = root(fx.pool(), alice.id, "Documents").await;
    let node = fx.store_file(alice.id, parent, name, bytes).await;
    let sha = node.blob_sha256.clone().unwrap();
    let kind = MediaKind::for_mime(node.mime.as_deref().unwrap()).unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, kind).await.unwrap();
    (node, sha)
}

fn text_job(sha: &str) -> JobContext {
    job("extract_text", json!({"sha256": sha}))
}

fn thumbs_job(sha: &str) -> JobContext {
    job("thumbnails", json!({"sha256": sha}))
}

async fn row(fx: &Fixture, sha: &str) -> MediaRecord {
    media::find(fx.pool(), sha).await.unwrap().unwrap()
}

async fn body(fx: &Fixture, sha: &str) -> Option<String> {
    sqlx::query_scalar("SELECT body FROM document_text WHERE sha256 = $1")
        .bind(sha)
        .fetch_optional(fx.pool())
        .await
        .unwrap()
}

async fn found(fx: &Fixture, node: &NodeRecord, query: &str) -> bool {
    documents::search(fx.pool(), node.owner_id, query, 0, 20)
        .await
        .unwrap()
        .items
        .iter()
        .any(|hit| hit.node.id == node.id)
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn plain_text_and_markdown_become_searchable(pool: PgPool) {
    let fx = Fixture::new(pool);
    let (note, sha) = stored(&fx, "budget.txt", b"Quarterly squirrel budget\n").await;
    assert_eq!(note.mime.as_deref(), Some("text/plain"));
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert_eq!(row(&fx, &sha).await.text_state(), MediaState::Ready);
    assert_eq!(
        body(&fx, &sha).await.as_deref(),
        Some("Quarterly squirrel budget\n")
    );
    assert!(found(&fx, &note, "squir").await);

    let (md, sha) = stored(&fx, "notes.md", b"# Recettes\n\nTarte aux noisettes").await;
    assert_eq!(md.mime.as_deref(), Some("text/markdown"));
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert!(found(&fx, &md, "noisette").await);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn long_and_broken_text_is_bounded_and_cleaned(pool: PgPool) {
    let fx = Fixture::new(pool);
    let big = "acorn ".repeat(1024 * 1024 / 6 + 1);
    let (_, sha) = stored(&fx, "big.txt", big.as_bytes()).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert_eq!(body(&fx, &sha).await.unwrap().len(), 256 * 1024);

    let (_, sha) = stored(&fx, "broken.txt", b"caf\xE9 au\0 lait").await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert_eq!(
        body(&fx, &sha).await.as_deref(),
        Some("caf\u{FFFD} au lait")
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn images_are_left_alone(pool: PgPool) {
    let fx = Fixture::new(pool);
    let (_, sha) = stored(&fx, "a.jpg", &jpeg_with_exif(&ExifSpec::default())).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.text_state(), MediaState::Unsupported);
    assert!(media.last_error.is_none());
    assert!(body(&fx, &sha).await.is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn without_pdfium_pdfs_are_unsupported(pool: PgPool) {
    let fx = Fixture::new(pool);
    let (_, sha) = stored(&fx, "a.pdf", &pdf_with_text("Hello")).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.text_state(), MediaState::Unsupported);
    assert_eq!(media.thumbs_state(), MediaState::Unsupported);
    assert_eq!(media.last_error.as_deref(), Some(thumbnails::NO_PDFIUM));
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn with_pdfium_pdfs_get_text_pages_and_thumbnails(pool: PgPool) {
    let Some(pdfium) = pdfium_dir() else { return };
    let fx = Fixture::with_pdfium(pool, Executor::InProcess, pdfium);
    let pdf = pdf_with_pages(&["Facture electricite", "Total 42 euros"], "");
    let (node, sha) = stored(&fx, "facture.pdf", &pdf).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.text_state(), MediaState::Ready);
    assert_eq!(media.page_count, Some(2));
    assert!(found(&fx, &node, "electri euros").await);

    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.thumbs_state(), MediaState::Ready);
    assert_eq!((media.width, media.height), (Some(1018), Some(1440)));
    assert!(media.blurhash.is_some());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn encrypted_and_broken_pdfs_end_without_retries(pool: PgPool) {
    let Some(pdfium) = pdfium_dir() else { return };
    let fx = Fixture::with_pdfium(pool, Executor::InProcess, pdfium);
    let (_, sha) = stored(&fx, "secret.pdf", &encrypted_pdf()).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&sha)).await.unwrap();
    let media = row(&fx, &sha).await;
    assert_eq!(media.text_state(), MediaState::Unsupported);
    assert_eq!(media.thumbs_state(), MediaState::Unsupported);
    assert_eq!(media.last_error.as_deref(), Some("encrypted PDF"));

    let (_, sha) = stored(&fx, "broken.pdf", b"%PDF-1.4 broken").await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert_eq!(row(&fx, &sha).await.text_state(), MediaState::Failed);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_sandbox_text_file_linking_elsewhere_is_refused(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let program = bin.path().join("fake-squirrel");
    std::fs::write(
        &program,
        "#!/bin/sh\n\
         dir=$(cat | sed 's/.*\"out_dir\":\"\\([^\"]*\\)\".*/\\1/')\n\
         ln -s /etc/hosts \"$dir/text.txt\"\n\
         echo '{\"status\":\"pdf\",\"rendered\":null,\"page_count\":1}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let executor = Executor::Subprocess(SubprocessConfig {
        program,
        max_memory_mb: 64,
        max_cpu_secs: 1,
        timeout: Duration::from_secs(10),
    });
    // The fake child never loads it: any directory will do.
    let fx = Fixture::with_pdfium(pool, executor, bin.path().into());
    let (_, sha) = stored(&fx, "a.pdf", &pdf_with_text("Hi")).await;
    text::run(&fx.ctx, text_job(&sha)).await.unwrap();
    assert_eq!(row(&fx, &sha).await.text_state(), MediaState::Failed);
    assert!(body(&fx, &sha).await.is_none());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_queues_pdfs_again_once_pdfium_is_configured(pool: PgPool) {
    let fx = Fixture::new(pool);
    let (_, waiting) = stored(&fx, "a.pdf", &pdf_with_text("Hello")).await;
    text::run(&fx.ctx, text_job(&waiting)).await.unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&waiting))
        .await
        .unwrap();
    // Unsupported for another reason: stays so.
    let (_, huge) = stored(&fx, "huge.pdf", &pdf_with_text("Big")).await;
    sqlx::query("UPDATE blobs SET size = $2 WHERE sha256 = $1")
        .bind(&huge)
        .bind(513_i64 * 1024 * 1024)
        .execute(fx.pool())
        .await
        .unwrap();
    thumbnails::run(&fx.ctx, thumbs_job(&huge)).await.unwrap();
    text::run(&fx.ctx, text_job(&huge)).await.unwrap();
    assert_eq!(row(&fx, &huge).await.text_state(), MediaState::Unsupported);
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();

    // Still no pdfium: nothing to do.
    backfill::run(&fx.ctx, job("backfill", json!({})))
        .await
        .unwrap();
    assert!(queued_kinds(fx.pool()).await.is_empty());

    let with_pdfium = fx.reopened_with_pdfium("/opt/pdfium/lib".into());
    backfill::run(&with_pdfium, job("backfill", json!({})))
        .await
        .unwrap();
    assert_eq!(
        queued_kinds(fx.pool()).await,
        vec![
            ("extract_text".to_owned(), -10),
            ("thumbnails".to_owned(), 30)
        ]
    );
    let media = row(&fx, &waiting).await;
    assert_eq!(media.thumbs_state(), MediaState::Pending);
    assert_eq!(media.text_state(), MediaState::Pending);
    assert_eq!(
        row(&fx, &huge).await.thumbs_state(),
        MediaState::Unsupported
    );

    // Queued once: a second run finds nothing left waiting.
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run(&with_pdfium, job("backfill", json!({})))
        .await
        .unwrap();
    assert!(queued_kinds(fx.pool()).await.is_empty());
}
