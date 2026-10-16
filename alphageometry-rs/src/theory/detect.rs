use super::{of_atom, Forbid, Thm};
use crate::elimination::ANGLE_UNIT;
use crate::human::cert::Echelon;
use crate::human::model::{AtomKey, Stmt};
use crate::human::theorems::{self, config_holds, Hyp, Spec};
use crate::human::trace::{EngineTrace, Table};
use crate::lincomb::LinComb;
use crate::predicate::{PointId, Problem};
use crate::proof::Reason;
use crate::rational::Rat;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub struct Detection {
    pub thm: Thm,
    pub args: Vec<PointId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Guard {
    pub detected: Vec<Thm>,
    pub instances: Vec<Detection>,
    pub forbid: Forbid,
}

impl Guard {
    pub fn fired(&self) -> bool {
        !self.detected.is_empty()
    }
    pub fn unavoidable(&self) -> Option<Thm> {
        self.detected.iter().copied().find(|t| !t.avoidable())
    }
    pub fn primary(&self) -> Option<Thm> {
        self.detected.first().copied()
    }
}

pub const DETECT_BUDGET: Duration = Duration::from_millis(400);

pub fn guard(problem: &Problem) -> Guard {
    if std::env::var_os("GEO_NO_THEORY_GUARD").is_some() {
        return Guard::default();
    }
    let found = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::quiet_panic::quiet(|| goal_theorems(problem, Instant::now() + DETECT_BUDGET))
    }))
    .unwrap_or_default();
    let mut detected: Vec<Thm> = Vec::new();
    for d in &found {
        if !detected.contains(&d.thm) {
            detected.push(d.thm);
        }
    }
    let forbid = Forbid::closure_of(&detected);
    Guard {
        detected,
        instances: found,
        forbid,
    }
}

pub fn guard_conjunction(problem: &Problem, goals: &[crate::Predicate]) -> Guard {
    let mut all = Guard::default();
    for g in goals {
        let mut p = problem.clone();
        p.goal = Some(g.clone());
        let one = guard(&p);
        for t in one.detected {
            if !all.detected.contains(&t) {
                all.detected.push(t);
            }
        }
        all.instances.extend(one.instances);
    }
    all.forbid = Forbid::closure_of(&all.detected);
    all
}

struct Given<'t> {
    t: &'t EngineTrace,
    all: [Echelon; 4],
    flat: [Echelon; 4],
    incidence: [Echelon; 4],
    lines: Vec<Vec<PointId>>,
    circles: Vec<(Vec<PointId>, RowSet)>,
    facts: Vec<Vec<(Table, LinComb)>>,
}

type RowSet = Vec<(Table, LinComb)>;

fn canon(t: &EngineTrace, tb: Table, r: &LinComb) -> LinComb {
    crate::human::library::canon_row(t, tb, r)
}

impl<'t> Given<'t> {
    fn new(t: &'t EngineTrace) -> Given<'t> {
        let mk = || -> [Echelon; 4] {
            [
                Echelon::new(&t.lhs[0]),
                Echelon::new(&t.lhs[1]),
                Echelon::new(&t.lhs[2]),
                Echelon::new(&t.lhs[3]),
            ]
        };
        let mut all = mk();
        let mut flat = mk();
        let mut incidence = mk();
        let mut lines = Vec::new();
        let mut circles = Vec::new();
        let mut facts = Vec::new();
        for (f, fact) in t.facts.iter().enumerate() {
            let mut rows: Vec<(Table, LinComb)> = Vec::new();
            for tb in Table::ALL {
                for r in t.rows_of(tb, f as u32) {
                    rows.push((tb, canon(t, tb, r)));
                }
            }
            let circle = matches!(fact.reason, Reason::Concyclic(_));
            for (tb, c) in &rows {
                all[tb.idx()].insert(c);
                if !circle {
                    flat[tb.idx()].insert(c);
                }
            }
            match &fact.reason {
                Reason::Collinear(p) => {
                    lines.push(p.clone());
                    for (tb, c) in &rows {
                        incidence[tb.idx()].insert(c);
                    }
                }
                Reason::Concyclic(p) => circles.push((p.clone(), rows)),
                _ => {
                    if !rows.is_empty() {
                        facts.push(rows);
                    }
                }
            }
        }
        Given {
            t,
            all,
            flat,
            incidence,
            lines,
            circles,
            facts,
        }
    }

    fn has(&self, tb: Table, r: &LinComb) -> bool {
        self.t.holds(tb, r) && self.all[tb.idx()].contains(&canon(self.t, tb, r))
    }

    fn direct(&self, tb: Table, r: &LinComb) -> bool {
        if !self.has(tb, r) {
            return false;
        }
        if tb != Table::Angle {
            return true;
        }
        let c = canon(self.t, tb, r);
        if self.incidence[tb.idx()].contains(&c) {
            return true;
        }
        let one = |rows: &[(Table, LinComb)]| {
            let mut e = self.incidence[tb.idx()].clone();
            for (t2, x) in rows {
                if *t2 == tb {
                    e.insert(x);
                }
            }
            e.contains(&c)
        };
        self.facts.iter().any(|f| one(f)) || self.circles.iter().any(|(_, f)| one(f))
    }

    fn cong(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> bool {
        match (self.t.dm(a, b), self.t.dm(c, d)) {
            (Some(x), Some(y)) => (a, b) == (c, d) || self.has(Table::Ratio, &(&x - &y)),
            _ => false,
        }
    }

    fn on_line(&self, v: &[PointId]) -> bool {
        if v.len() < 3 {
            return true;
        }
        if self.lines.iter().any(|l| v.iter().all(|p| l.contains(p))) {
            return true;
        }
        let t = self.t;
        (2..v.len()).all(|i| match (t.dir(v[0], v[1]), t.dir(v[0], v[i])) {
            (Some(x), Some(y)) => self.has(Table::Angle, &t.exact(Table::Angle, &(&x - &y))),
            _ => false,
        })
    }

    fn centre_of(&self, v: &[PointId]) -> Option<PointId> {
        (0..self.t.n as PointId)
            .find(|&o| !v.contains(&o) && v[1..].iter().all(|&p| self.cong(o, v[0], o, p)))
    }

    fn on_circle(&self, v: &[PointId]) -> bool {
        self.circles
            .iter()
            .any(|c| v.iter().all(|p| c.0.contains(p)))
            || self.centre_of(v).is_some()
    }

    fn instance_rows(&self, spec: &Spec) -> Vec<(Table, LinComb)> {
        let t = self.t;
        let mut rows: Vec<(Table, LinComb)> = spec.rows.clone();
        for h in &spec.hyps {
            match h {
                Hyp::Eq(tb, r) => rows.push((*tb, r.clone())),
                Hyp::EqOrHalf(tb, r) => {
                    let mut two = r.clone();
                    two.mul_assign_scalar(&Rat::from_int(2));
                    rows.push((*tb, r.clone()));
                    rows.push((*tb, t.exact(*tb, &two)));
                }
                Hyp::Circle(v) => {
                    for (pts, crow) in &self.circles {
                        if v.iter().all(|p| pts.contains(p)) {
                            rows.extend(crow.iter().cloned());
                        }
                    }
                    if let Some(o) = self.centre_of(v) {
                        for &p in &v[1..] {
                            if let (Some(x), Some(y)) = (t.dm(o, v[0]), t.dm(o, p)) {
                                rows.push((Table::Ratio, &x - &y));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        rows
    }

    fn follows(&self, alt: &[(Table, LinComb)], rows: &[(Table, LinComb)]) -> bool {
        let t = self.t;
        let mut e = self.incidence.clone();
        for (tb, r) in rows {
            e[tb.idx()].insert(&canon(t, *tb, r));
        }
        alt.iter()
            .all(|(tb, r)| t.holds(*tb, r) && e[tb.idx()].contains(&canon(t, *tb, r)))
    }

    fn hyp_ok(&self, h: &Hyp) -> bool {
        match h {
            Hyp::Line(v) => self.on_line(v),
            Hyp::Circle(v) => self.on_circle(v),
            Hyp::Eq(tb, r) => self.direct(*tb, r),
            Hyp::EqOrHalf(tb, r) => {
                let mut two = r.clone();
                two.mul_assign_scalar(&Rat::from_int(2));
                self.direct(*tb, r) || self.direct(*tb, &self.t.exact(*tb, &two))
            }
            other => config_holds(self.t, other),
        }
    }

    fn holds_all(
        &self,
        alt: &[(Table, LinComb)],
        extra: Option<&[(Table, LinComb)]>,
        flat: bool,
    ) -> bool {
        let t = self.t;
        let mut e = if flat {
            self.flat.clone()
        } else {
            self.all.clone()
        };
        if let Some(rows) = extra {
            for (tb, r) in rows {
                e[tb.idx()].insert(&canon(t, *tb, r));
            }
        }
        alt.iter()
            .all(|(tb, r)| t.holds(*tb, r) && e[tb.idx()].contains(&canon(t, *tb, r)))
    }
}

fn goal_alts(t: &EngineTrace, s: &Stmt) -> Option<Vec<Vec<(Table, LinComb)>>> {
    match s {
        Stmt::Coll { .. } | Stmt::Cyclic { .. } | Stmt::Cong { .. } | Stmt::RatioConst { .. } => {
            let forms = crate::human::check::conclusion_forms(t, s)?;
            Some(forms.into_iter().map(|f| vec![f]).collect())
        }
        Stmt::Formula { .. } => None,
        _ => Some(vec![crate::human::check::stmt_targets(t, s)?]),
    }
}

fn distmeq_alts(t: &EngineTrace, p: &crate::Predicate) -> Option<Vec<Vec<(Table, LinComb)>>> {
    let k = p.constants.len().checked_sub(1)?;
    if k == 0 || p.points.len() != 2 * k || !p.constants[k].is_one() {
        return None;
    }
    let mut row = LinComb::zero();
    for (i, c) in p.constants[..k].iter().enumerate() {
        row.iadd_mul(&t.dm(p.points[2 * i], p.points[2 * i + 1])?, c);
    }
    (!row.is_zero()).then(|| vec![vec![(Table::Ratio, row)]])
}

struct Rows<'t>(&'t EngineTrace);

impl Rows<'_> {
    fn ang(&self, x: PointId, y: PointId, z: PointId) -> Option<LinComb> {
        Some(&self.0.dir(y, z)? - &self.0.dir(y, x)?)
    }
    fn ex(&self, c: LinComb) -> LinComb {
        self.0.exact(Table::Angle, &c)
    }
    fn perp(&self, l: (PointId, PointId), m: (PointId, PointId)) -> Option<LinComb> {
        let mut r = &self.0.dir(l.0, l.1)? - &self.0.dir(m.0, m.1)?;
        r.add_term(ANGLE_UNIT, Rat::new(-1, 2));
        Some(self.ex(r))
    }
    fn para(&self, l: (PointId, PointId), m: (PointId, PointId)) -> Option<LinComb> {
        Some(self.ex(&self.0.dir(l.0, l.1)? - &self.0.dir(m.0, m.1)?))
    }
    fn coll(&self, a: PointId, b: PointId, c: PointId) -> Option<LinComb> {
        Some(self.ex(&self.0.dir(a, b)? - &self.0.dir(a, c)?))
    }
    fn cong(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(&self.0.dm(a, b)? - &self.0.dm(c, d)?)
    }
    fn inscribed(&self, p: PointId, q: PointId, c: PointId, d: PointId) -> Option<LinComb> {
        Some(self.ex(&self.ang(p, c, q)? - &self.ang(p, d, q)?))
    }
    fn bisects(&self, v: PointId, p: PointId, q: PointId, x: PointId) -> Option<LinComb> {
        Some(self.ex(&self.ang(p, v, x)? - &self.ang(x, v, q)?))
    }
}

struct Ctx<'g, 't> {
    g: &'g Given<'t>,
    pts: Vec<PointId>,
    mids: Vec<(PointId, PointId, PointId)>,
    deadline: Instant,
}

impl Ctx<'_, '_> {
    fn late(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

fn midpoints(g: &Given, pts: &[PointId]) -> Vec<(PointId, PointId, PointId)> {
    let mut out = Vec::new();
    for l in &g.lines {
        for &m in l {
            for &a in l {
                for &b in l {
                    if a < b
                        && m != a
                        && m != b
                        && theorems::distinct(g.t, a, b)
                        && pts.contains(&m)
                        && g.cong(m, a, m, b)
                        && (g.t.dist(m, a) - g.t.dist(m, b)).abs()
                            < 1e-9 * g.t.dist(a, b).max(1e-12)
                    {
                        out.push((m, a, b));
                    }
                }
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn local_specs(cx: &Ctx, out: &mut Vec<(Thm, Vec<PointId>, Spec)>) {
    let g = cx.g;
    let t = g.t;
    let r = Rows(t);
    let tri = |a: PointId, b: PointId, c: PointId| Hyp::Triangle(a, b, c);
    let push = |out: &mut Vec<(Thm, Vec<PointId>, Spec)>,
                thm: Thm,
                args: Vec<PointId>,
                hyps: Vec<Option<Hyp>>,
                rows: Vec<Option<(Table, LinComb)>>| {
        let (Some(hyps), Some(rows)) = (
            hyps.into_iter().collect::<Option<Vec<Hyp>>>(),
            rows.into_iter().collect::<Option<Vec<(Table, LinComb)>>>(),
        ) else {
            return;
        };
        if rows.iter().any(|(tb, x)| x.is_zero() || !t.holds(*tb, x)) {
            return;
        }
        out.push((thm, args, Spec { hyps, rows }));
    };
    let mids = &cx.mids;
    for &(m, p1, p2) in mids {
        for &(n, q1, q2) in mids {
            if cx.late() {
                return;
            }
            if m >= n {
                continue;
            }
            for (p, q) in [(p1, p2), (p2, p1)] {
                for (pp, rr) in [(q1, q2), (q2, q1)] {
                    if p != pp || q == rr || t.orient(p, q, rr) == 0 {
                        continue;
                    }
                    push(
                        out,
                        Thm::Midline,
                        vec![m, n, q, rr, p],
                        vec![Some(tri(p, q, rr))],
                        vec![r.para((m, n), (q, rr)).map(|x| (Table::Angle, x))],
                    );
                }
            }
        }
    }
    let pts = &cx.pts;
    for &(m, a, b) in mids {
        for &z in pts {
            if [m, a, b].contains(&z) {
                continue;
            }
            let Some(right) = r.perp((z, a), (z, b)) else {
                continue;
            };
            if !g.direct(Table::Angle, &right) {
                continue;
            }
            push(
                out,
                Thm::ThalesConverse,
                vec![a, b, z, m],
                vec![],
                vec![r.cong(m, z, m, a).map(|x| (Table::Ratio, x))],
            );
        }
    }
    for &o in pts {
        for &p in pts {
            for &q in pts {
                if p >= q || o == p || o == q || !g.cong(o, p, o, q) {
                    continue;
                }
                let flat = t.orient(o, p, q) == 0;
                if !flat {
                    push(
                        out,
                        Thm::Isosceles,
                        vec![o, p, q],
                        vec![Some(tri(o, p, q))],
                        vec![r
                            .ang(o, p, q)
                            .zip(r.ang(p, q, o))
                            .map(|(x, y)| (Table::Angle, r.ex(&x - &y)))],
                    );
                }
                for &z in pts {
                    if [o, p, q].contains(&z) || !g.cong(o, p, o, z) {
                        continue;
                    }
                    if flat && g.on_line(&[p, o, q]) {
                        push(
                            out,
                            Thm::Thales,
                            vec![p, q, z, o],
                            vec![],
                            vec![r.perp((z, p), (z, q)).map(|x| (Table::Angle, x))],
                        );
                    } else if !flat {
                        let central = r.ang(p, o, q).zip(r.ang(p, z, q)).map(|(x, y)| {
                            let mut two = y;
                            two.mul_assign_scalar(&Rat::from_int(2));
                            (Table::Angle, r.ex(&x - &two))
                        });
                        push(
                            out,
                            Thm::CentralAngle,
                            vec![o, p, q, z],
                            vec![],
                            vec![central],
                        );
                    }
                }
            }
        }
        if cx.late() {
            return;
        }
    }
    for &x in pts {
        for &y in pts {
            if x >= y {
                continue;
            }
            for &a in pts {
                for &b in pts {
                    if a >= b
                        || [x, y].contains(&a)
                        || [x, y].contains(&b)
                        || !g.cong(x, a, x, b)
                        || !g.cong(y, a, y, b)
                    {
                        continue;
                    }
                    push(
                        out,
                        Thm::PerpBisector,
                        vec![x, y, a, b],
                        vec![Some(Hyp::Distinct(vec![x, y, a, b]))],
                        vec![r.perp((x, y), (a, b)).map(|z| (Table::Angle, z))],
                    );
                }
            }
        }
    }
    for &h in pts {
        for &a in pts {
            for &b in pts {
                for &c in pts {
                    if cx.late() {
                        return;
                    }
                    if a >= b
                        || [a, b].contains(&c)
                        || [a, b, c].contains(&h)
                        || t.orient(a, b, c) == 0
                    {
                        continue;
                    }
                    let (Some(ha), Some(hb)) = (r.perp((h, a), (b, c)), r.perp((h, b), (a, c)))
                    else {
                        continue;
                    };
                    if !g.direct(Table::Angle, &ha) || !g.direct(Table::Angle, &hb) {
                        continue;
                    }
                    push(
                        out,
                        Thm::Orthocentre,
                        vec![h, a, b, c],
                        vec![],
                        vec![r.perp((h, c), (a, b)).map(|z| (Table::Angle, z))],
                    );
                    for &o in pts {
                        if [a, b, c, h].contains(&o) || !g.cong(o, a, o, b) || !g.cong(o, b, o, c) {
                            continue;
                        }
                        for &gg in pts {
                            if [a, b, c, h, o].contains(&gg) {
                                continue;
                            }
                            let med = |v: PointId, s1: PointId, s2: PointId| {
                                mids.iter().any(|&(m, x, y)| {
                                    (x, y) == (s1.min(s2), s1.max(s2)) && g.on_line(&[v, gg, m])
                                })
                            };
                            let medians = [(a, b, c), (b, c, a), (c, a, b)]
                                .iter()
                                .filter(|(v, s1, s2)| med(*v, *s1, *s2))
                                .count();
                            if medians >= 2 {
                                push(
                                    out,
                                    Thm::EulerLine,
                                    vec![o, gg, h, a, b, c],
                                    vec![],
                                    vec![r.coll(o, gg, h).map(|z| (Table::Angle, z))],
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    for &i in pts {
        for &a in pts {
            for &b in pts {
                for &c in pts {
                    if a >= b
                        || [a, b].contains(&c)
                        || [a, b, c].contains(&i)
                        || t.orient(a, b, c) == 0
                    {
                        continue;
                    }
                    let (Some(ba), Some(bb)) = (r.bisects(a, b, c, i), r.bisects(b, a, c, i))
                    else {
                        continue;
                    };
                    if !g.direct(Table::Angle, &ba) || !g.direct(Table::Angle, &bb) {
                        continue;
                    }
                    push(
                        out,
                        Thm::Incentre,
                        vec![i, a, b, c],
                        vec![],
                        vec![r.bisects(c, a, b, i).map(|z| (Table::Angle, z))],
                    );
                }
            }
        }
    }
    let on_side = |v: PointId, w: PointId, skip: &[PointId]| -> Vec<PointId> {
        let mut found: Vec<PointId> = g
            .lines
            .iter()
            .filter(|l| l.contains(&v) && l.contains(&w))
            .flat_map(|l| l.iter().copied())
            .filter(|x| !skip.contains(x) && pts.contains(x))
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    };
    for (ia, &a) in pts.iter().enumerate() {
        for (ib, &b) in pts.iter().enumerate().skip(ia + 1) {
            for &c in &pts[ib + 1..] {
                if cx.late() {
                    return;
                }
                if t.orient(a, b, c) == 0 {
                    continue;
                }
                let tri3 = [a, b, c];
                for d in on_side(b, c, &tri3) {
                    for e in on_side(c, a, &tri3) {
                        for f in on_side(a, b, &tri3) {
                            if d == e || e == f || d == f {
                                continue;
                            }
                            let row = (|| {
                                let num = &(&t.dm(b, d)? + &t.dm(c, e)?) + &t.dm(a, f)?;
                                let den = &(&t.dm(d, c)? + &t.dm(e, a)?) + &t.dm(f, b)?;
                                Some((Table::Ratio, &num - &den))
                            })();
                            let lines = [
                                Some(Hyp::Line(vec![b, d, c])),
                                Some(Hyp::Line(vec![c, e, a])),
                                Some(Hyp::Line(vec![a, f, b])),
                            ];
                            if g.on_line(&[d, e, f]) {
                                let mut hyps = lines.to_vec();
                                hyps.push(Some(Hyp::Line(vec![d, e, f])));
                                push(
                                    out,
                                    Thm::Menelaus,
                                    vec![a, b, c, d, e, f],
                                    hyps,
                                    vec![row.clone()],
                                );
                                continue;
                            }
                            for &q in pts {
                                if [a, b, c, d, e, f].contains(&q) {
                                    continue;
                                }
                                if g.on_line(&[a, d, q])
                                    && g.on_line(&[b, e, q])
                                    && g.on_line(&[c, f, q])
                                {
                                    let mut hyps = lines.to_vec();
                                    hyps.push(Some(Hyp::Line(vec![a, d, q])));
                                    hyps.push(Some(Hyp::Line(vec![b, e, q])));
                                    hyps.push(Some(Hyp::Line(vec![c, f, q])));
                                    push(
                                        out,
                                        Thm::Ceva,
                                        vec![a, b, c, d, e, f, q],
                                        hyps,
                                        vec![row.clone()],
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let side = |v: PointId, w: PointId| -> Option<PointId> {
        mids.iter()
            .find(|&&(_, x, y)| (x, y) == (v.min(w), v.max(w)))
            .map(|m| m.0)
    };
    for (ia, &a) in pts.iter().enumerate() {
        for (ib, &b) in pts.iter().enumerate().skip(ia + 1) {
            for &c in &pts[ib + 1..] {
                if cx.late() {
                    return;
                }
                if t.orient(a, b, c) == 0 {
                    continue;
                }
                let (Some(ma), Some(mb), Some(mc)) = (side(b, c), side(c, a), side(a, b)) else {
                    continue;
                };
                for (v, w, u, mv, mw, mu) in [
                    (a, b, c, ma, mb, mc),
                    (b, c, a, mb, mc, ma),
                    (c, a, b, mc, ma, mb),
                ] {
                    for &gg in pts {
                        if [a, b, c, ma, mb, mc].contains(&gg) {
                            continue;
                        }
                        if g.on_line(&[v, gg, mv]) && g.on_line(&[w, gg, mw]) {
                            push(
                                out,
                                Thm::Centroid,
                                vec![gg, a, b, c],
                                vec![],
                                vec![r.coll(u, gg, mu).map(|z| (Table::Angle, z))],
                            );
                        }
                    }
                    for &o in pts {
                        if [a, b, c, ma, mb, mc].contains(&o) {
                            continue;
                        }
                        let (Some(p1), Some(p2)) =
                            (r.perp((o, mv), (w, u)), r.perp((o, mw), (u, v)))
                        else {
                            continue;
                        };
                        if g.direct(Table::Angle, &p1) && g.direct(Table::Angle, &p2) {
                            push(
                                out,
                                Thm::Circumcentre,
                                vec![o, a, b, c],
                                vec![],
                                vec![r.perp((o, mu), (v, w)).map(|z| (Table::Angle, z))],
                            );
                        }
                    }
                }
                for &f in pts {
                    if [a, b, c, ma, mb, mc].contains(&f) {
                        continue;
                    }
                    for (v, s1, s2, m1, m2, m3) in [
                        (a, b, c, ma, mb, mc),
                        (b, c, a, mb, mc, ma),
                        (c, a, b, mc, ma, mb),
                    ] {
                        let foot = g.on_line(&[s1, f, s2])
                            && r.perp((v, f), (s1, s2))
                                .is_some_and(|z| g.direct(Table::Angle, &z));
                        if foot {
                            push(
                                out,
                                Thm::NinePointCircle,
                                vec![m1, m2, m3, f],
                                vec![],
                                vec![r.inscribed(m1, m2, m3, f).map(|z| (Table::Angle, z))],
                            );
                        }
                    }
                }
            }
        }
    }
    for &x in pts {
        for (ip, &p) in pts.iter().enumerate() {
            for &q in &pts[ip + 1..] {
                if x == p || x == q || !g.on_line(&[x, p, q]) {
                    continue;
                }
                for &c in pts {
                    for &d in pts {
                        if c >= d || [x, p, q].contains(&c) || [x, p, q].contains(&d) {
                            continue;
                        }
                        if !g.on_line(&[x, c, d]) {
                            continue;
                        }
                        if g.on_circle(&[p, q, c, d]) {
                            let row = r
                                .cong(x, p, x, c)
                                .zip(r.cong(x, d, x, q))
                                .map(|(u, v)| (Table::Ratio, &u - &v));
                            push(
                                out,
                                Thm::PowerOfPoint,
                                vec![x, p, q, c, d],
                                vec![],
                                vec![row],
                            );
                        }
                    }
                }
                for &u in pts {
                    for &v in pts {
                        if u >= v || [x, p, q].contains(&u) || [x, p, q].contains(&v) {
                            continue;
                        }
                        if !g.on_circle(&[u, v, p, q]) {
                            continue;
                        }
                        for &c in pts {
                            for &d in pts {
                                if c >= d
                                    || [x, p, q, u, v].contains(&c)
                                    || [x, p, q, u, v].contains(&d)
                                {
                                    continue;
                                }
                                if g.on_line(&[x, c, d]) && g.on_circle(&[u, v, c, d]) {
                                    push(
                                        out,
                                        Thm::RadicalAxis,
                                        vec![x, u, v, p, q, c, d],
                                        vec![],
                                        vec![r.coll(x, u, v).map(|z| (Table::Angle, z))],
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        if cx.late() {
            return;
        }
    }
    for &p in pts {
        for &x in pts {
            if p == x {
                continue;
            }
            for &o in pts {
                if [p, x].contains(&o) {
                    continue;
                }
                let Some(tan) = r.perp((p, x), (o, p)) else {
                    continue;
                };
                if !g.direct(Table::Angle, &tan) {
                    continue;
                }
                for &y in pts {
                    for &z in pts {
                        if y == z || [p, x, o].contains(&y) || [p, x, o].contains(&z) {
                            continue;
                        }
                        if !g.cong(o, p, o, y) || !g.cong(o, p, o, z) {
                            continue;
                        }
                        let row = r.ang(p, z, y).and_then(|a| {
                            Some((Table::Angle, r.ex(&(&t.dir(p, y)? - &t.dir(p, x)?) - &a)))
                        });
                        push(
                            out,
                            Thm::TangentChord,
                            vec![p, x, y, z, o],
                            vec![],
                            vec![row],
                        );
                    }
                }
            }
        }
    }
}

fn similar(t: &EngineTrace, t1: [PointId; 3], t2: [PointId; 3]) -> Option<bool> {
    let ang = |a: PointId, v: PointId, b: PointId| theorems::interior_degrees(t, a, v, b);
    for k in 0..3 {
        let (p, q, r) = (t1[k], t1[(k + 1) % 3], t1[(k + 2) % 3]);
        let (u, v, w) = (t2[k], t2[(k + 1) % 3], t2[(k + 2) % 3]);
        if (ang(q, p, r) - ang(v, u, w)).abs() > 1e-6 {
            return None;
        }
    }
    let ratio = t.dist(t1[0], t1[1]) / t.dist(t2[0], t2[1]).max(1e-300);
    Some((ratio - 1.0).abs() < 1e-9)
}

fn triangle_specs(cx: &Ctx, goal_pts: &[PointId], out: &mut Vec<(Thm, Vec<PointId>, Spec)>) {
    let g = cx.g;
    let t = g.t;
    let r = Rows(t);
    let pts = &cx.pts;
    let mut tris: Vec<[PointId; 3]> = Vec::new();
    for (i, &a) in goal_pts.iter().enumerate() {
        for &b in &goal_pts[i + 1..] {
            for &c in pts {
                if c == a || c == b || t.orient(a, b, c) == 0 {
                    continue;
                }
                let mut v = [a, b, c];
                v.sort_unstable();
                if !tris.contains(&v) {
                    tris.push(v);
                }
            }
        }
    }
    for (i, t1) in tris.iter().enumerate() {
        if cx.late() {
            return;
        }
        for t2 in &tris[i + 1..] {
            let mut s1 = *t1;
            let mut s2 = *t2;
            s1.sort_unstable();
            s2.sort_unstable();
            if s1 == s2 {
                continue;
            }
            let perms: [[usize; 3]; 6] = [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ];
            for pm in perms {
                let u = [t2[pm[0]], t2[pm[1]], t2[pm[2]]];
                let Some(congruent) = similar(t, *t1, u) else {
                    continue;
                };
                let [p, q, c] = *t1;
                let [x, y, z] = u;
                let same = t.orient(p, q, c) == t.orient(x, y, z);
                let corner =
                    |a: PointId, v: PointId, b: PointId, aa: PointId, vv: PointId, bb: PointId| {
                        let l = r.ang(a, v, b)?;
                        let m = r.ang(aa, vv, bb)?;
                        Some(r.ex(if same { &l - &m } else { &l + &m }))
                    };
                let (Some(a0), Some(a1), Some(a2)) = (
                    corner(q, p, c, y, x, z),
                    corner(p, q, c, x, y, z),
                    corner(p, c, q, x, z, y),
                ) else {
                    continue;
                };
                let rat =
                    |a: PointId, b: PointId, cc: PointId, aa: PointId, bb: PointId, c2: PointId| {
                        Some(&(&t.dm(a, b)? - &t.dm(a, cc)?) - &(&t.dm(aa, bb)? - &t.dm(aa, c2)?))
                    };
                let (Some(r0), Some(r1), Some(r2)) = (
                    rat(p, q, c, x, y, z),
                    rat(q, p, c, y, x, z),
                    rat(c, p, q, z, x, y),
                ) else {
                    continue;
                };
                let angles = [(Table::Angle, a0), (Table::Angle, a1), (Table::Angle, a2)];
                let ratios = [(Table::Ratio, r0), (Table::Ratio, r1), (Table::Ratio, r2)];
                let have = |rw: &(Table, LinComb)| rw.1.is_zero() || g.has(rw.0, &rw.1);
                let args = vec![p, q, c, x, y, z];
                let mut emit = |thm: Thm| {
                    let mut rows: Vec<(Table, LinComb)> = Vec::new();
                    rows.extend(angles.iter().cloned());
                    rows.extend(ratios.iter().cloned());
                    let hyps: Vec<Hyp> = rows
                        .iter()
                        .filter(|rw| !rw.1.is_zero() && have(rw))
                        .map(|rw| Hyp::Eq(rw.0, rw.1.clone()))
                        .collect();
                    rows.retain(|rw| !rw.1.is_zero() && !have(rw) && t.holds(rw.0, &rw.1));
                    if !rows.is_empty() {
                        out.push((thm, args.clone(), Spec { hyps, rows }));
                    }
                };
                let n_ang = angles.iter().filter(|a| have(a)).count();
                let n_rat = ratios.iter().filter(|a| have(a)).count();
                let sas = (0..3).any(|k| have(&angles[k]) && have(&ratios[k]));
                if congruent {
                    for key in [
                        AtomKey::CongruentSss,
                        AtomKey::CongruentSas,
                        AtomKey::CongruentAsa,
                        AtomKey::CongruentRhs,
                    ] {
                        for k in 0..3 {
                            let rot = |v: &[PointId; 3]| [v[k], v[(k + 1) % 3], v[(k + 2) % 3]];
                            let (aa, bb) = (rot(t1), rot(&u));
                            let a6: Vec<PointId> = aa.iter().chain(bb.iter()).copied().collect();
                            if let Some(spec) = theorems::spec(t, key, &a6) {
                                if spec.hyps.iter().all(|h| g.hyp_ok(h)) {
                                    for &th in of_atom(key) {
                                        out.push((th, a6.clone(), spec.clone()));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    if n_ang >= 2 {
                        emit(Thm::SimilarityAa);
                    }
                    if sas {
                        emit(Thm::SimilaritySas);
                    }
                    if n_rat >= 2 {
                        emit(Thm::SimilaritySss);
                    }
                }
            }
        }
    }
}

pub fn goal_theorems(problem: &Problem, deadline: Instant) -> Vec<Detection> {
    let Some(goal) = problem.goal.as_ref() else {
        return Vec::new();
    };
    if matches!(goal.name.as_str(), "acompute" | "rcompute") {
        return Vec::new();
    }
    let mut d = crate::Ddar::new_tracked(&problem.points);
    for p in &problem.preds {
        d.force_pred(p);
    }
    let t = d.trace();
    let mut gpred = goal.clone();
    gpred.points = goal
        .points
        .iter()
        .map(|&p| t.subst.get(p as usize).copied().unwrap_or(p))
        .collect();
    let stmt = crate::human::classify::pred_stmt(&gpred);
    let alts = if gpred.name == "distmeq" {
        distmeq_alts(&t, &gpred)
    } else {
        goal_alts(&t, &stmt)
    };
    let Some(alts) = alts else {
        return Vec::new();
    };
    let alts: Vec<Vec<(Table, LinComb)>> = alts
        .into_iter()
        .filter(|a| a.iter().all(|(tb, r)| t.holds(*tb, r)))
        .collect();
    if alts.is_empty() {
        return Vec::new();
    }
    let g = Given::new(&t);
    let any = |extra: Option<&[(Table, LinComb)]>, flat: bool| {
        alts.iter().any(|a| g.holds_all(a, extra, flat))
    };
    let from = |rows: &[(Table, LinComb)]| alts.iter().any(|a| g.follows(a, rows));
    if let Stmt::Cyclic { pts } = &stmt {
        if g.on_circle(pts) {
            return Vec::new();
        }
        if g.facts.iter().any(|rows| from(rows)) {
            return vec![Detection {
                thm: Thm::InscribedConverse,
                args: pts.clone(),
            }];
        }
    } else {
        if any(None, true) {
            return Vec::new();
        }
        if g.circles.iter().any(|(_, rows)| from(rows)) {
            return vec![Detection {
                thm: Thm::InscribedAngle,
                args: gpred.points.clone(),
            }];
        }
    }
    let pts: Vec<PointId> = (0..t.n as PointId)
        .filter(|&p| t.subst.get(p as usize).copied().unwrap_or(p) == p && t.coord(p).0.is_finite())
        .collect();
    let mids = midpoints(&g, &pts);
    let cx = Ctx {
        g: &g,
        pts,
        mids,
        deadline,
    };
    let mut specs: Vec<(Thm, Vec<PointId>, Spec)> = Vec::new();
    let mut circles: Vec<Vec<PointId>> = g.circles.iter().map(|c| c.0.clone()).collect();
    for &o in &cx.pts {
        let mut groups: Vec<Vec<PointId>> = Vec::new();
        for &p in &cx.pts {
            if p == o || !theorems::distinct(&t, o, p) {
                continue;
            }
            match groups.iter_mut().find(|grp| g.cong(o, p, o, grp[0])) {
                Some(grp) => grp.push(p),
                None => groups.push(vec![p]),
            }
        }
        circles.extend(groups.into_iter().filter(|grp| grp.len() >= 3));
    }
    let fig = theorems::Figure::new(&t, g.lines.clone(), circles);
    for (key, args) in theorems::candidates(&fig, 4000) {
        if Instant::now() >= deadline {
            break;
        }
        if let Some(spec) = theorems::spec(&t, key, &args) {
            for &th in of_atom(key) {
                specs.push((th, args.clone(), spec.clone()));
            }
        }
    }
    local_specs(&cx, &mut specs);
    let mut goal_pts = gpred.points.clone();
    goal_pts.sort_unstable();
    goal_pts.dedup();
    if goal_pts.len() <= 8 && std::env::var_os("GEO_DETECT_CRITERIA").is_some() {
        triangle_specs(&cx, &goal_pts, &mut specs);
    }
    let mut out: Vec<Detection> = Vec::new();
    for (thm, args, spec) in specs {
        if Instant::now() >= deadline {
            break;
        }
        if out.iter().any(|d| d.thm == thm) || !super::detectable(thm) {
            continue;
        }
        if !spec.hyps.iter().all(|h| g.hyp_ok(h)) {
            continue;
        }
        if from(&g.instance_rows(&spec)) {
            out.push(Detection { thm, args });
        }
    }
    out
}
