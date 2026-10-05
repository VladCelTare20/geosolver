use super::atoms::{build_all, Atom, AtomSrc};
use super::cert::certify_greedy;
use super::chain::{angle_expr, atom_points, bfs, dfs_order, ratio_expr, single_angle, Path};
use super::classify::{fact_obligations, fact_points, fact_stmt, goal_obligations, kind, pred_stmt, Kind, Obl, Target};
use super::ctx::{Ctx, FactClass};
use super::model::*;
use super::trace::Table;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason as ER};
use crate::rational::Rat;
use std::collections::{BTreeMap, BTreeSet};

pub const GOAL: FactId = FactId::MAX - 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Claim,
    Step,
    Inline,
    MergeConclusion,
    Conclusion,
    Raw,
}

#[derive(Clone, Debug)]
pub struct Part {
    pub target: Target,
    pub support: Vec<(usize, Rat)>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub fact: FactId,
    pub extra: Vec<FactId>,
    pub kind: Kind,
    pub stmt: Stmt,
    pub horizon: FactId,
    pub label: &'static str,
    pub parts: Option<Vec<Part>>,
    pub theorem: bool,
    pub cost: i64,
    pub deps: BTreeSet<FactId>,
    pub role: Role,
    pub block: Option<u16>,
    pub claim_no: u16,
    pub links: usize,
    pub pooled: bool,
}

pub struct Writer<'c, 'a> {
    pub cx: &'c Ctx<'a>,
    pub atoms: Vec<Atom>,
    pub nodes: BTreeMap<FactId, Node>,
    pub claim_cost: BTreeSet<FactId>,
    pub bfs_budget: usize,
}

fn eff(atom: &Atom, focus: &BTreeSet<PointId>, claims: &BTreeSet<FactId>, cx: &Ctx) -> u32 {
    if atom.cost == 0 {
        return 0;
    }
    let mut cost = atom.cost;
    if let Some(f) = atom.fact() {
        if claims.contains(&cx.displayable(f)) && matches!(atom.src, AtomSrc::Engine(_) | AtomSrc::Line(_)) {
            cost = 1;
        }
    }
    let pts = atom_points(cx, atom);
    let foreign = pts.iter().filter(|p| !focus.contains(p)).count() as u32;
    cost * 4 + foreign.min(4) + atom.sources.len() as u32
}

impl<'c, 'a> Writer<'c, 'a> {
    pub fn new(cx: &'c Ctx<'a>) -> Writer<'c, 'a> {
        let atoms = build_all(cx);
        Writer { cx, atoms, nodes: BTreeMap::new(), claim_cost: BTreeSet::new(), bfs_budget: 20_000 }
    }

    pub fn certify(&self, tb: Table, target: &LinComb, h: FactId, focus: &BTreeSet<PointId>, exclude: Option<FactId>) -> Option<(Vec<(usize, Rat)>, i64)> {
        let cx = self.cx;
        let mut idx: Vec<(u32, usize)> = self
            .atoms
            .iter()
            .enumerate()
            .filter(|(_, a)| a.table == tb && a.admissible(h) && (exclude.is_none() || a.fact() != exclude))
            .map(|(i, a)| (eff(a, focus, &self.claim_cost, cx), i))
            .collect();
        idx.sort();
        let rows: Vec<(u32, &LinComb)> = idx.iter().map(|(c, i)| (*c, &self.atoms[*i].row)).collect();
        let cert = certify_greedy(&cx.piv[tb.idx()], &rows, target, &|k| self.atoms[idx[k].1].cost > 0)?;
        let mut cost = 0i64;
        let out: Vec<(usize, Rat)> = cert
            .into_iter()
            .map(|(k, l)| {
                cost += idx[k].0 as i64 + if l.abs().is_one() { 0 } else { 2 };
                (idx[k].1, l)
            })
            .collect();
        Some((out, cost))
    }

    fn focus_of(&self, f: FactId) -> BTreeSet<PointId> {
        if f == GOAL {
            self.cx.goal.points.iter().copied().collect()
        } else {
            fact_points(self.cx, f).into_iter().collect()
        }
    }

    fn certify_obls(&self, obls: &[Obl], h: FactId, focus: &BTreeSet<PointId>, exclude: Option<FactId>) -> Option<(&'static str, Vec<Part>, i64)> {
        let mut best: Option<(&'static str, Vec<Part>, i64)> = None;
        for o in obls {
            if self.cx.timed_out() {
                break;
            }
            let mut parts = Vec::new();
            let mut total = 0i64;
            let mut ok = true;
            for t in &o.targets {
                match self.certify(t.table, &t.row, h, focus, exclude) {
                    Some((s, c)) => {
                        total += c;
                        parts.push(Part { target: t.clone(), support: s });
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && best.as_ref().is_none_or(|b| total < b.2) {
                best = Some((o.label, parts, total));
            }
        }
        best
    }

    pub fn deps_of(&self, parts: &[Part]) -> BTreeSet<FactId> {
        let mut out = BTreeSet::new();
        for p in parts {
            for (i, _) in &p.support {
                for &s in &self.atoms[*i].sources {
                    out.insert(self.cx.displayable(s));
                }
            }
        }
        out
    }

    pub fn first_pass(&mut self) {
        let cx = self.cx;
        let goal_fact = self.goal_fact();
        for &f in &cx.closure {
            if cx.timed_out() {
                break;
            }
            let class = cx.class[f as usize];
            let needs = class == FactClass::Derived || (class == FactClass::Silent && cx.fact_sources(f).contains(&f));
            if !needs || Some(f) == goal_fact {
                continue;
            }
            let node = self.make_node(f);
            self.nodes.insert(f, node);
        }
        for &f in &cx.closure {
            if let FactClass::TheoremReg(g) | FactClass::MergeReg(g) = cx.class[f as usize] {
                if let Some(n) = self.nodes.get_mut(&g) {
                    n.extra.push(f);
                    if let ER::Collinear(p) = &cx.t.facts[f as usize].reason {
                        n.stmt = Stmt::Coll { pts: p.clone() };
                    }
                }
            }
        }
        let goal = self.make_goal(goal_fact);
        self.nodes.insert(GOAL, goal);
    }

    fn goal_fact(&self) -> Option<FactId> {
        let cx = self.cx;
        if cx.deps.len() == 1 {
            let g = cx.deps[0];
            if matches!(cx.t.facts.get(g as usize).map(|x| &x.reason), Some(ER::Collinear(_) | ER::Concyclic(_))) && cx.class[g as usize] == FactClass::Derived {
                return Some(g);
            }
        }
        None
    }

    fn make_node(&self, f: FactId) -> Node {
        let cx = self.cx;
        let focus = self.focus_of(f);
        let k = kind(cx, f);
        let stmt = fact_stmt(cx, f);
        let mut node = Node {
            fact: f,
            extra: Vec::new(),
            kind: k,
            stmt,
            horizon: f,
            label: "",
            parts: None,
            theorem: false,
            cost: 0,
            deps: BTreeSet::new(),
            role: Role::Raw,
            block: None,
            claim_no: 0,
            links: 0,
            pooled: false,
        };
        match fact_obligations(cx, f) {
            Some(obls) if obls.is_empty() && matches!(k, Kind::Formula(TheoremKey::LawOfSines)) => {
                node.parts = Some(Vec::new());
                node.label = "identity";
            }
            Some(obls) => {
                if let Some((label, parts, cost)) = self.certify_obls(&obls, f, &focus, Some(f)) {
                    node.deps = self.deps_of(&parts);
                    node.label = label;
                    node.parts = Some(parts);
                    node.cost = cost;
                }
            }
            None => {}
        }
        if node.parts.is_none() && matches!(k, Kind::Theorem(_) | Kind::Formula(_)) {
            node.theorem = true;
            let mut d = BTreeSet::new();
            for &p in &cx.t.facts[f as usize].premises {
                for s in cx.fact_sources(p) {
                    d.insert(cx.displayable(s));
                }
            }
            node.deps = d;
        } else if node.parts.is_none() {
            let mut d = BTreeSet::new();
            for &p in &cx.t.facts[f as usize].premises {
                for s in cx.fact_sources(p) {
                    d.insert(cx.displayable(s));
                }
            }
            node.deps = d;
        }
        node.deps.remove(&f);
        node
    }

    fn make_goal(&self, goal_fact: Option<FactId>) -> Node {
        let cx = self.cx;
        let h = goal_fact.unwrap_or(cx.closure.last().map(|x| x + 1).unwrap_or(0));
        let obls = goal_obligations(cx, &cx.goal);
        let focus = self.focus_of(GOAL);
        let mut node = Node {
            fact: GOAL,
            extra: goal_fact.into_iter().collect(),
            kind: Kind::Other,
            stmt: pred_stmt(&cx.goal),
            horizon: h,
            label: "",
            parts: None,
            theorem: false,
            cost: 0,
            deps: BTreeSet::new(),
            role: Role::Conclusion,
            block: None,
            claim_no: 0,
            links: 0,
            pooled: false,
        };
        if let Some((label, parts, cost)) = self.certify_obls(&obls, h, &focus, goal_fact) {
            node.deps = self.deps_of(&parts);
            node.label = label;
            node.parts = Some(parts);
            node.cost = cost;
        }
        node
    }

    pub fn recertify_with_claims(&mut self) {
        let claims: BTreeSet<FactId> = self.nodes.values().filter(|n| n.role == Role::Claim).map(|n| n.fact).collect();
        if claims.is_empty() {
            return;
        }
        self.claim_cost = claims.clone();
        let first = *claims.iter().next().unwrap();
        let keys: Vec<FactId> = self.nodes.keys().copied().filter(|&k| k > first).collect();
        for k in keys {
            if self.cx.timed_out() {
                break;
            }
            let n = &self.nodes[&k];
            if n.parts.is_none() || n.theorem || n.label == "identity" {
                continue;
            }
            let obls = if k == GOAL { goal_obligations(self.cx, &self.cx.goal) } else { fact_obligations(self.cx, k).unwrap_or_default() };
            let focus = self.focus_of(k);
            let exclude = if k == GOAL { n.extra.first().copied() } else { Some(k) };
            if let Some((label, parts, cost)) = self.certify_obls(&obls, n.horizon, &focus, exclude) {
                let deps = self.deps_of(&parts);
                let n = self.nodes.get_mut(&k).unwrap();
                n.label = label;
                n.parts = Some(parts);
                n.cost = cost;
                n.deps = deps;
                n.deps.remove(&k);
            }
        }
    }

    pub fn reachable(&self) -> BTreeSet<FactId> {
        let mut seen = BTreeSet::new();
        let mut stack = vec![GOAL];
        while let Some(f) = stack.pop() {
            if !seen.insert(f) {
                continue;
            }
            if let Some(n) = self.nodes.get(&f) {
                for &d in &n.deps {
                    if self.nodes.contains_key(&d) {
                        stack.push(d);
                    }
                }
            }
        }
        seen
    }

    pub fn users(&self, live: &BTreeSet<FactId>) -> BTreeMap<FactId, BTreeSet<FactId>> {
        let mut out: BTreeMap<FactId, BTreeSet<FactId>> = BTreeMap::new();
        for &f in live {
            for &d in &self.nodes[&f].deps {
                if live.contains(&d) {
                    out.entry(d).or_default().insert(f);
                }
            }
        }
        out
    }

    pub fn select(&mut self) {
        let cx = self.cx;
        let live = self.reachable();
        let users = self.users(&live);
        let goal_fact = self.nodes[&GOAL].extra.first().copied();
        let keys: Vec<FactId> = self.nodes.keys().copied().collect();
        for k in keys {
            if k == GOAL {
                continue;
            }
            if !live.contains(&k) {
                self.nodes.get_mut(&k).unwrap().role = Role::Raw;
                continue;
            }
            let n = &self.nodes[&k];
            let u = users.get(&k).map(|s| s.len()).unwrap_or(0);
            let only_goal = users.get(&k).is_some_and(|s| s.len() == 1 && s.contains(&GOAL));
            let mut pts = fact_points(cx, k);
            for &e in &n.extra {
                pts.extend(fact_points(cx, e));
            }
            let aux = pts.iter().any(|&p| cx.is_aux(p));
            let size: usize = n.parts.as_ref().map(|ps| ps.iter().map(|p| p.support.iter().filter(|(i, _)| self.atoms[*i].cost > 0).count()).sum()).unwrap_or(0);
            let long = size >= 3;
            let role = if n.parts.is_none() && !n.theorem {
                Role::Raw
            } else {
                match n.kind {
                    Kind::Formula(_) => Role::Inline,
                    Kind::Sim => {
                        if only_goal && !aux {
                            Role::MergeConclusion
                        } else {
                            Role::Claim
                        }
                    }
                    Kind::Congruent => {
                        if aux || u >= 2 || long {
                            Role::Claim
                        } else {
                            Role::Inline
                        }
                    }
                    Kind::Merge | Kind::Other => Role::Step,
                    _ => {
                        if aux || u >= 2 || long {
                            Role::Claim
                        } else {
                            Role::Step
                        }
                    }
                }
            };
            let _ = goal_fact;
            self.nodes.get_mut(&k).unwrap().role = role;
        }
        let mut claims: Vec<FactId> = self.nodes.values().filter(|n| n.role == Role::Claim).map(|n| n.fact).collect();
        claims.sort_unstable();
        if claims.len() > 7 {
            let excess = claims.len() - 7;
            let mut demote: Vec<FactId> = claims
                .iter()
                .copied()
                .filter(|c| users.get(c).map(|s| s.len()).unwrap_or(0) <= 1)
                .collect();
            demote.truncate(excess);
            for c in demote {
                self.nodes.get_mut(&c).unwrap().role = Role::Step;
            }
        }
    }

    pub fn live_nodes(&self) -> Vec<FactId> {
        let live = self.reachable();
        self.nodes.keys().copied().filter(|k| live.contains(k) && *k != GOAL).collect()
    }
}

pub struct Presenter<'w, 'c, 'a> {
    pub w: &'w Writer<'c, 'a>,
    pub blocks: Vec<Block>,
    pub block_of: BTreeMap<FactId, u16>,
    pub inline: BTreeMap<FactId, Reason>,
    pub role: BTreeMap<FactId, Role>,
    pub claim_no: BTreeMap<FactId, u16>,
    pub allowed: BTreeSet<FactId>,
    pub as_drawn: bool,
}

pub fn collapse_key(cx: &Ctx, a: &Atom, h: FactId) -> Option<(AtomKey, Vec<PointId>)> {
    match a.src {
        AtomSrc::Human(AtomKey::Inscribed) => {
            let best = cx
                .circles
                .iter()
                .filter(|c| c.fact < h && a.args.iter().all(|p| c.pts.contains(p)))
                .max_by_key(|c| (c.pts.len(), std::cmp::Reverse(c.fact)))?;
            Some((AtomKey::Inscribed, best.pts.clone()))
        }
        _ => None,
    }
}

pub fn subsets4(args: &[PointId]) -> Vec<[PointId; 4]> {
    let mut out = Vec::new();
    for i in 0..args.len() {
        for j in i + 1..args.len() {
            for k in j + 1..args.len() {
                for l in k + 1..args.len() {
                    out.push([args[i], args[j], args[k], args[l]]);
                }
            }
        }
    }
    out
}

impl<'w, 'c, 'a> Presenter<'w, 'c, 'a> {
    fn atom_reason(&self, a: &Atom, h: FactId, collapse: bool) -> (Reason, u16) {
        let cx = self.w.cx;
        match a.src {
            AtomSrc::Human(key) => {
                let from: Vec<u16> = a.sources.iter().filter_map(|s| self.block_of.get(&cx.displayable(*s)).copied()).collect();
                if collapse {
                    if let Some((k, args)) = collapse_key(cx, a, h) {
                        let mut sorted = a.args.clone();
                        sorted.sort_unstable();
                        let row = subsets4(&args).iter().position(|q| q.as_slice() == sorted.as_slice()).unwrap_or(0) as u16;
                        return (Reason::Atom { key: k, stmt: Stmt::Cyclic { pts: args.clone() }, args, from }, row);
                    }
                }
                (Reason::Atom { key, stmt: atom_stmt(key, &a.args), args: a.args.clone(), from }, a.row_idx)
            }
            AtomSrc::Hyp(f) => (Reason::Hyp { stmt: fact_stmt(cx, f), fact: f }, a.row_idx),
            AtomSrc::Glue(f) | AtomSrc::Line(f) | AtomSrc::Engine(f) => {
                let d = cx.displayable(f);
                if let (Some(&b), Some(Role::Claim)) = (self.block_of.get(&d), self.role.get(&d)) {
                    return (Reason::Claim { n: self.claim_no.get(&d).copied().unwrap_or(0), block: b, fact: f }, a.row_idx);
                }
                if let Some(r) = self.inline.get(&d) {
                    if let Reason::Fact { stmt, block, because, .. } = r {
                        return (Reason::Fact { stmt: stmt.clone(), fact: f, block: *block, because: because.clone() }, a.row_idx);
                    }
                }
                let block = self.block_of.get(&d).copied();
                (Reason::Fact { stmt: fact_stmt(cx, f), fact: f, block, because: Vec::new() }, a.row_idx)
            }
        }
    }

    fn reasons_for(&self, support: &[(usize, Rat)], h: FactId, collapse: bool) -> (Vec<Reason>, Vec<Term>) {
        let mut reasons: Vec<Reason> = Vec::new();
        let mut terms: Vec<Term> = Vec::new();
        for (i, lam) in support {
            let a = &self.w.atoms[*i];
            if a.cost == 0 {
                continue;
            }
            let (r, row) = self.atom_reason(a, h, collapse);
            let pos = match reasons.iter().position(|x| *x == r) {
                Some(p) => p,
                None => {
                    reasons.push(r);
                    reasons.len() - 1
                }
            };
            terms.push(Term { reason: pos as u16, row, coef: lam.clone() });
        }
        (reasons, terms)
    }

    fn eligible(&self, h: FactId) -> Vec<usize> {
        let cx = self.w.cx;
        self.w
            .atoms
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if !a.admissible(h) || a.cost == 0 {
                    return false;
                }
                let srcs_ok = a.sources.iter().all(|s| {
                    let d = cx.displayable(*s);
                    self.allowed.contains(&d) || cx.class.get(d as usize).is_some_and(|c| matches!(c, FactClass::Hyp | FactClass::HypReg))
                });
                if !srcs_ok {
                    return false;
                }
                match a.src {
                    AtomSrc::Engine(f) => {
                        let d = cx.displayable(f);
                        self.role.get(&d).is_some_and(|r| *r == Role::Claim) && matches!(cx.t.facts[f as usize].reason, ER::SimilarTriangles(..))
                    }
                    _ => a.cost <= 2,
                }
            })
            .map(|(i, _)| i)
            .collect()
    }


    fn chain_sentences(&mut self, target: &Target, support: &[(usize, Rat)], h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>) -> (Vec<Sentence>, usize, bool) {
        let cx = self.w.cx;
        let eligible = self.eligible(h);
        let splits: Vec<(LinComb, LinComb)> = target
            .splits
            .iter()
            .map(|s| {
                if target.table == Table::Angle {
                    (cx.quot.q(&s.lraw), cx.quot.q(&s.rraw))
                } else {
                    (s.lraw.clone(), s.rraw.clone())
                }
            })
            .collect();
        let path = bfs(cx, &self.w.atoms, &eligible, target.table, &fix_exact(cx, target, &splits), &penalty, self.w.bfs_budget);
        if let Some(p) = path {
            if p.links.len() <= 8 && !p.links.is_empty() {
                if let Some(s) = self.path_sentences(target, &p, h, then.clone(), focus) {
                    return (s, p.links.len(), false);
                }
            }
        }
        let items: Vec<(usize, Rat, LinComb)> = support
            .iter()
            .filter(|(i, _)| self.w.atoms[*i].cost > 0 || target.table != Table::Angle)
            .map(|(i, l)| (*i, l.clone(), self.w.atoms[*i].q.clone()))
            .filter(|(_, _, q)| target.table != Table::Angle || !q.terms.iter().all(|(v, _)| *v == ANGLE_UNIT))
            .collect();
        let ex = fix_exact(cx, target, &splits);
        let mut budget = 200_000usize;
        for (si, (l, r)) in ex.iter().enumerate() {
            if let Some(order) = dfs_order(cx, target.table, l, r, &items, &mut budget) {
                let mut nodes = vec![l.clone()];
                let mut links = Vec::new();
                let mut cur = l.clone();
                for &k in &order {
                    cur = LinComb::combine(&cur, &items[k].2, &-&items[k].1);
                    nodes.push(cur.clone());
                    links.push((items[k].0, items[k].1.clone()));
                }
                let p = Path { split: si, nodes, links, weight: 0 };
                if p.links.len() <= 8 && !p.links.is_empty() {
                    if let Some(s) = self.path_sentences(target, &p, h, then.clone(), focus) {
                        return (s, p.links.len(), false);
                    }
                }
            }
        }
        let collapse = true;
        let (reasons, combination) = self.reasons_for(support, h, collapse);
        let stmt = then.unwrap_or_else(|| target.stmt.clone());
        if target.table == Table::Ratio && support.iter().any(|(i, _)| self.is_trig(*i)) {
            if let Some(s) = self.computation(target, support, h) {
                let n = match &s {
                    Sentence::Computation { links, .. } => links.len(),
                    _ => 0,
                };
                return (vec![s], n, false);
            }
        }
        let n = reasons.len();
        (vec![Sentence::Pooled { stmt, reasons, combination }], n, true)
    }

    fn is_trig(&self, i: usize) -> bool {
        let a = &self.w.atoms[i];
        match a.src {
            AtomSrc::Engine(f) => matches!(self.w.cx.t.facts[f as usize].reason, ER::Formula(..)),
            _ => false,
        }
    }

    fn computation(&mut self, target: &Target, support: &[(usize, Rat)], h: FactId) -> Option<Sentence> {
        let cx = self.w.cx;
        let split = target.splits.first()?;
        let mut items: Vec<(usize, Rat)> = support.iter().filter(|(i, _)| self.w.atoms[*i].cost > 0).cloned().collect();
        let mut cur = split.lraw.clone();
        let lhs = |c: &LinComb| c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).count();
        let mut seq: Vec<(usize, Rat, LinComb)> = Vec::new();
        while !items.is_empty() {
            let (k, _) = items
                .iter()
                .enumerate()
                .map(|(k, (i, l))| {
                    let nx = LinComb::combine(&cur, &self.w.atoms[*i].row, &-l);
                    (k, (lhs(&nx), *i))
                })
                .min_by_key(|x| x.1)?;
            let (i, l) = items.remove(k);
            cur = LinComb::combine(&cur, &self.w.atoms[i].row, &-&l);
            seq.push((i, l, cur.clone()));
        }
        if !(&cur - &split.rraw).is_zero() {
            return None;
        }
        let mut groups: Vec<(Vec<(usize, Rat)>, LinComb)> = Vec::new();
        let mut tri: Option<BTreeSet<PointId>> = None;
        for (i, l, after) in seq {
            let t = self.triangle_of(i);
            let start_new = groups.is_empty() || (t.is_some() && tri.is_some() && t != tri);
            if start_new {
                groups.push((Vec::new(), after.clone()));
                tri = None;
            }
            if t.is_some() && tri.is_none() {
                tri = t;
            }
            let g = groups.last_mut().unwrap();
            g.0.push((i, l));
            g.1 = after;
        }
        let mut terms = vec![split.l.clone()];
        let mut links = Vec::new();
        let ng = groups.len();
        for (gi, (members, after)) in groups.into_iter().enumerate() {
            let (rs, cs) = self.reasons_for(&members, h, false);
            links.push(Link { reasons: rs, combination: cs });
            terms.push(if gi + 1 == ng { split.r.clone() } else { ratio_expr(cx, &after) });
        }
        Some(Sentence::Computation { comp: CompKind::Trig, terms, links })
    }

    fn triangle_of(&self, i: usize) -> Option<BTreeSet<PointId>> {
        let cx = self.w.cx;
        let a = &self.w.atoms[i];
        if let AtomSrc::Engine(f) = a.src {
            if let ER::Formula("law of sines", _, p) = &cx.t.facts[f as usize].reason {
                if p.len() >= 3 {
                    let mut s: BTreeSet<PointId> = BTreeSet::new();
                    s.extend(p.iter().copied());
                    return Some(s);
                }
            }
        }
        None
    }

    fn path_sentences(&mut self, target: &Target, p: &Path, h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>) -> Option<Vec<Sentence>> {
        let cx = self.w.cx;
        let split = &target.splits[p.split.min(target.splits.len() - 1)];
        let n = p.nodes.len();
        let half_at = (0..p.links.len()).find(|&j| {
            !p.links[j].1.is_integer() || single_angle(&p.nodes[j]).is_some_and(|k| !k.is_integer()) || single_angle(&p.nodes[j + 1]).is_some_and(|k| !k.is_integer())
        });
        let pts_of = |j: usize| -> Vec<PointId> {
            p.links.get(j).map(|(i, _)| atom_points(cx, &self.w.atoms[*i])).unwrap_or_default()
        };
        let mut terms: Vec<(Expr, bool)> = Vec::new();
        for j in 0..n {
            let directed = half_at.is_none_or(|h0| j <= h0) && target.table == Table::Angle;
            let directed_term = directed && half_at.is_none_or(|h0| j < h0 || (j == h0 && h0 > 0));
            let e = if j == 0 && directed_term {
                split.l.clone()
            } else if j == n - 1 && (directed_term || target.table != Table::Angle) {
                split.r.clone()
            } else if target.table == Table::Angle {
                let next = pts_of(j);
                let prev = if j > 0 { pts_of(j - 1) } else { Vec::new() };
                angle_expr(cx, &p.nodes[j], directed_term, &[&next, &prev], focus)?
            } else {
                ratio_expr(cx, &p.nodes[j])
            };
            terms.push((e, directed_term));
        }
        let mut sentences = Vec::new();
        let mut links: Vec<Link> = Vec::new();
        for (i, lam) in &p.links {
            let (rs, cs) = self.reasons_for(&[(*i, lam.clone())], h, false);
            links.push(Link { reasons: rs, combination: cs });
        }
        match half_at {
            None => sentences.push(Sentence::Chain { terms: terms.into_iter().map(|x| x.0).collect(), links, then, directed: target.table == Table::Angle }),
            Some(h0) => {
                self.as_drawn = true;
                if h0 > 0 {
                    let t1: Vec<Expr> = terms[..=h0].iter().map(|x| x.0.clone()).collect();
                    sentences.push(Sentence::Chain { terms: t1, links: links[..h0].to_vec(), then: None, directed: true });
                }
                let mut t2: Vec<Expr> = Vec::new();
                for j in h0..n {
                    let e = angle_expr(cx, &p.nodes[j], false, &[&pts_of(j), &pts_of(j.saturating_sub(1))], focus)?;
                    t2.push(e);
                }
                sentences.push(Sentence::Chain { terms: t2, links: links[h0..].to_vec(), then, directed: false });
            }
        }
        Some(sentences)
    }
}

fn penalty(a: &Atom) -> i64 {
    let mut p = 0i64;
    if let AtomSrc::Line(_) = a.src {
        p += 5;
    }
    p + 5 * a.sources.len() as i64
}

fn fix_exact(cx: &Ctx, target: &Target, splits: &[(LinComb, LinComb)]) -> Vec<(LinComb, LinComb)> {
    if target.table != Table::Angle {
        return splits.to_vec();
    }
    splits
        .iter()
        .zip(&target.splits)
        .map(|((l, r), s)| {
            let lv = cx.t.value(Table::Angle, &s.lraw);
            let rv = cx.t.value(Table::Angle, &s.rraw);
            let mut r2 = r.clone();
            let k = (lv - rv).round() as i64;
            r2.add_term(ANGLE_UNIT, Rat::from_int(k));
            (l.clone(), r2)
        })
        .collect()
}

pub fn atom_stmt(key: AtomKey, a: &[PointId]) -> Stmt {
    let ang = |x: PointId, y: PointId, z: PointId| Expr::Angle { a: x, b: y, c: z, directed: true };
    match key {
        AtomKey::Inscribed => Stmt::Cyclic { pts: a.to_vec() },
        AtomKey::Thales => Stmt::AngleConst { angle: Expr::Angle { a: a[0], b: a[2], c: a[1], directed: false }, degrees: Rat::from_int(90) },
        AtomKey::TangentChord => Stmt::Tangent { p: a[0], line: (a[0], a[1]), circle: vec![a[4]] },
        AtomKey::PerpBisector => Stmt::Perp { l1: (a[0], a[1]), l2: (a[2], a[3]) },
        AtomKey::Parallel => Stmt::Para { l1: (a[0], a[1]), l2: (a[2], a[3]) },
        AtomKey::Radii | AtomKey::Isosceles => Stmt::Cong { s1: (a[0], a[1]), s2: (a[0], a[2]) },
        AtomKey::CentralAngle => Stmt::Eq {
            lhs: ang(a[1], a[0], a[2]),
            rhs: Expr::Lin { terms: vec![(Rat::from_int(2), ang(a[1], a[3], a[2]))] },
        },
        AtomKey::PowerOfPoint => Stmt::Eq {
            lhs: Expr::Prod { factors: vec![(Expr::Seg { a: a[0], b: a[1] }, 1), (Expr::Seg { a: a[0], b: a[2] }, 1)] },
            rhs: Expr::Prod { factors: vec![(Expr::Seg { a: a[0], b: a[3] }, 1), (Expr::Seg { a: a[0], b: a[4] }, 1)] },
        },
        AtomKey::Midline => Stmt::Para { l1: (a[0], a[1]), l2: (a[2], a[3]) },
        AtomKey::Orthocentre => Stmt::Perp { l1: (a[3], a[0]), l2: (a[1], a[2]) },
    }
}

impl<'w, 'c, 'a> Presenter<'w, 'c, 'a> {
    pub fn new(w: &'w Writer<'c, 'a>) -> Self {
        Presenter {
            w,
            blocks: Vec::new(),
            block_of: BTreeMap::new(),
            inline: BTreeMap::new(),
            role: BTreeMap::new(),
            claim_no: BTreeMap::new(),
            allowed: BTreeSet::new(),
            as_drawn: false,
        }
    }

    pub fn node_sentences(&mut self, f: FactId) -> (Vec<Sentence>, usize, bool) {
        let cx = self.w.cx;
        let node = &self.w.nodes[&f];
        let h = node.horizon;
        let focus = self.w.focus_of(f);
        let mut out = Vec::new();
        let mut links = 0;
        let mut pooled = false;
        if node.theorem {
            let mut reasons = Vec::new();
            if f != GOAL {
                for &p in &cx.t.facts[f as usize].premises {
                    let d = cx.displayable(p);
                    if cx.is_hyp(p) {
                        if matches!(cx.class[p as usize], FactClass::Hyp) {
                            reasons.push(Reason::Hyp { stmt: fact_stmt(cx, p), fact: p });
                        }
                    } else if let Some(&b) = self.block_of.get(&d) {
                        match self.role.get(&d) {
                            Some(Role::Claim) => reasons.push(Reason::Claim { n: self.claim_no[&d], block: b, fact: d }),
                            _ => reasons.push(Reason::Fact { stmt: fact_stmt(cx, d), fact: d, block: Some(b), because: vec![] }),
                        }
                    } else if let Some(r) = self.inline.get(&d) {
                        reasons.push(r.clone());
                    }
                }
            }
            let key = match node.kind {
                Kind::Theorem(k) | Kind::Formula(k) => k,
                _ => TheoremKey::Other,
            };
            out.push(Sentence::Theorem { key, stmt: node.stmt.clone(), reasons });
            return (out, 0, false);
        }
        let Some(parts) = node.parts.clone() else {
            let cites = cx.t.facts.get(f as usize).map(|x| x.premises.clone()).unwrap_or_default();
            out.push(Sentence::Raw { engine_fact: f, cites });
            return (out, 0, false);
        };
        let node_kind = node.kind;
        let node_stmt = node.stmt.clone();
        let nparts = parts.len();
        for (pi, part) in parts.iter().enumerate() {
            if !part.support.iter().any(|(i, _)| self.w.atoms[*i].cost > 0) {
                continue;
            }
            let then = if pi + 1 == nparts {
                match (&node_stmt, node_kind) {
                    (Stmt::Cyclic { .. } | Stmt::Coll { .. }, _) if part.target.table == Table::Angle => Some(node_stmt.clone()),
                    _ => None,
                }
            } else {
                None
            };
            let then = if f == GOAL && pi + 1 == nparts { Some(node_stmt.clone()) } else { then };
            let (s, l, p) = self.chain_sentences(&part.target, &part.support, h, then, &focus);
            links += l;
            pooled |= p;
            out.extend(s);
        }
        match node_kind {
            Kind::Sim | Kind::Congruent if f != GOAL => {
                out.push(Sentence::Because { stmt: node_stmt.clone(), reasons: Vec::new(), combination: Vec::new() });
            }
            Kind::Cong => {
                out.push(Sentence::Theorem { key: TheoremKey::ArcChord, stmt: node_stmt.clone(), reasons: Vec::new() });
            }
            Kind::Theorem(k) => {
                out.push(Sentence::Theorem { key: k, stmt: node_stmt.clone(), reasons: Vec::new() });
            }
            _ => {}
        }
        (out, links, pooled)
    }

    pub fn run(&mut self) {
        let cx = self.w.cx;
        let live = self.w.live_nodes();
        let mut order: Vec<FactId> = live.clone();
        order.sort_unstable();
        let mut next_claim = 1u16;
        for &f in &order {
            let n = &self.w.nodes[&f];
            self.role.insert(f, n.role);
        }
        let mut merge_sentences: Vec<Sentence> = Vec::new();
        let mut merge_facts: Vec<FactId> = Vec::new();
        for &f in &order {
            if cx.timed_out() {
                let n = &self.w.nodes[&f];
                let id = self.blocks.len() as u16 + 1;
                self.blocks.push(raw_block(cx, id, f, n.horizon));
                self.block_of.insert(f, id);
                self.allowed.insert(f);
                continue;
            }
            let role = self.role[&f];
            let (sentences, links, pooled) = self.node_sentences(f);
            let node = &self.w.nodes[&f];
            let stmt = node.stmt.clone();
            let mut facts = vec![f];
            facts.extend(node.extra.iter().copied());
            match role {
                Role::Inline => {
                    let because: Vec<Reason> = sentences
                        .iter()
                        .flat_map(|s| match s {
                            Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().flat_map(|l| l.reasons.clone()).collect::<Vec<_>>(),
                            Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => reasons.clone(),
                            Sentence::Raw { .. } => Vec::new(),
                        })
                        .collect();
                    let mut dedup: Vec<Reason> = Vec::new();
                    for r in because {
                        if !dedup.contains(&r) {
                            dedup.push(r);
                        }
                    }
                    self.inline.insert(f, Reason::Fact { stmt, fact: f, block: None, because: dedup });
                    self.allowed.insert(f);
                }
                Role::MergeConclusion => {
                    merge_sentences.extend(sentences);
                    merge_facts.extend(facts);
                    self.allowed.insert(f);
                }
                Role::Claim | Role::Step | Role::Raw | Role::Conclusion => {
                    let id = self.blocks.len() as u16 + 1;
                    let kind = match role {
                        Role::Claim => {
                            self.claim_no.insert(f, next_claim);
                            next_claim += 1;
                            BlockKind::Claim(next_claim - 1)
                        }
                        Role::Raw => BlockKind::Raw,
                        _ => {
                            if sentences.iter().any(|s| matches!(s, Sentence::Raw { .. })) {
                                BlockKind::Raw
                            } else {
                                BlockKind::Step
                            }
                        }
                    };
                    let mut points = Vec::new();
                    for &x in &facts {
                        points.extend(fact_points(cx, x));
                    }
                    points.sort_unstable();
                    points.dedup();
                    let body = if role == Role::Raw && !sentences.iter().any(|s| matches!(s, Sentence::Raw { .. })) {
                        vec![Sentence::Raw { engine_fact: f, cites: cx.t.facts[f as usize].premises.clone() }]
                    } else {
                        sentences
                    };
                    let objects = objects_of(&stmt);
                    self.blocks.push(Block { id, kind, stmt, body, engine_facts: facts, points, objects, horizon: node.horizon });
                    self.block_of.insert(f, id);
                    self.allowed.insert(f);
                    let _ = (links, pooled);
                }
            }
        }
        let goal = &self.w.nodes[&GOAL];
        let id = self.blocks.len() as u16 + 1;
        let mut facts = merge_facts;
        facts.extend(goal.extra.iter().copied());
        let (mut body, _, _) = if goal.parts.is_some() { self.node_sentences(GOAL) } else { (Vec::new(), 0, false) };
        let mut all = merge_sentences;
        all.append(&mut body);
        let mut points: Vec<PointId> = cx.goal.points.clone();
        for &x in &facts {
            points.extend(fact_points(cx, x));
        }
        points.sort_unstable();
        points.dedup();
        let stmt = goal.stmt.clone();
        let objects = objects_of(&stmt);
        self.blocks.push(Block { id, kind: BlockKind::Conclusion, stmt, body: all, engine_facts: facts, points, objects, horizon: goal.horizon });
    }
}

fn objects_of(stmt: &Stmt) -> Vec<ObjRef> {
    match stmt {
        Stmt::Cyclic { pts } => vec![ObjRef::Circle { through: pts.clone() }],
        Stmt::Coll { pts } => vec![ObjRef::Line { through: pts.clone() }],
        _ => Vec::new(),
    }
}

pub fn raw_block(cx: &Ctx, id: u16, f: FactId, horizon: FactId) -> Block {
    let fact = &cx.t.facts[f as usize];
    Block {
        id,
        kind: BlockKind::Raw,
        stmt: fact_stmt(cx, f),
        body: vec![Sentence::Raw { engine_fact: f, cites: fact.premises.clone() }],
        engine_facts: vec![f],
        points: fact_points(cx, f),
        objects: Vec::new(),
        horizon,
    }
}
