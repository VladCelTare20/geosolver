//! Auxiliary-point search validation.
//!
//! We take a real IMO problem whose auxiliary points are provided, delete a
//! point whose *entire* definition is a single classical construction, confirm
//! that pure DDAR can no longer prove the goal, and check that the search
//! rediscovers a construction that restores the proof.

use ddar::aux_search::{solve_with_aux, strip_point};
use ddar::geo::compile;
use ddar::runner::{parse_dataset, solve_problem};
use ddar::Problem;

const DATASET: &str = include_str!("../problems.tsv");

fn load(name: &str) -> Problem {
    let entries = parse_dataset(DATASET);
    let e = entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("problem {name} not found"));
    Problem::parse(e.problem).expect("parse")
}

/// `o` in IMO 2001 P5 is exactly the circumcenter of `b, t, p`
/// (`cong o b o t, cong o t o p`), and the goal (`acompute b a b c`) does not
/// mention it. Removing `o` breaks the proof; a single circumcenter
/// construction should bring it back.
#[test]
fn rediscovers_circumcenter_2001_p5a() {
    let full = load("2001_p5a");
    assert!(solve_problem(&full).unwrap(), "full problem should solve");

    let stripped = strip_point(&full, "o");
    assert!(
        !solve_problem(&stripped).unwrap(),
        "DDAR alone should fail once the load-bearing point is removed"
    );

    let (proof, stats) = solve_with_aux(&stripped, 1, 100_000, false);
    let proof = proof.expect("aux search should find a construction");
    assert_eq!(proof.constructions.len(), 1);
    eprintln!(
        "2001_p5a: rediscovered `{}` in {} DDAR runs",
        proof.constructions[0].desc, stats.runs
    );
}

/// `a2` and `b2` in IMO 2019 P2 are *second intersections* of a cevian with
/// the circumcircle (`coll a2 a a1, cong o a2 o a`). These have the
/// line∩circle shape that the candidate generator previously could not
/// produce at all, so this test guards the coverage gained by the ranked
/// line/circle intersection candidates.
///
/// ~1500 DDAR runs per point: fast in release, minutes in debug — so it is
/// ignored under debug builds (run with `cargo test --release`).
#[cfg_attr(debug_assertions, ignore)]
#[test]
fn rediscovers_line_circle_intersection_2019_p2() {
    let full = load("2019_p2");
    assert!(solve_problem(&full).unwrap(), "full problem should solve");

    for name in ["a2", "b2"] {
        let stripped = strip_point(&full, name);
        assert!(
            !solve_problem(&stripped).unwrap(),
            "DDAR alone should fail without {name}"
        );
        let (proof, stats) = solve_with_aux(&stripped, 1, 100_000, false);
        let proof = proof.unwrap_or_else(|| panic!("aux search should rediscover {name}"));
        assert_eq!(proof.constructions.len(), 1);
        eprintln!(
            "2019_p2 minus {name}: rediscovered `{}` in {} DDAR runs",
            proof.constructions[0].desc, stats.runs
        );
    }
}

/// The hardest case: solve IMO 2019 P2 from the *bare* problem statement — no
/// circumcircle, no second intersections supplied. The search must invent the
/// circumcircle of the base triangle ABC and rediscover BOTH second
/// intersections (a depth-2 construction) entirely on its own. This is the
/// end-to-end guard for `--geo examples/imo/2019_p2.geo`.
///
/// With the cevian-aware ranking this now solves in ~0.4 s (a few hundred DDAR
/// runs), down from ~20 s; kept `#[ignore]` in debug where DDAR is far slower.
#[cfg_attr(debug_assertions, ignore)]
#[test]
fn solves_2019_p2_from_bare_statement() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/imo/2019_p2.geo"
    ))
    .expect("read geo");
    let compiled = compile(&src).expect("compile 2019_p2.geo");
    assert!(
        !solve_problem(&compiled.problem).unwrap(),
        "bare 2019_p2 should NOT be provable by pure DDAR"
    );
    let (proof, stats) = solve_with_aux(&compiled.problem, 2, 200_000, false);
    let proof = proof.expect("aux search must solve 2019 P2 from the bare statement");
    assert_eq!(
        proof.constructions.len(),
        2,
        "expected exactly two auxiliary points (the two second intersections)"
    );
    // Both constructions are second intersections with the circumcircle of ABC.
    for c in &proof.constructions {
        assert!(
            c.desc.contains("circumcircle(A,B,C)"),
            "unexpected construction: {}",
            c.desc
        );
    }
    eprintln!(
        "2019_p2 (bare): solved with {} in {} DDAR runs",
        proof
            .constructions
            .iter()
            .map(|c| c.desc.clone())
            .collect::<Vec<_>>()
            .join(" + "),
        stats.runs
    );
}
