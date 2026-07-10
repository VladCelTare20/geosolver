//! Robustness regressions for bugs found by adversarial review.

use ddar::aux_search::{solve_with_aux, strip_point};
use ddar::runner::{parse_dataset, solve_problem};
use ddar::Problem;

const DATASET: &str = include_str!("../problems.tsv");

fn load(name: &str) -> Problem {
    let e = parse_dataset(DATASET)
        .into_iter()
        .find(|e| e.name == name)
        .unwrap();
    Problem::parse(e.problem).unwrap()
}

/// A "collinear" triple with a long base and a middle point within ATOM of the
/// line (but with determinant > ATOM) must NOT be admitted as a triangle by the
/// similar-triangle search — doing so forced a false ratio and panicked the
/// debug assert / crashed release. It must simply not prove a false goal.
#[test]
fn flat_triangle_does_not_force_a_false_ratio() {
    // p2 is 1e-13 off the line p1p3 (perp distance < ATOM) but det = 100*1e-13
    // = 1e-11 > ATOM. cong p1p2 = p1p3 is FALSE (40 != 100).
    let p = Problem::parse(
        "p1@0.0_0.0 = ; p2@40.0_0.0000000000001 = ; p3@100.0_0.0 = ; q@1.0_1.0 = \
         coll p1 p2 p3 ? cong p1 p2 p1 p3",
    )
    .unwrap();
    // Must not panic (debug) or crash (release), and must not prove the false goal.
    let ok = solve_problem(&p).unwrap();
    assert!(!ok, "must not prove a numerically false ratio");
}

/// Regression for the salient-line completeness bug: the aux search must still
/// rediscover constructions built on lines whose endpoints never appear
/// adjacently in a predicate. In IMO 2005 P1 the point `p`'s role can be
/// restored by a reflection over line (a2, c2) — points that co-occur in no
/// predicate — so a salient-*only* generator would miss it.
#[test]
fn aux_finds_construction_on_non_salient_line_2005_p1() {
    let full = load("2005_p1");
    assert!(solve_problem(&full).unwrap());
    let stripped = strip_point(&full, "p");
    assert!(
        !solve_problem(&stripped).unwrap(),
        "DDAR alone should fail without p"
    );
    let (proof, stats) = solve_with_aux(&stripped, 1, 300_000, false);
    let proof = proof.expect("aux search must rediscover a completing construction");
    eprintln!(
        "2005_p1 minus p: rediscovered `{}` in {} runs",
        proof.constructions[0].desc, stats.runs
    );
}

/// The parallel aux search must be deterministic and never report a solvable
/// problem unsolved: repeated runs return the same construction.
#[test]
fn aux_search_is_deterministic() {
    let full = load("2001_p5a");
    let stripped = strip_point(&full, "o");
    let mut descs = std::collections::HashSet::new();
    for _ in 0..5 {
        let (proof, _) = solve_with_aux(&stripped, 1, 100_000, false);
        descs.insert(proof.expect("should solve").constructions[0].desc.clone());
    }
    assert_eq!(
        descs.len(),
        1,
        "parallel search must be deterministic: {descs:?}"
    );
}
