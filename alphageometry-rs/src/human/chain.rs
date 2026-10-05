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

pub type Group = Vec<(usize, Rat)>;

#[derive(Clone, Debug)]
pub struct Path {
    pub split: usize,
    pub nodes: Vec<LinComb>,
    pub links: Vec<Group>,
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
        if std::env::var_os("HP_BFS").is_some() {
            eprintln!("bfs split {si} expanded {expanded} nodes {} found {} eligible {} L {:?} R {:?}", nodes.len(), found.is_some(), eligible.len(), l, r);
        }
        if let Some(end) = found {
            let mut seq: Vec<usize> = vec![end];
            let mut links: Vec<Group> = Vec::new();
            let mut cur = end;
            while let Some((p, i, lam)) = prev[cur].clone() {
                links.push(vec![(i, lam)]);
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

pub type Term2 = (Rat, VarId, VarId);

fn small_coef(k: &Rat) -> bool {
    let a = k.abs();
    a.is_one() || a == Rat::from_int(2) || a == Rat::new(1, 2)
}

fn term_of(c: &(VarId, Rat), other: VarId) -> Term2 {
    if c.1.is_negative() {
        (-&c.1, other, c.0)
    } else {
        (c.1.clone(), c.0, other)
    }
}

pub fn decomps(c: &LinComb) -> Vec<Vec<Term2>> {
    if c.terms.iter().any(|(v, _)| *v != ANGLE_UNIT && *v < QBASE) {
        return Vec::new();
    }
    let cl = classes(c);
    let sum: Rat = cl.iter().fold(Rat::zero(), |s, x| &s + &x.1);
    if !sum.is_zero() {
        return Vec::new();
    }
    let mut out = Vec::new();
    match cl.len() {
        0 => out.push(Vec::new()),
        2 => {
            let k = cl[0].1.abs();
            if k.is_one() || k == Rat::from_int(2) || k == Rat::new(1, 2) {
                out.push(vec![term_of(&cl[0], cl[1].0)]);
            }
        }
        3 => {
            for s in 0..3 {
                let o: Vec<usize> = (0..3).filter(|&x| x != s).collect();
                let (i, j) = (o[0], o[1]);
                if small_coef(&cl[i].1) && small_coef(&cl[j].1) {
                    out.push(vec![term_of(&cl[i], cl[s].0), term_of(&cl[j], cl[s].0)]);
                }
            }
        }
        4 => {
            for (a, b, c2, d) in [(0, 1, 2, 3), (0, 2, 1, 3), (0, 3, 1, 2)] {
                if (&cl[a].1 + &cl[b].1).is_zero() && (&cl[c2].1 + &cl[d].1).is_zero() && small_coef(&cl[a].1) && small_coef(&cl[c2].1) {
                    out.push(vec![term_of(&cl[a], cl[b].0), term_of(&cl[c2], cl[d].0)]);
                }
            }
        }
        _ => {}
    }
    out
}

pub fn decomps_upto(c: &LinComb, max_terms: usize) -> Vec<Vec<Term2>> {
    fn go(m: &[(VarId, Rat)], left: usize, acc: &mut Vec<Term2>, out: &mut Vec<Vec<Term2>>) {
        if m.is_empty() {
            let mut v = acc.clone();
            v.sort();
            if !out.contains(&v) {
                out.push(v);
            }
            return;
        }
        if left == 0 || out.len() >= 64 {
            return;
        }
        let first = &m[0];
        if !small_coef(&first.1) {
            return;
        }
        for j in 1..m.len() {
            let mut rest: Vec<(VarId, Rat)> = m[1..].to_vec();
            let k = j - 1;
            rest[k].1 = &rest[k].1 + &first.1;
            rest.retain(|x| !x.1.is_zero());
            acc.push(term_of(first, m[j].0));
            go(&rest, left - 1, acc, out);
            acc.pop();
        }
    }
    if c.terms.iter().any(|(v, _)| *v != ANGLE_UNIT && *v < QBASE) {
        return Vec::new();
    }
    let cl = classes(c);
    let sum: Rat = cl.iter().fold(Rat::zero(), |s, x| &s + &x.1);
    if !sum.is_zero() {
        return Vec::new();
    }
    let mut out = Vec::new();
    go(&cl, max_terms, &mut Vec::new(), &mut out);
    out
}

pub fn term_comb(t: &Term2) -> LinComb {
    let mut c = LinComb::singleton(t.1, t.0.clone());
    c.add_term(t.2, -&t.0);
    c
}

pub fn angle_shape(c: &LinComb) -> Option<i64> {
    let d = decomps(c);
    let best = d.iter().map(|ts| ts.len()).min()?;
    Some(match best {
        0 => 0,
        1 => {
            let k = d.iter().find(|ts| ts.len() == 1).map(|ts| ts[0].0.clone()).unwrap_or_else(Rat::one);
            if k.is_one() {
                0
            } else {
                3
            }
        }
        _ => 9,
    })
}

pub fn named_pair(cx: &Ctx, a: VarId, b: VarId) -> bool {
    let (Some(m1), Some(m2)) = (cx.quot.members.get((a - QBASE) as usize), cx.quot.members.get((b - QBASE) as usize)) else { return false };
    m1.iter().any(|p| m2.contains(p))
}

pub fn angle_cost(cx: &Ctx, c: &LinComb) -> Option<i64> {
    decomps(c)
        .iter()
        .map(|ts| {
            let mut s: i64 = if ts.len() == 2 { 9 } else { 0 };
            for t in ts {
                if !named_pair(cx, t.1, t.2) {
                    s += 6;
                }
                if !t.0.is_one() {
                    s += 3;
                }
            }
            s
        })
        .min()
}

pub fn ratio_shape(cx: &Ctx, c: &LinComb) -> Option<i64> {
    let lhs: Vec<&(VarId, Rat)> = c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).collect();
    if !lhs.iter().all(|(_, k)| k.abs() == Rat::one()) {
        return None;
    }
    match lhs.len() {
        0..=2 => Some(0),
        3 | 4 => Some(9),
        _ => None,
    }
}

pub fn ratio_splits_loose(cx: &Ctx, s: &LinComb) -> Vec<(LinComb, LinComb)> {
    let piv = |v: VarId| cx.piv[1].get(v as usize).copied().unwrap_or(true);
    let p: Vec<(VarId, Rat)> = s.terms.iter().filter(|(v, _)| piv(*v)).cloned().collect();
    let two = Rat::from_int(2);
    if p.len() < 2 || p.len() > 6 || p.iter().any(|(_, k)| !k.abs().is_one() && k.abs() != two) {
        return Vec::new();
    }
    let pos: Vec<&(VarId, Rat)> = p.iter().filter(|x| !x.1.is_negative()).collect();
    let neg = p.len() - pos.len();
    if pos.is_empty() || neg == 0 || pos.len() > 3 || neg > 3 {
        return Vec::new();
    }
    let mut a = LinComb::zero();
    for x in pos {
        a.add_term(x.0, x.1.clone());
    }
    let end = LinComb::combine(&a, s, &Rat::from_int(-1));
    vec![(a, end)]
}

pub fn ratio_splits(cx: &Ctx, s: &LinComb) -> Vec<(LinComb, LinComb)> {
    let piv = |v: VarId| cx.piv[1].get(v as usize).copied().unwrap_or(true);
    let p: Vec<(VarId, Rat)> = s.terms.iter().filter(|(v, _)| piv(*v)).cloned().collect();
    if p.is_empty() || p.len() > 4 || p.iter().any(|(_, k)| !k.abs().is_one()) {
        return Vec::new();
    }
    let part = |idx: &[usize]| -> LinComb {
        let mut c = LinComb::zero();
        for &i in idx {
            c.add_term(p[i].0, p[i].1.clone());
        }
        c
    };
    let mut starts: Vec<LinComb> = Vec::new();
    match p.len() {
        1 => {}
        2 => {
            let i = if p[0].1.is_negative() { 1 } else { 0 };
            starts.push(part(&[i]));
        }
        3 => {
            for i in 0..3 {
                let o: Vec<usize> = (0..3).filter(|&j| j != i).collect();
                starts.push(part(&o));
            }
        }
        _ => {
            for (a, b) in [(0, 1), (0, 2), (0, 3)] {
                starts.push(part(&[a, b]));
            }
        }
    }
    starts.into_iter().map(|a| {
        let end = LinComb::combine(&a, s, &Rat::from_int(-1));
        (a, end)
    }).collect()
}

pub fn order_items(cx: &Ctx, table: Table, l: &LinComb, r: &LinComb, items: &[(usize, Rat, LinComb)], budget: &mut usize, cap: usize) -> Option<(Vec<Vec<usize>>, i64)> {
    order_items_mode(cx, table, l, r, items, budget, cap, false)
}

pub fn order_items_mode(cx: &Ctx, table: Table, l: &LinComb, r: &LinComb, items: &[(usize, Rat, LinComb)], budget: &mut usize, cap: usize, halves: bool) -> Option<(Vec<Vec<usize>>, i64)> {
    let n = items.len();
    if n == 0 || n > 24 {
        return None;
    }
    let integral_of = |c: &LinComb| c.terms.iter().all(|(v, k)| *v < QBASE || k.is_integer());
    let integral = !halves && table == Table::Angle && integral_of(l) && integral_of(r);
    let shape = |c: &LinComb| -> Option<i64> {
        if table == Table::Angle {
            if integral && !integral_of(c) {
                return None;
            }
            if halves {
                let named = decomps(c).iter().any(|ts| ts.iter().all(|t| named_pair(cx, t.1, t.2)));
                if !named {
                    return None;
                }
                return angle_cost(cx, c).map(|k| k + if integral_of(c) { 0 } else { 6 });
            }
            angle_cost(cx, c)
        } else {
            ratio_shape(cx, c)
        }
    };
    if shape(l).is_none() {
        return None;
    }
    let half: Vec<usize> = (0..n).filter(|&k| !items[k].1.is_integer()).collect();
    let full: u32 = if n == 32 { u32::MAX } else { (1u32 << n) - 1 };
    struct St {
        mask: u32,
        node: LinComb,
        g: i64,
        prev: Option<(usize, Vec<usize>)>,
    }
    let mut states: Vec<St> = vec![St { mask: 0, node: l.clone(), g: 0, prev: None }];
    let mut index: FxHashMap<u32, usize> = FxHashMap::default();
    index.insert(0, 0);
    let mut done: Vec<bool> = vec![false];
    let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
    heap.push(Reverse((10 * n as i64, 0)));
    let push = |states: &mut Vec<St>, done: &mut Vec<bool>, index: &mut FxHashMap<u32, usize>, heap: &mut BinaryHeap<Reverse<(i64, usize)>>, mask: u32, node: LinComb, g: i64, prev: (usize, Vec<usize>)| {
        let rem = (full & !mask).count_ones() as i64;
        match index.get(&mask) {
            Some(&k) => {
                if g < states[k].g && !done[k] {
                    states[k].g = g;
                    states[k].node = node;
                    states[k].prev = Some(prev);
                    heap.push(Reverse((g + 10 * rem, k)));
                }
            }
            None => {
                let k = states.len();
                states.push(St { mask, node, g, prev: Some(prev) });
                done.push(false);
                index.insert(mask, k);
                heap.push(Reverse((g + 10 * rem, k)));
            }
        }
    };
    let mut found: Option<usize> = None;
    let mut spent = 0usize;
    while let Some(Reverse((_, u))) = heap.pop() {
        if done[u] {
            continue;
        }
        done[u] = true;
        if states[u].mask == full {
            found = Some(u);
            break;
        }
        if *budget == 0 || spent >= cap || cx.timed_out() {
            return None;
        }
        *budget -= 1;
        spent += 1;
        let (mask, node, g) = (states[u].mask, states[u].node.clone(), states[u].g);
        for k in 0..n {
            if mask & (1 << k) != 0 {
                continue;
            }
            let nx = LinComb::combine(&node, &items[k].2, &-&items[k].1);
            let m2 = mask | (1 << k);
            if m2 == full {
                if nx == *r {
                    push(&mut states, &mut done, &mut index, &mut heap, m2, nx, g + 10, (u, vec![k]));
                }
                continue;
            }
            if let Some(c) = shape(&nx) {
                push(&mut states, &mut done, &mut index, &mut heap, m2, nx, g + 10 + c, (u, vec![k]));
                continue;
            }
            for k2 in 0..n {
                if m2 & (1 << k2) != 0 {
                    continue;
                }
                let nx2 = LinComb::combine(&nx, &items[k2].2, &-&items[k2].1);
                let m3 = m2 | (1 << k2);
                let c = if m3 == full {
                    (nx2 == *r).then_some(0)
                } else {
                    shape(&nx2)
                };
                if let Some(c) = c {
                    push(&mut states, &mut done, &mut index, &mut heap, m3, nx2, g + 30 + c, (u, vec![k, k2]));
                    continue;
                }
                if !half.contains(&k) || !half.contains(&k2) || k2 < k {
                    continue;
                }
                for &k3 in &half {
                    if k3 <= k2 || m3 & (1 << k3) != 0 {
                        continue;
                    }
                    let nx3 = LinComb::combine(&nx2, &items[k3].2, &-&items[k3].1);
                    let m4 = m3 | (1 << k3);
                    let c = if m4 == full { (nx3 == *r).then_some(0) } else { shape(&nx3) };
                    if let Some(c) = c {
                        push(&mut states, &mut done, &mut index, &mut heap, m4, nx3, g + 45 + c, (u, vec![k, k2, k3]));
                    }
                }
            }
        }
    }
    let end = found?;
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut cur = end;
    while let Some((p, grp)) = states[cur].prev.clone() {
        groups.push(grp);
        cur = p;
    }
    groups.reverse();
    Some((groups, states[end].g))
}

pub fn display_node(cx: &Ctx, node: &LinComb, directed: bool, near: &[&[PointId]], focus: &BTreeSet<PointId>) -> Option<Expr> {
    if single_angle(node).is_some() {
        return angle_expr(cx, node, directed, near, focus);
    }
    let mut best: Option<(i64, Expr)> = None;
    for ts in decomps(node) {
        if ts.len() != 2 {
            continue;
        }
        let mut terms: Vec<(Rat, Expr)> = Vec::new();
        let mut ok = true;
        for t in &ts {
            match angle_expr(cx, &term_comb(t), directed, near, focus) {
                Some(Expr::Lin { terms: tt }) => terms.extend(tt.into_iter().filter(|(_, x)| !matches!(x, Expr::Const { .. }))),
                Some(e) => terms.push((Rat::one(), e)),
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let Some(e) = with_const(cx, terms, node, directed) else { continue };
        let bad = expr_badness(&e);
        if best.as_ref().is_none_or(|b| bad < b.0) {
            best = Some((bad, e));
        }
    }
    best.map(|b| b.1)
}

fn with_const(cx: &Ctx, mut terms: Vec<(Rat, Expr)>, node: &LinComb, directed: bool) -> Option<Expr> {
    let (_, raw) = super::expr::eval(cx.t, &Expr::Lin { terms: terms.clone() })?;
    let rest = LinComb::combine(node, &cx.quot.q(&raw), &Rat::from_int(-1));
    if rest.terms.iter().any(|(v, _)| *v != ANGLE_UNIT) {
        return None;
    }
    let c = if directed { rest.get(ANGLE_UNIT).mod_one() } else { rest.get(ANGLE_UNIT) };
    terms.sort_by_key(|(k, _)| k.is_negative());
    if !c.is_zero() {
        terms.push((Rat::one(), Expr::Const { degrees: &c * &Rat::from_int(180) }));
    }
    Some(Expr::Lin { terms })
}

pub fn expr_badness(e: &Expr) -> i64 {
    match e {
        Expr::LineAngle { .. } => 4,
        Expr::Angle { .. } => 0,
        Expr::Lin { terms } => terms.iter().map(|(k, x)| expr_badness(x) + if k.is_negative() { 1 } else { 0 }).sum(),
        _ => 0,
    }
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
        if t.name(p).starts_with('_') {
            s -= 40;
        }
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
    let mut best: Option<(i64, Expr, Rat, Rat)> = None;
    let consider = |e: Expr, form: LinComb, best: &mut Option<(i64, Expr, Rat, Rat)>, sc: i64| {
        let q = cx.quot.q(&form);
        for kk in [k.clone(), -&k] {
            if directed && kk.is_negative() {
                continue;
            }
            let rest = LinComb::combine(node, &q, &-&kk);
            if rest.terms.iter().any(|(v, _)| *v != ANGLE_UNIT) {
                continue;
            }
            let c = rest.get(ANGLE_UNIT);
            let c = if directed { c.mod_one() } else { c };
            let sc = sc - if c.is_zero() { 0 } else { 5 } - if !directed && c.is_negative() { 20 } else { 0 } - if kk.is_negative() { 10 } else { 0 };
            if best.as_ref().is_none_or(|b| sc > b.0) {
                *best = Some((sc, e.clone(), c, kk));
            }
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
                } else {
                    if let Some(form) = interior(t, x, y, z) {
                        consider(Expr::Angle { a: x, b: y, c: z, directed: false }, form, &mut best, sc);
                    }
                    if let Some(form) = interior(t, z, y, x) {
                        consider(Expr::Angle { a: z, b: y, c: x, directed: false }, form, &mut best, sc);
                    }
                }
            }
        }
    }
    if best.is_none() && !directed && std::env::var_os("HP_BFS").is_some() {
        eprintln!("angle_expr undirected failed: common {:?} m1 {:?} m2 {:?} k {} node {:?}", common, m1, m2, k, node);
        if let Some(&y) = common.first() {
            for &x in m2.iter() {
                for &z in m1.iter() {
                    if x != y && z != y {
                        eprintln!("  interior({x},{y},{z}) = {:?}  q = {:?}", interior(t, x, y, z), interior(t, x, y, z).map(|f| cx.quot.q(&f)));
                    }
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
    let (_, e, c, k) = best?;
    let mut terms: Vec<(Rat, Expr)> = Vec::new();
    if k == Rat::one() && c.is_zero() {
        return Some(e);
    }
    if k.is_negative() && !c.is_zero() {
        terms.push((Rat::one(), Expr::Const { degrees: &c * &Rat::from_int(180) }));
        terms.push((k, e));
        return Some(Expr::Lin { terms });
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
