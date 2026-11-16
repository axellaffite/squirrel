use axum::extract::FromRequestParts;
use axum::http::HeaderValue;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use squirrel_auth::{hash_token, parse_access};
use squirrel_db::devices::{self, DeviceRecord};
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
                return Err(ApiError::CsrfInvalid);
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

/// Who is calling: a browser session (CSRF-checked) or a signed-in device.
#[derive(Debug)]
pub enum Principal {
    /// A browser session (its CSRF token was checked for non-GET requests).
    Session,
    Device(Box<DeviceRecord>),
}

/// An authenticated, non-disabled user, by session cookie or by device bearer
/// token (`Authorization: Bearer sqa_…`). Data routes take this; account,
/// invitation, admin and device-management routes keep [`CurrentUser`]
/// (session only). When an `Authorization` header is present the cookie is
/// ignored entirely, and no CSRF token is needed: a bearer is never sent
/// ambiently by a browser. Other `Authorization` schemes (a proxy's HTTP
/// authentication) are left to the proxy: the cookie path applies.
pub struct Caller {
    pub user: UserRecord,
    pub principal: Principal,
}

impl Caller {
    pub fn device(&self) -> Option<&DeviceRecord> {
        match &self.principal {
            Principal::Device(device) => Some(device.as_ref()),
            Principal::Session => None,
        }
    }
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match parts.headers.get(AUTHORIZATION) {
            // A Bearer header never falls back to the cookie, even when malformed.
            Some(value) if is_bearer_scheme(value) => bearer(value, state).await,
            // No header, or another scheme (e.g. `Basic` from a reverse proxy's
            // HTTP authentication): the browser session, CSRF-checked.
            _ => {
                let CurrentUser { user, .. } =
                    CurrentUser::from_request_parts(parts, state).await?;
                Ok(Self {
                    user,
                    principal: Principal::Session,
                })
            }
        }
    }
}

/// Whether an `Authorization` header uses the `Bearer` scheme (case-insensitive).
fn is_bearer_scheme(value: &HeaderValue) -> bool {
    value
        .to_str()
        .ok()
        .and_then(|text| text.split_whitespace().next())
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
}

/// The `sqa_…` token of an `Authorization: Bearer` header, if well formed.
fn bearer_token(value: &HeaderValue) -> Option<&str> {
    let (scheme, token) = value.to_str().ok()?.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    parse_access(token.trim())
}

async fn bearer(value: &HeaderValue, state: &AppState) -> Result<Caller, ApiError> {
    let token = bearer_token(value).ok_or(ApiError::Unauthorized)?;
    let device = devices::find_by_access_hash(&state.pool, &hash_token(token))
        .await?
        .ok_or(ApiError::Unauthorized)?;
    let user = users::find_by_id(&state.pool, device.owner_id)
        .await?
        .filter(|user| !user.is_disabled())
        .ok_or(ApiError::Unauthorized)?;
    let now = OffsetDateTime::now_utc();
    if now - device.last_seen_at > state.config.session_touch_interval {
        devices::touch(&state.pool, device.id).await?;
    }
    Ok(Caller {
        user,
        principal: Principal::Device(Box::new(device)),
    })
}

/// A signed-in device (bearer only): the device routes and sync. A browser
/// session is refused with 403 `forbidden`.
pub struct DeviceCaller {
    pub user: UserRecord,
    pub device: DeviceRecord,
}

impl FromRequestParts<AppState> for DeviceCaller {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let caller = Caller::from_request_parts(parts, state).await?;
        match caller.principal {
            Principal::Device(device) => Ok(Self {
                user: caller.user,
                device: *device,
            }),
            Principal::Session => Err(ApiError::Forbidden),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bearer_access_tokens_are_read() {
        let token = squirrel_auth::DeviceToken::access();
        let header = |v: String| HeaderValue::from_str(&v).unwrap();
        assert_eq!(
            bearer_token(&header(format!("Bearer {token}"))),
            Some(token.as_str())
        );
        assert_eq!(
            bearer_token(&header(format!("bearer {token}"))),
            Some(token.as_str())
        );
        assert_eq!(bearer_token(&header(format!("Basic {token}"))), None);
        assert_eq!(bearer_token(&header(token.clone())), None);
        assert!(is_bearer_scheme(&header("BEARER garbage".into())));
        assert!(is_bearer_scheme(&header("Bearer".into())));
        assert!(!is_bearer_scheme(&header("Basic YWxpY2U6eA==".into())));
        assert!(!is_bearer_scheme(&header("Bearerx token".into())));
        let refresh = squirrel_auth::DeviceToken::refresh();
        assert_eq!(bearer_token(&header(format!("Bearer {refresh}"))), None);
    }
}
