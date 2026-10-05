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

/// Trigonometric steps spell out what they use: the multiple-angle rows name x and the corner
/// 3x is, the converse of the law of sines names the triangle, the class w, the side ratio and the
/// configuration it reads, and concludes an angle equation; the link rows name both angles.
#[test]
fn trig_steps_are_spelled_out() {
    let fig = "a@0.1343253817924288_0.719445251994107 b@-0.5210702983270747_-0.8349066943186165 \
               c@0.13033572228610057_-0.8094323774776992 d@-0.28816543221817836_-0.825798561809395 \
               e@-0.07403070417618249_-0.8174244667961947 f@0.13102232498787159_-0.5463193199830535 \
               g@0.13200375239858442_-0.1702264702525801 h@-0.24396157915667044_-0.17770908065078395 \
               i@-0.39075010291486945_-0.525836226619042 j@-0.13598988595056216_-0.6644871233213832 \
               k@-0.03509469205440549_-0.5302258581151602 l@-0.20181595541531305_-0.5099786896844436 = \
               coll d b c, coll e b c, eqangle a b a d a d a e, eqangle a d a e a e a c, coll f c a, \
               coll g c a, eqangle b c b f b f b g, eqangle b f b g b g b a, coll h a b, coll i a b, \
               eqangle c a c h c h c i, eqangle c h c i c i c b, coll j b f, coll j c i, coll k a e, \
               coll k c h, coll l a d, coll l b g ? cong j l j k";
    let proof = solve_problem_with_proof(&Problem::parse(fig).unwrap()).unwrap().expect("Morley");
    let lines: Vec<&str> = proof.lines().collect();
    let triple: Vec<&&str> = lines.iter().filter(|l| l.contains("triple-angle formula: x = ∠(")).collect();
    assert_eq!(triple.len(), 3, "{proof}");
    assert!(triple.iter().all(|l| l.contains(", 3x ≡ ±∠(")), "{proof}");
    let converse: Vec<&&str> = lines.iter().filter(|l| l.contains("law of sines, converse: in △")).collect();
    assert_eq!(converse.len(), 3, "{proof}");
    assert!(
        converse.iter().all(|l| l.contains(" with w = ∠(") && l.contains("in this configuration") && l.contains(") = w [")),
        "{proof}"
    );
    assert!(lines.iter().any(|l| l.contains("law of sines: in △") && l.contains(" / |sin ∠(")), "{proof}");
    assert!(!proof.contains("formula: sin x"), "an unspelled row:\n{proof}");
}
