//! Opening an upload: dedup against the caller's library, resume, or a new session.

use squirrel_core::{ContentHash, NodeName, UploadPlan};
use squirrel_db::nodes::{self, NewFile, NodeRecord};
use squirrel_db::quota::{self, Pending};
use squirrel_db::uploads::{self, NewUpload, UploadRecord};
use squirrel_db::{DbError, blobs};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::blocking::run_storage;
use crate::error::ApiError;
use crate::files_dto::CreateUploadRequest;
use crate::state::AppState;

/// A validated `CreateUploadRequest`.
pub struct UploadInput {
    pub parent_id: Uuid,
    pub name: NodeName,
    pub hash: ContentHash,
    /// Fits in `i64` (checked by `parse`).
    pub size: u64,
}

impl UploadInput {
    pub fn parse(request: CreateUploadRequest) -> Result<Self, ApiError> {
        if i64::try_from(request.size).is_err() {
            return Err(ApiError::Validation("the file is too large".into()));
        }
        Ok(Self {
            parent_id: request.parent_id,
            name: NodeName::parse(&request.name)?,
            hash: ContentHash::parse(&request.sha256)?,
            size: request.size,
        })
    }

    fn db_size(&self) -> i64 {
        i64::try_from(self.size).expect("checked in parse")
    }
}

/// Dedup within the caller's own library: if one of their nodes already
/// references this content, link a new node to it. `None` otherwise — even when
/// another user has the content (never revealed).
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
        },
    )
    .await?;
    tx.commit().await.map_err(DbError::from)?;
    tracing::info!(user_id = %owner_id, node_id = %node.id, "upload deduplicated");
    Ok(Some(node))
}

/// Resumes the caller's open session for this exact file, or opens a new one
/// (after checking disk space and quota) with its sparse upload file.
pub async fn open_session(
    state: &AppState,
    owner_id: Uuid,
    input: &UploadInput,
) -> Result<UploadRecord, ApiError> {
    let pool = &state.pool;
    let resumable = uploads::find_resumable(
        pool,
        owner_id,
        input.parent_id,
        &input.name,
        &input.hash,
        input.db_size(),
    )
    .await?;
    if let Some(existing) = resumable {
        // Recreates the file if it went missing; keeps received bytes otherwise.
        let (store, id, size) = (state.storage.clone(), existing.id, input.size);
        run_storage(move || store.create_upload(id, size)).await?;
        return Ok(existing);
    }
    ensure_disk_space(state, input.size).await?;
    let plan = UploadPlan::new(input.size, state.config.upload_chunk_size)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("upload_chunk_size must be positive")))?;
    let chunk_count = i32::try_from(plan.chunk_count())
        .map_err(|_| ApiError::Validation("the file is too large".into()))?;
    let chunk_size = i64::try_from(plan.chunk_size())
        .map_err(|_| ApiError::Internal(anyhow::anyhow!("upload_chunk_size is too large")))?;
    let record = uploads::create(
        pool,
        &NewUpload {
            owner_id,
            parent_id: input.parent_id,
            name: &input.name,
            sha256: &input.hash,
            size: input.db_size(),
            chunk_size,
            chunk_count,
            expires_at: OffsetDateTime::now_utc() + state.config.upload_ttl,
        },
    )
    .await?;
    let (store, id) = (state.storage.clone(), record.id);
    if let Err(err) = run_storage(move || store.create_upload(id, plan.size())).await {
        uploads::delete(pool, record.id).await?;
        return Err(err);
    }
    tracing::info!(user_id = %owner_id, upload_id = %record.id, size = input.size, "upload started");
    Ok(record)
}

async fn ensure_disk_space(state: &AppState, size: u64) -> Result<(), ApiError> {
    let store = state.storage.clone();
    let available = run_storage(move || store.available_space()).await?;
    if available.saturating_sub(state.config.min_free_bytes) < size {
        tracing::warn!(
            available,
            size,
            "refusing upload: not enough free disk space"
        );
        return Err(ApiError::StorageFull);
    }
    Ok(())
}
