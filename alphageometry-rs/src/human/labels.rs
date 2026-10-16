use super::barem::{is_coll_reason, near_line, same_ray};
use super::ctx::Ctx;
use super::expr::eval;
use super::model::*;
use super::trace::Table;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::rational::Rat;

fn label(key: AtomKey, args: Vec<PointId>) -> Reason {
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

fn hyp_cong(cx: &Ctx, c: PointId, p: PointId, q: PointId) -> bool {
    let seg = |a: PointId, b: PointId| (a.min(b), a.max(b));
    let want = [seg(c, p), seg(c, q)];
    cx.hyp_pred.values().any(|h| {
        h.name == "cong" && h.points.len() == 4 && {
            let got = [seg(h.points[0], h.points[1]), seg(h.points[2], h.points[3])];
            (got[0] == want[0] && got[1] == want[1]) || (got[0] == want[1] && got[1] == want[0])
        }
    })
}

pub fn reflection_args(cx: &Ctx, a: &[PointId]) -> Option<Vec<PointId>> {
    let [x, y, p, q] = a else { return None };
    if !hyp_cong(cx, *x, *p, *q) || !hyp_cong(cx, *y, *p, *q) {
        return None;
    }
    let (src, img) = if p > q { (*q, *p) } else { (*p, *q) };
    (img > *x && img > *y).then(|| vec![*x, *y, src, img])
}

pub fn reflections(cx: &Ctx, s: &Sentence) -> Option<Sentence> {
    let mut out = s.clone();
    let mut changed = false;
    super::claims::sentence_reasons_mut(&mut out, &mut |r| {
        if let Reason::Atom { key, args, .. } = r {
            if *key == AtomKey::PerpBisector {
                if let Some(a) = reflection_args(cx, args) {
                    *key = AtomKey::Reflection;
                    *args = a;
                    changed = true;
                }
            }
        }
    });
    changed.then_some(out)
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

fn changed_angle(
    a: &Expr,
    b: &Expr,
) -> Option<((PointId, PointId, PointId), (PointId, PointId, PointId))> {
    match (a, b) {
        (Expr::Angle { .. }, Expr::Angle { .. }) => Some((plain(a)?, plain(b)?)),
        (Expr::Lin { terms: x }, Expr::Lin { terms: y }) if x.len() == y.len() => {
            let mut found = None;
            for ((k1, e1), (k2, e2)) in x.iter().zip(y.iter()) {
                if k1 != k2 {
                    return None;
                }
                if e1 == e2 {
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                found = Some((plain(e1)?, plain(e2)?));
            }
            found
        }
        _ => None,
    }
}

pub fn inner_outer(cx: &Ctx, v: PointId, x: PointId, y: PointId) -> Option<(PointId, PointId)> {
    if x == y || x == v || y == v || !same_ray(cx, v, x, y) {
        return None;
    }
    let t = cx.t;
    Some(if t.dist(v, x) < t.dist(v, y) {
        (x, y)
    } else {
        (y, x)
    })
}

pub fn on_segment_ok(cx: &Ctx, a: &[PointId]) -> bool {
    let [v, inner, outer] = a else { return false };
    inner_outer(cx, *v, *inner, *outer) == Some((*inner, *outer))
}

pub fn on_segment(cx: &Ctx, v: PointId, x: PointId, y: PointId) -> Option<Reason> {
    let (i, o) = inner_outer(cx, v, x, y)?;
    Some(label(AtomKey::OnSegment, vec![v, i, o]))
}

fn rename(
    cx: &Ctx,
    a: (PointId, PointId, PointId),
    b: (PointId, PointId, PointId),
) -> Option<Reason> {
    let ((p, v, q), (r, w, u)) = (a, b);
    if v != w {
        return None;
    }
    for ((x1, y1), (x2, y2)) in [((p, r), (q, u)), ((p, u), (q, r))] {
        if x1 == y1 && x2 != y2 {
            return on_segment(cx, v, x2, y2);
        }
        if x2 == y2 && x1 != y1 {
            return on_segment(cx, v, x1, y1);
        }
    }
    None
}

fn line_reason(r: &Reason) -> bool {
    is_coll_reason(r) && !matches!(r, Reason::Atom { .. })
}

pub fn segment_labels(cx: &Ctx, s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain {
        terms,
        links,
        then,
        directed: false,
    } = s
    else {
        return None;
    };
    let mut out = links.clone();
    let mut changed = false;
    for (k, l) in out.iter_mut().enumerate() {
        if l.reasons.is_empty() || !l.reasons.iter().all(line_reason) {
            continue;
        }
        let Some((a, b)) = changed_angle(&terms[k], &terms[k + 1]) else {
            continue;
        };
        if let Some(lab) = rename(cx, a, b) {
            if !l.reasons.contains(&lab) {
                l.reasons.push(lab);
                changed = true;
            }
        }
    }
    changed.then(|| Sentence::Chain {
        terms: terms.clone(),
        links: out,
        then: then.clone(),
        directed: false,
    })
}

fn unit(cx: &Ctx, from: PointId, to: PointId) -> (f64, f64) {
    let (a, b) = (cx.t.coord(from), cx.t.coord(to));
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let n = (dx * dx + dy * dy).sqrt().max(1e-300);
    (dx / n, dy / n)
}

pub fn parallel_kind(cx: &Ctx, a: &[PointId]) -> Option<AtomKey> {
    let [p, v, q, r, w, u] = a else { return None };
    if v == w {
        return None;
    }
    let on_vw = |x: PointId| x != *v && x != *w && near_line(cx, *v, *w, x) || x == *v || x == *w;
    for (t1, s1) in [(*p, *q), (*q, *p)] {
        for (t2, s2) in [(*r, *u), (*u, *r)] {
            if !on_vw(t1) || !on_vw(t2) || t1 == *v || t2 == *w || on_vw(s1) || on_vw(s2) {
                continue;
            }
            let (d1, d2) = (unit(cx, *v, s1), unit(cx, *w, s2));
            if (d1.0 * d2.1 - d1.1 * d2.0).abs() > 1e-7 {
                continue;
            }
            let (e1, e2) = (unit(cx, *v, t1), unit(cx, *w, t2));
            let tr = e1.0 * e2.0 + e1.1 * e2.1;
            let pa = d1.0 * d2.0 + d1.1 * d2.1;
            return match (tr > 0.0, pa > 0.0) {
                (true, true) => Some(AtomKey::CorrespondingAngles),
                (false, false) => Some(AtomKey::AlternateAngles),
                _ => None,
            };
        }
    }
    None
}

fn para_reason(r: &Reason) -> bool {
    matches!(
        r,
        Reason::Hyp {
            stmt: Stmt::Para { .. },
            ..
        } | Reason::Fact {
            stmt: Stmt::Para { .. },
            ..
        } | Reason::Lemma {
            stmt: Stmt::Para { .. },
            ..
        } | Reason::Atom {
            key: AtomKey::Parallel | AtomKey::Reim,
            ..
        }
    )
}

fn angle_label(
    cx: &Ctx,
    a: (PointId, PointId, PointId),
    b: (PointId, PointId, PointId),
) -> Option<Reason> {
    let args = vec![a.0, a.1, a.2, b.0, b.1, b.2];
    let key = parallel_kind(cx, &args)?;
    Some(label(key, args))
}

pub fn parallel_labels(cx: &Ctx, s: &Sentence) -> Option<Sentence> {
    match s {
        Sentence::Chain {
            terms,
            links,
            then,
            directed: false,
        } => {
            let mut out = links.clone();
            let mut changed = false;
            for (k, l) in out.iter_mut().enumerate() {
                let used: Vec<&Reason> = l
                    .combination
                    .iter()
                    .filter_map(|t| l.reasons.get(t.reason as usize))
                    .filter(|r| !is_coll_reason(r))
                    .collect();
                if used.len() != 1
                    || !para_reason(used[0])
                    || l.reasons
                        .iter()
                        .any(|r| matches!(r, Reason::Atom { key, .. } if key.is_config_label()))
                {
                    continue;
                }
                let Some((a, b)) = changed_angle(&terms[k], &terms[k + 1]) else {
                    continue;
                };
                if let Some(lab) = angle_label(cx, a, b) {
                    l.reasons.push(lab);
                    changed = true;
                }
            }
            changed.then(|| Sentence::Chain {
                terms: terms.clone(),
                links: out,
                then: then.clone(),
                directed: false,
            })
        }
        Sentence::Because {
            stmt: st @ Stmt::EqAngle { lhs, rhs },
            reasons,
            combination,
        } => {
            let used: Vec<&Reason> = combination
                .iter()
                .filter_map(|t| reasons.get(t.reason as usize))
                .filter(|r| !is_coll_reason(r))
                .collect();
            if used.len() != 1
                || !para_reason(used[0])
                || reasons
                    .iter()
                    .any(|r| matches!(r, Reason::Atom { key, .. } if key.is_config_label()))
            {
                return None;
            }
            let lab = angle_label(cx, plain(lhs)?, plain(rhs)?)?;
            let mut rs = reasons.clone();
            rs.push(lab);
            Some(Sentence::Because {
                stmt: st.clone(),
                reasons: rs,
                combination: combination.clone(),
            })
        }
        _ => None,
    }
}

fn line_fact(r: &Reason) -> bool {
    matches!(
        r,
        Reason::Hyp {
            stmt: Stmt::Para { .. } | Stmt::Perp { .. },
            ..
        } | Reason::Fact {
            stmt: Stmt::Para { .. } | Stmt::Perp { .. },
            ..
        } | Reason::Lemma {
            stmt: Stmt::Para { .. } | Stmt::Perp { .. },
            ..
        } | Reason::Atom {
            key: AtomKey::Parallel,
            ..
        }
    )
}

pub fn line_chain(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain {
        terms,
        links,
        then: then @ Some(Stmt::Coll { pts }),
        directed,
    } = s
    else {
        return None;
    };
    let zero = matches!(terms.last(), Some(Expr::Const { degrees }) if degrees.is_zero());
    let straight = !*directed
        && matches!(terms.last(), Some(Expr::Const { degrees }) if *degrees == Rat::from_int(180));
    if links.is_empty() || links.len() > 3 || !(zero || straight) || (zero && links.len() < 2) {
        return None;
    }
    if !links.iter().all(|l| {
        !l.reasons.is_empty() && l.reasons.iter().all(|r| line_fact(r) || is_coll_reason(r))
    }) {
        return None;
    }
    let mut link = links[0].clone();
    for l in &links[1..] {
        link = super::barem::merge_links(&link, l);
    }
    if zero && *directed {
        return Some(Sentence::Chain {
            terms: vec![terms[0].clone(), terms[terms.len() - 1].clone()],
            links: vec![link],
            then: then.clone(),
            directed: *directed,
        });
    }
    let t = cx.t;
    let (_, a) = eval(t, &terms[0])?;
    let (_, b) = eval(t, terms.last()?)?;
    let diff = &a - &b;
    let sum = super::barem::link_sum(cx, blocks, &link, false)?;
    for &v in pts {
        let rest: Vec<PointId> = pts.iter().copied().filter(|&p| p != v).collect();
        let [x, y, ..] = rest[..] else { continue };
        let e = Expr::Angle {
            a: x,
            b: v,
            c: y,
            directed: true,
        };
        let Some((_, d)) = eval(t, &e) else { continue };
        for (sign, l) in [
            (1, link.clone()),
            (-1, super::barem::negate_link(link.clone())),
        ] {
            let lsum = if sign == 1 {
                sum.clone()
            } else {
                sum.negated()
            };
            let target = if sign == 1 {
                diff.clone()
            } else {
                diff.negated()
            };
            if super::check::residual_ok(cx, Table::Angle, &(&target - &d), &LinComb::zero(), true)
                && super::check::residual_ok(cx, Table::Angle, &d, &lsum, true)
            {
                return Some(Sentence::Chain {
                    terms: vec![
                        e,
                        Expr::Const {
                            degrees: Rat::zero(),
                        },
                    ],
                    links: vec![l],
                    then: then.clone(),
                    directed: true,
                });
            }
        }
    }
    None
}

fn strip_lemma(l: &Link, st: &Stmt, own: u16) -> Option<(Link, u16)> {
    let used: Vec<u16> = l.combination.iter().map(|t| t.reason).collect();
    let i = l.reasons.iter().enumerate().position(|(i, r)| {
        !used.contains(&(i as u16))
            && matches!(r, Reason::Lemma { stmt, block, .. } if stmt == st && *block != own)
    })?;
    let Reason::Lemma { block, .. } = &l.reasons[i] else {
        return None;
    };
    let block = *block;
    let mut reasons = l.reasons.clone();
    reasons.remove(i);
    let combination = l
        .combination
        .iter()
        .map(|t| Term {
            reason: if t.reason as usize > i {
                t.reason - 1
            } else {
                t.reason
            },
            row: t.row,
            coef: t.coef.clone(),
        })
        .collect();
    Some((
        Link {
            reasons,
            combination,
        },
        block,
    ))
}

fn fold_links(cx: &Ctx, own: u16, links: &[Link]) -> Option<Vec<Link>> {
    let mut out = links.to_vec();
    let mut changed = false;
    for li in 0..out.len() {
        for ai in 0..out[li].reasons.len() {
            let Reason::Atom { key, args, .. } = &out[li].reasons[ai] else {
                continue;
            };
            let Some((st, _, _)) = super::claims::hyp_lemma(cx, *key, args) else {
                continue;
            };
            let Some((lj, (nl, block))) =
                (0..out.len()).find_map(|lj| strip_lemma(&out[lj], &st, own).map(|x| (lj, x)))
            else {
                continue;
            };
            out[lj] = nl;
            let used: Vec<u16> = out[li].combination.iter().map(|t| t.reason).collect();
            let keep: Vec<usize> = (0..out[li].reasons.len())
                .filter(|i| *i == ai || used.contains(&(*i as u16)))
                .collect();
            let mut reasons: Vec<Reason> =
                keep.iter().map(|&i| out[li].reasons[i].clone()).collect();
            let at = keep.iter().position(|&i| i == ai)?;
            if let Reason::Atom { from, .. } = &mut reasons[at] {
                *from = vec![block];
            }
            let combination: Vec<Term> = out[li]
                .combination
                .iter()
                .map(|t| Term {
                    reason: keep.iter().position(|&i| i as u16 == t.reason).unwrap_or(0) as u16,
                    row: t.row,
                    coef: t.coef.clone(),
                })
                .collect();
            out[li] = Link {
                reasons,
                combination,
            };
            changed = true;
            break;
        }
    }
    changed.then_some(out)
}

pub fn fold_hyp_lemmas(cx: &Ctx, own: u16, s: &Sentence) -> Option<Sentence> {
    match s {
        Sentence::Chain {
            terms,
            links,
            then,
            directed,
        } => Some(Sentence::Chain {
            terms: terms.clone(),
            links: fold_links(cx, own, links)?,
            then: then.clone(),
            directed: *directed,
        }),
        Sentence::Computation { comp, terms, links } => Some(Sentence::Computation {
            comp: *comp,
            terms: terms.clone(),
            links: fold_links(cx, own, links)?,
        }),
        _ => None,
    }
}
