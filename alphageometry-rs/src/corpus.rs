//! Reader for the original AlphaGeometry (AG1) problem corpus.
//!
//! `corpus/imo_ag_30.txt` and `corpus/jgex_ag_231.txt` are written in AG1's
//! construction language: one clause per new point (or point group), each a
//! list of named constructions from `corpus/defs.txt`, then `?` and the goal:
//!
//! ```text
//! a b c = triangle a b c; h = on_tline h a b c, on_tline h b c a ? perp a h b c
//! ```
//!
//! Translation is faithful to AG1's own problem builder:
//!
//! * the **premises** are exactly the predicates `defs.txt` lists for each
//!   construction (its fourth line), with the arguments substituted — nothing
//!   is added and nothing is dropped;
//! * **numerics** follow `defs.txt`'s fifth line: each construction yields a
//!   locus (line, circle) or explicit points, the loci of one point are
//!   intersected, and a sample is rejected when a new point lands too close to
//!   an existing one or too far away, as AG1's `numericals.py` does;
//! * like AG1's `build_problem`, the figure is re-sampled until the goal holds
//!   numerically (a failure after every attempt is reported, never hidden);
//! * construction arguments may omit the new points (`a1 = on_line b c`), in
//!   which case they are prepended, exactly as AG1's `add_clause` does.
//!
//! Every premise is checked numerically on the sampled figure before it is
//! handed to the engine, so a mis-ported construction cannot smuggle a false
//! hypothesis into a proof: the sample is rejected instead. Unknown
//! constructions, sketches and predicates are errors — nothing is skipped.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::numerics::Vec2;
use crate::predicate::{Point, PointId, Predicate, Problem};
use crate::rational::Rat;

/// AG1's construction definitions, bundled from `corpus/defs.txt`.
pub const DEFS_TXT: &str = include_str!("../../corpus/defs.txt");

/// A name applied to arguments: a construction use, a predicate, or a sketch.
#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub name: String,
    pub args: Vec<String>,
}

impl Term {
    fn parse(s: &str) -> Option<Term> {
        let mut it = s.split_whitespace();
        let name = it.next()?.to_string();
        Some(Term {
            name,
            args: it.map(str::to_string).collect(),
        })
    }

    fn subst(&self, map: &HashMap<&str, &str>) -> Term {
        Term {
            name: self.name.clone(),
            args: self
                .args
                .iter()
                .map(|a| map.get(a.as_str()).map_or_else(|| a.clone(), |s| s.to_string()))
                .collect(),
        }
    }
}

impl std::fmt::Display for Term {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)?;
        for a in &self.args {
            write!(f, " {a}")?;
        }
        Ok(())
    }
}

/// One construction of `defs.txt`.
#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    /// Header arguments, in order.
    pub args: Vec<String>,
    /// Arguments that must already exist (left of `=` on the third line).
    pub inputs: Vec<String>,
    /// Arguments the construction creates, in header order.
    pub new_points: Vec<String>,
    /// Non-degeneracy / applicability conditions on the inputs.
    pub preconditions: Vec<Term>,
    /// The predicates the construction asserts (all levels, flattened).
    pub premises: Vec<Term>,
    /// Numeric sketches whose loci/points place the new points.
    pub numerics: Vec<Term>,
}

/// Parse a `defs.txt` text: blocks of five lines (header, dependency graph,
/// `inputs = preconditions`, premises, numerics) separated by blank lines.
pub fn parse_definitions(text: &str) -> Result<HashMap<String, Definition>, String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        if i + 5 > lines.len() {
            return Err(format!("defs: truncated definition at line {}", i + 1));
        }
        let header = Term::parse(lines[i]).ok_or("defs: empty header")?;
        let (inp, conds) = lines[i + 2]
            .split_once('=')
            .ok_or_else(|| format!("defs: `{}` has no `=` line", header.name))?;
        let inputs: Vec<String> = inp.split_whitespace().map(str::to_string).collect();
        let preconditions: Vec<Term> = conds.split(',').filter_map(Term::parse).collect();
        let mut premises = Vec::new();
        for level in lines[i + 3].split(';') {
            let body = match level.split_once(':') {
                Some((_, b)) => b,
                None => level,
            };
            premises.extend(body.split(',').filter_map(Term::parse));
        }
        let numerics: Vec<Term> = lines[i + 4].split(',').filter_map(Term::parse).collect();
        let new_points: Vec<String> = header
            .args
            .iter()
            .filter(|a| !inputs.contains(a))
            .cloned()
            .collect();
        for a in &inputs {
            if !header.args.contains(a) {
                return Err(format!("defs: `{}` input `{a}` is not an argument", header.name));
            }
        }
        out.insert(
            header.name.clone(),
            Definition {
                name: header.name,
                args: header.args,
                inputs,
                new_points,
                preconditions,
                premises,
                numerics,
            },
        );
        i += 5;
    }
    Ok(out)
}

/// The bundled definitions, parsed once.
pub fn definitions() -> &'static HashMap<String, Definition> {
    static DEFS: OnceLock<HashMap<String, Definition>> = OnceLock::new();
    DEFS.get_or_init(|| parse_definitions(DEFS_TXT).expect("bundled defs.txt parses"))
}

/// A point introduced by a clause, optionally pinned to given coordinates
/// (`x@4.96_-0.13`).
#[derive(Clone, Debug)]
pub struct ClausePoint {
    pub name: String,
    pub coord: Option<Vec2>,
}

/// One `points = construction, construction` clause.
#[derive(Clone, Debug)]
pub struct Clause {
    pub points: Vec<ClausePoint>,
    /// Constructions with their argument lists completed to the definition's
    /// arity.
    pub constructions: Vec<Term>,
}

/// A parsed corpus problem.
#[derive(Clone, Debug)]
pub struct AgProblem {
    pub name: String,
    pub clauses: Vec<Clause>,
    pub goal: Term,
}

/// Split a corpus file into `(name, problem text)` pairs: non-empty lines
/// alternate between a problem name and its statement.
pub fn read_corpus(text: &str) -> Result<Vec<(String, String)>, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if !lines.len().is_multiple_of(2) {
        return Err(format!(
            "corpus has an odd number of non-empty lines ({}); expected name/statement pairs",
            lines.len()
        ));
    }
    Ok(lines
        .chunks(2)
        .map(|c| (c[0].to_string(), c[1].to_string()))
        .collect())
}

fn parse_point_token(tok: &str) -> Result<ClausePoint, String> {
    match tok.split_once('@') {
        None => Ok(ClausePoint {
            name: tok.to_string(),
            coord: None,
        }),
        Some((name, val)) => {
            let (xs, ys) = val
                .split_once('_')
                .ok_or_else(|| format!("bad point coordinate `{tok}`"))?;
            let x: f64 = xs.parse().map_err(|_| format!("bad x in `{tok}`"))?;
            let y: f64 = ys.parse().map_err(|_| format!("bad y in `{tok}`"))?;
            Ok(ClausePoint {
                name: name.to_string(),
                coord: Some(Vec2::new(x, y)),
            })
        }
    }
}

/// Parse one problem statement. Unknown constructions and arity mismatches are
/// errors (with the expected form, as AG1 reports them).
pub fn parse_problem(name: &str, text: &str) -> Result<AgProblem, String> {
    let defs = definitions();
    let (body, goal) = text
        .split_once('?')
        .ok_or_else(|| "problem has no `?` goal".to_string())?;
    let goal = Term::parse(goal).ok_or("empty goal")?;
    let mut clauses = Vec::new();
    for raw in body.split(';') {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        let (pts, cons) = raw
            .split_once('=')
            .ok_or_else(|| format!("clause without `=`: `{raw}`"))?;
        let points = pts
            .split_whitespace()
            .map(parse_point_token)
            .collect::<Result<Vec<_>, _>>()?;
        if points.is_empty() {
            return Err(format!("clause introduces no point: `{raw}`"));
        }
        let mut constructions = Vec::new();
        for c in cons.split(',') {
            let Some(mut t) = Term::parse(c) else {
                continue;
            };
            let def = defs
                .get(&t.name)
                .ok_or_else(|| format!("unknown construction `{}`", t.name))?;
            if t.args.len() != def.args.len() {
                if def.args.len() == t.args.len() + points.len() {
                    let mut full: Vec<String> = points.iter().map(|p| p.name.clone()).collect();
                    full.append(&mut t.args);
                    t.args = full;
                } else {
                    return Err(format!(
                        "argument mismatch in `{}`: expected `{} = {} {}`",
                        t,
                        def.new_points.join(" "),
                        def.name,
                        def.args.join(" ")
                    ));
                }
            }
            constructions.push(t);
        }
        if constructions.is_empty() {
            return Err(format!("clause has no construction: `{raw}`"));
        }
        clauses.push(Clause {
            points,
            constructions,
        });
    }
    Ok(AgProblem {
        name: name.to_string(),
        clauses,
        goal,
    })
}

#[derive(Clone, Copy, Debug)]
enum Obj {
    Line { p: Vec2, d: Vec2 },
    Ray { p: Vec2, d: Vec2 },
    Circle { c: Vec2, r: f64 },
}

enum Sketch {
    Locus(Obj),
    Points(Vec<Vec2>),
}

#[derive(Debug)]
enum Fail {
    Retry(String),
    Fatal(String),
}

type Res<T> = Result<T, Fail>;

fn retry<T>(m: impl Into<String>) -> Res<T> {
    Err(Fail::Retry(m.into()))
}

fn fatal<T>(m: impl Into<String>) -> Res<T> {
    Err(Fail::Fatal(m.into()))
}

fn v(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}
fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn rot(a: Vec2, t: f64) -> Vec2 {
    let (s, c) = t.sin_cos();
    v(a.x * c - a.y * s, a.x * s + a.y * c)
}
fn rot90(a: Vec2) -> Vec2 {
    v(-a.y, a.x)
}
fn ang(a: Vec2) -> f64 {
    a.y.atan2(a.x)
}
fn unit_at(t: f64) -> Vec2 {
    v(t.cos(), t.sin())
}
fn unit(a: Vec2) -> Res<Vec2> {
    let n = a.norm();
    if n.is_nan() || n <= 1e-12 {
        return retry("zero-length direction");
    }
    Ok(a * (1.0 / n))
}
fn mid(a: Vec2, b: Vec2) -> Vec2 {
    (a + b) * 0.5
}
fn foot(p: Vec2, a: Vec2, d: Vec2) -> Vec2 {
    let t = (p - a).dot(d) / d.dot(d);
    a + d * t
}
fn line(a: Vec2, b: Vec2) -> Res<Obj> {
    if (b - a).norm() < 1e-12 {
        return retry("line through coincident points");
    }
    Ok(Obj::Line { p: a, d: b - a })
}
fn line_dir(p: Vec2, d: Vec2) -> Res<Obj> {
    if d.norm().is_nan() || d.norm() <= 1e-12 {
        return retry("line with no direction");
    }
    Ok(Obj::Line { p, d })
}
fn circumcenter(a: Vec2, b: Vec2, c: Vec2) -> Res<Vec2> {
    let l1 = Obj::Line {
        p: mid(a, b),
        d: rot90(b - a),
    };
    let l2 = Obj::Line {
        p: mid(a, c),
        d: rot90(c - a),
    };
    match intersect(&l1, &l2).as_slice() {
        [x] => Ok(*x),
        _ => retry("circumcircle of collinear points"),
    }
}

fn intersect(o1: &Obj, o2: &Obj) -> Vec<Vec2> {
    use Obj::*;
    let as_line = |o: &Obj| match *o {
        Line { p, d } | Ray { p, d } => Some((p, d)),
        Circle { .. } => None,
    };
    let on_ray = |o: &Obj, x: Vec2| match *o {
        Ray { p, d } => (x - p).dot(d) >= -1e-12,
        _ => true,
    };
    let raw: Vec<Vec2> = match (as_line(o1), as_line(o2), o1, o2) {
        (Some((p1, d1)), Some((p2, d2)), _, _) => {
            let den = cross(d1, d2);
            if den.abs() < 1e-9 * d1.norm() * d2.norm() {
                vec![]
            } else {
                let t = cross(p2 - p1, d2) / den;
                vec![p1 + d1 * t]
            }
        }
        (Some((p, d)), None, _, Circle { c, r }) | (None, Some((p, d)), Circle { c, r }, _) => {
            let f = foot(*c, p, d);
            let h2 = r * r - (f - *c).dot(f - *c);
            if h2 < -1e-12 * r * r {
                vec![]
            } else {
                let h = h2.max(0.0).sqrt();
                let u = d * (1.0 / d.norm());
                vec![f + u * h, f - u * h]
            }
        }
        (None, None, Circle { c: c1, r: r1 }, Circle { c: c2, r: r2 }) => {
            let dv = *c2 - *c1;
            let dd = dv.norm();
            if dd < 1e-12 {
                vec![]
            } else {
                let a = (r1 * r1 - r2 * r2 + dd * dd) / (2.0 * dd);
                let h2 = r1 * r1 - a * a;
                if h2 < -1e-12 * r1 * r1 {
                    vec![]
                } else {
                    let h = h2.max(0.0).sqrt();
                    let u = dv * (1.0 / dd);
                    let base = *c1 + u * a;
                    vec![base + rot90(u) * h, base - rot90(u) * h]
                }
            }
        }
        _ => unreachable!(),
    };
    raw.into_iter()
        .filter(|&x| on_ray(o1, x) && on_ray(o2, x) && x.x.is_finite() && x.y.is_finite())
        .collect()
}

fn obj_distance(o: &Obj, x: Vec2) -> f64 {
    match *o {
        Obj::Line { p, d } | Obj::Ray { p, d } => cross(d, x - p).abs() / d.norm(),
        Obj::Circle { c, r } => ((x - c).norm() - r).abs(),
    }
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03 | 1);
        for _ in 0..4 {
            r.next();
        }
        r
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
}

struct Figure {
    names: Vec<String>,
    coords: HashMap<String, Vec2>,
}

impl Figure {
    fn pts(&self) -> impl Iterator<Item = Vec2> + '_ {
        self.names.iter().map(|n| self.coords[n])
    }
    fn centroid(&self) -> Vec2 {
        let n = self.names.len().max(1) as f64;
        self.pts().fold(v(0.0, 0.0), |a, b| a + b) * (1.0 / n)
    }
    fn radius(&self) -> f64 {
        if self.names.len() < 2 {
            return 1.0;
        }
        let c = self.centroid();
        self.pts().map(|p| (p - c).norm()).fold(0.0, f64::max).max(1e-9)
    }
    fn random_point(&self, rng: &mut Rng) -> Vec2 {
        if self.names.len() < 2 {
            return v(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        }
        let c = self.centroid();
        let r = self.radius();
        c + v(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * r
    }
    fn sample_on(&self, o: &Obj, rng: &mut Rng) -> Vec2 {
        let r = self.radius();
        match *o {
            Obj::Line { p, d } => {
                let u = d * (1.0 / d.norm());
                foot(self.centroid(), p, d) + u * (rng.range(-1.0, 1.0) * r)
            }
            Obj::Ray { p, d } => {
                let u = d * (1.0 / d.norm());
                p + u * (rng.range(0.3, 1.2) * r)
            }
            Obj::Circle { c, r: cr } => c + unit_at(rng.range(0.0, 2.0 * PI)) * cr,
        }
    }
    fn placement_ok(&self, p: Vec2) -> bool {
        if !(p.x.is_finite() && p.y.is_finite()) {
            return false;
        }
        if self.names.is_empty() {
            return true;
        }
        let r = self.radius();
        if self.pts().any(|q| (p - q).norm() < 0.02 * r) {
            return false;
        }
        self.names.len() < 2 || (p - self.centroid()).norm() <= 10.0 * r
    }
}

fn random_triangle(rng: &mut Rng) -> [Vec2; 3] {
    loop {
        let a = v(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        let b = v(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        let c = v(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0));
        let min_side = (a - b).norm().min((b - c).norm()).min((c - a).norm());
        if min_side < 0.4 {
            continue;
        }
        let angle = |p: Vec2, q: Vec2, r: Vec2| {
            let (u, w) = (q - p, r - p);
            (u.dot(w) / (u.norm() * w.norm())).clamp(-1.0, 1.0).acos()
        };
        let m = angle(a, b, c).min(angle(b, c, a)).min(angle(c, a, b));
        if m > 15f64.to_radians() {
            return [a, b, c];
        }
    }
}

fn random_segment(rng: &mut Rng) -> [Vec2; 2] {
    let a = v(rng.range(-1.0, 0.0), rng.range(-0.5, 0.5));
    let b = a + unit_at(rng.range(-0.6, 0.6)) * rng.range(1.0, 1.8);
    [a, b]
}

fn tritangent(a: Vec2, b: Vec2, c: Vec2, ex: bool) -> Res<Vec<Vec2>> {
    let la = (b - c).norm();
    let lb = (c - a).norm();
    let lc = (a - b).norm();
    let (wa, wb, wc) = if ex { (-la, lb, lc) } else { (la, lb, lc) };
    let s = wa + wb + wc;
    if s.abs() < 1e-9 {
        return retry("degenerate tritangent centre");
    }
    let i = (a * wa + b * wb + c * wc) * (1.0 / s);
    let x = foot(i, b, c - b);
    let y = foot(i, c, a - c);
    let z = foot(i, a, b - a);
    Ok(vec![x, y, z, i])
}

fn angle_locus(a: Vec2, b: Vec2, theta: f64) -> Res<Obj> {
    let t = theta.rem_euclid(PI);
    if t < 1e-6 || PI - t < 1e-6 {
        return retry("zero inscribed angle: locus is the line itself");
    }
    let phi = ang(a - b) + 0.5 * (PI - t);
    let l1 = line_dir(b, unit_at(phi))?;
    let l2 = line_dir(a, unit_at(phi + t))?;
    let x0 = match intersect(&l1, &l2).as_slice() {
        [x] => *x,
        _ => return retry("angle locus construction degenerate"),
    };
    let c = circumcenter(a, b, x0)?;
    Ok(Obj::Circle {
        c,
        r: (a - c).norm(),
    })
}

/// External common tangents of circles (o, ra) and (w, rb): AG1's
/// `sketch_cc_tangent`, returning `[x, y, z, t]` (x, z on the first circle).
fn cc_tangent(o: Vec2, a: Vec2, w: Vec2, b: Vec2) -> Res<Vec<Vec2>> {
    let ra = (o - a).norm();
    let rb = (w - b).norm();
    if (o - w).norm() < 1e-9 {
        return retry("concentric circles");
    }
    if (ra - rb).abs() < 1e-9 * ra.max(rb) {
        let n = unit(rot90(w - o))?;
        let (x, z) = (o + n * ra, o - n * ra);
        return Ok(vec![x, x + (w - o), z, z + (w - o)]);
    }
    let swap = rb > ra;
    let (o, ra, w) = if swap { (w, rb, o) } else { (o, ra, w) };
    let q = o + (w - o) * (ra / (ra - if swap { (o - a).norm() } else { rb }));
    let dia = Obj::Circle {
        c: mid(o, q),
        r: (o - q).norm() / 2.0,
    };
    let pts = intersect(&dia, &Obj::Circle { c: o, r: ra });
    let [x, z] = pts.as_slice() else {
        return retry("one circle inside the other: no external tangent");
    };
    let y = foot(w, *x, q - *x);
    let t = foot(w, *z, q - *z);
    if swap {
        Ok(vec![y, *x, t, *z])
    } else {
        Ok(vec![*x, y, *z, t])
    }
}

/// AG1's `sketch_2l1c`: the circle tangent to lines `ca`, `cb` and to the
/// circle `(o, |oa|)`; returns `[foot on ac, foot on bc, tangency, centre]`.
fn two_lines_one_circle(a: Vec2, b: Vec2, c: Vec2, p: Vec2) -> Res<Vec<Vec2>> {
    let r = (p - a).norm();
    let circle = Obj::Circle { c: p, r };
    let side = |l0: Vec2, l1: Vec2, x: Vec2| cross(l1 - l0, x - l0).signum();
    let pick = |l0: Vec2, l1: Vec2, other: Vec2| -> Res<Vec2> {
        let perp = line_dir(p, rot90(l1 - l0))?;
        let pts = intersect(&perp, &circle);
        let [d, d_] = pts.as_slice() else {
            return retry("2l1c: no perpendicular intersection");
        };
        Ok(if side(l0, l1, *d_) != side(l0, l1, other) {
            *d_
        } else {
            *d
        })
    };
    let d = pick(b, c, a)?;
    let e = pick(a, c, b)?;
    let df = line_dir(d, rot90(d - p))?;
    let ef = line_dir(e, rot90(e - p))?;
    let f = match intersect(&df, &ef).as_slice() {
        [f] => *f,
        _ => return retry("2l1c: parallel tangents"),
    };
    let cf = line(c, f)?;
    let pts = intersect(&cf, &circle);
    let [g, g_] = pts.as_slice() else {
        return retry("2l1c: line cf misses the circle");
    };
    let g = if side(b, c, *g_) == side(b, c, a) {
        *g_
    } else {
        *g
    };
    let b_ = c + unit(b - c)?;
    let a_ = c + unit(a - c)?;
    let m = mid(a_, b_);
    let x = match intersect(&line(c, m)?, &line(p, g)?).as_slice() {
        [x] => *x,
        _ => return retry("2l1c: centre lines parallel"),
    };
    Ok(vec![foot(x, a, c - a), foot(x, b, c - b), g, x])
}

#[derive(Clone, Copy)]
enum NArg {
    P(Vec2),
    N(f64),
}

fn sketch(name: &str, args: &[NArg], fig: &Figure, rng: &mut Rng) -> Res<Sketch> {
    let out = sketch_raw(name, args, fig, rng)?;
    Ok(match out {
        Sketch::Points(ps) if args.is_empty() && rng.unit() < 0.5 => {
            Sketch::Points(ps.into_iter().map(|q| v(-q.x, q.y)).collect())
        }
        other => other,
    })
}

fn sketch_raw(name: &str, args: &[NArg], fig: &Figure, rng: &mut Rng) -> Res<Sketch> {
    use Sketch::*;
    let p = |i: usize| -> Res<Vec2> {
        match args.get(i) {
            Some(NArg::P(x)) => Ok(*x),
            _ => fatal(format!("sketch `{name}`: argument {i} must be a point")),
        }
    };
    let num = |i: usize| -> Res<f64> {
        match args.get(i) {
            Some(NArg::N(x)) => Ok(*x),
            _ => fatal(format!("sketch `{name}`: argument {i} must be a number")),
        }
    };
    let locus = |o: Res<Obj>| o.map(Locus);
    Ok(match name {
        "line" => locus(line(p(0)?, p(1)?))?,
        "pline" => locus(line_dir(p(0)?, p(2)? - p(1)?))?,
        "tline" => locus(line_dir(p(0)?, rot90(p(2)? - p(1)?)))?,
        "bline" => {
            let (a, b) = (p(0)?, p(1)?);
            locus(line_dir(mid(a, b), rot90(b - a)))?
        }
        "bisect" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            locus(line_dir(b, unit(a - b)? + unit(c - b)?))?
        }
        "exbisect" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            locus(line_dir(b, unit(a - b)? - unit(c - b)?))?
        }
        "amirror" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            locus(line_dir(b, unit_at(2.0 * ang(c - b) - ang(a - b))))?
        }
        // on_aline x a b c d e: eqangle a x a b d c d e; numerics `aline e d c b a`.
        "aline" => {
            let (e, d, c, b, a) = (p(0)?, p(1)?, p(2)?, p(3)?, p(4)?);
            let t = ang(b - a) + ang(c - d) - ang(e - d);
            locus(line_dir(a, unit_at(t)))?
        }
        // on_aline2 x a b c d e: eqangle x a x b d c d e.
        "aline2" => {
            let (e, d, c, b, a) = (p(0)?, p(1)?, p(2)?, p(3)?, p(4)?);
            locus(angle_locus(a, b, ang(c - d) - ang(e - d)))?
        }
        // eqangle3 x a b d e f: eqangle x a x b d e d f.
        "eqangle3" => {
            let (a, b, d, e, f) = (p(0)?, p(1)?, p(2)?, p(3)?, p(4)?);
            locus(angle_locus(a, b, ang(e - d) - ang(f - d)))?
        }
        "circle" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            Locus(Obj::Circle {
                c: a,
                r: (b - c).norm(),
            })
        }
        "cyclic" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            let o = circumcenter(a, b, c)?;
            Locus(Obj::Circle {
                c: o,
                r: (a - o).norm(),
            })
        }
        "dia" => {
            let (a, b) = (p(0)?, p(1)?);
            Locus(Obj::Circle {
                c: mid(a, b),
                r: (a - b).norm() / 2.0,
            })
        }
        // s_angle a b x y: the line bx makes the directed angle y with ba.
        "s_angle" => {
            let (a, b, y) = (p(0)?, p(1)?, num(2)?);
            locus(line_dir(b, rot(a - b, y.to_radians())))?
        }
        "on_opline" => {
            let (a, b) = (p(0)?, p(1)?);
            if (a - b).norm() < 1e-12 {
                return retry("opline through coincident points");
            }
            Locus(Obj::Ray { p: a, d: a - b })
        }
        "midp" => Points(vec![mid(p(0)?, p(1)?)]),
        "pmirror" => Points(vec![p(1)? * 2.0 - p(0)?]),
        "reflect" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            if (c - b).norm() < 1e-12 {
                return retry("reflect over a degenerate line");
            }
            Points(vec![foot(a, b, c - b) * 2.0 - a])
        }
        "rotatep90" => {
            let (a, b) = (p(0)?, p(1)?);
            Points(vec![a + rot90(b - a)])
        }
        "rotaten90" => {
            let (a, b) = (p(0)?, p(1)?);
            Points(vec![a - rot90(b - a)])
        }
        // shift x b c d (numerics `shift d c b`): x = b + c - d.
        "shift" => Points(vec![p(2)? + p(1)? - p(0)?]),
        // eqangle2 x a b c: eqangle a b a x c x c b — a one-parameter family.
        "eqangle2" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            let phi = ang(b - a) + rng.range(0.15, 0.85) * PI;
            let l1 = line_dir(a, unit_at(phi))?;
            let l2 = line_dir(c, unit_at(ang(b - a) - phi + ang(b - c)))?;
            match intersect(&l1, &l2).as_slice() {
                [x] => Points(vec![*x]),
                _ => return retry("eqangle2: parallel"),
            }
        }
        "free" => Points(vec![fig.random_point(rng)]),
        "segment" => Points(random_segment(rng).to_vec()),
        "triangle" => Points(random_triangle(rng).to_vec()),
        "quadrangle" => {
            let [a, b, c] = random_triangle(rng);
            let d = a + (c - b) + v(rng.range(-0.4, 0.4), rng.range(-0.4, 0.4));
            Points(vec![a, b, c, d])
        }
        "pentagon" => {
            let c = v(0.0, 0.0);
            let mut ts: Vec<f64> = (0..5)
                .map(|k| (k as f64 + rng.range(-0.3, 0.3)) * 2.0 * PI / 5.0)
                .collect();
            ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
            Points(ts.iter().map(|&t| c + unit_at(t) * rng.range(0.8, 1.2)).collect())
        }
        "trapezoid" => {
            let [a, b, c] = random_triangle(rng);
            let d = c + (a - b) * rng.range(0.3, 1.2);
            Points(vec![a, b, c, d])
        }
        "r_trapezoid" => {
            let [a, b] = random_segment(rng);
            let d = a + rot90(b - a) * rng.range(0.5, 1.2);
            let c = d + (b - a) * rng.range(0.3, 1.2);
            Points(vec![a, b, c, d])
        }
        "eq_trapezoid" => {
            let [a, b] = random_segment(rng);
            let m = mid(a, b);
            let u = unit(b - a)?;
            let h = rot90(u) * rng.range(0.5, 1.2);
            let c = m + u * rng.range(0.2, 0.8) * (b - a).norm() * 0.5 + h;
            let d = c - u * 2.0 * (c - m).dot(u);
            Points(vec![a, b, c, d])
        }
        "eq_quadrangle" => {
            let [a, b, c] = random_triangle(rng);
            let d = a + unit_at(ang(c - a) + rng.range(0.3, 1.2)) * (b - c).norm();
            Points(vec![a, b, c, d])
        }
        "eqdia_quadrangle" => {
            let [a, b, c] = random_triangle(rng);
            let d = b + unit_at(ang(a - b) + rng.range(0.3, 1.2)) * (a - c).norm();
            Points(vec![a, b, c, d])
        }
        // iso_triangle a b c: cong a b a c (apex a).
        "isos" => {
            let [b, c] = random_segment(rng);
            let a = mid(b, c) + rot90(c - b) * rng.range(0.4, 1.2);
            Points(vec![a, b, c])
        }
        "r_triangle" => {
            let [a, b] = random_segment(rng);
            let c = a + rot90(b - a) * rng.range(0.5, 1.5);
            Points(vec![a, b, c])
        }
        "risos" => {
            let [a, b] = random_segment(rng);
            Points(vec![a, b, a + rot90(b - a)])
        }
        "ieq_triangle" => {
            let [a, b] = random_segment(rng);
            Points(vec![a, b, a + rot(b - a, PI / 3.0)])
        }
        "triangle12" => {
            let [a, b] = random_segment(rng);
            let c = a + unit_at(ang(b - a) + rng.range(0.4, 1.4)) * (2.0 * (b - a).norm());
            Points(vec![a, b, c])
        }
        "rectangle" => {
            let [a, b] = random_segment(rng);
            let c = b + rot90(b - a) * rng.range(0.5, 1.5);
            Points(vec![a, b, c, a + (c - b)])
        }
        // square a b x y (AG1 sketch_square): counter-clockwise a, b, x, y.
        "square" => {
            let (a, b) = (p(0)?, p(1)?);
            Points(vec![b + rot90(b - a), a + rot90(b - a)])
        }
        "isquare" => {
            let [a, b] = random_segment(rng);
            Points(vec![a, b, b + rot90(b - a), a + rot90(b - a)])
        }
        "trisegment" => {
            let (a, b) = (p(0)?, p(1)?);
            Points(vec![a + (b - a) * (1.0 / 3.0), a + (b - a) * (2.0 / 3.0)])
        }
        // trisect x y a b c: the trisectors of ∠abc meet ac at x (near a), y.
        "trisect" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            let ta = ang(a - b);
            let mut delta = ang(c - b) - ta;
            while delta > PI {
                delta -= 2.0 * PI;
            }
            while delta <= -PI {
                delta += 2.0 * PI;
            }
            let ac = line(a, c)?;
            let x = intersect(&line_dir(b, unit_at(ta + delta / 3.0))?, &ac);
            let y = intersect(&line_dir(b, unit_at(ta + 2.0 * delta / 3.0))?, &ac);
            match (x.as_slice(), y.as_slice()) {
                ([x], [y]) => Points(vec![*x, *y]),
                _ => return retry("trisect: parallel"),
            }
        }
        // 3peq x y z a b c (AG1 sketch_3peq).
        "3peq" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            let z = b + (c - b) * rng.range(-0.5, 1.5);
            let z_ = z * 2.0 - c;
            let l = line_dir(z_, c - a)?;
            match intersect(&l, &line(a, b)?).as_slice() {
                [x] => Points(vec![*x, z * 2.0 - *x, z]),
                _ => return retry("3peq: parallel"),
            }
        }
        "incenter2" => Points(tritangent(p(0)?, p(1)?, p(2)?, false)?),
        "excenter2" => Points(tritangent(p(0)?, p(1)?, p(2)?, true)?),
        "centroid" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            Points(vec![mid(b, c), mid(c, a), mid(a, b), (a + b + c) * (1.0 / 3.0)])
        }
        "ninepoints" => {
            let (a, b, c) = (p(0)?, p(1)?, p(2)?);
            let (x, y, z) = (mid(b, c), mid(c, a), mid(a, b));
            Points(vec![x, y, z, circumcenter(x, y, z)?])
        }
        "2l1c" => Points(two_lines_one_circle(p(0)?, p(1)?, p(2)?, p(3)?)?),
        // e5128 x y a b c d: y = midpoint of ab, x = second meet of line dy
        // with the circle (c, cb) through d.
        "e5128" => {
            let (a, b, c, d) = (p(0)?, p(1)?, p(2)?, p(3)?);
            let g = mid(a, b);
            let pts = intersect(
                &line(d, g)?,
                &Obj::Circle {
                    c,
                    r: (c - b).norm(),
                },
            );
            let Some(e) = pts
                .iter()
                .copied()
                .max_by(|u, w| (*u - d).norm().partial_cmp(&(*w - d).norm()).unwrap())
            else {
                return retry("e5128: line misses circle");
            };
            Points(vec![e, g])
        }
        "cc_tangent" => Points(cc_tangent(p(0)?, p(1)?, p(2)?, p(3)?)?),
        "cc_tangent0" => Points(cc_tangent(p(0)?, p(1)?, p(2)?, p(3)?)?[..2].to_vec()),
        // tangent x y a o b: tangency points from a to the circle (o, ob).
        "tangent" => {
            let (a, o, b) = (p(0)?, p(1)?, p(2)?);
            let pts = intersect(
                &Obj::Circle {
                    c: o,
                    r: (o - b).norm(),
                },
                &Obj::Circle {
                    c: mid(a, o),
                    r: (a - o).norm() / 2.0,
                },
            );
            if pts.len() != 2 {
                return retry("tangent: point inside the circle");
            }
            Points(pts)
        }
        other => return fatal(format!("unsupported numeric sketch `{other}`")),
    })
}

fn ang_mod_pi(x: f64) -> f64 {
    let y = x.rem_euclid(PI);
    if y > PI / 2.0 {
        y - PI
    } else {
        y
    }
}

fn holds(name: &str, x: &[Vec2], nums: &[f64], scale: f64) -> Option<bool> {
    let tol = 1e-7 * scale.max(1e-9);
    let atol = 1e-8;
    let dir = |i: usize, j: usize| ang(x[j] - x[i]);
    let dist = |i: usize, j: usize| (x[i] - x[j]).norm();
    let nonzero = |i: usize, j: usize| dist(i, j) > tol;
    let need = |n: usize| x.len() >= n;
    Some(match name {
        "coll" => {
            need(3) && {
                let base = (1..x.len())
                    .map(|j| x[j] - x[0])
                    .max_by(|a, b| a.norm().partial_cmp(&b.norm()).unwrap())
                    .unwrap();
                base.norm() < tol
                    || (1..x.len()).all(|k| cross(base, x[k] - x[0]).abs() / base.norm() < tol)
            }
        }
        "ncoll" => {
            need(3)
                && !holds("coll", x, nums, scale)?
                && (0..x.len()).all(|i| ((i + 1)..x.len()).all(|j| nonzero(i, j)))
        }
        "diff" => need(2) && nonzero(0, 1),
        "para" => need(4) && nonzero(0, 1) && nonzero(2, 3) && ang_mod_pi(dir(0, 1) - dir(2, 3)).abs() < atol,
        "npara" => need(4) && nonzero(0, 1) && nonzero(2, 3) && ang_mod_pi(dir(0, 1) - dir(2, 3)).abs() > 1e-4,
        "perp" => {
            need(4)
                && nonzero(0, 1)
                && nonzero(2, 3)
                && ang_mod_pi(dir(0, 1) - dir(2, 3) - PI / 2.0).abs() < atol
        }
        "nperp" => {
            need(4)
                && nonzero(0, 1)
                && nonzero(2, 3)
                && ang_mod_pi(dir(0, 1) - dir(2, 3) - PI / 2.0).abs() > 1e-4
        }
        "cong" => need(4) && (dist(0, 1) - dist(2, 3)).abs() < tol,
        "cyclic" => {
            need(4) && {
                let mut ok = false;
                'outer: for i in 0..x.len() {
                    for j in (i + 1)..x.len() {
                        for k in (j + 1)..x.len() {
                            if let Ok(o) = circumcenter(x[i], x[j], x[k]) {
                                let r = (x[i] - o).norm();
                                if r < 1e3 * scale {
                                    ok = x.iter().all(|&p| ((p - o).norm() - r).abs() < tol);
                                    break 'outer;
                                }
                            }
                        }
                    }
                }
                ok
            }
        }
        "eqangle" => {
            need(8)
                && [(0, 1), (2, 3), (4, 5), (6, 7)].iter().all(|&(i, j)| nonzero(i, j))
                && ang_mod_pi(dir(0, 1) - dir(2, 3) - dir(4, 5) + dir(6, 7)).abs() < atol
        }
        "eqratio" => {
            need(8)
                && [(2, 3), (6, 7)].iter().all(|&(i, j)| nonzero(i, j))
                && (dist(0, 1) / dist(2, 3) - dist(4, 5) / dist(6, 7)).abs() < 1e-7
        }
        "aconst" => {
            need(4)
                && nonzero(0, 1)
                && nonzero(2, 3)
                && ang_mod_pi(dir(0, 1) - dir(2, 3) - nums.first()?.to_radians()).abs() < atol
        }
        "rconst" => need(4) && nonzero(2, 3) && (dist(0, 1) / dist(2, 3) - nums.first()?).abs() < 1e-7,
        _ => return None,
    })
}

#[derive(Clone, Debug)]
struct NamedPred {
    name: String,
    points: Vec<String>,
    constants: Vec<Rat>,
}

fn is_number(s: &str) -> bool {
    s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '-')
}

fn parse_rat(s: &str) -> Result<Rat, String> {
    if let Some((n, d)) = s.split_once('/') {
        let n: i64 = n.parse().map_err(|_| format!("bad number `{s}`"))?;
        let d: i64 = d.parse().map_err(|_| format!("bad number `{s}`"))?;
        if d == 0 {
            return Err(format!("zero denominator in `{s}`"));
        }
        return Ok(Rat::new(n, d));
    }
    s.parse::<i64>()
        .map(Rat::from_int)
        .map_err(|_| format!("bad number `{s}`"))
}

fn lower_premise(t: &Term) -> Result<Vec<NamedPred>, String> {
    let pts: Vec<String> = t.args.iter().filter(|a| !is_number(a)).cloned().collect();
    let nums: Vec<&String> = t.args.iter().filter(|a| is_number(a)).collect();
    let simple = |name: &str, arity: usize| -> Result<Vec<NamedPred>, String> {
        if pts.len() != arity || !nums.is_empty() {
            return Err(format!("`{t}`: expected {arity} points"));
        }
        Ok(vec![NamedPred {
            name: name.to_string(),
            points: pts.clone(),
            constants: vec![],
        }])
    };
    match t.name.as_str() {
        "coll" if pts.len() >= 3 && nums.is_empty() => Ok(vec![NamedPred {
            name: "coll".into(),
            points: pts,
            constants: vec![],
        }]),
        "cyclic" if pts.len() >= 4 && nums.is_empty() => Ok(vec![NamedPred {
            name: "cyclic".into(),
            points: pts,
            constants: vec![],
        }]),
        "cong" | "para" | "perp" => simple(&t.name, 4),
        "eqangle" | "eqratio" => simple(&t.name, 8),
        // s_angle a b x y: dir(bx) - dir(ba) = y degrees (mod 180).
        "s_angle" => {
            if pts.len() != 3 || nums.len() != 1 {
                return Err(format!("`{t}`: expected 3 points and an angle"));
            }
            let y = parse_rat(nums[0])?;
            let y = Rat::from_int(((y.to_f64().round() as i64) % 180 + 180) % 180);
            if (parse_rat(nums[0])?.to_f64() - parse_rat(nums[0])?.to_f64().round()).abs() > 0.0 {
                return Err(format!("`{t}`: non-integer angle"));
            }
            Ok(vec![NamedPred {
                name: "aconst".into(),
                points: vec![pts[1].clone(), pts[2].clone(), pts[1].clone(), pts[0].clone()],
                constants: vec![y],
            }])
        }
        "rconst" => {
            if pts.len() != 4 || nums.is_empty() || nums.len() > 2 {
                return Err(format!("`{t}`: expected 4 points and a ratio"));
            }
            let r = if nums.len() == 2 {
                parse_rat(&format!("{}/{}", nums[0], nums[1]))?
            } else {
                parse_rat(nums[0])?
            };
            Ok(vec![NamedPred {
                name: "rconst".into(),
                points: pts,
                constants: vec![r],
            }])
        }
        _ => Err(format!("unsupported premise predicate `{t}`")),
    }
}

fn lower_goal(t: &Term, same_orientation: impl Fn(&[String]) -> bool) -> Result<Vec<NamedPred>, String> {
    let p = |name: &str, pts: &[&String]| NamedPred {
        name: name.to_string(),
        points: pts.iter().map(|s| s.to_string()).collect(),
        constants: vec![],
    };
    let a = &t.args;
    match t.name.as_str() {
        "midp" if a.len() == 3 => Ok(vec![
            p("coll", &[&a[0], &a[1], &a[2]]),
            p("cong", &[&a[0], &a[1], &a[0], &a[2]]),
        ]),
        "simtri" | "contri" if a.len() == 6 => {
            let (x, y, z, u, w, s) = (&a[0], &a[1], &a[2], &a[3], &a[4], &a[5]);
            let mut out = if same_orientation(a) {
                vec![
                    p("eqangle", &[y, x, y, z, w, u, w, s]),
                    p("eqangle", &[z, x, z, y, s, u, s, w]),
                ]
            } else {
                vec![
                    p("eqangle", &[y, x, y, z, w, s, w, u]),
                    p("eqangle", &[z, x, z, y, s, w, s, u]),
                ]
            };
            if t.name == "contri" {
                out.push(p("cong", &[x, y, u, w]));
            }
            Ok(out)
        }
        _ => lower_premise(t).map_err(|e| format!("goal: {e}")),
    }
}

/// A translated problem, ready for the engine.
#[derive(Clone, Debug)]
pub struct Translation {
    /// The figure and premises; `problem.goal` is the first conjunct.
    pub problem: Problem,
    /// The goal as a conjunction of engine predicates (usually one).
    pub goals: Vec<Predicate>,
    /// Every goal conjunct holds numerically on the figure.
    pub goal_holds: bool,
    /// The seed that produced the figure.
    pub seed: u64,
    /// Sampling attempts used.
    pub attempts: u32,
}

struct Sampled {
    fig: Figure,
    preds: Vec<NamedPred>,
    goals: Vec<NamedPred>,
    goal_holds: bool,
}

fn sample(prob: &AgProblem, seed: u64) -> Res<Sampled> {
    let defs = definitions();
    let mut rng = Rng::new(seed);
    let mut fig = Figure {
        names: vec![],
        coords: HashMap::new(),
    };
    let mut preds: Vec<NamedPred> = Vec::new();

    for clause in &prob.clauses {
        for cp in &clause.points {
            if fig.coords.contains_key(&cp.name) {
                return fatal(format!("point `{}` defined twice", cp.name));
            }
        }
        let clause_names: Vec<&str> = clause.points.iter().map(|p| p.name.as_str()).collect();
        let mut loci: Vec<Obj> = Vec::new();
        let mut explicit: HashMap<String, Vec2> = HashMap::new();
        let mut clause_premises: Vec<Term> = Vec::new();

        for c in &clause.constructions {
            let def = &defs[&c.name];
            let map: HashMap<&str, &str> = def
                .args
                .iter()
                .map(String::as_str)
                .zip(c.args.iter().map(String::as_str))
                .collect();
            let new_here: Vec<&str> = def
                .new_points
                .iter()
                .map(|n| map[n.as_str()])
                .filter(|n| !is_number(n))
                .collect();
            for n in &new_here {
                if !clause_names.contains(n) {
                    return fatal(format!(
                        "`{c}` constructs `{n}`, which the clause does not introduce"
                    ));
                }
            }
            for inp in &def.inputs {
                let actual = map[inp.as_str()];
                if !fig.coords.contains_key(actual) {
                    return fatal(format!("`{c}` uses `{actual}` before it is defined"));
                }
            }
            for pre in &def.preconditions {
                let pre = pre.subst(&map);
                let xs: Vec<Vec2> = pre
                    .args
                    .iter()
                    .filter(|a| !is_number(a))
                    .map(|a| fig.coords[a.as_str()])
                    .collect();
                let nums: Vec<f64> = pre
                    .args
                    .iter()
                    .filter(|a| is_number(a))
                    .filter_map(|a| parse_rat(a).ok().map(|r| r.to_f64()))
                    .collect();
                match holds(&pre.name, &xs, &nums, fig.radius()) {
                    Some(true) => {}
                    Some(false) => return retry(format!("precondition `{pre}` of `{c}` fails")),
                    None => return fatal(format!("unsupported precondition `{pre}`")),
                }
            }
            clause_premises.extend(def.premises.iter().map(|t| t.subst(&map)));

            if clause.points.iter().all(|p| p.coord.is_some()) {
                continue;
            }
            for nt in &def.numerics {
                let nt = nt.subst(&map);
                let args = nt
                    .args
                    .iter()
                    .map(|a| {
                        if is_number(a) {
                            parse_rat(a).map(|r| NArg::N(r.to_f64())).map_err(Fail::Fatal)
                        } else {
                            fig.coords.get(a.as_str()).map(|&p| NArg::P(p)).ok_or_else(|| {
                                Fail::Fatal(format!("sketch `{nt}` uses undefined `{a}`"))
                            })
                        }
                    })
                    .collect::<Res<Vec<_>>>()?;
                match sketch(&nt.name, &args, &fig, &mut rng)? {
                    Sketch::Locus(o) => loci.push(o),
                    Sketch::Points(ps) => {
                        if ps.len() != new_here.len() {
                            return fatal(format!(
                                "sketch `{}` returned {} points for {} new points",
                                nt.name,
                                ps.len(),
                                new_here.len()
                            ));
                        }
                        for (n, q) in new_here.iter().zip(ps) {
                            explicit.insert(n.to_string(), q);
                        }
                    }
                }
            }
        }

        // Place the clause's points.
        let mut placed: Vec<(String, Vec2)> = Vec::new();
        for cp in &clause.points {
            if let Some(q) = cp.coord {
                placed.push((cp.name.clone(), q));
            } else if let Some(&q) = explicit.get(&cp.name) {
                placed.push((cp.name.clone(), q));
            }
        }
        if placed.len() < clause.points.len() {
            if clause.points.len() != 1 || !placed.is_empty() {
                return fatal(format!(
                    "clause `{}` mixes loci with a multi-point construction",
                    clause_names.join(" ")
                ));
            }
            let q = match loci.len() {
                0 => return fatal(format!("no numeric placement for `{}`", clause_names[0])),
                1 => {
                    let mut got = None;
                    for _ in 0..20 {
                        let q = fig.sample_on(&loci[0], &mut rng);
                        if fig.placement_ok(q) {
                            got = Some(q);
                            break;
                        }
                    }
                    match got {
                        Some(q) => q,
                        None => return retry(format!("could not place `{}`", clause_names[0])),
                    }
                }
                _ => {
                    let tol = 1e-9 * fig.radius().max(1.0);
                    let cands: Vec<Vec2> = intersect(&loci[0], &loci[1])
                        .into_iter()
                        .filter(|&q| loci[2..].iter().all(|o| obj_distance(o, q) < tol))
                        .filter(|&q| fig.placement_ok(q))
                        .collect();
                    match cands.len() {
                        0 => {
                            return retry(format!(
                                "loci of `{}` do not meet away from existing points",
                                clause_names[0]
                            ))
                        }
                        1 => cands[0],
                        n => cands[(rng.next() % n as u64) as usize],
                    }
                }
            };
            placed.push((clause_names[0].to_string(), q));
        } else if clause.points.iter().any(|p| p.coord.is_none()) {
            for (n, q) in &placed {
                if !fig.placement_ok(*q) {
                    return retry(format!("`{n}` lands on or far from an existing point"));
                }
            }
            for (i, (_, q)) in placed.iter().enumerate() {
                for (_, w) in &placed[..i] {
                    if (*q - *w).norm() < 0.02 * fig.radius().max(0.1) {
                        return retry("construction produced coincident points");
                    }
                }
            }
        }
        for (n, q) in placed {
            fig.names.push(n.clone());
            fig.coords.insert(n, q);
        }

        // Every premise must hold numerically on the sample.
        for t in &clause_premises {
            let lowered = lower_premise(t).map_err(Fail::Fatal)?;
            for np in lowered {
                let xs: Vec<Vec2> = np.points.iter().map(|n| fig.coords[n.as_str()]).collect();
                let nums: Vec<f64> = np.constants.iter().map(Rat::to_f64).collect();
                match holds(&np.name, &xs, &nums, fig.radius()) {
                    Some(true) => preds.push(np),
                    Some(false) => return retry(format!("premise `{t}` fails numerically")),
                    None => return fatal(format!("cannot check premise `{t}`")),
                }
            }
        }
    }

    for a in &prob.goal.args {
        if !is_number(a) && !fig.coords.contains_key(a.as_str()) {
            return fatal(format!("goal uses undefined point `{a}`"));
        }
    }
    let orient = |a: &[String]| {
        let c = |i: usize| fig.coords[a[i].as_str()];
        cross(c(1) - c(0), c(2) - c(0)).signum() == cross(c(4) - c(3), c(5) - c(3)).signum()
    };
    let goals = lower_goal(&prob.goal, orient).map_err(Fail::Fatal)?;
    let mut goal_holds = true;
    for g in &goals {
        let xs: Vec<Vec2> = g.points.iter().map(|n| fig.coords[n.as_str()]).collect();
        let nums: Vec<f64> = g.constants.iter().map(Rat::to_f64).collect();
        match holds(&g.name, &xs, &nums, fig.radius()) {
            Some(h) => goal_holds &= h,
            None => return fatal(format!("cannot check goal `{}`", g.name)),
        }
    }
    Ok(Sampled {
        fig,
        preds,
        goals,
        goal_holds,
    })
}

fn to_predicate(np: &NamedPred, ids: &HashMap<String, PointId>) -> Predicate {
    Predicate {
        name: np.name.clone(),
        points: np.points.iter().map(|n| ids[n.as_str()]).collect(),
        constants: np.constants.clone(),
    }
}

/// Translate a parsed problem: sample figures from `first_seed` on (up to
/// `max_attempts`) until one satisfies every premise and the goal. If the goal
/// never holds, the last premise-valid figure is returned with
/// `goal_holds == false`; if no figure satisfies the premises, the last
/// failure reason is returned as the error.
pub fn translate(prob: &AgProblem, first_seed: u64, max_attempts: u32) -> Result<Translation, String> {
    let mut last_err = String::from("no attempt");
    let mut fallback: Option<(u64, u32, Sampled)> = None;
    for k in 0..max_attempts.max(1) {
        let seed = first_seed + k as u64;
        match sample(prob, seed) {
            Err(Fail::Fatal(m)) => return Err(m),
            Err(Fail::Retry(m)) => last_err = m,
            Ok(s) if s.goal_holds => return Ok(build_translation(s, seed, k + 1)),
            Ok(s) => {
                last_err = "goal does not hold numerically".into();
                if fallback.is_none() {
                    fallback = Some((seed, k + 1, s));
                }
            }
        }
    }
    match fallback {
        Some((seed, _, s)) => Ok(build_translation(s, seed, max_attempts)),
        None => Err(format!("no valid figure in {max_attempts} samples: {last_err}")),
    }
}

fn build_translation(s: Sampled, seed: u64, attempts: u32) -> Translation {
    let ids: HashMap<String, PointId> = s
        .fig
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i as PointId))
        .collect();
    let points: Vec<Point> = s
        .fig
        .names
        .iter()
        .map(|n| Point {
            name: n.clone(),
            value: s.fig.coords[n],
        })
        .collect();
    let preds = s.preds.iter().map(|p| to_predicate(p, &ids)).collect();
    let goals: Vec<Predicate> = s.goals.iter().map(|p| to_predicate(p, &ids)).collect();
    Translation {
        problem: Problem {
            points,
            preds,
            goal: goals.first().cloned(),
        },
        goals,
        goal_holds: s.goal_holds,
        seed,
        attempts,
    }
}

/// One premise-valid sampled figure of a corpus problem, by point name, in
/// construction order.
#[derive(Clone, Debug)]
pub struct FigureSample {
    pub names: Vec<String>,
    pub coords: HashMap<String, Vec2>,
    /// Every conjunct of the problem's own goal holds on this figure.
    pub goal_holds: bool,
}

impl FigureSample {
    fn figure(&self) -> Figure {
        Figure {
            names: self.names.clone(),
            coords: self.coords.clone(),
        }
    }

    fn same_orientation(&self, a: &[String]) -> bool {
        let c = |i: usize| self.coords[a[i].as_str()];
        cross(c(1) - c(0), c(2) - c(0)).signum() == cross(c(4) - c(3), c(5) - c(3)).signum()
    }
}

/// Sample one figure from `seed`: `Ok(None)` when this seed gives no
/// premise-valid figure, `Err` when the problem cannot be built at all.
pub fn sample_figure(prob: &AgProblem, seed: u64) -> Result<Option<FigureSample>, String> {
    match sample(prob, seed) {
        Ok(s) => Ok(Some(FigureSample {
            names: s.fig.names,
            coords: s.fig.coords,
            goal_holds: s.goal_holds,
        })),
        Err(Fail::Retry(_)) => Ok(None),
        Err(Fail::Fatal(m)) => Err(m),
    }
}

/// Whether a construction argument is a numeric constant, not a point name.
pub fn is_number_arg(s: &str) -> bool {
    is_number(s)
}

/// The conjuncts of a goal as plain predicate terms: `midp`, `simtri` and
/// `contri` are expanded (orientation read from `fig`); anything else is
/// returned unchanged.
pub fn goal_conjuncts(goal: &Term, fig: &FigureSample) -> Result<Vec<Term>, String> {
    if !matches!(goal.name.as_str(), "midp" | "simtri" | "contri") {
        return Ok(vec![goal.clone()]);
    }
    for a in &goal.args {
        if !fig.coords.contains_key(a.as_str()) {
            return Err(format!("goal uses undefined point `{a}`"));
        }
    }
    Ok(lower_goal(goal, |a| fig.same_orientation(a))?
        .into_iter()
        .map(|np| Term {
            name: np.name,
            args: np.points,
        })
        .collect())
}

/// Whether a goal term holds numerically on a figure, with the translator's
/// tolerances.
pub fn term_holds(t: &Term, fig: &FigureSample) -> Result<bool, String> {
    for a in &t.args {
        if !is_number(a) && !fig.coords.contains_key(a.as_str()) {
            return Err(format!("`{t}` uses undefined point `{a}`"));
        }
    }
    let scale = fig.figure().radius();
    let mut all = true;
    for g in lower_goal(t, |a| fig.same_orientation(a))? {
        let xs: Vec<Vec2> = g.points.iter().map(|n| fig.coords[n.as_str()]).collect();
        let nums: Vec<f64> = g.constants.iter().map(Rat::to_f64).collect();
        all &= holds(&g.name, &xs, &nums, scale).ok_or_else(|| format!("cannot check `{t}`"))?;
    }
    Ok(all)
}

/// Whether an engine predicate holds numerically on a translated problem's
/// figure, with the translator's tolerances.
pub fn pred_holds(problem: &Problem, pred: &Predicate) -> Option<bool> {
    let fig = Figure {
        names: problem.points.iter().map(|p| p.name.clone()).collect(),
        coords: problem
            .points
            .iter()
            .map(|p| (p.name.clone(), p.value))
            .collect(),
    };
    let xs: Vec<Vec2> = pred
        .points
        .iter()
        .map(|&i| problem.points[i as usize].value)
        .collect();
    let nums: Vec<f64> = pred.constants.iter().map(Rat::to_f64).collect();
    holds(&pred.name, &xs, &nums, fig.radius())
}

/// Render a problem as corpus text. A clause whose points all appear in `pins`
/// is written with those coordinates (`x@1.5_-0.25`, shortest round-trip
/// form), so translation reproduces that figure exactly.
pub fn render_problem(prob: &AgProblem, pins: &HashMap<String, Vec2>) -> String {
    let mut clauses = Vec::new();
    for c in &prob.clauses {
        let pinned = c.points.iter().all(|p| pins.contains_key(&p.name));
        let pts: Vec<String> = c
            .points
            .iter()
            .map(|p| match pins.get(&p.name) {
                Some(q) if pinned => format!("{}@{}_{}", p.name, q.x, q.y),
                _ => p.name.clone(),
            })
            .collect();
        let cons: Vec<String> = c.constructions.iter().map(Term::to_string).collect();
        clauses.push(format!("{} = {}", pts.join(" "), cons.join(", ")));
    }
    format!("{} ? {}", clauses.join("; "), prob.goal)
}

/// Parse and translate one corpus statement.
pub fn translate_text(name: &str, text: &str, first_seed: u64) -> Result<Translation, String> {
    let p = parse_problem(name, text)?;
    translate(&p, first_seed, 200)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::solve_problem;

    #[test]
    fn bundled_definitions_parse() {
        let d = definitions();
        assert_eq!(d.len(), 68);
        let inc = &d["incenter2"];
        assert_eq!(inc.new_points, vec!["x", "y", "z", "i"]);
        assert_eq!(inc.inputs, vec!["a", "b", "c"]);
        assert_eq!(inc.premises.len(), 11);
        assert_eq!(d["triangle"].new_points, vec!["a", "b", "c"]);
        assert_eq!(d["square"].new_points, vec!["x", "y"]);
        assert_eq!(d["on_tline"].numerics, vec![Term::parse("tline a b c").unwrap()]);
    }

    #[test]
    fn missing_new_point_arguments_are_prepended() {
        let p = parse_problem("t", "a b c = triangle; d = on_line b c ? coll d b c").unwrap();
        assert_eq!(p.clauses[0].constructions[0].args, vec!["a", "b", "c"]);
        assert_eq!(p.clauses[1].constructions[0].args, vec!["d", "b", "c"]);
    }

    #[test]
    fn unknown_constructions_are_reported() {
        let e = parse_problem("t", "a b c = triangle a b c; d = frobnicate d a b ? coll a b d")
            .unwrap_err();
        assert!(e.contains("unknown construction `frobnicate`"), "{e}");
        let e = parse_problem("t", "a b c = triangle a b c; d = on_line d ? coll a b d").unwrap_err();
        assert!(e.contains("argument mismatch"), "{e}");
    }

    fn proves(text: &str) -> bool {
        let t = translate_text("t", text, 1).expect("translates");
        assert!(t.goal_holds, "goal must hold numerically for `{text}`");
        t.goals.iter().all(|g| {
            let mut p = t.problem.clone();
            p.goal = Some(g.clone());
            solve_problem(&p).unwrap()
        })
    }

    #[test]
    fn bundled_examples_translate_and_prove() {
        let examples = read_corpus(include_str!("../../corpus/examples.txt")).unwrap();
        assert_eq!(examples.len(), 4);
        for (name, text) in &examples {
            let t = translate_text(name, text, 1).expect("translates");
            assert!(t.goal_holds, "{name}: goal must hold numerically");
            if name != "orthocenter" {
                assert!(proves(text), "{name} should prove by DDAR");
            }
        }
    }

    #[test]
    fn false_goal_is_flagged_and_not_proved() {
        let t = translate_text(
            "false",
            "a b c = triangle a b c; m = midpoint m b c ? perp a m b c",
            1,
        )
        .expect("translates");
        assert!(!t.goal_holds);
        assert!(!solve_problem(&t.problem).unwrap());
    }

    #[test]
    fn known_problems_hold_and_prove() {
        // Midpoint/circumcentre, incircle touch points, parallelogram, foot.
        assert!(proves(
            "a b c = triangle a b c; o = circle o a b c; m = midpoint m b c ? perp o m b c"
        ));
        assert!(proves(
            "a b c = triangle a b c; x y z i = incenter2 x y z i a b c ? cong i x i z"
        ));
        assert!(proves(
            "a b c = triangle a b c; d = parallelogram a b c d ? cong a d b c"
        ));
        assert!(proves(
            "a b c = r_triangle a b c; m = midpoint m b c ? cong m a m b"
        ));
    }

    #[test]
    fn every_premise_of_every_definition_samples() {
        // Each construction, applied to a generic triangle, must produce a
        // figure on which all of its premises hold.
        for (name, def) in definitions() {
            let generic = ["ncoll", "diff", "npara", "nperp"];
            if def.preconditions.iter().any(|p| !generic.contains(&p.name.as_str())) {
                continue;
            }
            let mut text = String::from("p1 p2 p3 p4 = quadrangle p1 p2 p3 p4; ");
            for k in 5..=def.inputs.len() {
                text.push_str(&format!("p{k} = free p{k}; "));
            }
            let mut args: Vec<String> = Vec::new();
            let mut k = 0;
            let mut fresh = 0;
            for a in &def.args {
                if name == "s_angle" && a == "y" {
                    args.push("30".into());
                } else if def.inputs.contains(a) {
                    args.push(format!("p{}", k + 1));
                    k += 1;
                } else {
                    args.push(format!("n{fresh}"));
                    fresh += 1;
                }
            }
            let news: Vec<String> = (0..fresh).map(|i| format!("n{i}")).collect();
            if news.is_empty() {
                continue;
            }
            text.push_str(&format!("{} = {} {} ? coll p1 p1 p2", news.join(" "), name, args.join(" ")));
            let p = parse_problem(name, &text).expect("parses");
            let mut ok = false;
            let mut last = String::new();
            for seed in 1..400 {
                match sample(&p, seed) {
                    Ok(_) => {
                        ok = true;
                        break;
                    }
                    Err(Fail::Retry(m)) => last = m,
                    Err(Fail::Fatal(m)) => panic!("{name}: {m}"),
                }
            }
            assert!(ok, "{name}: never sampled a valid figure ({last})");
        }
    }
}
