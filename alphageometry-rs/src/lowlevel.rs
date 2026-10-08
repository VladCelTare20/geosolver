use crate::numerics::{distance, NumCircle, NumLine, Vec2, ATOM};
use crate::predicate::{Predicate, Problem};
use crate::rational::Rat;
use std::f64::consts::PI;

pub const MAX_POINTS: usize = 100;

pub const MAX_COORD: f64 = 1e6;

pub const CONJUNCTIVE_GOALS: &[&str] = &["midp", "simtri", "contri"];

pub fn parse_checked(input: &str) -> Result<Problem, String> {
    let problem = Problem::parse(input)?;
    check(&problem, input)?;
    Ok(problem)
}

pub fn check(problem: &Problem, input: &str) -> Result<(), String> {
    if problem.points.len() > MAX_POINTS {
        return Err(format!(
            "too many points ({}); this engine supports at most {MAX_POINTS} in one problem",
            problem.points.len()
        ));
    }
    for p in &problem.points {
        let ok = |v: f64| v.is_finite() && v.abs() <= MAX_COORD;
        if !ok(p.value.x) || !ok(p.value.y) {
            return Err(format!(
                "bad coordinate `{}` (coordinates must be finite numbers of size at most {MAX_COORD:e})",
                point_token(input, &p.name)
            ));
        }
    }
    for pred in &problem.preds {
        shape(pred, false)?;
    }
    let goal = problem
        .goal
        .as_ref()
        .ok_or("the problem has no goal (add `? <relation>`)")?;
    shape(goal, true)?;
    for pred in &problem.preds {
        if hypothesis_holds(problem, pred) == Some(false) {
            return Err(format!(
                "hypothesis `{}` does not hold at the given coordinates",
                written(problem, pred)
            ));
        }
    }
    Ok(())
}

fn point_token(input: &str, name: &str) -> String {
    let head = input.split('?').next().unwrap_or(input);
    head.split(|c: char| c.is_whitespace() || c == ';' || c == '=' || c == ',')
        .find(|t| t.split_once('@').is_some_and(|(n, _)| n == name))
        .unwrap_or(name)
        .to_string()
}

fn written(problem: &Problem, p: &Predicate) -> String {
    let mut t = vec![p.name.clone()];
    t.extend(p.points.iter().map(|&i| problem.point_name(i).to_string()));
    t.extend(
        p.constants
            .iter()
            .map(|c| match (c.numer_i64(), c.denom_i64()) {
                (Some(n), Some(1)) => n.to_string(),
                (Some(n), Some(d)) => format!("{n}/{d}"),
                _ => format!("{c:?}"),
            }),
    );
    t.join(" ")
}

fn shape(p: &Predicate, goal: bool) -> Result<(), String> {
    let name = p.name.as_str();
    let (np, nc) = (p.points.len(), p.constants.len());
    let exact = |k: usize| {
        if np != k {
            return Err(format!("`{name}` expects {k} arguments, got {np}"));
        }
        Ok(())
    };
    let at_least = |k: usize| {
        if np < k {
            return Err(format!("`{name}` expects at least {k} arguments, got {np}"));
        }
        Ok(())
    };
    let malformed = |why: &str| Err(format!("malformed relation `{name}`: {why}"));
    let no_consts = || {
        if nc == 0 {
            Ok(())
        } else {
            malformed("it takes no numeric constant")
        }
    };
    let positive = |c: &Rat| !c.is_negative() && !c.is_zero();
    match name {
        "coll" => {
            at_least(if goal { 3 } else { 2 })?;
            no_consts()
        }
        "cyclic" => {
            at_least(if goal { 4 } else { 3 })?;
            no_consts()
        }
        "para" | "perp" | "cong" | "acompute" | "rcompute" => {
            exact(4)?;
            no_consts()
        }
        "eqangle" | "eqratio" => {
            exact(8)?;
            no_consts()
        }
        "overlap" => {
            exact(2)?;
            no_consts()
        }
        "aconst" | "s_angle" => {
            exact(4)?;
            if nc != 1 {
                return malformed("it takes 4 points and one angle constant");
            }
            Ok(())
        }
        "rconst" => {
            exact(4)?;
            if nc != 1 || !positive(&p.constants[0]) {
                return malformed("it takes 4 points and one positive ratio");
            }
            Ok(())
        }
        "angeq" => {
            if nc < 2 || np != 2 * (nc - 1) {
                return malformed("it takes one coefficient per pair of points, then a constant");
            }
            Ok(())
        }
        "distmeq" => {
            if nc < 2 || np != 2 * (nc - 1) || !positive(&p.constants[nc - 1]) {
                return malformed(
                    "it takes one exponent per pair of points, then a positive constant",
                );
            }
            Ok(())
        }
        "distseq" => {
            if nc < 1 || np != 2 * nc {
                return malformed("it takes one coefficient per pair of points");
            }
            Ok(())
        }
        "cyclic_with_centers" => {
            let k = p
                .constants
                .first()
                .filter(|c| c.denom_i64() == Some(1))
                .and_then(Rat::numer_i64);
            match k {
                Some(k) if nc == 1 && k >= 1 && (k as usize) < np => Ok(()),
                _ => malformed(
                    "it takes the number of centers, at least one center and at least one point",
                ),
            }
        }
        "midp" if goal => {
            exact(3)?;
            no_consts()
        }
        "simtri" | "contri" if goal => {
            exact(6)?;
            no_consts()
        }
        _ => Err(format!("unknown relation `{name}`")),
    }
}

fn coords(problem: &Problem, p: &Predicate) -> Vec<Vec2> {
    p.points
        .iter()
        .map(|&i| problem.points[i as usize].value)
        .collect()
}

fn scale(problem: &Problem) -> f64 {
    let n = problem.points.len().max(1) as f64;
    let c = problem
        .points
        .iter()
        .fold(Vec2::new(0.0, 0.0), |a, p| a + p.value)
        * (1.0 / n);
    problem
        .points
        .iter()
        .map(|p| distance(p.value, c))
        .fold(0.0, f64::max)
        .max(1e-9)
}

fn distinct(xs: &[Vec2]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for &x in xs {
        if !out.iter().any(|&y| distance(x, y) < ATOM) {
            out.push(x);
        }
    }
    out
}

fn farthest(xs: &[Vec2], from: Vec2) -> Vec2 {
    xs.iter().copied().fold(from, |best, x| {
        if distance(x, from) > distance(best, from) {
            x
        } else {
            best
        }
    })
}

fn engine_collinear(xs: &[Vec2]) -> bool {
    if distinct(xs).len() < 2 {
        return true;
    }
    let b = farthest(xs, xs[0]);
    let c = farthest(xs, b);
    let line = NumLine::through(b, c);
    xs.iter().all(|&x| line.distance(x) < ATOM)
}

fn engine_concyclic(xs: &[Vec2]) -> bool {
    let d = distinct(xs);
    if d.len() < 3 {
        return true;
    }
    match NumCircle::through(d[0], d[1], d[2]) {
        Some(circle) => xs.iter().all(|&x| circle.distance(x).powi(2) < ATOM),
        None => false,
    }
}

fn dir(a: Vec2, b: Vec2) -> f64 {
    (b.y - a.y).atan2(b.x - a.x)
}

fn mod_pi(x: f64) -> f64 {
    let y = x.rem_euclid(PI);
    if y > PI / 2.0 {
        y - PI
    } else {
        y
    }
}

fn integer(c: &Rat) -> Option<f64> {
    (c.denom_i64() == Some(1)).then(|| c.to_f64())
}

pub fn hypothesis_holds(problem: &Problem, p: &Predicate) -> Option<bool> {
    let xs = coords(problem, p);
    let tol = 1e-7 * scale(problem);
    let consts = &p.constants;
    match p.name.as_str() {
        "coll" => Some(engine_collinear(&xs)),
        "cyclic" => Some(engine_concyclic(&xs)),
        "cyclic_with_centers" => {
            let k = consts.first()?.numer_i64()? as usize;
            let (centers, points) = xs.split_at(k.min(xs.len()));
            let r = distance(*centers.first()?, *points.first()?);
            let equal = centers
                .iter()
                .all(|&c| points.iter().all(|&q| (distance(c, q) - r).abs() < tol));
            Some(equal && engine_concyclic(points))
        }
        "overlap" => Some(distance(xs[0], xs[1]) < tol),
        "s_angle" => {
            let alias = Predicate {
                name: "aconst".into(),
                points: p.points.clone(),
                constants: consts.clone(),
            };
            crate::corpus::pred_holds(problem, &alias)
        }
        "angeq" => {
            let (coefs, konst) = consts.split_at(consts.len() - 1);
            let mut sum = konst[0].to_f64().to_radians();
            for (i, c) in coefs.iter().enumerate() {
                let (a, b) = (xs[2 * i], xs[2 * i + 1]);
                if distance(a, b) < tol {
                    return Some(true);
                }
                sum += integer(c)? * dir(a, b);
            }
            Some(mod_pi(sum).abs() < 1e-7)
        }
        "distmeq" => {
            let (coefs, konst) = consts.split_at(consts.len() - 1);
            let mut sum = konst[0].to_f64().ln();
            for (i, c) in coefs.iter().enumerate() {
                let d = distance(xs[2 * i], xs[2 * i + 1]);
                if d < tol {
                    return Some(true);
                }
                sum += c.to_f64() * d.ln();
            }
            Some(sum.abs() < 1e-7)
        }
        "distseq" => {
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (i, c) in consts.iter().enumerate() {
                sum += c.to_f64() * distance(xs[2 * i], xs[2 * i + 1]);
                weight += c.to_f64().abs();
            }
            Some(sum.abs() < tol * weight.max(1.0))
        }
        "acompute" | "rcompute" => None,
        _ => crate::corpus::pred_holds(problem, p),
    }
}

pub fn conjuncts(problem: &Problem, goal: &Predicate) -> Option<Vec<Predicate>> {
    if !CONJUNCTIVE_GOALS.contains(&goal.name.as_str()) {
        return None;
    }
    crate::corpus::lower_engine_goal(problem, goal)
}

pub fn goal_holds(problem: &Problem, goal: &Predicate) -> Option<bool> {
    let check = |g: &Predicate| match g.name.as_str() {
        "s_angle" => {
            let alias = Predicate {
                name: "aconst".into(),
                points: g.points.clone(),
                constants: g.constants.clone(),
            };
            crate::corpus::pred_holds(problem, &alias)
        }
        "acompute"
        | "rcompute"
        | "overlap"
        | "angeq"
        | "distmeq"
        | "distseq"
        | "cyclic_with_centers" => None,
        _ => crate::corpus::pred_holds(problem, g),
    };
    match conjuncts(problem, goal) {
        Some(parts) => parts
            .iter()
            .map(check)
            .try_fold(true, |all, h| h.map(|h| all && h)),
        None => check(goal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(input: &str) -> String {
        parse_checked(input).expect_err(input)
    }

    #[test]
    fn malformed_low_level_inputs_are_located_errors() {
        assert!(
            err("a@0_0 b@1_0 c@0_1 = foo a b ? coll a b c").starts_with("unknown relation `foo`")
        );
        assert!(err("a@0_0 = ; b@1_0 = ; c@0_1 = ; d@2_2 = ? para a b")
            .starts_with("`para` expects 4 arguments, got 2"));
        assert!(
            err("a@0_0 = ; b@1_0 = ; c@0_1 = ? qqq a b c").starts_with("unknown relation `qqq`")
        );
        assert!(
            err("a@0_0 = ; b@1_0 = ; c@0_1 = ; m@0.5_0.5 = midp m b c ? coll a b c")
                .starts_with("unknown relation `midp`")
        );
        assert!(err("a@0_1.414 = ? angeq").starts_with("malformed relation `angeq`"));
        assert!(err(
            "a@0_0 = ; b@1_0 = ; c@0_1 = ; d@1_1 = cyclic_with_centers a b c d 1pi/3 ? coll a b c"
        )
        .starts_with("malformed relation `cyclic_with_centers`"));
        assert!(err("a@0_0 = ; b@1_0 = ; c@0_1 = ? ncoll a b c")
            .starts_with("unknown relation `ncoll`"));
        assert!(
            err("a@0_0 = ; b@1_0 = ; c@0_1 = rconst a b a c -2 ? coll a b c")
                .starts_with("malformed relation `rconst`")
        );
        assert!(err("a@0_0 = ; b@1_0 = ; c@0_1 = ").contains("no goal"));
    }

    #[test]
    fn non_finite_and_huge_coordinates_are_rejected() {
        let e = err("a@NaN_0 = ; b@1_0 = ; c@0_1 = ? cong a b a c");
        assert!(e.starts_with("bad coordinate `a@NaN_0`"), "{e}");
        assert!(err("a@inf_0 = ; b@1_0 = ; c@0_1 = ? cong a b a c")
            .starts_with("bad coordinate `a@inf_0`"));
        assert!(err("a@0_1e9 = ; b@1_0 = ; c@0_1 = ? cong a b a c").starts_with("bad coordinate"));
    }

    #[test]
    fn hypotheses_must_hold_at_the_given_coordinates() {
        let e = err("a@0_0 = ; b@1_0 = ; c@0_1 = coll a b c ? perp a b a c");
        assert_eq!(
            e,
            "hypothesis `coll a b c` does not hold at the given coordinates"
        );
        assert!(err("a@0_0 = ; b@2_0 = ; c@0_3 = cong a b a c ? coll a b c")
            .contains("`cong a b a c` does not hold"));
        assert!(
            err("a@0_0 = ; b@1_0 = ; c@2_0 = ; d@3_1 = cyclic a b c d ? coll a b c")
                .contains("`cyclic a b c d` does not hold")
        );
        assert!(
            err("a@0_0 = ; b@1_0 = ; c@0_1 = ; d@1_1 = para a b a c ? coll a b c")
                .contains("`para a b a c` does not hold")
        );
        parse_checked("a@0_0 = ; b@1_0 = ; c@0_1 = perp a b a c, cong a b a c ? cong a b a c")
            .unwrap();
        parse_checked("a@0_0 = ; b@2_0 = ; m@1_0 = coll a b m, cong m a m b ? coll a b m").unwrap();
    }

    #[test]
    fn more_than_max_points_is_rejected_before_any_allocation() {
        let pts: Vec<String> = (0..=MAX_POINTS)
            .map(|i| format!("p{i}@{}_{} = ", i as f64 * 0.01, (i * i) as f64 * 0.001))
            .collect();
        let input = format!("{} ? cong p0 p1 p0 p2", pts.join("; "));
        let t = std::time::Instant::now();
        let e = err(&input);
        assert!(e.starts_with("too many points (101)"), "{e}");
        assert!(t.elapsed() < std::time::Duration::from_millis(200));
        let ok: Vec<String> = (0..MAX_POINTS)
            .map(|i| format!("p{i}@{}_{} = ", i as f64 * 0.01, (i * i) as f64 * 0.001))
            .collect();
        parse_checked(&format!("{} ? cong p0 p1 p0 p2", ok.join("; "))).unwrap();
    }

    #[test]
    fn every_bundled_low_level_problem_passes_the_checks_and_its_goal_holds() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("problems.tsv");
        let text = std::fs::read_to_string(path).unwrap();
        let mut n = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let cols: Vec<&str> = line.split('\t').collect();
            let Some(input) = cols.last() else { continue };
            let p = parse_checked(input).unwrap_or_else(|e| panic!("{}: {e}", cols[1]));
            assert_ne!(
                goal_holds(&p, p.goal.as_ref().unwrap()),
                Some(false),
                "{}",
                cols[1]
            );
            n += 1;
        }
        assert!(n >= 26, "{n}");
    }

    #[test]
    fn goal_holds_reads_the_given_coordinates() {
        let p = parse_checked("a@0_0 = ; b@1_0 = ; c@0_2 = ? cong a b a c").unwrap();
        assert_eq!(goal_holds(&p, p.goal.as_ref().unwrap()), Some(false));
        let p = parse_checked("a@0_0 = ; b@1_0 = ; c@0_1 = ? cong a b a c").unwrap();
        assert_eq!(goal_holds(&p, p.goal.as_ref().unwrap()), Some(true));
        let p = parse_checked("b@0_0 = ; c@2_0 = ; f@0.5_0 = coll f b c ? midp f b c").unwrap();
        let g = p.goal.as_ref().unwrap();
        assert_eq!(conjuncts(&p, g).map(|c| c.len()), Some(2));
        assert_eq!(goal_holds(&p, g), Some(false));
        let p = parse_checked("b@0_0 = ; c@2_0 = ; f@1_0 = coll f b c ? midp f b c").unwrap();
        assert_eq!(goal_holds(&p, p.goal.as_ref().unwrap()), Some(true));
    }
}
