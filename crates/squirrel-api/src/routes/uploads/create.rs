//! Opening an upload: dedup against the caller's library, resume, or a new session.

use sqlx::PgConnection;
use squirrel_core::{ContentHash, NodeName, UploadPlan};
use squirrel_db::nodes::{self, ClientTaken, NewFile, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::uploads::{self, NewUpload, UploadRecord};
use squirrel_db::{DbError, blobs, device_assets};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::files_dto::CreateUploadRequest;
use crate::state::AppState;
use crate::sync_dto::device_asset_id;

/// UTC offsets beyond ±18 h do not exist (and the database refuses them).
const MAX_OFFSET_MINUTES: i16 = 18 * 60;

/// A validated `CreateUploadRequest`.
pub struct UploadInput {
    pub parent_id: Uuid,
    pub name: NodeName,
    pub hash: ContentHash,
    /// Fits in `i64` (checked by `parse`).
    pub size: u64,
    /// The client's capture time (a phone's photo date).
    pub client_taken: Option<ClientTaken>,
    /// The uploading device and its id for the asset.
    pub device_asset: Option<(Uuid, String)>,
}

impl UploadInput {
    /// `device`: the calling device, if the caller is one. Only a device may
    /// send `device_asset_id`.
    pub fn parse(request: CreateUploadRequest, device: Option<Uuid>) -> Result<Self, ApiError> {
        if i64::try_from(request.size).is_err() {
            return Err(ApiError::Validation("the file is too large".into()));
        }
        let device_asset = match (request.device_asset_id, device) {
            (None, _) => None,
            (Some(id), Some(device)) => Some((device, device_asset_id(&id)?.to_owned())),
            (Some(_), None) => {
                return Err(ApiError::Validation(
                    "device_asset_id is only accepted from a signed-in device".into(),
                ));
            }
        };
        Ok(Self {
            parent_id: request.parent_id,
            name: NodeName::parse(&request.name)?,
            hash: ContentHash::parse(&request.sha256)?,
            size: request.size,
            client_taken: request.taken_at.as_deref().map(client_taken).transpose()?,
            device_asset,
        })
    }

    fn db_size(&self) -> i64 {
        i64::try_from(self.size).expect("checked in parse")
    }

    fn device_asset(&self) -> Option<(Uuid, &str)> {
        self.device_asset
            .as_ref()
            .map(|(device, id)| (*device, id.as_str()))
    }
}

/// An RFC 3339 time with its UTC offset (`2024-07-14T18:30:05+02:00`) as a
/// wall clock and an offset in minutes.
fn client_taken(raw: &str) -> Result<ClientTaken, ApiError> {
    let invalid = || {
        ApiError::Validation(
            "taken_at must be an RFC 3339 date and time with a UTC offset of at most 18 hours"
                .into(),
        )
    };
    let parsed = OffsetDateTime::parse(raw, &Rfc3339).map_err(|_| invalid())?;
    let offset_min = parsed.offset().whole_minutes();
    if offset_min.abs() > MAX_OFFSET_MINUTES {
        return Err(invalid());
    }
    Ok(ClientTaken {
        local: PrimitiveDateTime::new(parsed.date(), parsed.time()),
        offset_min: Some(offset_min),
    })
}

/// Records which content the device's asset became, in the caller's transaction.
pub(crate) async fn record_device_asset(
    conn: &mut PgConnection,
    device_asset: Option<(Uuid, &str)>,
    node: &NodeRecord,
) -> Result<(), DbError> {
    let (Some((device, asset)), Some(sha256)) = (device_asset, node.blob_sha256.as_deref()) else {
        return Ok(());
    };
    device_assets::record(conn, device, asset, sha256, Some(node.id)).await
}

/// Dedup within the caller's own library: if one of their nodes already
/// references this content, link a new node to it. `None` otherwise — even when
/// another user has the content (never revealed). Idempotent: when the
/// destination already holds this content under this exact name (a completion
/// whose response was lost, or a cancel that came too late), that node is
/// returned instead of a "name (1)" copy.
pub async fn link_existing(
    state: &AppState,
    owner_id: Uuid,
    input: &UploadInput,
) -> Result<Option<NodeRecord>, ApiError> {
    let mut tx = state.pool.begin().await.map_err(DbError::from)?;
    let quota_bytes = quota::lock_owner(&mut tx, owner_id).await?;
    let Some(blob) = blobs::find_owned(&mut tx, owner_id, &input.hash).await? else {
        return Ok(None);
    };
    if blob.size != input.db_size() {
        return Err(ApiError::Validation(
            "size does not match the content with this sha256".into(),
        ));
    }
    nodes::find_live_folder(&mut *tx, owner_id, input.parent_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if let Some(node) = nodes::find_live_file(
        &mut *tx,
        owner_id,
        input.parent_id,
        &input.name,
        &input.hash,
    )
    .await?
    {
        record_device_asset(&mut tx, input.device_asset(), &node).await?;
        tx.commit().await.map_err(DbError::from)?;
        tracing::info!(user_id = %owner_id, node_id = %node.id, "upload already present");
        return Ok(Some(node));
    }
    quota::ensure_room(&mut tx, owner_id, quota_bytes, blob.size, Pending::Ignore).await?;
    blobs::lock(&mut tx, &input.hash).await?;
    let node = nodes::insert_file(
        &mut tx,
        &NewFile {
            owner_id,
            parent_id: input.parent_id,
            name: &input.name,
            blob: &input.hash,
            size: blob.size,
            mime: &blob.mime,
            client_taken: input.client_taken,
        },
    )
    .await?;
    record_device_asset(&mut tx, input.device_asset(), &node).await?;
    super::enqueue_media(&mut tx, &node).await?;
    tx.commit().await.map_err(DbError::from)?;
    tracing::info!(user_id = %owner_id, node_id = %node.id, "upload deduplicated");
    Ok(Some(node))
}

/// Resumes the caller's session for this exact file, or opens a new one (after
/// checking disk space, quota and the open-session cap) with its sparse file.
pub async fn open_session(
    state: &AppState,
    owner_id: Uuid,
    input: &UploadInput,
) -> Result<UploadRecord, ApiError> {
    let resumable = uploads::find_resumable(
        &state.pool,
        owner_id,
        input.parent_id,
        &input.name,
        &input.hash,
        input.db_size(),
    )
    .await?;
    if let Some(existing) = resumable {
        return resume(state, existing, input.size).await;
    }
    // One admission at a time, so parallel creations cannot all count the
    // same free space.
    let _admission = state.uploads.admission.lock().await;
    ensure_disk_space(state, owner_id, input.size).await?;
    let record = create_session(state, owner_id, input).await?;
    let (store, id, size) = (state.storage.clone(), record.id, input.size);
    if let Err(err) = run_storage(move || store.create_upload(id, size)).await {
        uploads::delete(&state.pool, record.id).await?;
        return Err(err);
    }
    tracing::info!(user_id = %owner_id, upload_id = %record.id, size = input.size, "upload started");
    Ok(record)
}

async fn create_session(
    state: &AppState,
    owner_id: Uuid,
    input: &UploadInput,
) -> Result<UploadRecord, ApiError> {
    let plan = UploadPlan::new(input.size, state.config.upload_chunk_size)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("upload_chunk_size must be positive")))?;
    let chunk_count = i32::try_from(plan.chunk_count())
        .map_err(|_| ApiError::Validation("the file is too large".into()))?;
    let chunk_size = i64::try_from(plan.chunk_size())
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("upload_chunk_size is too large")))?;
    let new = NewUpload {
        owner_id,
        parent_id: input.parent_id,
        name: &input.name,
        sha256: &input.hash,
        size: input.db_size(),
        chunk_size,
        chunk_count,
        expires_at: OffsetDateTime::now_utc() + state.config.upload_ttl,
        max_open: state.config.upload_limits.max_open_per_user,
        client_taken: input.client_taken,
        device_asset: input.device_asset(),
    };
    Ok(uploads::create(&state.pool, &new).await?)
}

/// Returns an existing session. An open one gets its upload file back if it
/// went missing, and then forgets its received chunks so the client re-sends
/// them. A session being completed is returned untouched.
async fn resume(
    state: &AppState,
    existing: UploadRecord,
    size: u64,
) -> Result<UploadRecord, ApiError> {
    if !existing.is_open() {
        return Ok(existing);
    }
    // Under the chunk lock, so a completion cannot start meanwhile.
    let _guard = state
        .uploads
        .locks
        .chunk(existing.id, state.config.upload_limits.lock_wait)
        .await
        .ok_or(DbError::UploadBusy)?;
    let current = uploads::find(&state.pool, existing.owner_id, existing.id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !current.is_open() {
        return Ok(current);
    }
    let (store, id) = (state.storage.clone(), current.id);
    let recreated = run_storage(move || store.create_upload(id, size)).await?;
    if recreated {
        tracing::warn!(upload_id = %id, "upload file was missing; chunks must be sent again");
        uploads::clear_chunks(&state.pool, id).await?;
    }
    Ok(current)
}

/// Refuses an upload that would leave less than `min_free_bytes`, counting
/// the bytes open sessions have declared but not written yet (their sparse
/// files grow into that space). One user's reservations together may take at
/// most [`MAX_PENDING_SHARE_PERCENT`] of the usable space, except for their
/// only open upload: a single large file must stay possible.
async fn ensure_disk_space(state: &AppState, owner_id: Uuid, size: u64) -> Result<(), ApiError> {
    let available = state.available_space().await?;
    let pending = u64::try_from(uploads::pending_bytes(&state.pool).await?).unwrap_or_default();
    let room = available.saturating_sub(state.config.min_free_bytes);
    if room.saturating_sub(pending) < size {
        tracing::warn!(
            available,
            pending,
            size,
            "refusing upload: not enough free disk space"
        );
        return Err(ApiError::StorageFull);
    }
    let mine =
        u64::try_from(uploads::pending_bytes_of(&state.pool, owner_id).await?).unwrap_or_default();
    if exceeds_share(mine, size, room) {
        tracing::warn!(user_id = %owner_id, mine, size, room,
            "refusing upload: the user's pending uploads would take most of the free space");
        return Err(ApiError::TooManyUploads);
    }
    Ok(())
}

/// Share of the usable free space one user's open uploads may reserve.
const MAX_PENDING_SHARE_PERCENT: u64 = 50;

/// Whether adding `size` to a user's `mine` pending bytes takes more than
/// their share of `room`. A first reservation is never refused here.
fn exceeds_share(mine: u64, size: u64, room: u64) -> bool {
    mine > 0 && mine.saturating_add(size) > room / 100 * MAX_PENDING_SHARE_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_upload_may_use_all_the_room() {
        assert!(!exceeds_share(0, 90, 100));
    }

    #[test]
    fn further_uploads_stay_within_half_the_room() {
        assert!(!exceeds_share(20, 30, 100));
        assert!(exceeds_share(20, 31, 100));
        assert!(exceeds_share(60, 1, 100));
    }
}
