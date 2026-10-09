use super::barem::{debug, diff_terms, is_coll_reason, link_sum, opposite_ray, same_ray, TermInfo};
use super::check::{self, sides, Checker};
use super::ctx::Ctx;
use super::expr::{eval, is_directed};
use super::model::*;
use super::trace::Table;
use crate::predicate::PointId;
use crate::rational::Rat;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CyclicKind {
    Plain,
    Opposite,
    Exterior,
}

pub(crate) fn cyclic_reason(blocks: &[Block], l: &Link) -> Option<(usize, Vec<PointId>, Vec<u16>)> {
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    let mut found = None;
    for &i in &used {
        let r = l.reasons.get(i as usize)?;
        if is_coll_reason(r) {
            continue;
        }
        if found.is_some() {
            return None;
        }
        let (pts, from) = match r {
            Reason::Atom { key: AtomKey::Inscribed | AtomKey::CyclicOpposite | AtomKey::CyclicExterior, args, from, .. } => (args.clone(), from.clone()),
            Reason::Hyp { stmt: Stmt::Cyclic { pts }, .. } => (pts.clone(), Vec::new()),
            Reason::Fact { stmt: Stmt::Cyclic { pts }, block: Some(b), .. } => (pts.clone(), vec![*b]),
            Reason::Fact { stmt: Stmt::Cyclic { pts }, block: None, .. } => (pts.clone(), Vec::new()),
            Reason::Lemma { stmt: Stmt::Cyclic { pts }, block, .. } => (pts.clone(), vec![*block]),
            Reason::Claim { block, .. } => match blocks.iter().find(|b| b.id == *block).map(|b| &b.stmt) {
                Some(Stmt::Cyclic { pts }) => (pts.clone(), vec![*block]),
                _ => return None,
            },
            _ => return None,
        };
        found = Some((i as usize, pts, from));
    }
    found
}

pub fn cyclic_kind(cx: &Ctx, blocks: &[Block], e1: &Expr, e2: &Expr, l: &Link) -> Option<(CyclicKind, [PointId; 4])> {
    let (_, pts, _) = cyclic_reason(blocks, l)?;
    let d = diff_terms(cx, e1, e2, false)?;
    let [(k1, t1), (k2, t2)] = d.as_slice() else { return None };
    if !k1.abs().is_one() || !k2.abs().is_one() {
        return None;
    }
    let resolve = |v: PointId, r: PointId| -> Option<(PointId, bool)> {
        pts.iter().copied().filter(|&w| w != v).find_map(|w| {
            if same_ray(cx, v, r, w) {
                Some((w, false))
            } else if opposite_ray(cx, v, r, w) {
                Some((w, true))
            } else {
                None
            }
        })
    };
    let term = |ti: &TermInfo| -> Option<(PointId, PointId, bool, PointId, bool)> {
        let Expr::Angle { a, b, c, .. } = ti.expr else { return None };
        if !pts.contains(&b) {
            return None;
        }
        let (x, fx) = resolve(b, a)?;
        let (y, fy) = resolve(b, c)?;
        Some((b, x, fx, y, fy))
    };
    let (p, x, f1a, y, f1b) = term(t1)?;
    let (q, x2, f2a, y2, f2b) = term(t2)?;
    if p == q || x == y || !check::same_set(&[x, y], &[x2, y2]) || [x, y].contains(&p) || [x, y].contains(&q) {
        return None;
    }
    if (f1a && f1b) || (f2a && f2b) {
        return None;
    }
    let same = sides(cx.t, p, q, x, y)?;
    let equal = k1.is_negative() != k2.is_negative();
    match (same, f1a != f1b, f2a != f2b, equal) {
        (true, false, false, true) => Some((CyclicKind::Plain, [p, x, q, y])),
        (false, false, false, false) => Some((CyclicKind::Opposite, [p, x, q, y])),
        (false, true, false, true) => Some((CyclicKind::Exterior, [p, x, q, y])),
        (false, false, true, true) => Some((CyclicKind::Exterior, [q, x, p, y])),
        _ => None,
    }
}

pub(crate) fn angle_chain(cx: &Ctx, terms: &[Expr]) -> bool {
    terms.first().and_then(|e| eval(cx.t, e)).is_some_and(|(tb, _)| tb == Table::Angle)
}

fn relink(cx: &Ctx, blocks: &[Block], a: &Expr, b: &Expr, l: &Link, i: usize, new: Vec<Reason>, directed: bool) -> Option<Link> {
    let mut reasons = l.reasons.clone();
    let n = new.len();
    reasons.splice(i..i + 1, new);
    let shift = |r: u16| if (r as usize) > i { r + n as u16 - 1 } else { r };
    let others: Vec<Term> = l.combination.iter().filter(|t| t.reason as usize != i).map(|t| Term { reason: shift(t.reason), ..t.clone() }).collect();
    let (Some((tb, ea)), Some((_, eb))) = (eval(cx.t, a), eval(cx.t, b)) else { return None };
    let diff = &ea - &eb;
    let signs = [Rat::one(), Rat::from_int(-1)];
    let base: Vec<Rat> = l.combination.iter().filter(|t| t.reason as usize == i).map(|t| t.coef.clone()).collect();
    let k = base.first().cloned().unwrap_or_else(Rat::one);
    let mut choices: Vec<Vec<Rat>> = vec![Vec::new()];
    for _ in 0..n {
        choices = choices.into_iter().flat_map(|c| signs.iter().map(move |s| [c.clone(), vec![s.clone()]].concat())).collect();
    }
    for ch in choices {
        let mut comb = others.clone();
        for (j, c) in ch.iter().enumerate() {
            comb.push(Term { reason: (i + j) as u16, row: 0, coef: c * &k });
        }
        let cand = Link { reasons: reasons.clone(), combination: comb };
        let Some(sum) = link_sum(cx, blocks, &cand, !directed) else { continue };
        if check::residual_ok(cx, tb, &diff, &sum, directed) {
            return Some(cand);
        }
    }
    None
}

pub fn cyclic_quads(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed: false, then } = s else { return None };
    if !angle_chain(cx, terms) {
        return None;
    }
    let mut out = links.clone();
    let mut changed = false;
    for (k, l) in links.iter().enumerate() {
        let Some((kind, quad)) = cyclic_kind(cx, blocks, &terms[k], &terms[k + 1], l) else { continue };
        let key = match kind {
            CyclicKind::Opposite => AtomKey::CyclicOpposite,
            CyclicKind::Exterior => AtomKey::CyclicExterior,
            CyclicKind::Plain => continue,
        };
        let Some((i, _, from)) = cyclic_reason(blocks, l) else { continue };
        let atom = Reason::Atom { key, stmt: Stmt::Cyclic { pts: quad.to_vec() }, args: quad.to_vec(), from };
        if matches!(&l.reasons[i], Reason::Fact { block: None, because, .. } if !because.is_empty()) {
            let mut nl = l.clone();
            nl.reasons.push(atom);
            out[k] = nl;
            changed = true;
            continue;
        }
        if let Some(nl) = relink(cx, blocks, &terms[k], &terms[k + 1], l, i, vec![atom], false) {
            out[k] = nl;
            changed = true;
        }
    }
    changed.then(|| Sentence::Chain { terms: terms.clone(), links: out, directed: false, then: then.clone() })
}

fn shows_vertex(cx: &Ctx, e: &Expr, o: PointId) -> bool {
    let _ = cx;
    match e {
        Expr::Angle { b, .. } => *b == o,
        Expr::Lin { terms } => terms.iter().any(|(_, x)| matches!(x, Expr::Angle { b, .. } if *b == o)),
        _ => false,
    }
}

pub fn central_isosceles(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    if !angle_chain(cx, terms) {
        return None;
    }
    let mut out = links.clone();
    let mut changed = false;
    for (k, l) in links.iter().enumerate() {
        let d = *directed && is_directed(&terms[k]) && is_directed(&terms[k + 1]);
        let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
        let hit = used.iter().copied().find(|&i| matches!(l.reasons.get(i as usize), Some(Reason::Atom { key: AtomKey::CentralAngle, args, .. }) if args.len() >= 4 && !shows_vertex(cx, &terms[k], args[0]) && !shows_vertex(cx, &terms[k + 1], args[0])));
        let Some(i) = hit else { continue };
        let coefs: Vec<&Rat> = l.combination.iter().filter(|t| t.reason == i).map(|t| &t.coef).collect();
        if coefs.len() != 1 || !coefs[0].abs().is_one() {
            continue;
        }
        let Some(Reason::Atom { args, from, .. }) = l.reasons.get(i as usize) else { continue };
        let (o, a, b, z) = (args[0], args[1], args[2], args[3]);
        let iso = |x: PointId| {
            let args = vec![o, x, z];
            Reason::Atom { key: AtomKey::Isosceles, stmt: super::claims::atom_stmt(AtomKey::Isosceles, &args), args, from: from.clone() }
        };
        if let Some(nl) = relink(cx, blocks, &terms[k], &terms[k + 1], l, i as usize, vec![iso(a), iso(b)], d) {
            if debug() {
                eprintln!("central_isosceles: {:?} -> {:?} via {:?}", terms[k], terms[k + 1], l.reasons);
            }
            out[k] = nl;
            changed = true;
        }
    }
    changed.then(|| Sentence::Chain { terms: terms.clone(), links: out, directed: *directed, then: then.clone() })
}

fn central_of(l: &Link) -> Option<(PointId, [PointId; 2], PointId, Vec<u16>)> {
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    let core: Vec<&Reason> = used.iter().filter_map(|&i| l.reasons.get(i as usize)).filter(|r| !is_coll_reason(r)).collect();
    match core.as_slice() {
        [Reason::Atom { key: AtomKey::CentralAngle, args, from, .. }] if args.len() >= 4 => Some((args[0], [args[1], args[2]], args[3], from.clone())),
        _ => None,
    }
}

pub fn merge_central(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    if !angle_chain(cx, terms) || links.len() < 2 {
        return None;
    }
    let mut terms = terms.clone();
    let mut links = links.clone();
    let mut changed = false;
    let mut k = 0;
    while k + 1 < links.len() {
        let (Some((o1, c1, z1, f1)), Some((o2, c2, z2, f2))) = (central_of(&links[k]), central_of(&links[k + 1])) else {
            k += 1;
            continue;
        };
        if o1 != o2 || !check::same_set(&c1, &c2) || z1 == z2 || !shows_vertex(cx, &terms[k + 1], o1) || shows_vertex(cx, &terms[k], o1) || shows_vertex(cx, &terms[k + 2], o1) {
            k += 1;
            continue;
        }
        let d = *directed && is_directed(&terms[k]) && is_directed(&terms[k + 2]);
        let args = vec![c1[0], c1[1], z1, z2];
        let from: Vec<u16> = f1.iter().chain(f2.iter()).copied().collect::<BTreeSet<u16>>().into_iter().collect();
        let atom = Reason::Atom { key: AtomKey::Inscribed, stmt: Stmt::Cyclic { pts: args.clone() }, args, from };
        let (Some((tb, ea)), Some((_, eb))) = (eval(cx.t, &terms[k]), eval(cx.t, &terms[k + 2])) else {
            k += 1;
            continue;
        };
        let diff = &ea - &eb;
        let mut merged: Option<Link> = None;
        for sg in [Rat::one(), Rat::from_int(-1)] {
            let cand = Link { reasons: vec![atom.clone()], combination: vec![Term { reason: 0, row: 0, coef: sg }] };
            let Some(sum) = link_sum(cx, blocks, &cand, !d) else { continue };
            if check::residual_ok(cx, tb, &diff, &sum, d) {
                merged = Some(cand);
                break;
            }
        }
        let Some(m) = merged else {
            k += 1;
            continue;
        };
        links.splice(k..k + 2, [m]);
        terms.remove(k + 1);
        changed = true;
    }
    changed.then(|| Sentence::Chain { terms, links, directed: *directed, then: then.clone() })
}

pub fn radius_chord(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed: true, then } = s else { return None };
    if !angle_chain(cx, terms) {
        return None;
    }
    let half = Rat::new(1, 2);
    let mut out = links.clone();
    let mut changed = false;
    for (k, l) in links.iter().enumerate() {
        if !is_directed(&terms[k]) || !is_directed(&terms[k + 1]) {
            continue;
        }
        let coef_half = |i: usize| -> bool {
            let v: Vec<&Term> = l.combination.iter().filter(|t| t.reason as usize == i).collect();
            matches!(v.as_slice(), [t] if t.coef.abs() == half)
        };
        let mut groups: BTreeMap<PointId, (Vec<usize>, Vec<PointId>, BTreeSet<u16>)> = BTreeMap::new();
        for (i, r) in l.reasons.iter().enumerate() {
            let Reason::Atom { key, args, from, .. } = r else { continue };
            let pts: &[PointId] = match key {
                AtomKey::CentralAngle if args.len() >= 4 => &args[1..4],
                AtomKey::Isosceles if args.len() == 3 => &args[1..3],
                _ => continue,
            };
            if !coef_half(i) {
                continue;
            }
            let g = groups.entry(args[0]).or_default();
            g.0.push(i);
            for &p in pts {
                if !g.1.contains(&p) {
                    g.1.push(p);
                }
            }
            g.2.extend(from.iter().copied());
        }
        let (Some((tb, ea)), Some((_, eb))) = (eval(cx.t, &terms[k]), eval(cx.t, &terms[k + 1])) else { continue };
        let diff = &ea - &eb;
        'groups: for (o, (idx, pts, from)) in groups {
            if idx.len() < 2 || pts.len() != 3 {
                continue;
            }
            let keep: Vec<usize> = (0..l.reasons.len()).filter(|i| !idx.contains(i)).collect();
            let remap: BTreeMap<usize, u16> = keep.iter().enumerate().map(|(n, &old)| (old, n as u16)).collect();
            let others: Vec<Term> = l.combination.iter().filter(|t| !idx.contains(&(t.reason as usize))).map(|t| Term { reason: remap[&(t.reason as usize)], ..t.clone() }).collect();
            for (x, y, z) in [(0, 1, 2), (1, 0, 2), (0, 2, 1), (2, 0, 1), (1, 2, 0), (2, 1, 0)] {
                for sg in [Rat::one(), Rat::from_int(-1)] {
                    let args = vec![o, pts[x], pts[y], pts[z]];
                    let atom = Reason::Atom { key: AtomKey::RadiusChord, stmt: Stmt::Formula { text: String::new(), pts: args.clone() }, args, from: from.iter().copied().collect() };
                    let mut reasons: Vec<Reason> = keep.iter().map(|&i| l.reasons[i].clone()).collect();
                    reasons.push(atom);
                    let mut comb = others.clone();
                    comb.push(Term { reason: (reasons.len() - 1) as u16, row: 0, coef: sg.clone() });
                    let cand = Link { reasons, combination: comb };
                    let Some(sum) = link_sum(cx, blocks, &cand, false) else { continue };
                    if check::residual_ok(cx, tb, &diff, &sum, true) {
                        out[k] = cand;
                        changed = true;
                        break 'groups;
                    }
                }
            }
        }
    }
    changed.then(|| Sentence::Chain { terms: terms.clone(), links: out, directed: true, then: then.clone() })
}

struct Converse {
    key: TheoremKey,
    lemma: Stmt,
    side: Option<Reason>,
    quad: Vec<PointId>,
}

fn end_angle(e: &Expr) -> Option<(PointId, PointId, PointId, bool)> {
    let one_eighty = Rat::from_int(180);
    match e {
        Expr::Angle { a, b, c, directed: false } => Some((*a, *b, *c, false)),
        Expr::Lin { terms } => match terms.as_slice() {
            [(k1, Expr::Const { degrees }), (k2, Expr::Angle { a, b, c, directed: false })] | [(k2, Expr::Angle { a, b, c, directed: false }), (k1, Expr::Const { degrees })]
                if k1.is_one() && *k2 == Rat::from_int(-1) && *degrees == one_eighty =>
            {
                Some((*a, *b, *c, true))
            }
            _ => None,
        },
        _ => None,
    }
}

fn converse_form(cx: &Ctx, s: &Sentence, set: &[PointId]) -> Option<Converse> {
    let Sentence::Chain { terms, links, directed, .. } = s else { return None };
    if !angle_chain(cx, terms) || terms.len() < 2 {
        return None;
    }
    let plain = Sentence::Chain { terms: terms.clone(), links: links.clone(), directed: *directed, then: None };
    let (e1, e2) = (&terms[0], terms.last()?);
    let dir = *directed && is_directed(e1) && is_directed(e2);
    let resolve = |v: PointId, r: PointId| -> Option<(PointId, bool)> {
        set.iter().copied().filter(|&w| w != v).find_map(|w| {
            if same_ray(cx, v, r, w) {
                Some((w, false))
            } else if opposite_ray(cx, v, r, w) {
                Some((w, true))
            } else {
                None
            }
        })
    };
    let quad_of = |q: [PointId; 4]| -> Vec<PointId> { set.iter().copied().filter(|z| q.contains(z)).collect() };
    let distinct = |q: [PointId; 4]| (0..4).all(|i| (i + 1..4).all(|j| q[i] != q[j]));
    if dir {
        let (Expr::Angle { a: a1, b: p, c: c1, .. }, Expr::Angle { a: a2, b: q, c: c2, .. }) = (e1, e2) else { return None };
        let ((x1, _), (y1, _), (x2, _), (y2, _)) = (resolve(*p, *a1)?, resolve(*p, *c1)?, resolve(*q, *a2)?, resolve(*q, *c2)?);
        if (x1, y1) != (x2, y2) || !distinct([*p, *q, x1, y1]) {
            return None;
        }
        let lemma = Stmt::EqAngle { lhs: Expr::Angle { a: x1, b: *p, c: y1, directed: true }, rhs: Expr::Angle { a: x1, b: *q, c: y1, directed: true } };
        if !check::establishes(cx, &plain, &lemma) {
            return None;
        }
        return Some(Converse { key: TheoremKey::InscribedConverse, lemma, side: None, quad: quad_of([*p, *q, x1, y1]) });
    }
    let (a1, p, c1, s1) = end_angle(e1)?;
    let (a2, q, c2, s2) = end_angle(e2)?;
    let ((x, fx1), (y, fy1), (x2, fx2), (y2, fy2)) = (resolve(p, a1)?, resolve(p, c1)?, resolve(q, a2)?, resolve(q, c2)?);
    if !check::same_set(&[x, y], &[x2, y2]) || !distinct([p, q, x, y]) {
        return None;
    }
    let same = sides(cx.t, p, q, x, y)?;
    let (f1, f2) = (usize::from(fx1) + usize::from(fy1), usize::from(fx2) + usize::from(fy2));
    let ninety = terms[1..terms.len() - 1].iter().any(|e| matches!(e, Expr::Const { degrees } if *degrees == Rat::from_int(90)));
    let ang = |v: PointId| Expr::Angle { a: x, b: v, c: y, directed: false };
    let label = |key: AtomKey, u: PointId, w: PointId| {
        let args = vec![u, w, x, y];
        Reason::Atom { key, stmt: Stmt::Formula { text: String::new(), pts: args.clone() }, args, from: Vec::new() }
    };
    let opposite = || Stmt::Eq { lhs: Expr::Lin { terms: vec![(Rat::one(), ang(p)), (Rat::one(), ang(q))] }, rhs: Expr::Const { degrees: Rat::from_int(180) } };
    let exterior = |v: PointId, r1: PointId, f1: bool, r2: PointId, other: PointId| -> Option<(Stmt, PointId, PointId)> {
        let (u, keep) = if f1 { (resolve(v, r2)?.0, r1) } else { (resolve(v, r1)?.0, r2) };
        let lhs = if f1 { Expr::Angle { a: keep, b: v, c: u, directed: false } } else { Expr::Angle { a: u, b: v, c: keep, directed: false } };
        Some((Stmt::EqAngle { lhs, rhs: ang(other) }, v, other))
    };
    let (key, lemma, side) = match (same, s1, s2, f1, f2) {
        (true, false, false, 0, 0) | (true, true, true, 0, 0) => (TheoremKey::InscribedConverse, Stmt::EqAngle { lhs: ang(p), rhs: ang(q) }, label(AtomKey::SameSide, p, q)),
        (false, false, true, 0, 0) | (false, true, false, 0, 0) => (TheoremKey::OppositeAnglesConverse, opposite(), label(AtomKey::OppositeSides, p, q)),
        (false, false, false, 0, 0) if ninety => (TheoremKey::OppositeAnglesConverse, opposite(), label(AtomKey::OppositeSides, p, q)),
        (false, false, false, 1, 0) => {
            let (st, u, w) = exterior(p, a1, fx1, c1, q)?;
            (TheoremKey::ExteriorAngleConverse, st, label(AtomKey::OppositeSides, u, w))
        }
        (false, false, false, 0, 1) => {
            let (st, u, w) = exterior(q, a2, fx2, c2, p)?;
            (TheoremKey::ExteriorAngleConverse, st, label(AtomKey::OppositeSides, u, w))
        }
        _ => return None,
    };
    if !check::establishes(cx, &plain, &lemma) {
        if debug() {
            eprintln!("converse_form: {lemma:?} not established by the chain {e1:?} .. {e2:?}");
        }
        return None;
    }
    Some(Converse { key, lemma, side: Some(side), quad: quad_of([p, q, x, y]) })
}

fn joins_previous(cx: &Ctx, body: &[Sentence], si: usize) -> bool {
    let (Some(Sentence::Chain { terms: prev, then: None, .. }), Sentence::Chain { terms, .. }) = (si.checked_sub(1).and_then(|j| body.get(j)), &body[si]) else { return false };
    let (Some(last), Some(first)) = (prev.last(), terms.first()) else { return false };
    if last == first {
        return true;
    }
    match (eval(cx.t, last), eval(cx.t, first)) {
        (Some((Table::Angle, x)), Some((Table::Angle, y))) => [&x - &y, &x + &y].iter().any(|d| {
            let q = cx.quot.q(d);
            q.terms.iter().all(|(v, _)| *v == crate::elimination::ANGLE_UNIT) && q.get(crate::elimination::ANGLE_UNIT).is_integer()
        }),
        _ => false,
    }
}

fn cyclic_then(s: &Sentence) -> Option<&Vec<PointId>> {
    match s {
        Sentence::Chain { then: Some(Stmt::Cyclic { pts }), .. } if pts.len() >= 4 => Some(pts),
        _ => None,
    }
}

fn converse_block(cx: &Ctx, hp: &HumanProof, bi: usize) -> Option<HumanProof> {
    let b = &hp.blocks[bi];
    let body = &b.body;
    let wide = body.iter().any(|s| cyclic_then(s).is_some_and(|p| p.len() > 4));
    let target: Option<&Vec<PointId>> = match &b.stmt {
        Stmt::Cyclic { pts } => Some(pts),
        _ => None,
    };
    let mut conv: BTreeMap<usize, Converse> = BTreeMap::new();
    let mut any_then = false;
    for (si, s) in body.iter().enumerate() {
        if joins_previous(cx, body, si) || (si + 1 < body.len() && matches!(s, Sentence::Chain { then: None, .. }) && joins_previous(cx, body, si + 1)) {
            if cyclic_then(s).is_some() {
                return None;
            }
            continue;
        }
        let set = match (cyclic_then(s), s) {
            (Some(p), _) => {
                any_then = true;
                p.clone()
            }
            (None, Sentence::Chain { then: None, .. }) if wide && target.is_some() && body[si + 1..].iter().any(|x| cyclic_then(x).is_some()) => target.cloned().unwrap_or_default(),
            _ => continue,
        };
        match converse_form(cx, s, &set) {
            Some(c) => {
                conv.insert(si, c);
            }
            None if cyclic_then(s).is_some() => return None,
            None => {}
        }
    }
    if !any_then {
        return None;
    }
    let id = b.id;
    let mut map: Vec<u16> = Vec::new();
    let mut out: Vec<Sentence> = Vec::new();
    let mut proved: Vec<(Vec<PointId>, u16)> = Vec::new();
    for (si, s) in body.iter().enumerate() {
        map.push(out.len() as u16);
        let Some(c) = conv.get(&si) else {
            out.push(s.clone());
            continue;
        };
        let Sentence::Chain { terms, links, directed, then } = s else { return None };
        out.push(Sentence::Chain { terms: terms.clone(), links: links.clone(), directed: *directed, then: None });
        let chain_at = (out.len() - 1) as u16;
        let mut reasons = vec![Reason::Lemma { stmt: c.lemma.clone(), block: id, sentence: chain_at }];
        reasons.extend(c.side.iter().cloned());
        out.push(Sentence::Theorem { key: c.key, stmt: Stmt::Cyclic { pts: c.quad.clone() }, reasons });
        proved.push((c.quad.clone(), (out.len() - 1) as u16));
        if let Some(Stmt::Cyclic { pts }) = then {
            if pts.len() > 4 {
                let mut pool: Vec<(Vec<PointId>, Reason)> = proved.iter().rev().map(|(quad, at)| (quad.clone(), Reason::Lemma { stmt: Stmt::Cyclic { pts: quad.clone() }, block: id, sentence: *at })).collect();
                for eb in hp.blocks[..bi].iter().rev() {
                    if let (Stmt::Cyclic { pts: q }, Some(&f)) = (&eb.stmt, eb.engine_facts.first()) {
                        if eb.kind != BlockKind::Raw {
                            pool.push((q.clone(), Reason::Fact { stmt: eb.stmt.clone(), fact: f, block: Some(eb.id), because: Vec::new() }));
                        }
                    }
                }
                pool.retain(|(q, _)| q.iter().all(|p| pts.contains(p)));
                let mut have: Vec<PointId> = c.quad.clone();
                let mut cites: Vec<Reason> = vec![pool[0].1.clone()];
                let mut used = vec![false; pool.len()];
                used[0] = true;
                while !pts.iter().all(|p| have.contains(p)) {
                    let next = (0..pool.len()).find(|&i| !used[i] && pool[i].0.iter().filter(|p| have.contains(p)).count() >= 3 && pool[i].0.iter().any(|p| !have.contains(p)));
                    let Some(i) = next else { return None };
                    used[i] = true;
                    cites.push(pool[i].1.clone());
                    for &p in &pool[i].0 {
                        if !have.contains(&p) {
                            have.push(p);
                        }
                    }
                }
                cites.reverse();
                if check::merge_ok(&Stmt::Cyclic { pts: pts.clone() }, &cites).is_err() {
                    return None;
                }
                out.push(Sentence::Theorem { key: TheoremKey::CircleMerge, stmt: Stmt::Cyclic { pts: pts.clone() }, reasons: cites });
                proved.push((pts.clone(), (out.len() - 1) as u16));
            }
        }
    }
    let mut cand = hp.clone();
    let remap = |s: &mut Sentence| {
        super::claims::sentence_reasons_mut(s, &mut |r| {
            if let Reason::Lemma { block, sentence, .. } = r {
                if *block == id {
                    if let Some(&n) = map.get(*sentence as usize) {
                        *sentence = n;
                    }
                }
            }
        });
    };
    for (k, blk) in cand.blocks.iter_mut().enumerate() {
        if k != bi {
            blk.body.iter_mut().for_each(remap);
        }
    }
    for &k in &map {
        if let Some(s) = out.get_mut(k as usize) {
            remap(s);
        }
    }
    cand.blocks[bi].body = out;
    Some(cand)
}

pub fn converse_steps(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut all = hp.clone();
    let mut any = false;
    for bi in 0..all.blocks.len() {
        if all.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        if let Some(cand) = converse_block(cx, &all, bi) {
            all = cand;
            any = true;
        }
    }
    if !any {
        return;
    }
    if check::violations_with(ch, &all).is_empty() {
        *hp = all;
        return;
    }
    for bi in 0..hp.blocks.len() {
        if cx.timed_out() {
            return;
        }
        if hp.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        let Some(cand) = converse_block(cx, hp, bi) else { continue };
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
        } else if debug() {
            eprintln!("barem converse in block {} rejected: {:?}", hp.blocks[bi].id, v.first().map(|x| &x.detail));
        }
    }
}

