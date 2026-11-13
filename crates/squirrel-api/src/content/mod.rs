//! Pure helpers for serving stored files safely.

mod disposition;
mod etag;
mod policy;
mod range;

pub use disposition::content_disposition;
pub use etag::{entity_tag, etag_matches};
pub use policy::ContentPolicy;
pub use range::{RangeRequest, resolve as resolve_range};
