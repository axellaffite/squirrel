//! `faces`: one inference per blob (YuNet + SFace on its preview), then rows
//! **per owner**: for each owner of a live copy who has faces on and no scan
//! yet, their faces and a scan row (also when none was found), and a
//! clustering run queued for them. Nothing is stored for owners with faces off.

use squirrel_core::{ContentHash, MediaKind};
use squirrel_db::faces::{self, NewFace};
use squirrel_db::media::{self, MediaRecord, MediaState, Step};
use squirrel_jobs::{JobContext, JobError};
use squirrel_ml::FACE_MODEL_ID;
use squirrel_ml::clip::{check_unit_vector, normalize};
use squirrel_ml::faces::{FACE_DIM, FaceFound};
use squirrel_ml::protocol::{MlBody, MlOp, MlOutput};
use squirrel_storage::Variant;

use crate::clip_job::WAIT_FOR_PREVIEW;
use crate::context::{MediaContext, MlContext};
use crate::jobs::{self, ShaPayload};
use crate::outcome;

pub async fn run(ctx: &MediaContext, ml: &MlContext, job: JobContext) -> Result<(), JobError> {
    let ShaPayload { sha256 } = job.payload()?;
    let hash = ContentHash::parse(&sha256).map_err(JobError::permanent)?;
    let Some(media) = media::find(&ctx.pool, &sha256)
        .await
        .map_err(JobError::retry)?
    else {
        return Ok(());
    };
    if media.kind() != MediaKind::Image {
        return mark(ctx, &sha256, MediaState::Unsupported, "not an image").await;
    }
    match media.thumbs_state() {
        MediaState::Pending => return Err(JobError::Reschedule(WAIT_FOR_PREVIEW)),
        MediaState::Failed | MediaState::Unsupported => {
            return mark(
                ctx,
                &sha256,
                MediaState::Unsupported,
                "no preview to analyse",
            )
            .await;
        }
        MediaState::Ready => {}
    }
    let owners = faces::owners_to_scan(&ctx.pool, &sha256, FACE_MODEL_ID)
        .await
        .map_err(JobError::retry)?;
    if owners.is_empty() {
        return Ok(());
    }
    let Some(found) = detect(ctx, ml, &job, &media, &hash).await? else {
        return Ok(());
    };
    let rows: Vec<NewFace> = found.into_iter().map(new_face).collect();
    for owner in owners {
        // The scan and its clustering run commit together.
        let mut tx = ctx.pool.begin().await.map_err(JobError::retry)?;
        let stored = faces::store_scan_in(&mut tx, owner, &sha256, FACE_MODEL_ID, &rows)
            .await
            .map_err(JobError::retry)?;
        if stored && !rows.is_empty() {
            squirrel_jobs::enqueue(&mut tx, &jobs::cluster_faces(owner))
                .await
                .map_err(JobError::retry)?;
        }
        tx.commit().await.map_err(JobError::retry)?;
    }
    media::set_state(&ctx.pool, &sha256, Step::Faces, MediaState::Ready, None)
        .await
        .map_err(JobError::retry)
}

/// Runs the face models on the preview. `None`: the step ended without a
/// result (state recorded).
async fn detect(
    ctx: &MediaContext,
    ml: &MlContext,
    job: &JobContext,
    media: &MediaRecord,
    hash: &ContentHash,
) -> Result<Option<Vec<FaceFound>>, JobError> {
    let sha256 = hash.as_str();
    let input = ctx
        .storage
        .derived()
        .variant_path(hash, Variant::Preview, media.thumbs_version);
    let reason = match ml.client.request(MlOp::Faces { input }).await {
        Ok(MlBody::Ok(MlOutput::Faces { faces })) => match faces.iter().find_map(invalid) {
            Some(why) => format!("the ML child answered an invalid face: {why}"),
            None => return Ok(Some(faces)),
        },
        Ok(MlBody::Unsupported { reason }) => {
            mark(ctx, sha256, MediaState::Unsupported, &reason).await?;
            return Ok(None);
        }
        Ok(MlBody::Ok(other)) => format!("unexpected answer {other:?}"),
        Ok(MlBody::Failed { reason }) => reason,
        Err(err) if err.retry_after().is_some() => {
            return Err(JobError::Reschedule(err.retry_after().unwrap_or_default()));
        }
        Err(err) => err.to_string(),
    };
    outcome::give_up_or_retry(ctx, job, sha256, Step::Faces, reason).await?;
    Ok(None)
}

/// Why `face` cannot be stored: a 128-d unit embedding, a box and landmarks
/// inside the image (0..1), a score in 0..1.
fn invalid(face: &FaceFound) -> Option<String> {
    if let Err(why) = check_unit_vector(&face.embedding, FACE_DIM) {
        return Some(why);
    }
    let unit = |v: &f32| v.is_finite() && (0.0..=1.0).contains(v);
    let [x, y, w, h] = face.bbox;
    if !face.bbox.iter().all(unit) || x + w > 1.0 + 1e-4 || y + h > 1.0 + 1e-4 {
        return Some(format!("box {:?}", face.bbox));
    }
    if !face.landmarks.iter().all(|v| v.is_finite()) || !unit(&face.score) {
        return Some("landmarks or score".into());
    }
    None
}

fn new_face(face: FaceFound) -> NewFace {
    NewFace {
        bbox: face.bbox,
        landmarks: face.landmarks,
        score: face.score,
        embedding: normalize(&face.embedding),
    }
}

async fn mark(
    ctx: &MediaContext,
    sha256: &str,
    state: MediaState,
    reason: &str,
) -> Result<(), JobError> {
    outcome::mark(ctx, sha256, Step::Faces, state, reason).await
}
