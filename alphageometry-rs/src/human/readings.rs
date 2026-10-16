use super::barem::{debug, near_line, opposite_ray};
use super::check::{self, sides, Checker};
use super::ctx::Ctx;
use super::expr::{eval, is_directed};
use super::model::*;
use super::trace::Table;
use crate::predicate::PointId;
use crate::rational::Rat;

struct Reading {
    key: TheoremKey,
    lemma: Stmt,
    line: Option<Reason>,
    line_pts: Option<[PointId; 3]>,
    side: Reason,
}

fn plain(e: &Expr) -> Option<(PointId, PointId, PointId)> {
    match e {
        Expr::Angle {
            a,
            b,
            c,
            directed: false,
        } => Some((*a, *b, *c)),
        _ => None,
    }
}

fn pair_sum(e: &Expr) -> Option<((PointId, PointId, PointId), (PointId, PointId, PointId))> {
    let Expr::Lin { terms } = e else { return None };
    let [(k1, e1), (k2, e2)] = terms.as_slice() else {
        return None;
    };
    (k1.is_one() && k2.is_one()).then_some(())?;
    Some((plain(e1)?, plain(e2)?))
}

fn is_180(e: &Expr) -> bool {
    matches!(e, Expr::Const { degrees } if *degrees == Rat::from_int(180))
}

pub(crate) fn side_label(key: AtomKey, args: Vec<PointId>) -> Reason {
    Reason::Atom {
        key,
        stmt: Stmt::Formula {
            text: String::new(),
            pts: args.clone(),
        },
        args,
        from: Vec::new(),
    }
}

fn same_line(cx: &Ctx, a: (PointId, PointId), b: (PointId, PointId)) -> bool {
    match (cx.t.dir(a.0, a.1), cx.t.dir(b.0, b.1)) {
        (Some(x), Some(y)) => cx
            .quot
            .q(&(&x - &y))
            .terms
            .iter()
            .all(|(v, _)| *v == crate::elimination::ANGLE_UNIT),
        _ => false,
    }
}

fn line_reason(cx: &Ctx, pts: &[PointId]) -> Option<Reason> {
    let l = cx
        .lines
        .iter()
        .filter(|l| pts.iter().all(|p| l.pts.contains(p)))
        .min_by_key(|l| (l.src.len(), l.fact))?;
    let hyp = matches!(
        cx.class.get(l.fact as usize),
        Some(super::ctx::FactClass::Hyp)
    ) && cx.in_cl.get(l.fact as usize).copied().unwrap_or(false);
    hyp.then(|| Reason::Hyp {
        stmt: super::classify::dehelper(cx, super::classify::fact_stmt(cx, l.fact)),
        fact: l.fact,
    })
}

fn vertical(cx: &Ctx, e1: &Expr, e2: &Expr, pts: &[PointId]) -> Option<Reading> {
    let (p1, v, p2) = plain(e1)?;
    let (q1, w, q2) = plain(e2)?;
    if v != w || !pts.contains(&v) {
        return None;
    }
    let rest: Vec<PointId> = pts.iter().copied().filter(|&p| p != v).collect();
    let [x, y] = rest[..] else { return None };
    for (xa, a) in [(p1, p2), (p2, p1)] {
        for (yb, b) in [(q1, q2), (q2, q1)] {
            let ends = (xa == x && yb == y) || (xa == y && yb == x);
            if !ends || pts.contains(&a) || pts.contains(&b) || a == b {
                continue;
            }
            if !opposite_ray(cx, v, a, b) || near_line(cx, a, b, xa) {
                continue;
            }
            if sides(cx.t, xa, yb, a, b) != Some(false) {
                continue;
            }
            if !same_line(cx, (v, a), (v, b)) {
                continue;
            }
            let line = line_reason(cx, &[a, v, b]);
            return Some(Reading {
                line_pts: Some([a, v, b]),
                key: TheoremKey::VerticalAnglesConverse,
                lemma: Stmt::EqAngle {
                    lhs: e1.clone(),
                    rhs: e2.clone(),
                },
                line,
                side: side_label(AtomKey::OppositeSides, vec![xa, yb, a, b]),
            });
        }
    }
    None
}

fn linear_pair(cx: &Ctx, e1: &Expr, e2: &Expr, pts: &[PointId]) -> Option<Reading> {
    if !is_180(e2) {
        return None;
    }
    let ((x, v, w), (w2, v2, y)) = pair_sum(e1)?;
    if v != v2 || w != w2 || !check::same_set(&[x, v, y], pts) {
        return None;
    }
    if sides(cx.t, x, y, v, w) != Some(false) {
        return None;
    }
    Some(Reading {
        key: TheoremKey::LinearPairConverse,
        lemma: Stmt::Eq {
            lhs: e1.clone(),
            rhs: e2.clone(),
        },
        line: None,
        line_pts: None,
        side: side_label(AtomKey::OppositeSides, vec![x, y, v, w]),
    })
}

fn same_ray(cx: &Ctx, v: PointId, a: PointId, p: PointId) -> bool {
    let (o, x, y) = (cx.t.coord(v), cx.t.coord(a), cx.t.coord(p));
    a != v
        && p != v
        && (a == p
            || (near_line(cx, v, a, p)
                && (x.0 - o.0) * (y.0 - o.0) + (x.1 - o.1) * (y.1 - o.1) > 0.0))
}

fn same_arm(cx: &Ctx, e1: &Expr, e2: &Expr, pts: &[PointId]) -> Option<Reading> {
    let (p1, v, p2) = plain(e1)?;
    let (q1, w, q2) = plain(e2)?;
    if v != w || !pts.contains(&v) {
        return None;
    }
    let rest: Vec<PointId> = pts.iter().copied().filter(|&p| p != v).collect();
    let [x, y] = rest[..] else { return None };
    for (xa, a) in [(p1, p2), (p2, p1)] {
        for (yb, b) in [(q1, q2), (q2, q1)] {
            let ends = (xa == x && yb == y) || (xa == y && yb == x);
            if !ends || pts.contains(&a) || pts.contains(&b) {
                continue;
            }
            if !same_ray(cx, v, a, b) || (a != b && !same_line(cx, (v, a), (v, b))) {
                continue;
            }
            if near_line(cx, v, a, xa) || sides(cx.t, xa, yb, v, a) != Some(true) {
                continue;
            }
            let line = if a == b {
                None
            } else {
                line_reason(cx, &[v, a, b])
            };
            return Some(Reading {
                line_pts: (a != b).then_some([v, a, b]),
                key: TheoremKey::AngleConstruction,
                lemma: Stmt::EqAngle {
                    lhs: e1.clone(),
                    rhs: e2.clone(),
                },
                line,
                side: side_label(AtomKey::SameSide, vec![xa, yb, v, a]),
            });
        }
    }
    None
}

fn toward(cx: &Ctx, v: PointId, a: PointId, other: PointId) -> Option<bool> {
    if !same_line(cx, (v, a), (v, other)) {
        return None;
    }
    if same_ray(cx, v, a, other) {
        Some(true)
    } else if opposite_ray(cx, v, a, other) {
        Some(false)
    } else {
        None
    }
}

fn arms(cx: &Ctx, t: (PointId, PointId, PointId), other: PointId) -> Vec<(PointId, bool)> {
    let mut out = Vec::new();
    for (free, along) in [(t.0, t.2), (t.2, t.0)] {
        if free == other || near_line(cx, t.1, other, free) {
            continue;
        }
        if let Some(s) = toward(cx, t.1, along, other) {
            out.push((free, s));
        }
    }
    out
}

fn parallel(
    cx: &Ctx,
    e1: &Expr,
    e2: &Expr,
    l1: (PointId, PointId),
    l2: (PointId, PointId),
) -> Option<Reading> {
    let t1 = plain(e1)?;
    let (equal, t2) = match (plain(e2), e2) {
        (Some(t), _) => (true, t),
        (None, Expr::Lin { terms }) => match terms.as_slice() {
            [(k1, c), (k2, a)] if k1.is_one() && *k2 == Rat::from_int(-1) && is_180(c) => {
                (false, plain(a)?)
            }
            _ => return None,
        },
        _ => return None,
    };
    let (x, y) = (t1.1, t2.1);
    if x == y {
        return None;
    }
    let on = |a: (PointId, PointId), l: (PointId, PointId)| same_line(cx, a, l);
    for (u, s1) in arms(cx, t1, y) {
        for (v, s2) in arms(cx, t2, x) {
            let fits = (on((u, x), l1) && on((y, v), l2)) || (on((u, x), l2) && on((y, v), l1));
            if !fits || u == v {
                continue;
            }
            let key = match (equal, s1, s2) {
                (true, true, true) | (true, false, false) => TheoremKey::AlternateAnglesConverse,
                (true, _, _) => TheoremKey::CorrespondingAnglesConverse,
                (false, true, true) => TheoremKey::CointeriorAnglesConverse,
                _ => continue,
            };
            let opposite = key == TheoremKey::AlternateAnglesConverse;
            if sides(cx.t, u, v, x, y) != Some(!opposite) {
                continue;
            }
            let lemma = if equal {
                Stmt::EqAngle {
                    lhs: e1.clone(),
                    rhs: e2.clone(),
                }
            } else {
                let second = Expr::Angle {
                    a: t2.0,
                    b: t2.1,
                    c: t2.2,
                    directed: false,
                };
                Stmt::Eq {
                    lhs: Expr::Lin {
                        terms: vec![(Rat::one(), e1.clone()), (Rat::one(), second)],
                    },
                    rhs: Expr::Const {
                        degrees: Rat::from_int(180),
                    },
                }
            };
            let side = if opposite {
                AtomKey::OppositeSides
            } else {
                AtomKey::SameSide
            };
            return Some(Reading {
                key,
                lemma,
                line: None,
                line_pts: None,
                side: side_label(side, vec![u, v, x, y]),
            });
        }
    }
    None
}

fn three_reading(cx: &Ctx, e1: &Expr, e2: &Expr, pts: &[PointId]) -> Option<Reading> {
    vertical(cx, e1, e2, pts)
        .or_else(|| same_arm(cx, e1, e2, pts))
        .or_else(|| linear_pair(cx, e1, e2, pts))
}

fn coll_reading(cx: &Ctx, e1: &Expr, e2: &Expr, pts: &[PointId]) -> Option<Reading> {
    match pts.len() {
        3 => three_reading(cx, e1, e2, pts),
        4 => pts.iter().find_map(|&p| {
            let rest: Vec<PointId> = pts.iter().copied().filter(|&q| q != p).collect();
            let on_given = cx.lines.iter().any(|l| {
                l.pts.contains(&p)
                    && rest.iter().filter(|q| l.pts.contains(q)).count() >= 2
                    && matches!(
                        cx.class.get(l.fact as usize),
                        Some(super::ctx::FactClass::Hyp | super::ctx::FactClass::HypReg)
                    )
            });
            on_given.then(|| three_reading(cx, e1, e2, &rest)).flatten()
        }),
        _ => None,
    }
}

fn reading(cx: &Ctx, s: &Sentence) -> Option<(Reading, Stmt)> {
    let Sentence::Chain {
        terms,
        links,
        directed,
        then: Some(st),
    } = s
    else {
        return None;
    };
    if links.len() < 2 || (*directed && terms.iter().any(is_directed)) {
        return None;
    }
    if !terms
        .first()
        .and_then(|e| eval(cx.t, e))
        .is_some_and(|x| x.0 == Table::Angle)
    {
        return None;
    }
    let (e1, e2) = (&terms[0], terms.last()?);
    let r = match st {
        Stmt::Coll { pts } => coll_reading(cx, e1, e2, pts),
        Stmt::Para { l1, l2 } => parallel(cx, e1, e2, *l1, *l2),
        _ => None,
    }?;
    let plain_chain = Sentence::Chain {
        terms: terms.clone(),
        links: links.clone(),
        directed: *directed,
        then: None,
    };
    if !check::establishes(cx, &plain_chain, &r.lemma) {
        if debug() {
            eprintln!("readings: {:?} not established by its chain", r.lemma);
        }
        return None;
    }
    Some((r, st.clone()))
}

fn given_line(cx: &Ctx, pts: &[PointId]) -> bool {
    cx.lines.iter().any(|l| {
        pts.iter().all(|p| l.pts.contains(p))
            && matches!(
                cx.class.get(l.fact as usize),
                Some(super::ctx::FactClass::Hyp | super::ctx::FactClass::HypReg)
            )
    })
}

fn claim_line(cx: &Ctx, earlier: &[Block], pts: &[PointId]) -> Option<Reason> {
    for l in cx
        .lines
        .iter()
        .filter(|l| pts.iter().all(|p| l.pts.contains(p)))
    {
        let shown = cx.displayable(l.fact);
        for b in earlier {
            let BlockKind::Claim(n) = b.kind else {
                continue;
            };
            for f in [l.fact, shown] {
                if b.engine_facts.contains(&f) {
                    return Some(Reason::Claim {
                        n,
                        block: b.id,
                        fact: f,
                    });
                }
            }
        }
    }
    None
}

fn reading_block(cx: &Ctx, hp: &HumanProof, bi: usize) -> Option<HumanProof> {
    let b = &hp.blocks[bi];
    let id = b.id;
    let mut map: Vec<u16> = Vec::new();
    let mut out: Vec<Sentence> = Vec::new();
    let mut any = false;
    for s in &b.body {
        map.push(out.len() as u16);
        let Some((mut r, st)) = reading(cx, s) else {
            out.push(s.clone());
            continue;
        };
        if r.line.is_none() {
            if let Some(pts) = r.line_pts {
                r.line = claim_line(cx, &hp.blocks[..bi], &pts);
                if r.line.is_none() && !given_line(cx, &pts) {
                    if debug() {
                        eprintln!("readings: no citable line for {pts:?}");
                    }
                    out.push(s.clone());
                    continue;
                }
            }
        }
        let Sentence::Chain {
            terms,
            links,
            directed,
            ..
        } = s
        else {
            return None;
        };
        out.push(Sentence::Chain {
            terms: terms.clone(),
            links: links.clone(),
            directed: *directed,
            then: None,
        });
        let at = (out.len() - 1) as u16;
        let mut reasons = vec![Reason::Lemma {
            stmt: r.lemma,
            block: id,
            sentence: at,
        }];
        reasons.extend(r.line);
        reasons.push(r.side);
        out.push(Sentence::Theorem {
            key: r.key,
            stmt: st,
            reasons,
        });
        any = true;
    }
    if !any {
        return None;
    }
    let mut cand = hp.clone();
    let remap = |s: &mut Sentence| {
        super::claims::sentence_reasons_mut(s, &mut |r| {
            if let Reason::Lemma {
                block, sentence, ..
            } = r
            {
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
    let fixed: Vec<usize> = map.iter().map(|&k| k as usize).collect();
    for k in fixed {
        if let Some(s) = out.get_mut(k) {
            remap(s);
        }
    }
    cand.blocks[bi].body = out;
    Some(cand)
}

pub fn line_readings(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    for bi in 0..hp.blocks.len() {
        if cx.timed_out() {
            return;
        }
        if hp.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        let Some(cand) = reading_block(cx, hp, bi) else {
            continue;
        };
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
        } else if debug() {
            eprintln!(
                "readings in block {} rejected: {:?}",
                hp.blocks[bi].id,
                v.first().map(|x| &x.detail)
            );
        }
    }
}

pub fn line_converse_ok(
    cx: &Ctx,
    key: TheoremKey,
    stmt: &Stmt,
    reasons: &[Reason],
) -> Result<(), String> {
    let Some(Reason::Lemma { stmt: ls, .. }) = reasons.first() else {
        return Err("a line converse cites the angle relation it uses first".into());
    };
    let has_side = |k: AtomKey, args: &[PointId]| {
        reasons.iter().any(|r| {
            matches!(r, Reason::Atom { key, args: a, .. }
                if *key == k && a.len() == 4
                    && check::same_set(&a[0..2], &args[0..2])
                    && check::same_set(&a[2..4], &args[2..4]))
        })
    };
    let r = match (key, stmt, ls) {
        (
            TheoremKey::VerticalAnglesConverse
            | TheoremKey::AngleConstruction
            | TheoremKey::LinearPairConverse,
            Stmt::Coll { pts },
            Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs },
        ) => coll_reading(cx, lhs, rhs, pts)
            .ok_or("the angle relation does not place the points on one line, as drawn")?,
        (
            TheoremKey::AlternateAnglesConverse | TheoremKey::CorrespondingAnglesConverse,
            Stmt::Para { l1, l2 },
            Stmt::EqAngle { lhs, rhs },
        ) => parallel(cx, lhs, rhs, *l1, *l2)
            .ok_or("not alternate or corresponding angles on a transversal, as drawn")?,
        (TheoremKey::CointeriorAnglesConverse, Stmt::Para { l1, l2 }, Stmt::Eq { lhs, rhs }) => {
            let Expr::Lin { terms } = lhs else {
                return Err("co-interior angles: a sum of two angles".into());
            };
            let [(_, a), (_, b)] = terms.as_slice() else {
                return Err("co-interior angles: a sum of two angles".into());
            };
            if !is_180(rhs) {
                return Err("co-interior angles sum to 180°".into());
            }
            let sup = Expr::Lin {
                terms: vec![
                    (
                        Rat::one(),
                        Expr::Const {
                            degrees: Rat::from_int(180),
                        },
                    ),
                    (Rat::from_int(-1), b.clone()),
                ],
            };
            parallel(cx, a, &sup, *l1, *l2)
                .ok_or("not co-interior angles on a transversal, as drawn")?
        }
        _ => return Err("line converse with a statement of the wrong shape".into()),
    };
    if r.key != key {
        return Err("the line converse is not the one the figure reads".into());
    }
    let Reason::Atom { key: sk, args, .. } = &r.side else {
        return Err("no side condition".into());
    };
    if !has_side(*sk, args) {
        return Err("a line converse must state on which sides the points lie".into());
    }
    Ok(())
}
