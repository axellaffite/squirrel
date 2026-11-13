/// One page of a keyset-paginated listing.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Whether rows exist after the last item.
    pub has_more: bool,
}

impl<T> Page<T> {
    /// Builds a page from a query that fetched `limit + 1` rows.
    pub(crate) fn from_overfetch(mut rows: Vec<T>, limit: u32) -> Self {
        let limit = limit as usize;
        let has_more = rows.len() > limit;
        rows.truncate(limit);
        Self {
            items: rows,
            has_more,
        }
    }
}
