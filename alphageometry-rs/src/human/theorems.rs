use super::model::{AtomKey, Expr, Stmt};
use super::trace::{EngineTrace, Table};
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::rational::Rat;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Hyp {
    Line(Vec<PointId>),
    Circle(Vec<PointId>),
    Eq(Table, LinComb),
    Distinct(Vec<PointId>),
    Triangle(PointId, PointId, PointId),
    Between(PointId, PointId, PointId),
    Outside(PointId, PointId, PointId),
    SplitAlike(PointId, PointId, PointId, PointId, PointId),
    OutsideParity([(PointId, PointId, PointId); 3], bool),
}

impl Hyp {
    pub fn is_config(&self) -> bool {
        !matches!(self, Hyp::Line(_) | Hyp::Circle(_) | Hyp::Eq(..))
    }
    pub fn reads_figure(&self) -> bool {
        matches!(self, Hyp::Between(..) | Hyp::Outside(..) | Hyp::SplitAlike(..) | Hyp::OutsideParity(..))
    }
}

#[derive(Clone, Debug)]
pub struct Spec {
    pub hyps: Vec<Hyp>,
    pub rows: Vec<(Table, LinComb)>,
}

pub const LIBRARY: [AtomKey; 23] = [
    AtomKey::EqualTangents,
    AtomKey::TangentSecant,
    AtomKey::PowerConverse,
    AtomKey::Intercept,
    AtomKey::InterceptConverse,
    AtomKey::BisectorRatio,
    AtomKey::ExtBisectorRatio,
    AtomKey::BisectorConverse,
    AtomKey::ExtBisectorConverse,
    AtomKey::Menelaus,
    AtomKey::MenelausConverse,
    AtomKey::Ceva,
    AtomKey::CevaConverse,
    AtomKey::MidlineConverse,
    AtomKey::Centroid,
    AtomKey::Pythagoras,
    AtomKey::PythagorasConverse,
    AtomKey::IsoscelesConverse,
    AtomKey::PerpBisectorLocus,
    AtomKey::MedianHypotenuse,
    AtomKey::Simson,
    AtomKey::Miquel,
    AtomKey::Reim,
];

pub const TRIG: [AtomKey; 2] = [AtomKey::LawOfSines, AtomKey::ExtLawOfSines];

pub const CONGRUENCE: [AtomKey; 4] = [AtomKey::CongruentSss, AtomKey::CongruentSas, AtomKey::CongruentAsa, AtomKey::CongruentRhs];

pub fn is_theorem(key: AtomKey) -> bool {
    LIBRARY.contains(&key) || TRIG.contains(&key) || CONGRUENCE.contains(&key)
}

pub fn is_congruence(key: AtomKey) -> bool {
    CONGRUENCE.contains(&key)
}

pub fn cost(key: AtomKey) -> u32 {
    match key {
        AtomKey::LawOfSines | AtomKey::ExtLawOfSines | AtomKey::Menelaus | AtomKey::Ceva => 2,
        AtomKey::PowerConverse | AtomKey::InterceptConverse | AtomKey::BisectorConverse | AtomKey::ExtBisectorConverse | AtomKey::ExtBisectorRatio | AtomKey::MenelausConverse | AtomKey::CevaConverse => 2,
        AtomKey::CongruentSss | AtomKey::CongruentSas | AtomKey::CongruentAsa | AtomKey::CongruentRhs => 2,
        _ => 1,
    }
}

fn half() -> Rat {
    Rat::new(1, 2)
}

struct Rows<'t> {
    t: &'t EngineTrace,
}

impl Rows<'_> {
    fn dir(&self, a: PointId, b: PointId) -> Option<LinComb> {
        self.t.dir(a, b)
    }
    fn dm(&self, a: PointId, b: PointId) -> Option<LinComb> {
        self.t.dm(a, b)
    }
    fn ang(&self, x: PointId, y: PointId, z: PointId) -> Option<LinComb> {
        Some(&self.dir(y, z)? - &self.dir(y, x)?)
    }
    fn ex(&self, c: LinComb) -> LinComb {
        self.t.exact(Table::Angle, &c)
    }
    fn perp(&self, l: (PointId, PointId), m: (PointId, PointId)) -> Option<LinComb> {
        let mut r = &self.dir(l.0, l.1)? - &self.dir(m.0, m.1)?;
        r.add_term(ANGLE_UNIT, -&half());
        Some(self.ex(r))
    }
    fn para(&self, l: (PointId, PointId), m: (PointId, PointId)) -> Option<LinComb> {
        Some(self.ex(&self.dir(l.0, l.1)? - &self.dir(m.0, m.1)?))
    }
    fn coll(&self, a: PointId, b: PointId, c: PointId) -> Option<LinComb> {
        Some(self.ex(&self.dir(a, b)? - &self.dir(a, c)?))
    }
    fn cong(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(&self.dm(a, b)? - &self.dm(c, d)?)
    }
    fn ratio(&self, num: &[(PointId, PointId)], den: &[(PointId, PointId)]) -> Option<LinComb> {
        let mut r = LinComb::zero();
        for &(a, b) in num {
            r = &r + &self.dm(a, b)?;
        }
        for &(a, b) in den {
            r = &r - &self.dm(a, b)?;
        }
        Some(r)
    }
    fn konst(&self, k: i64) -> Option<LinComb> {
        self.t.prime_const(&Rat::from_int(k))
    }
    fn inscribed(&self, p: PointId, q: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(self.ex(&self.ang(p, c, q)? - &self.ang(p, d, q)?))
    }
    fn sq(&self, a: PointId, b: PointId) -> Option<LinComb> {
        self.t.single(Table::Sq, a, b)
    }
    fn sine(&self, v: PointId, p: PointId, q: PointId) -> Option<LinComb> {
        let (lo, hi) = (p.min(q), p.max(q));
        let s = self.t.sines.iter().filter(|s| s.v == v && s.p.min(s.q) == lo && s.p.max(s.q) == hi && s.shift.mod_one().is_zero()).map(|s| s.var).min()?;
        Some(LinComb::singleton(self.t.sine_canon(s), Rat::one()))
    }
}

pub fn spec(t: &EngineTrace, key: AtomKey, a: &[PointId]) -> Option<Spec> {
    let r = Rows { t };
    let n = |k: usize| a.len() == k;
    let line = |v: &[PointId]| Hyp::Line(v.to_vec());
    let dist = |v: &[PointId]| Hyp::Distinct(v.to_vec());
    let s = match key {
        AtomKey::EqualTangents if n(4) => {
            let (p, t1, t2, o) = (a[0], a[1], a[2], a[3]);
            Spec {
                hyps: vec![Hyp::Eq(Table::Ratio, r.cong(o, t1, o, t2)?), Hyp::Eq(Table::Angle, r.perp((p, t1), (o, t1))?), Hyp::Eq(Table::Angle, r.perp((p, t2), (o, t2))?), dist(a), Hyp::Triangle(p, t1, t2)],
                rows: vec![(Table::Ratio, r.cong(p, t1, p, t2)?), (Table::Angle, r.ex(&r.ang(t1, p, o)? - &r.ang(o, p, t2)?)), (Table::Angle, r.ex(&r.ang(t1, o, p)? - &r.ang(p, o, t2)?))],
            }
        }
        AtomKey::TangentSecant if n(5) => {
            let (x, tt, p, q, o) = (a[0], a[1], a[2], a[3], a[4]);
            let mut row = r.dm(x, tt)?;
            row.mul_assign_scalar(&Rat::from_int(2));
            Spec {
                hyps: vec![Hyp::Eq(Table::Ratio, r.cong(o, tt, o, p)?), Hyp::Eq(Table::Ratio, r.cong(o, tt, o, q)?), Hyp::Eq(Table::Angle, r.perp((x, tt), (o, tt))?), line(&[x, p, q]), dist(a)],
                rows: vec![(Table::Ratio, &(&row - &r.dm(x, p)?) - &r.dm(x, q)?)],
            }
        }
        AtomKey::PowerConverse if n(5) => {
            let (x, p, q, c, d) = (a[0], a[1], a[2], a[3], a[4]);
            Spec {
                hyps: vec![line(&[x, p, q]), line(&[x, c, d]), Hyp::Eq(Table::Ratio, r.ratio(&[(x, p), (x, q)], &[(x, c), (x, d)])?), dist(a), Hyp::Triangle(x, p, c), Hyp::SplitAlike(x, p, q, c, d)],
                rows: vec![(Table::Angle, r.inscribed(p, q, c, d)?)],
            }
        }
        AtomKey::Intercept if n(5) => {
            let (o, p, q, c, d) = (a[0], a[1], a[2], a[3], a[4]);
            Spec {
                hyps: vec![line(&[o, p, q]), line(&[o, c, d]), Hyp::Eq(Table::Angle, r.para((p, c), (q, d))?), Hyp::Triangle(o, p, c), dist(a)],
                rows: vec![(Table::Ratio, r.ratio(&[(o, p), (o, d)], &[(o, q), (o, c)])?), (Table::Ratio, r.ratio(&[(o, p), (q, d)], &[(o, q), (p, c)])?)],
            }
        }
        AtomKey::InterceptConverse if n(5) => {
            let (o, p, q, c, d) = (a[0], a[1], a[2], a[3], a[4]);
            Spec {
                hyps: vec![line(&[o, p, q]), line(&[o, c, d]), Hyp::Eq(Table::Ratio, r.ratio(&[(o, p), (o, d)], &[(o, q), (o, c)])?), Hyp::Triangle(o, p, c), dist(a), Hyp::SplitAlike(o, p, q, c, d)],
                rows: vec![(Table::Angle, r.para((p, c), (q, d))?)],
            }
        }
        AtomKey::BisectorRatio | AtomKey::ExtBisectorRatio | AtomKey::BisectorConverse | AtomKey::ExtBisectorConverse if n(4) => {
            let (v, b, c, d) = (a[0], a[1], a[2], a[3]);
            let angle = r.ex(&r.ang(b, v, d)? - &r.ang(d, v, c)?);
            let ratio = r.ratio(&[(d, b), (v, c)], &[(d, c), (v, b)])?;
            let side = if matches!(key, AtomKey::BisectorRatio | AtomKey::BisectorConverse) { Hyp::Between(d, b, c) } else { Hyp::Outside(d, b, c) };
            let (hyp, row) = if matches!(key, AtomKey::BisectorRatio | AtomKey::ExtBisectorRatio) { (Hyp::Eq(Table::Angle, angle), (Table::Ratio, ratio)) } else { (Hyp::Eq(Table::Ratio, ratio), (Table::Angle, angle)) };
            Spec { hyps: vec![line(&[b, d, c]), hyp, Hyp::Triangle(v, b, c), dist(a), side], rows: vec![row] }
        }
        AtomKey::Menelaus | AtomKey::MenelausConverse | AtomKey::Ceva if n(6) || (key == AtomKey::Ceva && n(7)) => {
            let (p, q, c, d, e, f) = (a[0], a[1], a[2], a[3], a[4], a[5]);
            let prod = r.ratio(&[(d, q), (e, c), (f, p)], &[(d, c), (e, p), (f, q)])?;
            let mut hyps = vec![line(&[q, d, c]), line(&[c, e, p]), line(&[p, f, q]), Hyp::Triangle(p, q, c), dist(a)];
            match key {
                AtomKey::Menelaus => {
                    hyps.push(line(&[d, e, f]));
                    Spec { hyps, rows: vec![(Table::Ratio, prod)] }
                }
                AtomKey::MenelausConverse => {
                    hyps.push(Hyp::Eq(Table::Ratio, prod));
                    hyps.push(Hyp::OutsideParity([(d, q, c), (e, c, p), (f, p, q)], true));
                    Spec { hyps, rows: vec![(Table::Angle, r.coll(d, e, f)?)] }
                }
                _ => {
                    if a.len() != 7 {
                        return None;
                    }
                    let x = a[6];
                    hyps.extend([line(&[p, x, d]), line(&[q, x, e]), line(&[c, x, f])]);
                    Spec { hyps, rows: vec![(Table::Ratio, prod)] }
                }
            }
        }
        AtomKey::CevaConverse if n(7) => {
            let (p, q, c, d, e, f, x) = (a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            let prod = r.ratio(&[(d, q), (e, c), (f, p)], &[(d, c), (e, p), (f, q)])?;
            Spec {
                hyps: vec![
                    line(&[q, d, c]),
                    line(&[c, e, p]),
                    line(&[p, f, q]),
                    line(&[p, x, d]),
                    line(&[q, x, e]),
                    Hyp::Eq(Table::Ratio, prod),
                    Hyp::Triangle(p, q, c),
                    dist(a),
                    Hyp::OutsideParity([(d, q, c), (e, c, p), (f, p, q)], false),
                ],
                rows: vec![(Table::Angle, r.coll(c, x, f)?)],
            }
        }
        AtomKey::MidlineConverse if n(5) => {
            let (m, nn, p, q, c) = (a[0], a[1], a[2], a[3], a[4]);
            let mut rows = vec![(Table::Ratio, r.cong(nn, p, nn, c)?)];
            if let Some(two) = r.konst(2) {
                rows.push((Table::Ratio, &(&r.dm(q, c)? - &r.dm(m, nn)?) - &two));
            }
            Spec { hyps: vec![Hyp::Eq(Table::Ratio, r.cong(m, p, m, q)?), line(&[p, m, q]), line(&[p, nn, c]), Hyp::Eq(Table::Angle, r.para((m, nn), (q, c))?), Hyp::Triangle(p, q, c), dist(a)], rows }
        }
        AtomKey::Centroid if n(6) => {
            let (g, p, q, c, ma, mb) = (a[0], a[1], a[2], a[3], a[4], a[5]);
            let two = r.konst(2)?;
            Spec {
                hyps: vec![
                    Hyp::Eq(Table::Ratio, r.cong(ma, q, ma, c)?),
                    line(&[q, ma, c]),
                    Hyp::Eq(Table::Ratio, r.cong(mb, c, mb, p)?),
                    line(&[c, mb, p]),
                    line(&[p, g, ma]),
                    line(&[q, g, mb]),
                    Hyp::Triangle(p, q, c),
                    dist(a),
                ],
                rows: vec![(Table::Ratio, &(&r.dm(p, g)? - &r.dm(g, ma)?) - &two), (Table::Ratio, &(&r.dm(q, g)? - &r.dm(g, mb)?) - &two)],
            }
        }
        AtomKey::Pythagoras | AtomKey::PythagorasConverse if n(3) => {
            let (p, v, q) = (a[0], a[1], a[2]);
            let sq = &(&r.sq(p, v)? + &r.sq(v, q)?) - &r.sq(p, q)?;
            let perp = r.perp((v, p), (v, q))?;
            if key == AtomKey::Pythagoras {
                Spec { hyps: vec![Hyp::Eq(Table::Angle, perp), Hyp::Triangle(p, v, q)], rows: vec![(Table::Sq, sq)] }
            } else {
                Spec { hyps: vec![Hyp::Eq(Table::Sq, sq), Hyp::Triangle(p, v, q)], rows: vec![(Table::Angle, perp)] }
            }
        }
        AtomKey::IsoscelesConverse if n(3) => {
            let (o, p, q) = (a[0], a[1], a[2]);
            Spec { hyps: vec![Hyp::Eq(Table::Angle, r.ex(&r.ang(o, p, q)? - &r.ang(p, q, o)?)), Hyp::Triangle(o, p, q)], rows: vec![(Table::Ratio, r.cong(o, p, o, q)?)] }
        }
        AtomKey::PerpBisectorLocus if n(4) => {
            let (x, p, q, m) = (a[0], a[1], a[2], a[3]);
            Spec {
                hyps: vec![Hyp::Eq(Table::Ratio, r.cong(m, p, m, q)?), line(&[p, m, q]), Hyp::Eq(Table::Angle, r.perp((x, m), (p, q))?), dist(a)],
                rows: vec![(Table::Ratio, r.cong(x, p, x, q)?)],
            }
        }
        AtomKey::MedianHypotenuse if n(4) => {
            let (m, p, q, x) = (a[0], a[1], a[2], a[3]);
            Spec {
                hyps: vec![Hyp::Eq(Table::Angle, r.perp((x, p), (x, q))?), Hyp::Eq(Table::Ratio, r.cong(m, p, m, q)?), line(&[p, m, q]), dist(a)],
                rows: vec![(Table::Ratio, r.cong(m, x, m, p)?)],
            }
        }
        AtomKey::Simson if n(7) => {
            let (x, p, q, c, d, e, f) = (a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            Spec {
                hyps: vec![
                    Hyp::Circle(vec![x, p, q, c]),
                    line(&[q, d, c]),
                    Hyp::Eq(Table::Angle, r.perp((x, d), (q, c))?),
                    line(&[c, e, p]),
                    Hyp::Eq(Table::Angle, r.perp((x, e), (c, p))?),
                    line(&[p, f, q]),
                    Hyp::Eq(Table::Angle, r.perp((x, f), (p, q))?),
                    Hyp::Triangle(p, q, c),
                    dist(&[x, p, q, c]),
                    dist(&[d, e, f]),
                ],
                rows: vec![(Table::Angle, r.coll(d, e, f)?)],
            }
        }
        AtomKey::Miquel if n(7) => {
            let (p, q, c, d, e, f, m) = (a[0], a[1], a[2], a[3], a[4], a[5], a[6]);
            Spec {
                hyps: vec![line(&[q, d, c]), line(&[c, e, p]), line(&[p, f, q]), Hyp::Circle(vec![p, e, f, m]), Hyp::Circle(vec![q, f, d, m]), Hyp::Triangle(p, q, c), dist(a)],
                rows: vec![(Table::Angle, r.inscribed(c, d, e, m)?)],
            }
        }
        AtomKey::Reim if n(6) => {
            let (x, y, p, q, c, d) = (a[0], a[1], a[2], a[3], a[4], a[5]);
            Spec {
                hyps: vec![Hyp::Circle(vec![x, y, p, q]), Hyp::Circle(vec![x, y, c, d]), line(&[p, x, c]), line(&[q, y, d]), dist(a)],
                rows: vec![(Table::Angle, r.para((p, q), (c, d))?)],
            }
        }
        AtomKey::LawOfSines if n(3) => {
            let (p, q, c) = (a[0], a[1], a[2]);
            let row = &(&(&r.dm(q, c)? - &r.dm(c, p)?) - &r.sine(p, q, c)?) + &r.sine(q, c, p)?;
            Spec { hyps: vec![Hyp::Triangle(p, q, c)], rows: vec![(Table::Ratio, row)] }
        }
        AtomKey::ExtLawOfSines if n(4) => {
            let (p, q, c, o) = (a[0], a[1], a[2], a[3]);
            let row = &(&(&r.dm(q, c)? - &r.konst(2)?) - &r.dm(o, p)?) - &r.sine(p, q, c)?;
            Spec { hyps: vec![Hyp::Eq(Table::Ratio, r.cong(o, p, o, q)?), Hyp::Eq(Table::Ratio, r.cong(o, p, o, c)?), Hyp::Triangle(p, q, c)], rows: vec![(Table::Ratio, row)] }
        }
        AtomKey::CongruentSss | AtomKey::CongruentSas | AtomKey::CongruentAsa | AtomKey::CongruentRhs if n(6) => {
            let (p, q, c, x, y, z) = (a[0], a[1], a[2], a[3], a[4], a[5]);
            let same = t.orient(p, q, c) == t.orient(x, y, z);
            let corner = |u: PointId, v: PointId, w: PointId, uu: PointId, vv: PointId, ww: PointId| -> Option<LinComb> {
                let l = r.ang(u, v, w)?;
                let m = r.ang(uu, vv, ww)?;
                Some(r.ex(if same { &l - &m } else { &l + &m }))
            };
            let sides = [r.cong(p, q, x, y)?, r.cong(q, c, y, z)?, r.cong(c, p, z, x)?];
            let angles = [corner(q, p, c, y, x, z)?, corner(p, q, c, x, y, z)?, corner(p, c, q, x, z, y)?];
            let (hs, ha): (Vec<usize>, Vec<usize>) = match key {
                AtomKey::CongruentSss => (vec![0, 1, 2], vec![]),
                AtomKey::CongruentSas => (vec![0, 1], vec![1]),
                AtomKey::CongruentAsa => (vec![0], vec![0, 1]),
                _ => (vec![0, 2], vec![]),
            };
            let mut hyps: Vec<Hyp> = hs.iter().map(|&i| Hyp::Eq(Table::Ratio, sides[i].clone())).collect();
            hyps.extend(ha.iter().map(|&i| Hyp::Eq(Table::Angle, angles[i].clone())));
            if key == AtomKey::CongruentRhs {
                hyps.push(Hyp::Eq(Table::Angle, r.perp((q, p), (q, c))?));
                hyps.push(Hyp::Eq(Table::Angle, r.perp((y, x), (y, z))?));
            }
            hyps.retain(|h| !matches!(h, Hyp::Eq(_, r) if r.is_zero()));
            hyps.push(Hyp::Triangle(p, q, c));
            hyps.push(Hyp::Triangle(x, y, z));
            hyps.push(Hyp::Distinct(vec![p, q, c]));
            if [p, q, c] == [x, y, z] {
                return None;
            }
            let mut rows: Vec<(Table, LinComb)> = Vec::new();
            for i in 0..3 {
                if !hs.contains(&i) {
                    rows.push((Table::Ratio, sides[i].clone()));
                }
            }
            for i in 0..3 {
                if !ha.contains(&i) && !(key == AtomKey::CongruentRhs && i == 1) {
                    rows.push((Table::Angle, angles[i].clone()));
                }
            }
            rows.retain(|(_, r)| !r.is_zero());
            if rows.is_empty() {
                return None;
            }
            Spec { hyps, rows }
        }
        _ => return None,
    };
    if s.rows.iter().any(|(tb, row)| row.is_zero() || !t.holds(*tb, row)) {
        return None;
    }
    Some(s)
}

fn sub(t: &EngineTrace, a: PointId, b: PointId) -> (f64, f64) {
    let (p, q) = (t.coord(a), t.coord(b));
    (p.0 - q.0, p.1 - q.1)
}

fn tol(t: &EngineTrace) -> f64 {
    let mut s: f64 = 1.0;
    for c in &t.coords {
        if c.0.is_finite() && c.1.is_finite() {
            s = s.max(c.0.abs()).max(c.1.abs());
        }
    }
    1e-7 * s
}

pub fn distinct(t: &EngineTrace, a: PointId, b: PointId) -> bool {
    a != b && t.dist(a, b) > tol(t)
}

fn strictly_between(t: &EngineTrace, p: PointId, a: PointId, b: PointId) -> bool {
    let (u, v) = (sub(t, a, p), sub(t, b, p));
    distinct(t, p, a) && distinct(t, p, b) && u.0 * v.0 + u.1 * v.1 < 0.0
}

fn strictly_outside(t: &EngineTrace, p: PointId, a: PointId, b: PointId) -> bool {
    let (u, v) = (sub(t, a, p), sub(t, b, p));
    distinct(t, p, a) && distinct(t, p, b) && u.0 * v.0 + u.1 * v.1 > 0.0
}

pub fn config_holds(t: &EngineTrace, h: &Hyp) -> bool {
    match h {
        Hyp::Distinct(v) => (0..v.len()).all(|i| (i + 1..v.len()).all(|j| distinct(t, v[i], v[j]))),
        Hyp::Triangle(a, b, c) => distinct(t, *a, *b) && distinct(t, *b, *c) && distinct(t, *a, *c) && t.orient(*a, *b, *c) != 0,
        Hyp::Between(p, a, b) => strictly_between(t, *p, *a, *b),
        Hyp::Outside(p, a, b) => strictly_outside(t, *p, *a, *b),
        Hyp::SplitAlike(o, a, b, c, d) => {
            let x = (strictly_between(t, *o, *a, *b), strictly_outside(t, *o, *a, *b));
            let y = (strictly_between(t, *o, *c, *d), strictly_outside(t, *o, *c, *d));
            (x.0 || x.1) && x == y
        }
        Hyp::OutsideParity(v, odd) => {
            let mut outside = 0;
            for &(p, a, b) in v {
                if strictly_outside(t, p, a, b) {
                    outside += 1;
                } else if !strictly_between(t, p, a, b) {
                    return false;
                }
            }
            (outside % 2 == 1) == *odd
        }
        _ => true,
    }
}

pub fn stmt(key: AtomKey, a: &[PointId]) -> Option<Stmt> {
    let seg = |x: PointId, y: PointId| Expr::Seg { a: x, b: y };
    let prod = |v: Vec<(Expr, i32)>| Expr::Prod { factors: v };
    let ang = |x: PointId, y: PointId, z: PointId| Expr::Angle { a: x, b: y, c: z, directed: true };
    Some(match key {
        AtomKey::EqualTangents => Stmt::Cong { s1: (a[0], a[1]), s2: (a[0], a[2]) },
        AtomKey::TangentSecant => Stmt::Eq { lhs: prod(vec![(seg(a[0], a[1]), 2)]), rhs: prod(vec![(seg(a[0], a[2]), 1), (seg(a[0], a[3]), 1)]) },
        AtomKey::PowerConverse => Stmt::Cyclic { pts: a[1..5].to_vec() },
        AtomKey::Intercept => Stmt::EqRatio { segs: vec![(a[0], a[1]), (a[0], a[2]), (a[0], a[3]), (a[0], a[4])] },
        AtomKey::InterceptConverse => Stmt::Para { l1: (a[1], a[3]), l2: (a[2], a[4]) },
        AtomKey::BisectorRatio | AtomKey::ExtBisectorRatio => Stmt::EqRatio { segs: vec![(a[3], a[1]), (a[3], a[2]), (a[0], a[1]), (a[0], a[2])] },
        AtomKey::BisectorConverse | AtomKey::ExtBisectorConverse => Stmt::EqAngle { lhs: ang(a[1], a[0], a[3]), rhs: ang(a[3], a[0], a[2]) },
        AtomKey::Menelaus | AtomKey::Ceva => Stmt::Eq {
            lhs: prod(vec![(seg(a[3], a[1]), 1), (seg(a[4], a[2]), 1), (seg(a[5], a[0]), 1)]),
            rhs: prod(vec![(seg(a[3], a[2]), 1), (seg(a[4], a[0]), 1), (seg(a[5], a[1]), 1)]),
        },
        AtomKey::MenelausConverse => Stmt::Coll { pts: vec![a[3], a[4], a[5]] },
        AtomKey::CevaConverse => Stmt::Coll { pts: vec![a[2], a[6], a[5]] },
        AtomKey::MidlineConverse => Stmt::Cong { s1: (a[1], a[2]), s2: (a[1], a[4]) },
        AtomKey::Centroid => Stmt::RatioConst { s1: (a[1], a[0]), s2: (a[0], a[4]), value: Rat::from_int(2) },
        AtomKey::Pythagoras => Stmt::Eq { lhs: Expr::Lin { terms: vec![(Rat::one(), Expr::Sq { a: a[0], b: a[1] }), (Rat::one(), Expr::Sq { a: a[1], b: a[2] })] }, rhs: Expr::Sq { a: a[0], b: a[2] } },
        AtomKey::PythagorasConverse => Stmt::Perp { l1: (a[1], a[0]), l2: (a[1], a[2]) },
        AtomKey::IsoscelesConverse => Stmt::Cong { s1: (a[0], a[1]), s2: (a[0], a[2]) },
        AtomKey::PerpBisectorLocus => Stmt::Cong { s1: (a[0], a[1]), s2: (a[0], a[2]) },
        AtomKey::MedianHypotenuse => Stmt::Cong { s1: (a[0], a[3]), s2: (a[0], a[1]) },
        AtomKey::Simson => Stmt::Coll { pts: vec![a[4], a[5], a[6]] },
        AtomKey::Miquel => Stmt::Cyclic { pts: vec![a[2], a[3], a[4], a[6]] },
        AtomKey::Reim => Stmt::Para { l1: (a[2], a[3]), l2: (a[4], a[5]) },
        AtomKey::LawOfSines => {
            let s = |v: PointId, p: PointId, q: PointId| Expr::Sin { angle: Box::new(Expr::Angle { a: p, b: v, c: q, directed: false }) };
            Stmt::Eq { lhs: prod(vec![(seg(a[1], a[2]), 1), (s(a[0], a[1], a[2]), -1)]), rhs: prod(vec![(seg(a[2], a[0]), 1), (s(a[1], a[2], a[0]), -1)]) }
        }
        AtomKey::CongruentSss | AtomKey::CongruentSas | AtomKey::CongruentAsa | AtomKey::CongruentRhs => Stmt::Congruent { t1: (a[0], a[1], a[2]), t2: (a[3], a[4], a[5]), opposite: false },
        AtomKey::ExtLawOfSines => Stmt::Eq {
            lhs: seg(a[1], a[2]),
            rhs: prod(vec![(Expr::Num { value: Rat::from_int(2) }, 1), (seg(a[3], a[0]), 1), (Expr::Sin { angle: Box::new(Expr::Angle { a: a[1], b: a[0], c: a[2], directed: false }) }, 1)]),
        },
        _ => return None,
    })
}

pub struct Figure<'t> {
    pub t: &'t EngineTrace,
    pub lines: Vec<Vec<PointId>>,
    pub circles: Vec<Vec<PointId>>,
    pub sines: FxHashSet<(PointId, PointId, PointId)>,
    on: FxHashMap<(PointId, PointId), Vec<PointId>>,
    eps: f64,
}

impl<'t> Figure<'t> {
    pub fn new(t: &'t EngineTrace, lines: Vec<Vec<PointId>>, circles: Vec<Vec<PointId>>) -> Figure<'t> {
        let mut merged: Vec<Vec<PointId>> = Vec::new();
        for l in lines {
            if l.len() < 3 {
                continue;
            }
            if let Some(m) = merged.iter_mut().find(|m| l.iter().filter(|p| m.contains(p)).count() >= 2) {
                for p in l {
                    if !m.contains(&p) {
                        m.push(p);
                    }
                }
            } else {
                merged.push(l);
            }
        }
        for m in merged.iter_mut() {
            m.sort_unstable();
            m.dedup();
        }
        merged.sort();
        let mut on: FxHashMap<(PointId, PointId), Vec<PointId>> = FxHashMap::default();
        for m in &merged {
            for &a in m {
                for &b in m {
                    if a != b {
                        let e = on.entry((a, b)).or_default();
                        for &p in m {
                            if p != a && p != b && !e.contains(&p) {
                                e.push(p);
                            }
                        }
                    }
                }
            }
        }
        for v in on.values_mut() {
            v.sort_unstable();
        }
        let mut cs: Vec<Vec<PointId>> = circles.into_iter().filter(|c| c.len() >= 4).map(|mut c| {
            c.sort_unstable();
            c.dedup();
            c
        }).collect();
        cs.sort();
        cs.dedup();
        Figure { t, lines: merged, circles: cs, sines: FxHashSet::default(), on, eps: tol(t) }
    }

    pub fn on(&self, a: PointId, b: PointId) -> &[PointId] {
        self.on.get(&(a, b)).map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn angle_of(&self, a: PointId, b: PointId) -> f64 {
        let (dx, dy) = sub(self.t, b, a);
        dy.atan2(dx)
    }

    fn line_close(&self, x: f64) -> bool {
        let m = x.rem_euclid(std::f64::consts::PI);
        m < 1e-7 || std::f64::consts::PI - m < 1e-7
    }

    pub fn perp(&self, l: (PointId, PointId), m: (PointId, PointId)) -> bool {
        self.d(l.0, l.1) && self.d(m.0, m.1) && self.line_close(self.angle_of(l.0, l.1) - self.angle_of(m.0, m.1) - std::f64::consts::FRAC_PI_2)
    }

    pub fn para(&self, l: (PointId, PointId), m: (PointId, PointId)) -> bool {
        self.d(l.0, l.1) && self.d(m.0, m.1) && self.line_close(self.angle_of(l.0, l.1) - self.angle_of(m.0, m.1))
    }

    pub fn d(&self, a: PointId, b: PointId) -> bool {
        a != b && self.t.dist(a, b) > self.eps
    }

    pub fn eqlen(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> bool {
        let (x, y) = (self.t.dist(a, b), self.t.dist(c, d));
        x > self.eps && (x - y).abs() < 1e-7 * x.max(1.0)
    }

    pub fn concyclic(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> bool {
        let p = [a, b, c, d].map(|x| self.t.coord(x));
        let m = |i: usize| {
            let (x, y) = (p[i].0 - p[3].0, p[i].1 - p[3].1);
            (x, y, x * x + y * y)
        };
        let (r0, r1, r2) = (m(0), m(1), m(2));
        let det = r0.0 * (r1.1 * r2.2 - r1.2 * r2.1) - r0.1 * (r1.0 * r2.2 - r1.2 * r2.0) + r0.2 * (r1.0 * r2.1 - r1.1 * r2.0);
        let s = r0.2.max(r1.2).max(r2.2).max(1e-12);
        det.abs() < 1e-9 * s * s.sqrt() && self.t.orient(a, b, c) != 0
    }

    fn mid(&self, m: PointId, a: PointId, b: PointId) -> bool {
        self.d(a, b) && self.eqlen(m, a, m, b) && self.on(a, b).contains(&m)
    }
}

pub fn congruence_candidates(pairs: &[([PointId; 3], [PointId; 3])]) -> Vec<(AtomKey, Vec<PointId>)> {
    let mut out = Vec::new();
    for (t1, t2) in pairs {
        for k in 0..3 {
            let rot = |v: &[PointId; 3]| [v[k], v[(k + 1) % 3], v[(k + 2) % 3]];
            let (a, b) = (rot(t1), rot(t2));
            let args: Vec<PointId> = a.iter().chain(b.iter()).copied().collect();
            if k == 0 {
                out.push((AtomKey::CongruentSss, args.clone()));
            }
            out.push((AtomKey::CongruentSas, args.clone()));
            out.push((AtomKey::CongruentAsa, args.clone()));
            out.push((AtomKey::CongruentRhs, args));
        }
    }
    out
}

pub fn candidates(fig: &Figure, budget: usize) -> Vec<(AtomKey, Vec<PointId>)> {
    let t = fig.t;
    let n = t.n as PointId;
    let pts: Vec<PointId> = (0..n).filter(|&p| t.subst.get(p as usize).copied().unwrap_or(p) == p && t.coord(p).0.is_finite()).collect();
    let mut out: Vec<(AtomKey, Vec<PointId>)> = Vec::new();
    let mut seen: FxHashSet<(AtomKey, Vec<PointId>)> = FxHashSet::default();
    let mut push = |out: &mut Vec<(AtomKey, Vec<PointId>)>, k: AtomKey, v: Vec<PointId>| {
        if out.len() < budget && seen.insert((k, v.clone())) {
            out.push((k, v));
        }
    };
    for &o in &pts {
        for &t1 in &pts {
            for &t2 in &pts {
                if t1 >= t2 || o == t1 || o == t2 || !fig.eqlen(o, t1, o, t2) {
                    continue;
                }
                for &p in &pts {
                    if [o, t1, t2].contains(&p) || !fig.d(p, t1) || !fig.d(p, t2) {
                        continue;
                    }
                    if fig.perp((p, t1), (o, t1)) && fig.perp((p, t2), (o, t2)) {
                        push(&mut out, AtomKey::EqualTangents, vec![p, t1, t2, o]);
                    }
                }
            }
        }
    }
    for &o in &pts {
        for &tt in &pts {
            if o == tt {
                continue;
            }
            for &x in &pts {
                if x == o || x == tt || !fig.perp((x, tt), (o, tt)) {
                    continue;
                }
                for &p in &pts {
                    for &q in fig.on(x, p) {
                        if p < q && ![o, tt].contains(&p) && ![o, tt].contains(&q) && fig.eqlen(o, tt, o, p) && fig.eqlen(o, tt, o, q) {
                            push(&mut out, AtomKey::TangentSecant, vec![x, tt, p, q, o]);
                        }
                    }
                }
            }
        }
    }
    for &x in &pts {
        let mut through: Vec<&Vec<PointId>> = fig.lines.iter().filter(|l| l.contains(&x)).collect();
        through.sort();
        for (i, l1) in through.iter().enumerate() {
            for l2 in &through[i + 1..] {
                let a1: Vec<PointId> = l1.iter().copied().filter(|&p| p != x).collect();
                let a2: Vec<PointId> = l2.iter().copied().filter(|&p| p != x).collect();
                for (ia, &p) in a1.iter().enumerate() {
                    for &q in &a1[ia + 1..] {
                        for (ic, &c) in a2.iter().enumerate() {
                            for &d in &a2[ic + 1..] {
                                if fig.concyclic(p, q, c, d) {
                                    push(&mut out, AtomKey::PowerConverse, vec![x, p, q, c, d]);
                                }
                                for (pp, qq) in [(p, q), (q, p)] {
                                    for (cc, dd) in [(c, d), (d, c)] {
                                        if fig.para((pp, cc), (qq, dd)) && pp < qq {
                                            push(&mut out, AtomKey::Intercept, vec![x, pp, qq, cc, dd]);
                                            push(&mut out, AtomKey::InterceptConverse, vec![x, pp, qq, cc, dd]);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for &v in &pts {
        for l in &fig.lines {
            if l.contains(&v) {
                continue;
            }
            for (ib, &b) in l.iter().enumerate() {
                for &c in &l[ib + 1..] {
                    for &d in l {
                        if d == b || d == c || !fig.d(v, d) {
                            continue;
                        }
                        let (ab, ad, ac) = (fig.angle_of(v, b), fig.angle_of(v, d), fig.angle_of(v, c));
                        if !fig.line_close((ad - ab) - (ac - ad)) {
                            continue;
                        }
                        let inside = strictly_between(t, d, b, c);
                        let (r, cv) = if inside { (AtomKey::BisectorRatio, AtomKey::BisectorConverse) } else { (AtomKey::ExtBisectorRatio, AtomKey::ExtBisectorConverse) };
                        push(&mut out, r, vec![v, b, c, d]);
                        push(&mut out, cv, vec![v, b, c, d]);
                    }
                }
            }
        }
    }
    let tris: Vec<[PointId; 3]> = {
        let mut v = Vec::new();
        for (i, &a) in pts.iter().enumerate() {
            for (j, &b) in pts.iter().enumerate().skip(i + 1) {
                for &c in &pts[j + 1..] {
                    if t.orient(a, b, c) != 0 && [fig.on(a, b).is_empty(), fig.on(b, c).is_empty(), fig.on(c, a).is_empty()].iter().filter(|e| !**e).count() >= 2 {
                        v.push([a, b, c]);
                    }
                }
            }
        }
        v
    };
    for tri in &tris {
        let [a, b, c] = *tri;
        for &d in fig.on(b, c) {
            for &e in fig.on(c, a) {
                for &f in fig.on(a, b) {
                    let v = [a, b, c, d, e, f];
                    if (0..6).any(|i| (i + 1..6).any(|j| v[i] == v[j])) {
                        continue;
                    }
                    if fig.on(d, e).contains(&f) {
                        push(&mut out, AtomKey::Menelaus, v.to_vec());
                        push(&mut out, AtomKey::MenelausConverse, v.to_vec());
                        continue;
                    }
                    let px: Vec<PointId> = fig.on(a, d).iter().copied().filter(|x| fig.on(b, e).contains(x) && ![a, b, c, d, e, f].contains(x)).collect();
                    for x in px {
                        if fig.on(c, f).contains(&x) {
                            let mut w = v.to_vec();
                            w.push(x);
                            push(&mut out, AtomKey::Ceva, w.clone());
                        }
                        if t.orient(c, x, f) == 0 {
                            let mut w = v.to_vec();
                            w.push(x);
                            push(&mut out, AtomKey::CevaConverse, w);
                        }
                    }
                }
            }
        }
    }
    for tri in &tris {
        for k in 0..3 {
            let (a, b, c) = (tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]);
            for &m in fig.on(a, b) {
                if !fig.mid(m, a, b) {
                    continue;
                }
                for (p, q) in [(a, b), (b, a)] {
                    for &nn in fig.on(p, c) {
                        if nn != m && fig.para((m, nn), (q, c)) {
                            push(&mut out, AtomKey::MidlineConverse, vec![m, nn, p, q, c]);
                        }
                    }
                }
            }
            for &ma in fig.on(b, c) {
                if !fig.mid(ma, b, c) {
                    continue;
                }
                for &mb in fig.on(c, a) {
                    if !fig.mid(mb, c, a) {
                        continue;
                    }
                    for &g in fig.on(a, ma) {
                        if fig.on(b, mb).contains(&g) {
                            push(&mut out, AtomKey::Centroid, vec![g, a, b, c, ma, mb]);
                        }
                    }
                }
            }
        }
    }
    for &v in &pts {
        for &p in &pts {
            for &q in &pts {
                if p >= q || p == v || q == v || t.orient(p, v, q) == 0 {
                    continue;
                }
                if fig.perp((v, p), (v, q)) && t.var(Table::Sq, p, v).is_some() && t.var(Table::Sq, v, q).is_some() && t.var(Table::Sq, p, q).is_some() {
                    push(&mut out, AtomKey::Pythagoras, vec![p, v, q]);
                    push(&mut out, AtomKey::PythagorasConverse, vec![p, v, q]);
                }
                if fig.eqlen(v, p, v, q) {
                    push(&mut out, AtomKey::IsoscelesConverse, vec![v, p, q]);
                }
            }
        }
    }
    for l in &fig.lines {
        for &m in l {
            for &p in l {
                for &q in l {
                    if p >= q || m == p || m == q || !fig.mid(m, p, q) {
                        continue;
                    }
                    for &x in &pts {
                        if [m, p, q].contains(&x) {
                            continue;
                        }
                        if fig.perp((x, m), (p, q)) {
                            push(&mut out, AtomKey::PerpBisectorLocus, vec![x, p, q, m]);
                        }
                        if fig.perp((x, p), (x, q)) {
                            push(&mut out, AtomKey::MedianHypotenuse, vec![m, p, q, x]);
                        }
                    }
                }
            }
        }
    }
    for circ in &fig.circles {
        for &x in circ {
            let rest: Vec<PointId> = circ.iter().copied().filter(|&p| p != x).collect();
            for (i, &a) in rest.iter().enumerate() {
                for (j, &b) in rest.iter().enumerate().skip(i + 1) {
                    for &c in &rest[j + 1..] {
                        let foot = |u: PointId, w: PointId| -> Option<PointId> { fig.on(u, w).iter().copied().find(|&f| f != x && fig.perp((x, f), (u, w))) };
                        if let (Some(d), Some(e), Some(f)) = (foot(b, c), foot(c, a), foot(a, b)) {
                            push(&mut out, AtomKey::Simson, vec![x, a, b, c, d, e, f]);
                        }
                    }
                }
            }
        }
    }
    let ncirc = fig.circles.len();
    for i in 0..ncirc {
        for j in 0..ncirc {
            if i == j {
                continue;
            }
            let (c1, c2) = (&fig.circles[i], &fig.circles[j]);
            let common: Vec<PointId> = c1.iter().copied().filter(|p| c2.contains(p)).collect();
            if common.len() != 2 {
                continue;
            }
            for &(x, y) in &[(common[0], common[1]), (common[1], common[0])] {
                for &p in c1.iter().filter(|p| !common.contains(p)) {
                    for &q in c1.iter().filter(|q| !common.contains(q)) {
                        if p == q {
                            continue;
                        }
                        for &c in fig.on(p, x).iter().filter(|c| c2.contains(c) && !common.contains(c)) {
                            for &d in fig.on(q, y).iter().filter(|d| c2.contains(d) && !common.contains(d)) {
                                if i < j {
                                    push(&mut out, AtomKey::Reim, vec![x, y, p, q, c, d]);
                                }
                            }
                        }
                    }
                }
                let (m, f) = (x, y);
                for &p in c1.iter().filter(|p| !common.contains(p)) {
                    for &e in c1.iter().filter(|e| !common.contains(e) && **e != p) {
                        for &q in fig.on(p, f).iter().filter(|q| c2.contains(q) && !common.contains(q)) {
                            for &d in c2.iter().filter(|d| !common.contains(d) && **d != q) {
                                for &c in fig.on(p, e) {
                                    if fig.on(q, d).contains(&c) && ![m, f, p, e, q, d].contains(&c) {
                                        push(&mut out, AtomKey::Miquel, vec![p, q, c, d, e, f, m]);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut sine_tris: Vec<[PointId; 3]> = fig.sines.iter().map(|&(v, p, q)| [v, p, q]).collect();
    sine_tris.sort_unstable();
    sine_tris.dedup();
    let has = |v: PointId, p: PointId, q: PointId| fig.sines.contains(&(v, p.min(q), p.max(q)));
    for [a, p, q] in sine_tris {
        for (b, c) in [(p, q), (q, p)] {
            if !has(b, c, a) {
                continue;
            }
            push(&mut out, AtomKey::LawOfSines, vec![a, b, c]);
            for &o in &pts {
                if ![a, b, c].contains(&o) && fig.eqlen(o, a, o, b) && fig.eqlen(o, a, o, c) {
                    push(&mut out, AtomKey::ExtLawOfSines, vec![a, b, c, o]);
                }
            }
        }
    }
    out
}
