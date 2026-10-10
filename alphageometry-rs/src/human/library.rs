use super::atoms::{AtomSrc, Builder, NEVER};
use super::ctx::{Ctx, FactClass};
use super::model::AtomKey;
use super::theorems::{candidates, config_holds, congruence_candidates, cost, is_theorem, spec, Figure, Hyp};
use super::trace::{EngineTrace, Table};
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet};

pub const THEOREM_CANDIDATES: usize = 4000;
pub const PER_KEY: usize = 120;
pub const THEOREM_LEVEL: u32 = 4;
pub const ATOM_LEVEL: u32 = 3;

pub fn support_level(cx: &Ctx, g: FactId) -> u32 {
    match cx.class[g as usize] {
        FactClass::Hyp | FactClass::HypReg => 1,
        FactClass::Silent | FactClass::SilentHyp => match &cx.t.facts[g as usize].reason {
            Reason::SimilarTriangles(..) => 3,
            _ => 2,
        },
        _ => match &cx.t.facts[g as usize].reason {
            Reason::SimilarTriangles(..) => 12,
            Reason::Theorem(..) | Reason::Formula(..) => 5,
            _ => 6,
        },
    }
}

pub fn canon_row(t: &EngineTrace, tb: Table, r: &LinComb) -> LinComb {
    match tb {
        Table::Ratio => t.canon_ratio(r),
        Table::Angle => t.exact(Table::Angle, r),
        _ => r.clone(),
    }
}

pub fn quot_row(cx: &Ctx, tb: Table, r: &LinComb) -> LinComb {
    let c = canon_row(cx.t, tb, r);
    if tb == Table::Angle {
        cx.quot.q(&c)
    } else {
        c
    }
}

pub fn row_key(cx: &Ctx, tb: Table, r: &LinComb) -> Option<(Table, LinComb)> {
    let c = canon_row(cx.t, tb, r);
    let mut c = if tb == Table::Angle { cx.quot.q(&c) } else { c };
    let lead = c.terms.iter().find(|(v, _)| tb != Table::Angle || *v != ANGLE_UNIT)?.1.clone();
    c.mul_assign_scalar(&lead.recip());
    Some((tb, c))
}

#[derive(Clone, Debug)]
pub struct Est {
    pub level: u32,
    pub avail: FactId,
    pub fact: Option<FactId>,
    pub atom: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Found {
    pub avail: FactId,
    pub sources: BTreeSet<FactId>,
    pub via: Vec<usize>,
    pub level: u32,
}

pub struct Directory {
    map: FxHashMap<(Table, LinComb), Vec<Est>>,
    hyp_keys: Vec<Vec<(Table, LinComb)>>,
    concl_keys: Vec<Vec<(Table, LinComb)>>,
}

impl Directory {
    pub fn new(b: &Builder) -> Directory {
        let cx = b.cx;
        let mut map: FxHashMap<(Table, LinComb), Vec<Est>> = FxHashMap::default();
        for tb in Table::ALL {
            for (g, r) in &cx.t.rows[tb.idx()] {
                if !cx.in_cl[*g as usize] {
                    continue;
                }
                if let Some(k) = row_key(cx, tb, r) {
                    map.entry(k).or_default().push(Est { level: support_level(cx, *g), avail: g + 1, fact: Some(*g), atom: None });
                }
            }
        }
        let mut hyp_keys = vec![Vec::new(); b.atoms.len()];
        let mut concl_keys = vec![Vec::new(); b.atoms.len()];
        let mut memo: BTreeMap<(AtomKey, Vec<PointId>), (Vec<(Table, LinComb)>, Vec<(Table, LinComb)>)> = BTreeMap::new();
        for (i, a) in b.atoms.iter().enumerate() {
            let AtomSrc::Human(k) = a.src else { continue };
            if a.avail == NEVER || a.cost > 2 {
                continue;
            }
            let level = if is_theorem(k) { THEOREM_LEVEL + a.cost - 1 } else { ATOM_LEVEL };
            if let Some(key) = row_key(cx, a.table, &a.row) {
                map.entry(key).or_default().push(Est { level, avail: a.avail, fact: None, atom: Some(i) });
            }
            if is_theorem(k) {
                let (h, c) = memo
                    .entry((k, a.args.clone()))
                    .or_insert_with(|| match spec(cx.t, k, &a.args) {
                        Some(sp) => (
                            sp.hyps.iter().filter_map(|x| if let Hyp::Eq(tb, r) = x { row_key(cx, *tb, r) } else { None }).collect(),
                            sp.rows.iter().filter_map(|(tb, r)| row_key(cx, *tb, r)).collect(),
                        ),
                        None => (Vec::new(), Vec::new()),
                    })
                    .clone();
                hyp_keys[i] = h;
                concl_keys[i] = c;
            }
        }
        for v in map.values_mut() {
            v.sort_by_key(|e| (e.level, e.avail, e.fact, e.atom));
        }
        Directory { map, hyp_keys, concl_keys }
    }

    fn uses(&self, b: &Builder, u: usize, forbid: &[(Table, LinComb)], me: Option<usize>) -> bool {
        let mut stack = vec![u];
        let mut seen: Vec<usize> = Vec::new();
        while let Some(x) = stack.pop() {
            if seen.contains(&x) || seen.len() > 32 {
                continue;
            }
            seen.push(x);
            if Some(x) == me {
                return true;
            }
            if self.hyp_keys.get(x).is_some_and(|hs| hs.iter().any(|h| forbid.contains(h))) {
                return true;
            }
            if let Some(a) = b.atoms.get(x) {
                stack.extend(a.via.iter().copied());
            }
        }
        false
    }

    pub fn lookup(&self, b: &Builder, tb: Table, r: &LinComb, h: FactId, forbid: &[(Table, LinComb)], me: Option<usize>, atoms_ok: bool) -> Option<Found> {
        let cx = b.cx;
        let key = row_key(cx, tb, r)?;
        let ests = self.map.get(&key)?;
        for e in ests {
            if e.avail > h {
                continue;
            }
            match (e.fact, e.atom) {
                (Some(g), _) => return Some(Found { avail: e.avail, sources: cx.fact_sources(g), via: Vec::new(), level: e.level }),
                (None, Some(i)) if atoms_ok => {
                    if self.uses(b, i, forbid, me) {
                        continue;
                    }
                    return Some(Found { avail: e.avail, sources: b.atoms[i].sources.clone(), via: vec![i], level: e.level });
                }
                _ => {}
            }
        }
        None
    }

    pub fn conclusions(&self, i: usize) -> &[(Table, LinComb)] {
        self.concl_keys.get(i).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

pub const SHORT_SUPPORT: usize = 3;

pub struct HypEval<'d> {
    dir: &'d Directory,
    closure: Vec<super::cert::Basis>,
    rows: Vec<Vec<(u32, FactId, Option<FactId>, Option<usize>, LinComb)>>,
    pub store_checks: usize,
}

impl<'d> HypEval<'d> {
    pub fn new(cx: &Ctx, dir: &'d Directory) -> Self {
        let mut closure: Vec<super::cert::Basis> = Table::ALL.iter().map(|tb| super::cert::Basis::new(&cx.piv[tb.idx()], false)).collect();
        for tb in Table::ALL {
            for (i, (g, r)) in cx.t.rows[tb.idx()].iter().enumerate() {
                if cx.in_cl[*g as usize] {
                    let r2 = if tb == Table::Ratio { cx.t.canon_ratio(r) } else { r.clone() };
                    closure[tb.idx()].insert(i as u32, &r2);
                }
            }
        }
        HypEval { dir, closure, rows: Vec::new(), store_checks: 0 }
    }

    pub fn with_rows(mut self, b: &Builder) -> Self {
        let cx = b.cx;
        let mut rows: Vec<Vec<(u32, FactId, Option<FactId>, Option<usize>, LinComb)>> = vec![Vec::new(); 4];
        for tb in Table::ALL {
            for (g, r) in &cx.t.rows[tb.idx()] {
                if cx.in_cl[*g as usize] {
                    rows[tb.idx()].push((support_level(cx, *g), g + 1, Some(*g), None, quot_row(cx, tb, r)));
                }
            }
        }
        for (i, a) in b.atoms.iter().enumerate() {
            let AtomSrc::Human(k) = a.src else { continue };
            if a.avail == NEVER || a.cost > 2 {
                continue;
            }
            let level = if is_theorem(k) { THEOREM_LEVEL + a.cost - 1 } else { ATOM_LEVEL };
            rows[a.table.idx()].push((level, a.avail, None, Some(i), quot_row(cx, a.table, &a.row)));
        }
        for v in rows.iter_mut() {
            v.sort_by(|x, y| (x.0, x.1, x.2, x.3).cmp(&(y.0, y.1, y.2, y.3)));
        }
        self.rows = rows;
        self
    }

    fn short(&self, b: &Builder, tb: Table, r: &LinComb, h: FactId, forbid: &[(Table, LinComb)], me: Option<usize>) -> Option<Found> {
        let cx = b.cx;
        let all = self.rows.get(tb.idx())?;
        let usable: Vec<&(u32, FactId, Option<FactId>, Option<usize>, LinComb)> = all.iter().filter(|x| x.1 <= h && x.3.is_none_or(|i| !self.dir.uses(b, i, forbid, me))).collect();
        let input: Vec<(u32, &LinComb)> = usable.iter().map(|x| (x.0, &x.4)).collect();
        let target = quot_row(cx, tb, r);
        let cert = super::cert::certify_greedy(&cx.piv[tb.idx()], &input, &target, &|_| true)?;
        if cert.len() > SHORT_SUPPORT {
            return None;
        }
        let mut f = Found::default();
        for (k, _) in cert {
            let x = usable[k];
            f.avail = f.avail.max(x.1);
            f.level += x.0;
            match (x.2, x.3) {
                (Some(g), _) => f.sources.extend(cx.fact_sources(g)),
                (None, Some(i)) => {
                    f.sources.extend(b.atoms[i].sources.iter().copied());
                    if !f.via.contains(&i) {
                        f.via.push(i);
                    }
                }
                _ => {}
            }
        }
        Some(f)
    }

    pub fn verified(&mut self, cx: &Ctx, tb: Table, r: &LinComb) -> bool {
        let row = canon_row(cx.t, tb, r);
        if self.closure[tb.idx()].contains(&row) {
            return true;
        }
        self.store_checks += 1;
        cx.store_verified(tb, &row)
    }

    pub fn eval(&self, b: &Builder, h: &Hyp, forbid: &[(Table, LinComb)], me: Option<usize>) -> Option<Found> {
        let cx = b.cx;
        let t = cx.t;
        match h {
            Hyp::Eq(tb, r) => {
                if !t.holds(*tb, r) {
                    return None;
                }
                self.dir.lookup(b, *tb, r, NEVER, forbid, me, true).or_else(|| self.short(b, *tb, r, NEVER, forbid, me))
            }
            Hyp::Line(pts) => cx.line_through(pts, NEVER).map(|l| Found { avail: l.fact + 1, sources: l.src.clone(), via: Vec::new(), level: 1 }),
            Hyp::Circle(pts) => {
                let by_obj = cx.circles.iter().filter(|c| pts.iter().all(|p| c.pts.contains(p))).min_by_key(|c| (c.src.len(), c.fact)).map(|c| Found { avail: c.fact + 1, sources: c.src.clone(), via: Vec::new(), level: 1 });
                if by_obj.is_some() {
                    return by_obj;
                }
                let mut best: Option<Found> = None;
                for o in 0..t.n as PointId {
                    if pts.contains(&o) || t.dm(o, pts[0]).is_none() {
                        continue;
                    }
                    let r0 = t.dist(o, pts[0]);
                    if r0 < 1e-9 || pts.iter().any(|&p| (t.dist(o, p) - r0).abs() > 1e-7 * r0.max(1.0)) {
                        continue;
                    }
                    let mut acc = Found::default();
                    let mut ok = true;
                    for &p in &pts[1..] {
                        let row = match t.dm(o, pts[0]).zip(t.dm(o, p)) {
                            Some((x, y)) => &x - &y,
                            None => {
                                ok = false;
                                break;
                            }
                        };
                        match self.dir.lookup(b, Table::Ratio, &row, NEVER, forbid, me, true) {
                            Some(f) => {
                                acc.avail = acc.avail.max(f.avail);
                                acc.sources.extend(f.sources);
                                acc.via.extend(f.via);
                                acc.level += f.level;
                            }
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok && best.as_ref().is_none_or(|x| (acc.sources.len(), acc.avail) < (x.sources.len(), x.avail)) {
                        best = Some(acc);
                    }
                }
                best
            }
            _ => config_holds(t, h).then(Found::default),
        }
    }
}

fn lvl(cx: &Ctx, s: &BTreeSet<FactId>) -> u32 {
    s.iter().map(|&g| support_level(cx, g)).sum()
}

impl<'c, 'a> Builder<'c, 'a> {
    pub fn theorems(&mut self, radii_from: usize) -> usize {
        let cx = self.cx;
        let t = cx.t;
        let start = self.atoms.len();
        let t0 = std::time::Instant::now();
        let lines: Vec<Vec<PointId>> = cx.lines.iter().map(|l| l.pts.clone()).collect();
        let mut circles: Vec<Vec<PointId>> = cx.circles.iter().map(|c| c.pts.clone()).collect();
        for (_, rs) in self.groups(radii_from) {
            let mut m: Vec<PointId> = Vec::new();
            for r in rs {
                for p in [r.0, r.1] {
                    if !m.contains(&p) {
                        m.push(p);
                    }
                }
            }
            if m.len() >= 4 {
                circles.push(m);
            }
        }
        let mut fig = Figure::new(t, lines, circles);
        let mut used_sines: BTreeSet<crate::lincomb::VarId> = BTreeSet::new();
        if !t.sines.is_empty() {
            let sine_vars: BTreeSet<crate::lincomb::VarId> = t.sines.iter().map(|x| t.sine_canon(x.var)).collect();
            for (g, r) in &t.rows[Table::Ratio.idx()] {
                if cx.in_cl[*g as usize] {
                    for (v, _) in t.canon_ratio(r).terms.iter() {
                        if sine_vars.contains(v) {
                            used_sines.insert(*v);
                        }
                    }
                }
            }
            for x in &t.sines {
                if x.shift.mod_one().is_zero() && x.p != x.q && x.v != x.p && x.v != x.q && used_sines.contains(&t.sine_canon(x.var)) && t.orient(x.v, x.p, x.q) != 0 {
                    fig.sines.insert((x.v, x.p.min(x.q), x.p.max(x.q)));
                }
            }
        }
        let mut cands = candidates(&fig, THEOREM_CANDIDATES);
        let mut pairs: Vec<([PointId; 3], [PointId; 3])> = Vec::new();
        for &f in &cx.closure {
            if let Reason::SimilarTriangles(a, b) = &t.facts[f as usize].reason {
                if super::classify::self_similar(*a, *b) || !super::classify::ratio_one(cx, *a, *b) {
                    continue;
                }
                let p = ([a.0, a.1, a.2], [b.0, b.1, b.2]);
                if !pairs.contains(&p) {
                    pairs.push(p);
                }
            }
        }
        cands.extend(congruence_candidates(&pairs));
        let present: BTreeSet<(AtomKey, Vec<PointId>)> = self.atoms.iter().filter_map(|a| if let AtomSrc::Human(k) = a.src { is_theorem(k).then(|| (k, a.args.clone())) } else { None }).collect();
        let dir = Directory::new(self);
        let mut ev = HypEval::new(cx, &dir).with_rows(self);
        let mut made: Vec<(AtomKey, Vec<PointId>, Vec<(Table, LinComb)>, Found)> = Vec::new();
        let ncand = cands.len();
        let mut cand_keys: BTreeMap<AtomKey, usize> = BTreeMap::new();
        for (k, _) in &cands {
            *cand_keys.entry(*k).or_default() += 1;
        }
        let sine_vars: BTreeSet<crate::lincomb::VarId> = t.sines.iter().map(|x| t.sine_canon(x.var)).collect();
        let mut goal_keys: rustc_hash::FxHashSet<(Table, LinComb)> = rustc_hash::FxHashSet::default();
        for o in super::classify::goal_obligations(cx, &cx.goal) {
            for tg in &o.targets {
                for form in std::iter::once(tg).chain(tg.alts.iter()) {
                    if let Some(k) = row_key(cx, form.table, &form.row) {
                        goal_keys.insert(k);
                    }
                }
            }
        }
        let mut per_key: BTreeMap<AtomKey, usize> = BTreeMap::new();
        for (key, args) in cands {
            if cx.timed_out() {
                break;
            }
            if present.contains(&(key, args.clone())) || per_key.get(&key).copied().unwrap_or(0) >= PER_KEY {
                continue;
            }
            let Some(sp) = spec(t, key, &args) else { continue };
            let trig_ok = sp.rows.iter().all(|(tb, r)| *tb != Table::Ratio || t.canon_ratio(r).terms.iter().all(|(v, _)| !sine_vars.contains(v) || used_sines.contains(v)));
            if !trig_ok {
                continue;
            }
            if !sp.hyps.iter().filter(|h| h.is_config()).all(|h| config_holds(t, h)) {
                continue;
            }
            let forbid: Vec<(Table, LinComb)> = sp.rows.iter().filter_map(|(tb, r)| row_key(cx, *tb, r)).collect();
            let mut acc = Found::default();
            let mut ok = true;
            for h in sp.hyps.iter().filter(|h| !h.is_config()) {
                match ev.eval(self, h, &forbid, None) {
                    Some(f) => {
                        acc.avail = acc.avail.max(f.avail);
                        acc.sources.extend(f.sources);
                        for v in f.via {
                            if !acc.via.contains(&v) {
                                acc.via.push(v);
                            }
                        }
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && !sp.rows.iter().all(|(tb, r)| ev.verified(cx, *tb, r)) {
                ok = false;
            }
            let restates_goal = acc.sources.is_empty() && acc.via.is_empty() && sp.rows.iter().any(|(tb, r)| row_key(cx, *tb, r).is_some_and(|k| goal_keys.contains(&k)));
            if ok && !restates_goal {
                *per_key.entry(key).or_default() += 1;
                made.push((key, args, sp.rows, acc));
            }
        }
        let store_checks = ev.store_checks;
        drop(ev);
        drop(dir);
        for (key, args, rows, acc) in made {
            if std::env::var_os("HP_THM").is_some() {
                eprintln!("theorem atom {key:?} {args:?} avail {} sources {:?} via {:?}", acc.avail, acc.sources, acc.via);
            }
            let from = self.atoms.len();
            self.push(AtomSrc::Human(key), args, rows, cost(key), acc.sources, acc.avail, acc.avail);
            let mut via = acc.via;
            via.sort_unstable();
            for a in &mut self.atoms[from..] {
                a.via = via.clone();
            }
        }
        if std::env::var_os("HP_TIME").is_some() {
            eprintln!("candidates per key: {:?}", cand_keys);
            eprintln!("theorems: {ncand} candidates, {} atoms, {store_checks} store checks, {:.1} ms", self.atoms.len() - start, t0.elapsed().as_secs_f64() * 1e3);
        }
        start
    }

    pub fn refine(&mut self, radii_from: usize) {
        let cx = self.cx;
        let dir = Directory::new(self);
        let mut updates: Vec<(usize, BTreeSet<FactId>, Vec<usize>)> = Vec::new();
        for i in radii_from..self.atoms.len() {
            let a = &self.atoms[i];
            if a.src != AtomSrc::Human(AtomKey::Radii) || a.avail == NEVER {
                continue;
            }
            let before = lvl(cx, &a.sources);
            if before <= 2 * a.sources.len() as u32 {
                continue;
            }
            let Some(key) = row_key(cx, a.table, &a.row) else { continue };
            let Some(f) = dir.lookup(self, a.table, &a.row, a.avail, std::slice::from_ref(&key), Some(i), true) else { continue };
            if f.via.is_empty() || lvl(cx, &f.sources) >= before {
                continue;
            }
            updates.push((i, f.sources, f.via));
        }
        drop(dir);
        for (i, s, via) in updates {
            if std::env::var_os("HP_THM").is_some() {
                eprintln!("refine radii {:?} via {:?}", self.atoms[i].args, via);
            }
            self.atoms[i].sources = s;
            self.atoms[i].via = via;
        }
    }

    pub fn rejustify(&mut self) {
        let cx = self.cx;
        let dir = Directory::new(self);
        let ev = HypEval::new(cx, &dir).with_rows(self);
        let mut updates: Vec<(AtomKey, Vec<PointId>, BTreeSet<FactId>, Vec<usize>)> = Vec::new();
        for i in 0..self.atoms.len() {
            if cx.timed_out() {
                break;
            }
            let a = &self.atoms[i];
            let AtomSrc::Human(k) = a.src else { continue };
            if !is_theorem(k) || a.avail == NEVER || a.row_idx != 0 {
                continue;
            }
            let before = lvl(cx, &a.sources);
            if before <= 2 * a.sources.len() as u32 {
                continue;
            }
            let Some(sp) = spec(cx.t, k, &a.args) else { continue };
            let forbid: Vec<(Table, LinComb)> = dir.conclusions(i).to_vec();
            let mut acc = Found::default();
            let mut ok = true;
            for h in sp.hyps.iter().filter(|x| !x.is_config()) {
                match ev.eval(self, h, &forbid, Some(i)) {
                    Some(f) if f.avail <= a.avail => {
                        acc.sources.extend(f.sources);
                        for v in f.via {
                            if !acc.via.contains(&v) {
                                acc.via.push(v);
                            }
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && lvl(cx, &acc.sources) < before {
                acc.via.sort_unstable();
                updates.push((k, a.args.clone(), acc.sources, acc.via));
            }
        }
        drop(ev);
        drop(dir);
        for (k, args, s, via) in updates {
            for a in self.atoms.iter_mut() {
                if a.src == AtomSrc::Human(k) && a.args == args {
                    a.sources = s.clone();
                    a.via = via.clone();
                }
            }
        }
    }

    pub fn propagate_via(&mut self, radii_from: usize) {
        let mut by_pair: BTreeMap<(PointId, PointId, PointId), Vec<usize>> = BTreeMap::new();
        for a in &self.atoms[radii_from..] {
            if a.src == AtomSrc::Human(AtomKey::Radii) && a.avail != NEVER && !a.via.is_empty() {
                by_pair.entry((a.args[0], a.args[1].min(a.args[2]), a.args[1].max(a.args[2]))).or_default().extend(a.via.iter().copied());
            }
        }
        if by_pair.is_empty() {
            return;
        }
        let get = |o: PointId, p: PointId, q: PointId| by_pair.get(&(o, p.min(q), p.max(q))).cloned().unwrap_or_default();
        for i in radii_from..self.atoms.len() {
            let a = &self.atoms[i];
            let AtomSrc::Human(k) = a.src else { continue };
            let g = &a.args;
            let mut via: Vec<usize> = match k {
                AtomKey::Isosceles if g.len() == 3 => get(g[0], g[1], g[2]),
                AtomKey::Thales => [get(g[3], g[0], g[1]), get(g[3], g[0], g[2]), get(g[3], g[1], g[2])].concat(),
                AtomKey::CentralAngle => [get(g[0], g[1], g[2]), get(g[0], g[1], g[3]), get(g[0], g[2], g[3])].concat(),
                AtomKey::PerpBisector => [get(g[0], g[2], g[3]), get(g[1], g[2], g[3])].concat(),
                AtomKey::Midline => [get(g[0], g[4], g[2]), get(g[1], g[4], g[3])].concat(),
                AtomKey::TangentChord => [get(g[4], g[0], g[2]), get(g[4], g[0], g[3]), get(g[4], g[2], g[3])].concat(),
                _ => continue,
            };
            via.sort_unstable();
            via.dedup();
            if !via.is_empty() {
                self.atoms[i].via = via;
            }
        }
    }
}
