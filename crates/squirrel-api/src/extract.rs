use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use squirrel_auth::hash_token;
use squirrel_db::sessions::{self, SessionRecord};
use squirrel_db::users::{self, UserRecord};
use subtle::ConstantTimeEq;
use time::OffsetDateTime;

use crate::error::ApiError;
use crate::session::{CSRF_HEADER, SESSION_COOKIE};
use crate::state::AppState;

/// An authenticated, non-disabled user. Rejects state-changing requests whose
/// `X-CSRF-Token` header does not match the session's token.
pub struct CurrentUser {
    pub user: UserRecord,
    pub session: SessionRecord,
}

impl CurrentUser {
    pub fn require_admin(&self) -> Result<(), ApiError> {
        if self.user.role().is_admin() {
            Ok(())
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .ok_or(ApiError::Unauthorized)?
            .value()
            .to_owned();
        let session = sessions::find_active(&state.pool, &hash_token(&token))
            .await?
            .ok_or(ApiError::Unauthorized)?;
        let user = users::find_by_id(&state.pool, session.user_id)
            .await?
            .filter(|user| !user.is_disabled())
            .ok_or(ApiError::Unauthorized)?;

        if !parts.method.is_safe() {
            let provided = parts
                .headers
                .get(CSRF_HEADER)
                .map(|v| v.as_bytes())
                .unwrap_or_default();
            if !bool::from(provided.ct_eq(session.csrf_token.as_bytes())) {
                return Err(ApiError::Forbidden);
            }
        }

        let now = OffsetDateTime::now_utc();
        if now - session.last_seen_at > state.config.session_touch_interval {
            // `touch` returns false if the session expired or was deleted meanwhile.
            if !sessions::touch(&state.pool, session.id, now + state.config.session_ttl).await? {
                return Err(ApiError::Unauthorized);
            }
        }
        Ok(Self { user, session })
    }
}
