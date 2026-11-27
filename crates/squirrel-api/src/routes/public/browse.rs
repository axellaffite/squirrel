//! Browsing what a public link opens: a folder's tree or an album's photos.

use axum::Json;
use axum::extract::State;
use axum_extra::extract::CookieJar;
use squirrel_db::albums;
use squirrel_db::nodes::{self, ListCursor};
use squirrel_db::shares::Subject;

use crate::albums_dto::AlbumItemsQuery;
use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::files_dto::{NodeDto, PathSegmentDto};
use crate::input::{ApiPath, ApiQuery};
use crate::photos_dto::TimelinePage;
use crate::public_links_dto::{PublicFolderListing, PublicNodesQuery};
use crate::routes::nodes::thumbnails_of;
use crate::routes::photos::{photo_page_size, timeline_page};
use crate::state::AppState;

use super::{node, open, visible_dto};

#[utoipa::path(get, path = "/api/public/{token}/nodes", security(()), tag = "public",
    params(("token" = String, Path, description = "The token from the link"), PublicNodesQuery),
    responses(
        (status = 200, body = PublicFolderListing),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody, description = "Not a folder link, or a folder outside it"),
    ))]
pub async fn public_list_nodes(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath(token): ApiPath<String>,
    ApiQuery(query): ApiQuery<PublicNodesQuery>,
) -> Result<Json<PublicFolderListing>, ApiError> {
    let link = open(&state, &token, &jar).await?;
    let Subject::Node(root) = link.subject() else {
        return Err(ApiError::NotFound);
    };
    let folder = node(&state, &link, query.parent_id.unwrap_or(root)).await?;
    if !folder.is_folder() {
        return Err(ApiError::NotFound);
    }
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_folder)
        .transpose()?;
    let page = nodes::list_children(
        &state.pool,
        folder.owner_id,
        Some(folder.id),
        after.as_ref(),
        cursor::page_size(query.limit),
    )
    .await?;
    let next_cursor = page.items.last().filter(|_| page.has_more).map(|last| {
        cursor::encode_folder(&ListCursor {
            kind: last.kind(),
            name: last.name.clone(),
        })
    });
    let path = nodes::path_to(&state.pool, folder.owner_id, folder.id)
        .await?
        .into_iter()
        .skip_while(|segment| segment.id != root)
        .map(|segment| PathSegmentDto {
            id: segment.id,
            name: segment.name,
        })
        .collect();
    let thumbnails = thumbnails_of(&state, &page.items).await?;
    Ok(Json(PublicFolderListing {
        folder: visible_dto(&state, &link, &folder).await?,
        path,
        items: page
            .items
            .iter()
            .map(|child| NodeDto::with_thumbnails(child, &thumbnails))
            .collect(),
        next_cursor,
    }))
}

#[utoipa::path(get, path = "/api/public/{token}/album/items", security(()), tag = "public",
    params(("token" = String, Path, description = "The token from the link"), AlbumItemsQuery),
    responses(
        (status = 200, body = TimelinePage, description = "The album's photos, newest first"),
        (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody, description = "link_locked: give the password first"),
        (status = 404, body = ErrorBody, description = "Not an album link"),
    ))]
pub async fn public_album_items(
    State(state): State<AppState>,
    jar: CookieJar,
    ApiPath(token): ApiPath<String>,
    ApiQuery(query): ApiQuery<AlbumItemsQuery>,
) -> Result<Json<TimelinePage>, ApiError> {
    let link = open(&state, &token, &jar).await?;
    let Subject::Album(album) = link.subject() else {
        return Err(ApiError::NotFound);
    };
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?;
    let page = albums::items(
        &state.pool,
        album,
        after.as_ref(),
        photo_page_size(query.limit),
    )
    .await?;
    Ok(Json(timeline_page(page)))
}
