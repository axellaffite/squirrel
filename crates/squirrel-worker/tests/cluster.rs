mod common;

use std::time::{Duration, Instant};

use common::{Fixture, admin, invited, job, root};
use serde_json::json;
use sqlx::PgPool;
use squirrel_core::MediaKind;
use squirrel_db::{media, ml_settings};
use squirrel_ml::cluster::SFACE;
use squirrel_worker::cluster_job;
use uuid::Uuid;

/// A photo of `owner` (blob, node and media row) that faces can point at.
async fn photo(fx: &Fixture, owner: Uuid, name: &str) -> String {
    let parent = root(fx.pool(), owner, "Photos").await;
    let bytes = format!("photo {name} of {owner}");
    let node = fx.store_file(owner, parent, name, bytes.as_bytes()).await;
    let sha = node.blob_sha256.unwrap();
    let mut conn = fx.pool().acquire().await.unwrap();
    media::ensure(&mut conn, &sha, MediaKind::Image)
        .await
        .unwrap();
    sha
}

/// Inserts faces of `owner` on `sha` with these embeddings (score 0.9),
/// oldest first; returns their ids.
async fn faces(pool: &PgPool, owner: Uuid, sha: &str, embeddings: &[Vec<f32>]) -> Vec<Uuid> {
    let mut ids = Vec::new();
    for (i, embedding) in embeddings.iter().enumerate() {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, \
                                landmarks, score, embedding, created_at) \
             VALUES ($1, $2, $3, 'yunet-sface-2021dec', 0.1, 0.1, 0.2, 0.2, \
                     array_fill(0.2::real, ARRAY[10]), 0.9, $4::text::halfvec, \
                     now() - make_interval(secs => $5))",
        )
        .bind(id)
        .bind(owner)
        .bind(sha)
        .bind(format!("{embedding:?}"))
        .bind((embeddings.len() - i) as f64)
        .execute(pool)
        .await
        .unwrap();
        ids.push(id);
    }
    ids
}

/// A unit vector over `values` (index, weight) in 128 dimensions.
fn unit(values: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0f32; 128];
    for (i, x) in values {
        v[*i] = *x;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / norm).collect()
}

/// Three tight groups of five, then four lone faces.
fn groups_and_noise() -> Vec<Vec<f32>> {
    let mut all = Vec::new();
    for group in 0..3 {
        for i in 0..5 {
            all.push(unit(&[(group, 1.0), (10 + group * 5 + i, 0.25)]));
        }
    }
    for i in 0..4 {
        all.push(unit(&[(60 + i, 1.0)]));
    }
    all
}

async fn person_of(pool: &PgPool, face: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT person_id FROM faces WHERE id = $1")
        .bind(face)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn people(pool: &PgPool, owner: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM people WHERE owner_id = $1")
        .bind(owner)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn cluster(fx: &Fixture, owner: Uuid) -> cluster_job::ClusterRun {
    cluster_job::cluster_owner(&fx.ctx, owner, &SFACE, cluster_job::RUN_BUDGET)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn groups_become_people_and_noise_stays_unassigned(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let ids = faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;

    // Through the job, as the queue runs it.
    cluster_job::run(
        &fx.ctx,
        job("cluster_faces", json!({ "owner_id": alice.id })),
    )
    .await
    .unwrap();
    assert_eq!(people(fx.pool(), alice.id).await, 3);
    for group in ids[..15].chunks(5) {
        let first = person_of(fx.pool(), group[0]).await;
        assert!(first.is_some());
        for face in group {
            assert_eq!(person_of(fx.pool(), *face).await, first);
        }
    }
    for lone in &ids[15..] {
        assert_eq!(person_of(fx.pool(), *lone).await, None);
    }

    // A new face near the first group joins it.
    let new = faces(fx.pool(), alice.id, &sha, &[unit(&[(0, 1.0), (40, 0.2)])]).await;
    let run = cluster(&fx, alice.id).await;
    assert_eq!((run.faces_assigned, run.people_created), (1, 0));
    assert_eq!(
        person_of(fx.pool(), new[0]).await,
        person_of(fx.pool(), ids[0]).await
    );
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn clustering_reads_only_the_owners_faces(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let bob = invited(fx.pool(), &alice, "bob").await;
    let mine = photo(&fx, alice.id, "a.jpg").await;
    let theirs = photo(&fx, bob.id, "b.jpg").await;
    let group: Vec<Vec<f32>> = (0..5).map(|i| unit(&[(0, 1.0), (10 + i, 0.25)])).collect();
    faces(fx.pool(), alice.id, &mine, &group).await;
    cluster(&fx, alice.id).await;
    assert_eq!(people(fx.pool(), alice.id).await, 1);

    // Bob's identical faces: Alice's person is invisible to him.
    let bobs = faces(fx.pool(), bob.id, &theirs, &group[..2]).await;
    let run = cluster(&fx, bob.id).await;
    assert_eq!(
        run.faces_assigned, 0,
        "two faces are not enough to found a person"
    );
    assert_eq!(person_of(fx.pool(), bobs[0]).await, None);
    assert_eq!(people(fx.pool(), bob.id).await, 0);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn pinned_faces_survive_and_empty_unnamed_people_go(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let ids = faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;
    cluster(&fx, alice.id).await;
    let first = person_of(fx.pool(), ids[0]).await.unwrap();
    let second = person_of(fx.pool(), ids[5]).await.unwrap();

    // The user moves a face of the first group to the second, says another is
    // nobody, and names a third person whose faces then all go.
    let named = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id, name) VALUES ($1, $2, 'Named')")
        .bind(named)
        .bind(alice.id)
        .execute(fx.pool())
        .await
        .unwrap();
    for (face, person) in [(ids[1], Some(second)), (ids[2], None)] {
        sqlx::query("UPDATE faces SET person_id = $2, assigned_by = 'user' WHERE id = $1")
            .bind(face)
            .bind(person)
            .execute(fx.pool())
            .await
            .unwrap();
    }
    let third = person_of(fx.pool(), ids[10]).await.unwrap();
    sqlx::query("UPDATE faces SET person_id = NULL, assigned_by = 'user' WHERE person_id = $1")
        .bind(third)
        .execute(fx.pool())
        .await
        .unwrap();

    cluster(&fx, alice.id).await;
    assert_eq!(person_of(fx.pool(), ids[1]).await, Some(second));
    assert_eq!(person_of(fx.pool(), ids[2]).await, None);
    assert_eq!(person_of(fx.pool(), ids[3]).await, Some(first));
    for face in &ids[10..15] {
        assert_eq!(person_of(fx.pool(), *face).await, None);
    }
    let left: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM people WHERE owner_id = $1 ORDER BY id")
            .bind(alice.id)
            .fetch_all(fx.pool())
            .await
            .unwrap();
    let mut expected = vec![first, second, named];
    expected.sort();
    assert_eq!(left, expected, "the emptied unnamed person is deleted");
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn turning_faces_off_mid_cluster_leaves_nothing(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;

    // `turn_off` mid-way: it holds the owner row exclusively.
    let mut turning_off = fx.pool().begin().await.unwrap();
    sqlx::query("SELECT 1 FROM users WHERE id = $1 FOR UPDATE")
        .bind(alice.id)
        .execute(&mut *turning_off)
        .await
        .unwrap();
    ml_settings::set_faces_enabled(&mut *turning_off, alice.id, false)
        .await
        .unwrap();
    sqlx::query("DELETE FROM faces WHERE owner_id = $1")
        .bind(alice.id)
        .execute(&mut *turning_off)
        .await
        .unwrap();

    let ctx = fx.ctx.clone();
    let owner = alice.id;
    let run = tokio::spawn(async move {
        cluster_job::cluster_owner(&ctx, owner, &SFACE, cluster_job::RUN_BUDGET).await
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!run.is_finished(), "clustering waits for the owner lock");
    turning_off.commit().await.unwrap();
    assert_eq!(
        run.await.unwrap().unwrap(),
        cluster_job::ClusterRun::default()
    );
    assert_eq!(people(fx.pool(), alice.id).await, 0);
}

/// With the face models: the NASA portraits and their mirror images (four
/// photos of each astronaut) become exactly two people.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn the_portraits_become_two_people(pool: PgPool) {
    let Some((dylib, models)) =
        squirrel_ml::testutil::test_ml_for(&[squirrel_ml::ModelGroup::Faces])
    else {
        return;
    };
    squirrel_ml::runtime::init(&dylib).unwrap();
    let mut analyzer = squirrel_ml::faces::FaceAnalyzer::load(&models, 2).unwrap();
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let mut ids = Vec::new();
    for name in ["koch-a", "koch-b", "glover-a", "glover-b"] {
        let path = format!(
            "{}/../squirrel-ml/tests/fixtures/faces/{name}.jpg",
            env!("CARGO_MANIFEST_DIR")
        );
        let original = image::open(path).unwrap().to_rgb8();
        let mirrored = image::imageops::flip_horizontal(&original);
        for (variant, picture) in [("", original), ("-mirrored", mirrored)] {
            let found = analyzer.analyze(&picture).unwrap();
            assert_eq!(found.len(), 1, "{name}{variant}");
            let sha = photo(&fx, alice.id, &format!("{name}{variant}.jpg")).await;
            ids.push(faces(fx.pool(), alice.id, &sha, &[found[0].embedding.clone()]).await[0]);
        }
    }
    cluster(&fx, alice.id).await;
    assert_eq!(people(fx.pool(), alice.id).await, 2);
    let koch = person_of(fx.pool(), ids[0]).await;
    let glover = person_of(fx.pool(), ids[4]).await;
    assert!(koch.is_some() && glover.is_some() && koch != glover);
    for face in &ids[..4] {
        assert_eq!(person_of(fx.pool(), *face).await, koch);
    }
    for face in &ids[4..] {
        assert_eq!(person_of(fx.pool(), *face).await, glover);
    }
}

/// Clustering time for one owner with `SQUIRREL_CLUSTER_BENCH_FACES` synthetic
/// faces (default 1 000, which must take under 5 s): 50-face groups plus 10 %
/// lone faces, all unassigned.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_thousand_faces_cluster_quickly(pool: PgPool) {
    let count: usize = std::env::var("SQUIRREL_CLUSTER_BENCH_FACES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000);
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let lone = count / 10;
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut noise = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % 1000) as f32 / 1000.0 - 0.5
    };
    let embeddings: Vec<Vec<f32>> = (0..count)
        .map(|i| {
            let mut v: Vec<f32> = (0..128).map(|_| noise() * 0.12).collect();
            if i < count - lone {
                // 50-face groups on pairs of axes.
                let group = i % ((count - lone) / 50).max(1);
                v[group % 128] += 1.0;
                v[(group * 7 + 3) % 128] += 0.7;
            } else {
                v.iter_mut().for_each(|x| *x *= 8.0);
            }
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        })
        .collect();
    sqlx::query(
        "INSERT INTO faces (id, owner_id, sha256, model, box_x, box_y, box_w, box_h, \
                            landmarks, score, embedding) \
         SELECT gen_random_uuid(), $1, $2, 'yunet-sface-2021dec', 0.1, 0.1, 0.2, 0.2, \
                array_fill(0.2::real, ARRAY[10]), 0.9, e::halfvec \
           FROM unnest($3::text[]) e",
    )
    .bind(alice.id)
    .bind(&sha)
    .bind(
        embeddings
            .iter()
            .map(|v| format!("{v:?}"))
            .collect::<Vec<_>>(),
    )
    .execute(fx.pool())
    .await
    .unwrap();

    let started = Instant::now();
    let run = cluster(&fx, alice.id).await;
    let took = started.elapsed();
    println!(
        "clustered {count} faces in {took:?}: {} assigned, {} people",
        run.faces_assigned, run.people_created
    );
    assert!(run.faces_assigned as usize >= count - lone, "{run:?}");
    if count <= 1_000 {
        assert!(took < Duration::from_secs(5), "{took:?}");
    }
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_long_run_hands_over_to_the_next(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;
    // Out of time after its first batch: the job comes back at once.
    let many: Vec<Vec<f32>> = (0..250).map(|i| unit(&[(64 + i % 64, 1.0)])).collect();
    faces(fx.pool(), alice.id, &sha, &many).await;
    let out_of_time = cluster_job::cluster_owner(&fx.ctx, alice.id, &SFACE, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(out_of_time.left, cluster_job::Left::Budget);
    let rerun = cluster_job::run(
        &fx.ctx,
        job("cluster_faces", json!({ "owner_id": alice.id })),
    )
    .await;
    assert!(
        rerun.is_ok(),
        "lone faces seen by the run do not bring it back"
    );
    // The three groups, plus the 64 groups of the 250 extra faces.
    assert_eq!(people(fx.pool(), alice.id).await, 3 + 64);
}

/// Many faces that never cluster (lone faces) must not keep new faces from
/// being clustered: each run goes on from where the last one stopped, and a
/// lone face is tried again only once newer faces exist.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn waiting_faces_never_starve_new_ones(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let lone: Vec<Vec<f32>> = (0..450)
        .map(|i| {
            unit(&[
                (i % 128, 1.0),
                ((i * 37 + 11) % 128, 0.9),
                ((i * 53 + 5) % 128, 0.8),
            ])
        })
        .collect();
    faces(fx.pool(), alice.id, &sha, &lone).await;
    sqlx::query("UPDATE faces SET created_at = created_at - interval '1 hour'")
        .execute(fx.pool())
        .await
        .unwrap();
    let group: Vec<Vec<f32>> = (0..4)
        .map(|i| unit(&[(0, 1.0), (1, 1.0), (2 + i, 0.2)]))
        .collect();
    let new = faces(fx.pool(), alice.id, &sha, &group).await;

    // Each run gets time for one batch of 200 faces only.
    let mut runs = 0;
    while person_of(fx.pool(), new[0]).await.is_none() {
        runs += 1;
        assert!(runs <= 4, "new faces still unclustered after {runs} runs");
        cluster_job::cluster_owner(&fx.ctx, alice.id, &SFACE, Duration::ZERO)
            .await
            .unwrap();
    }
    // Nothing new: the lone faces are not tried again.
    let idle = cluster_job::cluster_owner(&fx.ctx, alice.id, &SFACE, Duration::ZERO)
        .await
        .unwrap();
    assert_eq!(idle.left, cluster_job::Left::Nothing, "{idle:?}");
}

async fn cluster_jobs(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as("SELECT dedupe_key, status FROM jobs WHERE kind = 'cluster_faces' ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// A request made while a run is going (its own key is taken by the running
/// job) is kept as a follow-up run, even when the faces are committed after
/// the run read its last page.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn a_request_during_a_run_is_never_lost(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let main = format!("cluster_faces:{}", alice.id);
    let mut conn = fx.pool().acquire().await.unwrap();
    squirrel_worker::jobs::request_clustering(&mut conn, alice.id)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE jobs SET status = 'running', claim_id = gen_random_uuid(), \
                 claimed_by = 't', locked_until = now() + interval '1 minute'",
    )
    .execute(fx.pool())
    .await
    .unwrap();
    for _ in 0..2 {
        squirrel_worker::jobs::request_clustering(&mut conn, alice.id)
            .await
            .unwrap();
    }
    assert_eq!(
        cluster_jobs(fx.pool()).await,
        [
            (main.clone(), "running".to_owned()),
            (format!("{main}:next"), "queued".to_owned())
        ]
    );
    // Turning faces off drops the queued follow-up too.
    squirrel_db::faces::turn_off(fx.pool(), alice.id)
        .await
        .unwrap();
    assert_eq!(
        cluster_jobs(fx.pool()).await,
        [(main.clone(), "running".to_owned())]
    );

    // While a request's transaction is open, the queued run cannot start:
    // it would miss the faces that transaction stores.
    sqlx::query("DELETE FROM jobs")
        .execute(fx.pool())
        .await
        .unwrap();
    squirrel_worker::jobs::request_clustering(&mut conn, alice.id)
        .await
        .unwrap();
    let mut storing = fx.pool().begin().await.unwrap();
    squirrel_worker::jobs::request_clustering(&mut storing, alice.id)
        .await
        .unwrap();
    // The queue's claim: `FOR UPDATE SKIP LOCKED` on queued rows.
    let claimable = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM (SELECT id FROM jobs WHERE status = 'queued' \
               FOR UPDATE SKIP LOCKED) j",
        )
        .fetch_one(fx.pool())
        .await
        .unwrap()
    };
    assert_eq!(
        claimable().await,
        0,
        "the storing transaction holds the queued run"
    );
    storing.commit().await.unwrap();
    assert_eq!(claimable().await, 1);
}

#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn an_emptied_hidden_person_stays_hidden(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let hidden = Uuid::now_v7();
    sqlx::query("INSERT INTO people (id, owner_id, hidden) VALUES ($1, $2, true)")
        .bind(hidden)
        .bind(alice.id)
        .execute(fx.pool())
        .await
        .unwrap();
    cluster(&fx, alice.id).await;
    assert_eq!(people(fx.pool(), alice.id).await, 1, "the hide is kept");
}

/// After the user merges and splits people, clustering moves none of the
/// pinned faces; faces pinned to nobody never count as anyone's neighbours.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn clustering_after_corrections_keeps_them(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let ids = faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;
    cluster(&fx, alice.id).await;
    let first = person_of(fx.pool(), ids[0]).await.unwrap();
    let second = person_of(fx.pool(), ids[5]).await.unwrap();
    let merged = squirrel_db::people::merge(fx.pool(), alice.id, first, &[second])
        .await
        .unwrap();
    assert!(matches!(merged, squirrel_db::people::Edit::Done(_)));
    let split = squirrel_db::people::split(fx.pool(), alice.id, first, &ids[5..7], None)
        .await
        .unwrap();
    let squirrel_db::people::Edit::Done(split) = split else {
        panic!("{split:?}");
    };
    let before: Vec<Option<Uuid>> = {
        let mut all = Vec::new();
        for face in &ids {
            all.push(person_of(fx.pool(), *face).await);
        }
        all
    };
    // New faces close to the second group: they join a person, never move one.
    let new = faces(fx.pool(), alice.id, &sha, &[unit(&[(1, 1.0), (50, 0.2)])]).await;
    cluster(&fx, alice.id).await;
    for (face, was) in ids.iter().zip(&before) {
        assert_eq!(person_of(fx.pool(), *face).await, *was);
    }
    let joined = person_of(fx.pool(), new[0]).await;
    assert!(
        joined == Some(first) || joined == Some(split.id),
        "{joined:?}"
    );

    // A group pinned to nobody but for two faces: those two are not enough.
    let lone_group: Vec<Vec<f32>> = (0..5).map(|i| unit(&[(80, 1.0), (90 + i, 0.25)])).collect();
    let pinned = faces(fx.pool(), alice.id, &sha, &lone_group).await;
    sqlx::query("UPDATE faces SET assigned_by = 'user' WHERE id = ANY($1)")
        .bind(&pinned[..3])
        .execute(fx.pool())
        .await
        .unwrap();
    cluster(&fx, alice.id).await;
    for face in &pinned {
        assert_eq!(person_of(fx.pool(), *face).await, None);
    }
}

/// `squirrel admin faces recluster`: automatic assignments start over, the
/// user's pins and named people stay.
#[sqlx::test(migrator = "squirrel_db::MIGRATOR")]
async fn reclustering_starts_automatic_assignments_over(pool: PgPool) {
    let fx = Fixture::new(pool);
    let alice = admin(fx.pool()).await;
    let sha = photo(&fx, alice.id, "a.jpg").await;
    let ids = faces(fx.pool(), alice.id, &sha, &groups_and_noise()).await;
    cluster(&fx, alice.id).await;
    let kept = person_of(fx.pool(), ids[0]).await.unwrap();
    sqlx::query("UPDATE people SET name = 'Anna' WHERE id = $1")
        .bind(kept)
        .execute(fx.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE faces SET assigned_by = 'user' WHERE id = $1")
        .bind(ids[0])
        .execute(fx.pool())
        .await
        .unwrap();

    let reset = squirrel_db::clustering::reset_automatic(fx.pool(), alice.id)
        .await
        .unwrap();
    assert_eq!(
        reset,
        Some(14),
        "every automatic assignment but the pinned face"
    );
    assert_eq!(person_of(fx.pool(), ids[0]).await, Some(kept));
    assert_eq!(person_of(fx.pool(), ids[1]).await, None);
    assert_eq!(people(fx.pool(), alice.id).await, 1, "only Anna is left");
    cluster(&fx, alice.id).await;
    // One pinned face among four unassigned ones does not pull them in (the
    // guard against merging through one face): they found their own person.
    let regrouped = person_of(fx.pool(), ids[1]).await;
    assert!(regrouped.is_some() && regrouped != Some(kept));
    assert_eq!(people(fx.pool(), alice.id).await, 4);

    squirrel_db::ml_settings::set_faces_enabled(fx.pool(), alice.id, false)
        .await
        .unwrap();
    assert_eq!(
        squirrel_db::clustering::reset_automatic(fx.pool(), alice.id)
            .await
            .unwrap(),
        None,
        "faces are off"
    );
}
