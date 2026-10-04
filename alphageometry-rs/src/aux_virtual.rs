//! Auxiliary points on virtual lines (parallels, perpendiculars, tangents,
//! isogonal lines), on circles the closure proved, and harmonic conjugates.
//! Kept only when they land on enough extra figure objects (`aux_score`).

use crate::aux_score::{DefObj, Scorer};
use crate::aux_search::{
    circle3, coincident_args, double_key, known_circles, landing, natural_name, pred, render_tpl,
    salient_lines, taken_names, tok, Construction, Kind, Landing,
};
use crate::numerics::{distance, intersect_ll, NumCircle, NumLine, Vec2};
use crate::predicate::{PointId, Predicate};
use crate::rational::Rat;
use crate::{Ddar, Problem};
use rayon::prelude::*;
use rustc_hash::FxHashSet;

type P = PointId;

#[derive(Clone, Copy, Debug)]
enum LineDef {
    Through(P, P),
    Para(P, P, P),
    Perp(P, P, P),
    Iso(P, P, P, P),
}

#[derive(Clone, Copy, Debug)]
enum CircDef {
    Centered(P, P),
    Through(P, P, P),
}

#[derive(Clone, Copy, Debug)]
struct VLine {
    num: NumLine,
    dir: Vec2,
    def: LineDef,
}

#[derive(Clone, Copy, Debug)]
struct VCircle {
    num: NumCircle,
    def: CircDef,
    proved: bool,
}

impl LineDef {
    fn on(self, x: P) -> Predicate {
        match self {
            LineDef::Through(a, b) => pred("coll", vec![a, b, x]),
            LineDef::Para(p, a, b) => pred("para", vec![p, x, a, b]),
            LineDef::Perp(p, a, b) => pred("perp", vec![p, x, a, b]),
            LineDef::Iso(v, a, b, q) => pred("eqangle", vec![v, a, v, x, v, q, v, b]),
        }
    }
    fn tpl(self) -> String {
        match self {
            LineDef::Through(a, b) => format!("{}{}", tok(a), tok(b)),
            LineDef::Para(p, a, b) => format!("para({}, {}{})", tok(p), tok(a), tok(b)),
            LineDef::Perp(p, a, b) if p == a || p == b => {
                let o = if p == a { b } else { a };
                format!("tangent_at({}, centre {})", tok(p), tok(o))
            }
            LineDef::Perp(p, a, b) => format!("perp({}, {}{})", tok(p), tok(a), tok(b)),
            LineDef::Iso(v, a, b, q) => {
                format!(
                    "isogonal({}{} in {}{}{})",
                    tok(v),
                    tok(q),
                    tok(a),
                    tok(v),
                    tok(b)
                )
            }
        }
    }
    fn points(self) -> Vec<P> {
        match self {
            LineDef::Through(a, b) => vec![a, b],
            LineDef::Para(p, a, b) | LineDef::Perp(p, a, b) => vec![p, a, b],
            LineDef::Iso(v, a, b, q) => vec![v, a, b, q],
        }
    }
    fn kind(self) -> Kind {
        match self {
            LineDef::Through(..) => Kind::IntersectLL,
            LineDef::Para(..) => Kind::ParaMeet,
            LineDef::Perp(p, a, b) if p == a || p == b => Kind::TangentMeet,
            LineDef::Perp(..) => Kind::PerpMeet,
            LineDef::Iso(..) => Kind::IsogonalMeet,
        }
    }
}

impl CircDef {
    fn on(self, x: P) -> Predicate {
        match self {
            CircDef::Centered(o, a) => pred("cong", vec![o, x, o, a]),
            CircDef::Through(a, b, c) => pred("cyclic", vec![a, b, c, x]),
        }
    }
    fn tpl(self) -> String {
        match self {
            CircDef::Centered(o, a) => format!("circle({},{})", tok(o), tok(a)),
            CircDef::Through(a, b, c) => format!("circumcircle({},{},{})", tok(a), tok(b), tok(c)),
        }
    }
    fn points(self) -> Vec<P> {
        match self {
            CircDef::Centered(o, a) => vec![o, a],
            CircDef::Through(a, b, c) => vec![a, b, c],
        }
    }
}

struct Raw {
    score: f64,
    coord: Vec2,
    preds: Vec<Predicate>,
    tpl: String,
    kind: Kind,
    args: Vec<P>,
}

fn cross(u: Vec2, v: Vec2) -> f64 {
    u.x * v.y - u.y * v.x
}

fn line_circle(l: &NumLine, c: &NumCircle) -> Vec<Vec2> {
    let foot = l.n * l.c;
    let d = c.center - foot;
    let along = Vec2::new(-l.n.y, l.n.x);
    let off = c.center.dot(l.n) - l.c;
    let h2 = c.r * c.r - off * off;
    if h2 <= (1e-6 * c.r).powi(2) {
        return Vec::new();
    }
    let h = h2.sqrt();
    let base = foot + along * d.dot(along);
    vec![base + along * h, base - along * h]
}

fn circle_circle(a: &NumCircle, b: &NumCircle) -> Vec<Vec2> {
    let dv = b.center - a.center;
    let d = dv.norm();
    if d < 1e-9 || d > a.r + b.r || d < (a.r - b.r).abs() {
        return Vec::new();
    }
    let t = (a.r * a.r - b.r * b.r + d * d) / (2.0 * d);
    let h2 = a.r * a.r - t * t;
    if h2 <= (1e-6 * a.r.min(b.r)).powi(2) {
        return Vec::new();
    }
    let h = h2.sqrt();
    let mid = a.center + dv * (t / d);
    let perp = Vec2::new(-dv.y, dv.x) * (1.0 / d);
    vec![mid + perp * h, mid - perp * h]
}

/// New-kind candidates over `problem` whose coincidence score reaches
/// `min_score`, as `(score, construction)`. With `must`, only those built on
/// that point.
pub(crate) fn virtual_candidates(
    problem: &Problem,
    ddar: Option<&Ddar>,
    scorer: &Scorer,
    min_score: f64,
    must: Option<P>,
    parallel: bool,
    doubles: bool,
) -> Vec<(f64, Construction)> {
    let n = problem.points.len();
    let new_id = n as P;
    let c = |i: P| problem.points[i as usize].value;
    let scale = scorer.scale();
    let tol = 1e-9 * scale;
    let involves = |pts: &[P]| must.is_none_or(|m| pts.contains(&m));

    let mut tlines: Vec<VLine> = Vec::new();
    let push_line =
        |v: &mut Vec<VLine>, seen: &mut FxHashSet<(i64, i64)>, def: LineDef, num: NumLine| {
            if !num.n.x.is_finite() || !num.n.y.is_finite() {
                return;
            }
            let sign = if num.n.y > 0.0 || (num.n.y == 0.0 && num.n.x > 0.0) {
                1.0
            } else {
                -1.0
            };
            let (n, c0) = (num.n * sign, num.c * sign);
            let key = (
                (n.y.atan2(n.x) * 1e8).round() as i64,
                (c0 / scale * 1e8).round() as i64,
            );
            if !seen.insert(key) {
                return;
            }
            let dir = Vec2::new(-num.n.y, num.n.x);
            v.push(VLine { num, dir, def });
        };
    let mut tseen: FxHashSet<(i64, i64)> = FxHashSet::default();
    let mut vseen: FxHashSet<(i64, i64)> = FxHashSet::default();
    let mut salient = salient_lines(problem);
    salient.sort();
    for &(a, b) in &salient {
        if distance(c(a), c(b)) > tol {
            push_line(
                &mut tlines,
                &mut tseen,
                LineDef::Through(a, b),
                NumLine::through(c(a), c(b)),
            );
        }
    }
    if let Some(d) = ddar {
        for l in d.proved_lines() {
            let l: Vec<P> = l.into_iter().filter(|&p| (p as usize) < n).collect();
            if l.len() >= 2 && distance(c(l[0]), c(l[1])) > tol {
                push_line(
                    &mut tlines,
                    &mut tseen,
                    LineDef::Through(l[0], l[1]),
                    NumLine::through(c(l[0]), c(l[1])),
                );
            }
        }
    }

    let mut circles: Vec<VCircle> = Vec::new();
    let push_circle = |v: &mut Vec<VCircle>, def: CircDef, num: NumCircle, proved: bool| {
        if !num.r.is_finite() || num.r > 1e3 * scale || num.r < tol {
            return;
        }
        if v.iter().any(|k| {
            distance(k.num.center, num.center) < 1e-8 * scale
                && (k.num.r - num.r).abs() < 1e-8 * scale
        }) {
            return;
        }
        v.push(VCircle { num, def, proved });
    };
    for (o, a) in known_circles(problem) {
        push_circle(
            &mut circles,
            CircDef::Centered(o, a),
            NumCircle::through1(c(o), c(a)),
            false,
        );
    }
    let sal: FxHashSet<(P, P)> = salient.iter().copied().collect();
    let sp = |a: P, b: P| sal.contains(&if a < b { (a, b) } else { (b, a) });
    for x in 0..n as P {
        for y in x + 1..n as P {
            if !sp(x, y) {
                continue;
            }
            for z in y + 1..n as P {
                if sp(x, z) && sp(y, z) {
                    if let Some(k) = circle3(c(x), c(y), c(z)) {
                        push_circle(&mut circles, CircDef::Through(x, y, z), k, false);
                    }
                }
            }
        }
    }
    if let Some(d) = ddar {
        for (pts, centers) in d.proved_circles() {
            let pts: Vec<P> = pts.into_iter().filter(|&p| (p as usize) < n).collect();
            let center = centers.into_iter().find(|&p| (p as usize) < n);
            match (center, pts.len()) {
                (Some(o), l) if l >= 1 => push_circle(
                    &mut circles,
                    CircDef::Centered(o, pts[0]),
                    NumCircle::through1(c(o), c(pts[0])),
                    true,
                ),
                (_, l) if l >= 3 => {
                    if let Some(k) = NumCircle::through(c(pts[0]), c(pts[1]), c(pts[2])) {
                        push_circle(
                            &mut circles,
                            CircDef::Through(pts[0], pts[1], pts[2]),
                            k,
                            true,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    let mut vlines: Vec<VLine> = Vec::new();
    for p in 0..n as P {
        for t in &tlines {
            let LineDef::Through(a, b) = t.def else {
                continue;
            };
            if t.num.distance(c(p)) > tol {
                push_line(
                    &mut vlines,
                    &mut vseen,
                    LineDef::Para(p, a, b),
                    NumLine::through1(t.num.n, c(p)),
                );
            }
            push_line(
                &mut vlines,
                &mut vseen,
                LineDef::Perp(p, a, b),
                NumLine::through1(t.dir, c(p)),
            );
        }
    }
    for k in &circles {
        let CircDef::Centered(o, _) = k.def else {
            continue;
        };
        for t in 0..n as P {
            if t != o && k.num.distance(c(t)) < 1e-8 * scale {
                push_line(
                    &mut vlines,
                    &mut vseen,
                    LineDef::Perp(t, o, t),
                    NumLine::through1((c(t) - c(o)).normalize(), c(t)),
                );
            }
        }
    }
    let angle = |v: Vec2| v.y.atan2(v.x);
    for v in 0..n as P {
        let nb: Vec<P> = (0..n as P)
            .filter(|&u| u != v && sp(u, v) && distance(c(u), c(v)) > tol)
            .collect();
        for ai in 0..nb.len() {
            for bi in ai + 1..nb.len() {
                for &q in &nb {
                    let (a, b) = (nb[ai], nb[bi]);
                    if q == a || q == b {
                        continue;
                    }
                    let th = angle(c(a) - c(v)) + angle(c(b) - c(v)) - angle(c(q) - c(v));
                    let dir = Vec2::new(th.cos(), th.sin());
                    if cross(dir, (c(q) - c(v)).normalize()).abs() < 1e-9 {
                        continue;
                    }
                    push_line(
                        &mut vlines,
                        &mut vseen,
                        LineDef::Iso(v, a, b, q),
                        NumLine::through1(dir.perp_rot(), c(v)),
                    );
                }
            }
        }
    }

    let keep = |x: Vec2, defs: [DefObj; 2]| -> Option<(f64, Vec2)> {
        if !x.x.is_finite() || !x.y.is_finite() || distance(x, c(0)) > 100.0 * scale {
            return None;
        }
        match landing(problem, x, scale, 1e-6 * scale.max(1.0)) {
            Landing::Clear => {
                let s = scorer.score(x, &defs);
                (s >= min_score).then_some((s, x))
            }
            Landing::On(p) if doubles => Some((0.0, c(p))),
            _ => None,
        }
    };

    let t_pts: Vec<Vec<P>> = tlines.iter().map(|t| t.def.points()).collect();
    let k_pts: Vec<Vec<P>> = circles.iter().map(|k| k.def.points()).collect();
    let t_inv: Vec<bool> = t_pts.iter().map(|p| involves(p)).collect();
    let k_inv: Vec<bool> = k_pts.iter().map(|p| involves(p)).collect();
    let per_line = |v: &VLine| {
        let mut out: Vec<Raw> = Vec::new();
        let vp = v.def.points();
        let v_inv = involves(&vp);
        for (ti, t) in tlines.iter().enumerate() {
            if !(v_inv || t_inv[ti]) || cross(v.dir, t.dir).abs() < 1e-6 {
                continue;
            }
            let tp = &t_pts[ti];
            let Some(x) = intersect_ll(&v.num, &t.num) else {
                continue;
            };

            if let Some((score, x)) = keep(x, [DefObj::Line(v.dir), DefObj::Line(t.dir)]) {
                out.push(Raw {
                    score,
                    coord: x,
                    preds: vec![v.def.on(new_id), t.def.on(new_id)],
                    tpl: format!("intersect({}, {})", v.def.tpl(), t.def.tpl()),
                    kind: v.def.kind(),
                    args: {
                        let mut a = vp.clone();
                        a.extend(tp.iter().copied());
                        a
                    },
                });
            }
        }
        for (ki, k) in circles.iter().enumerate() {
            if !(v_inv || k_inv[ki]) {
                continue;
            }
            let kp = &k_pts[ki];
            for x in line_circle(&v.num, &k.num) {
                if let Some((score, x)) = keep(
                    x,
                    [DefObj::Line(v.dir), DefObj::Circle(k.num.center, k.num.r)],
                ) {
                    out.push(Raw {
                        score,
                        coord: x,
                        preds: vec![v.def.on(new_id), k.def.on(new_id)],
                        tpl: format!("intersect({}, {})", v.def.tpl(), k.def.tpl()),
                        kind: v.def.kind(),
                        args: {
                            let mut a = vp.clone();
                            a.extend(kp.iter().copied());
                            a
                        },
                    });
                }
            }
        }
        out
    };
    let mut raws: Vec<Raw> = if parallel {
        vlines.par_iter().flat_map_iter(per_line).collect()
    } else {
        vlines.iter().flat_map(per_line).collect()
    };

    for (ki, k) in circles.iter().enumerate() {
        if !k.proved {
            continue;
        }
        let kp = k.def.points();
        for t in &tlines {
            let tp = t.def.points();
            if !(involves(&kp) || involves(&tp)) {
                continue;
            }
            for x in line_circle(&t.num, &k.num) {
                if let Some((score, x)) = keep(
                    x,
                    [DefObj::Line(t.dir), DefObj::Circle(k.num.center, k.num.r)],
                ) {
                    raws.push(Raw {
                        score,
                        coord: x,
                        preds: vec![t.def.on(new_id), k.def.on(new_id)],
                        tpl: format!("intersect({}, {})", t.def.tpl(), k.def.tpl()),
                        kind: Kind::IntersectLCircum,
                        args: {
                            let mut a = tp.clone();
                            a.extend(kp.iter().copied());
                            a
                        },
                    });
                }
            }
        }
        for (ji, j) in circles.iter().enumerate() {
            if ji == ki || (j.proved && ji < ki) {
                continue;
            }
            let jp = j.def.points();
            if !(involves(&kp) || involves(&jp)) {
                continue;
            }
            for x in circle_circle(&k.num, &j.num) {
                if let Some((score, x)) = keep(
                    x,
                    [
                        DefObj::Circle(k.num.center, k.num.r),
                        DefObj::Circle(j.num.center, j.num.r),
                    ],
                ) {
                    raws.push(Raw {
                        score,
                        coord: x,
                        preds: vec![k.def.on(new_id), j.def.on(new_id)],
                        tpl: format!("intersect({}, {})", k.def.tpl(), j.def.tpl()),
                        kind: Kind::CircleCircle,
                        args: {
                            let mut a = kp.clone();
                            a.extend(jp.iter().copied());
                            a
                        },
                    });
                }
            }
        }
    }

    for t in &tlines {
        let on: Vec<P> = (0..n as P)
            .filter(|&p| t.num.distance(c(p)) < tol * 10.0)
            .collect();
        if on.len() < 3 {
            continue;
        }
        let pos = |p: P| c(p).dot(t.dir);
        for ai in 0..on.len() {
            for bi in ai + 1..on.len() {
                for &cc in &on {
                    let (a, b) = (on[ai], on[bi]);
                    if cc == a || cc == b || !involves(&[a, b, cc]) {
                        continue;
                    }
                    let (sa, sb, sc) = (pos(a), pos(b), pos(cc));
                    let den = sa + sb - 2.0 * sc;
                    if den.abs() < 1e-6 * scale {
                        continue;
                    }
                    let sx = (2.0 * sa * sb - sc * (sa + sb)) / den;
                    let x = c(a) + t.dir * (sx - sa);
                    let ratio = Predicate {
                        name: "eqratio".to_string(),
                        points: vec![cc, a, cc, b, new_id, a, new_id, b],
                        constants: Vec::<Rat>::new(),
                    };
                    if let Some((score, x)) = keep(x, [DefObj::Line(t.dir), DefObj::Line(t.dir)]) {
                        raws.push(Raw {
                            score,
                            coord: x,
                            preds: vec![pred("coll", vec![a, b, new_id]), ratio],
                            tpl: format!("harmonic({} wrt {},{})", tok(cc), tok(a), tok(b)),
                            kind: Kind::Harmonic,
                            args: vec![cc, a, b],
                        });
                    }
                }
            }
        }
    }

    let taken = taken_names(problem);
    let real_name = |i: P| problem.points[i as usize].name.clone();
    let mut seen: FxHashSet<(Kind, i64, i64)> = FxHashSet::default();
    let mut out = Vec::new();
    for r in raws {
        if coincident_args(problem, &r.args, scale) {
            continue;
        }
        let on = (0..n as P).find(|&i| c(i) == r.coord);
        let key = match on {
            None => (
                r.kind,
                (r.coord.x * 1e6).round() as i64,
                (r.coord.y * 1e6).round() as i64,
            ),
            Some(p) => double_key(r.kind, p, &r.args),
        };
        if !seen.insert(key) {
            continue;
        }
        let name = natural_name(r.kind, &r.args, problem, &taken, new_id);
        out.push((
            r.score,
            Construction {
                name,
                coord: r.coord,
                preds: r.preds,
                desc: render_tpl(&r.tpl, real_name),
                kind: r.kind,
                args: r.args,
                tpl: r.tpl,
            },
        ));
    }
    out
}
