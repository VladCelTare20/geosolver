use super::atoms::{Atom, AtomSrc};
use super::ctx::{Ctx, QBASE};
use super::expr::interior;
use super::model::Expr;
use super::trace::Table;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::{LinComb, VarId};
use crate::predicate::PointId;
use crate::rational::Rat;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Clone, Debug)]
pub struct Path {
    pub split: usize,
    pub nodes: Vec<LinComb>,
    pub links: Vec<(usize, Rat)>,
    pub weight: i64,
}

pub fn classes(c: &LinComb) -> Vec<(VarId, Rat)> {
    c.terms.iter().filter(|(v, _)| *v >= QBASE).map(|(v, k)| (*v, k.clone())).collect()
}

pub fn single_angle(c: &LinComb) -> Option<Rat> {
    if c.terms.iter().any(|(v, _)| *v != ANGLE_UNIT && *v < QBASE) {
        return None;
    }
    let cl = classes(c);
    match cl.len() {
        0 => Some(Rat::zero()),
        2 => {
            let (a, b) = (&cl[0].1, &cl[1].1);
            if *a != -b {
                return None;
            }
            let k = a.abs();
            (k == Rat::one() || k == Rat::from_int(2) || k == Rat::new(1, 2)).then_some(k)
        }
        _ => None,
    }
}

pub fn single_ratio(cx: &Ctx, c: &LinComb) -> bool {
    let lhs: Vec<&(VarId, Rat)> = c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).collect();
    lhs.len() <= 2 && lhs.iter().all(|(_, k)| k.abs() == Rat::one())
}

const LAMBDAS: [(i64, i64); 6] = [(1, 1), (-1, 1), (2, 1), (-2, 1), (1, 2), (-1, 2)];

pub fn bfs(
    cx: &Ctx,
    atoms: &[Atom],
    eligible: &[usize],
    table: Table,
    splits: &[(LinComb, LinComb)],
    penalty: &dyn Fn(&Atom) -> i64,
    budget: usize,
) -> Option<Path> {
    let single = |c: &LinComb| -> bool {
        if table == Table::Angle {
            single_angle(c).is_some()
        } else {
            single_ratio(cx, c)
        }
    };
    let key_vars = |c: &LinComb| -> Vec<VarId> {
        if table == Table::Angle {
            classes(c).into_iter().map(|x| x.0).collect()
        } else {
            c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).map(|(v, _)| *v).collect()
        }
    };
    let mut index: BTreeMap<VarId, Vec<usize>> = BTreeMap::new();
    let mut binary: Vec<usize> = Vec::new();
    for &i in eligible {
        let a = &atoms[i];
        if a.table != table {
            continue;
        }
        let vs = key_vars(&a.q);
        if vs.is_empty() || vs.len() > 4 {
            continue;
        }
        if vs.len() == 2 {
            binary.push(i);
        }
        for v in vs {
            index.entry(v).or_default().push(i);
        }
    }
    let lambdas: Vec<Rat> = LAMBDAS
        .iter()
        .filter(|(n, d)| table == Table::Angle || (*d == 1 && n.abs() == 1))
        .map(|(n, d)| Rat::new(*n, *d))
        .collect();
    let mut best: Option<Path> = None;
    for (si, (l, r)) in splits.iter().enumerate() {
        if !single(l) || !single(r) {
            continue;
        }
        let mut ids: FxHashMap<LinComb, usize> = FxHashMap::default();
        let mut nodes: Vec<LinComb> = vec![l.clone()];
        let mut prev: Vec<Option<(usize, usize, Rat)>> = vec![None];
        let mut dist: Vec<i64> = vec![0];
        ids.insert(l.clone(), 0);
        let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
        heap.push(Reverse((0, 0)));
        let mut done = vec![false];
        let mut expanded = 0usize;
        let mut found: Option<usize> = None;
        while let Some(Reverse((d, u))) = heap.pop() {
            if done[u] || d > dist[u] {
                continue;
            }
            if best.as_ref().is_some_and(|b| d >= b.weight) {
                break;
            }
            done[u] = true;
            if nodes[u] == *r {
                found = Some(u);
                break;
            }
            expanded += 1;
            if expanded > budget || cx.timed_out() {
                break;
            }
            let x = nodes[u].clone();
            let vs = key_vars(&x);
            let mut cand: BTreeSet<usize> = BTreeSet::new();
            if vs.is_empty() {
                cand.extend(binary.iter().copied());
            } else {
                for v in &vs {
                    if let Some(list) = index.get(v) {
                        cand.extend(list.iter().copied());
                    }
                }
            }
            for i in cand {
                let a = &atoms[i];
                for lam in &lambdas {
                    let y = LinComb::combine(&x, &a.q, &-lam);
                    if !single(&y) {
                        continue;
                    }
                    let mut w = 100 + 10 * a.cost as i64 + penalty(a);
                    if lam.abs() != Rat::one() {
                        w += 20;
                    }
                    if !lam.is_integer() {
                        w += 400;
                    }
                    if table == Table::Angle && classes(&y).is_empty() {
                        w -= 3;
                    }
                    let nd = d + w.max(1);
                    let vid = match ids.get(&y) {
                        Some(&k) => k,
                        None => {
                            let k = nodes.len();
                            ids.insert(y.clone(), k);
                            nodes.push(y);
                            prev.push(None);
                            dist.push(i64::MAX);
                            done.push(false);
                            k
                        }
                    };
                    if nd < dist[vid] {
                        dist[vid] = nd;
                        prev[vid] = Some((u, i, lam.clone()));
                        heap.push(Reverse((nd, vid)));
                    }
                }
            }
        }
        if let Some(end) = found {
            let mut seq: Vec<usize> = vec![end];
            let mut links: Vec<(usize, Rat)> = Vec::new();
            let mut cur = end;
            while let Some((p, i, lam)) = prev[cur].clone() {
                links.push((i, lam));
                seq.push(p);
                cur = p;
            }
            seq.reverse();
            links.reverse();
            let path = Path { split: si, nodes: seq.iter().map(|&k| nodes[k].clone()).collect(), links, weight: dist[end] };
            if best.as_ref().is_none_or(|b| path.weight < b.weight) {
                best = Some(path);
            }
        }
    }
    best
}

pub fn dfs_order(cx: &Ctx, table: Table, l: &LinComb, r: &LinComb, items: &[(usize, Rat, LinComb)], budget: &mut usize) -> Option<Vec<usize>> {
    let single = |c: &LinComb| if table == Table::Angle { single_angle(c).is_some() } else { single_ratio(cx, c) };
    if items.len() > 14 || !single(l) {
        return None;
    }
    fn go(
        cur: &LinComb,
        items: &[(usize, Rat, LinComb)],
        used: &mut Vec<bool>,
        order: &mut Vec<usize>,
        target: &LinComb,
        single: &dyn Fn(&LinComb) -> bool,
        budget: &mut usize,
    ) -> bool {
        if order.len() == items.len() {
            return cur == target;
        }
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        for k in 0..items.len() {
            if used[k] {
                continue;
            }
            let nx = LinComb::combine(cur, &items[k].2, &-&items[k].1);
            if !single(&nx) {
                continue;
            }
            used[k] = true;
            order.push(k);
            if go(&nx, items, used, order, target, single, budget) {
                return true;
            }
            order.pop();
            used[k] = false;
        }
        false
    }
    let mut used = vec![false; items.len()];
    let mut order = Vec::new();
    go(l, items, &mut used, &mut order, r, &single, budget).then_some(order)
}

pub struct Hints {
    pub focus: BTreeSet<PointId>,
}

fn pick(cands: &[PointId], score: &dyn Fn(PointId) -> i64) -> Option<PointId> {
    cands.iter().copied().max_by_key(|&p| (score(p), Reverse(p)))
}

pub fn angle_expr(cx: &Ctx, node: &LinComb, directed: bool, near: &[&[PointId]], focus: &BTreeSet<PointId>) -> Option<Expr> {
    let t = cx.t;
    let cl = classes(node);
    let pi = node.get(ANGLE_UNIT);
    let score = |p: PointId| -> i64 {
        let mut s = 0;
        if focus.contains(&p) {
            s += 3;
        }
        for (i, set) in near.iter().enumerate() {
            if set.contains(&p) {
                s += if i == 0 { 4 } else { 2 };
            }
        }
        s
    };
    if cl.is_empty() {
        let deg = if directed { &pi.mod_one() * &Rat::from_int(180) } else { &pi * &Rat::from_int(180) };
        return Some(Expr::Const { degrees: deg });
    }
    let (pos, neg) = if cl[0].1.is_negative() { (cl[1].0, cl[0].0) } else { (cl[0].0, cl[1].0) };
    let k = cl[0].1.abs();
    let m1 = &cx.quot.members[(pos - QBASE) as usize];
    let m2 = &cx.quot.members[(neg - QBASE) as usize];
    let common: Vec<PointId> = m1.iter().copied().filter(|p| m2.contains(p)).collect();
    let mut best: Option<(i64, Expr, Rat)> = None;
    let consider = |e: Expr, form: LinComb, best: &mut Option<(i64, Expr, Rat)>, sc: i64| {
        let q = cx.quot.q(&form);
        let rest = LinComb::combine(node, &q, &-&k);
        if rest.terms.iter().any(|(v, _)| *v != ANGLE_UNIT) {
            return;
        }
        let c = rest.get(ANGLE_UNIT);
        let c = if directed { c.mod_one() } else { c };
        let sc = sc - if c.is_zero() { 0 } else { 5 } - if !directed && c.is_negative() { 20 } else { 0 };
        if best.as_ref().is_none_or(|b| sc > b.0) {
            *best = Some((sc, e, c));
        }
    };
    if let Some(y) = pick(&common, &score) {
        let xs: Vec<PointId> = m2.iter().copied().filter(|&p| p != y && t.dir(y, p).is_some()).collect();
        let zs: Vec<PointId> = m1.iter().copied().filter(|&p| p != y && t.dir(y, p).is_some()).collect();
        for &x in &xs {
            for &z in &zs {
                let sc = score(x) + score(z) + score(y);
                if directed {
                    if let (Some(a), Some(b)) = (t.dir(y, z), t.dir(y, x)) {
                        consider(Expr::Angle { a: x, b: y, c: z, directed: true }, &a - &b, &mut best, sc);
                    }
                } else if let Some(form) = interior(t, x, y, z) {
                    consider(Expr::Angle { a: x, b: y, c: z, directed: false }, form, &mut best, sc);
                }
            }
        }
    }
    if best.is_none() && directed {
        let l1 = line_pair(cx, m2, &score)?;
        let l2 = line_pair(cx, m1, &score)?;
        let form = &t.dir(l2.0, l2.1)? - &t.dir(l1.0, l1.1)?;
        let sc = 0;
        consider(Expr::LineAngle { l1, l2, directed: true }, form, &mut best, sc);
    }
    let (_, e, c) = best?;
    let mut terms: Vec<(Rat, Expr)> = Vec::new();
    if k == Rat::one() && c.is_zero() {
        return Some(e);
    }
    terms.push((k, e));
    if !c.is_zero() {
        terms.push((Rat::one(), Expr::Const { degrees: &c * &Rat::from_int(180) }));
    }
    Some(Expr::Lin { terms })
}

fn line_pair(cx: &Ctx, m: &[PointId], score: &dyn Fn(PointId) -> i64) -> Option<(PointId, PointId)> {
    let mut best: Option<(i64, (PointId, PointId))> = None;
    for (i, &a) in m.iter().enumerate() {
        for &b in &m[i + 1..] {
            if cx.t.dir(a, b).is_none() {
                continue;
            }
            let s = score(a) + score(b);
            if best.is_none_or(|x| s > x.0) {
                best = Some((s, (a, b)));
            }
        }
    }
    best.map(|x| x.1)
}

pub fn atom_points(cx: &Ctx, a: &Atom) -> Vec<PointId> {
    match a.src {
        AtomSrc::Human(_) => a.args.clone(),
        AtomSrc::Hyp(f) => cx.hyp_pred.get(&f).map(|p| p.points.clone()).unwrap_or_default(),
        AtomSrc::Glue(f) | AtomSrc::Line(f) | AtomSrc::Engine(f) => super::classify::fact_points(cx, f),
    }
}

pub fn ratio_expr(cx: &Ctx, c: &LinComb) -> Expr {
    let t = cx.t;
    let c = &t.canon_ratio(c);
    let mut num: Vec<(Expr, i32)> = Vec::new();
    let mut den: Vec<(Expr, i32)> = Vec::new();
    let mut konst = Rat::one();
    let mut seg_of: FxHashMap<VarId, (PointId, PointId)> = FxHashMap::default();
    for a in 0..t.n as PointId {
        for b in (a + 1)..t.n as PointId {
            if let Some(v) = t.var(Table::Ratio, a, b) {
                seg_of.entry(v).or_insert((a, b));
            }
        }
    }
    for (v, k) in c.terms.iter() {
        let e = k.numer_i64().unwrap_or(1) as i32;
        let base = if let Some(&(a, b)) = seg_of.get(v) {
            Some(Expr::Seg { a, b })
        } else if let Some(s) = t.sines.iter().find(|s| s.var == *v) {
            let ang = Expr::Angle { a: s.q, b: s.v, c: s.p, directed: false };
            if s.shift.mod_one() == Rat::new(1, 2) {
                Some(Expr::Cos { angle: Box::new(ang) })
            } else if s.shift.is_zero() {
                Some(Expr::Sin { angle: Box::new(ang) })
            } else {
                let d = &s.shift.mod_one() * &Rat::from_int(180);
                let sign = if s.flip { Rat::from_int(-1) } else { Rat::one() };
                Some(Expr::Sin { angle: Box::new(Expr::Lin { terms: vec![(sign, ang), (Rat::one(), Expr::Const { degrees: d })] }) })
            }
        } else if let Some(&(p, _)) = t.primes.iter().find(|x| x.1 == *v) {
            if k.is_integer() {
                let n = k.numer_i64().unwrap_or(0);
                for _ in 0..n.abs() {
                    if n > 0 {
                        konst = &konst * &Rat::from_int(p as i64);
                    } else {
                        konst = &konst / &Rat::from_int(p as i64);
                    }
                }
                None
            } else {
                Some(Expr::Num { value: Rat::from_int(p as i64) })
            }
        } else {
            None
        };
        if let Some(b) = base {
            if e > 0 {
                num.push((b, e));
            } else {
                den.push((b, -e));
            }
        }
    }
    let mut factors: Vec<(Expr, i32)> = Vec::new();
    if konst.numer_i64().is_some_and(|n| n != 1) {
        factors.push((Expr::Num { value: Rat::from_int(konst.numer_i64().unwrap()) }, 1));
    }
    factors.extend(num);
    if konst.denom_i64().is_some_and(|n| n != 1) {
        factors.push((Expr::Num { value: Rat::from_int(konst.denom_i64().unwrap()) }, -1));
    }
    for (b, e) in den {
        factors.push((b, -e));
    }
    Expr::Prod { factors }
}
