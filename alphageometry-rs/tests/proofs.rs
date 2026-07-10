//! Proof traceback validation.

use ddar::runner::{parse_dataset, solve_problem_with_proof};
use ddar::Problem;

const DATASET: &str = include_str!("../problems.tsv");

/// A tiny problem: M is the midpoint of AB and lies on segment CD as well; the
/// goal `cong m a m b` follows from one assumption. Crucially the problem also
/// carries an *irrelevant* assumption about other points, which must NOT appear
/// in the proof.
#[test]
fn proof_is_minimal_and_readable() {
    // NB: c sits on the perpendicular bisector of ab so `cong c a c b` is
    // numerically true (the engine's debug asserts verify every assumption).
    let p = Problem::parse(
        "a@0.0_0.0 = ; b@2.0_0.0 = ; m@1.0_0.0 = ; c@1.0_1.0 = ; d@2.0_3.0 = \
         cong m a m b, cong c a c b ? cong m a m b",
    )
    .unwrap();
    let proof = solve_problem_with_proof(&p).unwrap().expect("should prove");
    eprintln!("{proof}");
    assert!(proof.contains("cong m a m b"), "cites the used assumption");
    assert!(
        !proof.contains("cong c a c b"),
        "must not cite the irrelevant assumption:\n{proof}"
    );
}

/// Every bundled IMO problem must still be proved with tracking on, and the
/// proof must be non-trivial (cites at least one assumption).
#[test]
fn all_imo_problems_prove_with_tracking() {
    let entries = parse_dataset(DATASET);
    let mut failures = Vec::new();
    for e in &entries {
        let problem = Problem::parse(e.problem).unwrap();
        match solve_problem_with_proof(&problem) {
            Ok(Some(proof)) => {
                if !proof.contains("assumption:") {
                    failures.push(format!("{}: proof cites no assumption", e.name));
                }
            }
            Ok(None) => failures.push(format!("{}: not proven with tracking", e.name)),
            Err(err) => failures.push(format!("{}: {err}", e.name)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The proof of a real IMO problem should be dramatically smaller than the
/// total number of facts derived during the closure (that is the point of
/// provenance tracking).
#[test]
fn imo_proof_is_a_small_fraction_of_the_closure() {
    let entries = parse_dataset(DATASET);
    let e = entries.iter().find(|e| e.name == "2000_p1").unwrap();
    let problem = Problem::parse(e.problem).unwrap();
    let proof = solve_problem_with_proof(&problem).unwrap().expect("prove");
    // Header looks like: "Proof of ... (N steps, M facts recorded in total):"
    let header = proof.lines().next().unwrap().to_string();
    let nums: Vec<usize> = header
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    // First numbers after the goal text: steps, total facts. Goal text has
    // digits too ("2000_p1" is not in the goal), so take the last two.
    let steps = nums[nums.len() - 2];
    let total = nums[nums.len() - 1];
    eprintln!("2000_p1: {steps} proof steps of {total} recorded facts");
    assert!(
        steps >= 3,
        "expected a non-trivial proof, got {steps} steps"
    );
    assert!(
        steps * 2 < total,
        "proof should be much smaller than the closure ({steps} vs {total})"
    );
}
