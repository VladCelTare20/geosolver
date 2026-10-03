use super::*;

fn figure(cons: &str) -> Figure {
    let (sampled, insts) = sampled_instances(cons).expect("figure");
    Figure::gather(&sampled, insts)
}

fn proves(cons: &str, goal: &str) -> bool {
    matches!(prove_ratio(cons, goal), Ok(Outcome::Proved(_)))
}

fn all_rows(f: &Figure) -> BTreeSet<usize> {
    (0..f.steps.len()).filter(|&i| f.steps[i].eq.is_some()).collect()
}

#[test]
fn thirty_degrees_in_a_right_triangle_gives_half_the_hypotenuse() {
    assert!(proves(
        "A = free\nB = free\nC = point: angle(B, A, C) = 30, perp(C, A, C, B)",
        "dist(B,C) = dist(A,B) / 2"
    ));
}

#[test]
fn thirty_degrees_without_the_right_angle_is_not_half() {
    assert!(!proves(
        "A = free\nB = free\nC = point: angle(B, A, C) = 30",
        "dist(B,C) = dist(A,B) / 2"
    ));
}

#[test]
fn seventy_two_degrees_has_no_known_sine_row() {
    let mut f = figure("A = free\nB = free\nC = point: angle(B, A, C) = 72");
    let atom = f.sin_atom(0, 1, 2).expect("a proper corner");
    f.gather_known_sines(&[atom]);
    assert!(f.steps.is_empty(), "{:?}", f.steps.iter().map(|s| &s.text).collect::<Vec<_>>());
}

#[test]
fn uncertifiable_equal_sines_row_is_rejected() {
    let mut f = figure("A = free\nB = free\nC = free");
    let pts: Vec<PointId> = (0..3).collect();
    f.gather_trig(&pts, &pts.iter().copied().collect());
    f.push_equal_sines(sin_key(0, 1, 2), sin_key(1, 0, 2));
    let base = all_rows(&f);
    let mut goal = LEq::default();
    goal.add_term(latom(1, 2), Rat::one());
    goal.add_term(latom(0, 2), -Rat::one());
    assert!(f.certified_prove(&goal, &base).is_none());
}

#[test]
fn degenerate_triples_get_no_sine_atom() {
    let mut f = figure("B = free\nC = free\nM = midpoint(B, C)\nA = free");
    let (b, c, m) = (0, 1, 2);
    assert!(f.sin_atom(b, m, c).is_none());
    assert!(f.sin_atom(m, b, c).is_none());
    let pts: Vec<PointId> = (0..f.names.len() as PointId).collect();
    f.gather_trig(&pts, &BTreeSet::new());
    for s in &f.steps {
        assert!(!s.text.contains("△BCM") && !s.text.contains("△BMC"), "{}", s.text);
    }
}

#[test]
fn law_of_sines_alone_does_not_prove_its_restatement() {
    let mut f = figure("A = free\nB = free\nC = free");
    let pts: Vec<PointId> = (0..3).collect();
    f.gather_trig(&pts, &pts.iter().copied().collect());
    let base = all_rows(&f);
    let mut goal = LEq::default();
    goal.add_term(latom(1, 2), Rat::one());
    goal.add_term(sin_key(0, 1, 2), -Rat::one());
    goal.add_term(latom(0, 2), -Rat::one());
    goal.add_term(sin_key(1, 0, 2), Rat::one());
    assert!(f.certified_prove(&goal, &base).is_none());
}

#[test]
fn power_of_a_point_needs_one_configuration_in_every_instance() {
    let cons = "O = free\nA = free\nX = on_circle(O, A)\nY = on_circle(O, A)\nP = on_line(X, Y)";
    let (sampled, insts) = sampled_instances(cons).expect("figure");
    let (x, y, p) = (2usize, 3usize, 4usize);
    let mut inside = insts[0].clone();
    inside[p] = (inside[x] + inside[y]) * 0.5;
    let mut outside = insts[0].clone();
    outside[p] = outside[x] + (outside[y] - outside[x]) * 2.0;
    let mut f = Figure::gather(&sampled, vec![inside.clone(), outside]);
    f.gather_center_power(&[0, 1, 2, 3, 4]);
    assert!(f.psteps.is_empty(), "a power row was emitted with the inside/outside branch varying");
    let mut f = Figure::gather(&sampled, vec![inside.clone(), inside]);
    f.gather_center_power(&[0, 1, 2, 3, 4]);
    assert!(!f.psteps.is_empty(), "the consistent inside configuration gets its row");
}

#[test]
fn irrational_ratios_are_never_bridged() {
    let f = figure("A = free\nB = free\nC = eq_triangle(A, B)\nM = midpoint(A, B)");
    let (a, b, c, m) = (0, 1, 2, 3);
    assert!(f.exact_ratio(&[latom(c, m).into()], &[latom(a, b).into()]).is_none());
    assert_eq!(f.exact_ratio(&[latom(a, m).into()], &[latom(a, b).into()]), Some(Rat::new(1, 2)));
}

#[test]
fn a_rational_ratio_without_a_log_proof_gets_no_bridge() {
    let mut f = figure("A = free\nB = free\nM = midpoint(A, B)");
    let pool: BTreeSet<Mono> = [vec![latom(0, 2).into()], vec![latom(0, 1).into()]].into_iter().collect();
    f.gather_log_bridges(&pool, &pool);
    assert!(f.psteps.is_empty());
}

#[test]
fn symmedian_proof_reads_as_a_ratio_lemma() {
    let cons = "A B C = triangle\nO = circumcenter(A, B, C)\n\
                T = meet(perp_line(B, line(O, B)), perp_line(C, line(O, C)))\n\
                X = meet(line(A, T), line(B, C))";
    let Ok(Outcome::Proved(p)) = prove_ratio(cons, "dist(B,X)*dist(A,C)^2 = dist(X,C)*dist(A,B)^2") else {
        panic!("the symmedian ratio is proved");
    };
    assert!(p.contains("Ratio lemma in △ABC with cevian AX: BX/XC = AB·sin∠BAX / (AC·sin∠CAX)"), "{p}");
    assert!(p.contains("Multiplying the sine relations above"), "{p}");
}

#[test]
fn area_additivity_needs_the_same_convex_order_in_every_instance() {
    let cons = "A B C = triangle\nD = free";
    let (sampled, insts) = sampled_instances(cons).expect("figure");
    let (a, b, c, d) = (0usize, 1usize, 2usize, 3usize);
    let mut convex = insts[0].clone();
    convex[d] = (convex[a] + convex[c]) * 0.5 - (convex[b] - (convex[a] + convex[c]) * 0.5);
    let mut inside = insts[0].clone();
    inside[d] = (inside[a] + inside[b] + inside[c]) * (1.0 / 3.0);
    let f = Figure::gather(&sampled, vec![convex.clone(), inside]);
    assert!(!f.area_base_rows(&[0, 1, 2, 3]).iter().any(|r| r.text.contains("convex")));
    let f = Figure::gather(&sampled, vec![convex.clone(), convex]);
    assert!(f.area_base_rows(&[0, 1, 2, 3]).iter().any(|r| r.text.contains("convex")));
}
