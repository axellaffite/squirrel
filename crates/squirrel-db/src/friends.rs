//! Friends: each account's one-way list of other accounts, picked first when
//! sharing. Adding someone needs no consent and tells them nothing.

use sqlx::{FromRow, PgPool};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::DbError;

/// Friends kept at most per account.
pub const MAX_FRIENDS: i64 = 500;
/// Suggestions listed at most.
pub const MAX_SUGGESTIONS: i64 = 20;

/// A friend, or a suggested one (`added_at` is then the latest sharing between them).
#[derive(Debug, Clone, FromRow)]
pub struct FriendRecord {
    pub username: String,
    pub display_name: String,
    pub added_at: OffsetDateTime,
}

/// Adds `friend_id` to `user_id`'s friends (already there is fine) and returns
/// them. Only live friends count toward [`MAX_FRIENDS`].
pub async fn add(pool: &PgPool, user_id: Uuid, friend_id: Uuid) -> Result<FriendRecord, DbError> {
    if user_id == friend_id {
        return Err(DbError::InvalidInput("you cannot add yourself as a friend"));
    }
    let mut tx = pool.begin().await?;
    // Serializes one account's additions so the cap holds under concurrency.
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    if find(&mut tx, user_id, friend_id).await?.is_none() {
        let live: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM friends f JOIN users u ON u.id = f.friend_id \
              WHERE f.user_id = $1 AND u.disabled_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
        if live >= MAX_FRIENDS {
            return Err(DbError::InvalidInput(
                "you have too many friends: remove some first",
            ));
        }
        sqlx::query("INSERT INTO friends (user_id, friend_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(friend_id)
            .execute(&mut *tx)
            .await?;
    }
    let friend = find(&mut tx, user_id, friend_id)
        .await?
        .ok_or(DbError::InvalidInput("there is no account with this id"))?;
    tx.commit().await?;
    Ok(friend)
}

async fn find(
    conn: &mut sqlx::PgConnection,
    user_id: Uuid,
    friend_id: Uuid,
) -> Result<Option<FriendRecord>, DbError> {
    Ok(sqlx::query_as::<_, FriendRecord>(
        "SELECT u.username, u.display_name, f.created_at AS added_at \
           FROM friends f JOIN users u ON u.id = f.friend_id \
          WHERE f.user_id = $1 AND f.friend_id = $2",
    )
    .bind(user_id)
    .bind(friend_id)
    .fetch_optional(conn)
    .await?)
}

/// Removes `friend_id` from `user_id`'s friends. `false`: was not a friend.
pub async fn remove(pool: &PgPool, user_id: Uuid, friend_id: Uuid) -> Result<bool, DbError> {
    let removed = sqlx::query("DELETE FROM friends WHERE user_id = $1 AND friend_id = $2")
        .bind(user_id)
        .bind(friend_id)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(removed > 0)
}

/// `user_id`'s friends with a live account, by display name then username.
pub async fn list(pool: &PgPool, user_id: Uuid) -> Result<Vec<FriendRecord>, DbError> {
    Ok(sqlx::query_as::<_, FriendRecord>(
        "SELECT u.username, u.display_name, f.created_at AS added_at \
           FROM friends f JOIN users u ON u.id = f.friend_id \
          WHERE f.user_id = $1 AND u.disabled_at IS NULL \
          ORDER BY lower(u.display_name), u.username LIMIT $2",
    )
    .bind(user_id)
    .bind(MAX_FRIENDS)
    .fetch_all(pool)
    .await?)
}

/// Live accounts `user_id` shared with or got a share from, not yet friends,
/// most recent sharing first.
pub async fn suggestions(pool: &PgPool, user_id: Uuid) -> Result<Vec<FriendRecord>, DbError> {
    Ok(sqlx::query_as::<_, FriendRecord>(
        "WITH partners AS ( \
             SELECT CASE WHEN owner_id = $1 THEN recipient_id ELSE owner_id END AS person_id, \
                    max(created_at) AS added_at \
               FROM shares WHERE owner_id = $1 OR recipient_id = $1 \
              GROUP BY 1) \
         SELECT u.username, u.display_name, p.added_at \
           FROM partners p JOIN users u ON u.id = p.person_id \
          WHERE u.disabled_at IS NULL \
            AND NOT EXISTS (SELECT 1 FROM friends f WHERE f.user_id = $1 AND f.friend_id = u.id) \
          ORDER BY p.added_at DESC, u.username LIMIT $2",
    )
    .bind(user_id)
    .bind(MAX_SUGGESTIONS)
    .fetch_all(pool)
    .await?)
}
