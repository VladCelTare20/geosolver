use super::atoms::{build_all, Atom, AtomSrc};
use super::cert::certify_greedy;
use super::chain::{atom_points, bfs, classes, decomps, display_node, expr_badness, order_items, order_items_mode, ratio_expr, ratio_splits, single_angle, term_comb, Group, Path};
use super::ctx::QBASE;
use crate::lincomb::VarId;
use super::classify::{coll_targets, cyc_forms, fact_obligations, fact_points, fact_stmt, goal_obligations, kind, pred_stmt, Kind, Obl, Target};
use super::ctx::{Ctx, FactClass};
use super::model::*;
use super::trace::Table;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason as ER};
use crate::rational::Rat;
use std::collections::{BTreeMap, BTreeSet};

const MERGED: &str = "merged";
const MAX_CLAIMS: usize = 7;
const ORDER_BUDGET: usize = 40_000;
const DIRECT_CAP: usize = 10_000;
const MAIN_CAP: usize = 2_500;
const SUB_CAP: usize = 400;
const LONG_CHAIN: usize = 8;
const SENTENCE_COST: i64 = 14;
const SHORT_REASONS: usize = 3;

fn long_penalty(links: usize) -> i64 {
    8 * links.saturating_sub(6) as i64
}

pub struct LemmaSplit {
    pub cost: i64,
    pub plans: Vec<LemmaPlan>,
    pub reduced: Vec<(usize, Rat, LinComb)>,
    pub groups: Vec<Vec<usize>>,
    pub halves: bool,
}

#[derive(Clone, Debug)]
pub struct LemmaRef {
    pub stmt: Stmt,
    pub sentence: usize,
    pub pts: Vec<PointId>,
    pub scale: Rat,
}

#[derive(Clone, Debug)]
pub struct LemmaPlan {
    pub items: Vec<(usize, Rat, LinComb)>,
    pub groups: Vec<Vec<usize>>,
    pub start: LinComb,
    pub sum: LinComb,
    pub lhs: Expr,
    pub rhs: Expr,
}

#[derive(Clone, Debug)]
pub struct LemmaCand {
    pub members: Vec<usize>,
    pub sum: LinComb,
    pub start: LinComb,
    pub end: LinComb,
    pub groups: Vec<Vec<usize>>,
    pub cost: i64,
    pub lhs: Expr,
    pub rhs: Expr,
}

impl LemmaCand {
    fn plan(&self, work: &[(usize, Rat, LinComb)]) -> LemmaPlan {
        LemmaPlan {
            items: self.members.iter().map(|&k| work[k].clone()).collect(),
            groups: self.groups.clone(),
            start: self.start.clone(),
            sum: self.sum.clone(),
            lhs: self.lhs.clone(),
            rhs: self.rhs.clone(),
        }
    }
}

fn integral_node(c: &LinComb) -> bool {
    c.terms.iter().all(|(v, k)| *v < QBASE || k.is_integer())
}

fn item_points(w: &Writer, i: usize, lem: &[LemmaRef]) -> Vec<PointId> {
    let base = w.atoms.len();
    if i < base {
        atom_points(w.cx, &w.atoms[i])
    } else {
        lem.get(i - base).map(|l| l.pts.clone()).unwrap_or_default()
    }
}

fn reduce_items(work: &[(usize, Rat, LinComb)], c: &LemmaCand, id: usize) -> Vec<(usize, Rat, LinComb)> {
    let mut out: Vec<(usize, Rat, LinComb)> = work.iter().enumerate().filter(|(k, _)| !c.members.contains(k)).map(|(_, x)| x.clone()).collect();
    out.push((id, Rat::one(), c.sum.clone()));
    out
}

pub fn sentence_reasons_mut(s: &mut Sentence, f: &mut dyn FnMut(&mut Reason)) {
    fn go(r: &mut Reason, f: &mut dyn FnMut(&mut Reason)) {
        f(r);
        if let Reason::Fact { because, .. } = r {
            for x in because.iter_mut() {
                go(x, f);
            }
        }
    }
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => {
            for l in links.iter_mut() {
                for r in l.reasons.iter_mut() {
                    go(r, f);
                }
            }
        }
        Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => {
            for r in reasons.iter_mut() {
                go(r, f);
            }
        }
        Sentence::Raw { .. } => {}
    }
}

fn shift_pending(ss: &mut [Sentence], off: usize) {
    for s in ss.iter_mut() {
        sentence_reasons_mut(s, &mut |r| {
            if let Reason::Lemma { block: PENDING_BLOCK, sentence, .. } = r {
                *sentence += off as u16;
            }
        });
    }
}

fn bind_pending(ss: &mut [Sentence], id: u16) {
    for s in ss.iter_mut() {
        sentence_reasons_mut(s, &mut |r| {
            if let Reason::Lemma { block, .. } = r {
                if *block == PENDING_BLOCK {
                    *block = id;
                }
            }
        });
    }
}

fn path_from_groups(split: usize, l: &LinComb, items: &[(usize, Rat, LinComb)], groups: &[Vec<usize>]) -> Path {
    let mut nodes = vec![l.clone()];
    let mut links: Vec<Group> = Vec::new();
    let mut cur = l.clone();
    for g in groups {
        let mut grp: Group = Vec::new();
        for &k in g {
            cur = LinComb::combine(&cur, &items[k].2, &-&items[k].1);
            grp.push((items[k].0, items[k].1.clone()));
        }
        nodes.push(cur.clone());
        links.push(grp);
    }
    Path { split, nodes, links, weight: 0 }
}

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
    pub requires: Vec<FactId>,
}

pub struct Writer<'c, 'a> {
    pub cx: &'c Ctx<'a>,
    pub atoms: Vec<Atom>,
    pub nodes: BTreeMap<FactId, Node>,
    pub claim_cost: BTreeSet<FactId>,
    pub bfs_budget: usize,
    pub span_cache: std::cell::RefCell<BTreeMap<(FactId, Option<FactId>), Vec<super::cert::Basis>>>,
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
        Writer { cx, atoms, nodes: BTreeMap::new(), claim_cost: BTreeSet::new(), bfs_budget: 20_000, span_cache: std::cell::RefCell::new(BTreeMap::new()) }
    }

    pub fn certify(&self, tb: Table, target: &LinComb, h: FactId, focus: &BTreeSet<PointId>, exclude: Option<FactId>) -> Option<(Vec<(usize, Rat)>, i64)> {
        let mut banned: BTreeSet<usize> = BTreeSet::new();
        let mut best = self.certify_inner(tb, target, h, focus, exclude, &banned)?;
        for _ in 0..6 {
            if self.cx.timed_out() {
                break;
            }
            let worst = best
                .0
                .iter()
                .map(|(i, _)| *i)
                .filter(|&i| self.atoms[i].cost >= 3 && !banned.contains(&i))
                .max_by_key(|&i| (self.atoms[i].cost, i));
            let Some(w) = worst else { break };
            banned.insert(w);
            match self.certify_inner(tb, target, h, focus, exclude, &banned) {
                Some(c) if c.1 < best.1 => best = c,
                _ => {
                    banned.remove(&w);
                    break;
                }
            }
        }
        Some(best)
    }

    fn certify_inner(&self, tb: Table, target: &LinComb, h: FactId, focus: &BTreeSet<PointId>, exclude: Option<FactId>, banned: &BTreeSet<usize>) -> Option<(Vec<(usize, Rat)>, i64)> {
        let cx = self.cx;
        let mut idx: Vec<(u32, usize)> = self
            .atoms
            .iter()
            .enumerate()
            .filter(|(i, a)| a.table == tb && a.admissible(h) && (exclude.is_none() || a.fact() != exclude) && !banned.contains(i))
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

    pub fn in_admissible_span(&self, tb: Table, row: &LinComb, h: FactId, exclude: Option<FactId>) -> bool {
        let key = (h, exclude);
        let mut cache = self.span_cache.borrow_mut();
        let bases = cache.entry(key).or_insert_with(|| {
            let mut v: Vec<super::cert::Basis> = Table::ALL.iter().map(|t| super::cert::Basis::new(&self.cx.piv[t.idx()], false)).collect();
            for (i, a) in self.atoms.iter().enumerate() {
                if a.admissible(h) && (exclude.is_none() || a.fact() != exclude) {
                    v[a.table.idx()].insert(i as u32, &a.row);
                }
            }
            v
        });
        bases[tb.idx()].contains(row)
    }

    fn focus_of(&self, f: FactId) -> BTreeSet<PointId> {
        if f == GOAL {
            self.cx.goal.points.iter().copied().collect()
        } else {
            fact_points(self.cx, f).into_iter().collect()
        }
    }

    fn certify_obls(&self, obls: &[Obl], h: FactId, focus: &BTreeSet<PointId>, exclude: Option<FactId>) -> Option<(&'static str, Vec<Part>, i64, Vec<FactId>)> {
        let mut best: Option<(&'static str, Vec<Part>, i64, Vec<FactId>)> = None;
        for o in obls {
            if self.cx.timed_out() {
                break;
            }
            let mut parts = Vec::new();
            let mut total = 0i64;
            let mut ok = true;
            for t in &o.targets {
                let mut best_t: Option<(Vec<(usize, Rat)>, i64, Target)> = None;
                let forms: Vec<&Target> = std::iter::once(t).chain(t.alts.iter()).collect();
                for form in forms {
                    if !self.in_admissible_span(form.table, &form.row, h, exclude) {
                        continue;
                    }
                    if let Some((s, c)) = self.certify(form.table, &form.row, h, focus, exclude) {
                        if best_t.as_ref().is_none_or(|b| c < b.1) {
                            let mut chosen = form.clone();
                            chosen.alts.clear();
                            best_t = Some((s, c, chosen));
                        }
                    }
                }
                match best_t {
                    Some((s, c, chosen)) => {
                        total += c;
                        parts.push(Part { target: chosen, support: s });
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            let total = total + 4 * o.requires.iter().filter(|&&r| !self.cx.is_hyp(r)).count() as i64;
            if ok && best.as_ref().is_none_or(|b| total < b.2) {
                best = Some((o.label, parts, total, o.requires.clone()));
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
            requires: Vec::new(),
        };
        let obls = fact_obligations(cx, f).map(|mut o| {
            if let ER::Concyclic(p) | ER::Collinear(p) = &cx.t.facts[f as usize].reason {
                o.extend(self.cover(p, matches!(cx.t.facts[f as usize].reason, ER::Concyclic(_)), f, Some(f)));
            }
            o
        });
        match obls {
            Some(obls) if obls.is_empty() && matches!(k, Kind::Formula(TheoremKey::LawOfSines)) => {
                node.parts = Some(Vec::new());
                node.label = "identity";
            }
            Some(obls) => {
                if let Some((label, parts, cost, req)) = self.certify_obls(&obls, f, &focus, Some(f)) {
                    node.deps = self.deps_of(&parts);
                    for &r in &req {
                        for s in cx.fact_sources(r) {
                            node.deps.insert(cx.displayable(s));
                        }
                    }
                    node.requires = req;
                    node.label = label;
                    node.parts = Some(parts);
                    node.cost = cost;
                }
            }
            None => {}
        }
        if node.parts.is_none() && matches!(k, Kind::Theorem(_) | Kind::Formula(_) | Kind::Merge) {
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
        let obls = self.goal_obls(goal_fact);
        if std::env::var_os("HP_DEBUG").is_some() {
            eprintln!("goal {:?} h {h} obligations {}", cx.goal, obls.len());
            for o in &obls {
                for t in &o.targets {
                    let all = self.certify(t.table, &t.row, FactId::MAX - 2, &BTreeSet::new(), None).is_some();
                    let at_h = self.certify(t.table, &t.row, h, &BTreeSet::new(), None).is_some();
                    let excl = self.certify(t.table, &t.row, h, &BTreeSet::new(), goal_fact).is_some();
                    eprintln!("  target {:?} all {all} at_h {at_h} excl {excl}", t.stmt);
                    if let Some(g) = goal_fact {
                        for tb in Table::ALL {
                            let mut bc = super::cert::Basis::new(&cx.piv[tb.idx()], false);
                            let mut ba = super::cert::Basis::new(&cx.piv[tb.idx()], false);
                            for (i, (f, r)) in cx.t.rows[tb.idx()].iter().enumerate() {
                                if *f < g {
                                    ba.insert(i as u32, r);
                                    if cx.in_cl[*f as usize] {
                                        bc.insert(i as u32, r);
                                    }
                                }
                            }
                            for r in cx.t.rows_of(tb, g) {
                                eprintln!("    row of goal fact in {:?}: closure-span {} all-span {}", tb, bc.contains(r), ba.contains(r));
                            }
                        }
                        eprintln!("    premises {:?}", cx.t.facts[g as usize].premises);
                    }
                }
            }
        }
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
            requires: Vec::new(),
        };
        if let Some((label, parts, cost, req)) = self.certify_obls(&obls, h, &focus, goal_fact) {
            node.deps = self.deps_of(&parts);
            for &r in &req {
                for s in cx.fact_sources(r) {
                    node.deps.insert(cx.displayable(s));
                }
            }
            node.requires = req;
            node.label = label;
            node.parts = Some(parts);
            node.cost = cost;
        }
        node
    }

    fn goal_obls(&self, goal_fact: Option<FactId>) -> Vec<Obl> {
        let cx = self.cx;
        let mut obls = goal_obligations(cx, &cx.goal);
        let Some(gf) = goal_fact else { return obls };
        let cyclic = cx.goal.name == "cyclic";
        if cyclic || cx.goal.name == "coll" {
            obls.extend(self.cover(&cx.goal.points, cyclic, gf, Some(gf)));
        }
        let gp: BTreeSet<PointId> = cx.goal.points.iter().copied().collect();
        let fp: BTreeSet<PointId> = fact_points(cx, gf).into_iter().collect();
        let merged = matches!((cx.goal.name.as_str(), &cx.t.facts[gf as usize].reason), ("coll", ER::Collinear(_)) | ("cyclic", ER::Concyclic(_)));
        if merged && fp.len() > gp.len() && gp.is_subset(&fp) {
            let fpts: Vec<PointId> = fact_points(cx, gf);
            let cyclic = matches!(cx.t.facts[gf as usize].reason, ER::Concyclic(_));
            let mut extra = fact_obligations(cx, gf).unwrap_or_default();
            extra.extend(self.cover(&fpts, cyclic, gf, Some(gf)));
            for mut o in extra {
                o.label = MERGED;
                obls.push(o);
            }
        }
        obls
    }

    fn cover(&self, pts: &[PointId], cyclic: bool, h: FactId, exclude: Option<FactId>) -> Vec<Obl> {
        let cx = self.cx;
        let k = if cyclic { 4 } else { 3 };
        if pts.len() <= k || pts.len() > 8 {
            return Vec::new();
        }
        let subsets: Vec<Vec<PointId>> = if cyclic { subsets4(pts).into_iter().map(|q| q.to_vec()).collect() } else { subsets3(pts) };
        let mut usable: Vec<(Vec<PointId>, Vec<Target>)> = Vec::new();
        for q in subsets {
            let forms: Vec<Target> = if cyclic {
                cyc_forms(cx, [q[0], q[1], q[2], q[3]])
            } else {
                coll_targets(cx, &q, None).map(|(t, _)| t.into_iter().flat_map(|t| std::iter::once(t.clone()).chain(t.alts.clone())).collect()).unwrap_or_default()
            };
            let good: Vec<Target> = forms
                .into_iter()
                .filter(|t| self.in_admissible_span(t.table, &t.row, h, exclude))
                .map(|mut t| {
                    t.alts.clear();
                    t
                })
                .collect();
            if !good.is_empty() {
                usable.push((q, good));
            }
        }
        let Some(first) = usable.first() else { return Vec::new() };
        let mut have: BTreeSet<PointId> = first.0.iter().copied().collect();
        let mut chosen: Vec<usize> = vec![0];
        while have.len() < pts.len() {
            let next = usable.iter().enumerate().find(|(i, (q, _))| !chosen.contains(i) && q.iter().filter(|p| !have.contains(p)).count() == 1);
            let Some((i, (q, _))) = next else { return Vec::new() };
            have.extend(q.iter().copied());
            chosen.push(i);
        }
        let targets: Vec<Target> = chosen
            .into_iter()
            .map(|i| {
                let mut forms = usable[i].1.clone();
                let mut first = forms.remove(0);
                first.alts = forms;
                first
            })
            .collect();
        vec![Obl { label: if cyclic { "inscribed" } else { "coll" }, targets, requires: vec![] }]
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
            let obls = if k == GOAL { self.goal_obls(n.extra.first().copied()) } else { fact_obligations(self.cx, k).unwrap_or_default() };
            let focus = self.focus_of(k);
            let exclude = if k == GOAL { n.extra.first().copied() } else { Some(k) };
            if let Some((label, parts, cost, req)) = self.certify_obls(&obls, n.horizon, &focus, exclude) {
                let mut deps = self.deps_of(&parts);
                for &r in &req {
                    for s in self.cx.fact_sources(r) {
                        deps.insert(self.cx.displayable(s));
                    }
                }
                let n = self.nodes.get_mut(&k).unwrap();
                n.label = label;
                n.parts = Some(parts);
                n.cost = cost;
                n.deps = deps;
                n.requires = req;
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
                    Kind::Theorem(_) if matches!(n.stmt, Stmt::Formula { .. }) => {
                        if u <= 1 {
                            Role::Inline
                        } else {
                            Role::Step
                        }
                    }
                    Kind::Isosceles | Kind::Length => {
                        if u <= 1 {
                            Role::Inline
                        } else {
                            Role::Step
                        }
                    }
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
                        } else if only_goal {
                            Role::Inline
                        } else {
                            Role::Step
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
        if claims.len() > MAX_CLAIMS {
            let score = |c: &FactId| -> (usize, i64, std::cmp::Reverse<FactId>) {
                let n = &self.nodes[c];
                (users.get(c).map(|s| s.len()).unwrap_or(0), n.cost, std::cmp::Reverse(*c))
            };
            let mut ranked = claims.clone();
            ranked.sort_by_key(|c| std::cmp::Reverse(score(c)));
            for c in ranked.into_iter().skip(MAX_CLAIMS) {
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
    pub force_undirected: bool,
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

fn subsets3(args: &[PointId]) -> Vec<Vec<PointId>> {
    let mut out = Vec::new();
    for i in 0..args.len() {
        for j in i + 1..args.len() {
            for k in j + 1..args.len() {
                out.push(vec![args[i], args[j], args[k]]);
            }
        }
    }
    out
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
        let cx = self.w.cx;
        let mut reasons: Vec<Reason> = Vec::new();
        let mut terms: Vec<Term> = Vec::new();
        let mut extra: Vec<Reason> = Vec::new();
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
            if let AtomSrc::Human(_) = a.src {
                for s in &a.sources {
                    if let Some(r) = self.inline.get(&cx.displayable(*s)) {
                        if !extra.contains(r) {
                            extra.push(r.clone());
                        }
                    }
                }
            }
        }
        for r in extra {
            if !reasons.contains(&r) {
                reasons.push(r);
            }
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
                    self.allowed.contains(&d) || cx.class.get(d as usize).is_some_and(|c| matches!(c, FactClass::Hyp | FactClass::HypReg | FactClass::SilentHyp))
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
        if target.table == Table::Ratio && support.iter().any(|(i, _)| self.is_trig(*i)) {
            if let Some((s, n)) = self.trig_lemmas(target, h, then.clone(), focus) {
                return (s, n, false);
            }
        }
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
        let path = if matches!(target.table, Table::Angle | Table::Ratio) {
            bfs(cx, &self.w.atoms, &eligible, target.table, &fix_exact(cx, target, &splits), &penalty, self.w.bfs_budget)
        } else {
            None
        };
        if let Some(p) = path {
            if p.links.len() <= 8 && !p.links.is_empty() {
                let r = self.path_sentences(target, &p, h, then.clone(), focus, &[]);
                if std::env::var_os("HP_BFS").is_some() {
                    eprintln!("path links {} sentences {}", p.links.len(), r.is_some());
                }
                if let Some(s) = r {
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
        let items: Vec<(usize, Rat, LinComb)> = items
            .into_iter()
            .flat_map(|(i, lam, q)| {
                if target.table == Table::Angle && !lam.is_integer() && lam.abs() > Rat::one() {
                    let half = if lam.is_negative() { Rat::new(-1, 2) } else { Rat::new(1, 2) };
                    vec![(i, &lam - &half, q.clone()), (i, half, q)]
                } else {
                    vec![(i, lam, q)]
                }
            })
            .collect();
        let ex = if matches!(target.table, Table::Angle | Table::Ratio) { fix_exact(cx, target, &splits) } else { Vec::new() };
        if std::env::var_os("HP_ITEMS").is_some() {
            let names = |p: PointId| super::text::disp(cx.t.name(p));
            let nm = super::text::FnNames(&names);
            eprintln!("CHAIN {} (h {h})", super::text::stmt(&nm, &target.stmt));
            for (i, lam, _) in &items {
                let (r, _) = self.atom_reason(&self.w.atoms[*i], h, false);
                eprintln!("  item {i} lam {lam} cost {} eff-src {}: {}", self.w.atoms[*i].cost, self.w.atoms[*i].sources.len(), super::text::reason(&nm, &r, &BTreeMap::new()));
            }
        }
        let mut budget = ORDER_BUDGET;
        let mut best: Option<(i64, usize, Vec<Vec<usize>>)> = None;
        for (si, (l, r)) in ex.iter().enumerate() {
            if let Some((groups, cost)) = order_items(cx, target.table, l, r, &items, &mut budget, DIRECT_CAP) {
                let cost = cost + long_penalty(groups.len());
                if best.as_ref().is_none_or(|b| cost < b.0) {
                    best = Some((cost, si, groups));
                }
            }
        }
        let clean = best.as_ref().is_some_and(|(c, _, g)| *c <= 10 * g.len() as i64 + 6);
        let mut split: Option<(usize, LemmaSplit)> = None;
        if !clean {
            for (si, (l, r)) in ex.iter().enumerate() {
                if let Some(ls) = self.lemma_plan(target.table, l, r, &items, focus, &mut budget, false) {
                    if split.as_ref().is_none_or(|s| ls.cost < s.1.cost) {
                        split = Some((si, ls));
                    }
                }
            }
        }
        let full = |s: &Option<(usize, LemmaSplit)>| s.as_ref().is_some_and(|x| !x.1.groups.is_empty());
        let use_split = match (&best, &split) {
            (_, None) => false,
            (None, Some(_)) => full(&split),
            (Some(b), Some(s)) => full(&split) && s.1.cost < b.0,
        };
        if use_split {
            let (si, ls) = split.take().unwrap();
            if let Some(r) = self.build_split(target, si, &ex[si].0, &ls, h, then.clone(), focus) {
                return r;
            }
        }
        if let Some((_, si, groups)) = best {
            let p = path_from_groups(si, &ex[si].0, &items, &groups);
            if let Some(s) = self.path_sentences(target, &p, h, then.clone(), focus, &[]) {
                return (s, p.links.len(), false);
            }
        }
        let mut partial: Vec<(usize, LemmaSplit)> = split.into_iter().collect();
        if target.table == Table::Angle {
            let mut halves: Option<(i64, usize, Vec<Vec<usize>>)> = None;
            for (si, (l, r)) in ex.iter().enumerate() {
                if let Some((groups, cost)) = order_items_mode(cx, target.table, l, r, &items, &mut budget, DIRECT_CAP, true) {
                    if halves.as_ref().is_none_or(|b| cost < b.0) {
                        halves = Some((cost, si, groups));
                    }
                }
            }
            if let Some((_, si, groups)) = halves {
                let p = path_from_groups(si, &ex[si].0, &items, &groups);
                if let Some(s) = self.path_sentences(target, &p, h, then.clone(), focus, &[]) {
                    return (s, p.links.len(), false);
                }
            }
            for (si, (l, r)) in ex.iter().enumerate() {
                if let Some(ls) = self.lemma_plan(target.table, l, r, &items, focus, &mut budget, true) {
                    if !ls.groups.is_empty() {
                        if let Some(r) = self.build_split(target, si, &ex[si].0, &ls, h, then.clone(), focus) {
                            return r;
                        }
                    } else {
                        partial.push((si, ls));
                    }
                }
            }
        }
        partial.sort_by_key(|(_, ls)| (ls.reduced.len(), ls.plans.len()));
        for (si, ls) in &partial {
            if let Some(r) = self.build_split(target, *si, &ex[*si].0, ls, h, then.clone(), focus) {
                return r;
            }
        }
        if std::env::var_os("HP_POOL").is_some() {
            eprintln!("POOLED {:?} table {:?} budget left {budget}", target.stmt, target.table);
            for (si, (l, r)) in ex.iter().enumerate() {
                eprintln!("  split {si}: L {:?} R {:?} shapeL {:?} shapeR {:?}", l, r, super::chain::angle_shape(l), super::chain::angle_shape(r));
            }
            for (i, lam, q) in &items {
                let a = &self.w.atoms[*i];
                eprintln!("  item {i} {:?} args {:?} lam {lam} q {:?}", a.src, a.args, q);
            }
            for (i, lam) in support {
                if !items.iter().any(|x| x.0 == *i) {
                    eprintln!("  dropped support {i} {:?} lam {lam} cost {} q {:?}", self.w.atoms[*i].src, self.w.atoms[*i].cost, self.w.atoms[*i].q);
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
        if n <= SHORT_REASONS {
            return (vec![Sentence::Because { stmt, reasons, combination }], 1, false);
        }
        (vec![Sentence::Pooled { stmt, reasons, combination }], n, true)
    }

    fn trig_lemmas(&mut self, target: &Target, h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>) -> Option<(Vec<Sentence>, usize)> {
        let cx = self.w.cx;
        let t = cx.t;
        let (s1, s2, k) = match &target.stmt {
            Stmt::RatioConst { s1, s2, value } => (*s1, *s2, value.clone()),
            Stmt::Cong { s1, s2 } => (*s1, *s2, Rat::one()),
            _ => return None,
        };
        let (o, tri) = cx.circumcentre()?;
        let rv = t.dm(o, tri[0])?;
        let rlen = t.dist(o, tri[0]);
        let mut vars: Vec<VarId> = t.sines.iter().map(|s| t.sine_canon(s.var)).collect();
        vars.sort_unstable();
        vars.dedup();
        let main = |v: VarId| t.sines.iter().find(|s| s.var == v).is_some_and(|s| [s.v, s.p, s.q].iter().all(|p| tri.contains(p)));
        vars.sort_by_key(|&v| (!main(v), v));
        for &v in &vars {
            let sval = t.values[Table::Ratio.idx()].get(v as usize).copied().unwrap_or(0.0);
            if sval <= 1e-9 {
                continue;
            }
            let mut lem: Vec<((PointId, PointId), Rat, LinComb)> = Vec::new();
            for s in [s1, s2] {
                let Some(q) = super::classify::small_rational(t.dist(s.0, s.1) / (rlen * sval)) else { break };
                if q.denom_i64().is_none_or(|d| d > 2) || q.numer_i64().is_none_or(|n| n > 4) {
                    break;
                }
                let (Some(ds), Some(c)) = (t.dm(s.0, s.1), t.prime_const(&q)) else { break };
                let row = &(&(&ds - &rv) - &LinComb::singleton(v, Rat::one())) - &c;
                if !t.holds(Table::Ratio, &row) || !self.w.in_admissible_span(Table::Ratio, &row, h, None) {
                    break;
                }
                lem.push((s, q, row));
            }
            if lem.len() != 2 || &lem[0].1 / &lem[1].1 != k {
                continue;
            }
            let trig = match ratio_expr(cx, &LinComb::singleton(v, Rat::one())) {
                Expr::Prod { factors } if factors.len() == 1 => factors[0].0.clone(),
                _ => continue,
            };
            let mut out: Vec<Sentence> = Vec::new();
            let mut stmts: Vec<Stmt> = Vec::new();
            let mut ok = true;
            for (s, q, row) in &lem {
                let Some((support, _)) = self.w.certify(Table::Ratio, row, h, focus, None) else {
                    ok = false;
                    break;
                };
                let mut f: Vec<(Expr, i32)> = Vec::new();
                if !q.is_one() {
                    f.push((Expr::Num { value: q.clone() }, 1));
                }
                f.push((Expr::Seg { a: o, b: tri[0] }, 1));
                f.push((trig.clone(), 1));
                let l = Expr::Seg { a: s.0, b: s.1 };
                let r = Expr::Prod { factors: f };
                let (Some((_, lraw)), Some((_, rraw))) = (super::expr::eval(t, &l), super::expr::eval(t, &r)) else {
                    ok = false;
                    break;
                };
                let stmt = Stmt::Eq { lhs: l.clone(), rhs: r.clone() };
                let fake = Target { table: Table::Ratio, row: row.clone(), splits: vec![super::classify::Split { l, r, lraw, rraw }], stmt: stmt.clone(), alts: Vec::new() };
                match self.computation(&fake, &support, h) {
                    Some(c) => out.push(c),
                    None => {
                        ok = false;
                        break;
                    }
                }
                stmts.push(stmt);
            }
            if !ok {
                continue;
            }
            let reasons: Vec<Reason> = stmts.iter().enumerate().map(|(i, s)| Reason::Lemma { stmt: s.clone(), block: PENDING_BLOCK, sentence: i as u16 }).collect();
            let combination = vec![Term { reason: 0, row: 0, coef: Rat::one() }, Term { reason: 1, row: 0, coef: Rat::from_int(-1) }];
            out.push(Sentence::Because { stmt: then.unwrap_or_else(|| target.stmt.clone()), reasons, combination });
            let n = out.iter().map(|s| if let Sentence::Computation { links, .. } = s { links.len() } else { 1 }).sum();
            return Some((out, n));
        }
        None
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
        let canon = |c: &LinComb| cx.t.canon_ratio(c);
        let mut items: Vec<(usize, Rat)> = support.iter().filter(|(i, _)| self.w.atoms[*i].cost > 0 && !canon(&self.w.atoms[*i].row).is_zero()).cloned().collect();
        let mut cur = canon(&split.lraw);
        let lhs = |c: &LinComb| c.terms.iter().filter(|(v, _)| cx.piv[1].get(*v as usize).copied().unwrap_or(true)).count();
        let mut seq: Vec<(usize, Rat, LinComb)> = Vec::new();
        while !items.is_empty() {
            let (k, _) = items
                .iter()
                .enumerate()
                .map(|(k, (i, l))| {
                    let nx = LinComb::combine(&cur, &canon(&self.w.atoms[*i].row), &-l);
                    (k, (lhs(&nx), *i))
                })
                .min_by_key(|x| x.1)?;
            let (i, l) = items.remove(k);
            cur = LinComb::combine(&cur, &canon(&self.w.atoms[i].row), &-&l);
            seq.push((i, l, cur.clone()));
        }
        if !canon(&(&cur - &split.rraw)).is_zero() {
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

    fn group_reasons(&self, group: &[(usize, Rat)], h: FactId, lem: &[LemmaRef]) -> (Vec<Reason>, Vec<Term>) {
        let base = self.w.atoms.len();
        let atoms: Vec<(usize, Rat)> = group.iter().filter(|(i, _)| *i < base).cloned().collect();
        let (mut rs, mut cs) = self.reasons_for(&atoms, h, false);
        for (i, lam) in group.iter().filter(|(i, _)| *i >= base) {
            let lr = &lem[*i - base];
            rs.push(Reason::Lemma { stmt: lr.stmt.clone(), block: PENDING_BLOCK, sentence: lr.sentence as u16 });
            cs.push(Term { reason: (rs.len() - 1) as u16, row: 0, coef: lam * &lr.scale });
        }
        (rs, cs)
    }


    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn build_split(&mut self, target: &Target, si: usize, l: &LinComb, ls: &LemmaSplit, h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>) -> Option<(Vec<Sentence>, usize, bool)> {
        let saved = (self.force_undirected, self.as_drawn);
        self.force_undirected = ls.halves;
        let r = self.lemma_build(target, si, l, &ls.reduced, &ls.groups, &ls.plans, h, then, focus);
        self.force_undirected = saved.0;
        match r {
            Some((s, n)) => {
                let pooled = s.iter().any(|x| matches!(x, Sentence::Pooled { .. }));
                if ls.halves {
                    self.as_drawn = true;
                }
                Some((s, n, pooled))
            }
            None => {
                self.as_drawn = saved.1;
                None
            }
        }
    }

    fn lemma_plan(&self, table: Table, l: &LinComb, r: &LinComb, items: &[(usize, Rat, LinComb)], focus: &BTreeSet<PointId>, budget: &mut usize, halves: bool) -> Option<LemmaSplit> {
        let cx = self.w.cx;
        if !matches!(table, Table::Angle | Table::Ratio) || items.len() < 3 {
            return None;
        }
        let base = self.w.atoms.len();
        let mut work: Vec<(usize, Rat, LinComb)> = items.to_vec();
        let mut plans: Vec<LemmaPlan> = Vec::new();
        let mut acc = 0i64;
        for _depth in 0..8 {
            if *budget == 0 || cx.timed_out() {
                break;
            }
            let mut cands = self.lemma_candidates(table, &work, focus, budget, halves);
            if cands.is_empty() {
                break;
            }
            cands.sort_by(|a, b| a.cost.cmp(&b.cost).then(a.members.cmp(&b.members)));
            if std::env::var_os("HP_LEMMA").is_some() {
                let names = |p: PointId| super::text::disp(cx.t.name(p));
                let nm = super::text::FnNames(&names);
                eprintln!("lemma candidates at depth {} (work {}):", plans.len(), work.len());
                for c in cands.iter().take(30) {
                    eprintln!("  cost {} members {:?}: {} = {}", c.cost, c.members, super::text::expr(&nm, &c.lhs), super::text::expr(&nm, &c.rhs));
                }
            }
            let next_id = base + plans.len();
            let mut best: Option<LemmaSplit> = None;
            let mut hits = 0;
            for c in cands.iter().take(40) {
                if hits >= 8 {
                    break;
                }
                let reduced = reduce_items(&work, c, next_id);
                if let Some((groups, mc)) = order_items_mode(cx, table, l, r, &reduced, budget, MAIN_CAP, halves) {
                    hits += 1;
                    let total = acc + c.cost + mc + long_penalty(groups.len()) + SENTENCE_COST;
                    if best.as_ref().is_none_or(|b| total < b.cost) {
                        let mut all = plans.clone();
                        all.push(c.plan(&work));
                        best = Some(LemmaSplit { cost: total, plans: all, reduced, groups, halves });
                    }
                }
            }
            if best.is_some() {
                return best;
            }
            let Some(pick) = cands.iter().min_by_key(|c| (c.cost - 14 * c.members.len() as i64, c.members.clone())).cloned() else { break };
            acc += pick.cost + SENTENCE_COST;
            plans.push(pick.plan(&work));
            work = reduce_items(&work, &pick, next_id);
            if work.len() < 2 {
                break;
            }
        }
        if plans.is_empty() {
            return None;
        }
        Some(LemmaSplit { cost: i64::MAX / 4, plans, reduced: work, groups: Vec::new(), halves })
    }

    fn lemma_candidates(&self, table: Table, work: &[(usize, Rat, LinComb)], focus: &BTreeSet<PointId>, budget: &mut usize, halves: bool) -> Vec<LemmaCand> {
        let cx = self.w.cx;
        let n = work.len();
        let max = if n > 18 { 3 } else if n > 12 { 4 } else { 5 }.min(n - 1);
        let key = |v: VarId| if table == Table::Angle { v >= QBASE } else { cx.piv[1].get(v as usize).copied().unwrap_or(true) };
        let iv: Vec<Vec<VarId>> = work.iter().map(|x| x.2.terms.iter().filter(|(v, _)| key(*v)).map(|(v, _)| *v).collect()).collect();
        let mut seen: BTreeSet<u32> = BTreeSet::new();
        let mut frontier: Vec<u32> = (0..n).map(|k| 1u32 << k).collect();
        let mut out: Vec<LemmaCand> = Vec::new();
        for _size in 2..=max {
            let mut next: Vec<u32> = Vec::new();
            for &m in &frontier {
                let mv: BTreeSet<VarId> = (0..n).filter(|&k| m & (1 << k) != 0).flat_map(|k| iv[k].iter().copied()).collect();
                for k in 0..n {
                    if m & (1 << k) != 0 || !iv[k].iter().any(|v| mv.contains(v)) {
                        continue;
                    }
                    let m2 = m | (1 << k);
                    if seen.insert(m2) {
                        next.push(m2);
                    }
                }
            }
            if next.len() > 4000 {
                next.truncate(4000);
            }
            for &m in &next {
                let members: Vec<usize> = (0..n).filter(|&k| m & (1 << k) != 0).collect();
                let mut s = LinComb::zero();
                for &k in &members {
                    s.iadd_mul(&work[k].2, &work[k].1);
                }
                let ends: Vec<(LinComb, LinComb)> = if table == Table::Angle {
                    if classes(&s).is_empty() || (!halves && !integral_node(&s)) {
                        continue;
                    }
                    let mut v = Vec::new();
                    for ts in decomps(&s) {
                        if ts.is_empty() || (!halves && ts.iter().any(|t| !t.0.is_integer())) {
                            continue;
                        }
                        let starts: Vec<LinComb> = if ts.len() == 2 { vec![term_comb(&ts[0]), term_comb(&ts[1])] } else { vec![term_comb(&ts[0])] };
                        for a in starts {
                            let end = LinComb::combine(&a, &s, &Rat::from_int(-1));
                            if single_angle(&a).is_some() && single_angle(&end).is_some() {
                                v.push((a, end));
                            }
                        }
                    }
                    v
                } else {
                    ratio_splits(cx, &s)
                };
                if ends.is_empty() {
                    continue;
                }
                let sub: Vec<(usize, Rat, LinComb)> = members.iter().map(|&k| work[k].clone()).collect();
                for (a, end) in ends {
                    let shown = if table == Table::Angle {
                        (display_node(cx, &a, !halves, &[], focus), display_node(cx, &end, !halves, &[], focus))
                    } else {
                        (Some(ratio_expr(cx, &a)), Some(ratio_expr(cx, &end)))
                    };
                    let (Some(ea), Some(ee)) = shown else { continue };
                    if let Some((groups, c)) = order_items_mode(cx, table, &a, &end, &sub, budget, SUB_CAP, halves) {
                        let bad = expr_badness(&ea) + expr_badness(&ee);
                        let cost = c + long_penalty(groups.len()) + 3 * bad;
                        out.push(LemmaCand { members: members.clone(), sum: s.clone(), start: a.clone(), end: end.clone(), groups, cost, lhs: ea.clone(), rhs: ee.clone() });
                    }
                    if *budget == 0 {
                        return out;
                    }
                }
            }
            frontier = next;
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn lemma_build(
        &mut self,
        target: &Target,
        si: usize,
        l: &LinComb,
        reduced: &[(usize, Rat, LinComb)],
        groups: &[Vec<usize>],
        plans: &[LemmaPlan],
        h: FactId,
        then: Option<Stmt>,
        focus: &BTreeSet<PointId>,
    ) -> Option<(Vec<Sentence>, usize)> {
        let mut out: Vec<Sentence> = Vec::new();
        let mut refs: Vec<LemmaRef> = Vec::new();
        let mut links = 0;
        for plan in plans {
            let stmt = if target.table == Table::Angle { Stmt::EqAngle { lhs: plan.lhs.clone(), rhs: plan.rhs.clone() } } else { Stmt::Eq { lhs: plan.lhs.clone(), rhs: plan.rhs.clone() } };
            let (_, lraw) = super::expr::eval(self.w.cx.t, &plan.lhs)?;
            let (_, rraw) = super::expr::eval(self.w.cx.t, &plan.rhs)?;
            let fake = Target { table: target.table, row: plan.sum.clone(), splits: vec![super::classify::Split { l: plan.lhs.clone(), r: plan.rhs.clone(), lraw, rraw }], stmt: stmt.clone(), alts: Vec::new() };
            let p = path_from_groups(0, &plan.start, &plan.items, &plan.groups);
            let s = self.path_sentences(&fake, &p, h, None, focus, &refs)?;
            if s.len() != 1 {
                return None;
            }
            let (stmt, scale) = match &s[0] {
                Sentence::Chain { terms, .. } => {
                    let (a, b) = (terms[0].clone(), terms[terms.len() - 1].clone());
                    let st = if target.table == Table::Angle { Stmt::EqAngle { lhs: a.clone(), rhs: b.clone() } } else { Stmt::Eq { lhs: a.clone(), rhs: b.clone() } };
                    let (_, ea) = super::expr::eval(self.w.cx.t, &a)?;
                    let (_, eb) = super::expr::eval(self.w.cx.t, &b)?;
                    let strip = |c: LinComb| -> LinComb {
                        let mut x = if target.table == Table::Angle { self.w.cx.quot.q(&c) } else { self.w.cx.t.canon_ratio(&c) };
                        x.terms.retain(|(v, _)| *v != ANGLE_UNIT);
                        x
                    };
                    let mut want = plan.sum.clone();
                    want.terms.retain(|(v, _)| *v != ANGLE_UNIT);
                    let got = strip(&ea - &eb);
                    let scale = match (want.terms.first(), got.terms.first()) {
                        (Some((v, k)), Some((w, g))) if v == w && !g.is_zero() => k / g,
                        _ => return None,
                    };
                    let mut check = got.clone();
                    check.mul_assign_scalar(&scale);
                    if check != want {
                        return None;
                    }
                    (st, scale)
                }
                Sentence::Because { .. } => (stmt, Rat::one()),
                _ => return None,
            };
            links += p.links.len();
            let mut pts: Vec<PointId> = super::view::expr_points(&plan.lhs);
            pts.extend(super::view::expr_points(&plan.rhs));
            pts.sort_unstable();
            pts.dedup();
            refs.push(LemmaRef { stmt, sentence: out.len(), pts, scale });
            out.extend(s);
        }
        if groups.is_empty() {
            let all: Vec<(usize, Rat)> = reduced.iter().map(|x| (x.0, x.1.clone())).collect();
            let base = self.w.atoms.len();
            let atoms: Vec<(usize, Rat)> = all.iter().filter(|(i, _)| *i < base).cloned().collect();
            let (mut reasons, mut combination) = self.reasons_for(&atoms, h, true);
            for (i, lam) in all.iter().filter(|(i, _)| *i >= base) {
                let lr = &refs[*i - base];
                reasons.push(Reason::Lemma { stmt: lr.stmt.clone(), block: PENDING_BLOCK, sentence: lr.sentence as u16 });
                combination.push(Term { reason: (reasons.len() - 1) as u16, row: 0, coef: lam * &lr.scale });
            }
            let stmt = then.unwrap_or_else(|| target.stmt.clone());
            if reasons.len() <= SHORT_REASONS {
                out.push(Sentence::Because { stmt, reasons, combination });
            } else {
                out.push(Sentence::Pooled { stmt, reasons, combination });
            }
            return Some((out, links + 1));
        }
        let p = path_from_groups(si, l, reduced, groups);
        let s = self.path_sentences(target, &p, h, then, focus, &refs)?;
        links += p.links.len();
        out.extend(s);
        Some((out, links))
    }

    fn path_sentences(&mut self, target: &Target, p: &Path, h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>, lem: &[LemmaRef]) -> Option<Vec<Sentence>> {
        let saved = self.as_drawn;
        if let Some(s) = self.path_sentences_mode(target, p, h, then.clone(), focus, lem, true) {
            let clean = s.iter().all(|x| match x {
                Sentence::Chain { terms, directed: false, .. } => terms.iter().all(|t| matches!(t, Expr::Angle { .. })),
                _ => true,
            });
            if clean {
                return Some(s);
            }
            self.as_drawn = saved;
        }
        self.path_sentences_mode(target, p, h, then, focus, lem, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn path_sentences_mode(&mut self, target: &Target, p: &Path, h: FactId, then: Option<Stmt>, focus: &BTreeSet<PointId>, lem: &[LemmaRef], allow_halve: bool) -> Option<Vec<Sentence>> {
        let cx = self.w.cx;
        let split = &target.splits[p.split.min(target.splits.len() - 1)];
        let n = p.nodes.len();
        let even = |c: &LinComb| c.terms.iter().all(|(v, k)| *v < QBASE || (k.is_integer() && (k * &Rat::new(1, 2)).is_integer()));
        let halved: Path;
        let doubled = allow_halve && target.table == Table::Angle && then.is_none() && p.nodes.iter().all(even) && p.nodes.iter().any(|x| !classes(x).is_empty());
        if allow_halve && !doubled {
            return None;
        }
        let p = if doubled {
            let half = Rat::new(1, 2);
            halved = Path {
                split: p.split,
                nodes: p
                    .nodes
                    .iter()
                    .map(|x| {
                        let mut y = x.clone();
                        y.mul_assign_scalar(&half);
                        y
                    })
                    .collect(),
                links: p.links.iter().map(|g| g.iter().map(|(i, l)| (*i, l * &half)).collect()).collect(),
                weight: p.weight,
            };
            &halved
        } else {
            p
        };
        let half_at = if (doubled || self.force_undirected) && target.table == Table::Angle {
            Some(0)
        } else {
            (0..p.links.len()).find(|&j| !integral_node(&p.nodes[j]) || !integral_node(&p.nodes[j + 1]))
        };
        let w = self.w;
        let pts_of = |j: usize| -> Vec<PointId> {
            let mut v: Vec<PointId> = p.links.get(j).map(|g| g.iter().flat_map(|(i, _)| item_points(w, *i, lem)).collect()).unwrap_or_default();
            v.sort_unstable();
            v.dedup();
            v
        };
        let mut terms: Vec<(Expr, bool)> = Vec::new();
        for j in 0..n {
            let directed = half_at.is_none_or(|h0| j <= h0) && target.table == Table::Angle;
            let directed_term = directed && half_at.is_none_or(|h0| j < h0 || (j == h0 && h0 > 0));
            if std::env::var_os("HP_BFS").is_some() {
                eprintln!("term {j} directed {directed_term} node {:?}", p.nodes[j]);
            }
            let e = if j == 0 && directed_term {
                split.l.clone()
            } else if j == n - 1 && (directed_term || target.table != Table::Angle) {
                split.r.clone()
            } else if target.table == Table::Angle {
                let next = pts_of(j);
                let prev = if j > 0 { pts_of(j - 1) } else { Vec::new() };
                display_node(cx, &p.nodes[j], directed_term, &[&next, &prev], focus)?
            } else {
                ratio_expr(cx, &p.nodes[j])
            };
            terms.push((e, directed_term));
        }
        let mut sentences = Vec::new();
        let mut links: Vec<Link> = Vec::new();
        for g in &p.links {
            let (rs, cs) = self.group_reasons(g, h, lem);
            links.push(Link { reasons: rs, combination: cs });
        }
        if half_at.is_none() && links.len() > LONG_CHAIN {
            let simple = |e: &Expr| !matches!(e, Expr::Lin { terms } if terms.iter().filter(|(_, x)| !matches!(x, Expr::Const { .. })).count() > 1);
            let mid = links.len() / 2;
            let cut = (2..links.len() - 1).filter(|&j| simple(&terms[j].0)).min_by_key(|&j| (j as i64 - mid as i64).abs());
            if let Some(j) = cut {
                let all: Vec<Expr> = terms.into_iter().map(|x| x.0).collect();
                let directed = target.table == Table::Angle;
                sentences.push(Sentence::Chain { terms: all[..=j].to_vec(), links: links[..j].to_vec(), then: None, directed });
                sentences.push(Sentence::Chain { terms: all[j..].to_vec(), links: links[j..].to_vec(), then, directed });
                return Some(sentences);
            }
        }
        match half_at {
            None if links.len() == 1 && target.table != Table::Angle => {
                let l = links.into_iter().next().unwrap();
                sentences.push(Sentence::Because { stmt: then.unwrap_or_else(|| target.stmt.clone()), reasons: l.reasons, combination: l.combination });
            }
            None if links.len() == 1 && then.as_ref().is_none_or(|t| matches!(t, Stmt::EqAngle { .. })) => {
                let l = links.into_iter().next().unwrap();
                sentences.push(Sentence::Because { stmt: then.unwrap_or_else(|| target.stmt.clone()), reasons: l.reasons, combination: l.combination });
            }
            None => sentences.push(Sentence::Chain { terms: terms.into_iter().map(|x| x.0).collect(), links, then, directed: target.table == Table::Angle }),
            Some(h0) => {
                self.as_drawn = true;
                if h0 > 0 {
                    let t1: Vec<Expr> = terms[..=h0].iter().map(|x| x.0.clone()).collect();
                    sentences.push(Sentence::Chain { terms: t1, links: links[..h0].to_vec(), then: None, directed: true });
                }
                let mut t2: Vec<Expr> = Vec::new();
                for j in h0..n {
                    let e = display_node(cx, &p.nodes[j], false, &[&pts_of(j), &pts_of(j.saturating_sub(1))], focus);
                    if e.is_none() && std::env::var_os("HP_BFS").is_some() {
                        eprintln!("undirected node {j} has no display: {:?}", p.nodes[j]);
                    }
                    t2.push(e?);
                }
                let mut l2: Vec<Link> = links[h0..].to_vec();
                let negative = t2.iter().filter(|e| angle_sign(e) < 0).count();
                if negative * 2 > t2.len() {
                    t2 = t2.into_iter().map(negate_expr).collect();
                    for l in l2.iter_mut() {
                        for t in l.combination.iter_mut() {
                            t.coef = -t.coef.clone();
                        }
                    }
                }
                sentences.push(Sentence::Chain { terms: t2, links: l2, then, directed: false });
            }
        }
        Some(sentences)
    }
}

fn angle_sign(e: &Expr) -> i32 {
    match e {
        Expr::Lin { terms } => terms
            .iter()
            .find(|(_, x)| matches!(x, Expr::Angle { .. } | Expr::LineAngle { .. }))
            .map(|(k, _)| if k.is_negative() { -1 } else { 1 })
            .unwrap_or(1),
        _ => 1,
    }
}

fn negate_expr(e: Expr) -> Expr {
    match e {
        Expr::Lin { terms } => {
            let terms: Vec<(Rat, Expr)> = terms
                .into_iter()
                .map(|(k, x)| match x {
                    Expr::Const { degrees } => (k, Expr::Const { degrees: -degrees }),
                    x => (-k, x),
                })
                .collect();
            if terms.len() == 1 && terms[0].0.is_one() && !matches!(terms[0].1, Expr::Const { .. }) {
                terms.into_iter().next().unwrap().1
            } else {
                let mut t = terms;
                t.sort_by_key(|(_, x)| matches!(x, Expr::Const { .. }));
                Expr::Lin { terms: t }
            }
        }
        Expr::Const { degrees } => Expr::Const { degrees: -degrees },
        x => Expr::Lin { terms: vec![(Rat::from_int(-1), x)] },
    }
}

fn penalty(a: &Atom) -> i64 {
    let mut p = 0i64;
    match a.src {
        AtomSrc::Line(_) => p += 5,
        AtomSrc::Human(_) => p += 3,
        _ => {}
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
            force_undirected: false,
        }
    }

    pub fn fact_reason(&self, r: FactId) -> Reason {
        let cx = self.w.cx;
        let d = cx.displayable(r);
        if let (Some(&b), Some(Role::Claim)) = (self.block_of.get(&d), self.role.get(&d)) {
            return Reason::Claim { n: self.claim_no.get(&d).copied().unwrap_or(0), block: b, fact: d };
        }
        if let Some(Reason::Fact { stmt, block, because, .. }) = self.inline.get(&d) {
            return Reason::Fact { stmt: stmt.clone(), fact: r, block: *block, because: because.clone() };
        }
        Reason::Fact { stmt: fact_stmt(cx, r), fact: r, block: self.block_of.get(&d).copied(), because: Vec::new() }
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
                Kind::Merge if matches!(cx.t.facts.get(f as usize).map(|x| &x.reason), Some(ER::TangentMerge(..))) => TheoremKey::TangentMerge,
                Kind::Merge => TheoremKey::PointMerge,
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
        let node_label = node.label;
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
            let then = if f == GOAL && pi + 1 == nparts && node_label != "centre" {
                if node_label == MERGED {
                    node.extra.first().map(|&gf| fact_stmt(cx, gf))
                } else {
                    Some(node_stmt.clone())
                }
            } else {
                then
            };
            let (mut s, l, p) = self.chain_sentences(&part.target, &part.support, h, then, &focus);
            links += l;
            pooled |= p;
            shift_pending(&mut s, out.len());
            out.extend(s);
        }
        let requires = self.w.nodes[&f].requires.clone();
        let req_reasons: Vec<Reason> = requires.iter().map(|&r| self.fact_reason(r)).collect();
        if node_label == "centre" || node_label == "central" {
            out.push(Sentence::Because { stmt: node_stmt.clone(), reasons: Vec::new(), combination: Vec::new() });
            return (out, links, pooled);
        }
        if f == GOAL && node_label == MERGED {
            out.push(Sentence::Because { stmt: node_stmt.clone(), reasons: Vec::new(), combination: Vec::new() });
            return (out, links, pooled);
        }
        match node_kind {
            Kind::Sim | Kind::Congruent | Kind::Isosceles if f != GOAL => {
                out.push(Sentence::Because { stmt: node_stmt.clone(), reasons: Vec::new(), combination: Vec::new() });
            }
            Kind::Cong => {
                out.push(Sentence::Theorem { key: TheoremKey::ArcChord, stmt: node_stmt.clone(), reasons: req_reasons });
            }
            Kind::Theorem(k) => {
                out.push(Sentence::Theorem { key: k, stmt: node_stmt.clone(), reasons: req_reasons });
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
            let t0 = std::time::Instant::now();
            let (sentences, links, pooled) = self.node_sentences(f);
            if std::env::var_os("HP_TIME").is_some() {
                eprintln!("  node {f}: {:.1} ms", t0.elapsed().as_secs_f64() * 1e3);
            }
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
                        if !dedup.contains(&r) && !matches!(r, Reason::Lemma { .. }) {
                            dedup.push(r);
                        }
                    }
                    self.inline.insert(f, Reason::Fact { stmt, fact: f, block: None, because: dedup });
                    self.allowed.insert(f);
                }
                Role::MergeConclusion => {
                    let mut sentences = sentences;
                    shift_pending(&mut sentences, merge_sentences.len());
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
                    let mut body = if role == Role::Raw && !sentences.iter().any(|s| matches!(s, Sentence::Raw { .. })) {
                        vec![Sentence::Raw { engine_fact: f, cites: cx.t.facts[f as usize].premises.clone() }]
                    } else {
                        sentences
                    };
                    bind_pending(&mut body, id);
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
        shift_pending(&mut body, all.len());
        all.append(&mut body);
        bind_pending(&mut all, id);
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
