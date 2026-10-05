use std::collections::HashMap;
use std::time::{Duration, Instant};

use ddar::{Predicate, Problem};

pub const GOOD_SPREAD: f64 = 0.035;
pub const GOOD_SHAPE: f64 = 0.6;
pub const GOOD_FRAMING: f64 = 0.6;
const BATCHES: [usize; 3] = [12, 48, 160];
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
    let mut min = size;
    for i in 0..pts.len() {
        for j in i + 1..pts.len() {
            let d = ((pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2)).sqrt();
            if d > 1e-6 * size {
                min = min.min(d);
            }
        }
    }
    min / size
}

pub fn source_triangles(problem: &Problem, src: &str) -> Vec<[usize; 3]> {
    let index = |n: &str| problem.points.iter().position(|p| p.name == n);
    src.lines()
        .filter_map(|line| {
            let code = line.split('#').next().unwrap_or("");
            let (lhs, rhs) = code.split_once('=')?;
            let rhs = rhs.trim();
            if rhs != "triangle" && rhs.replace(' ', "") != "triangle()" {
                return None;
            }
            let names: Vec<&str> = lhs.split_whitespace().collect();
            match names[..] {
                [a, b, c] => Some([index(a)?, index(b)?, index(c)?]),
                _ => None,
            }
        })
        .collect()
}

pub fn triangle_angles(problem: &Problem, t: [usize; 3]) -> Option<[f64; 3]> {
    let at = |i: usize| problem.points.get(i).map(|p| (p.value.x, p.value.y));
    let (a, b, c) = (at(t[0])?, at(t[1])?, at(t[2])?);
    let angle = |o: (f64, f64), p: (f64, f64), q: (f64, f64)| {
        let (ux, uy, vx, vy) = (p.0 - o.0, p.1 - o.1, q.0 - o.0, q.1 - o.1);
        (ux * vy - uy * vx).abs().atan2(ux * vx + uy * vy).to_degrees()
    };
    let out = [angle(a, b, c), angle(b, c, a), angle(c, a, b)];
    out.iter().all(|x| x.is_finite()).then_some(out)
}

fn triangle_shape(angles: [f64; 3]) -> f64 {
    let mut a = angles;
    a.sort_by(|x, y| y.total_cmp(x));
    let scalene = (a[0] - a[1]).min(a[1] - a[2]);
    (a[2] / 50.0).min((90.0 - a[0]) / 15.0).min(scalene / 10.0).min(1.0)
}

pub fn shape(problem: &Problem, triangles: &[[usize; 3]]) -> f64 {
    triangles
        .iter()
        .map(|&t| triangle_angles(problem, t).map_or(0.0, triangle_shape))
        .fold(1.0, f64::min)
}

pub fn framing(problem: &Problem, triangles: &[[usize; 3]]) -> f64 {
    let pts: Vec<(f64, f64)> =
        problem.points.iter().map(|p| (p.value.x, p.value.y)).filter(|p| p.0.is_finite() && p.1.is_finite()).collect();
    let bbox = |pts: &[(f64, f64)]| {
        pts.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, p| (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1)))
    };
    let b = bbox(&pts);
    let extent = (b.2 - b.0).max(b.3 - b.1);
    if pts.len() < 3 || extent <= 0.0 {
        return 1.0;
    }
    let mut drawn = b;
    let at = |k: u32| problem.points.get(k as usize).map(|p| (p.value.x, p.value.y));
    for pred in problem.preds.iter().chain(problem.goal.iter()).filter(|p| p.name == "cyclic" && p.points.len() >= 3) {
        let q = &pred.points;
        if let (Some(a), Some(b2), Some(c)) = (at(q[0]), at(q[1]), at(q[2])) {
            if let Some((x, y, r)) = crate::present::circumcircle(a, b2, c) {
                drawn = (drawn.0.min(x - r), drawn.1.min(y - r), drawn.2.max(x + r), drawn.3.max(y + r));
            }
        }
    }
    let fit = ((b.2 - b.0).max(extent * 0.3) / (drawn.2 - drawn.0)).min((b.3 - b.1).max(extent * 0.3) / (drawn.3 - drawn.1));
    let tri = triangles
        .iter()
        .filter_map(|t| {
            let v: Vec<(f64, f64)> = t.iter().filter_map(|&k| at(k as u32)).collect();
            (v.len() == 3).then(|| {
                let tb = bbox(&v);
                (tb.2 - tb.0).max(tb.3 - tb.1) / extent
            })
        })
        .fold(1.0, f64::min);
    (fit / 0.9).min(tri / 0.45).min(1.0)
}

fn quality(problem: &Problem, triangles: &[[usize; 3]]) -> (f64, f64) {
    let s = spread(problem);
    let terms = [shape(problem, triangles), (s / GOOD_SPREAD).min(1.0), framing(problem, triangles)];
    let worst = terms.iter().copied().fold(f64::INFINITY, f64::min);
    (if worst <= 0.0 { worst } else { terms.iter().product() }, s)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Goal {
    Holds,
    Fails,
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
    let mut prog: Vec<String> = src.lines().map(str::to_string).collect();
    for i in aux_from..fig.points.len() {
        let mut cons = Vec::new();
        for pred in own_preds(fig, i) {
            cons.extend(geo_constraints(pred, &name)?);
        }
        if cons.is_empty() {
            return None;
        }
        prog.push(format!("{} = point: {}", name(i as u32), cons.join(", ")));
    }
    Some(prog.join("\n"))
}

fn own_preds(fig: &Problem, i: usize) -> impl Iterator<Item = &Predicate> {
    fig.preds
        .iter()
        .filter(move |pred| pred.points.iter().any(|&p| p as usize == i) && pred.points.iter().all(|&p| (p as usize) <= i))
}

type Pt = (f64, f64);

#[derive(Clone, Copy, Debug)]
enum Locus {
    Line(u32, u32),
    Circle(u32, u32, u32),
}

#[derive(Clone, Copy, Debug)]
enum Pick {
    Same(u32),
    Not(u32),
    Side(bool),
}

#[derive(Clone, Copy, Debug)]
struct AuxRule {
    loci: (Locus, Locus),
    pick: Pick,
}

fn locus_of(pred: &Predicate, i: u32) -> Option<Locus> {
    let others: Vec<u32> = pred.points.iter().copied().filter(|&p| p != i).collect();
    if pred.points.len() != others.len() + 1 {
        return None;
    }
    let mut distinct: Vec<u32> = Vec::new();
    for p in others {
        if !distinct.contains(&p) {
            distinct.push(p);
        }
    }
    match pred.name.as_str() {
        "coll" if distinct.len() >= 2 => Some(Locus::Line(distinct[0], distinct[1])),
        "cyclic" if distinct.len() >= 3 => Some(Locus::Circle(distinct[0], distinct[1], distinct[2])),
        _ => None,
    }
}

fn coords(p: &Problem, k: u32) -> Option<Pt> {
    p.points.get(k as usize).map(|q| (q.value.x, q.value.y)).filter(|q| q.0.is_finite() && q.1.is_finite())
}

fn roots(p: &Problem, loci: (Locus, Locus)) -> Option<(Vec<Pt>, Box<dyn Fn(Pt) -> f64>)> {
    let at = |k: u32| coords(p, k);
    let circle = |a: u32, b: u32, c: u32| crate::present::circumcircle(at(a)?, at(b)?, at(c)?);
    match loci {
        (Locus::Line(a, b), Locus::Line(c, d)) => {
            let (p1, p2, p3, p4) = (at(a)?, at(b)?, at(c)?, at(d)?);
            let den = (p1.0 - p2.0) * (p3.1 - p4.1) - (p1.1 - p2.1) * (p3.0 - p4.0);
            if den.abs() < 1e-12 {
                return None;
            }
            let t = ((p1.0 - p3.0) * (p3.1 - p4.1) - (p1.1 - p3.1) * (p3.0 - p4.0)) / den;
            Some((vec![(p1.0 + t * (p2.0 - p1.0), p1.1 + t * (p2.1 - p1.1))], Box::new(|_| 0.0)))
        }
        (Locus::Line(a, b), Locus::Circle(c1, c2, c3)) | (Locus::Circle(c1, c2, c3), Locus::Line(a, b)) => {
            let (pa, pb) = (at(a)?, at(b)?);
            let (ox, oy, r) = circle(c1, c2, c3)?;
            let d = (pb.0 - pa.0, pb.1 - pa.1);
            let dd = d.0 * d.0 + d.1 * d.1;
            if dd < 1e-18 {
                return None;
            }
            let tc = ((ox - pa.0) * d.0 + (oy - pa.1) * d.1) / dd;
            let foot = (pa.0 + tc * d.0, pa.1 + tc * d.1);
            let h2 = r * r - ((foot.0 - ox).powi(2) + (foot.1 - oy).powi(2));
            if h2 < 0.0 {
                return None;
            }
            let s = (h2 / dd).sqrt();
            let pts = vec![(foot.0 - s * d.0, foot.1 - s * d.1), (foot.0 + s * d.0, foot.1 + s * d.1)];
            Some((pts, Box::new(move |x: Pt| (x.0 - foot.0) * d.0 + (x.1 - foot.1) * d.1)))
        }
        (Locus::Circle(a1, a2, a3), Locus::Circle(b1, b2, b3)) => {
            let (x1, y1, r1) = circle(a1, a2, a3)?;
            let (x2, y2, r2) = circle(b1, b2, b3)?;
            let (dx, dy) = (x2 - x1, y2 - y1);
            let d = (dx * dx + dy * dy).sqrt();
            if d < 1e-12 || d > r1 + r2 || d < (r1 - r2).abs() {
                return None;
            }
            let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let h = (r1 * r1 - a * a).max(0.0).sqrt();
            let m = (x1 + a * dx / d, y1 + a * dy / d);
            let pts = vec![(m.0 + h * dy / d, m.1 - h * dx / d), (m.0 - h * dy / d, m.1 + h * dx / d)];
            Some((pts, Box::new(move |x: Pt| dx * (x.1 - y1) - dy * (x.0 - x1))))
        }
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn coincident(p: &Problem, x: Pt, upto: usize) -> Option<u32> {
    let tol = 1e-6 * spread_size(p, p.points.len()).max(1e-9);
    (0..upto as u32).find(|&k| coords(p, k).is_some_and(|q| dist(q, x) <= tol))
}

fn aux_rules(fig: &Problem, aux_from: usize) -> Option<Vec<AuxRule>> {
    (aux_from..fig.points.len())
        .map(|i| {
            let own: Vec<&Predicate> = own_preds(fig, i).collect();
            let loci: Vec<Locus> = own.iter().filter_map(|p| locus_of(p, i as u32)).collect();
            if loci.len() < 2 || own.iter().any(|p| !matches!(p.name.as_str(), "coll" | "cyclic")) {
                return None;
            }
            let pair = (loci[0], loci[1]);
            let (pts, key) = roots(fig, pair)?;
            let x = coords(fig, i as u32)?;
            let mine = (0..pts.len()).min_by(|&a, &b| dist(pts[a], x).total_cmp(&dist(pts[b], x)))?;
            if dist(pts[mine], x) > 1e-3 * spread_size(fig, fig.points.len()).max(1e-9) {
                return None;
            }
            let other = (pts.len() == 2).then(|| pts[1 - mine]);
            let pick = if let Some(k) = coincident(fig, x, i) {
                Pick::Same(k)
            } else if let Some(m) = other.and_then(|o| coincident(fig, o, i)) {
                Pick::Not(m)
            } else {
                Pick::Side(key(x) >= 0.0)
            };
            Some(AuxRule { loci: pair, pick })
        })
        .collect()
}

fn place_aux(out: &mut Problem, i: usize, rule: &AuxRule) -> Option<()> {
    let (pts, key) = roots(out, rule.loci)?;
    let x = match (rule.pick, pts.len()) {
        (Pick::Same(k), _) => coords(out, k)?,
        (_, 1) => pts[0],
        (Pick::Not(m), _) => {
            let q = coords(out, m)?;
            *pts.iter().max_by(|a, b| dist(**a, q).total_cmp(&dist(**b, q)))?
        }
        (Pick::Side(side), _) => *pts.iter().find(|x| (key(**x) >= 0.0) == side)?,
    };
    out.points[i].value = ddar::numerics::Vec2::new(x.0, x.1);
    Some(())
}

fn spread_size(p: &Problem, upto: usize) -> f64 {
    let pts: Vec<Pt> = (0..upto).filter_map(|k| coords(p, k as u32)).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x1 - x0).max(y1 - y0)
}

fn coincidences(p: &Problem, aux_from: usize) -> Vec<(usize, usize)> {
    let tol = 1e-6 * spread_size(p, p.points.len()).max(1e-9);
    let mut out = Vec::new();
    for j in aux_from..p.points.len() {
        for i in 0..j {
            if let (Some(a), Some(b)) = (coords(p, i as u32), coords(p, j as u32)) {
                if dist(a, b) <= tol {
                    out.push((i, j));
                }
            }
        }
    }
    out
}

fn candidate(
    fig: &Problem,
    aux_from: usize,
    inst: &[(String, ddar::numerics::Vec2)],
    rules: Option<&[AuxRule]>,
    goal: Goal,
) -> Option<Problem> {
    let at: HashMap<&str, ddar::numerics::Vec2> = inst.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let mut out = fig.clone();
    for i in 0..out.points.len() {
        if let (Some(rules), true) = (rules, i >= aux_from) {
            place_aux(&mut out, i, &rules[i - aux_from])?;
            continue;
        }
        let key = if i < aux_from { out.points[i].name.clone() } else { format!("zzaux{i}") };
        let v = *at.get(key.as_str())?;
        if !v.x.is_finite() || !v.y.is_finite() {
            return None;
        }
        out.points[i].value = v;
    }
    for pred in &out.preds {
        if ddar::geo::numeric_holds(&out, pred) != Some(true) {
            return None;
        }
    }
    if let Some(g) = &out.goal {
        if ddar::geo::numeric_holds(&out, g) != Some(goal == Goal::Holds) {
            return None;
        }
    }
    (coincidences(&out, aux_from) == coincidences(fig, aux_from)).then_some(out)
}

pub fn respread(fig: &Problem, aux_from: Option<usize>, src: &str, goal: Goal) -> Option<Problem> {
    if fig.points.len() < 3 {
        return None;
    }
    let triangles = source_triangles(fig, src);
    let current = quality(fig, &triangles);
    if current.1 >= GOOD_SPREAD && shape(fig, &triangles) >= GOOD_SHAPE && framing(fig, &triangles) >= GOOD_FRAMING {
        return None;
    }
    let aux_from = aux_from.unwrap_or(fig.points.len()).min(fig.points.len());
    let rules = aux_rules(fig, aux_from);
    let prog = match rules {
        Some(_) => src.to_string(),
        None => aux_program(fig, aux_from, src)?,
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("figure-respread".into())
        .spawn(move || {
            for n in BATCHES {
                let Ok(instances) = ddar::geo::build_instances(&prog, n) else { return };
                let short = instances.len() < n;
                if tx.send(instances).is_err() || short {
                    return;
                }
            }
        })
        .ok()?;
    let deadline = Instant::now() + BUDGET;
    let mut best: Option<((f64, f64), Problem)> = None;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(instances) = rx.recv_timeout(left) else { break };
        for inst in &instances {
            let Some(cand) = candidate(fig, aux_from, inst, rules.as_deref(), goal) else { continue };
            let q = quality(&cand, &triangles);
            if best.as_ref().is_none_or(|(b, _)| q.0 > b.0 + 1e-9 || (q.0 > b.0 - 1e-9 && q.1 > b.1)) {
                best = Some((q, cand));
            }
        }
        if best.as_ref().is_some_and(|((p, _), _)| *p >= 1.0) {
            break;
        }
    }
    let ((q, _), p) = best?;
    (q > current.0 + 0.05 + 0.25 * current.0.abs()).then_some(p)
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
        let out = respread(&squeezed, None, IMO_2019_P2, Goal::Holds).expect("a better instance exists");
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
        assert!(respread(&squeeze(&c.problem), None, src, Goal::Holds).is_none());
    }

    #[test]
    fn a_well_spread_figure_is_kept() {
        let src = "A B C = triangle\nM = midpoint(B, C)\nprove coll(B, M, C)";
        let c = ddar::geo::compile(src).unwrap();
        let tris = source_triangles(&c.problem, src);
        if spread(&c.problem) >= GOOD_SPREAD && shape(&c.problem, &tris) >= GOOD_SHAPE && framing(&c.problem, &tris) >= GOOD_FRAMING {
            assert!(respread(&c.problem, None, src, Goal::Holds).is_none());
        }
    }

    fn drawn(src: &str, goal: Goal) -> Problem {
        let c = ddar::geo::compile(src).unwrap();
        respread(&c.problem, None, src, goal).unwrap_or(c.problem)
    }

    fn assert_well_shaped(p: &Problem, src: &str) {
        let tris = source_triangles(p, src);
        assert_eq!(tris.len(), 1);
        let a = triangle_angles(p, tris[0]).unwrap();
        assert!(shape(p, &tris) >= GOOD_SHAPE, "{src}: angles {a:?}");
        assert!(a.iter().all(|x| (30.0..=81.0).contains(x)), "{src}: angles {a:?}");
    }

    #[test]
    fn the_default_triangle_is_drawn_acute_and_scalene() {
        for src in [
            "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
            "A B C = triangle\nO = circumcenter(A, B, C)\nG = centroid(A, B, C)\nH = orthocenter(A, B, C)\nprove coll(O, G, H)",
            "A B C = triangle\nMa = midpoint(B, C)\nMb = midpoint(A, C)\nMc = midpoint(A, B)\nF = foot(A, line(B, C))\nprove cyclic(Ma, Mb, Mc, F)",
            "A B C = triangle\nP = on_circum(A, B, C)\nX = foot(P, line(B, C))\nY = foot(P, line(C, A))\nZ = foot(P, line(A, B))\nprove coll(X, Y, Z)",
            IMO_2019_P2,
        ] {
            let p = drawn(src, Goal::Holds);
            assert_well_shaped(&p, src);
            for pred in &p.preds {
                assert_eq!(ddar::geo::numeric_holds(&p, pred), Some(true), "{src}: {}", pred.name);
            }
            assert_eq!(ddar::geo::numeric_holds(&p, p.goal.as_ref().unwrap()), Some(true), "{src}");
        }
    }

    #[test]
    fn a_refuted_figure_is_redrawn_only_where_the_claim_still_fails() {
        let src = "A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)";
        let p = drawn(src, Goal::Fails);
        assert_well_shaped(&p, src);
        assert_eq!(ddar::geo::numeric_holds(&p, p.goal.as_ref().unwrap()), Some(false));
    }

    #[test]
    fn triangle_lines_are_found_with_comments_and_parentheses() {
        let src = "# a comment = triangle\nX Y Z = triangle() # three points\nW = midpoint(X, Y)\nprove coll(X, W, Y)";
        let c = ddar::geo::compile(src).unwrap();
        let t = source_triangles(&c.problem, src);
        assert_eq!(t.len(), 1);
        let names: Vec<&str> = t[0].iter().map(|&i| c.problem.points[i].name.as_str()).collect();
        assert_eq!(names, ["X", "Y", "Z"]);
    }

    #[test]
    fn a_figure_with_search_aux_points_is_redrawn_with_them() {
        let sol = crate::engine::solve(IMO_2019_P2, &crate::engine::SolveOptions::default()).unwrap();
        let f = sol.figure.as_ref().unwrap();
        let aux_from = f.aux_from.expect("this proof needs auxiliary points");
        assert!(aux_rules(&f.problem, aux_from).is_some());
        let p = respread(&f.problem, f.aux_from, IMO_2019_P2, Goal::Holds).expect("redrawn");
        let tris = source_triangles(&p, IMO_2019_P2);
        let a = triangle_angles(&p, tris[0]).unwrap();
        assert!(a.iter().all(|x| (25.0..85.0).contains(x)), "angles {a:?}");
        for pred in &p.preds {
            assert_eq!(ddar::geo::numeric_holds(&p, pred), Some(true), "{}", pred.name);
        }
        assert_eq!(coincidences(&p, aux_from), coincidences(&f.problem, aux_from));
    }

    #[test]
    fn an_auxiliary_circle_does_not_take_over_the_frame() {
        let src = "A B C = triangle\nO = circumcenter(A, B, C)\nI = incenter(A, B, C)\nN = meet(line(A, I), circumcircle(A, B, C))\nS = meet(line(N, O), circumcircle(A, B, C))\nD = point: coll(B, S, D), perp(A, D, B, C)\nE = meet(line(A, D), circumcircle(A, B, C))\nL = point: coll(B, E, L), para(D, L, B, C)\nP = meet(circle(B, D, L), circumcircle(A, B, C))\nO1 = circumcenter(B, D, L)\nX = point: coll(B, S, X), perp(X, P, P, O1)\nprove eqangle(A, B, A, X, A, X, A, C)";
        let sol = crate::engine::solve(src, &crate::engine::SolveOptions::default()).unwrap();
        let f = sol.figure.as_ref().unwrap();
        let p = respread(&f.problem, f.aux_from, src, Goal::Holds).expect("redrawn");
        let tris = source_triangles(&p, src);
        let (before, after) = (framing(&f.problem, &tris), framing(&p, &tris));
        assert!(after >= GOOD_FRAMING && after >= before, "framing {before:.2} -> {after:.2}");
        let a = triangle_angles(&p, tris[0]).unwrap();
        assert!(a.iter().all(|x| (25.0..85.0).contains(x)), "angles {a:?}");
    }
}
