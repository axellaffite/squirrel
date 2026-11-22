//! What the user sees and decides about their duplicates: groups, keeping
//! one photo (the others are hidden), restoring hidden photos. Every query
//! filters on the caller's `owner_id`: another user's group is `NotFound`.

use sqlx::{FromRow, PgConnection, PgPool, Postgres, QueryBuilder};
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

use crate::people::Edit;
use crate::timeline::{AssetSummaryRecord, FROM_ANY, SUMMARY_COLUMNS, TimelineCursor};
use crate::{DbError, Page};

/// Which groups to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupState {
    /// Waiting for the user's decision.
    Open,
    /// The user kept a photo.
    Resolved,
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct GroupRecord {
    pub id: Uuid,
    /// The anchor's capture time (the list's sort key).
    pub taken_at: PrimitiveDateTime,
    pub kept_node_id: Option<Uuid>,
    pub resolved_at: Option<OffsetDateTime>,
}

impl GroupRecord {
    pub fn cursor(&self) -> TimelineCursor {
        TimelineCursor {
            taken_at: self.taken_at,
            node_id: self.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct MemberRecord {
    pub group_id: Uuid,
    #[sqlx(flatten)]
    pub summary: AssetSummaryRecord,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub hidden: bool,
    /// Bits from the group's anchor hash.
    pub distance: i16,
}

/// A group and its photos, oldest capture first.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub group: GroupRecord,
    pub members: Vec<MemberRecord>,
}

const GROUP_COLUMNS: &str = "g.id, g.anchor_taken_at AS taken_at, g.kept_node_id, g.resolved_at";

/// A page of `owner`'s groups in `state`, newest anchor first.
pub async fn list(
    pool: &PgPool,
    owner: Uuid,
    state: GroupState,
    after: Option<&TimelineCursor>,
    limit: u32,
) -> Result<Page<Group>, DbError> {
    let mut query = QueryBuilder::<Postgres>::new(format!(
        "SELECT {GROUP_COLUMNS} FROM duplicate_groups g WHERE g.owner_id = "
    ));
    query
        .push_bind(owner)
        .push(" AND (g.resolved_at IS NULL) = ")
        .push_bind(state == GroupState::Open);
    if let Some(cursor) = after {
        query
            .push(" AND (g.anchor_taken_at, g.id) < (")
            .push_bind(cursor.taken_at)
            .push(", ")
            .push_bind(cursor.node_id)
            .push(")");
    }
    query
        .push(" ORDER BY g.anchor_taken_at DESC, g.id DESC LIMIT ")
        .push_bind(i64::from(limit) + 1);
    let groups: Vec<GroupRecord> = query.build_query_as().fetch_all(pool).await?;
    let page = Page::from_overfetch(groups, limit);
    let ids: Vec<Uuid> = page.items.iter().map(|g| g.id).collect();
    let mut members = members_of(pool, owner, &ids).await?;
    let items = page
        .items
        .into_iter()
        .map(|group| {
            let (mine, rest): (Vec<_>, Vec<_>) =
                members.drain(..).partition(|m| m.group_id == group.id);
            members = rest;
            Group {
                group,
                members: mine,
            }
        })
        .collect();
    Ok(Page {
        items,
        has_more: page.has_more,
    })
}

/// How many groups `owner` has waiting for a decision.
pub async fn open_count(pool: &PgPool, owner: Uuid) -> Result<i64, DbError> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM duplicate_groups WHERE owner_id = $1 AND resolved_at IS NULL",
    )
    .bind(owner)
    .fetch_one(pool)
    .await?)
}

async fn members_of<'e>(
    executor: impl sqlx::PgExecutor<'e>,
    owner: Uuid,
    groups: &[Uuid],
) -> Result<Vec<MemberRecord>, DbError> {
    let sql = format!(
        "SELECT d.group_id, {SUMMARY_COLUMNS}, m.camera_make, m.camera_model, \
                a.hidden_at IS NOT NULL AS hidden, d.distance \
         {FROM_ANY} JOIN duplicate_members d ON d.node_id = a.node_id AND d.owner_id = a.owner_id \
          WHERE a.owner_id = $1 AND d.group_id = ANY($2) \
          ORDER BY a.taken_at, a.node_id"
    );
    Ok(sqlx::query_as(&sql)
        .bind(owner)
        .bind(groups)
        .fetch_all(executor)
        .await?)
}

/// One of `owner`'s groups.
pub async fn find(
    conn: &mut PgConnection,
    owner: Uuid,
    id: Uuid,
) -> Result<Option<Group>, DbError> {
    let sql = format!(
        "SELECT {GROUP_COLUMNS} FROM duplicate_groups g WHERE g.id = $1 AND g.owner_id = $2"
    );
    let Some(group) = sqlx::query_as::<_, GroupRecord>(&sql)
        .bind(id)
        .bind(owner)
        .fetch_optional(&mut *conn)
        .await?
    else {
        return Ok(None);
    };
    let members = members_of(&mut *conn, owner, &[id]).await?;
    Ok(Some(Group { group, members }))
}

/// Locks `owner`'s group `id` for the transaction; its member ids.
async fn lock(
    conn: &mut PgConnection,
    owner: Uuid,
    id: Uuid,
) -> Result<Option<Vec<Uuid>>, DbError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM duplicate_groups WHERE id = $1 AND owner_id = $2 FOR UPDATE",
    )
    .bind(id)
    .bind(owner)
    .fetch_optional(&mut *conn)
    .await?;
    if found.is_none() {
        return Ok(None);
    }
    Ok(Some(
        sqlx::query_scalar("SELECT node_id FROM duplicate_members WHERE group_id = $1")
            .bind(id)
            .fetch_all(conn)
            .await?,
    ))
}

/// Keeps `node` of group `id`: every other member is hidden, `node` is shown
/// (if it was hidden), and the group is resolved.
pub async fn keep(
    pool: &PgPool,
    owner: Uuid,
    id: Uuid,
    node: Uuid,
) -> Result<Edit<Group>, DbError> {
    let mut tx = pool.begin().await?;
    let Some(members) = lock(&mut tx, owner, id).await? else {
        return Ok(Edit::NotFound);
    };
    if !members.contains(&node) {
        return Ok(Edit::NotTheirs);
    }
    sqlx::query(
        "UPDATE assets SET \
                hidden_at = CASE WHEN node_id = $3 THEN NULL ELSE COALESCE(hidden_at, now()) END, \
                hidden_reason = CASE WHEN node_id = $3 THEN NULL ELSE 'duplicate' END \
          WHERE owner_id = $1 AND node_id = ANY($2)",
    )
    .bind(owner)
    .bind(&members)
    .bind(node)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE duplicate_groups SET kept_node_id = $3, resolved_at = now(), updated_at = now() \
          WHERE id = $1 AND owner_id = $2",
    )
    .bind(id)
    .bind(owner)
    .bind(node)
    .execute(&mut *tx)
    .await?;
    let group = find(&mut tx, owner, id).await?.ok_or(DbError::NotFound)?;
    tx.commit().await?;
    Ok(Edit::Done(group))
}

/// Shows hidden photos of group `id` again: `node` only, or all of them.
/// Once none is hidden the group is open again (or deleted, if only one
/// photo is left in it). `Done(None)`: the group is gone.
pub async fn restore(
    pool: &PgPool,
    owner: Uuid,
    id: Uuid,
    node: Option<Uuid>,
) -> Result<Edit<Option<Group>>, DbError> {
    let mut tx = pool.begin().await?;
    let Some(members) = lock(&mut tx, owner, id).await? else {
        return Ok(Edit::NotFound);
    };
    let targets = match node {
        Some(node) if !members.contains(&node) => return Ok(Edit::NotTheirs),
        Some(node) => vec![node],
        None => members.clone(),
    };
    sqlx::query(
        "UPDATE assets SET hidden_at = NULL, hidden_reason = NULL \
          WHERE owner_id = $1 AND node_id = ANY($2)",
    )
    .bind(owner)
    .bind(&targets)
    .execute(&mut *tx)
    .await?;
    let still_hidden: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM assets WHERE owner_id = $1 AND node_id = ANY($2) \
                          AND hidden_at IS NOT NULL)",
    )
    .bind(owner)
    .bind(&members)
    .fetch_one(&mut *tx)
    .await?;
    if !still_hidden {
        if members.len() < 2 {
            sqlx::query("DELETE FROM duplicate_groups WHERE id = $1 AND owner_id = $2")
                .bind(id)
                .bind(owner)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            return Ok(Edit::Done(None));
        }
        sqlx::query(
            "UPDATE duplicate_groups SET resolved_at = NULL, kept_node_id = NULL, \
                    updated_at = now() WHERE id = $1 AND owner_id = $2",
        )
        .bind(id)
        .bind(owner)
        .execute(&mut *tx)
        .await?;
    }
    let group = find(&mut tx, owner, id).await?;
    tx.commit().await?;
    Ok(Edit::Done(group))
}
