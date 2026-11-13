use std::fmt;

use sqlx::{FromRow, PgConnection, PgPool};
use squirrel_core::{DisplayName, Role, Username};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;
use crate::error::is_unique_violation;
use crate::invitations::REDEEMABLE_PREDICATE;

/// Arbitrary constant identifying the "first user" advisory lock.
const BOOTSTRAP_LOCK_KEY: i64 = 0x5351_5552_0001;
/// Failed logins before the account is temporarily locked.
pub const LOCKOUT_THRESHOLD: i32 = 5;
/// Name of the UNIQUE constraint on `users.username` (see the initial migration).
const USERNAME_UNIQUE_CONSTRAINT: &str = "users_username_key";

const USER_COLUMNS: &str = "id, username, display_name, password_hash, role, quota_bytes, \
     invited_by, failed_logins, locked_until, created_at, disabled_at";

#[derive(Clone, FromRow)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    role: String,
    pub quota_bytes: Option<i64>,
    pub invited_by: Option<Uuid>,
    pub failed_logins: i32,
    pub locked_until: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub disabled_at: Option<OffsetDateTime>,
}

impl fmt::Debug for UserRecord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserRecord")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("display_name", &self.display_name)
            .field("password_hash", &"<redacted>")
            .field("role", &self.role)
            .field("quota_bytes", &self.quota_bytes)
            .field("invited_by", &self.invited_by)
            .field("failed_logins", &self.failed_logins)
            .field("locked_until", &self.locked_until)
            .field("created_at", &self.created_at)
            .field("disabled_at", &self.disabled_at)
            .finish()
    }
}

impl UserRecord {
    /// The DB CHECK constraint guarantees a known value; fall back to least privilege anyway.
    pub fn role(&self) -> Role {
        self.role.parse().unwrap_or_else(|_| {
            tracing::warn!(user_id = %self.id, role = %self.role, "unknown role in database, treating as user");
            Role::User
        })
    }

    pub fn is_locked(&self, now: OffsetDateTime) -> bool {
        self.locked_until.is_some_and(|until| until > now)
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled_at.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct NewUser {
    pub username: Username,
    pub display_name: DisplayName,
    pub password_hash: String,
}

pub async fn any_exists(pool: &PgPool) -> Result<bool, DbError> {
    Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(pool)
        .await?)
}

/// Creates the admin account only if no user exists yet. Serialized with an
/// advisory transaction lock, so concurrent callers cannot both succeed.
pub async fn create_first_admin(pool: &PgPool, new_user: &NewUser) -> Result<UserRecord, DbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(BOOTSTRAP_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users)")
        .fetch_one(&mut *tx)
        .await?;
    if exists {
        return Err(DbError::NotFirstUser);
    }
    let user = insert(&mut tx, new_user, Role::Admin, None).await?;
    tx.commit().await?;
    Ok(user)
}

/// Atomically consumes one use of the invitation and creates a regular user.
/// Any failure (taken username, invalid token) rolls the invitation use back.
pub async fn create_with_invitation(
    pool: &PgPool,
    new_user: &NewUser,
    token_hash: &[u8],
) -> Result<UserRecord, DbError> {
    let mut tx = pool.begin().await?;
    let claim = format!(
        "UPDATE invitations i SET used_count = i.used_count + 1 \
         FROM users u, instance_settings s WHERE {REDEEMABLE_PREDICATE} \
         RETURNING i.created_by"
    );
    let inviter: Option<Uuid> = sqlx::query_scalar(&claim)
        .bind(token_hash)
        .fetch_optional(&mut *tx)
        .await?;
    let inviter = inviter.ok_or(DbError::InvitationInvalid)?;
    let user = insert(&mut tx, new_user, Role::User, Some(inviter)).await?;
    tx.commit().await?;
    Ok(user)
}

async fn insert(
    conn: &mut PgConnection,
    new_user: &NewUser,
    role: Role,
    invited_by: Option<Uuid>,
) -> Result<UserRecord, DbError> {
    let sql = format!(
        "INSERT INTO users (id, username, display_name, password_hash, role, invited_by) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {USER_COLUMNS}"
    );
    sqlx::query_as::<_, UserRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(new_user.username.as_str())
        .bind(new_user.display_name.as_str())
        .bind(&new_user.password_hash)
        .bind(role.as_str())
        .bind(invited_by)
        .fetch_one(conn)
        .await
        .map_err(|err| {
            if is_unique_violation(&err, USERNAME_UNIQUE_CONSTRAINT) {
                DbError::UsernameTaken
            } else {
                err.into()
            }
        })
}

pub async fn find_by_username(
    pool: &PgPool,
    username: &Username,
) -> Result<Option<UserRecord>, DbError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE username = $1");
    Ok(sqlx::query_as::<_, UserRecord>(&sql)
        .bind(username.as_str())
        .fetch_optional(pool)
        .await?)
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRecord>, DbError> {
    let sql = format!("SELECT {USER_COLUMNS} FROM users WHERE id = $1");
    Ok(sqlx::query_as::<_, UserRecord>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// Increments the failure counter (saturating at 1000); from the threshold on, locks
/// the account for 1, 2, 4… minutes, capped at one hour. The exponent is clamped so
/// the interval arithmetic cannot overflow however many failures accumulate.
pub async fn record_login_failure(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query(
        "UPDATE users SET \
           failed_logins = LEAST(failed_logins + 1, 1000), \
           locked_until = CASE WHEN failed_logins + 1 >= $2 \
             THEN now() + LEAST(interval '1 minute' * power(2, LEAST(failed_logins + 1 - $2, 6)), interval '1 hour') \
             ELSE locked_until END \
         WHERE id = $1",
    )
    .bind(id)
    .bind(LOCKOUT_THRESHOLD)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_login_success(pool: &PgPool, id: Uuid) -> Result<(), DbError> {
    sqlx::query("UPDATE users SET failed_logins = 0, locked_until = NULL WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
