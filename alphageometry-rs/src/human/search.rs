use super::chain::{ratio_expr, ratio_splits, Path};
use super::claims::{eff, fix_exact, Node, Part, Writer, GOAL};
use super::classify::{Kind, Split, Target};
use super::model::{AtomKey, Expr};
use super::route;
use super::trace::Table;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::FactId;
use std::collections::{BTreeMap, BTreeSet};

pub const EXTRA_CAP: i64 = 2_000;
pub const ROUTE_SLACK: i64 = 12;
pub const VIA_COST: i64 = 30;
pub const UNDIRECTED_BONUS: i64 = 10;
pub const MAX_FORMS: usize = 3;

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

fn via_cost(w: &Writer, a: &super::atoms::Atom) -> i64 {
    a.via
        .iter()
        .map(|&v| match w.atoms[v].key() {
            Some(AtomKey::PerpBisector | AtomKey::Thales | AtomKey::Midline | AtomKey::Orthocentre | AtomKey::Radii | AtomKey::Isosceles) => 6,
            _ => VIA_COST,
        })
        .sum()
}

fn kind_cost(a: &super::atoms::Atom) -> i64 {
    match a.key() {
        Some(AtomKey::CentralAngle) => 4,
        Some(k) if super::theorems::is_congruence(k) => 10,
        _ => 0,
    }
}

fn route_weights(w: &Writer, f: FactId, h: FactId, exclude: Option<FactId>, focus: &BTreeSet<PointId>, total: &BTreeMap<FactId, i64>) -> Vec<i64> {
    let cx = w.cx;
    let mut out = vec![route::OFF; w.atoms.len()];
    for (i, a) in w.atoms.iter().enumerate() {
        if a.cost == 0 || !a.admissible(h) || !matches!(a.table, Table::Angle | Table::Ratio) {
            continue;
        }
        if exclude.is_some() && a.fact() == exclude {
            continue;
        }
        let mut prem = 0i64;
        let mut seen: BTreeSet<FactId> = BTreeSet::new();
        let mut cyclic = false;
        for &s in &a.sources {
            let d = cx.displayable(s);
            if d == f {
                cyclic = true;
                break;
            }
            if w.nodes.contains_key(&d) && seen.insert(d) {
                prem += total.get(&d).copied().unwrap_or(0);
            }
        }
        if cyclic {
            continue;
        }
        out[i] = eff(a, focus, &w.claim_cost, cx) as i64 + 2 + via_cost(w, a) + kind_cost(a) + prem.min(EXTRA_CAP);
    }
    out
}

pub struct Graphs {
    g: [std::cell::OnceCell<route::Graph>; 3],
}

impl Graphs {
    pub fn new() -> Graphs {
        Graphs { g: [std::cell::OnceCell::new(), std::cell::OnceCell::new(), std::cell::OnceCell::new()] }
    }
    fn get(&self, w: &Writer, table: Table, undirected: bool) -> &route::Graph {
        let i = match (table, undirected) {
            (Table::Angle, true) => 0,
            (Table::Angle, false) => 1,
            _ => 2,
        };
        self.g[i].get_or_init(|| route::Graph::new(w.cx, &w.atoms, table, undirected))
    }
}

impl Default for Graphs {
    fn default() -> Self {
        Graphs::new()
    }
}

pub fn route_splits(w: &Writer, form: &mut Target) -> Vec<(LinComb, LinComb)> {
    let cx = w.cx;
    match form.table {
        Table::Angle => {
            let raw: Vec<(LinComb, LinComb)> = form.splits.iter().map(|s| (cx.quot.q(&s.lraw), cx.quot.q(&s.rraw))).collect();
            fix_exact(cx, form, &raw)
        }
        _ => {
            let row = cx.t.canon_ratio(&form.row);
            for (a, end) in ratio_splits(cx, &row) {
                for (l, r) in [(a.clone(), end.clone()), (end, a)] {
                    if form.splits.iter().any(|s| cx.t.canon_ratio(&s.lraw) == l && cx.t.canon_ratio(&s.rraw) == r) {
                        continue;
                    }
                    form.splits.push(Split { l: ratio_expr(cx, &l), r: ratio_expr(cx, &r), lraw: l, rraw: r });
                }
            }
            form.splits.iter().map(|s| (s.lraw.clone(), s.rraw.clone())).collect()
        }
    }
}

fn undirected_splits(w: &Writer, form: &Target) -> Vec<(LinComb, LinComb)> {
    let cx = w.cx;
    let t = cx.t;
    let undirect = |e: &Expr| -> Option<LinComb> {
        match e {
            Expr::Angle { a, b, c, .. } => super::expr::eval(t, &Expr::Angle { a: *a, b: *b, c: *c, directed: false }).map(|x| x.1),
            Expr::Const { degrees } => Some(LinComb::singleton(crate::elimination::ANGLE_UNIT, degrees / &crate::rational::Rat::from_int(180))),
            _ => None,
        }
    };
    let mut out = Vec::new();
    for s in &form.splits {
        let (Some(l), Some(r)) = (undirect(&s.l), undirect(&s.r)) else {
            out.push((LinComb::zero(), LinComb::singleton(crate::elimination::ANGLE_UNIT, crate::rational::Rat::new(1, 3))));
            continue;
        };
        let (vl, vr) = (t.value(Table::Angle, &l), t.value(Table::Angle, &r));
        let r = if (vl - vr).abs() < 1e-7 {
            r
        } else if (vl + vr - 1.0).abs() < 1e-7 {
            &LinComb::singleton(crate::elimination::ANGLE_UNIT, crate::rational::Rat::one()) - &r
        } else {
            out.push((LinComb::zero(), LinComb::singleton(crate::elimination::ANGLE_UNIT, crate::rational::Rat::new(1, 3))));
            continue;
        };
        out.push((cx.quot.q(&l), cx.quot.q(&r)));
    }
    out
}

fn route_edge_cost(w: &Writer, p: &Path, focus: &BTreeSet<PointId>, table: Table) -> i64 {
    let cx = w.cx;
    let mut c = 0i64;
    for (k, g) in p.links.iter().enumerate() {
        for (i, lam) in g {
            c += eff(&w.atoms[*i], focus, &w.claim_cost, cx) as i64 + 2 + via_cost(w, &w.atoms[*i]) + kind_cost(&w.atoms[*i]) + if lam.abs().is_one() { 0 } else { 8 };
        }
        if k + 1 < p.links.len() && !p.undirected {
            c += route::directed_shape(cx, table, &p.nodes[k + 1]).unwrap_or(0);
        }
    }
    c
}

pub fn route_node(w: &Writer, f: FactId, total: &BTreeMap<FactId, i64>, graphs: &Graphs) -> Option<(&'static str, Vec<Part>, i64, Vec<FactId>)> {
    let cx = w.cx;
    let node = w.nodes.get(&f)?;
    let h = node.horizon;
    let focus = w.focus_of(f);
    let (obls, exclude) = if f == GOAL { (w.goal_obls(node.extra.first().copied()), node.extra.first().copied()) } else { (w.node_obls(f)?, Some(f)) };
    let weights = route_weights(w, f, h, exclude, &focus, total);
    let mut best: Option<(i64, &'static str, Part, Vec<FactId>, i64)> = None;
    let mut forms_tried = 0usize;
    for o in &obls {
        if o.targets.len() != 1 || cx.timed_out() {
            continue;
        }
        let t = &o.targets[0];
        let req = 4 * o.requires.iter().filter(|&&r| !cx.is_hyp(r)).count() as i64;
        for form in std::iter::once(t).chain(t.alts.iter()) {
            if !matches!(form.table, Table::Angle | Table::Ratio) || forms_tried >= MAX_FORMS || w.route_spent.load(std::sync::atomic::Ordering::Relaxed) >= route::TOTAL_BUDGET {
                continue;
            }
            forms_tried += 1;
            let mut form = form.clone();
            form.alts.clear();
            let splits = route_splits(w, &mut form);
            let mut found: Vec<Path> = Vec::new();
            if form.table == Table::Angle {
                let usplits = undirected_splits(w, &form);
                let bound = best.as_ref().map(|b| b.0 - req + UNDIRECTED_BONUS + 1).unwrap_or(i64::MAX);
                let g = graphs.get(w, Table::Angle, true);
                if let Some(mut p) = g.search(cx, &usplits, route::UNDIRECTED_BUDGET, bound, &w.route_spent, &weights) {
                    p.weight -= UNDIRECTED_BONUS;
                    found.push(p);
                }
            }
            let bound = best.as_ref().map(|b| b.0 - req + 1).unwrap_or(i64::MAX).min(found.first().map(|p| p.weight + 1).unwrap_or(i64::MAX));
            let g = graphs.get(w, form.table, false);
            if let Some(p) = g.search(cx, &splits, route::BUDGET, bound, &w.route_spent, &weights) {
                found.push(p);
            }
            for p in found {
                let key = p.weight + req;
                if best.as_ref().is_none_or(|b| key < b.0) {
                    let c = route_edge_cost(w, &p, &focus, form.table) + req;
                    let support = route::support(&p);
                    best = Some((key, o.label, Part { target: form.clone(), support, route: Some(p) }, o.requires.clone(), c));
                }
            }
        }
    }
    let (_, label, part, req, c) = best?;
    Some((label, vec![part], c, req))
}

fn needs_route(w: &Writer, parts: &[Part]) -> bool {
    let atoms: Vec<&(usize, crate::rational::Rat)> = parts.iter().flat_map(|p| p.support.iter()).filter(|(i, _)| w.atoms[*i].cost > 0).collect();
    atoms.len() >= 3 || atoms.iter().any(|(i, lam)| !lam.abs().is_one() || w.atoms[*i].cost >= 3)
}

pub fn optimise(w: &mut Writer) -> Search {
    optimise_with(w, true)
}

pub fn optimise_with(w: &mut Writer, recert: bool) -> Search {
    let cx = w.cx;
    let mut out = Search::default();
    let keys: Vec<FactId> = w.nodes.keys().copied().collect();
    let none = BTreeSet::new();
    let graphs = Graphs::new();
    let mut route_time = 0.0f64;
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
        if recert {
            set_extra(w, &out.total, f, &none);
        }
        let obls = if f == GOAL { w.goal_obls(node.extra.first().copied()) } else { w.node_obls(f).unwrap_or_default() };
        let exclude = if f == GOAL { node.extra.first().copied() } else { Some(f) };
        out.examined += 1;
        let recertified = if recert { w.certify_obls(&obls, node.horizon, &focus, exclude) } else { None };
        if let Some((label, parts2, _, req2)) = recertified {
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
        let wants_route = f == GOAL || w.nodes[&f].parts.as_ref().is_some_and(|ps| needs_route(w, ps));
        let t_route = std::time::Instant::now();
        let routed = if wants_route { route_node(w, f, &out.total, &graphs) } else { None };
        route_time += t_route.elapsed().as_secs_f64();
        if let Some((label, parts3, ec3, req3)) = routed {
            let mut deps = w.deps_of(&parts3);
            for &r in &req3 {
                for s in cx.fact_sources(r) {
                    deps.insert(cx.displayable(s));
                }
            }
            deps.remove(&f);
            let alt = ec3 + present + deps_total(w, &out.total, &deps, f);
            if std::env::var_os("HP_ROUTE").is_some() {
                eprintln!("route node {f}: route {alt} vs best {best} ({} links)", parts3[0].route.as_ref().map(|p| p.links.len()).unwrap_or(0));
            }
            if alt <= best + ROUTE_SLACK {
                best = alt.min(best);
                out.improved += 1;
                let n = w.nodes.get_mut(&f).unwrap();
                n.label = label;
                n.parts = Some(parts3);
                n.deps = deps;
                n.requires = req3;
                n.cost = ec3;
            }
        }
        out.total.insert(f, best);
    }
    out.complete = true;
    if std::env::var_os("HP_TIME").is_some() {
        eprintln!("route search: {:.1} ms, {} expansions", route_time * 1e3, w.route_spent.load(std::sync::atomic::Ordering::Relaxed));
    }
    out
}

pub fn protect_pruned(w: &mut Writer, s: &Search) {
    let live = w.reachable();
    let skip: BTreeSet<FactId> = live.into_iter().collect();
    set_extra(w, &s.total, GOAL, &skip);
}
