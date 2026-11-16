use std::collections::HashSet;

use sqlx::PgConnection;
use squirrel_core::{ContentHash, NodeName};
use time::PrimitiveDateTime;
use uuid::Uuid;

use super::{NODE_COLUMNS, NodeRecord};
use crate::DbError;

/// Give up looking for a free "name (n).ext" after this many tries.
const MAX_NAME_VARIANTS: u32 = 1000;

/// The capture time an uploading client reported (a phone's photo date):
/// wall clock and, when known, its UTC offset in minutes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientTaken {
    pub local: PrimitiveDateTime,
    pub offset_min: Option<i16>,
}

#[derive(Debug, Clone)]
pub struct NewFile<'a> {
    pub owner_id: Uuid,
    pub parent_id: Uuid,
    pub name: &'a NodeName,
    pub blob: &'a ContentHash,
    pub size: i64,
    pub mime: &'a str,
    pub client_taken: Option<ClientTaken>,
}

/// The first free variant of `name` among the live children of `parent_id`
/// (`name`, `name (1)`, …). One query fetches the taken names sharing the
/// variants' common prefix. The caller holds the owner lock, so the answer
/// stays true until it writes.
pub async fn free_name(
    conn: &mut PgConnection,
    owner_id: Uuid,
    parent_id: Uuid,
    name: &NodeName,
) -> Result<NodeName, DbError> {
    let variants: Vec<NodeName> = (0..MAX_NAME_VARIANTS).map(|n| name.variant(n)).collect();
    let prefix = common_prefix(variants.iter().map(NodeName::as_str));
    let taken: HashSet<String> = sqlx::query_scalar(
        "SELECT name FROM nodes \
         WHERE owner_id = $1 AND parent_id = $2 AND deleted_at IS NULL AND starts_with(name, $3)",
    )
    .bind(owner_id)
    .bind(parent_id)
    .bind(prefix)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    variants
        .into_iter()
        .find(|candidate| !taken.contains(candidate.as_str()))
        .ok_or(DbError::NameConflict)
}

/// Longest prefix (on a character boundary) shared by every name.
fn common_prefix<'a>(mut names: impl Iterator<Item = &'a str>) -> &'a str {
    let Some(first) = names.next() else {
        return "";
    };
    names.fold(first, |prefix, name| {
        let len = prefix
            .char_indices()
            .zip(name.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((i, c), _)| i + c.len_utf8());
        &prefix[..len]
    })
}

/// Inserts a file node under a free variant of its name. Preconditions: the
/// caller holds the owner lock, checked that the parent is a live folder of the
/// owner, and the blob row exists.
pub async fn insert_file(
    conn: &mut PgConnection,
    file: &NewFile<'_>,
) -> Result<NodeRecord, DbError> {
    let name = free_name(conn, file.owner_id, file.parent_id, file.name).await?;
    let sql = format!(
        "INSERT INTO nodes (id, owner_id, parent_id, name, kind, blob_sha256, size, mime, \
                            client_taken_at, client_taken_offset_min) \
         VALUES ($1, $2, $3, $4, 'file', $5, $6, $7, $8, $9) RETURNING {NODE_COLUMNS}"
    );
    Ok(sqlx::query_as::<_, NodeRecord>(&sql)
        .bind(Uuid::now_v7())
        .bind(file.owner_id)
        .bind(file.parent_id)
        .bind(name.as_str())
        .bind(file.blob.as_str())
        .bind(file.size)
        .bind(file.mime)
        .bind(file.client_taken.map(|taken| taken.local))
        .bind(file.client_taken.and_then(|taken| taken.offset_min))
        .fetch_one(conn)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::common_prefix;

    #[test]
    fn common_prefix_stops_at_the_first_difference() {
        assert_eq!(
            common_prefix(["report.pdf", "report (1).pdf"].into_iter()),
            "report"
        );
        assert_eq!(common_prefix(["\u{e9}a", "\u{e9}b"].into_iter()), "\u{e9}");
        assert_eq!(common_prefix(["a", "b"].into_iter()), "");
        assert_eq!(common_prefix(std::iter::empty()), "");
    }
}
