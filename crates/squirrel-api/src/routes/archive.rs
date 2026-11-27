//! Several files downloaded as one zip: `POST` checks the files and returns a
//! ticket, `GET` streams the zip (store-only, built on the fly).

use std::collections::HashSet;
use std::io::{self, Seek, SeekFrom};
use std::sync::Arc;

use axum::Json;
use axum::body::Body;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE, X_CONTENT_TYPE_OPTIONS,
};
use axum::response::{IntoResponse, Response};
use squirrel_db::nodes::{self, NodeRecord};
use squirrel_storage::{BlobStore, StorageError};
use time::{Date, OffsetDateTime};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::access;
use crate::archive_dto::{ArchiveTicket, CreateArchiveRequest, MAX_ARCHIVE_FILES};
use crate::archives::{Archive, ArchiveFile};
use crate::content::{blob_hash, content_disposition, to_header_map};
use crate::downloads::HeldReader;
use crate::error::{ApiError, ErrorBody};
use crate::extract::Caller;
use crate::holder::Holder;
use crate::input::{ApiJson, ApiPath};
use crate::state::AppState;
use crate::zip::{self, ArchiveWriter, DosDateTime, Entry};

const STREAM_BUFFER: usize = 64 * 1024;

#[utoipa::path(post, path = "/api/files/archive", security(("device_bearer" = []), ("session_cookie" = [], "csrf_token" = [])), tag = "files",
    request_body = CreateArchiveRequest,
    responses(
        (status = 201, body = ArchiveTicket, description = "Download it with `GET /api/files/archive/{ticket}` within 10 minutes"),
        (status = 400, body = ErrorBody, description = "No ids, more than 500, or a folder among them"),
        (status = 404, body = ErrorBody, description = "An id the caller cannot download"),
        (status = 413, body = ErrorBody, description = "archive_too_large: the zip would reach 4 GiB"),
        (status = 422, body = ErrorBody),
    ))]
pub async fn create_archive(
    State(state): State<AppState>,
    caller: Caller,
    ApiJson(request): ApiJson<CreateArchiveRequest>,
) -> Result<(StatusCode, Json<ArchiveTicket>), ApiError> {
    let mut found = Vec::new();
    for id in checked_ids(&request.node_ids)? {
        found.push(access::node(&state, caller.user.id, id).await?.node);
    }
    let ticket = ticket_for(&state, caller.user.id, &found)?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// The requested ids, without repeats, refused when there are none or too many.
pub(crate) fn checked_ids(requested: &[Uuid]) -> Result<Vec<Uuid>, ApiError> {
    let ids = unique(requested);
    if ids.is_empty() || ids.len() > MAX_ARCHIVE_FILES {
        return Err(ApiError::BadRequest(format!(
            "select between 1 and {MAX_ARCHIVE_FILES} files"
        )));
    }
    Ok(ids)
}

/// A ticket for the zip of `nodes` (checked readable), downloadable by
/// `holder` (a user, or a public link) only. Folders are refused.
pub(crate) fn ticket_for(
    state: &AppState,
    holder: Uuid,
    nodes: &[NodeRecord],
) -> Result<ArchiveTicket, ApiError> {
    if nodes.iter().any(NodeRecord::is_folder) {
        return Err(ApiError::BadRequest(
            "folders cannot be put in a zip: select files only".into(),
        ));
    }
    let archive = archive_of(holder, nodes, OffsetDateTime::now_utc().date())?;
    Ok(ArchiveTicket {
        name: archive.name.clone(),
        size: i64::try_from(archive.size).map_err(|err| ApiError::Internal(err.into()))?,
        count: i64::try_from(archive.files.len()).map_err(|err| ApiError::Internal(err.into()))?,
        ticket: state.archives.insert(archive),
    })
}

/// `ids` without repeats, first occurrence kept.
fn unique(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

/// The zip of `files` (live file nodes, in order), refused at 4 GiB.
fn archive_of(owner_id: Uuid, nodes: &[NodeRecord], today: Date) -> Result<Archive, ApiError> {
    let names = zip::entry_names(nodes.iter().map(|node| node.name.as_str()));
    let files = nodes
        .iter()
        .zip(names)
        .map(|(node, name)| {
            Ok(ArchiveFile {
                node_id: node.id,
                hash: blob_hash(node)?,
                entry: Entry {
                    name,
                    size: u64::try_from(node.size).map_err(|err| ApiError::Internal(err.into()))?,
                    modified: DosDateTime::from_utc(node.modified_at),
                },
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let entries: Vec<Entry> = files.iter().map(|file| file.entry.clone()).collect();
    let size = zip::archive_size(&entries);
    if size >= zip::SIZE_LIMIT {
        return Err(ApiError::ArchiveTooLarge);
    }
    Ok(Archive {
        owner_id,
        name: archive_name(files.len(), nodes.iter().all(is_photo), today),
        size,
        files,
    })
}

/// `Squirrel 3 photos 2026-11-26.zip` when every file is a photo or a video
/// (by its recorded MIME type), else `Squirrel 3 files 2026-11-26.zip`.
fn archive_name(count: usize, all_photos: bool, today: Date) -> String {
    let noun = match (all_photos, count) {
        (true, 1) => "photo",
        (true, _) => "photos",
        (false, 1) => "file",
        (false, _) => "files",
    };
    format!(
        "Squirrel {count} {noun} {:04}-{:02}-{:02}.zip",
        today.year(),
        u8::from(today.month()),
        today.day()
    )
}

fn is_photo(node: &NodeRecord) -> bool {
    node.mime
        .as_deref()
        .is_some_and(|mime| mime.starts_with("image/") || mime.starts_with("video/"))
}

#[utoipa::path(get, path = "/api/files/archive/{ticket}", security(("device_bearer" = []), ("session_cookie" = [])), tag = "files",
    params(("ticket" = String, Path, description = "`ticket` from `POST /api/files/archive`")),
    responses(
        (status = 200, body = Vec<u8>, content_type = "application/zip", description = "The zip, streamed; `Content-Length` is the ticket's `size`"),
        (status = 404, body = ErrorBody, description = "Unknown or expired ticket, another user's, or a file deleted since"),
        (status = 503, body = ErrorBody, description = "Too many downloads in progress"),
    ))]
pub async fn download_archive(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(ticket): ApiPath<String>,
) -> Result<Response, ApiError> {
    stream(&state, Holder::User(caller.user.id), &ticket).await
}

/// Streams the zip of `ticket`, if `holder` created it and its files are all
/// still live.
pub(crate) async fn stream(
    state: &AppState,
    holder: Holder,
    ticket: &str,
) -> Result<Response, ApiError> {
    let archive = state
        .archives
        .get(ticket, holder.id())
        .ok_or(ApiError::NotFound)?;
    let ids: Vec<Uuid> = archive.files.iter().map(|file| file.node_id).collect();
    if nodes::find_live_many(&state.pool, &ids).await?.len() != ids.len() {
        return Err(ApiError::NotFound);
    }
    let slot = state.download_slot(holder).await?;
    let headers = to_header_map(vec![
        (CONTENT_TYPE, "application/zip".to_owned()),
        (CONTENT_LENGTH, archive.size.to_string()),
        (
            CONTENT_DISPOSITION,
            content_disposition(false, &archive.name),
        ),
        (CACHE_CONTROL, "no-store".to_owned()),
        (X_CONTENT_TYPE_OPTIONS, "nosniff".to_owned()),
    ])?;
    let (writer, reader) = tokio::io::duplex(STREAM_BUFFER);
    tokio::spawn(write_archive(state.storage.clone(), archive, writer));
    // The slot travels with the body, released when it ends or the client leaves.
    let body = Body::from_stream(ReaderStream::with_capacity(
        HeldReader::new(reader, slot),
        STREAM_BUFFER,
    ));
    Ok((StatusCode::OK, headers, body).into_response())
}

/// Why writing a zip stopped early.
#[derive(Debug, thiserror::Error)]
enum WriteError {
    #[error("blob missing from the data directory")]
    Missing { node_id: Uuid },
    #[error("blob size {found} differs from the recorded {expected}")]
    SizeChanged {
        node_id: Uuid,
        expected: u64,
        found: u64,
    },
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// Mostly the client going away (the body was dropped).
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Streams the zip into `out`. A failure ends the body short of its
/// `Content-Length`, which the client sees as an interrupted download.
async fn write_archive(store: BlobStore, archive: Arc<Archive>, out: DuplexStream) {
    match write_entries(&store, &archive, out).await {
        Ok(()) => {}
        Err(WriteError::Io(err)) => {
            tracing::debug!(error = %err, "zip download ended early");
        }
        Err(err @ WriteError::Missing { node_id }) => {
            tracing::error!(%node_id, "{err}");
        }
        Err(err @ WriteError::SizeChanged { node_id, .. }) => {
            tracing::error!(%node_id, "{err}");
        }
        Err(WriteError::Storage(err)) => {
            tracing::error!(error = %err, "cannot read a blob for a zip download");
        }
    }
}

async fn write_entries(
    store: &BlobStore,
    archive: &Archive,
    mut out: impl AsyncWrite + Unpin,
) -> Result<(), WriteError> {
    let mut writer = ArchiveWriter::new();
    for file in &archive.files {
        let (blob, crc) = open_with_crc(store, file).await?;
        out.write_all(&writer.local_header(&file.entry, crc))
            .await?;
        copy_exactly(blob.take(file.entry.size), &mut out).await?;
    }
    out.write_all(&writer.finish()).await?;
    out.shutdown().await?;
    Ok(())
}

/// The blob of `file`, rewound after a first read that computes its CRC-32.
async fn open_with_crc(
    store: &BlobStore,
    file: &ArchiveFile,
) -> Result<(tokio::fs::File, u32), WriteError> {
    let (store, hash, node_id, expected) = (
        store.clone(),
        file.hash.clone(),
        file.node_id,
        file.entry.size,
    );
    let (blob, crc) = tokio::task::spawn_blocking(move || {
        let (mut blob, found) = store.open_blob(&hash).map_err(|err| match err {
            StorageError::NotFound => WriteError::Missing { node_id },
            other => WriteError::Storage(other),
        })?;
        if found != expected {
            return Err(WriteError::SizeChanged {
                node_id,
                expected,
                found,
            });
        }
        let (crc, _) = zip::crc32(&mut blob)?;
        blob.seek(SeekFrom::Start(0))?;
        Ok((blob, crc))
    })
    .await
    .map_err(io::Error::other)??;
    Ok((tokio::fs::File::from_std(blob), crc))
}

async fn copy_exactly(
    mut from: impl AsyncRead + Unpin,
    to: &mut (impl AsyncWrite + Unpin),
) -> io::Result<()> {
    tokio::io::copy(&mut from, to).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    #[test]
    fn names_count_files_or_photos() {
        let day = date!(2026 - 11 - 26);
        assert_eq!(
            archive_name(3, false, day),
            "Squirrel 3 files 2026-11-26.zip"
        );
        assert_eq!(
            archive_name(12, true, day),
            "Squirrel 12 photos 2026-11-26.zip"
        );
        assert_eq!(
            archive_name(1, true, day),
            "Squirrel 1 photo 2026-11-26.zip"
        );
        assert_eq!(
            archive_name(1, false, date!(2027 - 01 - 05)),
            "Squirrel 1 file 2027-01-05.zip"
        );
    }

    #[test]
    fn repeated_ids_keep_their_first_place() {
        let (a, b, c) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        assert_eq!(unique(&[b, a, b, c, a]), [b, a, c]);
    }
}
