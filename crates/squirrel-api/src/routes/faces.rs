//! Faces on one photo, and assigning a face by hand. Every assignment pins
//! the face: clustering never moves it again. Changes need the browser
//! session (the phone is read-only for people in v1).

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use squirrel_db::people::{self, Assignment};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{ApiError, ErrorBody};
use crate::extract::{Caller, CurrentUser};
use crate::input::{ApiJson, ApiPath};
use crate::people_dto::{AssignFaceRequest, FaceDto, person_name};
use crate::routes::people::edited;
use crate::state::AppState;

#[utoipa::path(get, path = "/api/photos/{id}/faces", security(("device_bearer" = []), ("session_cookie" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Photo (file node) id")),
    responses((status = 200, body = Vec<FaceDto>, description = "Left to right"),
              (status = 404, body = ErrorBody)))]
pub async fn photo_faces(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<FaceDto>>, ApiError> {
    let faces = people::faces_of_photo(&state.pool, caller.user.id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(faces.into_iter().map(FaceDto::from).collect()))
}

/// The face's person after an assignment.
#[derive(Debug, Serialize, ToSchema)]
pub struct FaceAssigned {
    pub face_id: Uuid,
    /// `null`: pinned to nobody.
    pub person_id: Option<Uuid>,
}

#[utoipa::path(put, path = "/api/faces/{id}/person", security(("session_cookie" = [], "csrf_token" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Face id")),
    request_body = AssignFaceRequest,
    responses((status = 200, body = FaceAssigned), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn assign_face(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<AssignFaceRequest>,
) -> Result<Json<FaceAssigned>, ApiError> {
    let to = match (request.person_id, request.new_person) {
        (Some(person), None) => Assignment::Person(person),
        (None, Some(new)) => Assignment::NewPerson {
            name: new.name.as_deref().map(person_name).transpose()?,
        },
        _ => {
            return Err(ApiError::Validation(
                "give exactly one of person_id and new_person".into(),
            ));
        }
    };
    let edit = people::assign_face(&state.pool, current.user.id, id, &to).await?;
    let person_id = edited(edit, "unknown person")?;
    Ok(Json(FaceAssigned {
        face_id: id,
        person_id,
    }))
}

#[utoipa::path(delete, path = "/api/faces/{id}/person", security(("session_cookie" = [], "csrf_token" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Face id")),
    responses((status = 200, body = FaceAssigned, description = "Not this person: pinned to nobody, never grouped again"),
              (status = 404, body = ErrorBody)))]
pub async fn unassign_face(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<FaceAssigned>, ApiError> {
    let edit = people::assign_face(&state.pool, current.user.id, id, &Assignment::Nobody).await?;
    let person_id = edited(edit, "unknown person")?;
    Ok(Json(FaceAssigned {
        face_id: id,
        person_id,
    }))
}
