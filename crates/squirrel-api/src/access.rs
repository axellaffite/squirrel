//! Access checks shared by the routes. No access at all is `not_found` (an
//! unshared item of another user stays indistinguishable from a missing one);
//! reading without the right to change is `forbidden`.

use squirrel_core::NodeKind;
use squirrel_db::access::{self, Access, AlbumAccess, NodeAccess};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// The live node `id` as `user` may see it, else `not_found`.
pub async fn node(state: &AppState, user: Uuid, id: Uuid) -> Result<NodeAccess, ApiError> {
    access::node(&state.pool, user, id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// A live folder `user` may see.
pub async fn folder(state: &AppState, user: Uuid, id: Uuid) -> Result<NodeAccess, ApiError> {
    let found = node(state, user, id).await?;
    if found.node.kind() != NodeKind::Folder {
        return Err(ApiError::NotFound);
    }
    Ok(found)
}

/// A live file `user` may see.
pub async fn file(state: &AppState, user: Uuid, id: Uuid) -> Result<NodeAccess, ApiError> {
    let found = node(state, user, id).await?;
    if found.node.kind() != NodeKind::File {
        return Err(ApiError::NotFound);
    }
    Ok(found)
}

/// A live folder `user` may add to (create, upload, move into).
pub async fn writable_folder(
    state: &AppState,
    user: Uuid,
    id: Uuid,
) -> Result<NodeAccess, ApiError> {
    let found = folder(state, user, id).await?;
    require(found.access.can_write())?;
    Ok(found)
}

/// A node `user` may rename, move or trash: their own, or one inside a folder
/// shared with them for writing. The shared folder itself stays its owner's.
pub async fn changeable(state: &AppState, user: Uuid, id: Uuid) -> Result<NodeAccess, ApiError> {
    let found = node(state, user, id).await?;
    if found.access.is_owner() {
        return Ok(found);
    }
    let parent_writable = match found.node.parent_id {
        Some(parent) => access::node(&state.pool, user, parent)
            .await?
            .is_some_and(|parent| parent.access.can_write()),
        None => false,
    };
    require(parent_writable)?;
    Ok(found)
}

/// Album `id` as `user` may see it, else `not_found`.
pub async fn album(state: &AppState, user: Uuid, id: Uuid) -> Result<AlbumAccess, ApiError> {
    access::album(&state.pool, user, id)
        .await?
        .ok_or(ApiError::NotFound)
}

/// `forbidden` unless `allowed`.
pub fn require(allowed: bool) -> Result<(), ApiError> {
    if allowed {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

/// `forbidden` unless `access` is the owner's.
pub fn require_owner(access: Access) -> Result<(), ApiError> {
    require(access.is_owner())
}
