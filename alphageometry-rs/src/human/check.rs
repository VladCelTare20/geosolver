use super::barem::const_part;
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let ang =
        |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&d(y, z)? - &d(y, x)?) };
    let one = |tb: Table, c: LinComb| vec![(tb, exact(t, tb, c))];
    Some(match key {
        AtomKey::Inscribed => {
            let mut out = Vec::new();
            for q in super::claims::subsets4(a) {
                out.push((
                    Table::Angle,
                    exact(
                        t,
                        Table::Angle,
                        &ang(q[0], q[2], q[1])? - &ang(q[0], q[3], q[1])?,
                    ),
                ));
            }
            out
        }
        AtomKey::Thales => {
            let mut r = ang(a[0], a[2], a[1])?;
            r.add_term(ANGLE_UNIT, -&half);
            one(Table::Angle, r)
        }
        AtomKey::TangentChord => one(
            Table::Angle,
            &(&d(a[0], a[2])? - &d(a[0], a[1])?) - &ang(a[0], a[3], a[2])?,
        ),
        AtomKey::PerpBisector | AtomKey::Reflection => {
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
                    out.push((
                        Table::Angle,
                        exact(
                            t,
                            Table::Angle,
                            &ang(a[0], a[i], a[j])? - &ang(a[i], a[j], a[0])?,
                        ),
                    ));
                }
            }
            out
        }
        AtomKey::CentralAngle => {
            let mut two = ang(a[1], a[3], a[2])?;
            two.mul_assign_scalar(&Rat::from_int(2));
            one(Table::Angle, &ang(a[1], a[0], a[2])? - &two)
        }
        AtomKey::PowerOfPoint => vec![(
            Table::Ratio,
            &(&(&m(a[0], a[1])? + &m(a[0], a[2])?) - &m(a[0], a[3])?) - &m(a[0], a[4])?,
        )],
        AtomKey::Midline => {
            let mut out = one(Table::Angle, &d(a[0], a[1])? - &d(a[2], a[3])?);
            if let Some(two) = t.prime_const(&Rat::from_int(2)) {
                out.push((Table::Ratio, &(&m(a[2], a[3])? - &m(a[0], a[1])?) - &two));
            }
            out
        }
        k if k.is_identity_label() || k.is_config_label() => Vec::new(),
        AtomKey::RadiusChord => {
            if a.len() != 4 {
                return None;
            }
            let mut r = &ang(a[0], a[1], a[2])? - &ang(a[1], a[3], a[2])?;
            r.add_term(ANGLE_UNIT, -&half);
            one(Table::Angle, r)
        }
        AtomKey::CyclicOpposite | AtomKey::CyclicExterior => {
            if a.len() != 4 {
                return None;
            }
            one(
                Table::Angle,
                &ang(a[1], a[0], a[3])? - &ang(a[1], a[2], a[3])?,
            )
        }
        k => super::theorems::spec(t, k, a)?.rows,
    })
}

pub fn stmt_targets(t: &EngineTrace, s: &Stmt) -> Option<Vec<(Table, LinComb)>> {
    let d = |x: PointId, y: PointId| t.dir(x, y);
    let m = |x: PointId, y: PointId| t.dm(x, y);
    let ang =
        |x: PointId, y: PointId, z: PointId| -> Option<LinComb> { Some(&d(y, z)? - &d(y, x)?) };
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
                out.push((
                    Table::Angle,
                    &ang(pts[0], pts[2], pts[1])? - &ang(pts[0], pts[j], pts[1])?,
                ));
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
            let r = &(&m(segs[0].0, segs[0].1)? - &m(segs[1].0, segs[1].1)?)
                - &(&m(segs[2].0, segs[2].1)? - &m(segs[3].0, segs[3].1)?);
            vec![(Table::Ratio, r)]
        }
        Stmt::RatioConst { s1, s2, value } => vec![(
            Table::Ratio,
            &(&m(s1.0, s1.1)? - &m(s2.0, s2.1)?) - &t.prime_const(value)?,
        )],
        Stmt::Sim { t1, t2, opposite } | Stmt::Congruent { t1, t2, opposite } => {
            let (a, b, c) = *t1;
            let (x, y, z) = *t2;
            let sg = |l: LinComb| if *opposite { l.negated() } else { l };
            vec![
                (Table::Angle, &ang(b, a, c)? - &sg(ang(y, x, z)?)),
                (Table::Angle, &ang(a, b, c)? - &sg(ang(x, y, z)?)),
                (
                    Table::Ratio,
                    &(&m(a, b)? - &m(a, c)?) - &(&m(x, y)? - &m(x, z)?),
                ),
                (
                    Table::Ratio,
                    &(&m(b, a)? - &m(b, c)?) - &(&m(y, x)? - &m(y, z)?),
                ),
            ]
        }
        _ => return None,
    })
}

pub fn reason_rows(t: &EngineTrace, blocks: &[Block], r: &Reason) -> Option<Vec<(Table, LinComb)>> {
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
        Reason::Lemma { stmt, .. } => Some(
            stmt_targets(t, stmt)?
                .into_iter()
                .map(|(tb, row)| (tb, exact(t, tb, row)))
                .collect(),
        ),
    }
}

pub fn residual_ok(cx: &Ctx, tb: Table, diff: &LinComb, sum: &LinComb, directed: bool) -> bool {
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

pub fn establishes(cx: &Ctx, s: &Sentence, st: &Stmt) -> bool {
    let t = cx.t;
    if let (
        Sentence::Chain {
            terms,
            directed,
            then: None,
            ..
        },
        Stmt::AngleConst { angle, degrees },
    ) = (s, st)
    {
        if !terms
            .iter()
            .any(|e| matches!(e, Expr::Const { degrees: d } if d == degrees))
        {
            return false;
        }
        let Some((tb, x)) = eval(t, angle) else {
            return false;
        };
        return [&terms[0], &terms[terms.len() - 1]].iter().any(|e| {
            let dir = *directed && is_directed(e) && is_directed(angle);
            eval(t, e).is_some_and(|(tb2, y)| {
                tb2 == tb && residual_ok(cx, tb, &(&x - &y), &LinComb::zero(), dir)
            })
        });
    }
    match s {
        Sentence::Chain {
            terms,
            directed,
            then: None,
            ..
        } => {
            let (Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs }) = st else {
                return false;
            };
            let (Some((tb, a)), Some((_, b)), Some((_, x)), Some((_, y))) = (
                eval(t, &terms[0]),
                eval(t, terms.last().unwrap()),
                eval(t, lhs),
                eval(t, rhs),
            ) else {
                return false;
            };
            let directed =
                *directed && is_directed(&terms[0]) && is_directed(terms.last().unwrap());
            if residual_ok(cx, tb, &(&a - &b), &(&x - &y), directed) {
                return true;
            }
            let right = terms[1..terms.len() - 1]
                .iter()
                .any(|e| matches!(e, Expr::Const { degrees } if *degrees == Rat::from_int(90)));
            if !right || tb != Table::Angle {
                return false;
            }
            let mut both = &a + &b;
            both.add_term(ANGLE_UNIT, Rat::from_int(-1));
            residual_ok(cx, tb, &both, &(&x - &y), directed)
        }
        Sentence::Because { stmt, .. } | Sentence::Pooled { stmt, .. } => stmt == st,
        Sentence::Theorem {
            key,
            stmt: Stmt::Cyclic { pts },
            ..
        } if key.is_converse() || *key == TheoremKey::CircleMerge => {
            matches!(st, Stmt::Cyclic { pts: p2 } if same_set(pts, p2))
        }
        Sentence::Chain { then: Some(t2), .. }
            if matches!(
                st,
                Stmt::Coll { .. } | Stmt::Para { .. } | Stmt::Perp { .. }
            ) =>
        {
            t2 == st
        }
        Sentence::Theorem { key, stmt, .. } if key.is_line_converse() => stmt == st,
        Sentence::Computation { terms, .. } => {
            let (Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs }) = st else {
                return false;
            };
            terms.first() == Some(lhs) && terms.last() == Some(rhs)
        }
        _ => false,
    }
}

pub fn same_set(a: &[PointId], b: &[PointId]) -> bool {
    a.len() == b.len() && a.iter().all(|p| b.contains(p)) && b.iter().all(|p| a.contains(p))
}

pub fn sides(t: &EngineTrace, p: PointId, q: PointId, x: PointId, y: PointId) -> Option<bool> {
    let (a, b) = (t.orient(x, y, p), t.orient(x, y, q));
    (a != 0 && b != 0).then_some(a == b)
}

fn plain_angle(e: &Expr) -> Option<(PointId, PointId, PointId, bool)> {
    match e {
        Expr::Angle { a, b, c, directed } => Some((*a, *b, *c, *directed)),
        _ => None,
    }
}

fn side_label(
    reasons: &[Reason],
    key: AtomKey,
    p: PointId,
    q: PointId,
    x: PointId,
    y: PointId,
) -> bool {
    reasons.iter().any(|r| matches!(r, Reason::Atom { key: k, args, .. } if *k == key && args.len() == 4 && same_set(&args[0..2], &[p, q]) && same_set(&args[2..4], &[x, y])))
}

fn exterior_ok(cx: &Ctx, lhs: &Expr, rhs: &Expr) -> Option<(PointId, PointId, PointId, PointId)> {
    let (Some((a, p, c, false)), Some((x, q, y, false))) = (plain_angle(lhs), plain_angle(rhs))
    else {
        return None;
    };
    for (u, keep) in [(a, c), (c, a)] {
        if u != x && u != y {
            continue;
        }
        let w = if u == x { y } else { x };
        let line = match (cx.t.dir(p, keep), cx.t.dir(p, w)) {
            (Some(d1), Some(d2)) => cx
                .quot
                .q(&(&d1 - &d2))
                .terms
                .iter()
                .all(|(v, _)| *v == ANGLE_UNIT),
            _ => false,
        };
        if line && super::barem::opposite_ray(cx, p, keep, w) {
            return Some((p, q, x, y));
        }
    }
    None
}

pub fn converse_ok(
    cx: &Ctx,
    key: TheoremKey,
    stmt: &Stmt,
    reasons: &[Reason],
) -> Result<(), String> {
    let Stmt::Cyclic { pts } = stmt else {
        return Err("a converse of the inscribed angle theorem concludes a concyclicity".into());
    };
    let Some(Reason::Lemma { stmt: ls, .. }) = reasons.first() else {
        return Err("a converse step cites the angle equality it uses first".into());
    };
    if pts.len() != 4 {
        return Err("a converse step concludes four concyclic points".into());
    }
    let (p, q, x, y, directed) = match (key, ls) {
        (TheoremKey::InscribedConverse, Stmt::EqAngle { lhs, rhs }) => {
            let (Some((x1, p, y1, d1)), Some((x2, q, y2, d2))) =
                (plain_angle(lhs), plain_angle(rhs))
            else {
                return Err("the converse equality is not two inscribed angles".into());
            };
            let chord_ok = if d1 && d2 {
                x1 == x2 && y1 == y2
            } else {
                !d1 && !d2 && same_set(&[x1, y1], &[x2, y2])
            };
            if !chord_ok {
                return Err("the two angles do not stand on one chord".into());
            }
            (p, q, x1, y1, d1)
        }
        (TheoremKey::ExteriorAngleConverse, Stmt::EqAngle { lhs, rhs }) => {
            let Some((p, q, x, y)) = exterior_ok(cx, lhs, rhs) else {
                return Err("the exterior angle does not extend a side of the quadrilateral along a proven line".into());
            };
            (p, q, x, y, false)
        }
        (TheoremKey::ThalesConverse, Stmt::AngleConst { angle: e1, degrees })
            if *degrees == Rat::from_int(90) =>
        {
            let Some(Reason::Lemma {
                stmt:
                    Stmt::AngleConst {
                        angle: e2,
                        degrees: d2,
                    },
                ..
            }) = reasons.get(1)
            else {
                return Err("the converse of Thales' theorem cites two right angles".into());
            };
            let (Some((x1, p, y1, false)), Some((x2, q, y2, false))) =
                (plain_angle(e1), plain_angle(e2))
            else {
                return Err("Thales: two figure angles".into());
            };
            if *d2 != Rat::from_int(90) || !same_set(&[x1, y1], &[x2, y2]) {
                return Err("Thales: two right angles on one segment".into());
            }
            (p, q, x1, y1, true)
        }
        (
            TheoremKey::OppositeAnglesConverse,
            Stmt::Eq {
                lhs: Expr::Lin { terms },
                rhs: Expr::Const { degrees },
            },
        ) if *degrees == Rat::from_int(180) => {
            let [(k1, e1), (k2, e2)] = terms.as_slice() else {
                return Err("opposite angles: two angles".into());
            };
            let (Some((x1, p, y1, false)), Some((x2, q, y2, false))) =
                (plain_angle(e1), plain_angle(e2))
            else {
                return Err("opposite angles: two figure angles".into());
            };
            if !k1.is_one() || !k2.is_one() || !same_set(&[x1, y1], &[x2, y2]) {
                return Err("opposite angles: one chord, unit coefficients".into());
            }
            (p, q, x1, y1, false)
        }
        _ => return Err("converse step with an equality of the wrong shape".into()),
    };
    if p == q
        || [p, q].contains(&x)
        || [p, q].contains(&y)
        || x == y
        || !same_set(&[p, q, x, y], pts)
    {
        return Err("the converse equality is not about the four concyclic points".into());
    }
    if !directed {
        let want = if key == TheoremKey::InscribedConverse {
            AtomKey::SameSide
        } else {
            AtomKey::OppositeSides
        };
        if super::check::sides(cx.t, p, q, x, y) != Some(want == AtomKey::SameSide) {
            return Err("the side condition does not hold in the figure".into());
        }
        if !side_label(reasons, want, p, q, x, y) {
            return Err("an undirected converse step must state on which sides of the chord the two vertices lie".into());
        }
    }
    Ok(())
}

pub fn merge_ok(stmt: &Stmt, reasons: &[Reason]) -> Result<(), String> {
    let Stmt::Cyclic { pts } = stmt else {
        return Err("a circle merge concludes a concyclicity".into());
    };
    let mut sets: Vec<Vec<PointId>> = Vec::new();
    for r in reasons {
        match r {
            Reason::Lemma {
                stmt: Stmt::Cyclic { pts: q },
                ..
            }
            | Reason::Hyp {
                stmt: Stmt::Cyclic { pts: q },
                ..
            }
            | Reason::Fact {
                stmt: Stmt::Cyclic { pts: q },
                ..
            } => sets.push(q.clone()),
            _ => return Err("a circle merge cites only concyclicities".into()),
        }
    }
    let Some(first) = sets.first().cloned() else {
        return Err("empty circle merge".into());
    };
    let mut have = first;
    let mut used = vec![false; sets.len()];
    used[0] = true;
    loop {
        let next = (0..sets.len())
            .find(|&i| !used[i] && sets[i].iter().filter(|p| have.contains(p)).count() >= 3);
        let Some(i) = next else { break };
        used[i] = true;
        for &p in &sets[i] {
            if !have.contains(&p) {
                have.push(p);
            }
        }
    }
    if used.iter().any(|u| !u) || !same_set(&have, pts) {
        return Err(
            "the merged circles do not share three points or do not give the stated points".into(),
        );
    }
    Ok(())
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
                let ang = |x: PointId, y: PointId, z: PointId| -> Option<LinComb> {
                    Some(&t.dir(y, z)? - &t.dir(y, x)?)
                };
                for (a, b, c, d) in [
                    (q[0], q[1], q[2], q[3]),
                    (q[0], q[2], q[1], q[3]),
                    (q[0], q[3], q[1], q[2]),
                ] {
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
            if let (Some(x), Some(y)) = (
                t.single(Table::Add, s1.0, s1.1),
                t.single(Table::Add, s2.0, s2.1),
            ) {
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
        Checker {
            cx,
            bases: BTreeMap::new(),
        }
    }

    fn basis(&mut self, h: FactId) -> &Vec<Basis> {
        let cx = self.cx;
        self.bases.entry(h).or_insert_with(|| {
            let mut v: Vec<Basis> = Table::ALL
                .iter()
                .map(|tb| Basis::new(&cx.t.lhs[tb.idx()], false))
                .collect();
            for tb in Table::ALL {
                for (i, (g, r)) in cx.t.rows[tb.idx()].iter().enumerate() {
                    if *g < h && cx.in_cl.get(*g as usize).copied().unwrap_or(false) {
                        let r2 = if tb == Table::Ratio {
                            cx.t.canon_ratio(r)
                        } else {
                            r.clone()
                        };
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
        let row = if tb == Table::Ratio {
            t.canon_ratio(&row)
        } else {
            row
        };
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
        let Some(spec) = super::theorems::spec(t, key, a) else {
            return false;
        };
        for hyp in &spec.hyps {
            let ok = match hyp {
                super::theorems::Hyp::Line(p) => self.on_line(h, p),
                super::theorems::Hyp::Circle(p) => self.on_circle(h, p),
                super::theorems::Hyp::Eq(tb, row) => {
                    t.holds(*tb, row) && (self.proven(h, *tb, row) || self.verified(*tb, row))
                }
                super::theorems::Hyp::EqOrHalf(tb, row) => {
                    let mut two = row.clone();
                    two.mul_assign_scalar(&Rat::from_int(2));
                    let two = exact(t, *tb, two);
                    t.holds(*tb, row)
                        && (self.proven(h, *tb, row)
                            || self.verified(*tb, row)
                            || self.proven(h, *tb, &two)
                            || self.verified(*tb, &two))
                }
                other => super::theorems::config_holds(t, other),
            };
            if !ok {
                if std::env::var_os("HP_DEBUG").is_some() {
                    eprintln!("theorem {key:?} {a:?} hyp {hyp:?} fails before {h}");
                }
                return false;
            }
        }
        spec.rows.iter().all(|(tb, row)| {
            t.holds(*tb, row) && (super::theorems::is_identity(key) || self.verified(*tb, row))
        })
    }

    fn equidistant(&mut self, h: FactId, o: PointId, pts: &[PointId]) -> bool {
        let t = self.cx.t;
        for &p in &pts[1..] {
            let (Some(a), Some(b)) = (t.dm(o, pts[0]), t.dm(o, p)) else {
                return false;
            };
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
        let (Some(a), Some(b)) = (t.dir(l.0, l.1), t.dir(m.0, m.1)) else {
            return false;
        };
        let mut r = &a - &b;
        r.add_term(ANGLE_UNIT, Rat::new(-1, 2));
        self.proven(h, Table::Angle, &r)
    }

    fn reflex_inscribed(&self, a: &[PointId]) -> bool {
        let t = self.cx.t;
        let inscribed = super::theorems::interior_degrees(t, a[1], a[3], a[2]);
        let central = super::theorems::interior_degrees(t, a[1], a[0], a[2]);
        (inscribed - (180.0 - central / 2.0)).abs() < 1e-6
    }

    pub fn template(&mut self, h: FactId, key: AtomKey, a: &[PointId]) -> bool {
        match key {
            AtomKey::Inscribed => a.len() >= 4 && self.on_circle(h, a),
            AtomKey::Thales => {
                a.len() == 4
                    && self.equidistant(h, a[3], &[a[0], a[1], a[2]])
                    && self.on_line(h, &[a[0], a[3], a[1]])
            }
            AtomKey::TangentChord => {
                a.len() == 5
                    && self.equidistant(h, a[4], &[a[0], a[2], a[3]])
                    && self.perp_proven(h, (a[0], a[1]), (a[0], a[4]))
            }
            AtomKey::PerpBisector => {
                a.len() == 4
                    && self.equidistant(h, a[0], &[a[2], a[3]])
                    && self.equidistant(h, a[1], &[a[2], a[3]])
            }
            AtomKey::Reflection => {
                a.len() == 4
                    && super::labels::reflection_args(self.cx, a).as_deref() == Some(a)
                    && self.equidistant(h, a[0], &[a[2], a[3]])
                    && self.equidistant(h, a[1], &[a[2], a[3]])
            }
            AtomKey::OnSegment => super::labels::on_segment_ok(self.cx, a),
            AtomKey::AlternateAngles | AtomKey::CorrespondingAngles => {
                super::labels::parallel_kind(self.cx, a) == Some(key)
            }
            AtomKey::CommonSide => a.len() == 2 && a[0] != a[1],
            AtomKey::Parallel => {
                a.len() == 6
                    && self.perp_proven(h, (a[0], a[1]), (a[4], a[5]))
                    && self.perp_proven(h, (a[2], a[3]), (a[4], a[5]))
            }
            AtomKey::Radii | AtomKey::Isosceles => {
                a.len() >= 3 && self.equidistant(h, a[0], &a[1..])
            }
            AtomKey::RadiusChord => a.len() == 4 && self.equidistant(h, a[0], &[a[1], a[2], a[3]]),
            AtomKey::CentralAngle => {
                (a.len() == 4 || (a.len() == 5 && a[4] == a[3] && self.reflex_inscribed(a)))
                    && self.equidistant(h, a[0], &[a[1], a[2], a[3]])
            }
            AtomKey::PowerOfPoint => {
                a.len() == 5
                    && self.on_circle(h, &[a[1], a[2], a[3], a[4]])
                    && self.on_line(h, &[a[0], a[1], a[2]])
                    && self.on_line(h, &[a[0], a[3], a[4]])
            }
            AtomKey::Midline => {
                a.len() == 5
                    && !super::barem::near_line(self.cx, a[2], a[3], a[4])
                    && self.on_line(h, &[a[0], a[4], a[2]])
                    && self.on_line(h, &[a[1], a[4], a[3]])
                    && self.equidistant(h, a[0], &[a[4], a[2]])
                    && self.equidistant(h, a[1], &[a[4], a[3]])
            }
            AtomKey::Orthocentre => {
                a.len() == 4
                    && self.perp_proven(h, (a[1], a[0]), (a[2], a[3]))
                    && self.perp_proven(h, (a[2], a[0]), (a[3], a[1]))
            }
            k if k.is_identity_label() => true,
            AtomKey::SameSide | AtomKey::OppositeSides => {
                a.len() == 4
                    && sides(self.cx.t, a[0], a[1], a[2], a[3]) == Some(key == AtomKey::SameSide)
            }
            AtomKey::CyclicOpposite | AtomKey::CyclicExterior => {
                a.len() == 4
                    && sides(self.cx.t, a[0], a[2], a[1], a[3]) == Some(false)
                    && self.on_circle(h, a)
            }
            k => self.theorem_ok(h, k, a),
        }
    }

    fn reason_rows(&mut self, blocks: &[Block], r: &Reason) -> Option<Vec<(Table, LinComb)>> {
        reason_rows(self.cx.t, blocks, r)
    }

    fn establishes(&self, s: &Sentence, st: &Stmt) -> bool {
        establishes(self.cx, s, st)
    }

    fn reason_ok(
        &mut self,
        blocks: &[Block],
        cur: u16,
        sidx: usize,
        h: FactId,
        r: &Reason,
    ) -> Result<(), String> {
        let cx = self.cx;
        match r {
            Reason::Lemma {
                stmt,
                block,
                sentence,
            } => {
                if *block > cur || (*block == cur && *sentence as usize >= sidx) {
                    return Err(format!("lemma reason cites block {block} sentence {sentence} from block {cur} sentence {sidx}"));
                }
                let b = blocks
                    .iter()
                    .find(|b| b.id == *block)
                    .ok_or("lemma reason cites a missing block")?;
                let s = b
                    .body
                    .get(*sentence as usize)
                    .ok_or("lemma reason cites a missing sentence")?;
                if !self.establishes(s, stmt) {
                    if std::env::var_os("HP_DEBUG").is_some() {
                        eprintln!("lemma {stmt:?} vs sentence {s:?}");
                    }
                    return Err("lemma reason cites a sentence that does not establish it".into());
                }
            }
            Reason::Hyp { fact, stmt } => {
                if !matches!(cx.class.get(*fact as usize), Some(FactClass::Hyp))
                    || !cx.in_cl[*fact as usize]
                {
                    return Err(format!(
                        "hyp reason {fact} is not a hypothesis of the proof"
                    ));
                }
                if let Some(targets) = stmt_targets(cx.t, stmt) {
                    for (tb, row) in targets {
                        if !self.proven(fact + 1, tb, &row) && !self.proven(h, tb, &row) {
                            return Err(format!("hyp reason {fact} states something the hypothesis does not give: {stmt:?}"));
                        }
                    }
                }
            }
            Reason::Claim { block, fact, .. } => {
                if *block >= cur {
                    return Err(format!("claim reason cites block {block} from block {cur}"));
                }
                if !blocks.iter().any(|b| {
                    b.id == *block
                        && matches!(b.kind, BlockKind::Claim(_))
                        && b.engine_facts.contains(fact)
                }) {
                    return Err(format!(
                        "claim reason cites block {block} which is not a claim of fact {fact}"
                    ));
                }
            }
            Reason::Fact {
                fact,
                block,
                because,
                ..
            } => {
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
            Reason::Atom {
                key, args, from, ..
            } => {
                if from.iter().any(|&b| b >= cur) {
                    return Err(format!("atom {:?} relies on a later block", key));
                }
                if !self.template(h, *key, args) {
                    return Err(format!(
                        "atom {:?} {:?}: template hypotheses not established before fact {h}",
                        key, args
                    ));
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

    fn combo(
        &mut self,
        blocks: &[Block],
        h: FactId,
        reasons: &[Reason],
        terms: &[Term],
        exact_rows: bool,
    ) -> Option<(Table, LinComb)> {
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
            let theorem =
                matches!(r, Reason::Atom { key, .. } if super::theorems::is_theorem(*key));
            let identity =
                matches!(r, Reason::Atom { key, .. } if super::theorems::is_identity(*key));
            if theorem && !identity && !self.verified(t, &row) {
                return None;
            }
            if matches!(r, Reason::Atom { .. }) && !theorem && !self.proven(h, t, &row) {
                return None;
            }
            tb = t;
            let row = if exact_rows {
                exact(self.cx.t, t, row)
            } else {
                row
            };
            out.iadd_mul(&row, &term.coef);
        }
        Some((tb, out))
    }

    fn residual_ok(&self, tb: Table, diff: &LinComb, sum: &LinComb, directed: bool) -> bool {
        residual_ok(self.cx, tb, diff, sum, directed)
    }

    fn link_ok(
        &mut self,
        blocks: &[Block],
        h: FactId,
        a: &Expr,
        b: &Expr,
        link: &Link,
        directed: bool,
    ) -> Result<(), String> {
        let t = self.cx.t;
        let (tb, ea) = eval(t, a).ok_or("term does not evaluate")?;
        let (_, eb) = eval(t, b).ok_or("term does not evaluate")?;
        if link.combination.is_empty()
            && link
                .reasons
                .iter()
                .all(|r| matches!(r, Reason::Atom { key, .. } if key.is_identity_label()))
        {
            if tb != Table::Angle
                || a == b
                || !self.residual_ok(tb, &(&ea - &eb), &LinComb::zero(), directed)
            {
                return Err(format!(
                    "angle-addition link is not an identity: {:?} -> {:?}",
                    a, b
                ));
            }
            if let [Reason::Atom { key, args, .. }] = link.reasons.as_slice() {
                if super::barem::identity_label(self.cx, a, b, directed)
                    != Some((*key, args.clone()))
                {
                    return Err(format!(
                        "identity link label {key:?} {args:?} does not describe {a:?} -> {b:?}"
                    ));
                }
            } else if !link.reasons.is_empty() {
                return Err("identity link with several labels".into());
            }
            return Ok(());
        }
        let (_, sum) = self
            .combo(blocks, h, &link.reasons, &link.combination, !directed)
            .ok_or(
                "link combination does not resolve, or uses an atom row outside the engine span",
            )?;
        for r in &link.reasons {
            if let Reason::Atom { key, args, .. } = r {
                if key.is_cyclic_quad() {
                    let want = match super::cyclic::cyclic_kind(self.cx, blocks, a, b, link) {
                        Some((super::cyclic::CyclicKind::Opposite, q)) => {
                            Some((AtomKey::CyclicOpposite, q))
                        }
                        Some((super::cyclic::CyclicKind::Exterior, q)) => {
                            Some((AtomKey::CyclicExterior, q))
                        }
                        _ => None,
                    };
                    if directed
                        || want.is_none_or(|(k, q)| k != *key || q.as_slice() != args.as_slice())
                    {
                        return Err(format!("cyclic quadrilateral label {key:?} {args:?} does not describe {a:?} -> {b:?}"));
                    }
                }
            }
        }
        if link.combination.is_empty() {
            return Err("link without a combination".into());
        }
        if !self.residual_ok(tb, &(&ea - &eb), &sum, directed) {
            return Err(format!(
                "link equation is not the stated combination of its reasons: {:?} -> {:?}",
                a, b
            ));
        }
        Ok(())
    }

    fn sentence_ok(
        &mut self,
        blocks: &[Block],
        cur: u16,
        sidx: usize,
        h: FactId,
        s: &Sentence,
        start: Option<(&Expr, &Expr)>,
    ) -> Result<(), String> {
        let t = self.cx.t;
        match s {
            Sentence::Chain {
                terms,
                links,
                directed,
                then,
            } => {
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
                                if self.residual_ok(Table::Angle, &junction, &LinComb::zero(), true)
                                {
                                    found = Some(LinComb::combine(&pf, &last, &-&sgn));
                                    break;
                                }
                            }
                            found.ok_or("chains do not join")?
                        }
                    };
                    let targets =
                        conclusion_forms(t, st).ok_or("chain conclusion has no algebraic form")?;
                    let ok = targets.iter().any(|(_, x)| {
                        self.residual_ok(tb, &diff, x, true)
                            || self.residual_ok(tb, &diff, &x.negated(), true)
                    });
                    if !ok && !targets.is_empty() {
                        let mut ok_all = false;
                        if let Stmt::Cyclic { pts } | Stmt::Coll { pts } = st {
                            ok_all = pts.len() >= 3 && self.stmt_in_span_of_chain(&diff, st);
                        }
                        if !ok_all {
                            if std::env::var_os("HP_DEBUG").is_some() {
                                eprintln!("chain endpoints: terms {terms:?} then {st:?}");
                            }
                            return Err("chain endpoints do not give the stated conclusion".into());
                        }
                    }
                }
            }
            Sentence::Pooled {
                stmt,
                reasons,
                combination,
            }
            | Sentence::Because {
                stmt,
                reasons,
                combination,
            } => {
                for r in reasons {
                    self.reason_ok(blocks, cur, sidx, h, r)?;
                }
                if !combination.is_empty() {
                    let targets = conclusion_forms(t, stmt)
                        .ok_or("pooled statement has no algebraic form")?;
                    let (tb, sum) = self.combo(blocks, h, reasons, combination, false).ok_or("pooled combination does not resolve, or uses an atom row outside the engine span")?;
                    let ok = targets.iter().any(|(t2, x)| {
                        *t2 == tb
                            && (self.residual_ok(tb, x, &sum, true)
                                || self.residual_ok(tb, &x.negated(), &sum, true))
                    });
                    if !ok {
                        let q: Vec<String> = targets
                            .iter()
                            .map(|(_, x)| format!("{:?}", self.cx.quot.q(&(&x.clone() - &sum))))
                            .collect();
                        return Err(format!(
                            "pooled combination does not equal the statement {:?}: residuals {:?}",
                            stmt, q
                        ));
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
            Sentence::Theorem { key, stmt, reasons } => {
                for r in reasons {
                    self.reason_ok(blocks, cur, sidx, h, r)?;
                }
                if key.is_line_converse() {
                    super::readings::line_converse_ok(self.cx, *key, stmt, reasons)?;
                } else if key.is_converse() {
                    converse_ok(self.cx, *key, stmt, reasons)?;
                } else if *key == TheoremKey::CircleMerge {
                    merge_ok(stmt, reasons)?;
                }
            }
            Sentence::Raw { engine_fact, .. } => {
                if !self
                    .cx
                    .in_cl
                    .get(*engine_fact as usize)
                    .copied()
                    .unwrap_or(false)
                {
                    return Err("raw step outside the closure".into());
                }
            }
        }
        Ok(())
    }

    fn stmt_in_span_of_chain(&self, diff: &LinComb, st: &Stmt) -> bool {
        let t = self.cx.t;
        let Some(targets) = stmt_targets(t, st) else {
            return false;
        };
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
            let h = b
                .engine_facts
                .iter()
                .map(|f| f + 1)
                .max()
                .unwrap_or(0)
                .max(b.horizon + 1)
                .max(if b.kind == BlockKind::Conclusion {
                    cx.closure.last().map(|x| x + 1).unwrap_or(0)
                } else {
                    0
                });
            let by_body = b.kind != BlockKind::Conclusion && b.body.iter().any(|s| {
                establishes(cx, s, &b.stmt)
                    && !matches!(s, Sentence::Because { combination, .. } if combination.is_empty())
                    && !matches!(s, Sentence::Theorem { .. })
            });
            for (tb, row) in targets {
                if !t.holds(tb, &row) && !t.holds(tb, &row.negated()) {
                    return Err(format!(
                        "block statement does not hold in the figure: {:?}",
                        b.stmt
                    ));
                }
                if !by_body && !self.proven(h, tb, &row) {
                    return Err(
                        "block statement is not a consequence of the engine facts it re-presents"
                            .into(),
                    );
                }
            }
        }
        Ok(())
    }
}

fn same_angle(a: Option<&Expr>, b: Option<&Expr>) -> bool {
    fn strip(e: &Expr) -> Expr {
        match e {
            Expr::Angle { a, b, c, .. } => Expr::Angle {
                a: *a,
                b: *b,
                c: *c,
                directed: true,
            },
            Expr::LineAngle { l1, l2, .. } => Expr::LineAngle {
                l1: *l1,
                l2: *l2,
                directed: true,
            },
            Expr::Lin { terms } => Expr::Lin {
                terms: terms.iter().map(|(k, x)| (k.clone(), strip(x))).collect(),
            },
            x => x.clone(),
        }
    }
    match (a, b) {
        (Some(x), Some(y)) => strip(x) == strip(y),
        _ => false,
    }
}

pub fn violations(cx: &Ctx, hp: &HumanProof) -> Vec<Violation> {
    let mut out = violations_with(&mut Checker::new(cx), hp);
    out.extend(layout_violations(cx, hp));
    out
}

pub fn setup_violations(cx: &Ctx, hp: &HumanProof) -> Vec<Violation> {
    let mut maximal: Vec<Vec<PointId>> = Vec::new();
    for c in cx
        .circles
        .iter()
        .filter(|c| c.src.is_empty() && c.pts.len() >= 3)
    {
        if let Some(m) = maximal
            .iter_mut()
            .find(|m| c.pts.iter().filter(|p| m.contains(p)).count() >= 3)
        {
            for &p in &c.pts {
                if !m.contains(&p) {
                    m.push(p);
                }
            }
        } else {
            maximal.push(c.pts.clone());
        }
    }
    let mut out = Vec::new();
    for l in &hp.setup {
        if let SetupLine::Circle { through, .. } = l {
            if through.len() >= 4
                && !maximal
                    .iter()
                    .any(|m| through.iter().all(|p| m.contains(p)))
            {
                out.push(Violation { block: 0, rule: "I7", detail: format!("a setup circle lists points {through:?} whose concyclicity is not a hypothesis") });
            }
        }
    }
    out
}

fn bisector_wording(cx: &Ctx, hp: &HumanProof) -> Vec<(u16, String)> {
    fn walk<'a>(r: &'a Reason, out: &mut Vec<&'a Stmt>) {
        match r {
            Reason::Hyp { stmt, .. } => out.push(stmt),
            Reason::Fact { because, .. } => because.iter().for_each(|b| walk(b, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for b in &hp.blocks {
        let mut hyps: Vec<&Stmt> = Vec::new();
        for s in &b.body {
            match s {
                Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links
                    .iter()
                    .flat_map(|l| &l.reasons)
                    .for_each(|r| walk(r, &mut hyps)),
                Sentence::Because { reasons, .. }
                | Sentence::Pooled { reasons, .. }
                | Sentence::Theorem { reasons, .. } => {
                    reasons.iter().for_each(|r| walk(r, &mut hyps))
                }
                Sentence::Raw { .. } => {}
            }
        }
        for st in hyps {
            let (
                Some(shape),
                Stmt::EqAngle {
                    lhs: Expr::Angle { a, b: v, c, .. },
                    rhs: Expr::Angle { c: c2, .. },
                },
            ) = (super::barem::bisector_shape(st), st)
            else {
                continue;
            };
            let x = super::theorems::interior_degrees(cx.t, *a, *v, *c);
            let y = super::theorems::interior_degrees(cx.t, *c, *v, *c2);
            let equal = (x - y).abs() < 1e-6;
            let wrong = match shape {
                (false, false) => !super::barem::internal_bisector(cx.t, *a, *v, *c, *c2),
                (true, true) => equal,
                _ => false,
            };
            if wrong {
                out.push((
                    b.id,
                    format!(
                        "a hypothesis is worded as {} bisector but the figure reads {x:.3} and {y:.3} degrees",
                        if shape.0 { "an external" } else { "a" }
                    ),
                ));
            }
        }
    }
    out
}

pub fn layout_violations(cx: &Ctx, hp: &HumanProof) -> Vec<Violation> {
    let mut out = setup_violations(cx, hp);
    let mut bad = |block: u16, detail: String| {
        out.push(Violation {
            block,
            rule: "I6",
            detail,
        })
    };
    for (i, b) in hp.blocks.iter().enumerate() {
        if b.step as usize != i + 1 {
            bad(
                b.id,
                format!(
                    "block {} is numbered {} at position {}",
                    b.id,
                    b.step,
                    i + 1
                ),
            );
        }
    }
    let Some(goal) = &hp.goal else {
        if !hp.plan.is_empty() || hp.blocks.iter().any(|b| b.tag) {
            bad(
                0,
                "relation tags or a plan without the finished layout".into(),
            );
        }
        return out;
    };
    if *goal != super::barem::goal_words(cx) {
        bad(0, "the conclusion's wording does not match the goal".into());
    }
    for (block, detail) in bisector_wording(cx, hp) {
        bad(block, detail);
    }
    let mut cited: std::collections::BTreeSet<u16> = std::collections::BTreeSet::new();
    for b in &hp.blocks {
        cited.extend(
            super::barem::cited_blocks(b)
                .into_iter()
                .filter(|&x| x < b.id),
        );
    }
    for b in &hp.blocks {
        let want = cited.contains(&b.id) && b.kind != BlockKind::Conclusion;
        if b.tag != want {
            bad(
                b.id,
                format!(
                    "block {} is {}tagged but is {}cited later",
                    b.id,
                    if b.tag { "" } else { "not " },
                    if want { "" } else { "not " }
                ),
            );
        }
    }
    if let Some(c) = hp.blocks.last() {
        let direct = super::barem::cited_blocks(c);
        for p in &hp.plan {
            if !direct.contains(p) {
                bad(
                    c.id,
                    format!("the plan names block {p}, which the conclusion does not use"),
                );
            }
        }
    }
    out
}

pub fn violations_with(ch: &mut Checker, hp: &HumanProof) -> Vec<Violation> {
    let cx = ch.cx;
    let mut out = Vec::new();
    let blocks = hp.blocks.clone();
    for (i, b) in blocks.iter().enumerate() {
        if b.id as usize != i + 1 {
            out.push(Violation {
                block: b.id,
                rule: "I5",
                detail: "block ids are not in order".into(),
            });
        }
        if let Err(e) = ch.stmt_ok(b) {
            out.push(Violation {
                block: b.id,
                rule: "I1",
                detail: e,
            });
        }
        let h = if b.kind == BlockKind::Conclusion {
            b.horizon.max(cx.closure.last().map(|x| x + 1).unwrap_or(0))
        } else {
            b.horizon
        };
        let mut prev: Option<(Expr, Expr)> = None;
        for (sidx, s) in b.body.iter().enumerate() {
            let joins = |terms: &Vec<Expr>| -> bool {
                let Some(p) = prev.as_ref() else { return false };
                if same_angle(Some(&p.1), terms.first()) {
                    return true;
                }
                match (eval(cx.t, &p.1), terms.first().and_then(|e| eval(cx.t, e))) {
                    (Some((Table::Angle, x)), Some((Table::Angle, y))) => {
                        [&x - &y, &x + &y].iter().any(|d| {
                            let q = cx.quot.q(d);
                            q.terms.iter().all(|(v, _)| *v == ANGLE_UNIT)
                                && q.get(ANGLE_UNIT).is_integer()
                        })
                    }
                    _ => false,
                }
            };
            let st = match s {
                Sentence::Chain { terms, .. } if joins(terms) => {
                    prev.as_ref().map(|p| (&p.0, &p.1))
                }
                _ => None,
            };
            if let Err(e) = ch.sentence_ok(&blocks, b.id, sidx, h, s, st) {
                out.push(Violation {
                    block: b.id,
                    rule: "I2",
                    detail: e,
                });
            }
            prev = match s {
                Sentence::Chain {
                    terms, then: None, ..
                } => Some((terms[0].clone(), terms[terms.len() - 1].clone())),
                _ => None,
            };
        }
    }
    if !blocks
        .last()
        .is_some_and(|b| b.kind == BlockKind::Conclusion)
    {
        out.push(Violation {
            block: 0,
            rule: "I5",
            detail: "the last block does not prove the goal".into(),
        });
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
                eprintln!(
                    "  stmt {:?}\n  facts {:?}\n  body {:?}",
                    b.stmt, b.engine_facts, b.body
                );
            }
        }
    }
    if strict {
        panic!("human proof checker: {:?}", first);
    }
    let mut v: Vec<Violation> = first.iter().filter(|x| x.rule != "I6").cloned().collect();
    for _ in 0..32 {
        if v.is_empty() {
            break;
        }
        let bad: std::collections::BTreeSet<u16> = v.iter().map(|x| x.block).collect();
        if bad.contains(&0)
            || hp
                .blocks
                .iter()
                .any(|b| b.kind == BlockKind::Conclusion && bad.contains(&b.id))
        {
            hp.blocks.retain(|b| b.kind != BlockKind::Conclusion);
            break;
        }
        let demoted: std::collections::BTreeSet<u16> = hp
            .blocks
            .iter()
            .filter(|b| bad.contains(&b.id) && matches!(b.kind, BlockKind::Claim(_)))
            .map(|b| b.id)
            .collect();
        for b in hp.blocks.iter_mut() {
            if bad.contains(&b.id) {
                b.kind = BlockKind::Raw;
                if let Some(&f) = b.engine_facts.first() {
                    b.stmt = super::classify::fact_stmt(cx, f);
                }
                b.body = b
                    .engine_facts
                    .iter()
                    .map(|&f| Sentence::Raw {
                        engine_fact: f,
                        cites: cx.t.facts[f as usize].premises.clone(),
                    })
                    .collect();
            }
        }
        if !demoted.is_empty() {
            uncite(hp, &demoted);
        }
        v = violations_with(&mut Checker::new(cx), hp);
    }
    if hp.goal.is_some() {
        super::barem::finish(cx, hp);
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
                Sentence::Because { reasons, .. }
                | Sentence::Pooled { reasons, .. }
                | Sentence::Theorem { reasons, .. } => {
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

pub const LINT_RULES: [(&str, &str); 8] = [
    (
        "L1",
        "a row repeats the previous row and adds terms that cancel (zero insertion)",
    ),
    ("L2", "a row that changes nothing"),
    ("L3", "a constant carried through directed rows"),
    ("L4", "a row whose change is not the fact it cites"),
    (
        "L5",
        "an angle-addition label on an identity over several vertices",
    ),
    ("L6", "several facts in one row"),
    (
        "L7",
        "an angle sum or difference equated to 0° (or 180° identified with 0°)",
    ),
    ("L8", "a cited reason the row does not use"),
];

fn label_reason(r: &Reason) -> bool {
    super::barem::is_coll_reason(r)
        || matches!(r, Reason::Atom { key, .. }
            if key.is_config_label() || key.is_identity_label() || key.is_cyclic_quad())
}

fn zero_const(e: &Expr) -> bool {
    matches!(e, Expr::Const { degrees } if degrees.is_zero())
}

pub fn chain_lints(cx: &Ctx, hp: &HumanProof) -> Vec<Violation> {
    use super::barem::{identity_only, key_map, link_facts, opaque_link, term_infos};
    let mut out = Vec::new();
    if !hp.available {
        return out;
    }
    for b in &hp.blocks {
        if b.kind == BlockKind::Raw {
            continue;
        }
        for s in &b.body {
            let Sentence::Chain {
                terms,
                links,
                directed,
                ..
            } = s
            else {
                continue;
            };
            if terms.len() != links.len() + 1
                || !terms
                    .first()
                    .and_then(|e| eval(cx.t, e))
                    .is_some_and(|x| x.0 == Table::Angle)
            {
                continue;
            }
            let mut bad = |rule: &'static str, detail: String| {
                out.push(Violation {
                    block: b.id,
                    rule,
                    detail,
                })
            };
            let mut carried = 0usize;
            let end = terms
                .last()
                .map(super::barem::end_constant)
                .unwrap_or_else(Rat::zero);
            for (k, l) in links.iter().enumerate() {
                let (e1, e2) = (&terms[k], &terms[k + 1]);
                let d = *directed && is_directed(e1) && is_directed(e2);
                let row = k + 2;
                if e1 == e2 {
                    bad("L2", format!("row {row} repeats row {}", row - 1));
                    continue;
                }
                let (Some(ia), Some(ib)) = (term_infos(cx, e1, d), term_infos(cx, e2, d)) else {
                    continue;
                };
                let (ma, mb) = (key_map(&ia), key_map(&ib));
                let (c1, c2) = (const_part(e1), const_part(e2));
                let same_const = if d {
                    (&(&c1 - &c2) / &Rat::from_int(180)).is_integer()
                } else {
                    c1 == c2
                };
                let identity = identity_only(l);
                if super::barem::zero_insertion(cx, e1, e2, d) {
                    bad(
                        "L1",
                        format!(
                            "row {row} keeps every term of row {} and adds terms that cancel",
                            row - 1
                        ),
                    );
                }
                if ma == mb && same_const && !identity && l.reasons.iter().any(|r| !label_reason(r))
                {
                    bad("L2", format!("row {row} changes nothing but cites a fact"));
                }
                if identity {
                    let label = super::barem::identity_label(cx, e1, e2, d);
                    let honest = match l.reasons.as_slice() {
                        [] => matches!(&label, Some((AtomKey::AngleAddition, a)) if a.len() == 1),
                        [Reason::Atom {
                            key: AtomKey::AngleAddition,
                            args,
                            ..
                        }] => !args.is_empty(),
                        [Reason::Atom {
                            key: AtomKey::LineAngleAddition,
                            ..
                        }] => d,
                        _ => true,
                    };
                    if !honest {
                        bad(
                            "L5",
                            format!("row {row} is labelled angle addition but is not one angle split at one vertex"),
                        );
                    }
                } else {
                    let facts = link_facts(l);
                    if facts > 1 {
                        bad("L6", format!("row {row} cites {facts} facts"));
                    } else if facts == 1 && opaque_link(cx, &hp.blocks, e1, e2, l, d).is_some() {
                        bad(
                            "L4",
                            format!("row {row} changes more than the fact it cites"),
                        );
                    }
                    for (i, r) in l.reasons.iter().enumerate() {
                        let used = l
                            .combination
                            .iter()
                            .any(|t| t.reason as usize == i && !t.coef.is_zero());
                        if !used
                            && !label_reason(r)
                            && !matches!(r, Reason::Lemma { .. } | Reason::Atom { .. })
                        {
                            bad("L8", format!("row {row} cites a reason it does not use"));
                        }
                    }
                }
                let sum_terms = |m: &BTreeMap<String, Rat>| m.len() >= 2;
                if !d && ((zero_const(e2) && sum_terms(&ma)) || (zero_const(e1) && sum_terms(&mb)))
                {
                    bad("L7", format!("row {row} equates angles to 0°"));
                }
                if super::barem::carried_row(cx, e1, e2, d, &end) {
                    carried += 1;
                }
            }
            if carried >= 2 {
                bad(
                    "L3",
                    format!("a constant is carried through {carried} directed rows"),
                );
            }
        }
    }
    out
}
