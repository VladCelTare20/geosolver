use super::claims::{eff, Node, Part, Writer, GOAL};
use super::classify::Kind;
use crate::predicate::PointId;
use crate::proof::FactId;
use std::collections::{BTreeMap, BTreeSet};

pub const EXTRA_CAP: i64 = 2_000;

#[derive(Clone, Debug, Default)]
pub struct Search {
    pub total: BTreeMap<FactId, i64>,
    pub examined: usize,
    pub improved: usize,
    pub complete: bool,
}

pub fn present_cost(n: &Node) -> i64 {
    if n.fact == GOAL {
        return 0;
    }
    if n.parts.is_none() {
        return if n.theorem { 10 } else { 80 };
    }
    match n.kind {
        Kind::Sim => 60,
        Kind::Congruent => 40,
        Kind::Isosceles => 6,
        Kind::Cyclic | Kind::Coll => 14,
        Kind::Cong | Kind::Length | Kind::Angle => 8,
        Kind::Theorem(_) | Kind::Merge | Kind::Other => 10,
        Kind::Formula(_) => 6,
    }
}

pub fn edge_cost(w: &Writer, parts: &[Part], focus: &BTreeSet<PointId>) -> i64 {
    let cx = w.cx;
    let mut c = 0i64;
    let mut atoms = 0i64;
    let mut aux: BTreeSet<PointId> = BTreeSet::new();
    for p in parts {
        for (i, lam) in &p.support {
            let a = &w.atoms[*i];
            if a.cost == 0 {
                continue;
            }
            c += eff(a, focus, &w.claim_cost, cx) as i64 + if lam.abs().is_one() { 0 } else { 2 };
            atoms += 1;
            for &x in &a.args {
                if cx.is_aux(x) && !focus.contains(&x) {
                    aux.insert(x);
                }
            }
        }
    }
    c + 2 * atoms + 6 * (atoms - 6).max(0) + 3 * aux.len() as i64
}

fn deps_total(w: &Writer, total: &BTreeMap<FactId, i64>, deps: &BTreeSet<FactId>, me: FactId) -> i64 {
    deps.iter().filter(|&&d| d != me && w.nodes.contains_key(&d)).map(|d| total.get(d).copied().unwrap_or(0)).sum()
}

pub fn set_extra(w: &mut Writer, total: &BTreeMap<FactId, i64>, me: FactId, skip: &BTreeSet<FactId>) {
    let cx = w.cx;
    let mut extra = vec![0u32; w.atoms.len()];
    for (i, a) in w.atoms.iter().enumerate() {
        let mut ds: BTreeSet<FactId> = BTreeSet::new();
        for &s in &a.sources {
            let d = cx.displayable(s);
            if d != me && !skip.contains(&d) && w.nodes.contains_key(&d) {
                ds.insert(d);
            }
        }
        let sum: i64 = ds.iter().map(|d| total.get(d).copied().unwrap_or(0)).sum();
        extra[i] = sum.min(EXTRA_CAP) as u32;
    }
    w.extra = extra;
}

pub fn clear_extra(w: &mut Writer) {
    w.extra = vec![0; w.atoms.len()];
}

pub fn optimise(w: &mut Writer) -> Search {
    let cx = w.cx;
    let mut out = Search::default();
    let keys: Vec<FactId> = w.nodes.keys().copied().collect();
    let none = BTreeSet::new();
    for f in keys {
        if cx.timed_out() {
            clear_extra(w);
            return out;
        }
        let node = w.nodes[&f].clone();
        let present = present_cost(&node);
        let Some(parts) = node.parts.as_ref() else {
            let t = present + deps_total(w, &out.total, &node.deps, f);
            out.total.insert(f, t);
            continue;
        };
        if node.theorem || node.label == "identity" {
            let t = present + deps_total(w, &out.total, &node.deps, f);
            out.total.insert(f, t);
            continue;
        }
        let focus = w.focus_of(f);
        let req = 4 * node.requires.iter().filter(|&&r| !cx.is_hyp(r)).count() as i64;
        let cur = edge_cost(w, parts, &focus) + present + req + deps_total(w, &out.total, &node.deps, f);
        let mut best = cur;
        set_extra(w, &out.total, f, &none);
        let obls = if f == GOAL { w.goal_obls(node.extra.first().copied()) } else { w.node_obls(f).unwrap_or_default() };
        let exclude = if f == GOAL { node.extra.first().copied() } else { Some(f) };
        out.examined += 1;
        if let Some((label, parts2, _, req2)) = w.certify_obls(&obls, node.horizon, &focus, exclude) {
            let mut deps = w.deps_of(&parts2);
            for &r in &req2 {
                for s in cx.fact_sources(r) {
                    deps.insert(cx.displayable(s));
                }
            }
            deps.remove(&f);
            clear_extra(w);
            let ec = edge_cost(w, &parts2, &focus);
            let req_c = 4 * req2.iter().filter(|&&r| !cx.is_hyp(r)).count() as i64;
            let alt = ec + present + req_c + deps_total(w, &out.total, &deps, f);
            if alt < cur {
                best = alt;
                out.improved += 1;
                let n = w.nodes.get_mut(&f).unwrap();
                n.label = label;
                n.parts = Some(parts2);
                n.deps = deps;
                n.requires = req2;
                n.cost = ec;
            }
        }
        clear_extra(w);
        out.total.insert(f, best);
    }
    out.complete = true;
    out
}

pub fn protect_pruned(w: &mut Writer, s: &Search) {
    let live = w.reachable();
    let skip: BTreeSet<FactId> = live.into_iter().collect();
    set_extra(w, &s.total, GOAL, &skip);
}
