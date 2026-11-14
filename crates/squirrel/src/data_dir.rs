//! Binding the data directory to its database (`.squirrel-instance`), checked
//! by `serve` and adopted deliberately by `squirrel admin adopt-data-dir`.

use std::path::Path;
use std::time::{Duration, SystemTime};

use anyhow::Context;
use sqlx::PgPool;
use squirrel_core::ContentHash;
use squirrel_storage::{Binding, BindingError, BlobStore};
use uuid::Uuid;

/// Blob files checked against the catalogue per query when adopting.
const ADOPT_BATCH: usize = 1000;

/// Refuses to serve a data directory that belongs to another installation, or
/// that holds blobs without saying which installation they belong to: the
/// orphan sweep would otherwise delete every file the database does not know.
pub async fn bind(pool: &PgPool, storage: &BlobStore, data_dir: &Path) -> anyhow::Result<()> {
    let installation = installation_id(pool).await?;
    let store = storage.clone();
    let bound = tokio::task::spawn_blocking(move || store.bind(installation))
        .await
        .context("checking the data directory")?;
    match bound {
        Ok(Binding::Bound) => Ok(()),
        Ok(Binding::Claimed) => {
            tracing::info!(%installation, "new data directory bound to this installation");
            Ok(())
        }
        Err(err) => Err(anyhow::anyhow!(refusal(&err, data_dir))),
    }
}

fn refusal(err: &BindingError, data_dir: &Path) -> String {
    let dir = data_dir.display();
    match err {
        BindingError::Mismatch { marker, database } => format!(
            "refusing to start: the data directory {dir} belongs to installation {marker}, \
             but the database (SQUIRREL_DATABASE_URL) is installation {database}. Serving it \
             would hide its files and let maintenance delete them. Point SQUIRREL_DATABASE_URL \
             at the database this data directory was used with, or SQUIRREL_DATA_DIR at this \
             database's data directory."
        ),
        BindingError::Unmarked => format!(
            "refusing to start: the data directory {dir} holds files but no installation \
             marker (it predates markers, or was used with another database). If \
             SQUIRREL_DATABASE_URL is the database these files were uploaded with, bind them \
             once with `squirrel admin adopt-data-dir` (same SQUIRREL_DATABASE_URL and \
             SQUIRREL_DATA_DIR), then start the server again."
        ),
        BindingError::InvalidMarker(path) => format!(
            "refusing to start: {} is not a valid installation marker; restore it, or remove \
             it and run `squirrel admin adopt-data-dir`",
            path.display()
        ),
        BindingError::Storage(err) => {
            format!("checking the data directory {dir}'s installation marker: {err}")
        }
    }
}

async fn installation_id(pool: &PgPool) -> anyhow::Result<Uuid> {
    squirrel_db::instance::installation_id(pool)
        .await
        .context("reading the installation id from the database")
}

/// Binds an existing data directory to this database. Refuses one already
/// bound to another installation, and (unless `force`) one whose blob files
/// the database knows none of: that is almost certainly the wrong database.
pub async fn adopt(pool: &PgPool, data_dir: &Path, force: bool) -> anyhow::Result<String> {
    let installation = installation_id(pool).await?;
    let store = BlobStore::open(data_dir)
        .with_context(|| format!("opening data directory {}", data_dir.display()))?;
    match store.read_marker()? {
        Some(marker) if marker == installation => {
            return Ok(format!(
                "{} is already bound to installation {installation}",
                data_dir.display()
            ));
        }
        Some(marker) => anyhow::bail!(
            "{} is bound to another installation ({marker}; this database is {installation}). \
             Use the database it belongs to. If you are certain these files belong to this \
             database, delete {} and run this command again.",
            data_dir.display(),
            store.marker_path().display()
        ),
        None => {}
    }
    let on_disk = list_all_blobs(&store).await?;
    let known = count_catalogued(pool, &on_disk).await?;
    if !on_disk.is_empty() && known == 0 && !force {
        anyhow::bail!(
            "none of the {} blob files in {} is known to this database: it is probably not \
             the database they were uploaded with. Re-run with --force to bind them anyway.",
            on_disk.len(),
            data_dir.display()
        );
    }
    store.write_marker(installation)?;
    Ok(format!(
        "{} bound to installation {installation} ({} blob files, {known} known to the database)",
        data_dir.display(),
        on_disk.len()
    ))
}

async fn list_all_blobs(store: &BlobStore) -> anyhow::Result<Vec<ContentHash>> {
    let store = store.clone();
    // Every file, however recent (and despite a clock a little ahead).
    let cutoff = SystemTime::now() + Duration::from_secs(24 * 60 * 60);
    Ok(tokio::task::spawn_blocking(move || store.list_blobs_older_than(cutoff)).await??)
}

async fn count_catalogued(pool: &PgPool, hashes: &[ContentHash]) -> anyhow::Result<usize> {
    let mut known = 0;
    for batch in hashes.chunks(ADOPT_BATCH) {
        known += squirrel_db::blobs::existing(pool, batch).await?.len();
    }
    Ok(known)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn catalogue(pool: &PgPool, hash: &ContentHash) {
        sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, 1, 'text/plain')")
            .bind(hash.as_str())
            .execute(pool)
            .await
            .unwrap();
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn a_fresh_data_directory_is_bound_on_first_start(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        bind(&pool, &store, dir.path()).await.unwrap();
        let installation = squirrel_db::instance::installation_id(&pool).await.unwrap();
        assert_eq!(store.read_marker().unwrap(), Some(installation));
        bind(&pool, &store, dir.path()).await.unwrap();
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn another_installations_data_directory_is_refused(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        store.write_marker(Uuid::now_v7()).unwrap();
        let err = bind(&pool, &store, dir.path()).await.unwrap_err();
        assert!(err.to_string().contains("belongs to installation"), "{err}");
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn unmarked_blobs_are_refused_until_adopted(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let (hash, _) = store.put_reader(&b"from before markers"[..]).unwrap();
        catalogue(&pool, &hash).await;
        let err = bind(&pool, &store, dir.path()).await.unwrap_err();
        assert!(err.to_string().contains("adopt-data-dir"), "{err}");

        let report = adopt(&pool, dir.path(), false).await.unwrap();
        assert!(report.contains("1 blob files, 1 known"), "{report}");
        bind(&pool, &store, dir.path()).await.unwrap();
        assert!(store.contains(&hash));
        let again = adopt(&pool, dir.path(), false).await.unwrap();
        assert!(again.contains("already bound"), "{again}");
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn adopting_files_the_database_does_not_know_needs_force(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        store.put_reader(&b"unknown here"[..]).unwrap();
        let err = adopt(&pool, dir.path(), false).await.unwrap_err();
        assert!(err.to_string().contains("--force"), "{err}");
        assert_eq!(store.read_marker().unwrap(), None);
        adopt(&pool, dir.path(), true).await.unwrap();
        bind(&pool, &store, dir.path()).await.unwrap();
    }

    #[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
    async fn adopting_another_installations_directory_is_refused(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let other = Uuid::now_v7();
        store.write_marker(other).unwrap();
        assert!(adopt(&pool, dir.path(), true).await.is_err());
        assert_eq!(store.read_marker().unwrap(), Some(other));
    }
}
