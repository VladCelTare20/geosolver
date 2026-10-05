use super::cert::{certify_greedy, Basis};
use super::trace::{EngineTrace, Table};
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::{LinComb, VarId};
use crate::predicate::{PointId, Predicate};
use crate::proof::{FactId, Reason};
use crate::rational::Rat;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::Instant;

pub const QBASE: VarId = 1 << 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactClass {
    Hyp,
    HypReg,
    TheoremReg(FactId),
    MergeReg(FactId),
    Silent,
    SilentHyp,
    Derived,
    Outside,
}

#[derive(Clone, Debug)]
pub struct AuxInfo {
    pub point: PointId,
    pub name: String,
    pub desc: String,
}

#[derive(Clone, Debug)]
pub struct LineObj {
    pub pts: Vec<PointId>,
    pub fact: FactId,
    pub src: BTreeSet<FactId>,
}

#[derive(Clone, Debug)]
pub struct CircleObj {
    pub pts: Vec<PointId>,
    pub fact: FactId,
    pub src: BTreeSet<FactId>,
    pub hyp: bool,
}

pub struct Quot {
    pub cls: FxHashMap<VarId, (u32, i64)>,
    pub members: Vec<Vec<PointId>>,
}

impl Quot {
    pub fn q(&self, c: &LinComb) -> LinComb {
        let mut out = LinComb::zero();
        for (v, k) in c.terms.iter() {
            if *v == ANGLE_UNIT {
                out.add_term(ANGLE_UNIT, k.clone());
            } else if let Some(&(cl, off)) = self.cls.get(v) {
                out.add_term(QBASE + cl, k.clone());
                if off != 0 {
                    out.add_term(ANGLE_UNIT, k * &Rat::from_int(off));
                }
            } else {
                out.add_term(*v, k.clone());
            }
        }
        out
    }

    pub fn class_of(&self, v: VarId) -> Option<u32> {
        self.cls.get(&v).map(|x| x.0)
    }
}

pub struct Ctx<'a> {
    pub t: &'a EngineTrace,
    pub closure: Vec<FactId>,
    pub in_cl: Vec<bool>,
    pub class: Vec<FactClass>,
    pub quot: Quot,
    pub piv: [Vec<bool>; 4],
    pub lines: Vec<LineObj>,
    pub circles: Vec<CircleObj>,
    pub goal: Predicate,
    pub deps: Vec<FactId>,
    pub aux: Vec<AuxInfo>,
    pub deadline: Option<Instant>,
    pub hyp_pred: FxHashMap<FactId, Predicate>,
    src_memo: RefCell<FxHashMap<FactId, BTreeSet<FactId>>>,
}

pub fn transfer_add_row(t: &EngineTrace, f: FactId) -> Option<LinComb> {
    let Reason::TransferAddMul(a, b) = &t.facts[f as usize].reason else { return None };
    let ratio = t.dist(b.0, b.1) / t.dist(a.0, a.1).max(1e-300);
    let k = super::classify::small_rational(ratio)?;
    let mut kx = t.single(Table::Add, a.0, a.1)?;
    let y = t.single(Table::Add, b.0, b.1)?;
    kx.mul_assign_scalar(&k);
    Some(&y - &kx)
}

fn registration_of(t: &EngineTrace, f: FactId) -> Option<FactId> {
    let fact = &t.facts[f as usize];
    if !matches!(fact.reason, Reason::Collinear(_) | Reason::Concyclic(_)) {
        return None;
    }
    if let Some(&g) = fact.premises.iter().find(|&&g| g + 1 == f) {
        return Some(g);
    }
    fact.premises
        .iter()
        .copied()
        .find(|&g| matches!(t.facts[g as usize].reason, Reason::PointMerge(..) | Reason::TangentMerge(..)))
}

impl<'a> Ctx<'a> {
    pub fn new(t: &'a EngineTrace, goal: &Predicate, deps: &[FactId], aux: &[AuxInfo], deadline: Option<Instant>) -> Ctx<'a> {
        let closure = t.closure(deps);
        let mut in_cl = vec![false; t.facts.len()];
        for &f in &closure {
            in_cl[f as usize] = true;
        }
        let piv = [t.lhs[0].clone(), t.lhs[1].clone(), t.lhs[2].clone(), t.lhs[3].clone()];
        let mut hyp_pred = FxHashMap::default();
        for &f in &closure {
            if let Reason::Assumption(s) | Reason::Construction(s) = &t.facts[f as usize].reason {
                if let Some(p) = t.parse_assumption(s) {
                    hyp_pred.insert(f, p);
                }
            }
        }
        let mut class = vec![FactClass::Outside; t.facts.len()];
        for &f in &closure {
            class[f as usize] = Self::raw_class(t, f);
        }
        let mut g = goal.clone();
        g.points = goal.points.iter().map(|&p| t.subst.get(p as usize).copied().unwrap_or(p)).collect();
        let quot = Self::build_quot(t, &closure, &class);
        let mut cx = Ctx {
            t,
            closure,
            in_cl,
            class,
            quot,
            piv,
            lines: Vec::new(),
            circles: Vec::new(),
            goal: g,
            deps: deps.to_vec(),
            aux: aux.to_vec(),
            deadline,
            hyp_pred,
            src_memo: RefCell::new(FxHashMap::default()),
        };
        cx.refine_silent();
        cx.build_objects();
        cx
    }

    fn raw_class(t: &EngineTrace, f: FactId) -> FactClass {
        let fact = &t.facts[f as usize];
        match &fact.reason {
            Reason::Assumption(_) | Reason::Construction(_) => return FactClass::Hyp,
            Reason::EqualRadius(..) | Reason::TransferAddMul(..) => return FactClass::Silent,
            Reason::Concyclic(p) if {
                let mut q = p.clone();
                q.sort_unstable();
                q.dedup();
                q.len() < 4
            } => return FactClass::Silent,
            Reason::SimilarTriangles(a, b) => {
                let mut s1 = [a.0, a.1, a.2];
                let mut s2 = [b.0, b.1, b.2];
                s1.sort_unstable();
                s2.sort_unstable();
                if s1 == s2 {
                    return FactClass::Silent;
                }
            }
            _ => {}
        }
        if let Some(g) = registration_of(t, f) {
            return match &t.facts[g as usize].reason {
                Reason::Assumption(_) | Reason::Construction(_) => FactClass::HypReg,
                Reason::Theorem(..) | Reason::Formula(..) => FactClass::TheoremReg(g),
                Reason::PointMerge(..) | Reason::TangentMerge(..) => FactClass::MergeReg(g),
                Reason::EqualRadius(..) => FactClass::Silent,
                _ => FactClass::Derived,
            };
        }
        FactClass::Derived
    }

    fn build_quot(t: &EngineTrace, closure: &[FactId], class: &[FactClass]) -> Quot {
        let n = t.n;
        let mut parent: Vec<usize> = (0..n * n).collect();
        fn find(p: &mut Vec<usize>, x: usize) -> usize {
            let mut r = x;
            while p[r] != r {
                r = p[r];
            }
            let mut y = x;
            while p[y] != r {
                let nx = p[y];
                p[y] = r;
                y = nx;
            }
            r
        }
        let key = |a: PointId, b: PointId| if a < b { a as usize * n + b as usize } else { b as usize * n + a as usize };
        for &f in closure {
            if class[f as usize] != FactClass::HypReg {
                continue;
            }
            if let Reason::Collinear(pts) = &t.facts[f as usize].reason {
                let mut pairs: Vec<usize> = Vec::new();
                for (i, &a) in pts.iter().enumerate() {
                    for &b in &pts[i + 1..] {
                        if t.var(Table::Angle, a, b).is_some() {
                            pairs.push(key(a, b));
                        }
                    }
                }
                for w in pairs.windows(2) {
                    let (x, y) = (find(&mut parent, w[0]), find(&mut parent, w[1]));
                    if x != y {
                        parent[x.max(y)] = x.min(y);
                    }
                }
            }
        }
        let mut cls: FxHashMap<VarId, (u32, i64)> = FxHashMap::default();
        let mut root_id: FxHashMap<usize, (u32, f64)> = FxHashMap::default();
        let mut members: Vec<Vec<PointId>> = Vec::new();
        let vals = &t.values[0];
        for a in 0..n as PointId {
            for b in (a + 1)..n as PointId {
                let Some(v) = t.var(Table::Angle, a, b) else { continue };
                if cls.contains_key(&v) {
                    continue;
                }
                let r = find(&mut parent, key(a, b));
                let val = vals.get(v as usize).copied().unwrap_or(0.0);
                let (id, rv) = *root_id.entry(r).or_insert_with(|| {
                    members.push(Vec::new());
                    ((members.len() - 1) as u32, val)
                });
                let off = (val - rv).round() as i64;
                cls.insert(v, (id, off));
                let m = &mut members[id as usize];
                for p in [a, b] {
                    if !m.contains(&p) {
                        m.push(p);
                    }
                }
            }
        }
        for m in members.iter_mut() {
            m.sort_unstable();
        }
        Quot { cls, members }
    }

    fn refine_silent(&mut self) {
        let t = self.t;
        let mut hyp_circles: Vec<Vec<PointId>> = Vec::new();
        for &f in &self.closure {
            if self.class[f as usize] == FactClass::HypReg {
                if let Reason::Concyclic(p) = &t.facts[f as usize].reason {
                    hyp_circles.push(p.clone());
                }
            }
        }
        let mut hyp_rows: [Vec<&LinComb>; 4] = Default::default();
        for tb in Table::ALL {
            for (g, r) in &t.rows[tb.idx()] {
                if self.in_cl[*g as usize] && matches!(self.class[*g as usize], FactClass::Hyp | FactClass::HypReg) {
                    hyp_rows[tb.idx()].push(r);
                }
            }
        }
        let mut hbs: Vec<Basis> = Table::ALL.iter().map(|tb| Basis::new(&self.piv[tb.idx()], false)).collect();
        for tb in Table::ALL {
            for (i, r) in hyp_rows[tb.idx()].iter().enumerate() {
                hbs[tb.idx()].insert(i as u32, r);
            }
        }
        for &f in &self.closure.clone() {
            if self.class[f as usize] != FactClass::Derived || !matches!(t.facts[f as usize].reason, Reason::TransferAddMul(..)) {
                continue;
            }
            let mut any = false;
            let mut all = true;
            for tb in Table::ALL {
                for r in t.rows_of(tb, f) {
                    any = true;
                    all &= hbs[tb.idx()].contains(r);
                }
            }
            let additive = transfer_add_row(t, f).is_some_and(|row| hbs[Table::Add.idx()].contains(&row));
            if (any && all) || additive {
                self.class[f as usize] = FactClass::SilentHyp;
            }
        }
        let hb = &hbs[1];
        if std::env::var_os("HP_DEBUG").is_some() {
            for &f in &self.closure {
                eprintln!("fact {f} class {:?} premises {:?} reason {:?}", self.class[f as usize], t.facts[f as usize].premises, t.facts[f as usize].reason);
            }
        }
        for &f in &self.closure.clone() {
            if self.class[f as usize] != FactClass::Derived {
                continue;
            }
            if let Reason::Concyclic(p) = &t.facts[f as usize].reason {
                if hyp_circles.iter().any(|c| p.iter().all(|x| c.contains(x))) {
                    self.class[f as usize] = FactClass::SilentHyp;
                    continue;
                }
                let n = t.n as PointId;
                let by_centre = (0..n).any(|o| {
                    !p.contains(&o)
                        && p.iter().all(|&q| t.dm(o, q).is_some())
                        && p[1..].iter().all(|&q| {
                            let tgt = &t.dm(o, p[0]).unwrap() - &t.dm(o, q).unwrap();
                            t.holds(Table::Ratio, &tgt) && hb.contains(&tgt)
                        })
                });
                if by_centre {
                    self.class[f as usize] = FactClass::SilentHyp;
                }
            }
        }
    }

    fn build_objects(&mut self) {
        let t = self.t;
        for &f in &self.closure.clone() {
            match &t.facts[f as usize].reason {
                Reason::Collinear(p) => {
                    let src = self.fact_sources(f);
                    let mut pts = p.clone();
                    pts.sort_unstable();
                    self.lines.push(LineObj { pts, fact: f, src });
                }
                Reason::Concyclic(p) => {
                    let src = self.fact_sources(f);
                    let mut pts = p.clone();
                    pts.sort_unstable();
                    pts.dedup();
                    let hyp = self.class[f as usize] == FactClass::HypReg;
                    self.circles.push(CircleObj { pts, fact: f, src, hyp });
                }
                _ => {}
            }
        }
    }

    pub fn is_hyp(&self, f: FactId) -> bool {
        matches!(self.class.get(f as usize), Some(FactClass::Hyp | FactClass::HypReg))
    }

    pub fn displayable(&self, f: FactId) -> FactId {
        match self.class.get(f as usize) {
            Some(FactClass::TheoremReg(g)) | Some(FactClass::MergeReg(g)) => *g,
            _ => f,
        }
    }

    pub fn fact_sources(&self, f: FactId) -> BTreeSet<FactId> {
        if let Some(s) = self.src_memo.borrow().get(&f) {
            return s.clone();
        }
        let out: BTreeSet<FactId> = match self.class.get(f as usize).copied().unwrap_or(FactClass::Outside) {
            FactClass::Hyp | FactClass::HypReg | FactClass::SilentHyp | FactClass::Outside => BTreeSet::new(),
            FactClass::Derived => [f].into_iter().collect(),
            FactClass::TheoremReg(g) | FactClass::MergeReg(g) => [g].into_iter().collect(),
            FactClass::Silent if transfer_add_row(self.t, f).and_then(|row| self.support_facts(Table::Add, &row, f)).is_some() => {
                let row = transfer_add_row(self.t, f).unwrap();
                let mut s = BTreeSet::new();
                for g in self.support_facts(Table::Add, &row, f).unwrap_or_default() {
                    s.extend(self.fact_sources(g));
                }
                s
            }
            FactClass::Silent => {
                let mut s = BTreeSet::new();
                for tb in Table::ALL {
                    let rows: Vec<LinComb> = self.t.rows_of(tb, f).cloned().collect();
                    for r in rows.iter().take(12) {
                        match self.support_facts(tb, r, f) {
                            Some(sup) => {
                                for g in sup {
                                    s.extend(self.fact_sources(g));
                                }
                            }
                            None => {
                                s.insert(f);
                            }
                        }
                    }
                }
                s
            }
        };
        self.src_memo.borrow_mut().insert(f, out.clone());
        out
    }

    pub fn row_cost(&self, g: FactId) -> u32 {
        match self.class[g as usize] {
            FactClass::Hyp | FactClass::HypReg => 1,
            FactClass::Silent | FactClass::SilentHyp => 2,
            _ => 5,
        }
    }

    pub fn support_facts(&self, tb: Table, target: &LinComb, h: FactId) -> Option<Vec<FactId>> {
        let rows: Vec<(u32, FactId, &LinComb)> = self.t.rows[tb.idx()]
            .iter()
            .filter(|(g, _)| *g < h && self.in_cl[*g as usize])
            .map(|(g, r)| (self.row_cost(*g), *g, r))
            .collect();
        let mut sorted: Vec<(u32, FactId, &LinComb)> = rows;
        sorted.sort_by_key(|x| (x.0, x.1));
        let input: Vec<(u32, &LinComb)> = sorted.iter().map(|x| (x.0, x.2)).collect();
        let cert = certify_greedy(&self.piv[tb.idx()], &input, target, &|_| true)?;
        let mut out: Vec<FactId> = cert.iter().map(|(i, _)| sorted[*i].1).collect();
        out.sort_unstable();
        out.dedup();
        Some(out)
    }

    pub fn timed_out(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }

    pub fn is_aux(&self, p: PointId) -> bool {
        self.aux.iter().any(|a| a.point == p)
    }

    pub fn line_through(&self, pts: &[PointId], h: FactId) -> Option<&LineObj> {
        self.lines.iter().filter(|l| l.fact < h && pts.iter().all(|p| l.pts.contains(p))).min_by_key(|l| (l.src.len(), l.fact))
    }

    pub fn collinear_hyp(&self, pts: &[PointId]) -> bool {
        let mut cl: Option<u32> = None;
        for (i, &a) in pts.iter().enumerate() {
            for &b in &pts[i + 1..] {
                let Some(v) = self.t.var(Table::Angle, a, b) else { continue };
                let c = self.quot.class_of(v);
                if cl.is_none() {
                    cl = c;
                } else if c != cl {
                    return false;
                }
            }
        }
        cl.is_some()
    }
}
