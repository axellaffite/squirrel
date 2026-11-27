//! The files of a public link: thumbnails, previews, the bytes and zips.
//! Download slots, preview renders and zip tickets are counted per link.

use axum::Json;
use axum::extract::State;
use axum::http::header::CACHE_CONTROL;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum_extra::extract::CookieJar;
use uuid::Uuid;

use crate::archive_dto::{ArchiveTicket, CreateArchiveRequest};
use crate::error::{ApiError, ErrorBody};
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::routes::content::ContentQuery;
use crate::routes::thumbnails::ThumbnailQuery;
use crate::routes::{archive, content, thumbnails};
use crate::state::AppState;

use super::{file, open};

#[utoipa::path(get, path = "/api/public/{token}/files/{id}/thumbnail", security(()), tag = "public",
    params(
        ("token" = String, Path, description = "The token from the link"),
        ("id" = Uuid, Path, description = "A file inside the link"),
        ThumbnailQuery,
    ),
    responses(
        (status = 200, description = "WebP, or JPEG for an opaque photo preview", content(
            (Vec<u8> = "image/webp"),
            (Vec<u8> = "image/jpeg"),
        )),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody, description = "not_found, or not_ready while the worker has not produced it"),
        (status = 429, body = ErrorBody),
        (status = 503, body = ErrorBody),
    ))]
pub async fn public_thumbnail(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath((token, id)): ApiPath<(String, Uuid)>,
    ApiQuery(query): ApiQuery<ThumbnailQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    thumbnails::check_px(&query)?;
    let link = open(&state, &token, &jar).await?;
    let node = file(&state, &link, id).await?;
    let mut response = thumbnails::serve(&state, &node, link.id, &query, &headers).await?;
    // Revalidated every time (ETags keep it cheap): a revoked link stops showing them.
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    Ok(response)
}

#[utoipa::path(get, path = "/api/public/{token}/files/{id}/content", security(()), tag = "public",
    params(
        ("token" = String, Path, description = "The token from the link"),
        ("id" = Uuid, Path, description = "A file inside the link"),
        ContentQuery,
    ),
    responses(
        (status = 200, body = Vec<u8>, description = "The file; active content types are served as application/octet-stream"),
        (status = 206, body = Vec<u8>, description = "One byte range"),
        (status = 304, description = "Unchanged (If-None-Match)"),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody),
        (status = 416, body = ErrorBody),
        (status = 503, body = ErrorBody, description = "Too many downloads in progress"),
    ))]
pub async fn public_file_content(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath((token, id)): ApiPath<(String, Uuid)>,
    ApiQuery(query): ApiQuery<ContentQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let link = open(&state, &token, &jar).await?;
    let node = file(&state, &link, id).await?;
    content::serve(&state, &node, link.id, query.download, &headers).await
}

#[utoipa::path(post, path = "/api/public/{token}/archive", security(()), tag = "public",
    params(("token" = String, Path, description = "The token from the link")),
    request_body = CreateArchiveRequest,
    responses(
        (status = 201, body = ArchiveTicket, description = "Download it with `GET /api/public/{token}/archive/{ticket}` within 10 minutes"),
        (status = 400, body = ErrorBody, description = "No ids, more than 500, or a folder among them"),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody, description = "An id outside the link"),
        (status = 413, body = ErrorBody, description = "archive_too_large: the zip would reach 4 GiB"),
        (status = 422, body = ErrorBody),
    ))]
pub async fn public_create_archive(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath(token): ApiPath<String>,
    ApiJson(request): ApiJson<CreateArchiveRequest>,
) -> Result<(StatusCode, Json<ArchiveTicket>), ApiError> {
    let link = open(&state, &token, &jar).await?;
    let mut found = Vec::new();
    for id in archive::checked_ids(&request.node_ids)? {
        found.push(super::node(&state, &link, id).await?);
    }
    let ticket = archive::ticket_for(&state, link.id, &found)?;
    Ok((StatusCode::CREATED, Json(ticket)))
}

#[utoipa::path(get, path = "/api/public/{token}/archive/{ticket}", security(()), tag = "public",
    params(
        ("token" = String, Path, description = "The token from the link"),
        ("ticket" = String, Path, description = "`ticket` from `POST /api/public/{token}/archive`"),
    ),
    responses(
        (status = 200, body = Vec<u8>, content_type = "application/zip", description = "The zip, streamed"),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody, description = "Unknown or expired ticket, another link's, or a file deleted since"),
        (status = 503, body = ErrorBody, description = "Too many downloads in progress"),
    ))]
pub async fn public_download_archive(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath((token, ticket)): ApiPath<(String, String)>,
) -> Result<Response, ApiError> {
    let link = open(&state, &token, &jar).await?;
    // Every file must still be inside the link (one may have moved out since).
    let archive = state
        .archives
        .get(&ticket, link.id)
        .ok_or(ApiError::NotFound)?;
    for file in &archive.files {
        super::node(&state, &link, file.node_id).await?;
    }
    archive::stream(&state, link.id, &ticket).await
}
