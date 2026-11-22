//! `group_duplicates:<owner>`: puts one owner's newly hashed photos into
//! groups of near-identical photos ([`squirrel_db::duplicates`]). Incremental:
//! each photo is looked at once, oldest capture first. A photo joins the group
//! whose anchor it is nearest to (within the distance and the time window);
//! otherwise it founds a group with the ungrouped photos close to it. Groups
//! of one are never made. Nothing is ever hidden here.

use std::time::{Duration, Instant};

use serde::Deserialize;
use squirrel_db::DbError;
use squirrel_db::duplicates::{self, Candidate, MAX_DISTANCE, WINDOW_HOURS};
use squirrel_jobs::{JobContext, JobError};
use squirrel_media::phash::distance;
use uuid::Uuid;

use crate::context::MediaContext;

/// Photos looked at per transaction.
pub const BATCH: i64 = 200;
/// Time one run may take before it hands over to a fresh run of the job.
pub const RUN_BUDGET: Duration = Duration::from_secs(20);

#[derive(Deserialize)]
struct Payload {
    owner_id: Uuid,
}

/// What one run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GroupRun {
    pub checked: u64,
    pub joined: u64,
    pub groups_created: u64,
    /// Out of time with photos left: the job runs again at once.
    pub more: bool,
}

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let Payload { owner_id } = job.payload()?;
    let done = group_owner(&ctx.pool, owner_id, RUN_BUDGET)
        .await
        .map_err(JobError::retry)?;
    tracing::info!(
        owner_id = %owner_id,
        checked = done.checked,
        joined = done.joined,
        created = done.groups_created,
        "duplicates grouped"
    );
    if done.more {
        return Err(JobError::Reschedule(Duration::ZERO));
    }
    Ok(())
}

/// Groups `owner`'s photos not looked at yet, for at most `budget` (and at
/// least one batch).
pub async fn group_owner(
    pool: &sqlx::PgPool,
    owner: Uuid,
    budget: Duration,
) -> Result<GroupRun, DbError> {
    let started = Instant::now();
    let mut total = GroupRun::default();
    loop {
        let mut tx = pool.begin().await?;
        duplicates::lock_owner(&mut tx, owner).await?;
        let page = duplicates::unchecked_page(&mut tx, owner, BATCH).await?;
        if page.is_empty() {
            tx.commit().await?;
            return Ok(total);
        }
        for photo in &page {
            group_one(&mut tx, owner, photo, &mut total).await?;
        }
        let ids: Vec<Uuid> = page.iter().map(|p| p.node_id).collect();
        duplicates::mark_checked(&mut tx, owner, &ids).await?;
        tx.commit().await?;
        total.checked += ids.len() as u64;
        if (ids.len() as i64) < BATCH {
            return Ok(total);
        }
        if started.elapsed() >= budget {
            total.more = true;
            return Ok(total);
        }
    }
}

async fn group_one(
    tx: &mut sqlx::PgConnection,
    owner: Uuid,
    photo: &Candidate,
    total: &mut GroupRun,
) -> Result<(), DbError> {
    // Grouped since it was read (founding member of a group made earlier in
    // this batch), or before it came back unchecked.
    if photo.grouped || is_grouped(tx, photo.node_id).await? {
        return Ok(());
    }
    if let Some((group, d)) = duplicates::nearest_group(tx, owner, photo).await? {
        duplicates::join_group(tx, owner, group, photo.node_id, d).await?;
        total.joined += 1;
        return Ok(());
    }
    let matches = duplicates::ungrouped_matches(tx, owner, photo).await?;
    if let Some(plan) = plan_group(photo, &matches) {
        duplicates::create_group(tx, owner, &plan.anchor, &plan.members).await?;
        total.groups_created += 1;
    }
    Ok(())
}

async fn is_grouped(tx: &mut sqlx::PgConnection, node: Uuid) -> Result<bool, DbError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM duplicate_members WHERE node_id = $1)")
            .bind(node)
            .fetch_one(tx)
            .await?,
    )
}

/// A new group: its anchor and its members with their distance to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPlan {
    pub anchor: Candidate,
    pub members: Vec<(Uuid, i16)>,
}

/// The group `photo` founds with `matches` (ungrouped photos close to it),
/// or `None` for a group of one. The anchor is the oldest of them, and only
/// photos close to the anchor are members (no chaining through `photo`).
/// `photo` is always one: the oldest is `photo` or one of its matches.
pub fn plan_group(photo: &Candidate, matches: &[Candidate]) -> Option<GroupPlan> {
    let all: Vec<Candidate> = std::iter::once(*photo)
        .chain(matches.iter().copied())
        .collect();
    let anchor = all
        .iter()
        .min_by_key(|c| (c.taken_at, c.node_id))
        .copied()?;
    let members = close_to(&anchor, &all);
    (members.len() >= 2).then_some(GroupPlan { anchor, members })
}

/// Those of `photos` within the limits of `anchor` (itself included).
fn close_to(anchor: &Candidate, photos: &[Candidate]) -> Vec<(Uuid, i16)> {
    let window = time::Duration::hours(i64::from(WINDOW_HOURS));
    photos
        .iter()
        .filter_map(|c| {
            let d = distance(anchor.phash(), c.phash());
            let within = (c.taken_at - anchor.taken_at).abs() <= window;
            (i64::from(d) <= MAX_DISTANCE && within).then_some((c.node_id, d as i16))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn photo(n: u128, hours: i64, phash: u64) -> Candidate {
        Candidate::new(
            Uuid::from_u128(n),
            datetime!(2026-06-01 12:00) + time::Duration::hours(hours),
            phash,
        )
    }

    #[test]
    fn a_photo_alone_founds_nothing() {
        assert_eq!(plan_group(&photo(1, 0, 0), &[]), None);
    }

    #[test]
    fn the_oldest_photo_anchors_the_group() {
        let new = photo(1, 5, 0b11);
        let older = photo(2, 0, 0b01);
        let plan = plan_group(&new, &[older]).unwrap();
        assert_eq!(plan.anchor, older);
        assert_eq!(
            plan.members,
            vec![(Uuid::from_u128(1), 1), (Uuid::from_u128(2), 0)]
        );
    }

    #[test]
    fn photos_far_from_the_anchor_are_left_out() {
        // `far` is 6 bits from the new photo but 12 from the oldest one.
        let new = photo(1, 2, 0b11_1111);
        let oldest = photo(2, 0, 0);
        let far = photo(3, 4, 0b1111_1111_1111);
        let plan = plan_group(&new, &[oldest, far]).unwrap();
        assert_eq!(plan.anchor, oldest);
        let ids: Vec<Uuid> = plan.members.iter().map(|m| m.0).collect();
        assert_eq!(ids, vec![Uuid::from_u128(1), Uuid::from_u128(2)]);
    }

    #[test]
    fn no_chaining_through_the_new_photo() {
        // The new photo is close to both others, which are far from each
        // other: the group is the oldest and the new photo only.
        let new = photo(1, 10, 0b111);
        let oldest = photo(2, 0, 0b1_1111_1111);
        let other = photo(3, 11, 0);
        let plan = plan_group(&new, &[oldest, other]).unwrap();
        assert_eq!(plan.anchor, oldest);
        let ids: Vec<Uuid> = plan.members.iter().map(|m| m.0).collect();
        assert_eq!(ids, vec![Uuid::from_u128(1), Uuid::from_u128(2)]);
    }

    #[test]
    fn a_day_apart_is_the_limit() {
        let new = photo(1, 24, 0);
        let older = photo(2, 0, 0);
        assert!(plan_group(&new, &[older]).unwrap().members.len() == 2);
        let anchor = photo(3, 0, 0);
        assert_eq!(close_to(&anchor, &[photo(4, 25, 0)]), vec![]);
    }
}
