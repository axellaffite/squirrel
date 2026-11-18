//! Incremental face clustering: what to do with one unassigned face, given its
//! nearest neighbours among the owner's faces (Immich's algorithm, adapted).
//! Pure: the worker finds the neighbours in Postgres and applies the decision.
//!
//! A face is **core** when it has at least `min_faces - 1` neighbours within
//! `max_distance` and a detector score of at least `min_score`. A face joins
//! the person most of its close neighbours belong to (ties: the nearest),
//! unless it is core, and fewer than `min_faces - 1` of them, and fewer than
//! its unassigned ones, belong to that person: then, as with no person among
//! them, a core face founds a person with its unassigned neighbours. Any other
//! face waits for a later run.

use std::collections::HashMap;
use std::hash::Hash;

/// Clustering thresholds of one face model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterParams {
    /// Cosine distance (1 − similarity) under which two faces are neighbours.
    pub max_distance: f32,
    /// Faces (this one included) a core face needs within `max_distance`.
    pub min_faces: usize,
    /// Detector score a face needs to found a person.
    pub min_score: f32,
}

/// YuNet + SFace (`yunet-sface-2021dec`). SFace's "same person" threshold is
/// cosine similarity 0.363 (distance 0.637); clustering is transitive, so it
/// uses a stricter distance. **Provisional** until tuned on a real library.
pub const SFACE: ClusterParams = ClusterParams {
    max_distance: 0.50,
    min_faces: 3,
    min_score: 0.7,
};

/// Neighbours read per face (the nearest first).
pub const NEIGHBOURS: usize = 16;

/// One of a face's nearest faces. Faces pinned to nobody are never
/// neighbours (the caller leaves them out).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Neighbour<F, P> {
    pub face: F,
    /// `None`: an unassigned automatic face.
    pub person: Option<P>,
    pub distance: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<F, P> {
    /// Join this existing person.
    Join(P),
    /// Found a new person with these unassigned faces (the face itself
    /// first).
    Found(Vec<F>),
    /// Stay unassigned until a later run.
    Wait,
}

/// What to do with unassigned face `face` (detector score `score`), given its
/// neighbours in any order.
pub fn decide<F, P>(
    face: F,
    score: f32,
    neighbours: &[Neighbour<F, P>],
    params: &ClusterParams,
) -> Decision<F, P>
where
    F: Copy + Eq,
    P: Copy + Eq + Hash,
{
    let close: Vec<&Neighbour<F, P>> = neighbours
        .iter()
        .filter(|n| n.face != face && n.distance.is_finite() && n.distance <= params.max_distance)
        .collect();
    let unassigned: Vec<F> = close
        .iter()
        .filter(|n| n.person.is_none())
        .map(|n| n.face)
        .collect();
    let core = close.len() + 1 >= params.min_faces && score >= params.min_score;
    match majority(&close) {
        // A single face of a person among mostly unassigned neighbours: a
        // group of its own that merely touches that person through one
        // ambiguous face, which must not merge the two groups.
        Some((_, votes)) if core && votes < unassigned.len() && votes + 1 < params.min_faces => {}
        Some((person, _)) => return Decision::Join(person),
        None if !core => return Decision::Wait,
        None => {}
    }
    Decision::Found(std::iter::once(face).chain(unassigned).collect())
}

/// The person most `close` neighbours belong to, with its vote count; ties go
/// to the person with the nearest face.
fn majority<F, P: Copy + Eq + Hash>(close: &[&Neighbour<F, P>]) -> Option<(P, usize)> {
    let mut votes: HashMap<P, (usize, f32)> = HashMap::new();
    for n in close {
        if let Some(person) = n.person {
            let entry = votes.entry(person).or_insert((0, f32::INFINITY));
            entry.0 += 1;
            entry.1 = entry.1.min(n.distance);
        }
    }
    votes
        .into_iter()
        .max_by(|(_, (a_count, a_near)), (_, (b_count, b_near))| {
            a_count.cmp(b_count).then_with(|| b_near.total_cmp(a_near))
        })
        .map(|(person, (votes, _))| (person, votes))
}

/// Cosine distance of two unit vectors.
pub fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    1.0 - a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>()
}

#[cfg(test)]
mod tests;
