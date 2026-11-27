//! Who a download or a preview render is counted against. Visitors of public
//! links draw on capacity of their own, so they never hold up the users.

use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Holder {
    /// A signed-in user (or one of their devices).
    User(Uuid),
    /// Whoever opened this public link.
    Link(Uuid),
}

impl Holder {
    /// The key per-holder limits and zip tickets are counted under.
    pub fn id(self) -> Uuid {
        match self {
            Self::User(id) | Self::Link(id) => id,
        }
    }

    pub fn is_visitor(self) -> bool {
        matches!(self, Self::Link(_))
    }
}
