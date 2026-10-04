use std::collections::HashMap;
use std::time::Duration;

use ddar::{Predicate, Problem};

pub const GOOD_SPREAD: f64 = 0.035;
const INSTANCES: usize = 12;
const BUDGET: Duration = Duration::from_millis(1500);

pub fn spread(problem: &Problem) -> f64 {
    let pts: Vec<(f64, f64)> = problem
        .points
        .iter()
        .map(|p| (p.value.x, p.value.y))
        .filter(|p| p.0.is_finite() && p.1.is_finite())
        .collect();
    if pts.len() < 2 {
        return 1.0;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let size = (x1 - x0).max(y1 - y0);
    if size <= 0.0 {
        return 0.0;
    }
    let mut min = f64::MAX;
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            min = min.min(((pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2)).sqrt());
        }
    }
    min / size
}

fn is_ident(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_') && cs.all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
}

fn geo_constraints(pred: &Predicate, name: &dyn Fn(u32) -> String) -> Option<Vec<String>> {
    let args = |ids: &[u32]| ids.iter().map(|&i| name(i)).collect::<Vec<_>>().join(", ");
    let p = &pred.points;
    let ok = |n: usize| p.len() == n;
    match pred.name.as_str() {
        "coll" if p.len() >= 3 => Some(vec![format!("coll({})", args(p))]),
        "cyclic" if p.len() >= 4 => Some(vec![format!("cyclic({})", args(p))]),
        "para" | "perp" | "cong" if ok(4) => Some(vec![format!("{}({})", pred.name, args(p))]),
        "eqangle" | "eqratio" if ok(8) => Some(vec![format!("{}({})", pred.name, args(p))]),
        "midp" if ok(3) => Some(vec![
            format!("coll({})", args(p)),
            format!("cong({}, {}, {}, {})", name(p[0]), name(p[1]), name(p[0]), name(p[2])),
        ]),
        _ => None,
    }
}

fn aux_program(fig: &Problem, aux_from: usize, src: &str) -> Option<String> {
    let base: Vec<&str> = fig.points[..aux_from].iter().map(|p| p.name.as_str()).collect();
    if !base.iter().all(|n| is_ident(n)) {
        return None;
    }
    let name = |i: u32| -> String {
        let i = i as usize;
        if i < aux_from {
            fig.points[i].name.clone()
        } else {
            format!("zzaux{i}")
        }
    };
    let mut prog: Vec<String> = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("prove"))
        .map(str::to_string)
        .collect();
    for i in aux_from..fig.points.len() {
        let mut cons = Vec::new();
        for pred in &fig.preds {
            if pred.points.iter().any(|&p| p as usize == i) && pred.points.iter().all(|&p| (p as usize) <= i) {
                cons.extend(geo_constraints(pred, &name)?);
            }
        }
        if cons.is_empty() {
            return None;
        }
        prog.push(format!("{} = point: {}", name(i as u32), cons.join(", ")));
    }
    Some(prog.join("\n"))
}

fn candidate(fig: &Problem, aux_from: usize, inst: &[(String, ddar::numerics::Vec2)]) -> Option<Problem> {
    let at: HashMap<&str, ddar::numerics::Vec2> = inst.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let mut out = fig.clone();
    for (i, p) in out.points.iter_mut().enumerate() {
        let key = if i < aux_from { p.name.clone() } else { format!("zzaux{i}") };
        let v = *at.get(key.as_str())?;
        if !v.x.is_finite() || !v.y.is_finite() {
            return None;
        }
        p.value = v;
    }
    for pred in &out.preds {
        if ddar::geo::numeric_holds(&out, pred) != Some(true) {
            return None;
        }
    }
    if let Some(g) = &out.goal {
        if ddar::geo::numeric_holds(&out, g) != Some(true) {
            return None;
        }
    }
    Some(out)
}

pub fn respread(fig: &Problem, aux_from: Option<usize>, src: &str) -> Option<Problem> {
    let current = spread(fig);
    if current >= GOOD_SPREAD || fig.points.len() < 3 {
        return None;
    }
    let aux_from = aux_from.unwrap_or(fig.points.len()).min(fig.points.len());
    let prog = aux_program(fig, aux_from, src)?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("figure-respread".into())
        .spawn(move || {
            let _ = tx.send(ddar::geo::build_instances(&prog, INSTANCES));
        })
        .ok()?;
    let instances = rx.recv_timeout(BUDGET).ok()?.ok()?;
    let mut best: Option<(f64, Problem)> = None;
    for inst in &instances {
        let Some(cand) = candidate(fig, aux_from, inst) else { continue };
        let s = spread(&cand);
        if best.as_ref().is_none_or(|(b, _)| s > *b) {
            best = Some((s, cand));
        }
    }
    let (s, p) = best?;
    (s > current * 1.5).then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMO_2019_P2: &str = "A B C = triangle
A1 = point: coll(B, C, A1)
B1 = point: coll(A, C, B1)
P = point: coll(A, A1, P)
Q = point: coll(B, B1, Q), para(P, Q, A, B)
P1 = point: coll(B1, P, P1), eqangle(P1, P, P1, C, A, B, A, C)
Q1 = point: coll(A1, Q, Q1), eqangle(Q1, C, Q1, Q, B, C, B, A)
prove cyclic(P, Q, P1, Q1)";

    #[test]
    fn a_crowded_figure_is_redrawn_with_every_hypothesis_intact() {
        let c = ddar::geo::compile(IMO_2019_P2).unwrap();
        let squeezed = squeeze(&c.problem);
        assert!(spread(&squeezed) < GOOD_SPREAD);
        let out = respread(&squeezed, None, IMO_2019_P2).expect("a better instance exists");
        assert!(spread(&out) > spread(&squeezed) * 1.5);
        for pred in &out.preds {
            assert_eq!(ddar::geo::numeric_holds(&out, pred), Some(true), "{}", pred.name);
        }
        assert_eq!(ddar::geo::numeric_holds(&out, out.goal.as_ref().unwrap()), Some(true));
    }

    fn squeeze(p: &Problem) -> Problem {
        let mut q = p.clone();
        if let Some(pt) = q.points.last_mut() {
            pt.value = ddar::numerics::Vec2::new(pt.value.x * 1e4, pt.value.y * 1e4);
        }
        q
    }

    #[test]
    fn a_false_goal_never_gets_a_redrawn_figure() {
        let src = "A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)";
        let c = ddar::geo::compile(src).unwrap();
        assert!(respread(&squeeze(&c.problem), None, src).is_none());
    }

    #[test]
    fn a_well_spread_figure_is_kept() {
        let src = "A B C = triangle\nM = midpoint(B, C)\nprove coll(B, M, C)";
        let c = ddar::geo::compile(src).unwrap();
        if spread(&c.problem) >= GOOD_SPREAD {
            assert!(respread(&c.problem, None, src).is_none());
        }
    }
}
