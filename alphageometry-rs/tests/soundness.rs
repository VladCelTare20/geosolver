//! Soundness and robustness regressions: inputs that used to yield a false
//! "proved", crash the process, or run unbounded.

use ddar::geo::compile;
use ddar::metric;
use ddar::runner::solve_problem;
use ddar::synthetic::{prove_euclidean, Outcome};
use ddar::Problem;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Duration;

/// Run `f` on a fresh 2 MiB-stack thread (the size a server worker gets) and
/// wait at most `secs` for it — an unbounded loop fails the test instead of
/// hanging the suite.
fn bounded<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(secs))
        .unwrap_or_else(|_| panic!("did not finish within {secs}s"))
}

/// Whether the DDAR path (`compile` + `solve_problem`) reports the goal proven.
fn ddar_claims_proof(src: &str) -> bool {
    match compile(src) {
        Ok(c) => solve_problem(&c.problem).unwrap_or(false),
        Err(_) => false,
    }
}

const PYTHAGORAS_WRONG_LEG: &str =
    "A B C = triangle\nprove dist(A,B)^2 = dist(B,C)^2 + dist(A,C)^2";
const FREE_LENGTH_THREE: &str = "A = free\nB = free\nprove dist(A,B) = 3";
const RIGHT_TRAPEZOID_FLAG: &str = "B = free\nC = free\nA = point: perp(A, B, B, C)\n\
     D = point: perp(B, C, C, D), para(A, B, D, C)\nP = free\n\
     Q = point: dist(P, Q) = dist(P, A)\n\
     prove dist(P,Q)^2 + dist(P,C)^2 = dist(P,B)^2 + dist(P,D)^2";

#[test]
fn false_metric_goals_are_not_proved_by_ddar() {
    for src in [PYTHAGORAS_WRONG_LEG, FREE_LENGTH_THREE, RIGHT_TRAPEZOID_FLAG] {
        assert!(!ddar_claims_proof(src), "DDAR claimed a false metric goal:\n{src}");
    }
}

#[test]
fn metric_goals_compile_to_a_routable_error() {
    for src in [PYTHAGORAS_WRONG_LEG, FREE_LENGTH_THREE, RIGHT_TRAPEZOID_FLAG] {
        match compile(src) {
            Err(e) => assert!(e.is_metric_goal(), "expected a metric-goal error, got {e}"),
            Ok(_) => panic!("a metric goal must not compile to a DDAR problem:\n{src}"),
        }
    }
    let e = compile(FREE_LENGTH_THREE).err().unwrap();
    assert!(e.to_string().contains("metric"), "{e}");
    assert!(!compile("A B = segment\nprove coll(A, A, B)")
        .err()
        .is_some_and(|e| e.is_metric_goal()));
}

#[test]
fn metric_goals_still_work_as_hypotheses() {
    let c = compile("A B = segment\nP = point: dist(P,A) = 2*dist(P,B)\nQ = point: dist(A,Q) = 3\n\
         prove cong(A, B, B, A)")
    .expect("metric relations are fine as hypotheses");
    assert!(solve_problem(&c.problem).unwrap());
}

#[test]
fn nan_goal_is_not_proved() {
    let r = metric::solve("A = free\nB = free", "angle(A, A, B) = 1234", 48);
    assert!(r.is_err(), "a NaN quantity must not be proved: {r:?}");
}

#[test]
fn refuted_metric_goal_is_an_err() {
    let r = metric::solve("A = free\nB = free", "dist(A,B) = 3", 48);
    let e = r.expect_err("a refuted goal must be an Err, not an Ok report");
    assert!(e.is_refuted(), "{e}");
    assert!(e.to_string().contains("counterexample"), "{e}");
    assert!(metric::check_numerically("A = free\nB = free", "dist(A,B) = 3", 8).is_err());
}

#[test]
fn true_metric_goal_passes_the_numeric_check() {
    let r = metric::check_numerically(
        "A B = segment\nC = on_tline(B, A, B)",
        "dist(A,C)^2 = dist(A,B)^2 + dist(B,C)^2",
        16,
    );
    let ev = r.expect("a true identity holds in every sampled figure");
    assert!(ev.samples >= 8, "{ev:?}");
    assert!(ev.report.contains("not a proof"), "{}", ev.report);
}

/// A numerical certificate is not a proof: a true goal that no theorem-citing
/// prover reaches must come back as `NoProof` carrying the numeric evidence,
/// never as an `Ok` report.
#[test]
fn numeric_only_goal_is_no_proof_not_ok() {
    let r = metric::solve(
        "A B C = triangle\nM = midpoint(B, C)",
        "area(A,B,M) = area(A,M,C)",
        48,
    );
    match r {
        Err(e @ metric::MetricError::NoProof { .. }) => {
            assert_eq!(e.numerically_holds(), Some(true));
            let ev = e.evidence().expect("NoProof carries its evidence");
            assert!(ev.samples >= 8, "{ev:?}");
            assert!(!e.is_refuted());
            assert!(e.report().starts_with("NOT PROVED"), "{}", e.report());
        }
        other => panic!("expected NoProof with numeric evidence, got {other:?}"),
    }
}

/// The refuted, unproved and failed outcomes report distinct numeric verdicts.
#[test]
fn metric_errors_carry_a_typed_numeric_verdict() {
    let refuted = metric::solve("A = free\nB = free", "dist(A,B) = 3", 16).unwrap_err();
    assert_eq!(refuted.numerically_holds(), Some(false));
    assert!(refuted.evidence().is_none());
    let failed = metric::solve("A = free\nB = free", "dist(A,Z) = 3", 16).unwrap_err();
    assert_eq!(failed.numerically_holds(), None, "{failed:?}");
}

#[test]
fn short_variadic_relations_are_clean_errors() {
    for src in [
        "A = free\nP = point: cyclic(P, A)\nprove cong(A, P, A, P)",
        "A = free\nP = point: coll(P, A)\nprove cong(A, P, A, P)",
        "A B = segment\nassume cyclic(A, B, A)\nprove cong(A, B, A, B)",
        "A B C = triangle\nprove cyclic(A, B, C)",
        "A B = segment\nprove coll(A, B)",
    ] {
        let r = catch_unwind(AssertUnwindSafe(|| compile(src)));
        match r {
            Ok(Err(_)) => {}
            Ok(Ok(_)) => panic!("expected a compile error for:\n{src}"),
            Err(_) => panic!("compile panicked on:\n{src}"),
        }
    }
}

#[test]
fn nested_constructions_count_toward_the_point_cap() {
    let mut x = String::from("C");
    for i in 0..150 {
        x = format!("reflect({x}, line(A, {}))", if i % 2 == 1 { "B" } else { "D" });
    }
    let src = format!("A = free\nB = free\nC = free\nD = free\nX = {x}\nprove coll(A, B, X)");
    let r = bounded(20, move || compile(&src).map(|_| ()).map_err(|e| e.to_string()));
    let e = r.expect_err("hundreds of anonymous points must be rejected");
    assert!(e.contains("too many points"), "{e}");
}

#[test]
fn huge_exponent_is_rejected_quickly() {
    let r = bounded(20, || {
        metric::solve(
            "A B C = triangle",
            "dist(A,B)^100000 = dist(A,C)^2",
            48,
        )
        .map_err(|e| e.to_string())
    });
    assert!(r.is_err(), "{r:?}");
    let r = bounded(20, || {
        compile("A B C = triangle\nP = point: dist(P,A)^100000 = dist(P,B)\nprove coll(A, B, C)")
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    assert!(r.unwrap_err().contains("exponent"));
}

#[test]
fn long_operator_chains_do_not_overflow_the_stack() {
    let chain = vec!["1"; 300_000].join("+");
    let goal = format!("dist(A,B) = {chain}");
    let g2 = goal.clone();
    let r = bounded(60, move || metric::solve("A B = segment", &g2, 8).map(|_| ()).map_err(|e| e.to_string()));
    assert!(r.unwrap_err().contains("too long"));
    let src = format!("A B = segment\nP = point: {goal}\nprove coll(A, B, P)");
    let r = bounded(60, move || compile(&src).map(|_| ()).map_err(|e| e.to_string()));
    assert!(r.unwrap_err().contains("too long"));
    let prod = vec!["dist(A,B)"; 300_000].join("*");
    let src = format!("A B C = triangle\nprove {prod} = dist(A,C)");
    let r = bounded(60, move || compile(&src).map(|_| ()).map_err(|e| e.to_string()));
    assert!(r.unwrap_err().contains("too long"));
}

#[test]
fn extreme_low_level_constants_do_not_wrap_or_panic() {
    let base = "a@0.0_0.0 = ; b@1.0_0.0 = ; c@0.0_1.0 = ? aconst a b a c ";
    for k in ["1/0", "1pi/0", "-9223372036854775808/-1", "9223372036854775807pi/1"] {
        let r = catch_unwind(|| Problem::parse(&format!("{base}{k}")));
        assert!(r.is_ok(), "parsing `{k}` panicked");
    }
    assert!(Problem::parse(&format!("{base}1/0")).is_err());
    let p = Problem::parse(&format!("{base}9223372036854775807pi/1")).unwrap();
    let c = &p.goal.unwrap().constants[0];
    assert!(!c.is_negative(), "180·(2^63−1) wrapped to {c}");
    assert!((c.to_f64() - 9223372036854775807.0 * 180.0).abs() < 1e10);
}

#[test]
fn negating_i64_min_does_not_wrap() {
    use ddar::rational::Rat;
    let m = Rat::from_int(i64::MIN);
    let n = -m.clone();
    assert!(!n.is_negative(), "−(−2^63) wrapped to {n}");
    assert_eq!(&n + &m, Rat::zero());
    assert!(!m.abs().is_negative());
    assert!(!(&Rat::zero() - &m).is_negative());
}

const RIGHT_TRAPEZOID: &str =
    "B = free\nC = free\nA = on_tline(B, B, C)\nD = on_tline(C, B, C)\nP = free";

#[test]
fn right_trapezoid_is_not_a_rectangle() {
    // British flag (false here) + Pythagoras at B, so the flag is a lemma.
    let goal = "dist(P,A)^2 + dist(P,C)^2 + dist(A,C)^2 = \
                dist(P,B)^2 + dist(P,D)^2 + dist(A,B)^2 + dist(B,C)^2";
    if let Ok(Outcome::Proved(p)) = prove_euclidean(RIGHT_TRAPEZOID, goal) {
        panic!("a right trapezoid was treated as a rectangle:\n{p}");
    }
    assert!(metric::solve(RIGHT_TRAPEZOID, goal, 48).is_err());
}

#[test]
fn square_still_gets_the_british_flag() {
    let cons = "A B = segment\nC D = square(A, B)\nP = free";
    let goal = "dist(P,A)^2 + dist(P,C)^2 + dist(A,C)^2 = \
                dist(P,B)^2 + dist(P,D)^2 + dist(A,B)^2 + dist(B,C)^2";
    match prove_euclidean(cons, goal) {
        Ok(Outcome::Proved(p)) => assert!(p.contains("British flag"), "{p}"),
        other => panic!("square lost its rectangle detection: {:?}", other.map(|_| ())),
    }
}

#[test]
fn collinear_points_are_not_a_parallelogram() {
    let cons = "A B = segment\nC = on_line(A, B)\nD = on_line(A, B)\nE = on_tline(A, A, B)\n\
                assume para(A, B, D, C), para(A, D, B, C)";
    let goal = "dist(A,C)^2 + dist(B,D)^2 + dist(B,E)^2 = \
                3*dist(A,B)^2 + 2*dist(B,C)^2 + dist(A,E)^2";
    if let Ok(Outcome::Proved(p)) = prove_euclidean(cons, goal) {
        panic!("four collinear points were treated as a parallelogram:\n{p}");
    }
    assert!(metric::solve(cons, goal, 48).is_err());
}

/// `k` is a free point that merely *happens* to lie on circle(o, x) in the
/// given coordinates; nothing in the hypotheses puts it there, so
/// `cong o k o x` is false in general. Aux constructions that took numeric
/// circle membership as a fact (the antipode of `k` "on" the circle, the pole
/// of chord `kx`) asserted it anyway and "proved" this — found on IMO 2008 P6,
/// where `k` is on the circle only because that is the goal.
#[test]
fn aux_search_does_not_assume_numeric_circle_membership() {
    let problem = Problem::parse(
        "o@0.0_0.0 x@1.0_0.0 y@0.0_1.0 = cong o x o y; k@0.6_0.8 = ? cong o k o x",
    )
    .unwrap();
    assert!(!solve_problem(&problem).unwrap());
    let (found, _) = ddar::aux_search::solve_with_aux_opts(&problem, 2, 200_000, false, true);
    if let Some(p) = found {
        let used: Vec<&str> = p.constructions.iter().map(|c| c.desc.as_str()).collect();
        panic!("a free point was proved on a circle using {used:?}");
    }
    for c in ddar::aux_search::candidates(&problem, true) {
        let mentions_k = c.args.contains(&3);
        let circle_claim = c.preds.iter().any(|p| p.name == "cong" && p.points.contains(&0));
        assert!(
            !(mentions_k
                && circle_claim
                && matches!(c.kind, ddar::aux_search::Kind::Antipode | ddar::aux_search::Kind::PoleOfChord)),
            "candidate `{}` treats k as on circle(o,x)",
            c.desc
        );
    }
}

#[test]
fn non_ascii_in_a_metric_goal_is_a_clean_error_not_a_panic() {
    for goal in ["dist(A,B) = é", "dist(A,B) = 2é", "dist(Aé,B) = 2", "dist(A,B) = 1 ∙ 2"] {
        let res = catch_unwind(AssertUnwindSafe(|| metric::solve("A = free\nB = free", goal, 8)));
        assert!(res.is_ok(), "metric prover panicked on {goal:?}");
        assert!(res.unwrap().is_err(), "{goal:?} must be rejected");
    }
}

/// False neighbours of the theorems the DDAR-certified facts (derived
/// midpoints, right angles, equal lengths, bisectors, collinear splits) now
/// prove. Each must be refuted, never proved.
#[test]
fn certified_facts_do_not_prove_false_neighbours() {
    let cases = [
        // British flag needs a rectangle; a parallelogram is not one.
        (
            "A B C = triangle\nD = parallelogram(A, B, C)\nP = free",
            "dist(P,A)^2 + dist(P,C)^2 = dist(P,B)^2 + dist(P,D)^2",
        ),
        // Wrong coefficient in the parallelogram law.
        (
            "A B C = triangle\nD = parallelogram(A, B, C)",
            "dist(A,C)^2 + dist(B,D)^2 = 2*dist(A,B)^2 + 3*dist(B,C)^2",
        ),
        // Carnot with one side term swapped.
        (
            "A B C = triangle\nP = free\nFa = foot(P, line(B, C))\nFb = foot(P, line(C, A))\n\
             Fc = foot(P, line(A, B))",
            "dist(B,Fa)^2 + dist(C,Fb)^2 + dist(A,Fc)^2 = dist(Fa,C)^2 + dist(Fb,A)^2 + dist(Fc,A)^2",
        ),
        // The incenter ratio with the wrong side.
        (
            "A B C = triangle\nI = incenter(A, B, C)\nX = meet(bisector(B, A, C), line(B, C))",
            "dist(A,I)*dist(B,C) = dist(I,X)*(dist(A,B) + dist(B,C))",
        ),
        // Leibniz with the wrong weight on PG² (Stewart with a wrong ratio).
        (
            "A B C = triangle\nG = centroid(A, B, C)\nP = free",
            "dist(P,A)^2 + dist(P,B)^2 + dist(P,C)^2 = dist(G,A)^2 + dist(G,B)^2 + \
             dist(G,C)^2 + 2*dist(P,G)^2",
        ),
    ];
    for (cons, goal) in cases {
        match metric::solve(cons, goal, 48) {
            Ok(proof) => panic!("a false statement was proved:\n{cons}\n{goal}\n{proof}"),
            Err(e) => assert!(e.is_refuted(), "{goal}: expected a refutation, got {e}"),
        }
    }
}

/// The closure buckets triangles and inscribed angles by 61-bit fingerprints
/// and acts on a match only after an exact comparison. False neighbours of
/// true statements in figures dense with similar triangles and circles must
/// stay unproved, by DDAR and by the auxiliary search.
#[test]
fn fingerprint_bucketing_does_not_prove_false_neighbours() {
    let orthic = "A B C = triangle\nFa = foot(A, line(B, C))\nFb = foot(B, line(C, A))\n\
                  Fc = foot(C, line(A, B))\nMa = midpoint(B, C)\n";
    let bisector = "A B C = triangle\nX = meet(bisector(B, A, C), line(B, C))\n";
    let excenter = "A B C = triangle\nI = incenter(A, B, C)\nIa = excenter(A, B, C)\n";
    let truths = [
        format!("{orthic}prove cyclic(Fa, Fb, Fc, Ma)"),
        format!("{bisector}prove eqratio(X, B, X, C, A, B, A, C)"),
    ];
    for src in &truths {
        assert!(ddar_claims_proof(src), "control case no longer proved:\n{src}");
    }
    let falsehoods = [
        format!("{orthic}prove cyclic(Fa, Fb, Fc, A)"),
        format!("{orthic}prove eqangle(Fa, Fb, Fa, Fc, A, B, A, C)"),
        format!("{bisector}prove eqratio(X, B, X, C, A, C, A, B)"),
        format!("{excenter}prove cyclic(A, I, C, Ia)"),
    ];
    for src in falsehoods {
        assert!(!ddar_claims_proof(&src), "DDAR proved a false statement:\n{src}");
        let problem = compile(&src).unwrap().problem;
        let found = bounded(300, move || {
            ddar::aux_search::solve_with_aux_opts(&problem, 1, 400, false, true).0
        });
        if let Some(p) = found {
            let used: Vec<String> = p.constructions.iter().map(|c| c.desc.clone()).collect();
            panic!("the aux search proved a false statement with {used:?}:\n{src}");
        }
    }
}

/// Found by `ddar --fuzz-false` (jgex `complete_016_ex-gao_gao_M_M021-64` with
/// `cong b c b a` dropped). AB = AC, D the midpoint of AB, E and F the second
/// points of the circle on diameter AB on AC and BC, X the centre of circle DEF
/// (on AF), Y the reflection of A in X. A is the external centre of similitude
/// of circles (X, XE) and (Y, YC), but C is the image of the OTHER point of
/// line AC on circle X (the midpoint of AC), so AE = EF is false. On this
/// figure BC = AB as well, E is that midpoint, line AC touches circle X, and
/// the image/antihomologous pairing read off the coordinates is a coincidence.
#[test]
fn similitude_pairing_is_not_read_off_a_tangent_coincidence() {
    let special = "a@0.873411653346327_0.22024962392785574 b@-0.3678402298414716_0.22639228009786294 = segment a b; \
        d@0.2527857117524277_0.22332095201285934 = midpoint d b a; \
        c@0.24746601546248825_-0.8516347113230487 = on_circle c a b; \
        e@0.5604388344044076_-0.3156925436975965 = on_line e a c, on_circle e d a; \
        f@-0.060187107189491634_-0.3126212156125929 = on_circle f d a, on_line f b c; \
        x = on_line x a f, on_circum x a d e; y = mirror y a x ? cong a e e f";
    let o = ddar::fuzz::solve_case("tangent-similitude", special, Duration::from_secs(60), false);
    assert!(o.parsed, "{}", o.detail);
    assert_eq!(o.goal_numeric, Some(true), "the special figure is equilateral");
    assert!(!o.proved, "DDAR proved a false statement:\n{special}\n{}", o.proof.unwrap_or_default());
}

/// Second fuzzer finding of the same rule (jgex `E061-63f` with the tangent
/// construction of `e` dropped, aux points p, q, r as the aux search found
/// them). On the original figure line DE touches the circle through A, C, D,
/// so DA = DE holds there, but nothing in the hypotheses says so.
#[test]
fn similitude_pairing_is_not_read_off_a_tangent_line() {
    let special = "a@-0.9675444639971735_-0.4702425265711835 b@0.17646760846163745_-0.21674859371974686 = segment a b; \
        c@-0.39553842776776804_-0.3434955601454652 = midpoint c b a; \
        d@-0.21930150243035063_0.215249681560101 = s_angle b a d 30, on_circle d c a; \
        e@0.7484736446910423_-0.09000162729402861 = on_line e a b; \
        p = on_line p d e, on_circum p a c d; q = reflect q a d e; \
        r = on_line r d e, angle_bisector r d c e ? cong d a d e";
    let o = ddar::fuzz::solve_case("tangent-line-similitude", special, Duration::from_secs(60), false);
    assert!(o.parsed, "{}", o.detail);
    assert_eq!(o.goal_numeric, Some(true), "DA = DE on the tangent figure");
    assert!(!o.proved, "DDAR proved a false statement:\n{special}\n{}", o.proof.unwrap_or_default());
}

/// The classical closure rules (squared lengths, Menelaus/Ceva converses,
/// bisector concurrency) on near-miss configurations: true controls stay
/// proved, false neighbours stay unproved by DDAR and by the aux search.
#[test]
fn classical_rules_do_not_prove_near_misses() {
    let truths = [
        "A B C = triangle\nH = point: perp(A, H, B, C), perp(B, H, C, A)\nprove perp(C, H, A, B)",
        "A B C = triangle\nX = meet(bisector(B, A, C), bisector(A, B, C))\nprove eqangle(C, B, C, X, C, X, C, A)",
        "A B C = triangle\nI = incenter(A, B, C)\nTa = foot(I, line(B, C))\nTb = foot(I, line(C, A))\n\
         Tc = foot(I, line(A, B))\nX = meet(line(A, Ta), line(B, Tb))\nprove coll(C, Tc, X)",
    ];
    for src in truths {
        assert!(ddar_claims_proof(src), "control case no longer proved:\n{src}");
    }
    let falsehoods = [
        "A B C = triangle\nH = point: perp(A, H, B, C)\nprove perp(C, H, A, B)",
        "A B C = triangle\nM = midpoint(B, C)\nprove perp(A, M, B, C)",
        "A B = segment\nC = on_tline(B, A, B)\nprove cong(A, C, B, C)",
        "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nF = midpoint(A, B)\nprove coll(D, E, F)",
        "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nX = meet(line(A, D), line(B, E))\n\
         F = on_line(A, B)\nprove coll(C, X, F)",
        "A B C = triangle\nX = meet(bisector(B, A, C), line(B, midpoint(C, A)))\n\
         prove eqangle(C, B, C, X, C, X, C, A)",
        "A B C = triangle\nG = centroid(A, B, C)\nMa = midpoint(B, C)\nMb = midpoint(C, A)\n\
         Mc = midpoint(A, B)\nP = midpoint(A, G)\nprove cyclic(Ma, Mb, Mc, P)",
    ];
    for src in falsehoods {
        let c = compile(src).unwrap();
        assert_eq!(c.goal_numerically_holds, Some(false), "near-miss must be false:\n{src}");
        assert!(!ddar_claims_proof(src), "DDAR proved a false statement:\n{src}");
        let problem = c.problem;
        let found = bounded(300, move || {
            ddar::aux_search::solve_with_aux_opts(&problem, 1, 400, false, true).0
        });
        if let Some(p) = found {
            let used: Vec<String> = p.constructions.iter().map(|c| c.desc.clone()).collect();
            panic!("the aux search proved a false statement with {used:?}:\n{src}");
        }
    }
}

/// The same false neighbours against the rollout search (several points at
/// once, coincidence-ranked) for a few seconds each.
#[test]
fn rollout_search_does_not_prove_false_neighbours() {
    let orthic = "A B C = triangle\nFa = foot(A, line(B, C))\nFb = foot(B, line(C, A))\n\
                  Fc = foot(C, line(A, B))\nMa = midpoint(B, C)\n";
    let excenter = "A B C = triangle\nI = incenter(A, B, C)\nIa = excenter(A, B, C)\n";
    let falsehoods = [
        format!("{orthic}prove cyclic(Fa, Fb, Fc, A)"),
        format!("{orthic}prove eqangle(Fa, Fb, Fa, Fc, A, B, A, C)"),
        format!("{excenter}prove cyclic(A, I, C, Ia)"),
    ];
    for src in falsehoods {
        let problem = compile(&src).unwrap().problem;
        let found = bounded(120, move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            ddar::aux_search::solve_max_until(&problem, false, Some(deadline)).0
        });
        if let Some(p) = found {
            let used: Vec<String> = p.constructions.iter().map(|c| c.desc.clone()).collect();
            panic!("the rollout search proved a false statement with {used:?}:\n{src}");
        }
    }
}

fn fmt_pt(name: &str, x: f64, y: f64) -> String {
    format!("{name}@{x:.17}_{y:.17}")
}

/// Each new auxiliary kind (virtual lines met with figure objects, harmonic
/// conjugates, intersections with circles the closure proved) next to a free
/// point `k` that only *happens* to satisfy the goal relation in the sampled
/// coordinates. The coincidence ranking puts exactly the candidates through `k`
/// first; none may assert the coincidence, so neither DDAR nor the rollout
/// search may prove the goal.
#[test]
fn new_aux_kinds_do_not_assume_numeric_coincidences() {
    use ddar::aux_search::{candidate_pool, solve_max_until, Kind, WarmBase};
    let free = format!(
        "{} {} {} = ",
        fmt_pt("d", -0.7, 0.9),
        fmt_pt("e", 1.6, -0.8),
        fmt_pt("g", 0.2, -1.1)
    );
    let (a, b, c) = ((0.0f64, 0.0f64), (1.3f64, 0.2f64), (0.4f64, 1.1f64));
    let base = format!(
        "{} {} {} = ; {free}",
        fmt_pt("a", a.0, a.1),
        fmt_pt("b", b.0, b.1),
        fmt_pt("c", c.0, c.1)
    );
    let (ux, uy) = (b.0 - a.0, b.1 - a.1);
    let para_k = (c.0 + 0.8 * ux, c.1 + 0.8 * uy);
    let perp_k = (c.0 - 0.7 * uy, c.1 + 0.7 * ux);
    let (tx, ty) = (0.6f64, 0.8f64);
    let tangent_k = (tx - 0.9 * ty, ty + 0.9 * tx);
    let th = |p: (f64, f64)| p.1.atan2(p.0);
    let q = (-0.5f64, 0.8f64);
    let iso = th(b) + th(c) - th(q);
    let iso_k = (1.1 * iso.cos(), 1.1 * iso.sin());
    let (ha, hb, hc) = (0.0f64, 2.0f64, 0.5f64);
    let hk = (2.0 * ha * hb - hc * (ha + hb)) / (ha + hb - 2.0 * hc);
    let cases: Vec<(Kind, String)> = vec![
        (Kind::ParaMeet, format!("{base}; {} = ? para c k a b", fmt_pt("k", para_k.0, para_k.1))),
        (Kind::PerpMeet, format!("{base}; {} = ? perp c k a b", fmt_pt("k", perp_k.0, perp_k.1))),
        (
            Kind::TangentMeet,
            format!(
                "o@0.0_0.0 x@1.0_0.0 = ; {} = cong o t o x; {free}; {} = ? perp o t t k",
                fmt_pt("t", tx, ty),
                fmt_pt("k", tangent_k.0, tangent_k.1)
            ),
        ),
        (
            Kind::IsogonalMeet,
            format!(
                "{base}; {} = ; {} = ? eqangle a b a k a q a c",
                fmt_pt("q", q.0, q.1),
                fmt_pt("k", iso_k.0, iso_k.1)
            ),
        ),
        (
            Kind::Harmonic,
            format!(
                "a@0.0_0.0 b@2.0_0.0 = ; c@0.5_0.0 = coll a b c; {free}; {} = coll a b k ? eqratio c a c b k a k b",
                fmt_pt("k", hk, 0.0)
            ),
        ),
        (
            Kind::CircleCircle,
            format!(
                "o@0.0_0.0 x@1.0_0.0 = ; y@0.0_1.0 = cong o y o x; z@-1.0_0.0 = cong o z o x; \
                 w@1.0_0.5 = ; {} = cong w u w o; {free}; k@0.6_-0.8 = ? cong o k o x",
                fmt_pt("u", 1.0, 0.5 + 1.25f64.sqrt())
            ),
        ),
    ];
    for (kind, src) in cases {
        let problem = Problem::parse(&src).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert!(!solve_problem(&problem).unwrap(), "DDAR proved a coincidence:\n{src}");
        let all = candidate_pool(&problem, 0.0);
        assert!(
            all.iter().any(|(_, c)| c.kind == kind),
            "no {kind:?} candidate generated for\n{src}"
        );
        let warm = WarmBase::new(&problem).unwrap();
        for (_, c) in all.iter().filter(|(_, c)| c.kind == kind) {
            assert!(!warm.check(c), "`{}` proves a numeric coincidence:\n{src}", c.desc);
        }
        let found = bounded(120, move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(4);
            solve_max_until(&problem, false, Some(deadline)).0
        });
        if let Some(p) = found {
            let used: Vec<String> = p.constructions.iter().map(|c| c.desc.clone()).collect();
            panic!("{kind:?}: the aux search proved a numeric coincidence with {used:?}:\n{src}");
        }
    }
}

/// The rollout search adds up to four points at once; a free point that
/// merely sits on a circle must stay off it whatever combination is tried.
#[test]
fn rollouts_do_not_assume_numeric_circle_membership() {
    let problem = Problem::parse(
        "o@0.0_0.0 x@1.0_0.0 y@0.0_1.0 = cong o x o y; k@0.6_0.8 = ? cong o k o x",
    )
    .unwrap();
    let found = bounded(120, move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(6);
        ddar::aux_search::solve_max_until(&problem, false, Some(deadline)).0
    });
    if let Some(p) = found {
        let used: Vec<&str> = p.constructions.iter().map(|c| c.desc.as_str()).collect();
        panic!("a free point was proved on a circle using {used:?}");
    }
}

const TRAPEZOID: &str = "A B C = triangle\nD = reflect(C, perp_bisector(A, B))";
const CYCLIC_QUAD: &str = "A B C = triangle\nO = circumcenter(A, B, C)\nD = on_circle(O, A)";

/// Neither prover may return a proof of `goal`; checked directly, because the
/// `checked_proof` backstop in `metric::solve` would hide an unsound derivation.
fn provers_reject(cons: &str, goal: &str) {
    if let Ok(Outcome::Proved(p)) = prove_euclidean(cons, goal) {
        panic!("additive prover proved a false statement:\n{goal}\n{p}");
    }
    if let Ok(Outcome::Proved(p)) = ddar::ratio::prove_ratio(cons, goal) {
        panic!("ratio prover proved a false statement:\n{goal}\n{p}");
    }
    match metric::check_numerically(cons, goal, 48) {
        Ok(ev) => panic!("{goal}: expected a false statement, it holds numerically: {}", ev.report),
        Err(e) => assert!(e.is_refuted(), "{goal}: expected a refutation, got {e}"),
    }
}

const SYMMEDIAN: &str = "A B C = triangle\nO = circumcenter(A, B, C)\n\
     T = meet(perp_line(B, line(O, B)), perp_line(C, line(O, C)))\n\
     X = meet(line(A, T), line(B, C))";

/// TRIG_PLAN §3, items 1–3: false neighbours of the symmedian ratio.
#[test]
fn symmedian_false_neighbours_are_never_proved() {
    provers_reject(SYMMEDIAN, "dist(B,X)*dist(A,C) = dist(X,C)*dist(A,B)");
    provers_reject(SYMMEDIAN, "dist(B,X)*dist(A,B)^2 = dist(X,C)*dist(A,C)^2");
    provers_reject(
        "A B C = triangle\nX = midpoint(B, C)",
        "dist(B,X)*dist(A,C)^2 = dist(X,C)*dist(A,B)^2",
    );
}

const EULER: &str = "A B C = triangle\nO = circumcenter(A, B, C)\nI = incenter(A, B, C)\n\
     T = foot(I, line(B, C))";

/// TRIG_PLAN §3, items 4–6: false neighbours of Euler's OI² = R² − 2Rr.
#[test]
fn euler_false_neighbours_are_never_proved() {
    provers_reject(EULER, "dist(O,I)^2 = dist(O,A)^2 + 2*dist(O,A)*dist(I,T)");
    provers_reject(EULER, "dist(O,I)^2 = dist(O,A)^2 - dist(O,A)*dist(I,T)");
    provers_reject(
        &format!("{EULER}\nN = meet(line(B, I), circumcircle(A, B, C))"),
        "dist(I,B)*dist(N,C) = 3*dist(O,A)*dist(I,T)",
    );
}

/// TRIG_PLAN §3, item 9: power of a point outside the circle with the sign of
/// the inside case. The correct sign is proved.
#[test]
fn power_of_a_point_sign_is_read_from_the_configuration() {
    let cons = "O = free\nA = free\nP = point: dist(O,P) = 2*dist(O,A)\nX = on_circle(O, A)\n\
                Y = meet(line(P, X), circle(O, A))";
    provers_reject(cons, "dist(P,X)*dist(P,Y) = dist(O,A)^2 - dist(O,P)^2");
    let proof = metric::solve(cons, "dist(P,X)*dist(P,Y) = dist(O,P)^2 - dist(O,A)^2", 48)
        .expect("the outside power of a point is proved");
    assert!(proof.starts_with("EUCLIDEAN PROOF"), "{proof}");
}

/// TRIG_PLAN §3, item 16 and P5: goals stated with `sin(angle(..))`. The law
/// of sines never proves its own restatement, the extended law of sines is
/// not a lone citation, and wrong constants or angles stay unproved.
#[test]
fn sine_goals_are_not_lone_citations_and_false_ones_fail() {
    let tri = "A B C = triangle";
    let restated = "dist(B,C)*sin(angle(A,B,C)) = dist(A,C)*sin(angle(B,A,C))";
    match metric::solve(tri, restated, 48) {
        Err(e @ metric::MetricError::NoProof { .. }) => assert_eq!(e.numerically_holds(), Some(true)),
        other => panic!("the law of sines must not prove its own restatement: {other:?}"),
    }
    let circ = "A B C = triangle\nO = circumcenter(A, B, C)";
    let proof = metric::solve(circ, "dist(B,C) = 2*dist(O,A)*sin(angle(B,A,C))", 48)
        .expect("BC = 2R sin A is derived");
    assert!(proof.contains("Law of sines in") && proof.contains("Extended law of sines"), "{proof}");
    provers_reject(circ, "dist(B,C) = dist(O,A)*sin(angle(B,A,C))");
    provers_reject(circ, "dist(B,C) = 2*dist(O,A)*sin(angle(A,B,C))");
    provers_reject(
        "A = free\nB = free\nC = point: angle(B, A, C) = 30",
        "dist(B,C) = dist(A,B)*sin(angle(B,A,C))",
    );
}

/// TRIG_PLAN §3, item 17 and T7: an equal-or-supplementary angle pair has
/// equal sines but cosines of opposite sign; the law of cosines does not prove
/// its own restatement, even spread over multiples of `sin² + cos² = 1`.
#[test]
fn cosine_sign_trap_and_restatement() {
    let supp = "A B C = triangle\nD = reflect(C, B)";
    provers_reject(supp, "cos(angle(A,B,C)) = cos(angle(A,B,D))");
    for goal in [
        "sin(angle(A,B,C)) = sin(angle(A,B,D))",
        "cos(angle(A,B,C)) = -cos(angle(A,B,D))",
    ] {
        let proof = metric::solve(supp, goal, 48).unwrap_or_else(|e| panic!("{goal}: {e}"));
        assert!(proof.starts_with("EUCLIDEAN PROOF"), "{proof}");
    }
    let restated = "dist(B,C)^2 = dist(A,B)^2 + dist(A,C)^2 - 2*dist(A,B)*dist(A,C)*cos(angle(B,A,C))";
    match metric::solve("A B C = triangle", restated, 48) {
        Err(e @ metric::MetricError::NoProof { .. }) => assert_eq!(e.numerically_holds(), Some(true)),
        other => panic!("the law of cosines must not prove its own restatement: {other:?}"),
    }
    provers_reject(
        "A B C = triangle",
        "dist(B,C)^2 = dist(A,B)^2 + dist(A,C)^2 + 2*dist(A,B)*dist(A,C)*cos(angle(B,A,C))",
    );
    let right = "A = free\nB = free\nC = point: perp(A,B,A,C)\nH = foot(A, line(B,C))";
    let proof = metric::solve(right, "dist(A,B)*cos(angle(A,B,C)) = dist(B,H)", 48).expect("projection is proved");
    assert!(proof.contains("Law of cosines"), "{proof}");
    provers_reject(right, "dist(A,B)*cos(angle(A,B,C)) = dist(C,H)");
}

/// TRIG_PLAN §3, items 7–8: false neighbours of Ptolemy's second theorem.
#[test]
fn ptolemy_second_false_neighbours_are_never_proved() {
    provers_reject(
        TRAPEZOID,
        "dist(A,C)*(dist(A,B)*dist(B,C) + dist(C,D)*dist(D,A)) = \
         dist(B,D)*(dist(A,B)*dist(A,D) + 2*dist(B,C)*dist(C,D))",
    );
    // In the isosceles trapezoid the diagonals are equal, so the swap is only
    // false on a general cyclic quadrilateral.
    provers_reject(
        CYCLIC_QUAD,
        "dist(B,D)*(dist(A,B)*dist(B,C) + dist(C,D)*dist(D,A)) = \
         dist(A,C)*(dist(A,B)*dist(A,D) + dist(B,C)*dist(C,D))",
    );
    // The convex general case the sine-area stage proves: its neighbours.
    let convex = "A B C = triangle\nM = midpoint(A, C)\nD = meet(line(B, M), circumcircle(A, B, C))";
    provers_reject(
        convex,
        "dist(B,D)*(dist(A,B)*dist(B,C) + dist(C,D)*dist(D,A)) = \
         dist(A,C)*(dist(A,B)*dist(A,D) + dist(B,C)*dist(C,D))",
    );
    provers_reject(
        convex,
        "dist(A,C)*(dist(A,B)*dist(B,C) + dist(C,D)*dist(D,A)) = \
         dist(B,D)*(dist(A,B)*dist(A,D) + 2*dist(B,C)*dist(C,D))",
    );
    // Ptolemy's first theorem with a minus sign.
    provers_reject(
        convex,
        "dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D) - dist(A,D)*dist(B,C)",
    );
}

/// The DDAR closure with the law-of-sines rows switched on for this figure
/// (independent of `GEO_TRIG`), and whether it proves the goal.
fn ddar_trig_proves(src: &str, trig: bool) -> bool {
    let c = compile(src).expect("a DDAR goal");
    let mut d = ddar::Ddar::new(&c.problem.points);
    for p in &c.problem.preds {
        d.force_pred(p);
    }
    if trig {
        d.enable_trig();
    }
    d.deduction_closure();
    let (_, _, _, rejected) = d.trig_stats();
    assert_eq!(rejected, 0, "a trig row failed the residual guard");
    d.check_pred(c.problem.goal.as_ref().unwrap())
}

/// TRIG_PLAN P7 and §3 items 11–12 on the DDAR side: the known-sine rows need
/// the tabulated angle, and the law of sines needs real triangles.
#[test]
fn ddar_trig_rows_prove_only_what_follows() {
    let right30 = "A = free\nB = free\nC = point: angle(B,A,C) = 30, perp(C,A,C,B)\n\
                   prove dist(B,C) = dist(A,B) / 2";
    assert!(!ddar_trig_proves(right30, false), "plain DDAR was not expected to prove it");
    assert!(ddar_trig_proves(right30, true));
    for false_goal in [
        "A = free\nB = free\nC = point: angle(B,A,C) = 30\nprove dist(B,C) = dist(A,B) / 2",
        "A = free\nB = free\nC = point: angle(B,A,C) = 72, perp(C,A,C,B)\nprove dist(B,C) = dist(A,B) / 2",
        "A = free\nB = free\nC = point: angle(B,A,C) = 30, perp(C,A,C,B)\nprove dist(A,C) = dist(A,B) / 2",
        "A B C = triangle\nO = circumcenter(A, B, C)\nprove dist(B,C) = dist(O,A)",
    ] {
        assert!(!ddar_trig_proves(false_goal, true), "trig DDAR proved a false goal:\n{false_goal}");
    }
}

/// Fuzz case jgex E051-26~hs2: the tangent at B was dropped, but the figure
/// is the original one, where E is the midpoint of FG and E F H N is a
/// rectangle. There the chords HE and NF are diameters, so the side of the
/// centre (the arc-chord transfer's configuration branch) is rounding noise;
/// reading it paired the arcs as a rectangle's, which is false for the
/// isosceles trapezoid of a generic figure, and ended in an SSA "similarity"
/// proving GE = EF. Chords through the centre are no longer transferred.
#[test]
fn arc_chord_transfer_reads_no_side_off_a_diameter() {
    let low = "a@-0.52555007828554_0.24644985287467536 b@0.5787628638709572_0.6713292845322588 c@-1.6877412770044946_0.024321009002004373 e@0.3180434353940881_0.5969029357011043 d@-4.170870672315426_13.01620030840955 f@-3.5995943484790898_10.027248923150959 g@4.235681219267257_-8.83344305174873 n@105.73780307188756_44.39131239516121 h@101.82016528801438_53.82165838261104 = coll e b c, cong a c a b, perp d c c a, perp f e a e, coll f c d, coll g e f, coll g b d, cyclic d f g n, cong n f n g, para e g n h, para g n e h, cong e g n h, cong g n e h ? cong g e e f";
    let problem = Problem::parse(low).unwrap();
    assert!(!solve_problem(&problem).unwrap(), "proved GE = EF without the tangent at B");
}

/// jgex `complete_010_Other_gao_Y_yL182-1` (corpus figure): F = C + A − E is
/// pinned to line AC only by FC = AE and FA = CE, so `|AF| + |FC| = |AC|`, the
/// equality case of the triangle inequality, is what puts F on AC. The rule
/// fires on the problem; with any one of its three premises dropped (the
/// figure kept, so F still lies on AC numerically) nothing is proved.
#[test]
fn triangle_equality_needs_all_its_premises() {
    let pts = "a@0.1343253817924288_0.719445251994107 c@-0.5210702983270747_-0.8349066943186165 \
               d@0.13033572228610057_-0.8094323774776992 b@-0.5170806388207465_0.6939709351531897 \
               e@0.11522266674292991_0.6741407997102106 f@-0.5019675832775758_-0.7896022420347201";
    let full = "para b c d a, para b a d c, coll e a c, cong f c a e, cong f a c e";
    for goal in ["para d e f b", "coll f a c"] {
        let p = Problem::parse(&format!("{pts} = {full} ? {goal}")).unwrap();
        assert!(
            solve_problem(&p).unwrap(),
            "the rule no longer proves {goal}"
        );
    }
    for hyps in [
        "para b c d a, para b a d c, cong f c a e, cong f a c e",
        "para b c d a, para b a d c, coll e a c, cong f c a e",
        "para b c d a, para b a d c, coll e a c, cong f a c e",
    ] {
        for goal in ["para d e f b", "coll f a c"] {
            let src = format!("{pts} = {hyps} ? {goal}");
            let p = Problem::parse(&src).unwrap();
            assert!(
                !solve_problem(&p).unwrap(),
                "proved without a premise:\n{src}"
            );
        }
    }
}

/// Two circles through a numerically identical pair x, k are tangent there.
/// The merge x = k is a theorem only when the tangency is proved: k on the
/// line of centres (circle–circle) or the line ⟂ the radius (line–circle).
/// Each control is proved; each decoy, where the same figure has the
/// tangency only numerically, is not — by DDAR or by the aux search.
#[test]
fn tangent_merge_needs_a_proved_tangency() {
    let proved = [
        // External tangency at k, k on the line of centres by hypothesis.
        "o@0.0_0.0 p@3.0_0.0 = ; k@1.0_0.0 = coll o p k; x@1.0_0.0 = cong o x o k, cong p x p k ? coll x o p",
        // Tangent line at t: tq ⟂ ot by hypothesis.
        "o@0.0_0.0 a@1.0_0.0 = ; t@0.6_0.8 = cong o t o a; q@0.2_1.1 = perp o t t q; \
         x@0.6_0.8 = cong o x o a, coll x t q ? perp o x x q",
    ];
    for src in proved {
        let p = Problem::parse(src).unwrap();
        assert!(
            solve_problem(&p).unwrap(),
            "tangent merge no longer fires:\n{src}"
        );
    }
    let decoys = [
        // k is on circle o and on line op, but b is free: circle p touches
        // circle o at k only in this figure.
        "o@0.0_0.0 p@3.0_0.0 a@0.6_0.8 = ; k@1.0_0.0 = coll o p k, cong o k o a; b@4.2_1.6 = ; \
         x@1.0_0.0 = cong o x o a, cong p x p b ? coll x o p",
        // The same with x off the line of centres symbolically and k on both
        // circles but not on the line.
        "o@0.0_0.0 p@3.0_0.0 a@0.6_0.8 b@4.2_1.6 = ; k@1.0_0.0 = cong o k o a, cong p k p b; \
         x@1.0_0.0 = cong o x o a, cong p x p b ? coll x o p",
        // q free: tq touches the circle only in this figure.
        "o@0.0_0.0 a@1.0_0.0 = ; t@0.6_0.8 = cong o t o a; q@0.2_1.1 = ; \
         x@0.6_0.8 = cong o x o a, coll x t q ? perp o x x q",
    ];
    for src in decoys {
        let p = Problem::parse(src).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert!(
            !solve_problem(&p).unwrap(),
            "DDAR merged without a proved tangency:\n{src}"
        );
        let found = bounded(120, move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(4);
            ddar::aux_search::solve_max_until(&p, false, Some(deadline)).0
        });
        if let Some(found) = found {
            let used: Vec<String> = found.constructions.iter().map(|c| c.desc.clone()).collect();
            panic!("the aux search proved a decoy with {used:?}:\n{src}");
        }
    }
}

/// IMO 2011 P6 in AG1's form: the goal point x is the meeting point of two
/// tangent circles (a double point). The proof needs the aux point K = the
/// second construction of x (circle(pc, pb, a1) ∩ ω) and the tangent merge.
#[test]
fn double_point_candidate_proves_imo_2011_p6() {
    let text = "a b c = triangle a b c; o = circle o a b c; p = on_circle p o a; q = on_tline q p o p; \
        pa = reflect pa p b c; pb = reflect pb p c a; pc = reflect pc p a b; qa = reflect qa q b c; \
        qb = reflect qb q c a; qc = reflect qc q a b; a1 = on_line a1 pb qb, on_line a1 pc qc; \
        b1 = on_line b1 pa qa, on_line b1 pc qc; c1 = on_line c1 pa qa, on_line c1 pb qb; \
        o1 = circle o1 a1 b1 c1; x = on_circle x o a, on_circle x o1 a1 ? coll x o o1";
    let o = bounded(200, move || {
        ddar::fuzz::solve_case("imo_2011_p6", text, Duration::from_secs(120), true)
    });
    assert!(o.proved, "{} {}", o.status, o.detail);
    assert!(o.proof.unwrap_or_default().contains("proved tangent there"));
}

/// A double point must never stand in for a numeric coincidence: k is a
/// point of line pq that only happens to sit on circle o. The candidate
/// pq ∩ circle(o) lands on k and is kept as a double point (line pq holds k
/// symbolically), yet only one object holds both, so nothing may identify
/// them, and the goal stays unproved.
#[test]
fn double_points_do_not_assume_numeric_coincidences() {
    let src = "o@0.0_0.0 a@1.0_0.0 = ; b@-0.6_0.8 = cong o b o a; c@0.0_-1.0 = cong o c o a; \
               p@1.6_1.3 q@-0.4_0.3 = ; k@0.6_0.8 = coll p q k ? cong o k o a";
    let p = Problem::parse(src).unwrap();
    assert!(!solve_problem(&p).unwrap());
    let pool = ddar::aux_search::candidate_pool(&p, 1.0);
    assert!(
        pool.iter()
            .any(|(_, c)| (c.coord.x - 0.6).abs() < 1e-12 && (c.coord.y - 0.8).abs() < 1e-12),
        "no double point on k was generated"
    );
    let found = bounded(120, move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(6);
        ddar::aux_search::solve_max_until(&p, false, Some(deadline)).0
    });
    if let Some(found) = found {
        let used: Vec<String> = found.constructions.iter().map(|c| c.desc.clone()).collect();
        panic!("a double point proved a coincidence with {used:?}:\n{src}");
    }
}

/// A low-level problem through the DDAR closure with the law-of-sines rows on
/// (independent of `GEO_TRIG`).
fn ddar_trig_proves_low(src: &str) -> bool {
    let p = Problem::parse(src).unwrap_or_else(|e| panic!("{src}: {e}"));
    let mut d = ddar::Ddar::new(&p.points);
    for pred in &p.preds {
        d.force_pred(pred);
    }
    d.enable_trig();
    d.deduction_closure();
    let (_, _, _, rejected) = d.trig_stats();
    assert_eq!(rejected, 0, "a trig row failed the residual guard:\n{src}");
    d.check_pred(p.goal.as_ref().unwrap())
}

/// The double-angle row `sin x · sin(x + 90°) = sin 2x / 2`: an isosceles
/// triangle OAB with apex angle 2x has AB/OA = 2 sin x, a right triangle PQR
/// with angle x at P has QR/PQ = sin x, so AB·PQ = 2·OA·QR. Plain DDAR cannot
/// get it; with the row it does. Every neighbour with a hypothesis dropped (on
/// the same figure, where the goal still holds numerically) or with the wrong
/// constant stays unproved.
#[test]
fn double_angle_row_fires_and_needs_its_premises() {
    let pts = |rest: &str| {
        format!(
            "o@0.3_2.0 a@1.3_2.0 = ; b@0.6623577544766737_1.0679609140327737 = {}; p@0_0 r@1_0 = ; \
             q@1_0.6841368083416923 = {} ? {}",
            if rest.contains("NO_ISO") { "" } else { "cong o a o b" },
            match (rest.contains("NO_PERP"), rest.contains("NO_ANG")) {
                (false, false) => "perp r p r q, angeq p q p r o a o b 2 -2 -1 1 0",
                (true, false) => "angeq p q p r o a o b 2 -2 -1 1 0",
                (false, true) => "perp r p r q",
                (true, true) => "",
            },
            if rest.contains("WRONG") {
                "distmeq a b p q o a q r 1 1 -1 -1 1"
            } else {
                "distmeq a b p q o a q r 1 1 -1 -1 1/2"
            }
        )
    };
    assert!(ddar_trig_proves_low(&pts("")), "the double-angle row no longer fires");
    assert!(!ddar_claims_low(&pts("")), "plain DDAR proves it: the test does not exercise the row");
    for tag in ["NO_ISO", "NO_PERP", "NO_ANG", "WRONG"] {
        assert!(!ddar_trig_proves_low(&pts(tag)), "proved a false neighbour ({tag}):\n{}", pts(tag));
    }
}

fn ddar_claims_low(src: &str) -> bool {
    let p = Problem::parse(src).unwrap();
    let mut d = ddar::Ddar::new(&p.points);
    for pred in &p.preds {
        d.force_pred(pred);
    }
    d.deduction_closure();
    d.check_pred(p.goal.as_ref().unwrap())
}

/// Morley's trisector theorem by the triple-angle rows and the converse of
/// the law of sines, on all 27 figures of the same hypotheses: each trisector
/// is fixed only up to π/3, and on the 9 figures whose choices sum to 2 mod 3
/// JKL is not equilateral (JL = 1.998, JK = 2.316 for choice 0 0 2). The 18
/// true figures are proved, the 9 false ones are not; and on the true corpus
/// figure, dropping one trisection hypothesis (the goal still holds there)
/// proves nothing.
#[test]
fn morley_by_trig_rows_only_where_true() {
    use std::f64::consts::PI;
    type C = (f64, f64);
    let (a, b, c): (C, C, C) = (
        (0.1343253817924288, 0.719445251994107),
        (-0.5210702983270747, -0.8349066943186165),
        (0.13033572228610057, -0.8094323774776992),
    );
    let ang = |p: C, q: C| (q.1 - p.1).atan2(q.0 - p.0);
    let meet = |p: C, t1: f64, q: C, t2: f64| -> C {
        let (d1, d2) = ((t1.cos(), t1.sin()), (t2.cos(), t2.sin()));
        let r = (q.0 - p.0, q.1 - p.1);
        let den = d1.0 * (-d2.1) - d1.1 * (-d2.0);
        let s = (r.0 * (-d2.1) - r.1 * (-d2.0)) / den;
        (p.0 + s * d1.0, p.1 + s * d1.1)
    };
    let third = |v: C, x: C, y: C, k: f64| {
        let (u, w) = (ang(v, x), ang(v, y));
        (u, (w - u).rem_euclid(PI) / 3.0 + k * PI / 3.0)
    };
    let preds = "coll d b c, coll e b c, eqangle a b a d a d a e, eqangle a d a e a e a c, coll f c a, \
                 coll g c a, eqangle b c b f b f b g, eqangle b f b g b g b a, coll h a b, coll i a b, \
                 eqangle c a c h c h c i, eqangle c h c i c i c b, coll j b f, coll j c i, coll k a e, \
                 coll k c h, coll l a d, coll l b g";
    let dist = |p: C, q: C| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
    let (mut proved, mut refused) = (0, 0);
    for ia in 0..3 {
        for ib in 0..3 {
            for ic in 0..3 {
                let (ua, ta) = third(a, b, c, ia as f64);
                let (ub, tb) = third(b, c, a, ib as f64);
                let (uc, tc) = third(c, a, b, ic as f64);
                let (bc, ca, ab) = (ang(b, c), ang(c, a), ang(a, b));
                let pts: Vec<(&str, C)> = vec![
                    ("a", a),
                    ("b", b),
                    ("c", c),
                    ("d", meet(a, ua + ta, b, bc)),
                    ("e", meet(a, ua + 2.0 * ta, b, bc)),
                    ("f", meet(b, ub + tb, c, ca)),
                    ("g", meet(b, ub + 2.0 * tb, c, ca)),
                    ("h", meet(c, uc + tc, a, ab)),
                    ("i", meet(c, uc + 2.0 * tc, a, ab)),
                    ("j", meet(b, ub + tb, c, uc + 2.0 * tc)),
                    ("k", meet(a, ua + 2.0 * ta, c, uc + tc)),
                    ("l", meet(a, ua + ta, b, ub + 2.0 * tb)),
                ];
                let get = |n: &str| pts.iter().find(|p| p.0 == n).unwrap().1;
                let truth = (dist(get("j"), get("l")) - dist(get("j"), get("k"))).abs() < 1e-9;
                let head: Vec<String> = pts.iter().map(|(n, p)| fmt_pt(n, p.0, p.1)).collect();
                let src = format!("{} = {preds} ? cong j l j k", head.join(" "));
                let got = bounded(120, move || ddar_trig_proves_low(&src));
                assert_eq!((ia + ib + ic) % 3 != 2, truth, "figure {ia} {ib} {ic}: unexpected truth value");
                if truth {
                    assert!(got, "true Morley figure {ia} {ib} {ic} not proved");
                    proved += 1;
                } else {
                    assert!(!got, "false Morley figure {ia} {ib} {ic} proved");
                    refused += 1;
                }
            }
        }
    }
    assert_eq!((proved, refused), (18, 9));
    let fig = "a@0.1343253817924288_0.719445251994107 b@-0.5210702983270747_-0.8349066943186165 \
               c@0.13033572228610057_-0.8094323774776992 d@-0.28816543221817836_-0.825798561809395 \
               e@-0.07403070417618249_-0.8174244667961947 f@0.13102232498787159_-0.5463193199830535 \
               g@0.13200375239858442_-0.1702264702525801 h@-0.24396157915667044_-0.17770908065078395 \
               i@-0.39075010291486945_-0.525836226619042 j@-0.13598988595056216_-0.6644871233213832 \
               k@-0.03509469205440549_-0.5302258581151602 l@-0.20181595541531305_-0.5099786896844436";
    assert!(ddar_trig_proves_low(&format!("{fig} = {preds} ? cong j l j k")));
    for dropped in ["eqangle a d a e a e a c, ", "eqangle b f b g b g b a, ", "eqangle c h c i c i c b, "] {
        let src = format!("{fig} = {} ? cong j l j k", preds.replace(dropped, ""));
        assert!(!ddar_trig_proves_low(&src), "proved Morley without `{dropped}`");
    }
}

/// Fuzzer finding on wf/final-hard-r1 (jgex GDD_FULL_81-109_100~hs1: D made free, the figure kept,
/// so F is only numerically the point with CF ⟂ CA). The aux search proved it with two double
/// points, x1 = circle(A,C,E) ∩ circle(B,E,F) on F and the spiral centre z on B, never merged with
/// F and B: the circles (x1 f e b) and (x1 f z e) share three symbolic points, are one circle for a
/// generic F, and the radical-axis rule treated them as two. The DDAR part was a pre-existing hole
/// (wf/final proves the augmented problem too). Now the radical axis skips circle pairs sharing
/// three points, and the aux search accepts no proof that leaves a double point unmerged.
#[test]
fn double_points_and_coinciding_circles_prove_nothing_false() {
    let aug = "a@-0.23981029018905176_-0.39281463739256384 c@0.8811080127867896_0.17516087337911768 \
        b@0.8125300823545871_-1.0795706079507439 e@0.7439521519223846_-2.3343020892806052 \
        d@0.6753742214901821_-3.5890335706104666 f@1.864870454898226_-1.7663265785089237 = \
        cong b c c a, cong c a a b, eqangle c b c a a c a b, eqangle b a b c c b c a, coll e c b, \
        cong b c b e, coll f a b; x@-0.37696615105345677_-2.9022776000522867 = para a x c b, coll e f x; \
        x1@1.864870454898226_-1.7663265785089237 = cyclic a c e x1, cyclic b e f x1; \
        z@0.812530082354587_-1.0795706079507439 = eqangle z e z c z f z a, eqratio z e z c z f z a \
        ? perp a c c f";
    let p = Problem::parse(aug).unwrap();
    assert!(!solve_problem(&p).unwrap(), "radical axis of two circle objects that may be one circle");
    let case = "a@-0.23981029018905176_-0.39281463739256384 c@0.8811080127867896_0.17516087337911768 = \
        segment a c; b@0.8125300823545871_-1.0795706079507439 = eq_triangle b c a; \
        e@0.7439521519223846_-2.3343020892806052 = mirror e c b; d@0.6753742214901821_-3.5890335706104666 = \
        free d; f@1.864870454898226_-1.7663265785089237 = foot f d a b ? perp a c c f";
    let o = bounded(200, move || ddar::fuzz::solve_case("hs1", case, Duration::from_secs(20), true));
    assert!(o.parsed, "{}", o.detail);
    assert_eq!(o.goal_numeric, Some(true), "the goal holds on the pinned figure");
    assert!(!o.proved, "the aux search proved a false statement:\n{case}\n{}", o.proof.unwrap_or_default());
}

/// Verification finding on wf/final-hard-r1: two circle objects through the same two points
/// U, V can be one circle whose identity the closure has not proved. Here the circles (a1 a2 b1)
/// and (a1 a2 b2) of IMO 2008 P1 are one circle by that theorem, so the eqratio is automatic
/// power of a point and x is free off line a1a2 (generic figures put it 0.25-1.09 away); the
/// pinned figure has x on the line. The radical-axis rule proved `coll x a1 a2`, its only
/// support being the numeric guard that x lies on UV. The rule now needs numerically distinct
/// circles. Control: two genuinely distinct circles through a1, a2 still give the radical axis.
#[test]
fn radical_axis_needs_two_distinct_circles() {
    let control = "a1@-1.0_0.0 a2@1.0_0.0 = ; b1@-0.5885205001836284_2.285940753247836 \
        b2@1.7102391228277416_-3.4405145409711757 = ; p1@1.4141768296295327_1.0101928670629128 = \
        cyclic a1 a2 b1 p1; q1@2.2357325516682938_-2.0387292836337005 = cyclic a1 a2 b2 q1; \
        x@3.0_0.0 = coll x b1 p1, coll x b2 q1, eqratio x b1 x b2 x q1 x p1 ? coll x a1 a2";
    assert!(solve_problem(&Problem::parse(control).unwrap()).unwrap(), "radical axis no longer fires");
    let decoy = "a@-0.4192717273280908_0.24372487354225303 b@-0.8479310819447494_0.047647597687965115 \
        c@-0.06030967450309488_-0.1487507397252532 h@-0.355965288429601_0.49760434707588064 \
        d@-0.45412037822392215_-0.05055157101864405 e@-0.23979070091559285_0.04748706690849991 \
        f@-0.6336014046364201_0.14568623561510907 a1@0.08620909112185093_-0.1852861128076666 \
        a2@-0.9944498475696952_0.0841829707703785 b1@0.07394834727633426_-0.2955434132663001 \
        b2@-0.55352974910752_0.39051754708329994 c1@-0.22597205855549413_0.3321439691907351 \
        c2@-1.0412307507173462_-0.040771497960516934 = perp h a b c, perp h b c a, perp h c a b, \
        coll d b c, cong d b d c, coll e a c, cong e a e c, coll f a b, cong f a f b, cong d a1 d h, \
        coll a1 b c, cong d a2 d h, coll a2 b c, cong e b1 e h, coll b1 c a, cong e b2 e h, \
        coll b2 c a, cong f c1 f h, coll c1 a b, cong f c2 f h, coll c2 a b; \
        p1@-0.7238244857928878_0.3426593722409412 = cyclic a1 a2 b1 p1; \
        q1@0.05549540973243461_-0.3625664732957618 = cyclic a1 a2 b2 q1; \
        x@-0.13184458450827166_-0.13091306129335806 = coll x b1 p1, coll x b2 q1, \
        eqratio x b1 x b2 x q1 x p1 ? coll x a1 a2";
    let p = Problem::parse(decoy).unwrap();
    assert!(!solve_problem(&p).unwrap(), "radical axis of two circle objects that are one circle");
    let found = bounded(120, move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(4);
        ddar::aux_search::solve_max_until(&p, false, Some(deadline)).0
    });
    if let Some(found) = found {
        let used: Vec<String> = found.constructions.iter().map(|c| c.desc.clone()).collect();
        panic!("the aux search proved the decoy with {used:?}");
    }
}

/// The angle bisector theorem needs a triangle. With `a` on line bc (only the bisector relation
/// asserted, which a point of the line satisfies as 0 = 0) the rule forced |xb|/|xc| = |ab|/|ac|,
/// 1/2 = 5/2 on this figure, and release builds proved the false ratio. Now the rule needs `a`
/// clearly off the line, and every table refuses a row the figure contradicts. The generic
/// figure is still proved.
#[test]
fn bisector_theorem_needs_a_triangle() {
    let close = |src: &str| {
        let p = Problem::parse(src).unwrap();
        let mut d = ddar::Ddar::new(&p.points);
        for pred in &p.preds {
            d.force_pred(pred);
        }
        d.deduction_closure();
        let proved = d.check_pred(p.goal.as_ref().unwrap());
        (proved, d.rejected_rows())
    };
    let degenerate = "b@0.0_0.0 c@3.0_0.0 = ; x@1.0_0.0 = coll x b c; a@5.0_0.0 = eqangle a b a x a x a c \
                      ? eqratio x b x c a b a c";
    assert_eq!(close(degenerate), (false, 0), "proved a ratio false on its own figure");
    let generic = "b@0.0_0.0 c@3.0_0.0 = ; a@0.6_2.0 = ; x@1.2018400234482116_0.0 = coll x b c, \
                   eqangle a b a x a x a c ? eqratio x b x c a b a c";
    assert_eq!(close(generic), (true, 0), "the bisector theorem no longer fires");
}

/// A goal or hypothesis naming a direction or length of two numerically identical points has
/// no variable to state it with. It used to panic the engine (fuzz case
/// translated_imo_2002_p2a~d0, f pinned on e, goal `eqangle e c e j e j e f`; TM3, goal
/// `cong x k x k` with x on k); now it is simply not proved.
#[test]
fn degenerate_goals_are_unproved_not_panics() {
    let tm3 = "o@0.0_0.0 a@0.0_2.0 p@3.0_0.0 b@3.0_1.0 = ; k@2.0_0.0 = cong o k o a, cong p k p b; \
               x@2.0_0.0 = cong o x o a, cong p x p b ? cong x k x k";
    let p = Problem::parse(tm3).unwrap();
    let r = catch_unwind(AssertUnwindSafe(|| ddar::runner::solve_problem_with_proof(&p)));
    assert!(matches!(r, Ok(Ok(None))), "a degenerate goal panicked or was proved");
    let case = "b@0.14236045802074582_-0.47028866105807154 c@-1.4194196564654133_-1.3243862501986787 = \
        segment b c; o@-0.6385295992223338_-0.8973374556283751 = midpoint o b c; \
        a@-1.5275828092207768_-0.9391028029454259 = on_circle a o b; \
        d@-0.39796465619885596_-1.7542437881018609 = on_circle d o b, on_bline d a b; \
        e@-1.1192260559960014_-0.1482774641121476 = on_bline e o a, on_circle e o b; \
        f@-1.1192260559960014_-0.1482774641121476 = on_bline f o a, on_circle f o b; \
        j = on_pline j o a d, on_line j a c ? eqangle e c e j e j e f";
    let o = bounded(60, move || ddar::fuzz::solve_case("d0", case, Duration::from_secs(3), true));
    assert!(o.parsed, "{}", o.detail);
    assert_ne!(o.status, "engine-panic", "{}", o.detail);
    assert!(!o.proved);
}

/// A point 9e-16 from another (not bit-identical) is one point to the pair tables but not to the
/// arc-chord transfer's side test, which then asked for the missing pair variable and panicked.
/// IMO 2011 P6 with x1 = circle(pc, pb, a1) ∩ ω placed that close to x proves like the exact
/// double point; the same figure with a false goal does not.
#[test]
fn near_identical_points_do_not_panic_the_closure() {
    let base = "a@-0.8048578064433936_-0.6219504215719476 b@0.8684669896894479_-0.025585732571094866 \
        c@-0.22291632195609368_0.9274971408947663 o@-0.07994750791986127_-0.010205650197596394 \
        p@0.3037711317812146_-0.877665511952185 q@-0.6196887172454079_-1.286155602053097 \
        pa@1.6367329495163112_0.6487204125441275 pb@-1.807721808949386_-0.08463103555013651 \
        pc@-0.10784770243060504_0.277285501321081 qa@1.9171335048067668_1.618781119442272 \
        qb@-1.381491138113092_-1.0000378640768677 qc@-1.0815079204750302_0.009651373690455323 \
        a1@-1.7642407934507416_-0.17801431653421007 b1@1.670678408503232_0.7661565421078668 \
        c1@0.1866529922372926_-4.367908698478167 o1@0.4945758181370108_-1.6754003804699222 \
        x@-0.38931560147564687_0.8864648516585711 = cong o a o b, cong o b o c, cong o p o a, \
        perp q p o p, cong b p b pa, cong c p c pa, perp b c p pa, cong c p c pb, cong a p a pb, \
        perp c a p pb, cong a p a pc, cong b p b pc, perp a b p pc, cong b q b qa, cong c q c qa, \
        perp b c q qa, cong c q c qb, cong a q a qb, perp c a q qb, cong a q a qc, cong b q b qc, \
        perp a b q qc, coll a1 pb qb, coll a1 pc qc, coll b1 pa qa, coll b1 pc qc, coll c1 pa qa, \
        coll c1 pb qb, cong o1 a1 o1 b1, cong o1 b1 o1 c1, cong o x o a, cong o1 x o1 a1; \
        x1@-0.38931560147564775_0.8864648516585714 = cyclic pc pb a1 x1, cong o x1 o a";
    for (goal, truth) in [("coll x o o1", true), ("coll x o a", false)] {
        let p = Problem::parse(&format!("{base} ? {goal}")).unwrap();
        let r = catch_unwind(AssertUnwindSafe(|| solve_problem(&p)));
        assert_eq!(r.ok().and_then(|r| r.ok()), Some(truth), "{goal}");
    }
}

/// Every row is forced from raw pair quantities, so the elimination records the facts that
/// normalised it: `cong o x o a` reduced through `cong o t o a` used to be stored citing itself
/// alone, and the proof of `cong o t o x` (and of the tangent merge at t) printed one premise.
/// A different radius is still not proved.
#[test]
fn proof_steps_cite_the_rows_that_normalised_them() {
    let proof = |src: &str| ddar::runner::solve_problem_with_proof(&Problem::parse(src).unwrap()).unwrap();
    let radii = proof("o@0.0_0.0 a@1.0_0.0 = ; t@0.6_0.8 = cong o t o a; x@-0.6_0.8 = cong o x o a ? cong o t o x")
        .expect("equal radii");
    assert!(radii.contains("cong o t o a") && radii.contains("cong o x o a"), "{radii}");
    let tangent = proof(
        "o@0.0_0.0 a@1.0_0.0 = ; t@0.6_0.8 = cong o t o a; q@0.2_1.1 = perp o t t q; \
         x@0.6_0.8 = cong o x o a, coll x t q ? perp o x x q",
    )
    .expect("tangent merge");
    assert!(tangent.contains("cong o t o a"), "{tangent}");
    assert!(proof("o@0.0_0.0 a@1.0_0.0 b@2.0_0.0 = ; t@0.6_0.8 = cong o t o a; x@-1.2_1.6 = cong o x o b ? cong o t o x")
        .is_none());
}
