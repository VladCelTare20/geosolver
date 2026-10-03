//! The web app's figure: an SVG drawn from the solved problem, framed to its
//! whole content (circles and labels included, never clipped), with every
//! element tagged by class and by the points it involves (`data-p`) so the
//! page can theme it and highlight what a proof step talks about.

use std::collections::HashMap;
use std::fmt::Write as _;

use ddar::{Predicate, Problem};

use crate::present::{circumcircle, Names, Pt};

pub struct Extras {
    pub polygons: Vec<Vec<String>>,
    pub metric_goal: Option<String>,
}

const SPAN: f64 = 520.0;
const PAD: f64 = 22.0;
const LABEL_FS: f64 = 18.0;
const DOT_R: f64 = 4.0;

const INK: &str = "#1b1f24";
const CONSTRUCTION: &str = "#6e7a86";
const CIRCLE: &str = "#2a6fb0";
const GOAL: &str = "#c8102e";
const AUX: &str = "#b45309";
const MARK: &str = "#4b525a";

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Base,
    Aux,
    Goal,
}

enum Shape {
    Line(Pt, Pt),
    Circle(Pt, f64),
    Path(String, Vec<Pt>),
}

struct El {
    shape: Shape,
    class: &'static str,
    role: Role,
    dashed: bool,
    width: f64,
    pts: Vec<String>,
    centre: Option<String>,
}

struct Frame {
    x0: f64,
    y1: f64,
    s: f64,
}

impl Frame {
    fn map(&self, x: f64, y: f64) -> Pt {
        ((x - self.x0) * self.s, (self.y1 - y) * self.s)
    }
}

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: Pt, k: f64) -> Pt {
    (a.0 * k, a.1 * k)
}
fn len(a: Pt) -> f64 {
    (a.0 * a.0 + a.1 * a.1).sqrt()
}
fn unit(a: Pt) -> Pt {
    let l = len(a).max(1e-9);
    (a.0 / l, a.1 / l)
}
fn finite(p: Pt) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

fn intersect(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    let r = sub(b, a);
    let s = sub(d, c);
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-9 * len(r).max(1e-9) * len(s).max(1e-9) {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / den;
    Some(add(a, mul(r, t)))
}

fn dist_to_line(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = sub(b, a);
    ((ab.0 * (a.1 - p.1) - (a.0 - p.0) * ab.1) / len(ab).max(1e-12)).abs()
}

fn pairs(pred: &Predicate) -> Vec<(u32, u32)> {
    match pred.name.as_str() {
        "cong" | "perp" | "para" | "eqangle" | "eqratio" | "distmeq" | "distseq" | "s_angle"
        | "aconst" | "angeq" | "rconst" | "acompute" => {
            pred.points.chunks_exact(2).map(|c| (c[0], c[1])).collect()
        }
        _ => Vec::new(),
    }
}

fn cong_centre(p: &[u32]) -> Option<(u32, u32, u32)> {
    for (c1, t1, c2, t2) in [(0, 1, 2, 3), (0, 1, 3, 2), (1, 0, 2, 3), (1, 0, 3, 2)] {
        if p[c1] == p[c2] && p[t1] != p[t2] {
            return Some((p[c1], p[t1], p[t2]));
        }
    }
    None
}

struct Uf(Vec<usize>);
impl Uf {
    fn find(&mut self, i: usize) -> usize {
        if self.0[i] != i {
            let r = self.find(self.0[i]);
            self.0[i] = r;
        }
        self.0[i]
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a] = b;
        }
    }
}

fn metric_pairs(goal: &str) -> (Vec<(String, String)>, Vec<(String, String, String)>) {
    let mut d = Vec::new();
    let mut a = Vec::new();
    for (key, out_angles) in [("dist(", false), ("angle(", true)] {
        let mut rest = goal;
        while let Some(at) = rest.find(key) {
            let after = &rest[at + key.len()..];
            let Some(close) = after.find(')') else { break };
            let args: Vec<String> = after[..close].split(',').map(|s| s.trim().to_string()).collect();
            if !out_angles && args.len() == 2 {
                d.push((args[0].clone(), args[1].clone()));
            } else if out_angles && args.len() == 3 {
                a.push((args[0].clone(), args[1].clone(), args[2].clone()));
            }
            rest = &after[close + 1..];
        }
    }
    (d, a)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn arc_path(v: Pt, p1: Pt, p2: Pt, r: f64) -> Option<(String, Vec<Pt>)> {
    let a1 = (p1.1 - v.1).atan2(p1.0 - v.0);
    let a2 = (p2.1 - v.1).atan2(p2.0 - v.0);
    let mut delta = a2 - a1;
    while delta > std::f64::consts::PI {
        delta -= 2.0 * std::f64::consts::PI;
    }
    while delta < -std::f64::consts::PI {
        delta += 2.0 * std::f64::consts::PI;
    }
    if delta.abs() < 1e-3 || !finite(v) {
        return None;
    }
    let steps = 14;
    let mut d = String::new();
    let mut pts = Vec::new();
    for k in 0..=steps {
        let t = a1 + delta * (k as f64) / (steps as f64);
        let p = (v.0 + r * t.cos(), v.1 + r * t.sin());
        let _ = write!(d, "{}{:.1},{:.1}", if k == 0 { "M" } else { " L" }, p.0, p.1);
        pts.push(p);
    }
    Some((d, pts))
}

pub fn render(problem: &Problem, aux_from: Option<usize>, names: &Names, ex: &Extras) -> String {
    let aux_from = aux_from.unwrap_or(usize::MAX);
    let n = problem.points.len();
    let world: Vec<Pt> = problem.points.iter().map(|p| (p.value.x, p.value.y)).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &p in &world {
        if finite(p) {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
    }
    if x0 > x1 {
        (x0, y0, x1, y1) = (0.0, 0.0, 1.0, 1.0);
    }
    let span = (x1 - x0).max(y1 - y0).max(1e-9);
    let frame = Frame { x0, y1, s: SPAN / span };
    let _ = y0;
    let scr: Vec<Pt> = world.iter().map(|&p| frame.map(p.0, p.1)).collect();
    let raw = |i: u32| problem.point_name(i).to_string();
    let dn = |i: u32| names.get(&raw(i));
    let is_aux = |i: u32| (i as usize) >= aux_from;
    let id_of: HashMap<String, u32> = problem
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.clone(), i as u32))
        .collect();
    let max_r = SPAN * 3.0;

    let mut els: Vec<El> = Vec::new();

    // Lines through collinear sets.
    let mut lines: Vec<(Pt, Pt, Vec<u32>)> = Vec::new();
    for pred in problem.preds.iter().filter(|p| p.name == "coll") {
        let ids: Vec<u32> = {
            let mut v = Vec::new();
            for &i in &pred.points {
                if !v.contains(&i) && finite(scr[i as usize]) {
                    v.push(i);
                }
            }
            v
        };
        let mut best: Option<(f64, Pt, Pt)> = None;
        for a in 0..ids.len() {
            for b in a + 1..ids.len() {
                let (pa, pb) = (scr[ids[a] as usize], scr[ids[b] as usize]);
                let d = len(sub(pa, pb));
                if best.is_none_or(|(bd, _, _)| d > bd) {
                    best = Some((d, pa, pb));
                }
            }
        }
        let Some((d, lo, hi)) = best else { continue };
        if d < 1e-6 {
            continue;
        }
        if let Some(existing) = lines.iter_mut().find(|(a, b, _)| dist_to_line(lo, *a, *b) < 0.5 && dist_to_line(hi, *a, *b) < 0.5) {
            for i in ids {
                if !existing.2.contains(&i) {
                    existing.2.push(i);
                }
            }
            let all: Vec<Pt> = existing.2.iter().map(|&i| scr[i as usize]).collect();
            let mut far = (0.0, existing.0, existing.1);
            for a in 0..all.len() {
                for b in a + 1..all.len() {
                    let dd = len(sub(all[a], all[b]));
                    if dd > far.0 {
                        far = (dd, all[a], all[b]);
                    }
                }
            }
            existing.0 = far.1;
            existing.1 = far.2;
            continue;
        }
        lines.push((lo, hi, ids));
    }
    let covered = |a: Pt, b: Pt| lines.iter().any(|(lo, hi, _)| dist_to_line(a, *lo, *hi) < 0.5 && dist_to_line(b, *lo, *hi) < 0.5);
    for (lo, hi, ids) in &lines {
        let ext = mul(sub(*hi, *lo), 0.07);
        let aux = ids.iter().any(|&i| is_aux(i));
        els.push(El {
            shape: Shape::Line(sub(*lo, ext), add(*hi, ext)),
            class: "f-line",
            role: if aux { Role::Aux } else { Role::Base },
            dashed: aux,
            width: 1.6,
            pts: ids.iter().map(|&i| dn(i)).collect(),
            centre: None,
        });
    }

    // Circles: cyclic sets, and centres with three or more equidistant points.
    let mut circles: Vec<(Pt, f64, Vec<u32>, Option<u32>, bool)> = Vec::new();
    let push_circle = |c: Pt, r: f64, on: Vec<u32>, centre: Option<u32>, aux: bool, circles: &mut Vec<(Pt, f64, Vec<u32>, Option<u32>, bool)>| {
        if !finite(c) || !r.is_finite() || r < 1e-6 || r > max_r {
            return;
        }
        if let Some(e) = circles.iter_mut().find(|(cc, rr, ..)| len(sub(*cc, c)) < 0.5 && (rr - r).abs() < 0.5) {
            for i in on {
                if !e.2.contains(&i) {
                    e.2.push(i);
                }
            }
            if e.3.is_none() {
                e.3 = centre;
            }
            e.4 = e.4 && aux;
            return;
        }
        circles.push((c, r, on, centre, aux));
    };
    let mut centre_groups: HashMap<(u32, i64), (Vec<u32>, bool)> = HashMap::new();
    let mut tick_pairs: Vec<((u32, u32), (u32, u32))> = Vec::new();
    for pred in &problem.preds {
        let aux = pred.points.iter().any(|&i| is_aux(i));
        if pred.name == "cyclic" && pred.points.len() >= 3 {
            let p = &pred.points;
            if let Some((ux, uy, r)) = circumcircle(scr[p[0] as usize], scr[p[1] as usize], scr[p[2] as usize]) {
                let mut on = Vec::new();
                for &i in p {
                    if !on.contains(&i) {
                        on.push(i);
                    }
                }
                push_circle((ux, uy), r, on, None, aux, &mut circles);
            }
        }
        if pred.name == "cong" && pred.points.len() == 4 {
            let p = &pred.points;
            if (p[0] == p[2] && p[1] == p[3]) || (p[0] == p[3] && p[1] == p[2]) {
                continue;
            }
            if let Some((c, a, b)) = cong_centre(p) {
                let r = len(sub(scr[c as usize], scr[a as usize]));
                let key = (c, (r * 10.0).round() as i64);
                let e = centre_groups.entry(key).or_insert((Vec::new(), true));
                for i in [a, b] {
                    if !e.0.contains(&i) {
                        e.0.push(i);
                    }
                }
                e.1 = e.1 && aux;
            }
            tick_pairs.push(((p[0], p[1]), (p[2], p[3])));
        }
    }
    let mut centre_list: Vec<_> = centre_groups.into_iter().collect();
    centre_list.sort_by_key(|((c, r), _)| (*c, *r));
    for ((c, _), (on, aux)) in centre_list {
        if on.len() >= 3 {
            let r = len(sub(scr[c as usize], scr[on[0] as usize]));
            push_circle(scr[c as usize], r, on, Some(c), aux, &mut circles);
        }
    }
    for (c, r, on, centre, aux) in &circles {
        els.push(El {
            shape: Shape::Circle(*c, *r),
            class: "f-circ",
            role: if *aux { Role::Aux } else { Role::Base },
            dashed: *aux,
            width: 1.5,
            pts: on.iter().map(|&i| dn(i)).collect(),
            centre: centre.map(dn),
        });
    }

    // Segments named by hypotheses and declared shapes.
    let mut segs: Vec<(u32, u32)> = Vec::new();
    let add_seg = |a: u32, b: u32, segs: &mut Vec<(u32, u32)>| {
        if a == b {
            return;
        }
        let k = if a < b { (a, b) } else { (b, a) };
        if !segs.contains(&k) {
            segs.push(k);
        }
    };
    for poly in &ex.polygons {
        let ids: Vec<u32> = poly.iter().filter_map(|n| id_of.get(n).copied()).collect();
        if ids.len() == 2 {
            add_seg(ids[0], ids[1], &mut segs);
        } else if ids.len() >= 3 {
            for k in 0..ids.len() {
                add_seg(ids[k], ids[(k + 1) % ids.len()], &mut segs);
            }
        }
    }
    for pred in &problem.preds {
        for (a, b) in pairs(pred) {
            add_seg(a, b, &mut segs);
        }
    }
    for &(a, b) in &segs {
        let (pa, pb) = (scr[a as usize], scr[b as usize]);
        if !finite(pa) || !finite(pb) || covered(pa, pb) {
            continue;
        }
        let aux = is_aux(a) || is_aux(b);
        els.push(El {
            shape: Shape::Line(pa, pb),
            class: "f-seg",
            role: if aux { Role::Aux } else { Role::Base },
            dashed: aux,
            width: 1.3,
            pts: vec![dn(a), dn(b)],
            centre: None,
        });
    }

    // Right-angle marks.
    let right_angle = |a: u32, b: u32, c: u32, d: u32| -> Option<(String, Vec<Pt>)> {
        let (pa, pb, pc, pd) = (scr[a as usize], scr[b as usize], scr[c as usize], scr[d as usize]);
        let x = intersect(pa, pb, pc, pd)?;
        let near = scr.iter().any(|&p| finite(p) && len(sub(p, x)) < SPAN * 0.6);
        if !near {
            return None;
        }
        let far = |e1: Pt, e2: Pt| if len(sub(e1, x)) >= len(sub(e2, x)) { e1 } else { e2 };
        let u = unit(sub(far(pa, pb), x));
        let v = unit(sub(far(pc, pd), x));
        let t = 10.0;
        let p1 = add(x, mul(u, t));
        let p2 = add(add(x, mul(u, t)), mul(v, t));
        let p3 = add(x, mul(v, t));
        Some((format!("M{:.1},{:.1} L{:.1},{:.1} L{:.1},{:.1}", p1.0, p1.1, p2.0, p2.1, p3.0, p3.1), vec![p1, p2, p3]))
    };
    for pred in problem.preds.iter().filter(|p| p.name == "perp" && p.points.len() == 4) {
        let p = &pred.points;
        if let Some((d, pts)) = right_angle(p[0], p[1], p[2], p[3]) {
            els.push(El {
                shape: Shape::Path(d, pts),
                class: "f-mark",
                role: if p.iter().any(|&i| is_aux(i)) { Role::Aux } else { Role::Base },
                dashed: false,
                width: 1.2,
                pts: p.iter().map(|&i| dn(i)).collect(),
                centre: None,
            });
        }
    }

    // Equal-length ticks: segments joined by congruences share a tick count.
    {
        let mut seg_ids: Vec<(u32, u32)> = Vec::new();
        let idx = |s: (u32, u32), seg_ids: &mut Vec<(u32, u32)>| {
            let k = if s.0 < s.1 { s } else { (s.1, s.0) };
            match seg_ids.iter().position(|x| *x == k) {
                Some(i) => i,
                None => {
                    seg_ids.push(k);
                    seg_ids.len() - 1
                }
            }
        };
        let edges: Vec<(usize, usize)> = tick_pairs.iter().map(|(a, b)| (idx(*a, &mut seg_ids), idx(*b, &mut seg_ids))).collect();
        let mut uf = Uf((0..seg_ids.len()).collect());
        for (a, b) in edges {
            uf.join(a, b);
        }
        let mut class_of: HashMap<usize, usize> = HashMap::new();
        let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
        for i in 0..seg_ids.len() {
            let r = uf.find(i);
            members.entry(r).or_default().push(i);
        }
        let mut roots: Vec<usize> = members.keys().copied().filter(|r| members[r].len() >= 2).collect();
        roots.sort();
        for (k, r) in roots.iter().enumerate() {
            class_of.insert(*r, (k % 3) + 1);
        }
        for (i, &(a, b)) in seg_ids.iter().enumerate() {
            let r = uf.find(i);
            let Some(&count) = class_of.get(&r) else { continue };
            let (pa, pb) = (scr[a as usize], scr[b as usize]);
            if !finite(pa) || !finite(pb) || len(sub(pa, pb)) < 18.0 {
                continue;
            }
            let mid = mul(add(pa, pb), 0.5);
            let dir = unit(sub(pb, pa));
            let nrm = (-dir.1, dir.0);
            let mut d = String::new();
            let mut pts = Vec::new();
            for t in 0..count {
                let off = (t as f64 - (count as f64 - 1.0) / 2.0) * 4.0;
                let c = add(mid, mul(dir, off));
                let q1 = add(c, mul(nrm, 5.5));
                let q2 = sub(c, mul(nrm, 5.5));
                let _ = write!(d, "M{:.1},{:.1} L{:.1},{:.1} ", q1.0, q1.1, q2.0, q2.1);
                pts.push(q1);
                pts.push(q2);
            }
            els.push(El {
                shape: Shape::Path(d.trim().to_string(), pts),
                class: "f-mark",
                role: if is_aux(a) || is_aux(b) { Role::Aux } else { Role::Base },
                dashed: false,
                width: 1.3,
                pts: vec![dn(a), dn(b)],
                centre: None,
            });
        }
    }

    // Equal-angle arcs at shared vertices.
    let angle_arc = |a: u32, b: u32, c: u32, d: u32, r: f64| -> Option<(String, Vec<Pt>, Vec<u32>)> {
        let (mut a, mut b, mut c, mut d) = (a, b, c, d);
        if b == c || b == d {
            std::mem::swap(&mut a, &mut b);
        }
        if a == d {
            std::mem::swap(&mut c, &mut d);
        }
        if a != c {
            return None;
        }
        let v = scr[a as usize];
        let (d1, pts) = arc_path(v, scr[b as usize], scr[d as usize], r)?;
        Some((d1, pts, vec![a, b, d]))
    };
    let mut eq_count = 0usize;
    for pred in problem.preds.iter().filter(|p| p.name == "eqangle" && p.points.len() == 8) {
        eq_count += 1;
        let p = &pred.points;
        let r = 15.0 + 4.0 * ((eq_count - 1) % 3) as f64;
        for half in [&p[..4], &p[4..]] {
            if let Some((d, pts, ids)) = angle_arc(half[0], half[1], half[2], half[3], r) {
                els.push(El {
                    shape: Shape::Path(d, pts),
                    class: "f-mark",
                    role: if ids.iter().any(|&i| is_aux(i)) { Role::Aux } else { Role::Base },
                    dashed: false,
                    width: 1.3,
                    pts: ids.iter().map(|&i| dn(i)).collect(),
                    centre: None,
                });
            }
        }
    }

    // The goal, emphasised.
    let mut goal_els: Vec<El> = Vec::new();
    let gseg = |a: u32, b: u32, out: &mut Vec<El>| {
        let (pa, pb) = (scr[a as usize], scr[b as usize]);
        if a != b && finite(pa) && finite(pb) {
            out.push(El {
                shape: Shape::Line(pa, pb),
                class: "f-goal",
                role: Role::Goal,
                dashed: false,
                width: 2.4,
                pts: vec![dn(a), dn(b)],
                centre: None,
            });
        }
    };
    if let Some(goal) = &problem.goal {
        let p = &goal.points;
        let gp: Vec<String> = p.iter().map(|&i| dn(i)).collect();
        match (goal.name.as_str(), p.len()) {
            ("cyclic", k) if k >= 3 => {
                if let Some((ux, uy, r)) = circumcircle(scr[p[0] as usize], scr[p[1] as usize], scr[p[2] as usize]) {
                    if r < max_r {
                        goal_els.push(El {
                            shape: Shape::Circle((ux, uy), r),
                            class: "f-goal",
                            role: Role::Goal,
                            dashed: true,
                            width: 2.2,
                            pts: gp.clone(),
                            centre: None,
                        });
                    }
                }
            }
            ("coll", k) if k >= 2 => {
                let all: Vec<Pt> = p.iter().map(|&i| scr[i as usize]).collect();
                let mut far = (0.0, all[0], all[0]);
                for a in 0..all.len() {
                    for b in a + 1..all.len() {
                        let d = len(sub(all[a], all[b]));
                        if d > far.0 {
                            far = (d, all[a], all[b]);
                        }
                    }
                }
                goal_els.push(El {
                    shape: Shape::Line(far.1, far.2),
                    class: "f-goal",
                    role: Role::Goal,
                    dashed: false,
                    width: 2.4,
                    pts: gp.clone(),
                    centre: None,
                });
            }
            ("perp", 4) => {
                gseg(p[0], p[1], &mut goal_els);
                gseg(p[2], p[3], &mut goal_els);
                let (u, v) = (sub(scr[p[1] as usize], scr[p[0] as usize]), sub(scr[p[3] as usize], scr[p[2] as usize]));
                let cos = (u.0 * v.0 + u.1 * v.1).abs() / (len(u) * len(v)).max(1e-9);
                let holds = cos < 0.02;
                if let Some((d, pts)) = right_angle(p[0], p[1], p[2], p[3]).filter(|_| holds) {
                    goal_els.push(El { shape: Shape::Path(d, pts), class: "f-goal", role: Role::Goal, dashed: false, width: 1.8, pts: gp.clone(), centre: None });
                }
            }
            ("midp", 3) => {
                gseg(p[0], p[1], &mut goal_els);
                gseg(p[0], p[2], &mut goal_els);
            }
            ("eqangle", 8) => {
                for half in [&p[..4], &p[4..]] {
                    match angle_arc(half[0], half[1], half[2], half[3], 24.0) {
                        Some((d, pts, ids)) => {
                            gseg(ids[0], ids[1], &mut goal_els);
                            gseg(ids[0], ids[2], &mut goal_els);
                            goal_els.push(El {
                                shape: Shape::Path(d, pts),
                                class: "f-goal",
                                role: Role::Goal,
                                dashed: false,
                                width: 2.0,
                                pts: ids.iter().map(|&i| dn(i)).collect(),
                                centre: None,
                            });
                        }
                        None => {
                            gseg(half[0], half[1], &mut goal_els);
                            gseg(half[2], half[3], &mut goal_els);
                        }
                    }
                }
            }
            _ => {
                for c in p.chunks_exact(2) {
                    gseg(c[0], c[1], &mut goal_els);
                }
            }
        }
    } else if let Some(g) = &ex.metric_goal {
        let (ds, angs) = metric_pairs(g);
        for (a, b) in ds {
            if let (Some(&ia), Some(&ib)) = (id_of.get(&a), id_of.get(&b)) {
                gseg(ia, ib, &mut goal_els);
            }
        }
        for (a, b, c) in angs {
            if let (Some(&ia), Some(&ib), Some(&ic)) = (id_of.get(&a), id_of.get(&b), id_of.get(&c)) {
                gseg(ib, ia, &mut goal_els);
                gseg(ib, ic, &mut goal_els);
                if let Some((d, pts)) = arc_path(scr[ib as usize], scr[ia as usize], scr[ic as usize], 24.0) {
                    goal_els.push(El { shape: Shape::Path(d, pts), class: "f-goal", role: Role::Goal, dashed: false, width: 2.0, pts: vec![dn(ia), dn(ib), dn(ic)], centre: None });
                }
            }
        }
    }
    els.extend(goal_els);

    // Labels: every point, placed in its roomiest direction.
    let mut placed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let dots: Vec<Pt> = (0..n).map(|i| scr[i]).filter(|p| finite(*p)).collect();
    let mut labels: Vec<(usize, String, Pt, Pt)> = Vec::new();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (is_aux(i as u32), i));
    for i in order {
        let p = scr[i];
        if !finite(p) {
            continue;
        }
        let name = dn(i as u32);
        let mut dirs: Vec<f64> = Vec::new();
        for el in &els {
            match &el.shape {
                Shape::Line(a, b) => {
                    if dist_to_line(p, *a, *b) < 1.0 {
                        let along = unit(sub(*b, *a));
                        let ta = len(sub(p, *a));
                        let tb = len(sub(p, *b));
                        if ta > 2.0 {
                            dirs.push((-along.1).atan2(-along.0));
                        }
                        if tb > 2.0 {
                            dirs.push(along.1.atan2(along.0));
                        }
                    }
                }
                Shape::Circle(c, r) => {
                    if (len(sub(p, *c)) - r).abs() < 1.0 {
                        let rad = unit(sub(p, *c));
                        let tan = (-rad.1, rad.0);
                        dirs.push(tan.1.atan2(tan.0));
                        dirs.push((-tan.1).atan2(-tan.0));
                        dirs.push((-rad.1).atan2(-rad.0));
                    }
                }
                Shape::Path(..) => {}
            }
        }
        let chars = name.chars().count() as f64;
        let w = LABEL_FS * (0.55 * chars + 0.15);
        let h = LABEL_FS * 0.9;
        let mut best: Option<(f64, Pt)> = None;
        for k in 0..24 {
            let th = (k as f64) * std::f64::consts::PI / 12.0;
            let dv = (th.cos(), th.sin());
            let gap = dirs
                .iter()
                .map(|&a| {
                    let mut d = (a - th).abs() % (2.0 * std::f64::consts::PI);
                    if d > std::f64::consts::PI {
                        d = 2.0 * std::f64::consts::PI - d;
                    }
                    d
                })
                .fold(std::f64::consts::PI, f64::min);
            let reach = 7.0 + (dv.0.abs() * w / 2.0).max(dv.1.abs() * h / 2.0);
            let c = add(p, mul(dv, reach));
            let bx = (c.0 - w / 2.0, c.1 - h / 2.0, c.0 + w / 2.0, c.1 + h / 2.0);
            let mut penalty = 0.0;
            for q in &placed {
                let ox = (bx.2.min(q.2) - bx.0.max(q.0)).max(0.0);
                let oy = (bx.3.min(q.3) - bx.1.max(q.1)).max(0.0);
                penalty += ox * oy * 0.08;
            }
            for d in &dots {
                if (d.0 - p.0).abs() < 1e-6 && (d.1 - p.1).abs() < 1e-6 {
                    continue;
                }
                if d.0 > bx.0 - 4.0 && d.0 < bx.2 + 4.0 && d.1 > bx.1 - 4.0 && d.1 < bx.3 + 4.0 {
                    penalty += 6.0;
                }
            }
            for el in &els {
                if let Shape::Line(a, b) = &el.shape {
                    if dist_to_line(c, *a, *b) < h * 0.45 {
                        let t = {
                            let ab = sub(*b, *a);
                            let l2 = (ab.0 * ab.0 + ab.1 * ab.1).max(1e-9);
                            ((c.0 - a.0) * ab.0 + (c.1 - a.1) * ab.1) / l2
                        };
                        if (0.0..=1.0).contains(&t) {
                            penalty += 0.6;
                        }
                    }
                }
            }
            let score = gap.min(1.6) - penalty;
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, c));
            }
        }
        let (_, c) = best.unwrap_or((0.0, add(p, (10.0, -10.0))));
        placed.push((c.0 - w / 2.0, c.1 - h / 2.0, c.0 + w / 2.0, c.1 + h / 2.0));
        labels.push((i, name, p, sub(c, p)));
    }

    // Frame everything: shapes in full, every dot and label, plus padding.
    let (mut bx0, mut by0, mut bx1, mut by1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut grow = |p: Pt| {
        if finite(p) {
            bx0 = bx0.min(p.0);
            by0 = by0.min(p.1);
            bx1 = bx1.max(p.0);
            by1 = by1.max(p.1);
        }
    };
    for el in &els {
        match &el.shape {
            Shape::Line(a, b) => {
                grow(*a);
                grow(*b);
            }
            Shape::Circle(c, r) => {
                grow((c.0 - r, c.1 - r));
                grow((c.0 + r, c.1 + r));
            }
            Shape::Path(_, pts) => pts.iter().for_each(|&p| grow(p)),
        }
    }
    for &p in &dots {
        grow((p.0 - DOT_R, p.1 - DOT_R));
        grow((p.0 + DOT_R, p.1 + DOT_R));
    }
    for (_, name, p, off) in &labels {
        let c = add(*p, *off);
        let w = LABEL_FS * (0.55 * name.chars().count() as f64 + 0.15);
        grow((c.0 - w / 2.0, c.1 - LABEL_FS * 0.55));
        grow((c.0 + w / 2.0, c.1 + LABEL_FS * 0.55));
    }
    if bx0 > bx1 {
        (bx0, by0, bx1, by1) = (0.0, 0.0, SPAN, SPAN);
    }
    let (vx, vy) = (bx0 - PAD, by0 - PAD);
    let (vw, vh) = (bx1 - bx0 + 2.0 * PAD, by1 - by0 + 2.0 * PAD);

    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" class="gs-fig" viewBox="{vx:.1} {vy:.1} {vw:.1} {vh:.1}" width="{vw:.0}" height="{vh:.0}" font-family="'STIX Two Text', 'DejaVu Serif', Georgia, serif" data-base-fs="{LABEL_FS}" data-base-r="{DOT_R}">"#
    );
    let order = |e: &El| match (e.role, e.class) {
        (Role::Goal, _) => 5,
        (_, "f-circ") => 0,
        (_, "f-line") => 1,
        (_, "f-seg") => 2,
        (_, "f-mark") => 3,
        _ => 4,
    };
    let mut sorted: Vec<&El> = els.iter().collect();
    sorted.sort_by_key(|e| order(e));
    for el in sorted {
        let stroke = match (el.role, el.class) {
            (Role::Goal, _) => GOAL,
            (Role::Aux, _) => AUX,
            (_, "f-circ") => CIRCLE,
            (_, "f-line") => INK,
            (_, "f-mark") => MARK,
            _ => CONSTRUCTION,
        };
        let role = match el.role {
            Role::Base => "",
            Role::Aux => " f-aux",
            Role::Goal => "",
        };
        let dash = if el.dashed { r#" stroke-dasharray="7 5""# } else { "" };
        let data = {
            let mut d = format!(r#" data-p="{}""#, esc(&el.pts.join(" ")));
            if let Some(c) = &el.centre {
                let _ = write!(d, r#" data-c="{}""#, esc(c));
            }
            d
        };
        let common = format!(
            r#"class="{}{role}" fill="none" stroke="{stroke}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke"{dash}{data}"#,
            el.class, el.width
        );
        match &el.shape {
            Shape::Line(a, b) => {
                let _ = writeln!(s, r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" {common}/>"#, a.0, a.1, b.0, b.1);
            }
            Shape::Circle(c, r) => {
                let _ = writeln!(s, r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" {common}/>"#, c.0, c.1, r);
            }
            Shape::Path(d, _) => {
                let _ = writeln!(s, r#"<path d="{d}" {common}/>"#);
            }
        }
    }
    let mut dot_order: Vec<usize> = (0..n).collect();
    dot_order.sort_by_key(|&i| is_aux(i as u32));
    for i in dot_order {
        let p = scr[i];
        if !finite(p) {
            continue;
        }
        let aux = is_aux(i as u32);
        let _ = writeln!(
            s,
            r#"<circle class="f-dot{}" cx="{:.1}" cy="{:.1}" r="{DOT_R}" fill="{}" data-p="{}"/>"#,
            if aux { " f-aux" } else { "" },
            p.0,
            p.1,
            if aux { AUX } else { INK },
            esc(&dn(i as u32))
        );
    }
    for (i, name, p, off) in &labels {
        let aux = is_aux(*i as u32);
        let c = add(*p, *off);
        let _ = writeln!(
            s,
            r##"<text class="f-lbl{}" x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{LABEL_FS}" font-style="italic" fill="{}" stroke="#ffffff" stroke-width="4" stroke-linejoin="round" paint-order="stroke" data-p="{}" data-x="{:.1}" data-y="{:.1}" data-dx="{:.1}" data-dy="{:.1}">{}</text>"##,
            if aux { " f-aux" } else { "" },
            c.0,
            c.1 + LABEL_FS * 0.34,
            if aux { AUX } else { INK },
            esc(name),
            p.0,
            p.1,
            off.0,
            off.1,
            esc(name)
        );
    }
    s.push_str("</svg>\n");
    s
}
