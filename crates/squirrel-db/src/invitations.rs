use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

const INVITATION_COLUMNS: &str =
    "id, created_by, email_hint, max_uses, used_count, expires_at, revoked_at, created_at";
const LIST_LIMIT: i64 = 200;

#[derive(Debug, Clone, FromRow)]
pub struct InvitationRecord {
    pub id: Uuid,
    pub created_by: Uuid,
    pub email_hint: Option<String>,
    pub max_uses: i32,
    pub used_count: i32,
    pub expires_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitationStatus {
    Active,
    Used,
    Expired,
    Revoked,
}

impl InvitationRecord {
    pub fn status(&self, now: OffsetDateTime) -> InvitationStatus {
        if self.revoked_at.is_some() {
            InvitationStatus::Revoked
        } else if self.used_count >= self.max_uses {
            InvitationStatus::Used
        } else if self.expires_at <= now {
            InvitationStatus::Expired
        } else {
            InvitationStatus::Active
        }
    }
}

pub struct NewInvitation<'a> {
    pub created_by: Uuid,
    pub token_hash: &'a [u8],
    pub email_hint: Option<&'a str>,
    pub expires_at: OffsetDateTime,
}

pub async fn create(pool: &PgPool, new: &NewInvitation<'_>) -> Result<InvitationRecord, DbError> {
    let sql = format!(
        "INSERT INTO invitations (id, token_hash, created_by, email_hint, expires_at) \
         VALUES ($1, $2, $3, $4, $5) RETURNING {INVITATION_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, InvitationRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new.token_hash)
        .bind(new.created_by)
        .bind(new.email_hint)
        .bind(new.expires_at)
        .fetch_one(pool)
        .await?)
}

pub async fn list_created_by(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<InvitationRecord>, DbError> {
    let sql = format!(
        "SELECT {INVITATION_COLUMNS} FROM invitations WHERE created_by = $1 \
         ORDER BY created_at DESC, id DESC LIMIT $2"
    );
    Ok(sqlx::query_as::<_, InvitationRecord>(&sql)
        .bind(user_id)
        .bind(LIST_LIMIT)
        .fetch_all(pool)
        .await?)
}

/// Returns `false` when the invitation does not exist, is not the caller's, or is already revoked.
pub async fn revoke(pool: &PgPool, id: Uuid, created_by: Uuid) -> Result<bool, DbError> {
    let result = sqlx::query(
        "UPDATE invitations SET revoked_at = now() WHERE id = $1 AND created_by = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(created_by)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// SQL predicate (over `invitations i`, `users u`, `instance_settings s`) shared by
/// `is_redeemable` and the claim in `users::create_with_invitation`: the invitation is
/// live, its creator is enabled, and admin-only mode is respected.
pub(crate) const REDEEMABLE_PREDICATE: &str = "i.token_hash = $1 AND u.id = i.created_by \
     AND i.revoked_at IS NULL AND i.expires_at > now() AND i.used_count < i.max_uses \
     AND u.disabled_at IS NULL AND (NOT s.invites_admin_only OR u.role = 'admin')";

pub async fn is_redeemable(pool: &PgPool, token_hash: &[u8]) -> Result<bool, DbError> {
    let sql = format!(
        "SELECT EXISTS (SELECT 1 FROM invitations i, users u, instance_settings s \
         WHERE {REDEEMABLE_PREDICATE})"
    );
    Ok(sqlx::query_scalar(&sql)
        .bind(token_hash)
        .fetch_one(pool)
        .await?)
}
