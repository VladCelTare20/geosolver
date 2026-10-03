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
