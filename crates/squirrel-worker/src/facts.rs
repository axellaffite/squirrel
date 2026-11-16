//! From stored media facts to a node's timeline entry.

use squirrel_db::assets::{AssetKind, BlobNode, NewAsset, TakenSource};
use squirrel_db::media::{MediaRecord, MediaState};
use squirrel_media::capture::{self, ClientTaken, is_screenshot, resolve_taken};
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

/// Offsets the schema accepts (±18 h), in minutes.
pub fn offset_minutes(offset: UtcOffset) -> Option<i16> {
    Some(offset.whole_minutes()).filter(|m| (-1080..=1080).contains(m))
}

fn dimension(value: Option<i32>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v).ok())
}

fn source(source: capture::TakenSource) -> TakenSource {
    match source {
        capture::TakenSource::Exif => TakenSource::Exif,
        capture::TakenSource::Filename => TakenSource::Filename,
        capture::TakenSource::Device => TakenSource::Device,
        capture::TakenSource::Upload => TakenSource::Upload,
    }
}

/// The timeline entry of `node`; provisional until metadata is ready
/// (`extract_metadata` recomputes it).
/// The capture time the node's uploader reported, if any.
pub fn client_taken(node: &BlobNode) -> Option<ClientTaken> {
    let local = node.client_taken_at?;
    let offset = node
        .client_taken_offset_min
        .and_then(|minutes| UtcOffset::from_whole_seconds(i32::from(minutes) * 60).ok());
    Some(ClientTaken { local, offset })
}

pub fn asset_for(node: &BlobNode, media: &MediaRecord, now: OffsetDateTime) -> NewAsset {
    let taken = resolve_taken(
        capture_time(media),
        &node.name,
        client_taken(node),
        node.created_at,
        now,
    );
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
        taken_offset_min: taken.offset.and_then(offset_minutes),
        taken_source: source(taken.source),
    }
}
