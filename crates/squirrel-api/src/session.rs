use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use axum_extra::extract::cookie::{Cookie, SameSite};
use squirrel_auth::{generate_token, hash_token};
use squirrel_db::sessions::{self, NewSession};
use squirrel_db::users::UserRecord;
use time::{Duration, OffsetDateTime};

use crate::dto::MeResponse;
use crate::error::ApiError;
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "squirrel_session";
pub const CSRF_HEADER: &str = "x-csrf-token";
/// Browser-side lifetime; the server-side `expires_at` is authoritative.
const COOKIE_MAX_AGE: Duration = Duration::days(365);
const USER_AGENT_MAX_LEN: usize = 256;

/// Creates a session row and returns the cookie to set plus the `/me` payload.
pub async fn start(
    state: &AppState,
    user: &UserRecord,
    headers: &HeaderMap,
) -> Result<(Cookie<'static>, MeResponse), ApiError> {
    let token = generate_token();
    let csrf_token = generate_token();
    let user_agent = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|ua| ua.chars().take(USER_AGENT_MAX_LEN).collect::<String>());
    let token_hash = hash_token(&token);
    sessions::create(
        &state.pool,
        &NewSession {
            user_id: user.id,
            token_hash: &token_hash,
            csrf_token: &csrf_token,
            user_agent: user_agent.as_deref(),
            expires_at: OffsetDateTime::now_utc() + state.config.session_ttl,
        },
    )
    .await?;
    let cookie = Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(state.config.cookie_secure)
        .max_age(COOKIE_MAX_AGE)
        .build();
    Ok((cookie, MeResponse::new(user, csrf_token)))
}

pub fn removal() -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, "")).path("/").build()
}
