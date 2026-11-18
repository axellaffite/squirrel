use super::*;

const P: ClusterParams = SFACE;

fn n(face: u32, person: Option<u32>, distance: f32) -> Neighbour<u32, u32> {
    Neighbour {
        face,
        person,
        distance,
    }
}

#[test]
fn a_face_joins_the_person_most_of_its_neighbours_have() {
    let neighbours = [
        n(1, Some(10), 0.30),
        n(2, Some(20), 0.10),
        n(3, Some(10), 0.35),
        n(4, None, 0.20),
    ];
    assert_eq!(decide(0, 0.9, &neighbours, &P), Decision::Join(10));
}

#[test]
fn a_tie_goes_to_the_nearest_person() {
    let neighbours = [n(1, Some(10), 0.30), n(2, Some(20), 0.10)];
    assert_eq!(decide(0, 0.9, &neighbours, &P), Decision::Join(20));
}

#[test]
fn an_exact_tie_goes_to_the_smallest_person_id() {
    for order in [[(1, 30), (2, 20)], [(2, 20), (1, 30)]] {
        let neighbours: Vec<_> = order.iter().map(|&(f, p)| n(f, Some(p), 0.2)).collect();
        assert_eq!(decide(0, 0.9, &neighbours, &P), Decision::Join(20));
    }
}

#[test]
fn neighbours_past_the_distance_do_not_count() {
    let neighbours = [n(1, Some(10), 0.51), n(2, None, 0.2), n(3, None, 0.3)];
    assert_eq!(
        decide(0, 0.9, &neighbours, &P),
        Decision::Found(vec![0, 2, 3])
    );
}

#[test]
fn a_core_face_founds_a_person_with_its_unassigned_neighbours() {
    let neighbours = [n(1, None, 0.2), n(2, None, 0.4), n(3, None, 0.7)];
    assert_eq!(
        decide(0, 0.9, &neighbours, &P),
        Decision::Found(vec![0, 1, 2])
    );
}

#[test]
fn a_face_without_enough_neighbours_or_score_waits() {
    assert_eq!(decide(0, 0.9, &[n(1, None, 0.2)], &P), Decision::Wait);
    let two = [n(1, None, 0.2), n(2, None, 0.3)];
    assert_eq!(decide(0, 0.65, &two, &P), Decision::Wait, "score below 0.7");
    let nan = [n(1, None, f32::NAN), n(2, None, 0.3)];
    assert_eq!(decide(0, 0.9, &nan, &P), Decision::Wait);
}

#[test]
fn a_core_face_mostly_among_unassigned_faces_founds_its_own_person() {
    let neighbours = [n(1, Some(10), 0.1), n(2, None, 0.2), n(3, None, 0.3)];
    assert_eq!(
        decide(0, 0.9, &neighbours, &P),
        Decision::Found(vec![0, 2, 3])
    );
    let even = [n(1, Some(10), 0.1), n(2, None, 0.2)];
    assert_eq!(decide(0, 0.9, &even, &P), Decision::Join(10));
    // Two faces of the person: a group that grows, whatever is unassigned.
    let growing = [
        n(1, Some(10), 0.1),
        n(2, Some(10), 0.2),
        n(3, None, 0.2),
        n(4, None, 0.3),
        n(5, None, 0.3),
    ];
    assert_eq!(decide(0, 0.9, &growing, &P), Decision::Join(10));
}

#[test]
fn a_low_score_face_still_joins_a_known_person() {
    assert_eq!(
        decide(0, 0.61, &[n(1, Some(7), 0.2)], &P),
        Decision::Join(7)
    );
}

#[test]
fn the_face_itself_is_never_its_own_neighbour() {
    let neighbours = [n(0, None, 0.0), n(1, None, 0.2)];
    assert_eq!(decide(0, 0.9, &neighbours, &P), Decision::Wait);
}

/// Unit vectors near `axis` (a tight group) or spread over unused axes.
fn unit(values: &[(usize, f32)]) -> Vec<f32> {
    let mut v = [0.0f32; 32];
    for (i, x) in values {
        v[*i] = *x;
    }
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / norm).collect()
}

/// Runs `decide` over `faces` in order, as the worker does with the
/// database (exact neighbours, assignments visible to later faces).
fn cluster(faces: &[Vec<f32>]) -> Vec<Option<usize>> {
    let mut person: Vec<Option<usize>> = vec![None; faces.len()];
    let mut next = 0;
    for f in 0..faces.len() {
        if person[f].is_some() {
            continue;
        }
        let mut neighbours: Vec<Neighbour<usize, usize>> = (0..faces.len())
            .filter(|&o| o != f)
            .map(|o| Neighbour {
                face: o,
                person: person[o],
                distance: cosine_distance(&faces[f], &faces[o]),
            })
            .collect();
        neighbours.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        neighbours.truncate(NEIGHBOURS);
        match decide(f, 0.9, &neighbours, &P) {
            Decision::Join(p) => person[f] = Some(p),
            Decision::Found(members) => {
                for m in members {
                    person[m].get_or_insert(next);
                }
                next += 1;
            }
            Decision::Wait => {}
        }
    }
    person
}

#[test]
fn three_tight_groups_become_three_people_and_noise_stays_alone() {
    let mut faces = Vec::new();
    for group in 0..3 {
        for i in 0..5 {
            faces.push(unit(&[(group, 1.0), (10 + i, 0.25)]));
        }
    }
    for i in 0..4 {
        faces.push(unit(&[(20 + i, 1.0)]));
    }
    let people = cluster(&faces);
    for group in 0..3 {
        let members = &people[group * 5..group * 5 + 5];
        assert!(
            members.iter().all(|p| p.is_some() && *p == members[0]),
            "{people:?}"
        );
    }
    let firsts: std::collections::HashSet<_> = (0..3).map(|g| people[g * 5]).collect();
    assert_eq!(firsts.len(), 3, "{people:?}");
    assert!(people[15..].iter().all(Option::is_none), "{people:?}");
}

#[test]
fn a_face_between_two_people_joins_one_and_never_merges_them() {
    // Two groups 0.7 apart in cosine distance; a face halfway is 0.19 from both.
    let a: Vec<Vec<f32>> = (0..4).map(|i| unit(&[(0, 1.0), (10 + i, 0.1)])).collect();
    let b: Vec<Vec<f32>> = (0..3)
        .map(|i| unit(&[(0, 0.3), (1, 0.954), (14 + i, 0.1)]))
        .collect();
    let bridge = unit(&[(0, 0.809), (1, 0.588)]);
    let faces: Vec<Vec<f32>> = a.iter().chain(&b).cloned().chain([bridge]).collect();
    let people = cluster(&faces);
    assert_ne!(people[0], people[4], "{people:?}");
    assert!(
        people[7] == people[0] || people[7] == people[4],
        "{people:?}"
    );
}
