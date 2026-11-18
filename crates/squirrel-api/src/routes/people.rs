//! The caller's people: list, show, rename, hide, merge, split, and their
//! faces. Reads accept the phone; changes need the browser session (the phone
//! is read-only for people in v1). Another user's person is `not_found`.

use axum::Json;
use axum::extract::State;
use squirrel_db::people::{self, Edit, PersonUpdate};
use squirrel_db::timeline::TimelineCursor;
use uuid::Uuid;

use crate::cursor;
use crate::error::{ApiError, ErrorBody};
use crate::extract::{Caller, CurrentUser};
use crate::input::{ApiJson, ApiPath, ApiQuery};
use crate::people_dto::{
    FaceDto, FacesPage, FacesQuery, MAX_MERGE, MAX_SPLIT, MergePeopleRequest, PeopleQuery,
    PersonDto, SplitPersonRequest, UpdatePersonRequest, check_ids, person_name,
};
use crate::state::AppState;

/// A person edit's outcome as a response.
pub(crate) fn edited<T>(edit: Edit<T>, not_theirs: &str) -> Result<T, ApiError> {
    match edit {
        Edit::Done(value) => Ok(value),
        Edit::NotFound => Err(ApiError::NotFound),
        Edit::NotTheirs => Err(ApiError::Validation(not_theirs.into())),
    }
}

#[utoipa::path(get, path = "/api/people", security(("device_bearer" = []), ("session_cookie" = [])), tag = "people",
    params(PeopleQuery),
    responses((status = 200, body = Vec<PersonDto>, description = "Named first, then by face count; at most 5000")))]
pub async fn list_people(
    State(state): State<AppState>,
    caller: Caller,
    ApiQuery(query): ApiQuery<PeopleQuery>,
) -> Result<Json<Vec<PersonDto>>, ApiError> {
    let records = people::list(
        &state.pool,
        caller.user.id,
        query.include_hidden.unwrap_or(false),
    )
    .await?;
    Ok(Json(records.into_iter().map(PersonDto::from).collect()))
}

#[utoipa::path(get, path = "/api/people/{id}", security(("device_bearer" = []), ("session_cookie" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Person id")),
    responses((status = 200, body = PersonDto), (status = 404, body = ErrorBody)))]
pub async fn get_person(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<PersonDto>, ApiError> {
    let record = people::find(&state.pool, caller.user.id, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(record.into()))
}

#[utoipa::path(patch, path = "/api/people/{id}", security(("session_cookie" = [], "csrf_token" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Person id")),
    request_body = UpdatePersonRequest,
    responses((status = 200, body = PersonDto), (status = 404, body = ErrorBody),
              (status = 422, body = ErrorBody)))]
pub async fn update_person(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<UpdatePersonRequest>,
) -> Result<Json<PersonDto>, ApiError> {
    if request.name.is_none() && request.hidden.is_none() && request.cover_face_id.is_none() {
        return Err(ApiError::Validation(
            "give a name, hidden or cover_face_id".into(),
        ));
    }
    let changes = PersonUpdate {
        name: request
            .name
            .map(|name| name.as_deref().map(person_name).transpose())
            .transpose()?,
        hidden: request.hidden,
        cover_face_id: request.cover_face_id,
    };
    let edit = people::update(&state.pool, current.user.id, id, &changes).await?;
    let record = edited(edit, "cover_face_id is not one of this person's faces")?;
    Ok(Json(record.into()))
}

#[utoipa::path(post, path = "/api/people/{id}/merge", security(("session_cookie" = [], "csrf_token" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "The person that keeps the faces")),
    request_body = MergePeopleRequest,
    responses((status = 200, body = PersonDto, description = "The merged person; the others are gone"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn merge_people(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<MergePeopleRequest>,
) -> Result<Json<PersonDto>, ApiError> {
    check_ids("from", &request.from, MAX_MERGE)?;
    if request.from.contains(&id) {
        return Err(ApiError::Validation(
            "a person cannot be merged into itself".into(),
        ));
    }
    let edit = people::merge(&state.pool, current.user.id, id, &request.from).await?;
    Ok(Json(edited(edit, "unknown person")?.into()))
}

#[utoipa::path(post, path = "/api/people/{id}/split", security(("session_cookie" = [], "csrf_token" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "The person the faces leave")),
    request_body = SplitPersonRequest,
    responses((status = 200, body = PersonDto, description = "The new person"),
              (status = 404, body = ErrorBody), (status = 422, body = ErrorBody)))]
pub async fn split_person(
    State(state): State<AppState>,
    current: CurrentUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(request): ApiJson<SplitPersonRequest>,
) -> Result<Json<PersonDto>, ApiError> {
    check_ids("face_ids", &request.face_ids, MAX_SPLIT)?;
    let name = request.name.as_deref().map(person_name).transpose()?;
    let edit = people::split(
        &state.pool,
        current.user.id,
        id,
        &request.face_ids,
        name.as_deref(),
    )
    .await?;
    Ok(Json(
        edited(edit, "face_ids must all be faces of this person")?.into(),
    ))
}

#[utoipa::path(get, path = "/api/people/{id}/faces", security(("device_bearer" = []), ("session_cookie" = [])), tag = "people",
    params(("id" = Uuid, Path, description = "Person id"), FacesQuery),
    responses((status = 200, body = FacesPage), (status = 400, body = ErrorBody),
              (status = 404, body = ErrorBody)))]
pub async fn person_faces(
    State(state): State<AppState>,
    caller: Caller,
    ApiPath(id): ApiPath<Uuid>,
    ApiQuery(query): ApiQuery<FacesQuery>,
) -> Result<Json<FacesPage>, ApiError> {
    let owner = caller.user.id;
    if people::find(&state.pool, owner, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let after = query
        .cursor
        .as_deref()
        .map(cursor::decode_timeline)
        .transpose()?
        .map(|c| (c.taken_at, c.node_id));
    let limit = cursor::page_size(query.limit);
    let mut rows =
        people::faces_of_person(&state.pool, owner, id, after, i64::from(limit) + 1).await?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = more.then(|| rows.last()).flatten().map(|last| {
        cursor::encode_timeline(&TimelineCursor {
            taken_at: last.taken_at,
            node_id: last.id,
        })
    });
    Ok(Json(FacesPage {
        items: rows.into_iter().map(FaceDto::from).collect(),
        next_cursor,
    }))
}
