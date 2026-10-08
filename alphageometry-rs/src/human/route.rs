use super::atoms::Atom;
use super::chain::{angle_cost, classes, display_node, ratio_shape, Group, Path};
use super::ctx::{Ctx, QBASE};
use super::trace::Table;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::{LinComb, VarId};
use crate::rational::Rat;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

pub const MAX_LINKS: usize = 8;
pub const BUDGET: usize = 300;
pub const UNDIRECTED_BUDGET: usize = 200;
pub const TOTAL_BUDGET: usize = 3_500;
pub const GOAL_RESERVE: usize = 1_500;

pub const OFF: i64 = i64::MAX;

pub struct Graph {
    table: Table,
    undirected: bool,
    atom: Vec<usize>,
    qs: Vec<LinComb>,
    vars: Vec<Vec<VarId>>,
    fq: Vec<Vec<(VarId, f64)>>,
    index: FxHashMap<VarId, Vec<u32>>,
    binary: Vec<u32>,
    lambdas: Vec<Rat>,
    shown: RefCell<FxHashMap<LinComb, Option<i64>>>,
    pub max_vars: usize,
}

fn integral(c: &LinComb) -> bool {
    c.terms.iter().all(|(v, k)| *v < QBASE || k.is_integer())
}

fn key_vars(cx: &Ctx, table: Table, c: &LinComb) -> Vec<VarId> {
    if table == Table::Angle {
        classes(c).into_iter().map(|x| x.0).collect()
    } else {
        c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).map(|(v, _)| *v).collect()
    }
}

pub fn directed_shape(cx: &Ctx, table: Table, c: &LinComb) -> Option<i64> {
    match table {
        Table::Angle => {
            if !integral(c) || c.terms.iter().any(|(v, _)| *v != ANGLE_UNIT && *v < QBASE) {
                return None;
            }
            let cl = classes(c);
            if cl.is_empty() {
                return Some(0);
            }
            let base = angle_cost(cx, c)?;
            let doubled = cl.iter().all(|(_, k)| k.numer_i64().is_some_and(|n| n % 2 == 0));
            Some(base + if doubled { 14 } else { 0 })
        }
        Table::Ratio => match ratio_shape(cx, c)? {
            0 => Some(0),
            _ => Some(16),
        },
        _ => None,
    }
}

fn wide_ratio_shape(cx: &Ctx, c: &LinComb, max: usize) -> Option<i64> {
    let lhs: Vec<&(VarId, Rat)> = c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).collect();
    if !lhs.iter().all(|(_, k)| k.abs() == Rat::one()) || lhs.len() > max {
        return None;
    }
    Some(match lhs.len() {
        0..=2 => 0,
        3 | 4 => 9,
        _ => 30,
    })
}

fn undirected_shape(cx: &Ctx, c: &LinComb) -> Option<i64> {
    if c.terms.iter().any(|(v, _)| *v != ANGLE_UNIT && *v < QBASE) || classes(c).len() > 4 {
        return None;
    }
    let e = display_node(cx, c, false, &[], &BTreeSet::new())?;
    let sc = super::barem::term_score(&e)?;
    if sc >= 4 {
        return None;
    }
    Some(sc + if super::barem::lin_terms(&e) >= 2 { 9 } else { 0 })
}

impl Graph {
    pub fn new(cx: &Ctx, atoms: &[Atom], table: Table, undirected: bool) -> Graph {
        let undirected = undirected && table == Table::Angle;
        let mut g = Graph {
            table,
            undirected,
            atom: Vec::new(),
            qs: Vec::new(),
            vars: Vec::new(),
            fq: Vec::new(),
            index: FxHashMap::default(),
            binary: Vec::new(),
            lambdas: Vec::new(),
            shown: RefCell::new(FxHashMap::default()),
            max_vars: 4,
        };
        for (ai, a) in atoms.iter().enumerate() {
            if a.table != table || a.cost == 0 {
                continue;
            }
            let q = match table {
                Table::Angle if undirected => cx.quot.q(&cx.t.exact(Table::Angle, &a.row)),
                Table::Angle => a.q.clone(),
                _ => cx.t.canon_ratio(&a.row),
            };
            let vs = key_vars(cx, table, &q);
            if vs.is_empty() || vs.len() > if table == Table::Ratio { 6 } else { 4 } {
                continue;
            }
            let k = g.atom.len() as u32;
            if vs.len() == 2 {
                g.binary.push(k);
            }
            for &v in &vs {
                g.index.entry(v).or_default().push(k);
            }
            g.atom.push(ai);
            g.fq.push(vs.iter().map(|v| (*v, q.get(*v).to_f64())).collect());
            g.qs.push(q);
            g.vars.push(vs);
        }
        g.lambdas = if table == Table::Angle { vec![Rat::one(), Rat::from_int(-1), Rat::from_int(2), Rat::from_int(-2)] } else { vec![Rat::one(), Rat::from_int(-1)] };
        if undirected {
            g.lambdas.extend([Rat::new(1, 2), Rat::new(-1, 2)]);
        }
        g
    }

    pub fn is_empty(&self) -> bool {
        self.atom.is_empty()
    }

    pub fn canon(&self, cx: &Ctx, c: &LinComb) -> LinComb {
        match self.table {
            Table::Angle if self.undirected => c.clone(),
            Table::Angle => {
                let k = c.get(ANGLE_UNIT);
                let mut out = c.clone();
                out.add_term(ANGLE_UNIT, &k.mod_one() - &k);
                out
            }
            Table::Ratio => cx.t.canon_ratio(c),
            _ => c.clone(),
        }
    }

    pub fn shape(&self, cx: &Ctx, c: &LinComb) -> Option<i64> {
        if let Some(v) = self.shown.borrow().get(c) {
            return *v;
        }
        let r = if self.undirected {
            undirected_shape(cx, c)
        } else if self.table == Table::Ratio && self.max_vars > 4 {
            wide_ratio_shape(cx, c, self.max_vars)
        } else {
            directed_shape(cx, self.table, c)
        };
        self.shown.borrow_mut().insert(c.clone(), r);
        r
    }

    pub fn search(&self, cx: &Ctx, splits: &[(LinComb, LinComb)], budget: usize, bound: i64, spent: &AtomicUsize, weights: &[i64]) -> Option<Path> {
        let mut best: Option<Path> = None;
        for (si, (l0, r0)) in splits.iter().enumerate() {
            if spent.load(Ordering::Relaxed) >= TOTAL_BUDGET || cx.timed_out() {
                break;
            }
            let l = self.canon(cx, l0);
            let r = self.canon(cx, r0);
            if l == r || self.shape(cx, &l).is_none() || self.shape(cx, &r).is_none() {
                continue;
            }
            let b = bound.min(best.as_ref().map(|b| b.weight).unwrap_or(i64::MAX));
            if let Some(p) = self.one(cx, si, &l, &r, budget, b, spent, weights) {
                if best.as_ref().is_none_or(|x| p.weight < x.weight) {
                    best = Some(p);
                }
            }
        }
        best
    }

    fn neighbours(&self, cx: &Ctx, x: &LinComb, vs: &[VarId], out: &mut Vec<(LinComb, usize, Rat)>, weights: &[i64]) {
        out.clear();
        let mut cand: Vec<u32> = Vec::new();
        if vs.is_empty() {
            cand.extend(self.binary.iter().copied());
        } else {
            for v in vs {
                if let Some(list) = self.index.get(v) {
                    cand.extend(list.iter().copied());
                }
            }
            cand.sort_unstable();
            cand.dedup();
        }
        let fx: Vec<(VarId, f64)> = vs.iter().map(|v| (*v, x.get(*v).to_f64())).collect();
        let xget = |v: VarId| fx.iter().find(|p| p.0 == v).map(|p| p.1).unwrap_or(0.0);
        let flam: Vec<f64> = self.lambdas.iter().map(|l| l.to_f64()).collect();
        for ek in cand {
            let ek = ek as usize;
            if weights[self.atom[ek]] == OFF {
                continue;
            }
            let fq = &self.fq[ek];
            let mut lams: Vec<usize> = Vec::new();
            if vs.is_empty() {
                lams.extend(0..self.lambdas.len());
            } else {
                for (v, a) in fq {
                    let xv = xget(*v);
                    if xv == 0.0 {
                        continue;
                    }
                    let lam = xv / a;
                    if let Some(li) = flam.iter().position(|l| (l - lam).abs() < 1e-9) {
                        if !lams.contains(&li) {
                            lams.push(li);
                        }
                    }
                }
            }
            for li in lams {
                let lam = flam[li];
                let mut cnt = vs.len();
                for (v, a) in fq {
                    let xv = xget(*v);
                    let nv = xv - lam * a;
                    if xv == 0.0 {
                        if nv.abs() > 1e-9 {
                            cnt += 1;
                        }
                    } else if nv.abs() < 1e-9 {
                        cnt -= 1;
                    }
                }
                if cnt > self.max_vars {
                    continue;
                }
                let lam = self.lambdas[li].clone();
                let y = self.canon(cx, &LinComb::combine(x, &self.qs[ek], &-&lam));
                out.push((y, ek, lam));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn one(&self, cx: &Ctx, si: usize, l: &LinComb, r: &LinComb, budget: usize, bound: i64, spent: &AtomicUsize, weights: &[i64]) -> Option<Path> {
        struct Side {
            ids: FxHashMap<LinComb, usize>,
            nodes: Vec<LinComb>,
            nvars: Vec<Vec<VarId>>,
            prev: Vec<Option<(usize, usize, Rat)>>,
            dist: Vec<i64>,
            depth: Vec<usize>,
            done: Vec<bool>,
            heap: BinaryHeap<Reverse<(i64, usize)>>,
        }
        let side = |x: &LinComb| {
            let mut s = Side { ids: FxHashMap::default(), nodes: vec![x.clone()], nvars: vec![key_vars(cx, self.table, x)], prev: vec![None], dist: vec![0], depth: vec![0], done: vec![false], heap: BinaryHeap::new() };
            s.ids.insert(x.clone(), 0);
            s.heap.push(Reverse((0, 0)));
            s
        };
        let mut sides = [side(l), side(r)];
        let ends = [r.clone(), l.clone()];
        let mut best: Option<(i64, usize, usize)> = None;
        let mut expanded = 0usize;
        let mut buf: Vec<(LinComb, usize, Rat)> = Vec::new();
        loop {
            let tops: Vec<Option<i64>> = sides.iter().map(|s| s.heap.peek().map(|Reverse((d, _))| *d)).collect();
            let (Some(t0), Some(t1)) = (tops[0], tops[1]) else { break };
            let limit = best.map(|b| b.0).unwrap_or(i64::MAX).min(bound);
            if t0 + t1 >= limit {
                break;
            }
            let (n0, n1) = (sides[0].heap.len(), sides[1].heap.len());
            let k = if n0 < n1 || (n0 == n1 && t0 <= t1) { 0 } else { 1 };
            let Reverse((d, u)) = sides[k].heap.pop().unwrap();
            if sides[k].done[u] || d > sides[k].dist[u] {
                continue;
            }
            sides[k].done[u] = true;
            let other = 1 - k;
            if let Some(&v) = sides[other].ids.get(&sides[k].nodes[u]) {
                if sides[other].dist[v] < i64::MAX && sides[k].depth[u] + sides[other].depth[v] <= MAX_LINKS {
                    let x = &sides[k].nodes[u];
                    let meet = if *x == *l || *x == *r { 0 } else { self.shape(cx, x).unwrap_or(0) };
                    let total = d + sides[other].dist[v] - meet;
                    if best.is_none_or(|b| total < b.0) {
                        best = Some(if k == 0 { (total, u, v) } else { (total, v, u) });
                    }
                }
            }
            if sides[k].depth[u] >= MAX_LINKS {
                continue;
            }
            expanded += 1;
            spent.fetch_add(1, Ordering::Relaxed);
            if expanded > budget || spent.load(Ordering::Relaxed) > TOTAL_BUDGET || cx.timed_out() {
                break;
            }
            let x = sides[k].nodes[u].clone();
            let vs = sides[k].nvars[u].clone();
            self.neighbours(cx, &x, &vs, &mut buf, weights);
            if std::env::var_os("HP_ROUTE2").is_some() {
                eprintln!("expand side {k} node {:?} vars {:?} neighbours {}", x, vs, buf.len());
                for (y, ek, lam) in buf.iter().take(12) {
                    eprintln!("   -> {:?} via atom {} lam {} shape {:?}", y, self.atom[*ek], lam, self.shape(cx, y));
                }
            }
            for (y, ek, lam) in buf.drain(..) {
                let s = if y == ends[k] || y == ends[other] {
                    0
                } else {
                    match self.shape(cx, &y) {
                        Some(s) => s,
                        None => continue,
                    }
                };
                let w = weights[self.atom[ek]] + s + if lam.abs().is_one() { 0 } else { 8 };
                let nd = d + w.max(1);
                let sd = &mut sides[k];
                let vid = match sd.ids.get(&y) {
                    Some(&id) => id,
                    None => {
                        let id = sd.nodes.len();
                        sd.ids.insert(y.clone(), id);
                        sd.nvars.push(key_vars(cx, self.table, &y));
                        sd.nodes.push(y);
                        sd.prev.push(None);
                        sd.dist.push(i64::MAX);
                        sd.depth.push(0);
                        sd.done.push(false);
                        id
                    }
                };
                if nd < sd.dist[vid] {
                    sd.dist[vid] = nd;
                    sd.prev[vid] = Some((u, self.atom[ek], lam));
                    sd.depth[vid] = sd.depth[u] + 1;
                    sd.heap.push(Reverse((nd, vid)));
                }
            }
        }
        if std::env::var_os("HP_ROUTE").is_some() {
            eprintln!("route split {si} ({}): expanded {expanded} nodes {}+{} found {:?}", if self.undirected { "as drawn" } else { "directed" }, sides[0].nodes.len(), sides[1].nodes.len(), best.map(|b| b.0));
        }
        let (weight, mf, mb) = best?;
        let mut fwd_nodes: Vec<LinComb> = vec![sides[0].nodes[mf].clone()];
        let mut fwd_links: Vec<Group> = Vec::new();
        let mut cur = mf;
        while let Some((p, i, lam)) = sides[0].prev[cur].clone() {
            fwd_links.push(vec![(i, lam)]);
            fwd_nodes.push(sides[0].nodes[p].clone());
            cur = p;
        }
        fwd_nodes.reverse();
        fwd_links.reverse();
        let mut cur = mb;
        while let Some((p, i, lam)) = sides[1].prev[cur].clone() {
            fwd_links.push(vec![(i, -&lam)]);
            fwd_nodes.push(sides[1].nodes[p].clone());
            cur = p;
        }
        if fwd_links.is_empty() || fwd_links.len() > MAX_LINKS {
            return None;
        }
        Some(Path { split: si, nodes: fwd_nodes, links: fwd_links, weight, undirected: self.undirected })
    }
}

pub fn support(path: &Path) -> Vec<(usize, Rat)> {
    let mut out: Vec<(usize, Rat)> = Vec::new();
    for g in &path.links {
        for (i, lam) in g {
            match out.iter_mut().find(|x| x.0 == *i) {
                Some(x) => x.1 = &x.1 + lam,
                None => out.push((*i, lam.clone())),
            }
        }
    }
    out.retain(|x| !x.1.is_zero());
    out
}
