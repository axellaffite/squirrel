use serde::{Deserialize, Serialize};
use squirrel_core::Role;
use squirrel_db::users::UserRecord;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AccountRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// Send back as `X-CSRF-Token` on every non-GET request.
    pub csrf_token: String,
}

impl MeResponse {
    pub fn new(user: &UserRecord, csrf_token: String) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role(),
            csrf_token,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatus {
    pub needs_setup: bool,
}
