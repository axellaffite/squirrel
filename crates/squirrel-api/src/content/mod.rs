//! Pure helpers for serving stored files safely.

mod disposition;
mod etag;
mod policy;
mod range;

pub use disposition::content_disposition;
pub use etag::{entity_tag, etag_matches};
pub use policy::{ContentPolicy, SANDBOX_CSP};
pub use range::{RangeRequest, resolve as resolve_range};

use axum::http::{HeaderMap, HeaderName, HeaderValue};
use squirrel_core::ContentHash;
use squirrel_db::nodes::NodeRecord;

use crate::error::ApiError;

pub const CROSS_ORIGIN_RESOURCE_POLICY: HeaderName =
    HeaderName::from_static("cross-origin-resource-policy");

/// The blob a file node points at.
pub fn blob_hash(node: &NodeRecord) -> Result<ContentHash, ApiError> {
    node.blob_sha256
        .as_deref()
        .and_then(|sha| ContentHash::parse(sha).ok())
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "file node {} has no valid blob hash",
                node.id
            ))
        })
}

pub fn header_str(headers: &HeaderMap, name: HeaderName) -> Option<&str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

pub fn to_header_map(pairs: Vec<(HeaderName, String)>) -> Result<HeaderMap, ApiError> {
    pairs
        .into_iter()
        .map(|(name, value)| HeaderValue::try_from(value).map(|value| (name, value)))
        .collect::<Result<HeaderMap, _>>()
        .map_err(|err| ApiError::Internal(err.into()))
}
