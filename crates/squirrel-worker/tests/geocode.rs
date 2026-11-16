mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::{Fixture, job, queued};
use serde_json::json;
use sqlx::PgPool;
use squirrel_jobs::JobError;
use squirrel_media::geo_data::{GeoSource, PINNED};
use squirrel_worker::{backfill, geocode};

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../squirrel-media/tests/fixtures/geo-sample.tsv")
}

fn sample_fixture(pool: PgPool) -> Fixture {
    Fixture::with_geo(pool, |_| GeoSource::Explicit(sample()))
}

/// A blob with a media row at `position` (`None`: no GPS).
async fn media_at(pool: &PgPool, sha: &str, position: Option<(f64, f64)>) {
    sqlx::query("INSERT INTO blobs (sha256, size, mime) VALUES ($1, 1, 'image/jpeg')")
        .bind(sha)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, latitude, longitude) \
         VALUES ($1, 'image', 'ready', $2, $3)",
    )
    .bind(sha)
    .bind(position.map(|p| p.0))
    .bind(position.map(|p| p.1))
    .execute(pool)
    .await
    .unwrap();
}

type PlaceRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

async fn place(pool: &PgPool, sha: &str) -> PlaceRow {
    sqlx::query_as(
        "SELECT place_name, place_admin1, place_country, place_country_code, geocoded_with \
           FROM media WHERE sha256 = $1",
    )
    .bind(sha)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn sha(c: char) -> String {
    c.to_string().repeat(64)
}

async fn run(fx: &Fixture, sha: &str) -> Result<(), JobError> {
    geocode::run(&fx.ctx, job("geocode", json!({ "sha256": sha }))).await
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn names_the_place_of_a_geotagged_blob(pool: PgPool) {
    let fx = sample_fixture(pool);
    media_at(fx.pool(), &sha('a'), Some((48.8606, 2.3376))).await;
    run(&fx, &sha('a')).await.unwrap();
    assert_eq!(
        place(fx.pool(), &sha('a')).await,
        (
            Some("Paris".into()),
            Some("Île-de-France".into()),
            Some("France".into()),
            Some("FR".into()),
            Some("custom:geo-sample.tsv".into()),
        )
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_open_sea_is_no_place_but_counts_as_geocoded(pool: PgPool) {
    let fx = sample_fixture(pool);
    media_at(fx.pool(), &sha('b'), Some((30.0, -40.0))).await;
    run(&fx, &sha('b')).await.unwrap();
    assert_eq!(
        place(fx.pool(), &sha('b')).await,
        (None, None, None, None, Some("custom:geo-sample.tsv".into()))
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn blobs_without_gps_or_media_are_left_alone(pool: PgPool) {
    let fx = sample_fixture(pool);
    media_at(fx.pool(), &sha('c'), None).await;
    run(&fx, &sha('c')).await.unwrap();
    assert_eq!(place(fx.pool(), &sha('c')).await, Default::default());
    run(&fx, &sha('d')).await.unwrap();
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_missing_snapshot_reschedules_without_using_an_attempt(pool: PgPool) {
    let fx = Fixture::with_geo(pool, |dir| GeoSource::resolve(None, dir));
    media_at(fx.pool(), &sha('e'), Some((48.8606, 2.3376))).await;
    match run(&fx, &sha('e')).await {
        Err(JobError::Reschedule(delay)) => assert_eq!(delay, Duration::from_secs(3600)),
        other => panic!("{other:?}"),
    }
    assert_eq!(place(fx.pool(), &sha('e')).await, Default::default());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_corrupt_snapshot_is_retried_and_never_used(pool: PgPool) {
    let fx = Fixture::with_geo(pool, |dir| {
        std::fs::create_dir(dir.join("geo")).unwrap();
        std::fs::copy(sample(), dir.join("geo").join(PINNED.file_name)).unwrap();
        GeoSource::resolve(None, dir)
    });
    media_at(fx.pool(), &sha('f'), Some((48.8606, 2.3376))).await;
    match run(&fx, &sha('f')).await {
        Err(JobError::Retry(err)) => assert!(format!("{err:#}").contains("checksum"), "{err:#}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(place(fx.pool(), &sha('f')).await, Default::default());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn disabled_geocoding_does_nothing(pool: PgPool) {
    let fx = Fixture::new(pool);
    media_at(fx.pool(), &sha('9'), Some((48.8606, 2.3376))).await;
    run(&fx, &sha('9')).await.unwrap();
    assert_eq!(place(fx.pool(), &sha('9')).await, Default::default());
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn backfill_geocodes_what_another_dataset_or_none_handled(pool: PgPool) {
    let fx = sample_fixture(pool);
    media_at(fx.pool(), &sha('1'), Some((48.8606, 2.3376))).await;
    media_at(fx.pool(), &sha('2'), Some((48.8606, 2.3376))).await;
    media_at(fx.pool(), &sha('3'), Some((48.8606, 2.3376))).await;
    media_at(fx.pool(), &sha('4'), None).await;
    sqlx::query("UPDATE media SET geocoded_with = 'old.tsv' WHERE sha256 = $1")
        .bind(sha('2'))
        .execute(fx.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE media SET geocoded_with = 'custom:geo-sample.tsv' WHERE sha256 = $1")
        .bind(sha('3'))
        .execute(fx.pool())
        .await
        .unwrap();
    backfill::run_page(&fx.ctx, job("backfill", json!({})), 10)
        .await
        .unwrap();
    let mut geocodes: Vec<(i16, String)> = queued(fx.pool())
        .await
        .into_iter()
        .filter(|(kind, _, _)| kind == "geocode")
        .map(|(_, priority, payload)| (priority, payload["sha256"].as_str().unwrap().to_owned()))
        .collect();
    geocodes.sort();
    assert_eq!(geocodes, vec![(-20, sha('1')), (-20, sha('2'))]);
}
