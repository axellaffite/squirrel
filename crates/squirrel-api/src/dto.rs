use std::fmt;

use serde::{Deserialize, Serialize};
use squirrel_core::Role;
use squirrel_db::invitations::{InvitationRecord, InvitationStatus};
use squirrel_db::settings::InstanceSettings;
use squirrel_db::users::UserRecord;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Deserialize, ToSchema)]
pub struct AccountRequest {
    pub username: String,
    pub display_name: String,
    pub password: String,
}

/// Creates the first administrator. `setup_token` is the setup code printed in
/// the server log on first start (or `SQUIRREL_SETUP_TOKEN`).
#[derive(Deserialize, ToSchema)]
pub struct SetupRequest {
    /// Missing is treated like a wrong code (403), not a validation error.
    #[serde(default)]
    #[schema(required = true)]
    pub setup_token: String,
    pub username: String,
    pub display_name: String,
    pub password: String,
}

impl SetupRequest {
    pub fn into_account(self) -> AccountRequest {
        AccountRequest {
            username: self.username,
            display_name: self.display_name,
            password: self.password,
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

/// Shown instead of a secret in `Debug` output.
const REDACTED: &str = "<redacted>";

impl fmt::Debug for LoginRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginRequest")
            .field("username", &self.username)
            .field("password", &REDACTED)
            .finish()
    }
}

impl fmt::Debug for AccountRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountRequest")
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("password", &REDACTED)
            .finish()
    }
}

impl fmt::Debug for SetupRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SetupRequest")
            .field("setup_token", &REDACTED)
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("password", &REDACTED)
            .finish()
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    /// Send back as `X-CSRF-Token` on every non-GET request.
    pub csrf_token: String,
    /// Whether this user may create invitations (instance setting and role).
    pub can_invite: bool,
}

impl MeResponse {
    pub fn new(user: &UserRecord, csrf_token: String, settings: &InstanceSettings) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role(),
            csrf_token,
            can_invite: settings.allows_invites_from(user.role()),
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SetupStatus {
    pub needs_setup: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateInvitationRequest {
    /// Optional reminder of who the invitation is for. Not used to send email.
    #[serde(default)]
    pub email_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatusDto {
    Active,
    Used,
    Expired,
    Revoked,
}

impl From<InvitationStatus> for InvitationStatusDto {
    fn from(status: InvitationStatus) -> Self {
        match status {
            InvitationStatus::Active => Self::Active,
            InvitationStatus::Used => Self::Used,
            InvitationStatus::Expired => Self::Expired,
            InvitationStatus::Revoked => Self::Revoked,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct InvitationDto {
    pub id: Uuid,
    pub email_hint: Option<String>,
    pub status: InvitationStatusDto,
    pub used_count: i32,
    pub max_uses: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

impl InvitationDto {
    pub fn new(record: &InvitationRecord, now: OffsetDateTime) -> Self {
        Self {
            id: record.id,
            email_hint: record.email_hint.clone(),
            status: record.status(now).into(),
            used_count: record.used_count,
            max_uses: record.max_uses,
            expires_at: record.expires_at,
            created_at: record.created_at,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedInvitation {
    pub invitation: InvitationDto,
    /// Shown once. Build the link as `<origin>/join/<token>`.
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinStatus {
    pub valid: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct InstanceSettingsDto {
    pub invites_admin_only: bool,
}

impl From<InstanceSettings> for InstanceSettingsDto {
    fn from(settings: InstanceSettings) -> Self {
        Self {
            invites_admin_only: settings.invites_admin_only,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_never_shows_passwords_or_setup_codes() {
        let login = LoginRequest {
            username: "alice".into(),
            password: "secret-password".into(),
        };
        let setup = SetupRequest {
            setup_token: "secret-code".into(),
            username: "alice".into(),
            display_name: "Alice".into(),
            password: "secret-password".into(),
        };
        let account = AccountRequest {
            username: "bob".into(),
            display_name: "Bob".into(),
            password: "secret-password".into(),
        };
        for shown in [
            format!("{login:?}"),
            format!("{setup:?}"),
            format!("{account:?}"),
        ] {
            assert!(!shown.contains("secret"), "{shown}");
        }
    }
}
