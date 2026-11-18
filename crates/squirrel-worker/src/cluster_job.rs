//! `cluster_faces:<owner>`: incremental clustering of one owner's unassigned
//! automatic faces into people ([`squirrel_ml::cluster`]). One transaction per
//! batch, each holding the owner's row lock shared and re-checking that faces
//! are on, so turning faces off never interleaves with a batch. Faces pinned
//! by the user are never read as candidates nor written.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sqlx::PgConnection;
use squirrel_db::clustering::{self, NeighbourRow, Unclustered};
use squirrel_jobs::{JobContext, JobError};
use squirrel_ml::cluster::{self, ClusterParams, Decision, Neighbour};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::context::MediaContext;
use crate::jobs;

/// Faces decided per transaction.
pub const BATCH: i64 = 200;

#[derive(Deserialize)]
struct Payload {
    owner_id: Uuid,
}

/// What one run did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClusterRun {
    pub faces_assigned: u64,
    pub people_created: u64,
    pub people_deleted: u64,
    pub left: Left,
}

/// Why a run ended with work left.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    #[default]
    Nothing,
    /// Out of time: the run continues at once (the job is rescheduled).
    Budget,
    /// Faces stored while it ran: their own `cluster_faces` request was
    /// dropped as a duplicate of this running job, so it runs again later.
    NewFaces,
}

/// Longest a `faces` transaction may run before it commits its faces.
const STORE_MARGIN: Duration = Duration::from_secs(60);

/// Time one run may take before it hands over to a fresh run of the job (its
/// timeout is longer): short, as it holds the worker's ML slot and the next
/// run goes on where it stopped.
pub const RUN_BUDGET: Duration = Duration::from_secs(20);

pub async fn run(ctx: &MediaContext, job: JobContext) -> Result<(), JobError> {
    let Payload { owner_id } = job.payload()?;
    let done = cluster_owner(ctx, owner_id, &cluster::SFACE, RUN_BUDGET)
        .await
        .map_err(JobError::retry)?;
    tracing::info!(
        owner_id = %owner_id,
        faces = done.faces_assigned,
        created = done.people_created,
        deleted = done.people_deleted,
        left = ?done.left,
        "faces clustered"
    );
    match done.left {
        Left::Nothing => Ok(()),
        Left::Budget => Err(JobError::Reschedule(Duration::ZERO)),
        Left::NewFaces => Err(JobError::Reschedule(jobs::cluster_delay())),
    }
}

/// Clusters `owner`'s unassigned automatic faces, oldest first, for at most
/// `budget` (and at least one batch). A face left waiting is tried again only
/// once a face newer than this run's watermark exists, so the next run goes
/// on where this one stopped and waiting faces never starve new ones.
pub async fn cluster_owner(
    ctx: &MediaContext,
    owner: Uuid,
    params: &ClusterParams,
    budget: Duration,
) -> Result<ClusterRun, squirrel_db::DbError> {
    let started = Instant::now();
    // Rows stored by transactions that began before the run carry an older
    // `created_at`: the margin covers them.
    let since = OffsetDateTime::now_utc() - STORE_MARGIN;
    let mut seen = HashSet::new();
    let mut total = ClusterRun::default();
    let mut after = None;
    let mut newest = None;
    loop {
        let mut tx = ctx.pool.begin().await?;
        if !clustering::lock_enabled(&mut tx, owner).await? {
            return Ok(total); // turned off: its data is gone
        }
        // At least one batch per run, so a run always makes progress.
        if after.is_some() && started.elapsed() >= budget {
            total.left = Left::Budget;
            return Ok(total);
        }
        let watermark = match newest {
            Some(at) => at,
            None => {
                let Some(at) = clustering::newest_face(&mut tx, owner).await? else {
                    total.people_deleted = clustering::delete_empty_people(&mut tx, owner).await?;
                    tx.commit().await?;
                    return Ok(total);
                };
                *newest.insert(at)
            }
        };
        let page = clustering::unclustered_page(&mut tx, owner, watermark, after, BATCH).await?;
        let Some(last) = page.last() else {
            total.people_deleted = clustering::delete_empty_people(&mut tx, owner).await?;
            let recent = clustering::unclustered_since(&mut tx, owner, since).await?;
            if recent.iter().any(|id| !seen.contains(id)) {
                total.left = Left::NewFaces;
            }
            tx.commit().await?;
            return Ok(total);
        };
        after = Some((last.created_at, last.id));
        seen.extend(page.iter().map(|f| f.id));
        let batch = cluster_batch(&mut tx, owner, &page, params, watermark).await?;
        tx.commit().await?;
        total.faces_assigned += batch.faces_assigned;
        total.people_created += batch.people_created;
    }
}

/// Decides `page` in order; assignments made earlier in the batch count for
/// later faces.
async fn cluster_batch(
    tx: &mut PgConnection,
    owner: Uuid,
    page: &[Unclustered],
    params: &ClusterParams,
    watermark: OffsetDateTime,
) -> Result<ClusterRun, squirrel_db::DbError> {
    let ids: Vec<Uuid> = page.iter().map(|f| f.id).collect();
    let rows = clustering::neighbours(tx, owner, &ids, cluster::NEIGHBOURS as i64).await?;
    let lists = by_face(rows);
    let mut decided: HashMap<Uuid, Uuid> = HashMap::new();
    let mut waiting = Vec::new();
    let mut run = ClusterRun::default();
    for face in page {
        if decided.contains_key(&face.id) {
            continue;
        }
        let neighbours: Vec<Neighbour<Uuid, Uuid>> = lists
            .get(&face.id)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|n| Neighbour {
                face: n.neighbour,
                person: n.person_id.or_else(|| decided.get(&n.neighbour).copied()),
                distance: n.distance,
            })
            .collect();
        let (person, members) = match cluster::decide(face.id, face.score, &neighbours, params) {
            Decision::Wait => {
                waiting.push(face.id);
                continue;
            }
            Decision::Join(person) => (person, vec![face.id]),
            Decision::Found(members) => {
                run.people_created += 1;
                (clustering::create_person(tx, owner).await?, members)
            }
        };
        run.faces_assigned += clustering::assign(tx, owner, &members, person).await?;
        decided.extend(members.into_iter().map(|m| (m, person)));
    }
    clustering::mark_waiting(tx, owner, &waiting, watermark).await?;
    Ok(run)
}

fn by_face(rows: Vec<NeighbourRow>) -> HashMap<Uuid, Vec<NeighbourRow>> {
    let mut lists: HashMap<Uuid, Vec<NeighbourRow>> = HashMap::new();
    for row in rows {
        lists.entry(row.face).or_default().push(row);
    }
    lists
}
