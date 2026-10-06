use super::cert::Basis;
use super::ctx::{Ctx, FactClass};
use super::model::AtomKey;
use super::trace::Table;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason};
use crate::rational::Rat;
use std::collections::{BTreeMap, BTreeSet};

pub const NEVER: FactId = FactId::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AtomSrc {
    Human(AtomKey),
    Hyp(FactId),
    Glue(FactId),
    Line(FactId),
    Engine(FactId),
}

#[derive(Clone, Debug)]
pub struct Atom {
    pub src: AtomSrc,
    pub args: Vec<PointId>,
    pub table: Table,
    pub row: LinComb,
    pub q: LinComb,
    pub cost: u32,
    pub sources: BTreeSet<FactId>,
    pub tmpl: FactId,
    pub avail: FactId,
    pub row_idx: u16,
    pub via: Vec<usize>,
}

impl Atom {
    pub fn admissible(&self, h: FactId) -> bool {
        self.avail != NEVER && self.avail <= h
    }
    pub fn fact(&self) -> Option<FactId> {
        match self.src {
            AtomSrc::Hyp(f) | AtomSrc::Glue(f) | AtomSrc::Line(f) | AtomSrc::Engine(f) => Some(f),
            AtomSrc::Human(_) => None,
        }
    }
    pub fn key(&self) -> Option<AtomKey> {
        match self.src {
            AtomSrc::Human(k) => Some(k),
            _ => None,
        }
    }
}

pub fn fact_rows(cx: &Ctx, f: FactId) -> Vec<(Table, LinComb)> {
    let mut out = Vec::new();
    for tb in Table::ALL {
        for r in cx.t.rows_of(tb, f) {
            out.push((tb, r.clone()));
        }
    }
    out
}

pub fn engine_cost(cx: &Ctx, f: FactId) -> u32 {
    match &cx.t.facts[f as usize].reason {
        Reason::Concyclic(_) => 3,
        Reason::SimilarTriangles(..) => {
            if cx.class[f as usize] == FactClass::Silent {
                6
            } else {
                5
            }
        }
        Reason::Theorem(..) | Reason::Formula(..) | Reason::TransferArcChord(..) => 3,
        Reason::TransferAddMul(..) => 4,
        _ => 4,
    }
}

pub struct Builder<'c, 'a> {
    pub cx: &'c Ctx<'a>,
    pub atoms: Vec<Atom>,
}

pub fn human_row(cx: &Ctx, key: AtomKey, a: &[PointId]) -> Option<Vec<(Table, LinComb)>> {
    let t = cx.t;
    let half = Rat::new(1, 2);
    let ang = |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&t.dir(y, z)? - &t.dir(y, x)?) };
    let ex = |c: LinComb| t.exact(Table::Angle, &c);
    let rows = match key {
        AtomKey::Inscribed => {
            let (p, q, c, d) = (a[0], a[1], a[2], a[3]);
            vec![(Table::Angle, ex(&ang(p, c, q)? - &ang(p, d, q)?))]
        }
        AtomKey::Thales => {
            let (x, y, z) = (a[0], a[1], a[2]);
            let mut r = ang(x, z, y)?;
            r.add_term(crate::elimination::ANGLE_UNIT, -&half);
            vec![(Table::Angle, ex(r))]
        }
        AtomKey::TangentChord => {
            let (p, x, y, z) = (a[0], a[1], a[2], a[3]);
            let lhs = &t.dir(p, y)? - &t.dir(p, x)?;
            let rhs = ang(p, z, y)?;
            vec![(Table::Angle, ex(&lhs - &rhs))]
        }
        AtomKey::PerpBisector | AtomKey::Orthocentre => {
            let (l, m) = if key == AtomKey::PerpBisector { ((a[0], a[1]), (a[2], a[3])) } else { ((a[3], a[0]), (a[1], a[2])) };
            let mut r = &t.dir(l.0, l.1)? - &t.dir(m.0, m.1)?;
            r.add_term(crate::elimination::ANGLE_UNIT, -&half);
            vec![(Table::Angle, ex(r))]
        }
        AtomKey::Parallel => vec![(Table::Angle, ex(&t.dir(a[0], a[1])? - &t.dir(a[2], a[3])?))],
        AtomKey::Radii => vec![(Table::Ratio, &t.dm(a[0], a[1])? - &t.dm(a[0], a[2])?)],
        AtomKey::Isosceles => {
            let (o, p, q) = (a[0], a[1], a[2]);
            vec![(Table::Angle, ex(&ang(o, p, q)? - &ang(p, q, o)?))]
        }
        AtomKey::CentralAngle => {
            let (o, p, q, z) = (a[0], a[1], a[2], a[3]);
            let mut two = ang(p, z, q)?;
            two.mul_assign_scalar(&Rat::from_int(2));
            vec![(Table::Angle, ex(&ang(p, o, q)? - &two))]
        }
        AtomKey::PowerOfPoint => {
            let (x, p, q, c, d) = (a[0], a[1], a[2], a[3], a[4]);
            let r = &(&(&t.dm(x, p)? + &t.dm(x, q)?) - &t.dm(x, c)?) - &t.dm(x, d)?;
            vec![(Table::Ratio, r)]
        }
        AtomKey::Midline => {
            let (m, n, q, r) = (a[0], a[1], a[2], a[3]);
            let mut out = vec![(Table::Angle, ex(&t.dir(m, n)? - &t.dir(q, r)?))];
            if let Some(two) = t.prime_const(&Rat::from_int(2)) {
                out.push((Table::Ratio, &(&t.dm(q, r)? - &t.dm(m, n)?) - &two));
            }
            out
        }
        k => super::theorems::spec(t, k, a)?.rows,
    };
    Some(rows)
}

impl<'c, 'a> Builder<'c, 'a> {
    pub fn new(cx: &'c Ctx<'a>) -> Builder<'c, 'a> {
        Builder { cx, atoms: Vec::new() }
    }

    pub fn push(&mut self, src: AtomSrc, args: Vec<PointId>, rows: Vec<(Table, LinComb)>, cost: u32, sources: BTreeSet<FactId>, tmpl: FactId, avail: FactId) {
        for (k, (table, row)) in rows.into_iter().enumerate() {
            if row.is_zero() {
                continue;
            }
            let q = if table == Table::Angle { self.cx.quot.q(&row) } else { row.clone() };
            self.atoms.push(Atom { src, args: args.clone(), table, row, q, cost, sources: sources.clone(), tmpl, avail, row_idx: k as u16, via: Vec::new() });
        }
    }

    pub fn engine_atoms(&mut self) {
        let cx = self.cx;
        for &g in &cx.closure {
            let rows = fact_rows(cx, g);
            if rows.is_empty() {
                continue;
            }
            let class = cx.class[g as usize];
            let srcs = cx.fact_sources(g);
            match class {
                FactClass::Hyp => self.push(AtomSrc::Hyp(g), vec![], rows, 1, BTreeSet::new(), g + 1, g + 1),
                FactClass::HypReg => {
                    let is_line = matches!(cx.t.facts[g as usize].reason, Reason::Collinear(_));
                    for (k, (tb, r)) in rows.into_iter().enumerate() {
                        let q = if tb == Table::Angle { cx.quot.q(&r) } else { r.clone() };
                        let glue = is_line && tb == Table::Angle;
                        let (src, cost) = if glue { (AtomSrc::Glue(g), 0) } else { (AtomSrc::Engine(g), 2) };
                        self.atoms.push(Atom { src, args: vec![], table: tb, row: r, q, cost, sources: BTreeSet::new(), tmpl: g + 1, avail: g + 1, row_idx: k as u16, via: Vec::new() });
                    }
                }
                _ => {
                    let is_line = matches!(cx.t.facts[g as usize].reason, Reason::Collinear(_));
                    if is_line {
                        self.push(AtomSrc::Line(g), vec![], rows, 1, srcs, g + 1, g + 1);
                    } else {
                        let c = engine_cost(cx, g);
                        self.push(AtomSrc::Engine(g), vec![], rows, c, srcs, g + 1, g + 1);
                    }
                }
            }
        }
    }

    fn add_human(&mut self, key: AtomKey, args: Vec<PointId>, cost: u32, sources: BTreeSet<FactId>, tmpl: FactId) {
        if tmpl == NEVER {
            return;
        }
        if let Some(rows) = human_row(self.cx, key, &args) {
            let ok = rows.iter().all(|(tb, r)| self.cx.t.holds(*tb, r));
            if ok {
                self.push(AtomSrc::Human(key), args, rows, cost, sources, tmpl, NEVER);
            }
        }
    }

    pub fn sweep(&mut self, from: usize) {
        let cx = self.cx;
        let mut finals: Vec<Basis> = Table::ALL.iter().map(|tb| Basis::new(&cx.piv[tb.idx()], false)).collect();
        for tb in Table::ALL {
            for (i, (g, r)) in cx.t.rows[tb.idx()].iter().enumerate() {
                if cx.in_cl[*g as usize] {
                    finals[tb.idx()].insert(i as u32, r);
                }
            }
        }
        let mut pending: Vec<Vec<usize>> = vec![Vec::new(); 4];
        for i in from..self.atoms.len() {
            let a = &self.atoms[i];
            if a.avail != NEVER {
                continue;
            }
            if finals[a.table.idx()].contains(&a.row) {
                pending[a.table.idx()].push(i);
            }
        }
        let mut inc: Vec<Basis> = Table::ALL.iter().map(|tb| Basis::new(&cx.piv[tb.idx()], false)).collect();
        let mut by_fact: BTreeMap<FactId, Vec<(Table, usize)>> = BTreeMap::new();
        for tb in Table::ALL {
            for (i, (g, _)) in cx.t.rows[tb.idx()].iter().enumerate() {
                if cx.in_cl[*g as usize] {
                    by_fact.entry(*g).or_default().push((tb, i));
                }
            }
        }
        for (g, rows) in by_fact {
            let mut grew = [false; 4];
            for (tb, i) in rows {
                if inc[tb.idx()].insert(i as u32, &cx.t.rows[tb.idx()][i].1) {
                    grew[tb.idx()] = true;
                }
            }
            for tb in Table::ALL {
                if !grew[tb.idx()] {
                    continue;
                }
                let mut keep = Vec::new();
                for &i in &pending[tb.idx()] {
                    if inc[tb.idx()].contains(&self.atoms[i].row) {
                        let a = &mut self.atoms[i];
                        a.avail = a.tmpl.max(g + 1);
                    } else {
                        keep.push(i);
                    }
                }
                pending[tb.idx()] = keep;
            }
        }
    }

    pub fn radii(&mut self) -> usize {
        let cx = self.cx;
        let t = cx.t;
        let n = t.n as PointId;
        let start = self.atoms.len();
        for o in 0..n {
            if t.subst.get(o as usize).copied() != Some(o) && !cx.closure.iter().any(|&f| fact_mentions(cx, f, o)) {
                continue;
            }
            let mut others: Vec<(f64, PointId)> = (0..n).filter(|&p| p != o && t.dm(o, p).is_some()).map(|p| (t.dist(o, p), p)).collect();
            others.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal).then(x.1.cmp(&y.1)));
            for i in 0..others.len() {
                for j in i + 1..others.len() {
                    let (ri, rj) = (others[i].0, others[j].0);
                    if (rj - ri).abs() > 1e-9 * (1.0 + ri) {
                        break;
                    }
                    let (a, b) = (others[i].1.min(others[j].1), others[i].1.max(others[j].1));
                    if t.dist(a, b) < 1e-9 {
                        continue;
                    }
                    self.add_human(AtomKey::Radii, vec![o, a, b], 1, BTreeSet::new(), 0);
                }
            }
        }
        self.sweep(start);
        let mut i = start;
        while i < self.atoms.len() {
            if self.atoms[i].avail != NEVER {
                let h = self.atoms[i].avail;
                let row = self.atoms[i].row.clone();
                let mut s = BTreeSet::new();
                match cx.support_facts(Table::Ratio, &row, h) {
                    Some(sup) => {
                        for g in sup {
                            s.extend(cx.fact_sources(g));
                        }
                    }
                    None => {
                        self.atoms[i].avail = NEVER;
                    }
                }
                self.atoms[i].sources = s;
            }
            i += 1;
        }
        start
    }

    pub fn groups(&self, radii_from: usize) -> BTreeMap<PointId, Vec<(PointId, PointId, FactId, BTreeSet<FactId>)>> {
        let mut out: BTreeMap<PointId, Vec<(PointId, PointId, FactId, BTreeSet<FactId>)>> = BTreeMap::new();
        for a in &self.atoms[radii_from..] {
            if a.src == AtomSrc::Human(AtomKey::Radii) && a.avail != NEVER {
                out.entry(a.args[0]).or_default().push((a.args[1], a.args[2], a.avail, a.sources.clone()));
            }
        }
        out
    }

    pub fn composites(&mut self, radii_from: usize) -> usize {
        let cx = self.cx;
        let t = cx.t;
        let start = self.atoms.len();
        let groups = self.groups(radii_from);
        let eq = |o: PointId, a: PointId, b: PointId| -> Option<(FactId, BTreeSet<FactId>)> {
            if a == b {
                return Some((0, BTreeSet::new()));
            }
            let (x, y) = (a.min(b), a.max(b));
            groups.get(&o)?.iter().find(|r| r.0 == x && r.1 == y).map(|r| (r.2, r.3.clone()))
        };
        let merge = |xs: &[&(FactId, BTreeSet<FactId>)]| -> (FactId, BTreeSet<FactId>) {
            let mut h = 0;
            let mut s = BTreeSet::new();
            for x in xs {
                h = h.max(x.0);
                s.extend(x.1.iter().copied());
            }
            (h, s)
        };
        let mut members: BTreeMap<PointId, Vec<PointId>> = BTreeMap::new();
        for (o, rs) in &groups {
            let m = members.entry(*o).or_default();
            for r in rs {
                for p in [r.0, r.1] {
                    if !m.contains(&p) {
                        m.push(p);
                    }
                }
            }
            m.sort_unstable();
        }
        let mut inscribed: BTreeMap<[PointId; 4], (FactId, BTreeSet<FactId>)> = BTreeMap::new();
        for c in &cx.circles {
            let s = &c.pts;
            if s.len() < 4 || s.len() > 12 {
                continue;
            }
            for i in 0..s.len() {
                for j in i + 1..s.len() {
                    for k in j + 1..s.len() {
                        for l in k + 1..s.len() {
                            let key = [s[i], s[j], s[k], s[l]];
                            let cand = (c.fact + 1, c.src.clone());
                            let better = inscribed.get(&key).is_none_or(|old| (cand.1.len(), cand.0) < (old.1.len(), old.0));
                            if better {
                                inscribed.insert(key, cand);
                            }
                        }
                    }
                }
            }
        }
        for (o, m) in &members {
            if m.len() < 4 || m.len() > 10 {
                continue;
            }
            for i in 0..m.len() {
                for j in i + 1..m.len() {
                    for k in j + 1..m.len() {
                        for l in k + 1..m.len() {
                            let q = [m[i], m[j], m[k], m[l]];
                            let parts: Vec<(FactId, BTreeSet<FactId>)> = q[1..].iter().filter_map(|&p| eq(*o, q[0], p)).collect();
                            if parts.len() != 3 {
                                continue;
                            }
                            let refs: Vec<&(FactId, BTreeSet<FactId>)> = parts.iter().collect();
                            let cand = merge(&refs);
                            let better = inscribed.get(&q).is_none_or(|old| (cand.1.len(), cand.0) < (old.1.len(), old.0));
                            if better {
                                inscribed.insert(q, cand);
                            }
                        }
                    }
                }
            }
        }
        for (q, (h, s)) in inscribed {
            if (0..4).any(|i| (i + 1..4).any(|j| t.dist(q[i], q[j]) < 1e-9)) {
                continue;
            }
            self.add_human(AtomKey::Inscribed, q.to_vec(), 1, s, h);
        }
        for (o, m) in &members {
            let o = *o;
            for (ia, &a) in m.iter().enumerate() {
                for &b in &m[ia + 1..] {
                    let Some(ab) = eq(o, a, b) else { continue };
                    self.add_human(AtomKey::Isosceles, vec![o, a, b], 2, ab.1.clone(), ab.0);
                    let diam = cx.line_through(&[a, o, b], NEVER).map(|l| (l.fact + 1, l.src.clone()));
                    for &z in m {
                        if z == a || z == b {
                            continue;
                        }
                        let (Some(az), Some(bz)) = (eq(o, a, z), eq(o, b, z)) else { continue };
                        if let Some(d) = &diam {
                            let (h, s) = merge(&[&ab, &az, &bz, d]);
                            self.add_human(AtomKey::Thales, vec![a, b, z, o], 1, s, h);
                        } else if t.orient(a, o, b) != 0 {
                            let (h, s) = merge(&[&ab, &az, &bz]);
                            self.add_human(AtomKey::CentralAngle, vec![o, a, b, z], 2, s, h);
                        }
                    }
                }
            }
        }
        let owners: Vec<PointId> = members.keys().copied().collect();
        for (ix, &x) in owners.iter().enumerate() {
            for &y in &owners[ix + 1..] {
                if t.dir(x, y).is_none() {
                    continue;
                }
                let (mx, my) = (&members[&x], &members[&y]);
                for (ia, &a) in mx.iter().enumerate() {
                    for &b in &mx[ia + 1..] {
                        if !my.contains(&a) || !my.contains(&b) || [a, b].contains(&x) || [a, b].contains(&y) {
                            continue;
                        }
                        let (Some(e1), Some(e2)) = (eq(x, a, b), eq(y, a, b)) else { continue };
                        let (h, s) = merge(&[&e1, &e2]);
                        self.add_human(AtomKey::PerpBisector, vec![x, y, a, b], 1, s, h);
                    }
                }
            }
        }
        let mut power_seen: BTreeSet<(PointId, [PointId; 4])> = BTreeSet::new();
        let mut circle_sets: Vec<(Vec<PointId>, FactId, BTreeSet<FactId>)> = cx.circles.iter().map(|c| (c.pts.clone(), c.fact + 1, c.src.clone())).collect();
        for (o, m) in &members {
            if m.len() >= 4 {
                let parts: Vec<(FactId, BTreeSet<FactId>)> = m[1..].iter().filter_map(|&p| eq(*o, m[0], p)).collect();
                if parts.len() == m.len() - 1 {
                    let refs: Vec<&(FactId, BTreeSet<FactId>)> = parts.iter().collect();
                    let (h, s) = merge(&refs);
                    circle_sets.push((m.clone(), h, s));
                }
            }
        }
        for (s, h, src) in &circle_sets {
            if s.len() < 4 || s.len() > 12 {
                continue;
            }
            for x in 0..t.n as PointId {
                if s.contains(&x) {
                    continue;
                }
                let mut chords: Vec<(PointId, PointId, FactId, BTreeSet<FactId>)> = Vec::new();
                for (i, &a) in s.iter().enumerate() {
                    for &b in &s[i + 1..] {
                        if let Some(l) = cx.line_through(&[x, a, b], NEVER) {
                            chords.push((a, b, l.fact + 1, l.src.clone()));
                        }
                    }
                }
                for i in 0..chords.len() {
                    for j in i + 1..chords.len() {
                        let (a, b, h1, s1) = &chords[i];
                        let (c, d, h2, s2) = &chords[j];
                        let key = (x, [*a, *b, *c, *d]);
                        if !power_seen.insert(key) {
                            continue;
                        }
                        let mut so = src.clone();
                        so.extend(s1.iter().copied());
                        so.extend(s2.iter().copied());
                        self.add_human(AtomKey::PowerOfPoint, vec![x, *a, *b, *c, *d], 1, so, (*h).max(*h1).max(*h2));
                    }
                }
            }
        }
        let mut mids: Vec<(PointId, PointId, PointId, FactId, BTreeSet<FactId>)> = Vec::new();
        for (m, rs) in &groups {
            for r in rs {
                let (b, c) = (r.0, r.1);
                if let Some(l) = cx.line_through(&[*m, b, c], NEVER) {
                    let mut s = r.3.clone();
                    s.extend(l.src.iter().copied());
                    mids.push((*m, b, c, r.2.max(l.fact + 1), s));
                }
            }
        }
        for i in 0..mids.len() {
            for j in 0..mids.len() {
                if i == j {
                    continue;
                }
                let (m, p1, p2, h1, s1) = &mids[i];
                let (n2, q1, q2, h2, s2) = &mids[j];
                for (p, q) in [(*p1, *p2), (*p2, *p1)] {
                    for (pp, r) in [(*q1, *q2), (*q2, *q1)] {
                        if p != pp || q == r || m == n2 {
                            continue;
                        }
                        if m > n2 {
                            continue;
                        }
                        let mut s = s1.clone();
                        s.extend(s2.iter().copied());
                        self.add_human(AtomKey::Midline, vec![*m, *n2, q, r, p], 1, s, (*h1).max(*h2));
                    }
                }
            }
        }
        self.sweep(start);
        start
    }

    pub fn perps(&self) -> Vec<((PointId, PointId), (PointId, PointId), FactId, BTreeSet<FactId>)> {
        let cx = self.cx;
        let mut out = Vec::new();
        for a in &self.atoms {
            if a.avail == NEVER {
                continue;
            }
            match a.src {
                AtomSrc::Hyp(f) => {
                    if let Some(p) = cx.hyp_pred.get(&f) {
                        if p.name == "perp" && p.points.len() == 4 {
                            let pts = &p.points;
                            out.push(((pts[0], pts[1]), (pts[2], pts[3]), a.avail, BTreeSet::new()));
                        }
                    }
                }
                AtomSrc::Human(AtomKey::PerpBisector) => {
                    out.push(((a.args[0], a.args[1]), (a.args[2], a.args[3]), a.avail, a.sources.clone()));
                }
                AtomSrc::Human(AtomKey::Thales) => {
                    out.push(((a.args[2], a.args[0]), (a.args[2], a.args[1]), a.avail, a.sources.clone()));
                }
                _ => {}
            }
        }
        out
    }

    pub fn second_order(&mut self, radii_from: usize) -> usize {
        let cx = self.cx;
        let t = cx.t;
        let start = self.atoms.len();
        let perps = self.perps();
        let cls = |l: (PointId, PointId)| t.var(Table::Angle, l.0, l.1).and_then(|v| cx.quot.class_of(v));
        for i in 0..perps.len() {
            for j in 0..perps.len() {
                if i >= j {
                    continue;
                }
                let (l1, m1, h1, s1) = &perps[i];
                let (l2, m2, h2, s2) = &perps[j];
                let combos = [(*l1, *m1, *l2, *m2), (*l1, *m1, *m2, *l2), (*m1, *l1, *l2, *m2), (*m1, *l1, *m2, *l2)];
                for (a, ma, b, mb) in combos {
                    if cls(ma).is_none() || cls(ma) != cls(mb) || cls(a) == cls(b) {
                        continue;
                    }
                    let mut s = s1.clone();
                    s.extend(s2.iter().copied());
                    self.add_human(AtomKey::Parallel, vec![a.0, a.1, b.0, b.1, ma.0, ma.1], 1, s, (*h1).max(*h2));
                }
            }
        }
        for i in 0..perps.len() {
            for j in 0..perps.len() {
                if i == j {
                    continue;
                }
                for (l1, m1) in [(perps[i].0, perps[i].1), (perps[i].1, perps[i].0)] {
                    for (l2, m2) in [(perps[j].0, perps[j].1), (perps[j].1, perps[j].0)] {
                        let hs = [l1.0, l1.1].into_iter().find(|p| [l2.0, l2.1].contains(p));
                        let Some(h) = hs else { continue };
                        let a = if l1.0 == h { l1.1 } else { l1.0 };
                        let b = if l2.0 == h { l2.1 } else { l2.0 };
                        let sm1: BTreeSet<PointId> = [m1.0, m1.1].into_iter().collect();
                        let sm2: BTreeSet<PointId> = [m2.0, m2.1].into_iter().collect();
                        if a == b || sm1.contains(&a) || sm2.contains(&b) || !sm1.contains(&b) || !sm2.contains(&a) {
                            continue;
                        }
                        let c1: Vec<PointId> = sm1.iter().copied().filter(|&x| x != b).collect();
                        let c2: Vec<PointId> = sm2.iter().copied().filter(|&x| x != a).collect();
                        if c1.len() != 1 || c2.len() != 1 || c1[0] != c2[0] || a > b {
                            continue;
                        }
                        let c = c1[0];
                        let mut s = perps[i].3.clone();
                        s.extend(perps[j].3.iter().copied());
                        self.add_human(AtomKey::Orthocentre, vec![h, a, b, c], 1, s, perps[i].2.max(perps[j].2));
                    }
                }
            }
        }
        let groups = self.groups(radii_from);
        for (o, rs) in &groups {
            let mut m: Vec<PointId> = Vec::new();
            for r in rs {
                for p in [r.0, r.1] {
                    if !m.contains(&p) {
                        m.push(p);
                    }
                }
            }
            m.sort_unstable();
            for (l, mm, h, s) in &perps {
                for (pl, pm) in [(*l, *mm), (*mm, *l)] {
                    let pt = [pl.0, pl.1].into_iter().find(|p| [pm.0, pm.1].contains(p) && m.contains(p));
                    let Some(p) = pt else { continue };
                    let other_m = if pm.0 == p { pm.1 } else { pm.0 };
                    if other_m != *o {
                        continue;
                    }
                    let x = if pl.0 == p { pl.1 } else { pl.0 };
                    for &y in &m {
                        for &z in &m {
                            if y == p || z == p || y == z || x == y || x == z {
                                continue;
                            }
                            let ok = [p, y, z].iter().all(|&q| q == m[0] || rs.iter().any(|r| r.0 == m[0].min(q) && r.1 == m[0].max(q)));
                            if !ok {
                                continue;
                            }
                            let mut hh = *h;
                            let mut ss = s.clone();
                            for r in rs {
                                if [p, y, z].contains(&r.0) || [p, y, z].contains(&r.1) {
                                    hh = hh.max(r.2);
                                    ss.extend(r.3.iter().copied());
                                }
                            }
                            self.add_human(AtomKey::TangentChord, vec![p, x, y, z, *o], 1, ss, hh);
                        }
                    }
                }
            }
        }
        self.sweep(start);
        start
    }
}

fn fact_mentions(cx: &Ctx, f: FactId, p: PointId) -> bool {
    match &cx.t.facts[f as usize].reason {
        Reason::Concyclic(v) | Reason::Collinear(v) | Reason::Theorem(_, v) | Reason::Formula(_, _, v) => v.contains(&p),
        Reason::SimilarTriangles(a, b) => [a.0, a.1, a.2, b.0, b.1, b.2].contains(&p),
        Reason::EqualRadius(o, v) => *o == p || v.contains(&p),
        Reason::PointMerge(a, b) | Reason::TangentMerge(a, b) => *a == p || *b == p,
        Reason::TransferAddMul(a, b) | Reason::TransferArcChord(a, b) => [a.0, a.1, b.0, b.1].contains(&p),
        Reason::Assumption(_) | Reason::Construction(_) => cx.hyp_pred.get(&f).is_some_and(|q| q.points.contains(&p)),
    }
}

pub fn build_all(cx: &Ctx) -> Vec<Atom> {
    let mut b = Builder::new(cx);
    b.engine_atoms();
    let r = b.radii();
    if cx.library {
        b.theorems(r);
        b.refine(r);
    }
    b.composites(r);
    b.second_order(r);
    if cx.library {
        b.propagate_via(r);
        b.theorems(r);
        b.rejustify();
    }
    let mut map: Vec<Option<usize>> = Vec::with_capacity(b.atoms.len());
    let mut k = 0;
    for a in &b.atoms {
        if a.avail != NEVER {
            map.push(Some(k));
            k += 1;
        } else {
            map.push(None);
        }
    }
    b.atoms.retain(|a| a.avail != NEVER);
    for a in b.atoms.iter_mut() {
        let via: Option<Vec<usize>> = a.via.iter().map(|&v| map.get(v).copied().flatten()).collect();
        a.via = via.unwrap_or_default();
    }
    b.atoms
}
