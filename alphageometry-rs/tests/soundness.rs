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
