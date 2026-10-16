use super::barem::same_ray;
use super::check;
use super::ctx::Ctx;
use super::cyclic::angle_chain;
use super::expr::eval;
use super::model::*;
use super::trace::Table;
use crate::predicate::PointId;
use crate::rational::Rat;

fn base_angles(cx: &Ctx, e: &Expr, v: PointId, rays: [PointId; 2]) -> bool {
    match e {
        Expr::Angle {
            a,
            b,
            c,
            directed: false,
        } if *b == v => {
            (same_ray(cx, v, *a, rays[0]) && same_ray(cx, v, *c, rays[1]))
                || (same_ray(cx, v, *a, rays[1]) && same_ray(cx, v, *c, rays[0]))
        }
        _ => false,
    }
}

pub fn arc_chord_isosceles(cx: &Ctx, b: &Block) -> Option<Block> {
    let mut nb = b.clone();
    let mut changed = false;
    for si in 1..b.body.len() {
        let Sentence::Theorem {
            key: TheoremKey::ArcChord,
            stmt: st @ Stmt::Cong { s1, s2 },
            ..
        } = &b.body[si]
        else {
            continue;
        };
        let Sentence::Chain { terms, .. } = &b.body[si - 1] else {
            continue;
        };
        let Some(o) = [s1.0, s1.1].into_iter().find(|p| *p == s2.0 || *p == s2.1) else {
            continue;
        };
        let p = if s1.0 == o { s1.1 } else { s1.0 };
        let q = if s2.0 == o { s2.1 } else { s2.0 };
        let (Some(e1), Some(e2)) = (terms.first(), terms.last()) else {
            continue;
        };
        let ok = (base_angles(cx, e1, p, [o, q]) && base_angles(cx, e2, q, [o, p]))
            || (base_angles(cx, e1, q, [o, p]) && base_angles(cx, e2, p, [o, q]));
        if !ok {
            continue;
        }
        let args = vec![o, p, q];
        let atom = Reason::Atom {
            key: AtomKey::IsoscelesConverse,
            stmt: super::claims::atom_stmt(AtomKey::IsoscelesConverse, &args),
            args,
            from: Vec::new(),
        };
        let Some(rows) = check::reason_rows(cx.t, &[], &atom) else {
            continue;
        };
        let Some(targets) = check::conclusion_forms(cx.t, st) else {
            continue;
        };
        let Some((_, row)) = rows.first() else {
            continue;
        };
        let coef = [Rat::one(), Rat::from_int(-1)].into_iter().find(|k| {
            let mut x = row.clone();
            x.mul_assign_scalar(k);
            targets.iter().any(|(tb, y)| {
                *tb == Table::Ratio && check::residual_ok(cx, Table::Ratio, y, &x, true)
            })
        });
        let Some(coef) = coef else { continue };
        nb.body[si] = Sentence::Because {
            stmt: st.clone(),
            reasons: vec![atom],
            combination: vec![Term {
                reason: 0,
                row: 0,
                coef,
            }],
        };
        changed = true;
    }
    changed.then_some(nb)
}

fn ray_coll(cx: &Ctx, blocks: &[Block], v: PointId, x: PointId, y: PointId) -> Option<Reason> {
    let trio = [x, v, y];
    let hyp = cx
        .hyp_pred
        .iter()
        .filter(|(f, p)| {
            p.name == "coll"
                && matches!(cx.class.get(**f as usize), Some(super::ctx::FactClass::Hyp))
                && trio.iter().all(|z| p.points.contains(z))
        })
        .map(|(f, _)| *f)
        .min();
    if let Some(fact) = hyp {
        return Some(Reason::Hyp {
            stmt: Stmt::Coll { pts: trio.to_vec() },
            fact,
        });
    }
    let blk = blocks.iter().find(|bk| {
        bk.kind != BlockKind::Raw
            && matches!(&bk.stmt, Stmt::Coll { pts } if trio.iter().all(|z| pts.contains(z)))
    })?;
    Some(Reason::Fact {
        stmt: blk.stmt.clone(),
        fact: *blk.engine_facts.first()?,
        block: Some(blk.id),
        because: Vec::new(),
    })
}

fn angle_facts(cx: &Ctx, body: &[Sentence]) -> usize {
    body.iter()
        .filter(|s| match s {
            Sentence::Chain { terms, .. } => angle_chain(cx, terms),
            Sentence::Because {
                stmt: Stmt::EqAngle { .. } | Stmt::AngleConst { .. },
                ..
            } => true,
            _ => false,
        })
        .count()
}

fn ratio_facts(cx: &Ctx, body: &[Sentence]) -> usize {
    body.iter()
        .filter(|s| match s {
            Sentence::Chain { terms, .. } | Sentence::Computation { terms, .. } => {
                terms.first().is_some_and(|e| {
                    !super::expr::has_angle(e)
                        && eval(cx.t, e).is_some_and(|(tb, _)| tb == Table::Ratio)
                })
            }
            Sentence::Because {
                stmt: Stmt::EqRatio { .. } | Stmt::RatioConst { .. },
                ..
            } => true,
            _ => false,
        })
        .count()
}

pub fn common_angle(cx: &Ctx, blocks: &[Block], b: &Block) -> Option<Block> {
    let (Stmt::Sim { t1, t2, .. } | Stmt::Congruent { t1, t2, .. }) = &b.stmt else {
        return None;
    };
    let last = b.body.len().checked_sub(1)?;
    if !matches!(
        &b.body[last],
        Sentence::Because {
            stmt: Stmt::Sim { .. } | Stmt::Congruent { .. },
            ..
        }
    ) {
        return None;
    }
    let shown = angle_facts(cx, &b.body);
    if shown > 1
        || (shown == 0 && (!matches!(b.stmt, Stmt::Sim { .. }) || ratio_facts(cx, &b.body) != 1))
    {
        return None;
    }
    let (a, c) = ([t1.0, t1.1, t1.2], [t2.0, t2.1, t2.2]);
    if super::barem::debug() {
        eprintln!("common_angle block {}: {:?} {:?}", b.id, a, c);
    }
    for i in 0..3 {
        if a[i] != c[i] {
            continue;
        }
        let v = a[i];
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        let direct = [(a[j], c[j]), (a[k], c[k])];
        let crossed = [(a[j], c[k]), (a[k], c[j])];
        let pairs = if direct.iter().all(|&(x, y)| same_ray(cx, v, x, y)) {
            direct
        } else if crossed.iter().all(|&(x, y)| same_ray(cx, v, x, y)) {
            crossed
        } else {
            continue;
        };
        let lhs = Expr::Angle {
            a: a[j],
            b: v,
            c: a[k],
            directed: false,
        };
        let rhs = Expr::Angle {
            a: c[j],
            b: v,
            c: c[k],
            directed: false,
        };
        if pairs.iter().all(|(x, y)| x == y) {
            continue;
        }
        if let Some(link) = super::barem::identity_link(cx, &lhs, &rhs, false) {
            if matches!(
                link.reasons.first(),
                Some(Reason::Atom {
                    key: AtomKey::CommonAngle,
                    ..
                })
            ) {
                let mut nb = b.clone();
                nb.body.insert(
                    last,
                    Sentence::Chain {
                        terms: vec![lhs, rhs],
                        links: vec![link],
                        then: None,
                        directed: false,
                    },
                );
                return Some(nb);
            }
        }
        let mut reasons: Vec<Reason> = Vec::new();
        for &(x, y) in &pairs {
            if x != y {
                reasons.push(ray_coll(cx, blocks, v, x, y)?);
            }
        }
        let (Some((_, e0)), Some((_, e1))) = (eval(cx.t, &lhs), eval(cx.t, &rhs)) else {
            continue;
        };
        let diff = &e0 - &e1;
        let rows: Vec<Vec<(Table, crate::lincomb::LinComb)>> = reasons
            .iter()
            .map(|r| check::reason_rows(cx.t, blocks, r).unwrap_or_default())
            .collect();
        let signs = [Rat::one(), Rat::from_int(-1)];
        let mut combination: Option<Vec<Term>> = None;
        'search: for s0 in &signs {
            for s1 in &signs {
                let mut sum = crate::lincomb::LinComb::zero();
                let mut comb = Vec::new();
                for (ri, rs) in rows.iter().enumerate() {
                    let Some((_, r)) = rs.first() else {
                        continue 'search;
                    };
                    let sg = if ri == 0 { s0 } else { s1 };
                    sum.iadd_mul(&cx.t.exact(Table::Angle, r), sg);
                    comb.push(Term {
                        reason: ri as u16,
                        row: 0,
                        coef: sg.clone(),
                    });
                }
                if check::residual_ok(cx, Table::Angle, &diff, &sum, false) {
                    combination = Some(comb);
                    break 'search;
                }
            }
        }
        let Some(combination) = combination else {
            continue;
        };
        let mut nb = b.clone();
        nb.body.insert(
            last,
            Sentence::Because {
                stmt: Stmt::EqAngle { lhs, rhs },
                reasons,
                combination,
            },
        );
        return Some(nb);
    }
    None
}

fn same_angle_as(cx: &Ctx, e: &Expr, v: PointId, p: PointId, q: PointId) -> bool {
    base_angles(cx, e, v, [p, q])
}

pub fn triangle_names(cx: &Ctx, b: &Block) -> Option<Block> {
    let (Stmt::Sim { t1, t2, .. } | Stmt::Congruent { t1, t2, .. }) = &b.stmt else {
        return None;
    };
    let (a, c) = ([t1.0, t1.1, t1.2], [t2.0, t2.1, t2.2]);
    let corner = |t: &[PointId; 3], i: usize| Expr::Angle {
        a: t[(i + 1) % 3],
        b: t[i],
        c: t[(i + 2) % 3],
        directed: false,
    };
    let mut nb = b.clone();
    let mut changed = false;
    for s in nb.body.iter_mut() {
        let Sentence::Chain {
            terms,
            directed: false,
            ..
        } = s
        else {
            continue;
        };
        let n = terms.len();
        if n < 2 {
            continue;
        }
        for k in [0, n - 1] {
            let Expr::Angle { b: v, .. } = terms[k] else {
                continue;
            };
            for t in [&a, &c] {
                let Some(i) = t.iter().position(|&p| p == v) else {
                    continue;
                };
                let want = corner(t, i);
                let same_points = matches!((&terms[k], &want), (Expr::Angle { a: p, c: q, .. }, Expr::Angle { a: p2, c: q2, .. }) if (p == p2 && q == q2) || (p == q2 && q == p2));
                if same_points || !same_angle_as(cx, &terms[k], v, t[(i + 1) % 3], t[(i + 2) % 3]) {
                    continue;
                }
                let (Some((_, e0)), Some((_, e1))) = (eval(cx.t, &terms[k]), eval(cx.t, &want))
                else {
                    continue;
                };
                if check::residual_ok(cx, Table::Angle, &e0, &e1, false) {
                    terms[k] = want;
                    changed = true;
                    break;
                }
            }
        }
        let (x, y) = (terms[0].clone(), terms[n - 1].clone());
        let Expr::Angle { b: vx, .. } = x else {
            continue;
        };
        let Expr::Angle {
            a: ya,
            b: vy,
            c: yc,
            ..
        } = y
        else {
            continue;
        };
        if vx != vy || !same_angle_as(cx, &x, vy, ya, yc) {
            continue;
        }
        'found: for i in 0..3 {
            for (from, to) in [(&a, &c), (&c, &a)] {
                if !same_angle_as(cx, &x, from[i], from[(i + 1) % 3], from[(i + 2) % 3]) {
                    continue;
                }
                let target = corner(to, i);
                if same_angle_as(cx, &target, vx, ya, yc) {
                    continue;
                }
                let (Some((_, ey)), Some((_, et))) = (eval(cx.t, &y), eval(cx.t, &target)) else {
                    continue;
                };
                if !check::residual_ok(cx, Table::Angle, &ey, &et, false) {
                    continue;
                }
                terms[n - 1] = target;
                changed = true;
                break 'found;
            }
        }
    }
    changed.then_some(nb)
}

fn cong_row(
    cx: &Ctx,
    x: (PointId, PointId),
    y: (PointId, PointId),
) -> Option<crate::lincomb::LinComb> {
    Some(&cx.t.dm(x.0, x.1)? - &cx.t.dm(y.0, y.1)?)
}

pub fn hence_premises(cx: &Ctx, before: &[Block], b: &Block) -> Option<Block> {
    let last = b.body.len().checked_sub(1)?;
    let Sentence::Because {
        stmt: st @ Stmt::Cong { s1, s2 },
        reasons,
        combination,
    } = &b.body[last]
    else {
        return None;
    };
    if !reasons.is_empty() || !combination.is_empty() || last == 0 || *st != b.stmt {
        return None;
    }
    let Sentence::Because {
        stmt: tri @ Stmt::Congruent { t1, t2, .. },
        ..
    } = &b.body[last - 1]
    else {
        return None;
    };
    let want = cong_row(cx, *s1, *s2)?;
    let (a, c) = ([t1.0, t1.1, t1.2], [t2.0, t2.1, t2.2]);
    let mut known: Vec<(crate::lincomb::LinComb, Reason)> = Vec::new();
    for eb in before.iter().filter(|eb| eb.kind != BlockKind::Raw) {
        if let (Stmt::Cong { s1: x, s2: y }, Some(&f)) = (&eb.stmt, eb.engine_facts.first()) {
            if let Some(r) = cong_row(cx, *x, *y) {
                known.push((
                    r,
                    Reason::Fact {
                        stmt: eb.stmt.clone(),
                        fact: f,
                        block: Some(eb.id),
                        because: Vec::new(),
                    },
                ));
            }
        }
    }
    for &f in &cx.closure {
        if !matches!(cx.class.get(f as usize), Some(super::ctx::FactClass::Hyp)) {
            continue;
        }
        if let st2 @ Stmt::Cong { s1: x, s2: y } = super::classify::fact_stmt(cx, f) {
            if let Some(r) = cong_row(cx, x, y) {
                known.push((
                    r,
                    Reason::Hyp {
                        stmt: st2.clone(),
                        fact: f,
                    },
                ));
            }
        }
    }
    let signs = [Rat::one(), Rat::from_int(-1)];
    for (i, j) in [(0, 1), (1, 2), (0, 2)] {
        let Some(side) = cong_row(cx, (a[i], a[j]), (c[i], c[j])) else {
            continue;
        };
        if cx.t.canon_ratio(&side).is_zero() {
            continue;
        }
        for (r, why) in &known {
            for k1 in &signs {
                for k2 in &signs {
                    let mut rest = want.clone();
                    rest.iadd_mul(&side, &-k1);
                    rest.iadd_mul(r, &-k2);
                    if cx.t.canon_ratio(&rest).is_zero() {
                        let mut nb = b.clone();
                        let reasons = vec![
                            Reason::Lemma {
                                stmt: tri.clone(),
                                block: b.id,
                                sentence: (last - 1) as u16,
                            },
                            why.clone(),
                        ];
                        nb.body[last] = Sentence::Because {
                            stmt: st.clone(),
                            reasons,
                            combination: Vec::new(),
                        };
                        return Some(nb);
                    }
                }
            }
        }
    }
    None
}
