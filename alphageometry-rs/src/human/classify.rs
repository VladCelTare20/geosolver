use super::ctx::{Ctx, FactClass};
use super::model::{Expr, Stmt, TheoremKey};
use super::trace::Table;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::{PointId, Predicate};
use crate::proof::{FactId, Reason};
use crate::rational::Rat;

#[derive(Clone, Debug)]
pub struct Split {
    pub l: Expr,
    pub r: Expr,
    pub lraw: LinComb,
    pub rraw: LinComb,
}

#[derive(Clone, Debug)]
pub struct Target {
    pub table: Table,
    pub row: LinComb,
    pub splits: Vec<Split>,
    pub stmt: Stmt,
    pub alts: Vec<Target>,
}

#[derive(Clone, Debug)]
pub struct Obl {
    pub label: &'static str,
    pub targets: Vec<Target>,
    pub requires: Vec<FactId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cyclic,
    Coll,
    Sim,
    Congruent,
    Cong,
    Isosceles,
    Length,
    Angle,
    Theorem(TheoremKey),
    Formula(TheoremKey),
    Merge,
    Other,
}

pub fn fact_points(cx: &Ctx, f: FactId) -> Vec<PointId> {
    let mut v: Vec<PointId> = match &cx.t.facts[f as usize].reason {
        Reason::Concyclic(p) | Reason::Collinear(p) | Reason::Theorem(_, p) | Reason::Formula(_, _, p) => p.clone(),
        Reason::SimilarTriangles(a, b) => vec![a.0, a.1, a.2, b.0, b.1, b.2],
        Reason::EqualRadius(o, p) => {
            let mut x = vec![*o];
            x.extend(p.iter().copied());
            x
        }
        Reason::PointMerge(a, b) | Reason::TangentMerge(a, b) => vec![*a, *b],
        Reason::TransferAddMul(a, b) | Reason::TransferArcChord(a, b) => vec![a.0, a.1, b.0, b.1],
        Reason::Assumption(_) | Reason::Construction(_) => cx.hyp_pred.get(&f).map(|p| p.points.clone()).unwrap_or_default(),
    };
    v.sort_unstable();
    v.dedup();
    v
}

pub fn ratio_one(cx: &Ctx, t1: (PointId, PointId, PointId), t2: (PointId, PointId, PointId)) -> bool {
    let t = cx.t;
    let a = t.dist(t1.0, t1.1);
    let b = t.dist(t2.0, t2.1);
    a > 0.0 && ((a - b) / a).abs() < 1e-9
}

pub fn theorem_key(name: &str) -> TheoremKey {
    match name {
        "radical axis" => TheoremKey::RadicalAxis,
        "angle bisector theorem" => TheoremKey::AngleBisectorThm,
        "angle bisector theorem (converse)" => TheoremKey::AngleBisectorThmConverse,
        "Monge–d'Alembert" => TheoremKey::Monge,
        "homothety at a centre of similitude" => TheoremKey::Homothety,
        "Menelaus' theorem" => TheoremKey::Menelaus,
        "Menelaus' theorem (converse)" => TheoremKey::MenelausConverse,
        "Ceva's theorem (converse)" => TheoremKey::CevaConverse,
        "angle bisectors concur (incentre/excentre)" => TheoremKey::BisectorConcurrency,
        "equality case of the triangle inequality" => TheoremKey::TriangleEquality,
        "perpendicular ⇒ squared lengths (Pythagoras)" => TheoremKey::Pythagoras,
        "perpendicular from squared lengths" => TheoremKey::PerpFromSquares,
        "squares of proportional lengths" => TheoremKey::SquaresOfRatio,
        "Stewart's theorem" => TheoremKey::Stewart,
        "lengths from squared lengths" => TheoremKey::LengthsFromSquares,
        "law of sines" => TheoremKey::LawOfSines,
        "equal or supplementary angles have equal sines" => TheoremKey::EqualSines,
        "double-angle formula" => TheoremKey::DoubleAngle,
        "triple-angle formula" => TheoremKey::TripleAngle,
        "law of sines, converse" => TheoremKey::SinesConverse,
        n if n.starts_with("sine of") => TheoremKey::SineConst,
        n if n.starts_with("intercept theorem") => TheoremKey::Intercept,
        _ => TheoremKey::Other,
    }
}

pub fn kind(cx: &Ctx, f: FactId) -> Kind {
    match &cx.t.facts[f as usize].reason {
        Reason::Concyclic(_) => Kind::Cyclic,
        Reason::Collinear(_) => Kind::Coll,
        Reason::SimilarTriangles(a, b) => {
            if self_similar(*a, *b) {
                Kind::Isosceles
            } else if ratio_one(cx, *a, *b) {
                Kind::Congruent
            } else {
                Kind::Sim
            }
        }
        Reason::TransferArcChord(..) => Kind::Cong,
        Reason::TransferAddMul(..) => Kind::Length,
        Reason::Theorem(name, _) => Kind::Theorem(theorem_key(name)),
        Reason::Formula(name, _, _) => Kind::Formula(theorem_key(name)),
        Reason::PointMerge(..) | Reason::TangentMerge(..) => Kind::Merge,
        _ => Kind::Other,
    }
}

pub fn self_similar(a: (PointId, PointId, PointId), b: (PointId, PointId, PointId)) -> bool {
    let mut s1 = [a.0, a.1, a.2];
    let mut s2 = [b.0, b.1, b.2];
    s1.sort_unstable();
    s2.sort_unstable();
    s1 == s2
}

pub fn opposite(cx: &Ctx, t1: (PointId, PointId, PointId), t2: (PointId, PointId, PointId)) -> bool {
    cx.t.orient(t1.0, t1.1, t1.2) != cx.t.orient(t2.0, t2.1, t2.2)
}

pub fn fact_stmt(cx: &Ctx, f: FactId) -> Stmt {
    let t = cx.t;
    if let Some((o, p)) = small_circle(cx, f) {
        return Stmt::Cong { s1: (o, p[0]), s2: (o, p[1]) };
    }
    match &t.facts[f as usize].reason {
        Reason::Concyclic(p) => Stmt::Cyclic { pts: p.clone() },
        Reason::Collinear(p) => Stmt::Coll { pts: p.clone() },
        Reason::SimilarTriangles(a, b) => {
            let opp = opposite(cx, *a, *b);
            if self_similar(*a, *b) {
                let ta = [a.0, a.1, a.2];
                let tb = [b.0, b.1, b.2];
                if let Some(i) = (0..3).find(|&i| ta[i] == tb[i]) {
                    let others: Vec<PointId> = (0..3).filter(|&j| j != i).map(|j| ta[j]).collect();
                    return Stmt::Cong { s1: (ta[i], others[0]), s2: (ta[i], others[1]) };
                }
            }
            if ratio_one(cx, *a, *b) {
                Stmt::Congruent { t1: *a, t2: *b, opposite: opp }
            } else {
                Stmt::Sim { t1: *a, t2: *b, opposite: opp }
            }
        }
        Reason::EqualRadius(o, p) => Stmt::Cong { s1: (*o, p[0]), s2: (*o, *p.last().unwrap_or(&p[0])) },
        Reason::PointMerge(a, b) | Reason::TangentMerge(a, b) => Stmt::Coincide { a: *a, b: *b },
        Reason::TransferAddMul(a, b) => {
            let r = t.dist(b.0, b.1) / t.dist(a.0, a.1).max(1e-300);
            match small_rational(r) {
                Some(q) if q.is_one() => Stmt::Cong { s1: *a, s2: *b },
                Some(q) => Stmt::RatioConst { s1: *b, s2: *a, value: q },
                None => Stmt::Cong { s1: *a, s2: *b },
            }
        }
        Reason::TransferArcChord(a, b) => {
            if t.rows_of(Table::Ratio, f).next().is_some() {
                Stmt::Cong { s1: *a, s2: *b }
            } else {
                Stmt::Formula { text: "arcs".into(), pts: vec![a.0, a.1, b.0, b.1] }
            }
        }
        Reason::Theorem(name, p) => match *name {
            "radical axis" | "Monge–d'Alembert" | "Menelaus' theorem (converse)" | "equality case of the triangle inequality" => {
                Stmt::Coll { pts: p.clone() }
            }
            "angle bisector theorem (converse)" if p.len() == 4 => Stmt::EqAngle {
                lhs: Expr::Angle { a: p[2], b: p[0], c: p[1], directed: true },
                rhs: Expr::Angle { a: p[1], b: p[0], c: p[3], directed: true },
            },
            "angle bisector theorem" if p.len() == 4 => Stmt::EqRatio { segs: vec![(p[1], p[2]), (p[1], p[3]), (p[0], p[2]), (p[0], p[3])] },
            "perpendicular from squared lengths" if p.len() == 4 => Stmt::Perp { l1: (p[0], p[1]), l2: (p[2], p[3]) },
            _ => {
                let rows: Vec<&LinComb> = t.rows_of(Table::Ratio, f).collect();
                if rows.len() == 1 && t.rows_of(Table::Angle, f).next().is_none() {
                    let r = rows[0];
                    let pos: LinComb = LinComb { terms: r.terms.iter().filter(|(_, k)| !k.is_negative()).cloned().collect() };
                    let neg: LinComb = LinComb { terms: r.terms.iter().filter(|(_, k)| k.is_negative()).map(|(v, k)| (*v, -k)).collect() };
                    let lhs = super::chain::ratio_expr(cx, &pos);
                    let rhs = super::chain::ratio_expr(cx, &neg);
                    Stmt::Eq { lhs, rhs }
                } else if let Some(s) = sq_stmt(cx, f) {
                    s
                } else {
                    Stmt::Formula { text: name.to_string(), pts: p.clone() }
                }
            }
        },
        Reason::Formula(name, text, p) => formula_stmt(cx, f).unwrap_or_else(|| Stmt::Formula { text: format!("{name}: {text}"), pts: p.clone() }),
        Reason::Assumption(_) | Reason::Construction(_) => cx.hyp_pred.get(&f).map(|p| pred_stmt(p)).unwrap_or(Stmt::Formula { text: String::new(), pts: vec![] }),
    }
}

fn line_angle_undirected(t: &crate::human::trace::EngineTrace, (a, b, c, d): (PointId, PointId, PointId, PointId)) -> Option<Expr> {
    let v = [a, b].into_iter().find(|p| *p == c || *p == d)?;
    let x = if a == v { b } else { a };
    let z = if c == v { d } else { c };
    if x == z || t.dir(v, x).is_none() || t.dir(v, z).is_none() {
        return None;
    }
    Some(Expr::Angle { a: x, b: v, c: z, directed: false })
}

pub fn formula_stmt(cx: &Ctx, f: FactId) -> Option<Stmt> {
    let t = cx.t;
    let Reason::Formula(name, text, pts) = &t.facts[f as usize].reason else { return None };
    let key = theorem_key(name);
    let angs = parse_angles(text, pts);
    let trig = |(s, l): &(Rat, (PointId, PointId, PointId, PointId))| -> Option<Expr> {
        let e = line_angle_undirected(t, *l)?;
        let s = s.mod_one();
        if s.is_zero() {
            Some(Expr::Sin { angle: Box::new(e) })
        } else if s == Rat::new(1, 2) {
            Some(Expr::Cos { angle: Box::new(e) })
        } else {
            None
        }
    };
    let stmt = match key {
        TheoremKey::LawOfSines => {
            let start = text.find("△")?;
            let tri: String = text[start..].chars().take_while(|c| *c != ',').collect();
            return Some(Stmt::Formula { text: format!("law of sines in {tri}"), pts: pts.clone() });
        }
        TheoremKey::EqualSines if angs.len() >= 2 => Stmt::Eq { lhs: trig(&angs[0])?, rhs: trig(&angs[1])? },
        TheoremKey::DoubleAngle if angs.len() >= 2 => {
            let x = line_angle_undirected(t, angs[0].1)?;
            let y = line_angle_undirected(t, angs[1].1)?;
            if !angs[0].0.is_zero() {
                return None;
            }
            Stmt::Eq {
                lhs: Expr::Sin { angle: Box::new(y) },
                rhs: Expr::Prod { factors: vec![(Expr::Num { value: Rat::from_int(2) }, 1), (Expr::Sin { angle: Box::new(x.clone()) }, 1), (Expr::Cos { angle: Box::new(x) }, 1)] },
            }
        }
        TheoremKey::SineConst if !angs.is_empty() && name.trim_start_matches("sine of ").trim_end_matches('°') == "90" && angs[0].0.is_zero() => {
            let e = line_angle_undirected(t, angs[0].1)?;
            let st = Stmt::AngleConst { angle: e.clone(), degrees: Rat::from_int(90) };
            let (_, raw) = super::expr::eval(t, &e)?;
            let mut row = raw;
            row.add_term(ANGLE_UNIT, Rat::new(-1, 2));
            if cx.support_facts(Table::Angle, &t.exact(Table::Angle, &row), f + 1).is_some() {
                return Some(st);
            }
            Stmt::Eq { lhs: Expr::Sin { angle: Box::new(e) }, rhs: Expr::Num { value: Rat::one() } }
        }
        _ => return None,
    };
    let Stmt::Eq { lhs, rhs } = &stmt else { return None };
    let (_, l) = super::expr::eval(t, lhs)?;
    let (_, r) = super::expr::eval(t, rhs)?;
    let mine = t.canon_ratio(&(&l - &r));
    let ok = t.rows_of(Table::Ratio, f).any(|row| {
        let c = t.canon_ratio(row);
        c == mine || c == mine.negated()
    });
    ok.then_some(stmt)
}

pub fn small_rational(r: f64) -> Option<Rat> {
    if !r.is_finite() || r <= 0.0 {
        return None;
    }
    for q in 1..=12i64 {
        let p = (r * q as f64).round();
        if p >= 1.0 && (p / q as f64 - r).abs() < 1e-6 * r.max(1.0) {
            return Some(Rat::new(p as i64, q));
        }
    }
    None
}

pub fn pred_stmt(p: &Predicate) -> Stmt {
    let q = &p.points;
    let pair = |i: usize| (q[i], q[i + 1]);
    match p.name.as_str() {
        "coll" => Stmt::Coll { pts: q.clone() },
        "cyclic" => Stmt::Cyclic { pts: q.clone() },
        "perp" if q.len() == 4 => Stmt::Perp { l1: pair(0), l2: pair(2) },
        "para" if q.len() == 4 => Stmt::Para { l1: pair(0), l2: pair(2) },
        "cong" if q.len() == 4 => Stmt::Cong { s1: pair(0), s2: pair(2) },
        "eqangle" if q.len() == 8 => Stmt::EqAngle { lhs: line_angle_expr(pair(0), pair(2)), rhs: line_angle_expr(pair(4), pair(6)) },
        "eqratio" if q.len() == 8 => Stmt::EqRatio { segs: vec![pair(0), pair(2), pair(4), pair(6)] },
        "rconst" if q.len() == 4 => Stmt::RatioConst { s1: pair(0), s2: pair(2), value: p.constants.first().cloned().unwrap_or_else(Rat::one) },
        "aconst" | "s_angle" if q.len() == 4 => Stmt::AngleConst {
            angle: line_angle_expr(pair(2), pair(0)),
            degrees: p.constants.first().cloned().unwrap_or_else(Rat::zero),
        },
        _ => Stmt::Formula { text: p.name.clone(), pts: q.clone() },
    }
}

pub fn line_angle_expr(l1: (PointId, PointId), l2: (PointId, PointId)) -> Expr {
    let common = [l1.0, l1.1].into_iter().find(|p| *p == l2.0 || *p == l2.1);
    match common {
        Some(v) => {
            let x = if l1.0 == v { l1.1 } else { l1.0 };
            let z = if l2.0 == v { l2.1 } else { l2.0 };
            if x == z {
                Expr::LineAngle { l1, l2, directed: true }
            } else {
                Expr::Angle { a: x, b: v, c: z, directed: true }
            }
        }
        None => Expr::LineAngle { l1, l2, directed: true },
    }
}

pub fn ang_expr(a: PointId, b: PointId, c: PointId) -> Expr {
    Expr::Angle { a, b, c, directed: true }
}

pub fn angle_target(cx: &Ctx, l: Expr, r: Expr, stmt: Stmt) -> Option<Target> {
    let (_, lraw) = super::expr::eval(cx.t, &l)?;
    let (_, rraw) = super::expr::eval(cx.t, &r)?;
    let raw = &lraw - &rraw;
    if !cx.t.holds(Table::Angle, &raw) {
        return None;
    }
    Some(Target { table: Table::Angle, row: cx.t.exact(Table::Angle, &raw), splits: vec![Split { l, r, lraw, rraw }], stmt, alts: Vec::new() })
}

pub fn ratio_target(cx: &Ctx, l: Expr, r: Expr, stmt: Stmt) -> Option<Target> {
    let (_, lraw) = super::expr::eval(cx.t, &l)?;
    let (_, rraw) = super::expr::eval(cx.t, &r)?;
    let raw = &lraw - &rraw;
    if !cx.t.holds(Table::Ratio, &raw) {
        return None;
    }
    Some(Target { table: Table::Ratio, row: raw, splits: vec![Split { l, r, lraw, rraw }], stmt, alts: Vec::new() })
}

fn seg(a: PointId, b: PointId) -> Expr {
    Expr::Seg { a, b }
}

fn quot(num: Vec<Expr>, den: Vec<Expr>) -> Expr {
    let mut f: Vec<(Expr, i32)> = num.into_iter().map(|e| (e, 1)).collect();
    f.extend(den.into_iter().map(|e| (e, -1)));
    Expr::Prod { factors: f }
}

pub fn cyc_forms(cx: &Ctx, q: [PointId; 4]) -> Vec<Target> {
    let stmt = Stmt::Cyclic { pts: q.to_vec() };
    let mut out: Vec<Target> = Vec::new();
    for (a, b, c, d) in [(q[0], q[1], q[2], q[3]), (q[0], q[2], q[1], q[3]), (q[0], q[3], q[1], q[2])] {
        let mut t: Option<Target> = None;
        for (x, y, u, v) in [(a, b, c, d), (c, d, a, b)] {
            if let Some(n) = angle_target(cx, ang_expr(x, u, y), ang_expr(x, v, y), stmt.clone()) {
                match &mut t {
                    None => t = Some(n),
                    Some(t0) => {
                        if t0.row == n.row || t0.row == n.row.negated() {
                            t0.splits.extend(n.splits);
                        }
                    }
                }
            }
        }
        out.extend(t);
    }
    out
}

fn with_alts(mut forms: Vec<Target>) -> Option<Target> {
    if forms.is_empty() {
        return None;
    }
    let mut first = forms.remove(0);
    first.alts = forms;
    Some(first)
}

fn triples(base: &[PointId], cap: usize) -> Vec<(PointId, PointId, PointId)> {
    let mut out = Vec::new();
    for i in 0..base.len() {
        for j in i + 1..base.len() {
            for k in j + 1..base.len() {
                if out.len() < cap {
                    out.push((base[i], base[j], base[k]));
                }
            }
        }
    }
    out
}

pub fn cyclic_targets(cx: &Ctx, p: &[PointId], f: Option<FactId>) -> Option<(Vec<Target>, Vec<FactId>)> {
    if p.len() < 4 {
        return Some((Vec::new(), Vec::new()));
    }
    let mut base: Vec<PointId> = p[..3].to_vec();
    let mut requires = Vec::new();
    if let Some(f) = f {
        let best = cx
            .circles
            .iter()
            .filter(|c| c.fact < f && c.pts.len() >= 3 && c.pts.len() < p.len() && c.pts.iter().all(|x| p.contains(x)))
            .max_by_key(|c| (c.pts.len(), std::cmp::Reverse(c.src.len()), std::cmp::Reverse(c.fact)));
        if let Some(c) = best {
            base = c.pts.clone();
            requires.push(c.fact);
        }
    }
    let mut out = Vec::new();
    for &x in p.iter().filter(|x| !base.contains(x)) {
        let mut forms = Vec::new();
        for (a, b, c) in triples(&base, 10) {
            forms.extend(cyc_forms(cx, [a, b, c, x]));
        }
        out.push(with_alts(forms)?);
    }
    Some((out, requires))
}

fn sq_stmt(cx: &Ctx, f: FactId) -> Option<Stmt> {
    let t = cx.t;
    let rows: Vec<&LinComb> = t.rows_of(Table::Sq, f).collect();
    if rows.len() != 1 || Table::ALL.iter().any(|&tb| tb != Table::Sq && t.rows_of(tb, f).next().is_some()) {
        return None;
    }
    let mut pair_of: std::collections::BTreeMap<crate::lincomb::VarId, (PointId, PointId)> = std::collections::BTreeMap::new();
    for a in 0..t.n as PointId {
        for b in (a + 1)..t.n as PointId {
            if let Some(v) = t.var(Table::Sq, a, b) {
                pair_of.entry(v).or_insert((a, b));
            }
        }
    }
    let mut lhs = Vec::new();
    let mut rhs = Vec::new();
    for (v, k) in rows[0].terms.iter() {
        let &(a, b) = pair_of.get(v)?;
        if k.is_negative() {
            rhs.push((-k, Expr::Sq { a, b }));
        } else {
            lhs.push((k.clone(), Expr::Sq { a, b }));
        }
    }
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let side = |v: Vec<(Rat, Expr)>| if v.len() == 1 && v[0].0.is_one() { v.into_iter().next().unwrap().1 } else { Expr::Lin { terms: v } };
    Some(Stmt::Eq { lhs: side(lhs), rhs: side(rhs) })
}

pub fn centre_obligations(cx: &Ctx, p: &[PointId]) -> Vec<Obl> {
    let t = cx.t;
    let mut out = Vec::new();
    if p.len() < 4 {
        return out;
    }
    for o in 0..t.names.len() as PointId {
        if p.contains(&o) {
            continue;
        }
        let r0 = t.dist(o, p[0]);
        if r0 < 1e-9 || p.iter().any(|&x| (t.dist(o, x) - r0).abs() > 1e-7 * r0.max(1.0)) {
            continue;
        }
        let targets: Option<Vec<Target>> = p[1..].iter().map(|&x| ratio_target(cx, seg(o, p[0]), seg(o, x), Stmt::Cong { s1: (o, p[0]), s2: (o, x) })).collect();
        if let Some(targets) = targets {
            out.push(Obl { label: "centre", targets, requires: vec![] });
        }
    }
    out
}

pub fn coll_targets(cx: &Ctx, p: &[PointId], f: Option<FactId>) -> Option<(Vec<Target>, Vec<FactId>)> {
    if p.len() < 3 {
        return Some((Vec::new(), Vec::new()));
    }
    let mut base: Vec<PointId> = p[..2].to_vec();
    let mut requires = Vec::new();
    if let Some(f) = f {
        let best = cx
            .lines
            .iter()
            .filter(|l| l.fact < f && l.pts.len() >= 2 && l.pts.len() < p.len() && l.pts.iter().all(|x| p.contains(x)))
            .max_by_key(|l| (l.pts.len(), std::cmp::Reverse(l.src.len()), std::cmp::Reverse(l.fact)));
        if let Some(l) = best {
            base = l.pts.clone();
            requires.push(l.fact);
        }
    }
    let mut out = Vec::new();
    for &c in p.iter().filter(|x| !base.contains(x)) {
        if cx.collinear_hyp(&[base[0], base[1], c]) {
            continue;
        }
        let mut forms = Vec::new();
        let mut pairs = 0;
        for i in 0..base.len() {
            for j in i + 1..base.len() {
                if pairs >= 6 {
                    continue;
                }
                pairs += 1;
                let (a, b) = (base[i], base[j]);
                let stmt = Stmt::Coll { pts: vec![a, b, c] };
                for (x, v, y) in [(b, a, c), (a, b, c), (a, c, b)] {
                    forms.extend(angle_target(cx, ang_expr(x, v, y), Expr::Const { degrees: Rat::zero() }, stmt.clone()));
                }
            }
        }
        out.push(with_alts(forms)?);
    }
    Some((out, requires))
}

pub fn sim_obligations(cx: &Ctx, t1: (PointId, PointId, PointId), t2: (PointId, PointId, PointId)) -> Vec<Obl> {
    let (a, b, c) = t1;
    let (x, y, z) = t2;
    let opp = opposite(cx, t1, t2);
    let st = Stmt::Sim { t1, t2, opposite: opp };
    let angle_at = |p: PointId, q: PointId, r: PointId, pp: PointId, qq: PointId, rr: PointId| -> Option<Target> {
        let l = ang_expr(q, p, r);
        let rexp = if opp { ang_expr(rr, pp, qq) } else { ang_expr(qq, pp, rr) };
        angle_target(cx, l, rexp, Stmt::EqAngle { lhs: ang_expr(q, p, r), rhs: if opp { ang_expr(rr, pp, qq) } else { ang_expr(qq, pp, rr) } })
    };
    let ratio_at = |p: PointId, q: PointId, r: PointId, pp: PointId, qq: PointId, rr: PointId| -> Option<Target> {
        ratio_target(
            cx,
            quot(vec![seg(p, q)], vec![seg(p, r)]),
            quot(vec![seg(pp, qq)], vec![seg(pp, rr)]),
            Stmt::EqRatio { segs: vec![(p, q), (p, r), (pp, qq), (pp, rr)] },
        )
    };
    let _ = st;
    let mut out = Vec::new();
    let a1 = angle_at(a, b, c, x, y, z);
    let a2 = angle_at(b, a, c, y, x, z);
    let a3 = angle_at(c, a, b, z, x, y);
    let r1 = ratio_at(a, b, c, x, y, z);
    let r2 = ratio_at(b, a, c, y, x, z);
    let r3 = ratio_at(c, a, b, z, x, y);
    if let (Some(p), Some(q)) = (&a1, &a2) {
        out.push(Obl { label: "AA", targets: vec![p.clone(), q.clone()], requires: vec![] });
    }
    if let (Some(p), Some(q)) = (&a1, &a3) {
        out.push(Obl { label: "AA", targets: vec![p.clone(), q.clone()], requires: vec![] });
    }
    if let (Some(p), Some(q)) = (&a2, &a3) {
        out.push(Obl { label: "AA", targets: vec![p.clone(), q.clone()], requires: vec![] });
    }
    for (aa, rr) in [(&a1, &r1), (&a2, &r2), (&a3, &r3)] {
        if let (Some(p), Some(q)) = (aa, rr) {
            out.push(Obl { label: "SAS", targets: vec![p.clone(), q.clone()], requires: vec![] });
        }
    }
    if let (Some(p), Some(q)) = (&r1, &r2) {
        out.push(Obl { label: "SSS", targets: vec![p.clone(), q.clone()], requires: vec![] });
    }
    out
}

pub fn parse_angles(text: &str, pts: &[PointId]) -> Vec<(Rat, (PointId, PointId, PointId, PointId))> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    let num = |s: &str, k: &mut usize| -> Option<usize> {
        let bs = s.as_bytes();
        if bs.get(*k) != Some(&b'{') {
            return None;
        }
        let start = *k + 1;
        let mut e = start;
        while e < bs.len() && bs[e].is_ascii_digit() {
            e += 1;
        }
        if bs.get(e) != Some(&b'}') {
            return None;
        }
        let n = s[start..e].parse().ok()?;
        *k = e + 1;
        Some(n)
    };
    let marker = "∠(";
    while let Some(off) = text[i..].find(marker) {
        let mut k = i + off + marker.len();
        let parsed = (|| -> Option<(usize, usize, usize, usize, usize)> {
            let p0 = num(text, &mut k)?;
            let p1 = num(text, &mut k)?;
            if b.get(k) != Some(&b',') {
                return None;
            }
            k += 1;
            let p2 = num(text, &mut k)?;
            let p3 = num(text, &mut k)?;
            if b.get(k) != Some(&b')') {
                return None;
            }
            k += 1;
            Some((p0, p1, p2, p3, k))
        })();
        match parsed {
            Some((p0, p1, p2, p3, end)) => {
                let mut shift = Rat::zero();
                let rest = &text[end..];
                if let Some(r) = rest.strip_prefix(" + ") {
                    if let Some(deg_end) = r.find('°') {
                        if let Ok(d) = r[..deg_end].trim().parse::<i64>() {
                            shift = Rat::new(d, 180);
                        }
                    }
                }
                let g = |i: usize| pts.get(i).copied();
                if let (Some(a), Some(bb), Some(c), Some(d)) = (g(p0), g(p1), g(p2), g(p3)) {
                    out.push((shift, (a, bb, c, d)));
                }
                i = end;
            }
            None => i = i + off + marker.len(),
        }
    }
    out
}

pub fn formula_obligation(cx: &Ctx, f: FactId) -> Option<Vec<Obl>> {
    let Reason::Formula(name, text, pts) = &cx.t.facts[f as usize].reason else { return None };
    let key = theorem_key(name);
    let angs = parse_angles(text, pts);
    let t = cx.t;
    let raw = |(a, b, c, d): (PointId, PointId, PointId, PointId)| -> Option<LinComb> { Some(&t.dir(c, d)? - &t.dir(a, b)?) };
    let line = |(a, b, c, d): (PointId, PointId, PointId, PointId)| line_angle_expr((a, b), (c, d));
    let with_shift = |e: Expr, s: &Rat| -> Expr {
        if s.is_zero() {
            e
        } else {
            Expr::Lin { terms: vec![(Rat::one(), e), (Rat::one(), Expr::Const { degrees: s * &Rat::from_int(180) })] }
        }
    };
    let pick = |l: Expr, r1: Expr, r2: Expr| -> Option<Target> {
        let stmt1 = Stmt::EqAngle { lhs: l.clone(), rhs: r1.clone() };
        angle_target(cx, l.clone(), r1, stmt1).or_else(|| {
            let stmt2 = Stmt::EqAngle { lhs: l.clone(), rhs: r2.clone() };
            angle_target(cx, l, r2, stmt2)
        })
    };
    let neg = |e: Expr| Expr::Lin { terms: vec![(Rat::from_int(-1), e)] };
    let target = match key {
        TheoremKey::EqualSines if angs.len() >= 2 => {
            let (s, x) = angs[0].clone();
            let (_, y) = angs[1].clone();
            raw(x)?;
            raw(y)?;
            pick(with_shift(line(x), &s), line(y), neg(line(y)))
        }
        TheoremKey::DoubleAngle | TheoremKey::TripleAngle if angs.len() >= 2 => {
            let n = if key == TheoremKey::DoubleAngle { 2 } else { 3 };
            let (s, x) = angs[0].clone();
            let (_, y) = angs[1].clone();
            let lx = Expr::Lin { terms: vec![(Rat::from_int(n), with_shift(line(x), &s))] };
            pick(lx, line(y), neg(line(y)))
        }
        TheoremKey::SineConst if !angs.is_empty() => {
            let (s, x) = angs[0].clone();
            let deg: i64 = name.trim_start_matches("sine of ").trim_end_matches('°').parse().ok()?;
            let c = Expr::Const { degrees: Rat::from_int(deg) };
            let c2 = Expr::Const { degrees: Rat::from_int(180 - deg) };
            pick(with_shift(line(x), &s), c, c2)
        }
        TheoremKey::LawOfSines => return Some(Vec::new()),
        _ => return None,
    }?;
    Some(vec![Obl { label: "angles", targets: vec![target], requires: vec![] }])
}

pub fn goal_obligations(cx: &Ctx, g: &Predicate) -> Vec<Obl> {
    let p = &g.points;
    let pair = |i: usize| (p[i], p[i + 1]);
    let stmt = pred_stmt(g);
    let la = line_angle_expr;
    let mut out = Vec::new();
    match g.name.as_str() {
        "coll" => {
            if let Some((t, _)) = coll_targets(cx, p, None) {
                out.push(Obl { label: "coll", targets: t, requires: vec![] });
            }
        }
        "cyclic" => {
            if let Some((t, _)) = cyclic_targets(cx, p, None) {
                out.push(Obl { label: "cyclic", targets: t, requires: vec![] });
            }
            out.extend(centre_obligations(cx, p));
        }
        "eqangle" if p.len() == 8 => {
            let l = line_angle_expr(pair(0), pair(2));
            let r = line_angle_expr(pair(4), pair(6));
            if let Some(t) = angle_target(cx, l, r, stmt.clone()) {
                out.push(Obl { label: "eqangle", targets: vec![t], requires: vec![] });
            }
        }
        "para" if p.len() == 4 => {
            if let Some(t) = angle_target(cx, la(pair(2), pair(0)), Expr::Const { degrees: Rat::zero() }, stmt.clone()) {
                out.push(Obl { label: "para", targets: vec![t], requires: vec![] });
            }
        }
        "perp" if p.len() == 4 => {
            if let Some(t) = angle_target(cx, la(pair(2), pair(0)), Expr::Const { degrees: Rat::from_int(90) }, stmt.clone()) {
                out.push(Obl { label: "perp", targets: vec![t], requires: vec![] });
            }
        }
        "aconst" | "s_angle" if p.len() == 4 => {
            let d = g.constants.first().cloned().unwrap_or_else(Rat::zero);
            if let Some(t) = angle_target(cx, la(pair(2), pair(0)), Expr::Const { degrees: d }, stmt.clone()) {
                out.push(Obl { label: "aconst", targets: vec![t], requires: vec![] });
            }
        }
        "cong" if p.len() == 4 => {
            if let Some(t) = ratio_target(cx, seg(p[0], p[1]), seg(p[2], p[3]), stmt.clone()) {
                out.push(Obl { label: "cong", targets: vec![t], requires: vec![] });
            }
        }
        "eqratio" if p.len() == 8 => {
            if let Some(t) = ratio_target(cx, quot(vec![seg(p[0], p[1])], vec![seg(p[2], p[3])]), quot(vec![seg(p[4], p[5])], vec![seg(p[6], p[7])]), stmt.clone()) {
                out.push(Obl { label: "eqratio", targets: vec![t], requires: vec![] });
            }
        }
        "rconst" if p.len() == 4 => {
            let k = g.constants.first().cloned().unwrap_or_else(Rat::one);
            let mut den = vec![(seg(p[2], p[3]), 1)];
            if !k.is_one() {
                den.insert(0, (Expr::Num { value: k.clone() }, 1));
            }
            let mut f = vec![(seg(p[0], p[1]), 1)];
            f.extend(den.into_iter().map(|(e, _)| (e, -1)));
            if let Some(t) = ratio_target(cx, Expr::Prod { factors: f }, Expr::Num { value: Rat::one() }, stmt.clone()) {
                out.push(Obl { label: "rconst", targets: vec![t], requires: vec![] });
            }
        }
        _ => {}
    }
    let _ = ANGLE_UNIT;
    out
}

pub fn small_circle(cx: &Ctx, f: FactId) -> Option<(PointId, Vec<PointId>)> {
    let Reason::Concyclic(p) = &cx.t.facts[f as usize].reason else { return None };
    let mut q = p.clone();
    q.sort_unstable();
    q.dedup();
    if q.len() != 3 {
        return None;
    }
    let t = cx.t;
    let rows: Vec<&LinComb> = t.rows_of(Table::Ratio, f).collect();
    (0..t.n as PointId).find(|&o| {
        !q.contains(&o)
            && q.iter().all(|&x| t.dm(o, x).is_some())
            && rows.iter().any(|r| r.terms.iter().all(|(v, _)| q.iter().any(|&x| t.dm(o, x).is_some_and(|d| d.terms.iter().any(|(w, _)| w == v)))))
    })
    .map(|o| (o, p.clone()))
}

pub fn central_obligations(cx: &Ctx, o: PointId, q: &[PointId]) -> Vec<Obl> {
    let mut forms = Vec::new();
    for (x, y, z) in [(q[0], q[1], q[2]), (q[0], q[2], q[1]), (q[1], q[2], q[0])] {
        let l = Expr::Angle { a: x, b: o, c: y, directed: true };
        let r = Expr::Lin { terms: vec![(Rat::from_int(2), Expr::Angle { a: x, b: z, c: y, directed: true })] };
        forms.extend(angle_target(cx, l.clone(), r.clone(), Stmt::Eq { lhs: l, rhs: r }));
    }
    with_alts(forms).map(|t| vec![Obl { label: "central", targets: vec![t], requires: vec![] }]).unwrap_or_default()
}

pub fn fact_obligations(cx: &Ctx, f: FactId) -> Option<Vec<Obl>> {
    let t = cx.t;
    if let Some((o, p)) = small_circle(cx, f) {
        return Some(central_obligations(cx, o, &p));
    }
    match &t.facts[f as usize].reason {
        Reason::Concyclic(p) => {
            let mut out = Vec::new();
            if let Some((ts, req)) = cyclic_targets(cx, p, Some(f)) {
                out.push(Obl { label: "inscribed", targets: ts, requires: req });
            }
            if let Some((ts, _)) = cyclic_targets(cx, p, None) {
                out.push(Obl { label: "inscribed", targets: ts, requires: vec![] });
            }
            out.extend(centre_obligations(cx, p));
            Some(out)
        }
        Reason::Collinear(p) => {
            let mut out = Vec::new();
            if let Some((ts, req)) = coll_targets(cx, p, Some(f)) {
                out.push(Obl { label: "coll", targets: ts, requires: req });
            }
            if let Some((ts, _)) = coll_targets(cx, p, None) {
                out.push(Obl { label: "coll", targets: ts, requires: vec![] });
            }
            Some(out)
        }
        Reason::SimilarTriangles(a, b) => Some(sim_obligations(cx, *a, *b)),
        Reason::TransferArcChord(ab, cd) => {
            let (a, b, c, d) = (ab.0, ab.1, cd.0, cd.1);
            if t.rows_of(Table::Ratio, f).next().is_some() {
                let mut out = Vec::new();
                for circle in &cx.circles {
                    if circle.fact >= f || ![a, b, c, d].iter().all(|x| circle.pts.contains(x)) {
                        continue;
                    }
                    for &z in &circle.pts {
                        if z == a || z == b {
                            continue;
                        }
                        for &w in &circle.pts {
                            if w == c || w == d {
                                continue;
                            }
                            for (l, r) in [(ang_expr(a, z, b), ang_expr(c, w, d)), (ang_expr(a, z, b), ang_expr(d, w, c))] {
                                let st = Stmt::EqAngle { lhs: l.clone(), rhs: r.clone() };
                                if let Some(tg) = angle_target(cx, l, r, st) {
                                    out.push(Obl { label: "arcs", targets: vec![tg], requires: vec![circle.fact] });
                                }
                            }
                        }
                    }
                }
                Some(out)
            } else {
                let cf = cx.circles.iter().filter(|k| k.fact < f && [a, b, c, d].iter().all(|x| k.pts.contains(x))).min_by_key(|k| (k.src.len(), k.fact))?.fact;
                ratio_target(cx, seg(a, b), seg(c, d), Stmt::Cong { s1: (a, b), s2: (c, d) }).map(|tg| vec![Obl { label: "chords", targets: vec![tg], requires: vec![cf] }])
            }
        }
        Reason::Theorem("radical axis", p) if p.len() == 3 => {
            let x = p[0];
            let mut out = Vec::new();
            let cs: Vec<&super::ctx::CircleObj> = cx.circles.iter().filter(|c| c.fact < f && c.pts.contains(&p[1]) && c.pts.contains(&p[2]) && !c.pts.contains(&x)).collect();
            let chords = |c: &super::ctx::CircleObj| -> Vec<(PointId, PointId)> {
                let mut v = Vec::new();
                for (i, &u) in c.pts.iter().enumerate() {
                    for &w in &c.pts[i + 1..] {
                        if cx.line_through(&[x, u, w], f).is_some() {
                            v.push((u, w));
                        }
                    }
                }
                v
            };
            for (i, c1) in cs.iter().enumerate() {
                for c2 in &cs[i + 1..] {
                    for (d1, d2) in chords(c1) {
                        for (e1, e2) in chords(c2) {
                            if (d1, d2) == (e1, e2) {
                                continue;
                            }
                            let l = Expr::Prod { factors: vec![(seg(x, d1), 1), (seg(x, d2), 1)] };
                            let r = Expr::Prod { factors: vec![(seg(x, e1), 1), (seg(x, e2), 1)] };
                            let st = Stmt::Eq { lhs: l.clone(), rhs: r.clone() };
                            if let Some(tg) = ratio_target(cx, l, r, st) {
                                let mut req = vec![c1.fact, c2.fact];
                                req.extend(cx.line_through(&[x, d1, d2], f).map(|l| l.fact));
                                req.extend(cx.line_through(&[x, e1, e2], f).map(|l| l.fact));
                                out.push(Obl { label: "powers", targets: vec![tg], requires: req });
                            }
                        }
                    }
                }
            }
            Some(out)
        }
        Reason::Formula(..) => formula_obligation(cx, f),
        Reason::TransferAddMul(ab, cd) => {
            let stmt = fact_stmt(cx, f);
            let (s1, s2, k) = match &stmt {
                Stmt::RatioConst { s1, s2, value } => (*s1, *s2, value.clone()),
                Stmt::Cong { s1, s2 } => (*s1, *s2, Rat::one()),
                _ => (*cd, *ab, Rat::one()),
            };
            let mut out = Vec::new();
            if let (Some(x), Some(y), Some(c)) = (t.dm(s1.0, s1.1), t.dm(s2.0, s2.1), t.prime_const(&k)) {
                let row = &(&x - &y) - &c;
                let tg = Target { table: Table::Ratio, row: row.clone(), splits: vec![Split { l: seg(s1.0, s1.1), r: seg(s2.0, s2.1), lraw: x, rraw: &y + &c }], stmt: stmt.clone(), alts: Vec::new() };
                out.push(Obl { label: "lengths", targets: vec![tg], requires: vec![] });
            }
            if let (Some(x), Some(y)) = (t.single(Table::Add, s1.0, s1.1), t.single(Table::Add, s2.0, s2.1)) {
                let mut ky = y.clone();
                ky.mul_assign_scalar(&k);
                let row = &x - &ky;
                let tg = Target { table: Table::Add, row: row.clone(), splits: vec![Split { l: seg(s1.0, s1.1), r: seg(s2.0, s2.1), lraw: x, rraw: ky }], stmt: stmt.clone(), alts: Vec::new() };
                out.push(Obl { label: "segments", targets: vec![tg], requires: vec![] });
            }
            Some(out)
        }
        _ => {
            let _ = FactClass::Derived;
            None
        }
    }
}
