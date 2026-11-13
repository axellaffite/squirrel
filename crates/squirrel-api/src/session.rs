use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use squirrel_auth::{generate_token, hash_token};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::users::UserRecord;
use time::OffsetDateTime;

use crate::dto::MeResponse;
use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "squirrel_session";
pub const CSRF_HEADER: &str = "x-csrf-token";
const USER_AGENT_MAX_LEN: usize = 256;

/// Creates a session row and returns the cookie to set plus the `/me` payload.
/// Any session the browser already carried is deleted first, so a new login
/// never leaves a previous (possibly planted) session alive.
pub async fn start(
    state: &AppState,
    user: &UserRecord,
    headers: &HeaderMap,
    jar: &CookieJar,
) -> Result<(Cookie<'static>, MeResponse), ApiError> {
    if let Some(previous) = jar.get(SESSION_COOKIE) {
        sessions::delete_by_token_hash(&state.pool, &hash_token(previous.value())).await?;
    }
    let token = generate_token();
    let csrf_token = generate_token();
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(USER_AGENT_MAX_LEN).collect::<String>());
    let token_hash = hash_token(&token);
    let now = OffsetDateTime::now_utc();
    sessions::create(
        &state.pool,
        &NewSession {
            user_id: user.id,
            token_hash: &token_hash,
            csrf_token: &csrf_token,
            user_agent: user_agent.as_deref(),
            expires_at: now + state.config.session_ttl,
            absolute_expires_at: now + state.config.session_absolute_ttl,
        },
    )
    .await?;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.cookie_secure)
        // The server-side expiry is authoritative; the cookie just outlives it.
        .max_age(state.config.session_absolute_ttl)
        .build();
    Ok((cookie, MeResponse::new(user, csrf_token)))
}

pub fn removal() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, "")).path("/").build()
}
