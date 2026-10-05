use crate::elimination::{prime_decomposition, ANGLE_UNIT};
use crate::lincomb::{LinComb, VarId};
use crate::predicate::{PointId, Predicate};
use crate::proof::{Fact, FactId, Reason};
use crate::rational::Rat;
use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Angle,
    Ratio,
    Add,
    Sq,
}

impl Table {
    pub const ALL: [Table; 4] = [Table::Angle, Table::Ratio, Table::Add, Table::Sq];
    pub fn idx(self) -> usize {
        match self {
            Table::Angle => 0,
            Table::Ratio => 1,
            Table::Add => 2,
            Table::Sq => 3,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SineVar {
    pub var: VarId,
    pub v: PointId,
    pub p: PointId,
    pub q: PointId,
    pub flip: bool,
    pub shift: Rat,
}

#[derive(Clone, Debug)]
pub struct EngineTrace {
    pub n: usize,
    pub names: Vec<String>,
    pub coords: Vec<(f64, f64)>,
    pub subst: Vec<PointId>,
    pub facts: Vec<Fact>,
    pub rows: [Vec<(FactId, LinComb)>; 4],
    pub values: [Vec<f64>; 4],
    pub lhs: [Vec<bool>; 4],
    pub pair: [Vec<Option<VarId>>; 4],
    pub primes: Vec<(u64, VarId)>,
    pub sines: Vec<SineVar>,
}

impl EngineTrace {
    pub fn pk(&self, a: PointId, b: PointId) -> usize {
        a as usize * self.n + b as usize
    }

    pub fn var(&self, t: Table, a: PointId, b: PointId) -> Option<VarId> {
        if a == b || a as usize >= self.n || b as usize >= self.n {
            return None;
        }
        self.pair[t.idx()][self.pk(a, b)]
    }

    pub fn single(&self, t: Table, a: PointId, b: PointId) -> Option<LinComb> {
        self.var(t, a, b).map(|v| LinComb::singleton(v, Rat::one()))
    }

    pub fn dir(&self, a: PointId, b: PointId) -> Option<LinComb> {
        self.single(Table::Angle, a, b)
    }

    pub fn dm(&self, a: PointId, b: PointId) -> Option<LinComb> {
        self.single(Table::Ratio, a, b)
    }

    pub fn angle(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(&self.dir(c, d)? - &self.dir(a, b)?)
    }

    pub fn ratio(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(&self.dm(c, d)? - &self.dm(a, b)?)
    }

    pub fn value(&self, t: Table, c: &LinComb) -> f64 {
        let vals = &self.values[t.idx()];
        match t {
            Table::Ratio => c
                .terms
                .iter()
                .map(|(v, k)| k.to_f64() * vals.get(*v as usize).copied().unwrap_or(1.0).ln())
                .sum(),
            _ => c.terms.iter().map(|(v, k)| k.to_f64() * vals.get(*v as usize).copied().unwrap_or(0.0)).sum(),
        }
    }

    pub fn exact(&self, t: Table, c: &LinComb) -> LinComb {
        if t != Table::Angle {
            return c.clone();
        }
        let mut out = c.clone();
        let k = self.value(Table::Angle, c).round() as i64;
        out.add_term(ANGLE_UNIT, Rat::from_int(-k));
        out
    }

    pub fn holds(&self, t: Table, c: &LinComb) -> bool {
        let v = self.value(t, c);
        match t {
            Table::Angle => (v - v.round()).abs() < 1e-7,
            Table::Ratio => v.abs() < 1e-7,
            _ => v.abs() < 1e-6 * (1.0 + c.terms.len() as f64),
        }
    }

    pub fn prime_const(&self, q: &Rat) -> Option<LinComb> {
        if q.is_one() {
            return Some(LinComb::zero());
        }
        if q.is_negative() || q.is_zero() {
            return None;
        }
        let num = q.numer_i64()? as u64;
        let den = q.denom_i64()? as u64;
        let mut comb = LinComb::zero();
        for (p, e) in prime_decomposition(num) {
            let v = self.primes.iter().find(|x| x.0 == p)?.1;
            comb.add_term(v, Rat::from_int(e as i64));
        }
        for (p, e) in prime_decomposition(den) {
            let v = self.primes.iter().find(|x| x.0 == p)?.1;
            comb.add_term(v, Rat::from_int(-(e as i64)));
        }
        Some(comb)
    }

    pub fn sine_key(s: &SineVar) -> (PointId, PointId, PointId, bool, Rat) {
        let sh = s.shift.mod_one();
        if sh.is_zero() || sh == Rat::new(1, 2) {
            (s.v, s.p.min(s.q), s.p.max(s.q), false, sh)
        } else {
            (s.v, s.p, s.q, s.flip, sh)
        }
    }

    pub fn sine_canon(&self, v: VarId) -> VarId {
        let Some(s) = self.sines.iter().find(|x| x.var == v) else { return v };
        let k = Self::sine_key(s);
        self.sines.iter().filter(|x| Self::sine_key(x) == k).map(|x| x.var).min().unwrap_or(v)
    }

    pub fn canon_ratio(&self, c: &LinComb) -> LinComb {
        let mut out = LinComb::zero();
        for (v, k) in c.terms.iter() {
            out.add_term(self.sine_canon(*v), k.clone());
        }
        out
    }

    pub fn closure(&self, used: &[FactId]) -> Vec<FactId> {
        let mut seen = vec![false; self.facts.len()];
        let mut stack: Vec<FactId> = used.to_vec();
        while let Some(f) = stack.pop() {
            let i = f as usize;
            if i >= self.facts.len() || seen[i] {
                continue;
            }
            seen[i] = true;
            stack.extend(self.facts[i].premises.iter().copied());
        }
        (0..self.facts.len() as FactId).filter(|&f| seen[f as usize]).collect()
    }

    pub fn reason(&self, f: FactId) -> Option<&Reason> {
        self.facts.get(f as usize).map(|x| &x.reason)
    }

    pub fn rows_of(&self, t: Table, f: FactId) -> impl Iterator<Item = &LinComb> {
        self.rows[t.idx()].iter().filter(move |(g, _)| *g == f).map(|(_, c)| c)
    }

    pub fn coord(&self, p: PointId) -> (f64, f64) {
        self.coords.get(p as usize).copied().unwrap_or((f64::NAN, f64::NAN))
    }

    pub fn dist(&self, a: PointId, b: PointId) -> f64 {
        let (p, q) = (self.coord(a), self.coord(b));
        ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()
    }

    pub fn orient(&self, a: PointId, b: PointId, c: PointId) -> i32 {
        let (pa, pb, pc) = (self.coord(a), self.coord(b), self.coord(c));
        let d = (pb.0 - pa.0) * (pc.1 - pa.1) - (pb.1 - pa.1) * (pc.0 - pa.0);
        let scale = self.dist(a, b).max(self.dist(a, c)).max(1e-300);
        if d.abs() < 1e-9 * scale * scale {
            0
        } else if d > 0.0 {
            1
        } else {
            -1
        }
    }

    pub fn name(&self, p: PointId) -> &str {
        self.names.get(p as usize).map(String::as_str).unwrap_or("?")
    }

    pub fn point_id(&self, name: &str) -> Option<PointId> {
        self.names.iter().position(|n| n == name).map(|i| i as PointId)
    }

    pub fn parse_assumption(&self, text: &str) -> Option<Predicate> {
        let mut it = text.split_whitespace();
        let name = it.next()?.to_string();
        let mut points = Vec::new();
        let mut constants = Vec::new();
        for tok in it {
            if let Some(p) = self.point_id(tok) {
                points.push(p);
            } else {
                constants.push(parse_rat(tok)?);
            }
        }
        Some(Predicate { name, points, constants })
    }
}

fn parse_rat(s: &str) -> Option<Rat> {
    if let Some((a, b)) = s.split_once('/') {
        let (a, b): (i64, i64) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
        (b != 0).then(|| Rat::new(a, b))
    } else {
        s.trim().parse::<i64>().ok().map(Rat::from_int)
    }
}
