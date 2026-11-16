//! `extract_metadata`: EXIF and header dimensions of an image blob, then the
//! capture time and kind of every node holding it.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::assets;
use squirrel_db::media::{self, MediaState, MetadataUpdate, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::metadata::{ImageMetadata, read_metadata_bounded};
use time::OffsetDateTime;

use crate::context::MediaContext;
use crate::facts;
use crate::jobs::{self, ShaPayload};

/// EXIF is parsed from at most this prefix of any file (containers can declare
/// sizes far beyond it); TIFF files larger than this are skipped (kamadak-exif
/// needs the whole file).
const MAX_EXIF_BYTES: u64 = 64 * 1024 * 1024;

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(()); // the blob was collected meanwhile
    };
    if media.kind() != MediaKind::Image {
        return Ok(());
    }
    let path = ctx.storage.blob_path(&hash);
    let extracted = tokio::task::spawn_blocking(move || extract(&path))
        .await
        .map_err(JobError::retry)?;
    let extracted = match extracted {
        Ok(found) => found,
        Err(err) => {
            if job.is_last_attempt() {
                let reason = err.to_string();
                media::set_state(
                    &ctx.pool,
                    &sha256,
                    Step::Metadata,
                    MediaState::Failed,
                    Some(&reason),
                )
                .await
                .map_err(JobError::retry)?;
            }
            return Err(JobError::retry(err));
        }
    };
    media::set_metadata(&ctx.pool, &sha256, &extracted.update())
        .await
        .map_err(JobError::retry)?;
    recompute_assets(ctx, &sha256).await?;
    if extracted.meta.gps.is_some() {
        let mut conn = ctx.pool.acquire().await.map_err(JobError::retry)?;
        squirrel_jobs::enqueue(&mut conn, &jobs::geocode(&sha256, false))
            .await
            .map_err(JobError::retry)?;
    }
    Ok(())
}

/// Settles the timeline entry of every node of the blob, in one transaction.
async fn recompute_assets(ctx: &MediaContext, sha256: &str) -> Result<(), JobError> {
    let nodes = assets::for_blob(&ctx.pool, sha256)
        .await
        .map_err(JobError::retry)?;
    let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
    let Some(media) = media::find(&mut *tx, sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    let now = OffsetDateTime::now_utc();
    for node in nodes
        .iter()
        .filter(|n| n.mime.as_deref().and_then(MediaKind::for_mime) == Some(MediaKind::Image))
    {
        let asset = facts::asset_for(node, &media, now);
        // `false`: deleted for good meanwhile, skipped without failing the transaction.
        assets::upsert(&mut tx, &asset)
            .await
            .map_err(JobError::retry)?;
    }
    tx.commit().await.map_err(JobError::retry)
}

struct Extracted {
    meta: ImageMetadata,
    /// Displayed size from the header, if the format's header is readable here.
    size: Option<(u32, u32)>,
}

impl Extracted {
    fn update(&self) -> MetadataUpdate {
        let meta = &self.meta;
        let gps = meta.gps;
        let (width, height) = self
            .size
            .and_then(|(w, h)| Some((i32::try_from(w).ok()?, i32::try_from(h).ok()?)))
            .unzip();
        MetadataUpdate {
            exif_taken_at: meta.taken.map(|t| t.local),
            exif_offset_min: meta
                .taken
                .and_then(|t| t.offset)
                .and_then(facts::offset_minutes),
            latitude: gps.map(|g| g.latitude),
            longitude: gps.map(|g| g.longitude),
            altitude: gps.and_then(|g| g.altitude).filter(|a| a.is_finite()),
            camera_make: meta.camera_make.clone(),
            camera_model: meta.camera_model.clone(),
            lens_model: meta.lens_model.clone(),
            exposure_time: meta.exposure_time.clone(),
            f_number: meta.f_number.map(|v| v as f32),
            iso: meta.iso.and_then(|v| i32::try_from(v).ok()),
            focal_length_mm: meta.focal_length_mm.map(|v| v as f32),
            is_screenshot: meta.says_screenshot,
            width,
            height,
        }
    }
}

fn extract(path: &Path) -> std::io::Result<Extracted> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    let head = reader
        .fill_buf()?
        .get(..4)
        .map(<[u8]>::to_vec)
        .unwrap_or_default();
    let is_tiff = head == *b"II*\0" || head == *b"MM\0*";
    let meta = if is_tiff && len > MAX_EXIF_BYTES {
        ImageMetadata::default()
    } else {
        read_metadata_bounded(reader, MAX_EXIF_BYTES)?
    };
    let size = image::ImageReader::open(path)
        .and_then(|r| r.with_guessed_format())
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .map(|(w, h)| {
            if matches!(meta.orientation, Some(5..=8)) {
                (h, w)
            } else {
                (w, h)
            }
        });
    Ok(Extracted { meta, size })
}
