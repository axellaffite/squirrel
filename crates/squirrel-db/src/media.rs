//! What the worker learned from a blob's bytes (`media`, one row per blob).
//!
//! Rows are content-derived and shared by every node referencing the blob,
//! whoever owns it: never answer a user request from them without first
//! resolving one of the caller's own nodes.

use std::collections::HashMap;

use sqlx::postgres::PgExecutor;
use sqlx::{FromRow, PgConnection, PgPool};
use squirrel_core::MediaKind;
use time::{OffsetDateTime, PrimitiveDateTime};

use crate::DbError;

const MEDIA_COLUMNS: &str = "sha256, kind, metadata_state, thumbs_state, text_state, \
     clip_state, faces_state, \
     thumbs_version, width, height, blurhash, exif_taken_at, exif_offset_min, latitude, \
     longitude, altitude, camera_make, camera_model, lens_model, exposure_time, f_number, iso, \
     focal_length_mm, is_screenshot, page_count, place_name, place_admin1, place_country, \
     place_country_code, geocoded_with, last_error, created_at, updated_at";
/// `media.last_error` holds at most this many characters.
const MAX_ERROR_CHARS: usize = 2000;

/// Progress of one processing step of a blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState {
    Pending,
    Ready,
    Failed,
    /// Nothing to do for this format (or this build).
    Unsupported,
}

impl MediaState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
        }
    }

    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => Self::Unsupported,
        }
    }

    /// Initial states (metadata, thumbnails, text, clip, faces) of a new row of `kind`.
    fn initial(kind: MediaKind) -> [Self; 5] {
        use MediaState::{Pending, Unsupported};
        match kind {
            MediaKind::Image => [Pending, Pending, Unsupported, Pending, Pending],
            MediaKind::Pdf => [Unsupported, Pending, Pending, Unsupported, Unsupported],
            MediaKind::Text => [Unsupported, Unsupported, Pending, Unsupported, Unsupported],
        }
    }
}

/// A processing step, each with its own state column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Metadata,
    Thumbs,
    Text,
    /// CLIP embedding and tags (Phase 5).
    Clip,
    /// Face detection (Phase 5).
    Faces,
}

impl Step {
    /// Fixed column names: never built from input.
    fn column(self) -> &'static str {
        match self {
            Self::Metadata => "metadata_state",
            Self::Thumbs => "thumbs_state",
            Self::Text => "text_state",
            Self::Clip => "clip_state",
            Self::Faces => "faces_state",
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct MediaRecord {
    pub sha256: String,
    kind: String,
    metadata_state: String,
    thumbs_state: String,
    text_state: String,
    clip_state: String,
    faces_state: String,
    pub thumbs_version: i16,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub blurhash: Option<String>,
    pub exif_taken_at: Option<PrimitiveDateTime>,
    pub exif_offset_min: Option<i16>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f32>,
    pub iso: Option<i32>,
    pub focal_length_mm: Option<f32>,
    pub is_screenshot: bool,
    pub page_count: Option<i32>,
    pub place_name: Option<String>,
    pub place_admin1: Option<String>,
    pub place_country: Option<String>,
    pub place_country_code: Option<String>,
    pub geocoded_with: Option<String>,
    pub last_error: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl MediaRecord {
    /// The CHECK constraint guarantees a known kind; an unknown one reads as text
    /// (no image or PDF work is attempted on it).
    pub fn kind(&self) -> MediaKind {
        self.kind.parse().unwrap_or(MediaKind::Text)
    }

    pub fn metadata_state(&self) -> MediaState {
        MediaState::parse(&self.metadata_state)
    }

    pub fn thumbs_state(&self) -> MediaState {
        MediaState::parse(&self.thumbs_state)
    }

    pub fn text_state(&self) -> MediaState {
        MediaState::parse(&self.text_state)
    }

    pub fn clip_state(&self) -> MediaState {
        MediaState::parse(&self.clip_state)
    }

    pub fn faces_state(&self) -> MediaState {
        MediaState::parse(&self.faces_state)
    }

    pub fn state(&self, step: Step) -> MediaState {
        match step {
            Step::Metadata => self.metadata_state(),
            Step::Thumbs => self.thumbs_state(),
            Step::Text => self.text_state(),
            Step::Clip => self.clip_state(),
            Step::Faces => self.faces_state(),
        }
    }
}

/// The row for `sha256`, created with the initial states of `kind` if missing,
/// and locked until the caller's transaction ends (see [`lock`]). An existing
/// row is returned unchanged.
pub async fn ensure(
    conn: &mut PgConnection,
    sha256: &str,
    kind: MediaKind,
) -> Result<MediaRecord, DbError> {
    let [metadata, thumbs, text, clip, faces] = MediaState::initial(kind);
    sqlx::query(
        "INSERT INTO media (sha256, kind, metadata_state, thumbs_state, text_state, \
                            clip_state, faces_state) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (sha256) DO NOTHING",
    )
    .bind(sha256)
    .bind(kind.as_str())
    .bind(metadata.as_str())
    .bind(thumbs.as_str())
    .bind(text.as_str())
    .bind(clip.as_str())
    .bind(faces.as_str())
    .execute(&mut *conn)
    .await?;
    lock(conn, sha256).await?.ok_or(DbError::NotFound)
}

/// The row, locked (`FOR UPDATE`) until the caller's transaction ends. Every
/// writer of a blob's timeline entries (`process_media`, `extract_metadata`)
/// takes it first, so they never interleave.
pub async fn lock(conn: &mut PgConnection, sha256: &str) -> Result<Option<MediaRecord>, DbError> {
    let sql = format!("SELECT {MEDIA_COLUMNS} FROM media WHERE sha256 = $1 FOR UPDATE");
    Ok(sqlx::query_as::<_, MediaRecord>(&sql)
        .bind(sha256)
        .fetch_optional(conn)
        .await?)
}

/// Internal (no owner check): the worker's view of a blob's media facts.
pub async fn find<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
) -> Result<Option<MediaRecord>, DbError> {
    let sql = format!("SELECT {MEDIA_COLUMNS} FROM media WHERE sha256 = $1");
    Ok(sqlx::query_as::<_, MediaRecord>(&sql)
        .bind(sha256)
        .fetch_optional(executor)
        .await?)
}

/// What EXIF extraction found. Text fields are already length-capped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MetadataUpdate {
    pub exif_taken_at: Option<PrimitiveDateTime>,
    pub exif_offset_min: Option<i16>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub exposure_time: Option<String>,
    pub f_number: Option<f32>,
    pub iso: Option<i32>,
    pub focal_length_mm: Option<f32>,
    pub is_screenshot: bool,
    /// Displayed size from the header; only fills a size not yet known
    /// (thumbnails record the decoded truth).
    pub width: Option<i32>,
    pub height: Option<i32>,
}

/// Records extracted metadata and marks the step ready.
pub async fn set_metadata<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
    update: &MetadataUpdate,
) -> Result<(), DbError> {
    let (width, height) = match (update.width, update.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (Some(w), Some(h)),
        _ => (None, None),
    };
    sqlx::query(
        "UPDATE media SET exif_taken_at = $2, exif_offset_min = $3, latitude = $4, \
                longitude = $5, altitude = $6, camera_make = $7, camera_model = $8, \
                lens_model = $9, exposure_time = $10, f_number = $11, iso = $12, \
                focal_length_mm = $13, is_screenshot = $14, \
                width = COALESCE(width, $15), height = COALESCE(height, $16), \
                metadata_state = 'ready', updated_at = now() \
          WHERE sha256 = $1",
    )
    .bind(sha256)
    .bind(update.exif_taken_at)
    .bind(update.exif_offset_min)
    .bind(update.latitude)
    .bind(update.longitude)
    .bind(update.altitude)
    .bind(update.camera_make.as_deref())
    .bind(update.camera_model.as_deref())
    .bind(update.lens_model.as_deref())
    .bind(update.exposure_time.as_deref())
    .bind(update.f_number)
    .bind(update.iso)
    .bind(update.focal_length_mm)
    .bind(update.is_screenshot)
    .bind(width)
    .bind(height)
    .execute(executor)
    .await?;
    Ok(())
}

/// Thumbnails of `version` are installed: the step is ready and the decoded
/// size replaces any size read from headers. The preview changed, so the
/// image's CLIP embedding (computed from it) is dropped and its step goes
/// back to `pending` (also retrying one left `unsupported` by a bad preview).
/// `false`: the row is gone (the blob was collected meanwhile).
pub async fn set_thumbnails(
    pool: &PgPool,
    sha256: &str,
    version: i16,
    width: u32,
    height: u32,
    blurhash: &str,
) -> Result<bool, DbError> {
    let width = i32::try_from(width).map_err(|_| DbError::InvalidInput("width is too large"))?;
    let height = i32::try_from(height).map_err(|_| DbError::InvalidInput("height is too large"))?;
    let updated = sqlx::query(
        "WITH stale AS (DELETE FROM clip_embeddings WHERE sha256 = $1) \
         UPDATE media SET thumbs_state = 'ready', thumbs_version = $2, width = $3, height = $4, \
                blurhash = $5, \
                clip_state = CASE WHEN kind = 'image' THEN 'pending' ELSE clip_state END, \
                updated_at = now() \
          WHERE sha256 = $1",
    )
    .bind(sha256)
    .bind(version)
    .bind(width)
    .bind(height)
    .bind(blurhash)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

/// Records how many pages a PDF has.
pub async fn set_page_count<'e>(
    executor: impl PgExecutor<'e>,
    sha256: &str,
    page_count: u32,
) -> Result<(), DbError> {
    let page_count =
        i32::try_from(page_count).map_err(|_| DbError::InvalidInput("page count is too large"))?;
    sqlx::query("UPDATE media SET page_count = $2, updated_at = now() WHERE sha256 = $1")
        .bind(sha256)
        .bind(page_count)
        .execute(executor)
        .await?;
    Ok(())
}

/// A place name found for a blob's coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceUpdate<'a> {
    pub name: &'a str,
    pub admin1: &'a str,
    pub country: &'a str,
    /// ISO 3166-1 alpha-2; anything else is stored as unknown.
    pub country_code: &'a str,
}

fn non_empty(text: &str) -> Option<&str> {
    Some(text.trim()).filter(|t| !t.is_empty())
}

fn country_code(code: &str) -> Option<&str> {
    Some(code).filter(|c| c.len() == 2 && c.bytes().all(|b| b.is_ascii_uppercase()))
}

/// Records the result of reverse geocoding with `dataset` (`None`: no place
/// near the coordinates). Either way `geocoded_with` is set, so the same
/// dataset is not tried again.
pub async fn set_place(
    pool: &PgPool,
    sha256: &str,
    place: Option<&PlaceUpdate<'_>>,
    dataset: &str,
) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE media SET place_name = $2, place_admin1 = $3, place_country = $4, \
                place_country_code = $5, geocoded_with = $6, updated_at = now() \
          WHERE sha256 = $1",
    )
    .bind(sha256)
    .bind(place.and_then(|p| non_empty(p.name)))
    .bind(place.and_then(|p| non_empty(p.admin1)))
    .bind(place.and_then(|p| non_empty(p.country)))
    .bind(place.and_then(|p| country_code(p.country_code)))
    .bind(dataset)
    .execute(pool)
    .await?;
    Ok(())
}

/// Sets one step's state, recording `error` (when given) as `last_error`.
pub async fn set_state(
    pool: &PgPool,
    sha256: &str,
    step: Step,
    state: MediaState,
    error: Option<&str>,
) -> Result<(), DbError> {
    let sql = format!(
        "UPDATE media SET {} = $2, last_error = COALESCE($3, last_error), updated_at = now() \
          WHERE sha256 = $1",
        step.column()
    );
    sqlx::query(&sql)
        .bind(sha256)
        .bind(state.as_str())
        .bind(error.map(truncate))
        .execute(pool)
        .await?;
    Ok(())
}

fn truncate(message: &str) -> String {
    message
        .chars()
        .filter(|c| *c != '\0')
        .take(MAX_ERROR_CHARS)
        .collect()
}

/// A ready thumbnail, as listed with files.
#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct ThumbRef {
    pub version: i16,
    pub blurhash: Option<String>,
}

#[derive(FromRow)]
struct ThumbRow {
    sha256: String,
    thumbs_version: i16,
    blurhash: Option<String>,
}

/// Ready thumbnails among `hashes`, by hash. Callers pass hashes of nodes the
/// requesting user owns.
pub async fn thumbnails_for(
    pool: &PgPool,
    hashes: &[String],
) -> Result<HashMap<String, ThumbRef>, DbError> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, ThumbRow>(
        "SELECT sha256, thumbs_version, blurhash FROM media \
          WHERE sha256 = ANY($1) AND thumbs_state = 'ready'",
    )
    .bind(hashes)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.sha256,
                ThumbRef {
                    version: row.thumbs_version,
                    blurhash: row.blurhash,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip() {
        for state in [
            MediaState::Pending,
            MediaState::Ready,
            MediaState::Failed,
            MediaState::Unsupported,
        ] {
            assert_eq!(MediaState::parse(state.as_str()), state);
        }
    }

    #[test]
    fn only_iso_country_codes_are_kept() {
        assert_eq!(country_code("FR"), Some("FR"));
        assert_eq!(country_code("fr"), None);
        assert_eq!(country_code("FRA"), None);
        assert_eq!(non_empty("  "), None);
    }

    #[test]
    fn errors_fit_the_column() {
        assert_eq!(truncate("a\0b"), "ab");
        assert_eq!(truncate(&"x".repeat(5000)).len(), MAX_ERROR_CHARS);
    }
}
