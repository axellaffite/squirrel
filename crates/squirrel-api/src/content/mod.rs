//! Pure helpers for serving stored files safely.

mod disposition;
mod policy;
mod range;

pub use disposition::content_disposition;
pub use policy::{ContentPolicy, etag_matches};
pub use range::{RangeRequest, resolve as resolve_range};
