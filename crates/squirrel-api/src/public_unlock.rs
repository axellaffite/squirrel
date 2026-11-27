//! The cookie that proves a browser gave a public link's password.
//!
//! Its value is `<issued unix seconds>.<base64url HMAC-SHA256>`, keyed by the
//! link's stored password hash (secret to visitors) over the link id and the
//! issue time. Nothing is stored server side: the cookie lasts its TTL, and
//! revoking the link ends it. It is scoped to `/api/public/<token>`, so the
//! links a browser has opened never share it.

use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use squirrel_db::public_links::PublicLinkRecord;
use time::{Duration, OffsetDateTime};

pub const LINK_COOKIE: &str = "squirrel_link";
const CONTEXT: &[u8] = b"squirrel public link unlock v1\0";
/// Clock skew tolerated on the issue time.
const SKEW_SECS: i64 = 60;

type HmacSha256 = Hmac<Sha256>;

fn mac(password_hash: &str, link: &PublicLinkRecord, issued: i64) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(password_hash.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(CONTEXT);
    mac.update(link.id.as_bytes());
    mac.update(&issued.to_be_bytes());
    mac
}

/// The path the cookie of `token` is sent to.
pub fn cookie_path(token: &str) -> String {
    format!("/api/public/{token}")
}

/// A cookie unlocking `link` (which has a password) for `ttl` from `now`.
pub fn cookie(
    token: &str,
    link: &PublicLinkRecord,
    now: OffsetDateTime,
    ttl: Duration,
    secure: bool,
) -> Option<Cookie<'static>> {
    let password_hash = link.password_hash.as_deref()?;
    let issued = now.unix_timestamp();
    let tag = mac(password_hash, link, issued).finalize().into_bytes();
    let value = format!("{issued}.{}", URL_SAFE_NO_PAD.encode(tag));
    Some(
        Cookie::build((LINK_COOKIE, value))
            .path(cookie_path(token))
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(secure)
            .max_age(ttl)
            .build(),
    )
}

/// Whether the visitor may see `link`: it has no password, or `jar` holds a
/// valid cookie for it issued less than `ttl` ago.
pub fn is_unlocked(
    jar: &CookieJar,
    link: &PublicLinkRecord,
    now: OffsetDateTime,
    ttl: Duration,
) -> bool {
    let Some(password_hash) = link.password_hash.as_deref() else {
        return true;
    };
    let Some(value) = jar.get(LINK_COOKIE).map(Cookie::value) else {
        return false;
    };
    verify(value, password_hash, link, now, ttl)
}

fn verify(
    value: &str,
    password_hash: &str,
    link: &PublicLinkRecord,
    now: OffsetDateTime,
    ttl: Duration,
) -> bool {
    let Some((issued, tag)) = value.split_once('.') else {
        return false;
    };
    let well_formed =
        !issued.is_empty() && issued.len() <= 12 && issued.bytes().all(|b| b.is_ascii_digit());
    let Some(issued) = well_formed.then(|| issued.parse::<i64>().ok()).flatten() else {
        return false;
    };
    let now = now.unix_timestamp();
    if issued > now + SKEW_SECS || now - issued >= ttl.whole_seconds() {
        return false;
    }
    let Ok(tag) = URL_SAFE_NO_PAD.decode(tag) else {
        return false;
    };
    mac(password_hash, link, issued).verify_slice(&tag).is_ok()
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use axum::http::header::COOKIE;
    use uuid::Uuid;

    use super::*;

    fn link(password_hash: Option<&str>) -> PublicLinkRecord {
        PublicLinkRecord {
            id: Uuid::now_v7(),
            owner_id: Uuid::now_v7(),
            node_id: Some(Uuid::now_v7()),
            album_id: None,
            password_hash: password_hash.map(str::to_owned),
            expires_at: None,
            created_at: OffsetDateTime::now_utc(),
        }
    }

    fn jar_with(value: &str) -> CookieJar {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, format!("{LINK_COOKIE}={value}").parse().unwrap());
        CookieJar::from_headers(&headers)
    }

    const TTL: Duration = Duration::days(7);

    #[test]
    fn a_link_without_password_is_always_open() {
        let open = link(None);
        let now = OffsetDateTime::now_utc();
        assert!(is_unlocked(&CookieJar::new(), &open, now, TTL));
        assert!(cookie("t", &open, now, TTL, true).is_none());
    }

    #[test]
    fn the_issued_cookie_unlocks_its_link_until_it_expires() {
        let locked = link(Some("$argon2id$hash-a"));
        let now = OffsetDateTime::now_utc();
        let cookie = cookie("tok", &locked, now, TTL, true).unwrap();
        assert_eq!(cookie.path(), Some("/api/public/tok"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(cookie.secure(), Some(true));
        let jar = jar_with(cookie.value());

        assert!(!is_unlocked(&CookieJar::new(), &locked, now, TTL));
        assert!(is_unlocked(&jar, &locked, now, TTL));
        assert!(is_unlocked(&jar, &locked, now + Duration::days(6), TTL));
        assert!(!is_unlocked(&jar, &locked, now + TTL, TTL), "expired");
        assert!(
            !is_unlocked(&jar, &locked, now - Duration::minutes(5), TTL),
            "issued in the future"
        );
    }

    #[test]
    fn a_cookie_does_not_open_another_link_or_a_new_password() {
        let locked = link(Some("$argon2id$hash-a"));
        let now = OffsetDateTime::now_utc();
        let value = cookie("tok", &locked, now, TTL, true)
            .unwrap()
            .value()
            .to_owned();
        let jar = jar_with(&value);

        let other = link(Some("$argon2id$hash-a"));
        assert!(!is_unlocked(&jar, &other, now, TTL));
        let rehashed = PublicLinkRecord {
            password_hash: Some("$argon2id$hash-b".into()),
            ..locked.clone()
        };
        assert!(!is_unlocked(&jar, &rehashed, now, TTL));
    }

    #[test]
    fn tampered_or_malformed_values_are_refused() {
        let locked = link(Some("$argon2id$hash-a"));
        let now = OffsetDateTime::now_utc();
        let value = cookie("tok", &locked, now, TTL, true)
            .unwrap()
            .value()
            .to_owned();
        let (issued, tag) = value.split_once('.').unwrap();
        let later = (issued.parse::<i64>().unwrap() + 1).to_string();
        for bad in [
            format!("{later}.{tag}"),
            format!("{issued}.{}", &tag[1..]),
            format!("{issued}."),
            format!(".{tag}"),
            format!("-1.{tag}"),
            tag.to_owned(),
            String::new(),
            format!("{}.{tag}", "9".repeat(40)),
        ] {
            assert!(!is_unlocked(&jar_with(&bad), &locked, now, TTL), "{bad}");
        }
    }
}
