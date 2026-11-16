//! From stored media facts to a node's timeline entry.

use squirrel_db::assets::{AssetKind, BlobNode, NewAsset, TakenSource};
use squirrel_db::media::{MediaRecord, MediaState};
use squirrel_media::capture::{self, is_screenshot, resolve_taken};
use squirrel_media::metadata::{CaptureTime, ImageMetadata};
use time::{OffsetDateTime, UtcOffset};

/// The EXIF capture time, once metadata extraction has run.
pub fn capture_time(media: &MediaRecord) -> Option<CaptureTime> {
    if media.metadata_state() != MediaState::Ready {
        return None;
    }
    let local = media.exif_taken_at?;
    let offset = media
        .exif_offset_min
        .and_then(|minutes| UtcOffset::from_whole_seconds(i32::from(minutes) * 60).ok());
    Some(CaptureTime { local, offset })
}

/// The parts of the image metadata that classify a node (screenshot or photo).
pub fn image_metadata(media: &MediaRecord) -> ImageMetadata {
    ImageMetadata {
        camera_make: media.camera_make.clone(),
        camera_model: media.camera_model.clone(),
        says_screenshot: media.is_screenshot,
        ..ImageMetadata::default()
    }
}

fn dimension(value: Option<i32>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

fn source(source: capture::TakenSource) -> TakenSource {
    match source {
        capture::TakenSource::Exif => TakenSource::Exif,
        capture::TakenSource::Filename => TakenSource::Filename,
        capture::TakenSource::Upload => TakenSource::Upload,
    }
}

/// The timeline entry of `node`; provisional until metadata is ready
/// (`extract_metadata` recomputes it).
pub fn asset_for(node: &BlobNode, media: &MediaRecord, now: OffsetDateTime) -> NewAsset {
    let taken = resolve_taken(capture_time(media), &node.name, node.created_at, now);
    let mime = node.mime.as_deref().unwrap_or_default();
    let screenshot = is_screenshot(
        &image_metadata(media),
        &node.name,
        mime,
        dimension(media.width),
        dimension(media.height),
    );
    NewAsset {
        node_id: node.node_id,
        owner_id: node.owner_id,
        sha256: media.sha256.clone(),
        kind: if screenshot {
            AssetKind::Screenshot
        } else {
            AssetKind::Photo
        },
        taken_at: taken.local,
        taken_offset_min: taken.offset.map(UtcOffset::whole_minutes),
        taken_source: source(taken.source),
    }
}
