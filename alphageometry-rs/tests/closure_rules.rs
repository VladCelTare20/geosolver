//! The classical closure rules (squared lengths, Menelaus/Ceva, bisector
//! concurrency, `rcompute`): each fires where DDAR alone cannot reach the
//! goal, and none proves a near-miss configuration whose goal is false.

use ddar::geo::compile;
use ddar::{Ddar, Problem};

fn close(p: &Problem, off: &[&str]) -> Ddar {
    let mut d = Ddar::new(&p.points);
    for r in off {
        assert!(d.set_rule(r, false), "unknown rule {r}");
    }
    for pred in &p.preds {
        d.force_pred(pred);
    }
    d.deduction_closure();
    d
}

fn proves(p: &Problem, off: &[&str]) -> bool {
    close(p, off).check_pred(p.goal.as_ref().unwrap())
}

fn low(src: &str) -> Problem {
    Problem::parse(src).unwrap()
}

fn geo(src: &str) -> Problem {
    let c = compile(src).unwrap();
    c.problem
}

fn geo_false(src: &str) -> Problem {
    let c = compile(src).unwrap();
    assert_eq!(c.goal_numerically_holds, Some(false), "near-miss goal must be false:\n{src}");
    c.problem
}

const ALL: &[&str] = &[];

/// Fires only with `rule`, and is otherwise out of DDAR's reach.
fn needs(rule: &str, p: &Problem) {
    assert!(proves(p, ALL), "not proved with every rule on");
    assert!(!proves(p, &[rule]), "proved without `{rule}`: the test does not exercise it");
}

fn never(p: &Problem) {
    assert!(!proves(p, ALL), "a false goal was proved");
}

#[test]
fn squared_lengths_third_altitude() {
    needs(
        "sqlen",
        &geo("A B C = triangle\nH = point: perp(A, H, B, C), perp(B, H, C, A)\nprove perp(C, H, A, B)"),
    );
    never(&geo_false("A B C = triangle\nH = point: perp(A, H, B, C)\nprove perp(C, H, A, B)"));
    never(&geo_false(
        "A B C = triangle\nM = midpoint(A, B)\nH = point: perp(A, H, B, C), perp(B, H, C, M)\nprove perp(C, H, A, B)",
    ));
}

#[test]
fn squared_lengths_nine_point_circle_through_euler_point() {
    needs(
        "sqlen",
        &geo("A B C = triangle\nH = orthocenter(A, B, C)\nMa = midpoint(B, C)\nMb = midpoint(C, A)\n\
              Mc = midpoint(A, B)\nP = midpoint(A, H)\nprove cyclic(Ma, Mb, Mc, P)"),
    );
    never(&geo_false(
        "A B C = triangle\nG = centroid(A, B, C)\nMa = midpoint(B, C)\nMb = midpoint(C, A)\n\
         Mc = midpoint(A, B)\nP = midpoint(A, G)\nprove cyclic(Ma, Mb, Mc, P)",
    ));
}

const ISO_RIGHT: &str = "c@0_0 = ; a@3_1 = ; b@-1_3 = perp c a c b, cong c a c b";

#[test]
fn squared_lengths_irrational_ratio_via_pythagoras() {
    let p = low(&format!("{ISO_RIGHT} ? rcompute a b c a"));
    needs("sqlen", &p);
    let d = close(&p, ALL);
    assert!(d.rcompute_deps(p.goal.as_ref().unwrap()).is_some());
    assert!(d.rcompute(p.goal.as_ref().unwrap()).is_none(), "√2 is not rational");
    never(&low(&format!("{ISO_RIGHT} ? cong a b c a")));
    never(&low(&format!("{ISO_RIGHT} ? rconst a b c a 3/2")));
    never(&low("c@0_0 = ; a@3_1 = ; b@-2_6 = perp c a c b ? rcompute a b c a"));
}

const STEWART: &str = "a@0_0 = ; c@1_0 = ; b@0.5_1.9364916731037085 = rconst a b a c 2, rconst b c a c 2";

#[test]
fn squared_lengths_stewart_median() {
    let mid = "m@0.75_0.96824583655185425 = coll b m c, cong m b m c";
    needs("sqlen", &low(&format!("{STEWART}; {mid} ? rcompute a m a c")));
    never(&low(&format!("{STEWART}; {mid} ? rconst a m a c 3/2")));
    never(&low(&format!(
        "{STEWART}; m@0.6_1.5491933384829668 = coll b m c ? rcompute a m a c"
    )));
}

const TRANSVERSAL: &str = "a@0_0 = ; b@4_0 = ; c@0_4 = ; f@2_0 = coll a f b; e@0_1 = coll c e a; \
     d@6_-2 = coll f e d, coll b c d";

#[test]
fn menelaus_ratio_from_a_transversal() {
    needs(
        "menelaus",
        &low(&format!("{TRANSVERSAL}, cong a f f b ? eqratio b d d c e a c e")),
    );
    never(&low(
        "a@0_0 = ; b@4_0 = ; c@0_4 = ; f@1_0 = coll a f b; e@0_2 = coll c e a; \
         d@-2_6 = coll f e d, coll b c d ? eqratio b d d c e a c e",
    ));
}

#[test]
fn menelaus_converse_needs_the_even_branch() {
    needs(
        "menelaus",
        &low(
            "a@0_0 = ; b@4_0 = ; c@0_4 = ; f@2_0 = coll a f b, cong a f f b; \
             e@0_1 = coll c e a, rconst c e e a 3; d@6_-2 = coll b c d, rconst b d d c 1/3 \
             ? coll d e f",
        ),
    );
    never(&low(
        "a@0_0 = ; b@4_0 = ; c@0_4 = ; f@2_0 = coll a f b, cong a f f b; \
         e@0_2 = coll c e a, cong c e e a; d@2_2 = coll b c d, cong b d d c ? coll d e f",
    ));
}

const CEVA_BASE: &str = "a@0_0 = ; b@6_0 = ; c@0_6 = ; d@2_4 = coll b d c, rconst b d d c 2; \
     e@0_4 = coll c e a, rconst c e e a 1/2";

#[test]
fn ceva_converse_concurrent_cevians() {
    needs(
        "menelaus",
        &low(&format!(
            "{CEVA_BASE}; f@3_0 = coll a f b, cong a f f b; x@1.5_3 = coll a x d, coll b x e ? coll c x f"
        )),
    );
    never(&low(&format!(
        "{CEVA_BASE}; f@4_0 = coll a f b, rconst a f f b 2; x@1.5_3 = coll a x d, coll b x e ? coll c x f"
    )));
}

#[test]
fn ceva_converse_gergonne_point() {
    needs(
        "menelaus",
        &geo("A B C = triangle\nI = incenter(A, B, C)\nTa = foot(I, line(B, C))\n\
              Tb = foot(I, line(C, A))\nTc = foot(I, line(A, B))\n\
              X = meet(line(A, Ta), line(B, Tb))\nprove coll(C, Tc, X)"),
    );
    never(&geo_false(
        "A B C = triangle\nI = incenter(A, B, C)\nTa = foot(I, line(B, C))\n\
         Tb = foot(I, line(C, A))\nTc = midpoint(A, B)\n\
         X = meet(line(A, Ta), line(B, Tb))\nprove coll(C, Tc, X)",
    ));
}

#[test]
fn bisectors_concur_at_incentre_and_excentre() {
    needs(
        "bisconc",
        &geo("A B C = triangle\nX = meet(bisector(B, A, C), bisector(A, B, C))\n\
              prove eqangle(C, B, C, X, C, X, C, A)"),
    );
    needs(
        "bisconc",
        &geo("A B C = triangle\nX = meet(bisector(B, A, C), perp_line(B, bisector(A, B, C)))\n\
              prove eqangle(C, B, C, X, C, X, C, A)"),
    );
    never(&geo_false(
        "A B C = triangle\nX = meet(bisector(B, A, C), line(B, midpoint(C, A)))\n\
         prove eqangle(C, B, C, X, C, X, C, A)",
    ));
}

#[test]
fn rcompute_is_a_determined_ratio() {
    let p = low("a@0_0 = ; b@4_1 = ; m@2_0.5 = coll a m b, cong a m m b ? rcompute a m a b");
    let d = close(&p, ALL);
    assert_eq!(
        d.rcompute(p.goal.as_ref().unwrap()),
        Some(ddar::rational::Rat::new(1, 2))
    );
    assert!(proves(&p, ALL));
    never(&low("a@0_0 = ; b@4_1 = ; m@1_0.25 = coll a m b ? rcompute a m a b"));
    never(&low("a@0_0 = ; b@4_1 = ; c@1_3 = ? rcompute a c a b"));
}

/// Named theorems that DDAR proves only with the new rules (measured on the
/// rule's introduction: each fails with its rule switched off).
#[test]
fn named_theorems_reached_by_the_new_rules() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/named");
    for (rule, file) in [
        ("sqlen", "bottema_theorem.geo"),
        ("sqlen", "la_hire_theorem.geo"),
        ("sqlen", "steiner_line.geo"),
        ("sqlen", "vecten_point.geo"),
        ("bisconc", "angle_bic_formula.geo"),
        ("bisconc", "japanese_theorem.geo"),
        ("menelaus", "desargues_theorem.geo"),
        ("menelaus", "nagel_point.geo"),
    ] {
        let src = std::fs::read_to_string(dir.join(file)).unwrap();
        let p = geo(&src);
        assert!(proves(&p, ALL), "{file}: not proved");
        assert!(!proves(&p, &[rule]), "{file}: proved without `{rule}`");
    }
}
