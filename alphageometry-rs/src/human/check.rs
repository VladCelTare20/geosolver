use super::cert::Basis;
use super::ctx::{Ctx, FactClass};
use super::expr::{eval, is_directed};
use super::model::*;
use super::trace::{EngineTrace, Table};
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason as ER};
use crate::rational::Rat;
use std::collections::BTreeMap;

pub struct Checker<'c, 'a> {
    cx: &'c Ctx<'a>,
    bases: BTreeMap<FactId, Vec<Basis>>,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub block: u16,
    pub rule: &'static str,
    pub detail: String,
}

fn rows_of_fact(t: &EngineTrace, f: FactId) -> Vec<(Table, LinComb)> {
    let mut out = Vec::new();
    for tb in Table::ALL {
        for (g, r) in &t.rows[tb.idx()] {
            if *g == f {
                out.push((tb, r.clone()));
            }
        }
    }
    out
}

fn exact(t: &EngineTrace, tb: Table, c: LinComb) -> LinComb {
    if tb != Table::Angle {
        return c;
    }
    let mut out = c;
    let k = t.value(Table::Angle, &out).round() as i64;
    out.add_term(ANGLE_UNIT, Rat::from_int(-k));
    out
}

pub fn check_rows(t: &EngineTrace, key: AtomKey, a: &[PointId]) -> Option<Vec<(Table, LinComb)>> {
    let half = Rat::new(1, 2);
    let d = |x: PointId, y: PointId| t.dir(x, y);
    let m = |x: PointId, y: PointId| t.dm(x, y);
    let ang = |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&d(y, z)? - &d(y, x)?) };
    let one = |tb: Table, c: LinComb| vec![(tb, exact(t, tb, c))];
    Some(match key {
        AtomKey::Inscribed => {
            let mut out = Vec::new();
            for q in super::claims::subsets4(a) {
                out.push((Table::Angle, exact(t, Table::Angle, &ang(q[0], q[2], q[1])? - &ang(q[0], q[3], q[1])?)));
            }
            out
        }
        AtomKey::Thales => {
            let mut r = ang(a[0], a[2], a[1])?;
            r.add_term(ANGLE_UNIT, -&half);
            one(Table::Angle, r)
        }
        AtomKey::TangentChord => one(Table::Angle, &(&d(a[0], a[2])? - &d(a[0], a[1])?) - &ang(a[0], a[3], a[2])?),
        AtomKey::PerpBisector => {
            let mut r = &d(a[0], a[1])? - &d(a[2], a[3])?;
            r.add_term(ANGLE_UNIT, -&half);
            one(Table::Angle, r)
        }
        AtomKey::Orthocentre => {
            let mut r = &d(a[3], a[0])? - &d(a[1], a[2])?;
            r.add_term(ANGLE_UNIT, -&half);
            one(Table::Angle, r)
        }
        AtomKey::Parallel => one(Table::Angle, &d(a[0], a[1])? - &d(a[2], a[3])?),
        AtomKey::Radii => {
            let mut out = Vec::new();
            for i in 1..a.len() {
                for j in i + 1..a.len() {
                    out.push((Table::Ratio, &m(a[0], a[i])? - &m(a[0], a[j])?));
                }
            }
            out
        }
        AtomKey::Isosceles => {
            let mut out = Vec::new();
            for i in 1..a.len() {
                for j in i + 1..a.len() {
                    out.push((Table::Angle, exact(t, Table::Angle, &ang(a[0], a[i], a[j])? - &ang(a[i], a[j], a[0])?)));
                }
            }
            out
        }
        AtomKey::CentralAngle => {
            let mut two = ang(a[1], a[3], a[2])?;
            two.mul_assign_scalar(&Rat::from_int(2));
            one(Table::Angle, &ang(a[1], a[0], a[2])? - &two)
        }
        AtomKey::PowerOfPoint => vec![(Table::Ratio, &(&(&m(a[0], a[1])? + &m(a[0], a[2])?) - &m(a[0], a[3])?) - &m(a[0], a[4])?)],
        AtomKey::Midline => {
            let mut out = one(Table::Angle, &d(a[0], a[1])? - &d(a[2], a[3])?);
            if let Some(two) = t.prime_const(&Rat::from_int(2)) {
                out.push((Table::Ratio, &(&m(a[2], a[3])? - &m(a[0], a[1])?) - &two));
            }
            out
        }
        k => super::theorems::spec(t, k, a)?.rows,
    })
}

pub fn stmt_targets(t: &EngineTrace, s: &Stmt) -> Option<Vec<(Table, LinComb)>> {
    let d = |x: PointId, y: PointId| t.dir(x, y);
    let m = |x: PointId, y: PointId| t.dm(x, y);
    let ang = |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&d(y, z)? - &d(y, x)?) };
    Some(match s {
        Stmt::Coll { pts } => {
            let mut out = Vec::new();
            for j in 2..pts.len() {
                out.push((Table::Angle, &d(pts[0], pts[1])? - &d(pts[0], pts[j])?));
            }
            out
        }
        Stmt::Cyclic { pts } => {
            let mut out = Vec::new();
            for j in 3..pts.len() {
                out.push((Table::Angle, &ang(pts[0], pts[2], pts[1])? - &ang(pts[0], pts[j], pts[1])?));
            }
            out
        }
        Stmt::Perp { l1, l2 } => {
            let mut r = &d(l1.0, l1.1)? - &d(l2.0, l2.1)?;
            r.add_term(ANGLE_UNIT, Rat::new(-1, 2));
            vec![(Table::Angle, r)]
        }
        Stmt::Para { l1, l2 } => vec![(Table::Angle, &d(l1.0, l1.1)? - &d(l2.0, l2.1)?)],
        Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs } => {
            let (tb, l) = eval(t, lhs)?;
            let (_, r) = eval(t, rhs)?;
            vec![(tb, &l - &r)]
        }
        Stmt::AngleConst { angle, degrees } => {
            let (_, l) = eval(t, angle)?;
            let mut r = l;
            r.add_term(ANGLE_UNIT, -&(degrees / &Rat::from_int(180)));
            vec![(Table::Angle, r)]
        }
        Stmt::Cong { s1, s2 } => vec![(Table::Ratio, &m(s1.0, s1.1)? - &m(s2.0, s2.1)?)],
        Stmt::EqRatio { segs } if segs.len() == 4 => {
            let r = &(&m(segs[0].0, segs[0].1)? - &m(segs[1].0, segs[1].1)?) - &(&m(segs[2].0, segs[2].1)? - &m(segs[3].0, segs[3].1)?);
            vec![(Table::Ratio, r)]
        }
        Stmt::RatioConst { s1, s2, value } => vec![(Table::Ratio, &(&m(s1.0, s1.1)? - &m(s2.0, s2.1)?) - &t.prime_const(value)?)],
        Stmt::Sim { t1, t2, opposite } | Stmt::Congruent { t1, t2, opposite } => {
            let (a, b, c) = *t1;
            let (x, y, z) = *t2;
            let sg = |l: LinComb| if *opposite { l.negated() } else { l };
            vec![
                (Table::Angle, &ang(b, a, c)? - &sg(ang(y, x, z)?)),
                (Table::Angle, &ang(a, b, c)? - &sg(ang(x, y, z)?)),
                (Table::Ratio, &(&m(a, b)? - &m(a, c)?) - &(&m(x, y)? - &m(x, z)?)),
                (Table::Ratio, &(&m(b, a)? - &m(b, c)?) - &(&m(y, x)? - &m(y, z)?)),
            ]
        }
        _ => return None,
    })
}

pub fn conclusion_forms(t: &EngineTrace, s: &Stmt) -> Option<Vec<(Table, LinComb)>> {
    match s {
        Stmt::Coll { pts } => {
            let mut out = Vec::new();
            for &v in pts {
                let rest: Vec<PointId> = pts.iter().copied().filter(|&x| x != v).collect();
                for (i, &a) in rest.iter().enumerate() {
                    for &c in &rest[i + 1..] {
                        if let (Some(x), Some(y)) = (t.dir(v, a), t.dir(v, c)) {
                            out.push((Table::Angle, &x - &y));
                        }
                    }
                }
            }
            Some(out)
        }
        Stmt::Cyclic { pts } => {
            let mut out = Vec::new();
            for q in super::claims::subsets4(pts) {
                let ang = |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&t.dir(y, z)? - &t.dir(y, x)?) };
                for (a, b, c, d) in [(q[0], q[1], q[2], q[3]), (q[0], q[2], q[1], q[3]), (q[0], q[3], q[1], q[2])] {
                    if let (Some(x), Some(y)) = (ang(a, c, b), ang(a, d, b)) {
                        out.push((Table::Angle, &x - &y));
                    }
                }
            }
            Some(out)
        }
        Stmt::Cong { s1, s2 } | Stmt::RatioConst { s1, s2, .. } => {
            let mut out = stmt_targets(t, s)?;
            let k = match s {
                Stmt::RatioConst { value, .. } => value.clone(),
                _ => Rat::one(),
            };
            if let (Some(x), Some(y)) = (t.single(Table::Add, s1.0, s1.1), t.single(Table::Add, s2.0, s2.1)) {
                let mut ky = y;
                ky.mul_assign_scalar(&k);
                out.push((Table::Add, &x - &ky));
            }
            Some(out)
        }
        _ => stmt_targets(t, s),
    }
}

impl<'c, 'a> Checker<'c, 'a> {
    pub fn new(cx: &'c Ctx<'a>) -> Self {
        Checker { cx, bases: BTreeMap::new() }
    }

    fn basis(&mut self, h: FactId) -> &Vec<Basis> {
        let cx = self.cx;
        self.bases.entry(h).or_insert_with(|| {
            let mut v: Vec<Basis> = Table::ALL.iter().map(|tb| Basis::new(&cx.t.lhs[tb.idx()], false)).collect();
            for tb in Table::ALL {
                for (i, (g, r)) in cx.t.rows[tb.idx()].iter().enumerate() {
                    if *g < h && cx.in_cl.get(*g as usize).copied().unwrap_or(false) {
                        let r2 = if tb == Table::Ratio { cx.t.canon_ratio(r) } else { r.clone() };
                        v[tb.idx()].insert(i as u32, &r2);
                    }
                }
            }
            v
        })
    }

    fn proven(&mut self, h: FactId, tb: Table, row: &LinComb) -> bool {
        let t = self.cx.t;
        let row = exact(t, tb, row.clone());
        let row = if tb == Table::Ratio { t.canon_ratio(&row) } else { row };
        self.basis(h)[tb.idx()].contains(&row)
    }

    fn verified(&mut self, tb: Table, row: &LinComb) -> bool {
        let cx = self.cx;
        let row = super::library::canon_row(cx.t, tb, row);
        if self.basis(FactId::MAX)[tb.idx()].contains(&row) {
            return true;
        }
        cx.store_verified(tb, &row)
    }

    pub fn theorem_ok(&mut self, h: FactId, key: AtomKey, a: &[PointId]) -> bool {
        let t = self.cx.t;
        let Some(spec) = super::theorems::spec(t, key, a) else { return false };
        for hyp in &spec.hyps {
            let ok = match hyp {
                super::theorems::Hyp::Line(p) => self.on_line(h, p),
                super::theorems::Hyp::Circle(p) => self.on_circle(h, p),
                super::theorems::Hyp::Eq(tb, row) => t.holds(*tb, row) && (self.proven(h, *tb, row) || self.verified(*tb, row)),
                other => super::theorems::config_holds(t, other),
            };
            if !ok {
                if std::env::var_os("HP_DEBUG").is_some() {
                    eprintln!("theorem {key:?} {a:?} hyp {hyp:?} fails before {h}");
                }
                return false;
            }
        }
        spec.rows.iter().all(|(tb, row)| t.holds(*tb, row) && self.verified(*tb, row))
    }

    fn equidistant(&mut self, h: FactId, o: PointId, pts: &[PointId]) -> bool {
        let t = self.cx.t;
        for &p in &pts[1..] {
            let (Some(a), Some(b)) = (t.dm(o, pts[0]), t.dm(o, p)) else { return false };
            if !self.proven(h, Table::Ratio, &(&a - &b)) {
                return false;
            }
        }
        true
    }

    fn on_line(&self, h: FactId, pts: &[PointId]) -> bool {
        let cx = self.cx;
        cx.closure.iter().any(|&f| {
            f < h && matches!(&cx.t.facts[f as usize].reason, ER::Collinear(p) if pts.iter().all(|x| p.contains(x)))
        })
    }

    fn on_circle(&mut self, h: FactId, pts: &[PointId]) -> bool {
        let cx = self.cx;
        let by_fact = cx.closure.iter().any(|&f| f < h && matches!(&cx.t.facts[f as usize].reason, ER::Concyclic(p) if pts.iter().all(|x| p.contains(x))));
        if by_fact {
            return true;
        }
        let n = cx.t.n as PointId;
        (0..n).any(|o| !pts.contains(&o) && self.equidistant(h, o, pts))
    }

    fn perp_proven(&mut self, h: FactId, l: (PointId, PointId), m: (PointId, PointId)) -> bool {
        let t = self.cx.t;
        let (Some(a), Some(b)) = (t.dir(l.0, l.1), t.dir(m.0, m.1)) else { return false };
        let mut r = &a - &b;
        r.add_term(ANGLE_UNIT, Rat::new(-1, 2));
        self.proven(h, Table::Angle, &r)
    }

    pub fn template(&mut self, h: FactId, key: AtomKey, a: &[PointId]) -> bool {
        match key {
            AtomKey::Inscribed => a.len() >= 4 && self.on_circle(h, a),
            AtomKey::Thales => a.len() == 4 && self.equidistant(h, a[3], &[a[0], a[1], a[2]]) && self.on_line(h, &[a[0], a[3], a[1]]),
            AtomKey::TangentChord => a.len() == 5 && self.equidistant(h, a[4], &[a[0], a[2], a[3]]) && self.perp_proven(h, (a[0], a[1]), (a[0], a[4])),
            AtomKey::PerpBisector => a.len() == 4 && self.equidistant(h, a[0], &[a[2], a[3]]) && self.equidistant(h, a[1], &[a[2], a[3]]),
            AtomKey::Parallel => a.len() == 6 && self.perp_proven(h, (a[0], a[1]), (a[4], a[5])) && self.perp_proven(h, (a[2], a[3]), (a[4], a[5])),
            AtomKey::Radii | AtomKey::Isosceles => a.len() >= 3 && self.equidistant(h, a[0], &a[1..]),
            AtomKey::CentralAngle => a.len() == 4 && self.equidistant(h, a[0], &[a[1], a[2], a[3]]),
            AtomKey::PowerOfPoint => {
                a.len() == 5 && self.on_circle(h, &[a[1], a[2], a[3], a[4]]) && self.on_line(h, &[a[0], a[1], a[2]]) && self.on_line(h, &[a[0], a[3], a[4]])
            }
            AtomKey::Midline => {
                a.len() == 5
                    && self.on_line(h, &[a[0], a[4], a[2]])
                    && self.on_line(h, &[a[1], a[4], a[3]])
                    && self.equidistant(h, a[0], &[a[4], a[2]])
                    && self.equidistant(h, a[1], &[a[4], a[3]])
            }
            AtomKey::Orthocentre => a.len() == 4 && self.perp_proven(h, (a[1], a[0]), (a[2], a[3])) && self.perp_proven(h, (a[2], a[0]), (a[3], a[1])),
            k => self.theorem_ok(h, k, a),
        }
    }

    fn reason_rows(&mut self, blocks: &[Block], r: &Reason) -> Option<Vec<(Table, LinComb)>> {
        let t = self.cx.t;
        match r {
            Reason::Hyp { fact, .. } => Some(rows_of_fact(t, *fact)),
            Reason::Fact { fact, .. } => Some(rows_of_fact(t, *fact)),
            Reason::Claim { block, fact, .. } => {
                let b = blocks.iter().find(|b| b.id == *block)?;
                if !b.engine_facts.contains(fact) {
                    return None;
                }
                Some(rows_of_fact(t, *fact))
            }
            Reason::Atom { key, args, .. } => check_rows(t, *key, args),
            Reason::Engine { fact } => Some(rows_of_fact(t, *fact)),
            Reason::Lemma { stmt, .. } => Some(stmt_targets(t, stmt)?.into_iter().map(|(tb, row)| (tb, exact(t, tb, row))).collect()),
        }
    }

    fn establishes(&self, s: &Sentence, st: &Stmt) -> bool {
        let t = self.cx.t;
        match s {
            Sentence::Chain { terms, directed, then: None, .. } => {
                let (Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs }) = st else { return false };
                let (Some((tb, a)), Some((_, b)), Some((_, x)), Some((_, y))) = (eval(t, &terms[0]), eval(t, terms.last().unwrap()), eval(t, lhs), eval(t, rhs)) else { return false };
                let directed = *directed && is_directed(&terms[0]) && is_directed(terms.last().unwrap());
                self.residual_ok(tb, &(&a - &b), &(&x - &y), directed)
            }
            Sentence::Because { stmt, .. } | Sentence::Pooled { stmt, .. } => stmt == st,
            Sentence::Computation { terms, .. } => {
                let (Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs }) = st else { return false };
                terms.first() == Some(lhs) && terms.last() == Some(rhs)
            }
            _ => false,
        }
    }

    fn reason_ok(&mut self, blocks: &[Block], cur: u16, sidx: usize, h: FactId, r: &Reason) -> Result<(), String> {
        let cx = self.cx;
        match r {
            Reason::Lemma { stmt, block, sentence } => {
                if *block > cur || (*block == cur && *sentence as usize >= sidx) {
                    return Err(format!("lemma reason cites block {block} sentence {sentence} from block {cur} sentence {sidx}"));
                }
                let b = blocks.iter().find(|b| b.id == *block).ok_or("lemma reason cites a missing block")?;
                let s = b.body.get(*sentence as usize).ok_or("lemma reason cites a missing sentence")?;
                if !self.establishes(s, stmt) {
                    return Err("lemma reason cites a sentence that does not establish it".into());
                }
            }
            Reason::Hyp { fact, .. } => {
                if !matches!(cx.class.get(*fact as usize), Some(FactClass::Hyp)) || !cx.in_cl[*fact as usize] {
                    return Err(format!("hyp reason {fact} is not a hypothesis of the proof"));
                }
            }
            Reason::Claim { block, fact, .. } => {
                if *block >= cur {
                    return Err(format!("claim reason cites block {block} from block {cur}"));
                }
                if !blocks.iter().any(|b| b.id == *block && matches!(b.kind, BlockKind::Claim(_)) && b.engine_facts.contains(fact)) {
                    return Err(format!("claim reason cites block {block} which is not a claim of fact {fact}"));
                }
            }
            Reason::Fact { fact, block, because, .. } => {
                if !cx.in_cl.get(*fact as usize).copied().unwrap_or(false) || *fact >= h {
                    return Err(format!("fact reason {fact} not in the closure before {h}"));
                }
                if let Some(b) = block {
                    if *b >= cur {
                        return Err(format!("fact reason cites block {b} from block {cur}"));
                    }
                }
                if let Some(st) = match r {
                    Reason::Fact { stmt, .. } => Some(stmt),
                    _ => None,
                } {
                    if let Some(targets) = stmt_targets(cx.t, st) {
                        for (tb, row) in targets {
                            if !self.proven(fact + 1, tb, &row) {
                                return Err(format!("fact reason {fact} states something its engine fact does not give"));
                            }
                        }
                    }
                }
                for x in because {
                    self.reason_ok(blocks, cur, sidx, h, x)?;
                }
            }
            Reason::Atom { key, args, from, .. } => {
                if from.iter().any(|&b| b >= cur) {
                    return Err(format!("atom {:?} relies on a later block", key));
                }
                if !self.template(h, *key, args) {
                    return Err(format!("atom {:?} {:?}: template hypotheses not established before fact {h}", key, args));
                }
                check_rows(cx.t, *key, args).ok_or("atom rows")?;
            }
            Reason::Engine { fact } => {
                if !cx.in_cl.get(*fact as usize).copied().unwrap_or(false) {
                    return Err(format!("engine reason {fact} outside the closure"));
                }
            }
        }
        Ok(())
    }

    fn combo(&mut self, blocks: &[Block], h: FactId, reasons: &[Reason], terms: &[Term]) -> Option<(Table, LinComb)> {
        let mut out = LinComb::zero();
        let mut tb = Table::Angle;
        let mut cache: BTreeMap<u16, Vec<(Table, LinComb)>> = BTreeMap::new();
        for term in terms {
            let r = reasons.get(term.reason as usize)?;
            if !cache.contains_key(&term.reason) {
                let rows = self.reason_rows(blocks, r)?;
                cache.insert(term.reason, rows);
            }
            let (t, row) = cache[&term.reason].get(term.row as usize)?.clone();
            let theorem = matches!(r, Reason::Atom { key, .. } if super::theorems::is_theorem(*key));
            if theorem && !self.verified(t, &row) {
                return None;
            }
            if matches!(r, Reason::Atom { .. }) && !theorem && !self.proven(h, t, &row) {
                return None;
            }
            tb = t;
            out.iadd_mul(&row, &term.coef);
        }
        Some((tb, out))
    }

    fn residual_ok(&self, tb: Table, diff: &LinComb, sum: &LinComb, directed: bool) -> bool {
        let cx = self.cx;
        let d = &diff.clone() - sum;
        if tb == Table::Angle {
            let q = cx.quot.q(&d);
            if q.terms.iter().any(|(v, _)| *v != ANGLE_UNIT) {
                return false;
            }
            let c = q.get(ANGLE_UNIT);
            if directed {
                c.is_integer()
            } else {
                c.is_zero()
            }
        } else if tb == Table::Ratio {
            cx.t.canon_ratio(&d).is_zero()
        } else {
            d.is_zero()
        }
    }

    fn link_ok(&mut self, blocks: &[Block], h: FactId, a: &Expr, b: &Expr, link: &Link, directed: bool) -> Result<(), String> {
        let t = self.cx.t;
        let (tb, ea) = eval(t, a).ok_or("term does not evaluate")?;
        let (_, eb) = eval(t, b).ok_or("term does not evaluate")?;
        let (_, sum) = self.combo(blocks, h, &link.reasons, &link.combination).ok_or("link combination does not resolve, or uses an atom row outside the engine span")?;
        if link.combination.is_empty() {
            return Err("link without a combination".into());
        }
        if !self.residual_ok(tb, &(&ea - &eb), &sum, directed) {
            return Err(format!("link equation is not the stated combination of its reasons: {:?} -> {:?}", a, b));
        }
        Ok(())
    }

    fn sentence_ok(&mut self, blocks: &[Block], cur: u16, sidx: usize, h: FactId, s: &Sentence, start: Option<(&Expr, &Expr)>) -> Result<(), String> {
        let t = self.cx.t;
        match s {
            Sentence::Chain { terms, links, directed, then } => {
                if terms.len() != links.len() + 1 {
                    return Err("chain terms and links do not match".into());
                }
                for (k, l) in links.iter().enumerate() {
                    for r in &l.reasons {
                        self.reason_ok(blocks, cur, sidx, h, r)?;
                    }
                    let d = *directed && is_directed(&terms[k]) && is_directed(&terms[k + 1]);
                    self.link_ok(blocks, h, &terms[k], &terms[k + 1], l, d)?;
                }
                if let Some(st) = then {
                    let (tb, last) = eval(t, terms.last().unwrap()).ok_or("chain end")?;
                    let diff = match start {
                        None => {
                            let (_, first) = eval(t, &terms[0]).ok_or("chain start")?;
                            &first - &last
                        }
                        Some((pfirst, plast)) => {
                            let (_, pf) = eval(t, pfirst).ok_or("chain start")?;
                            let (_, pl) = eval(t, plast).ok_or("chain start")?;
                            let (_, cf) = eval(t, &terms[0]).ok_or("chain start")?;
                            let mut found: Option<LinComb> = None;
                            for sgn in [Rat::one(), Rat::from_int(-1)] {
                                let junction = LinComb::combine(&pl, &cf, &-&sgn);
                                if self.residual_ok(Table::Angle, &junction, &LinComb::zero(), true) {
                                    found = Some(LinComb::combine(&pf, &last, &-&sgn));
                                    break;
                                }
                            }
                            found.ok_or("chains do not join")?
                        }
                    };
                    let targets = conclusion_forms(t, st).ok_or("chain conclusion has no algebraic form")?;
                    let ok = targets.iter().any(|(_, x)| self.residual_ok(tb, &diff, x, true) || self.residual_ok(tb, &diff, &x.negated(), true));
                    if !ok && !targets.is_empty() {
                        let mut ok_all = false;
                        if let Stmt::Cyclic { pts } | Stmt::Coll { pts } = st {
                            ok_all = pts.len() >= 3 && self.stmt_in_span_of_chain(&diff, st);
                        }
                        if !ok_all {
                            return Err("chain endpoints do not give the stated conclusion".into());
                        }
                    }
                }
            }
            Sentence::Pooled { stmt, reasons, combination } | Sentence::Because { stmt, reasons, combination } => {
                for r in reasons {
                    self.reason_ok(blocks, cur, sidx, h, r)?;
                }
                if !combination.is_empty() {
                    let targets = conclusion_forms(t, stmt).ok_or("pooled statement has no algebraic form")?;
                    let (tb, sum) = self.combo(blocks, h, reasons, combination).ok_or("pooled combination does not resolve, or uses an atom row outside the engine span")?;
                    let ok = targets.iter().any(|(t2, x)| *t2 == tb && (self.residual_ok(tb, x, &sum, true) || self.residual_ok(tb, &x.negated(), &sum, true)));
                    if !ok {
                        let q: Vec<String> = targets.iter().map(|(_, x)| format!("{:?}", self.cx.quot.q(&(&x.clone() - &sum)))).collect();
                        return Err(format!("pooled combination does not equal the statement {:?}: residuals {:?}", stmt, q));
                    }
                }
            }
            Sentence::Computation { terms, links, .. } => {
                if terms.len() != links.len() + 1 {
                    return Err("computation terms and links do not match".into());
                }
                for (k, l) in links.iter().enumerate() {
                    for r in &l.reasons {
                        self.reason_ok(blocks, cur, sidx, h, r)?;
                    }
                    self.link_ok(blocks, h, &terms[k], &terms[k + 1], l, true)?;
                }
            }
            Sentence::Theorem { reasons, .. } => {
                for r in reasons {
                    self.reason_ok(blocks, cur, sidx, h, r)?;
                }
            }
            Sentence::Raw { engine_fact, .. } => {
                if !self.cx.in_cl.get(*engine_fact as usize).copied().unwrap_or(false) {
                    return Err("raw step outside the closure".into());
                }
            }
        }
        Ok(())
    }

    fn stmt_in_span_of_chain(&self, diff: &LinComb, st: &Stmt) -> bool {
        let t = self.cx.t;
        let Some(targets) = stmt_targets(t, st) else { return false };
        targets.len() == 1 && {
            let q1 = self.cx.quot.q(diff);
            let q2 = self.cx.quot.q(&targets[0].1);
            let strip = |c: &LinComb| -> LinComb {
                let mut x = c.clone();
                x.terms.retain(|(v, _)| *v != ANGLE_UNIT);
                x
            };
            strip(&q1) == strip(&q2) || strip(&q1) == strip(&q2.negated())
        }
    }

    fn stmt_ok(&mut self, b: &Block) -> Result<(), String> {
        let cx = self.cx;
        let t = cx.t;
        for &f in &b.engine_facts {
            if !cx.in_cl.get(f as usize).copied().unwrap_or(false) {
                return Err(format!("block cites engine fact {f} outside the closure"));
            }
        }
        if b.kind == BlockKind::Conclusion {
            let goal = super::classify::pred_stmt(&cx.goal);
            if goal != b.stmt {
                return Err("conclusion does not state the goal".into());
            }
        }
        if let Some(targets) = stmt_targets(t, &b.stmt) {
            let h = b.engine_facts.iter().map(|f| f + 1).max().unwrap_or(0).max(b.horizon + 1).max(if b.kind == BlockKind::Conclusion { cx.closure.last().map(|x| x + 1).unwrap_or(0) } else { 0 });
            for (tb, row) in targets {
                if !t.holds(tb, &row) && !t.holds(tb, &row.negated()) {
                    return Err("block statement does not hold in the figure".into());
                }
                if !self.proven(h, tb, &row) {
                    return Err("block statement is not a consequence of the engine facts it re-presents".into());
                }
            }
        }
        Ok(())
    }
}

fn same_angle(a: Option<&Expr>, b: Option<&Expr>) -> bool {
    fn strip(e: &Expr) -> Expr {
        match e {
            Expr::Angle { a, b, c, .. } => Expr::Angle { a: *a, b: *b, c: *c, directed: true },
            Expr::LineAngle { l1, l2, .. } => Expr::LineAngle { l1: *l1, l2: *l2, directed: true },
            Expr::Lin { terms } => Expr::Lin { terms: terms.iter().map(|(k, x)| (k.clone(), strip(x))).collect() },
            x => x.clone(),
        }
    }
    match (a, b) {
        (Some(x), Some(y)) => strip(x) == strip(y),
        _ => false,
    }
}

pub fn violations(cx: &Ctx, hp: &HumanProof) -> Vec<Violation> {
    let mut ch = Checker::new(cx);
    let mut out = Vec::new();
    let blocks = hp.blocks.clone();
    for (i, b) in blocks.iter().enumerate() {
        if b.id as usize != i + 1 {
            out.push(Violation { block: b.id, rule: "I5", detail: "block ids are not in order".into() });
        }
        if let Err(e) = ch.stmt_ok(b) {
            out.push(Violation { block: b.id, rule: "I1", detail: e });
        }
        let h = if b.kind == BlockKind::Conclusion { b.horizon.max(cx.closure.last().map(|x| x + 1).unwrap_or(0)) } else { b.horizon };
        let mut prev: Option<(Expr, Expr)> = None;
        for (sidx, s) in b.body.iter().enumerate() {
            let joins = |terms: &Vec<Expr>| -> bool {
                let Some(p) = prev.as_ref() else { return false };
                if same_angle(Some(&p.1), terms.first()) {
                    return true;
                }
                match (eval(cx.t, &p.1), terms.first().and_then(|e| eval(cx.t, e))) {
                    (Some((Table::Angle, x)), Some((Table::Angle, y))) => [&x - &y, &x + &y].iter().any(|d| {
                        let q = cx.quot.q(d);
                        q.terms.iter().all(|(v, _)| *v == ANGLE_UNIT) && q.get(ANGLE_UNIT).is_integer()
                    }),
                    _ => false,
                }
            };
            let st = match s {
                Sentence::Chain { terms, .. } if joins(terms) => prev.as_ref().map(|p| (&p.0, &p.1)),
                _ => None,
            };
            if let Err(e) = ch.sentence_ok(&blocks, b.id, sidx, h, s, st) {
                out.push(Violation { block: b.id, rule: "I2", detail: e });
            }
            prev = match s {
                Sentence::Chain { terms, then: None, .. } => Some((terms[0].clone(), terms[terms.len() - 1].clone())),
                _ => None,
            };
        }
    }
    if !blocks.last().is_some_and(|b| b.kind == BlockKind::Conclusion) {
        out.push(Violation { block: 0, rule: "I5", detail: "the last block does not prove the goal".into() });
    }
    out
}

pub fn check(cx: &Ctx, hp: &mut HumanProof, strict: bool) -> usize {
    let first = violations(cx, hp);
    if first.is_empty() {
        return 0;
    }
    if std::env::var_os("HP_DEBUG").is_some() {
        for v in &first {
            eprintln!("violation block {} {}: {}", v.block, v.rule, v.detail);
            if let Some(b) = hp.blocks.iter().find(|b| b.id == v.block) {
                eprintln!("  stmt {:?}\n  facts {:?}\n  body {:?}", b.stmt, b.engine_facts, b.body);
            }
        }
    }
    if strict {
        panic!("human proof checker: {:?}", first);
    }
    let mut v = first.clone();
    for _ in 0..32 {
        if v.is_empty() {
            break;
        }
        let bad: std::collections::BTreeSet<u16> = v.iter().map(|x| x.block).collect();
        if bad.contains(&0) || hp.blocks.iter().any(|b| b.kind == BlockKind::Conclusion && bad.contains(&b.id)) {
            hp.blocks.retain(|b| b.kind != BlockKind::Conclusion);
            break;
        }
        let demoted: std::collections::BTreeSet<u16> = hp.blocks.iter().filter(|b| bad.contains(&b.id) && matches!(b.kind, BlockKind::Claim(_))).map(|b| b.id).collect();
        for b in hp.blocks.iter_mut() {
            if bad.contains(&b.id) {
                b.kind = BlockKind::Raw;
                if let Some(&f) = b.engine_facts.first() {
                    b.stmt = super::classify::fact_stmt(cx, f);
                }
                b.body = b
                    .engine_facts
                    .iter()
                    .map(|&f| Sentence::Raw { engine_fact: f, cites: cx.t.facts[f as usize].premises.clone() })
                    .collect();
            }
        }
        if !demoted.is_empty() {
            uncite(hp, &demoted);
        }
        v = violations(cx, hp);
    }
    first.len()
}

fn visit_reasons(hp: &mut HumanProof, f: &mut dyn FnMut(&mut Reason)) {
    fn go(r: &mut Reason, f: &mut dyn FnMut(&mut Reason)) {
        f(r);
        if let Reason::Fact { because, .. } = r {
            for x in because.iter_mut() {
                go(x, f);
            }
        }
    }
    for b in hp.blocks.iter_mut() {
        for s in b.body.iter_mut() {
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
    }
}

fn uncite(hp: &mut HumanProof, demoted: &std::collections::BTreeSet<u16>) {
    visit_reasons(hp, &mut |r| {
        if let Reason::Claim { block, fact, .. } = r {
            if demoted.contains(block) {
                *r = Reason::Engine { fact: *fact };
            }
        }
    });
    let mut renumber: std::collections::BTreeMap<u16, u16> = std::collections::BTreeMap::new();
    let mut k = 0u16;
    for b in hp.blocks.iter_mut() {
        if let BlockKind::Claim(n) = &mut b.kind {
            k += 1;
            *n = k;
            renumber.insert(b.id, k);
        }
    }
    visit_reasons(hp, &mut |r| {
        if let Reason::Claim { block, n, .. } = r {
            if let Some(&k) = renumber.get(block) {
                *n = k;
            }
        }
    });
}
