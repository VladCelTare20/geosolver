use super::chain::{angle_expr, decomps_upto, display_node, expr_badness, ratio_expr, term_comb, with_const};
use super::check::{self, Checker};
use super::ctx::Ctx;
use super::expr::{eval, is_directed};
use super::model::*;
use super::trace::Table;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::rational::Rat;
use std::collections::{BTreeMap, BTreeSet};

const MAX_GROUPS: usize = 6;
const MAX_GREEDY: usize = 14;
const PLAN_MIN_STEPS: usize = 4;
const PLAN_MAX: usize = 3;

pub fn polish(cx: &Ctx, hp: &mut HumanProof) {
    if !hp.available || hp.blocks.is_empty() {
        return;
    }
    let mut ch = Checker::new(cx);
    if !check::violations_with(&mut ch, hp).is_empty() {
        finish(cx, hp);
        return;
    }
    let original = hp.clone();
    per_sentence(cx, &mut ch, hp, false, &|cx, blocks, _bi, s| undirected(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| split_sentence(cx, blocks, bi, s));
    promote_atoms(cx, &mut ch, hp);
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| split_sentence(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|_, _, _, s| merge_sentence(s));
    cut_long(cx, &mut ch, hp);
    lemma_steps(cx, &mut ch, hp);
    prefix_steps(cx, &mut ch, hp);
    if !check::violations_with(&mut ch, hp).is_empty() {
        if std::env::var_os("HP_DEBUG").is_some() {
            eprintln!("barem polish produced violations; keeping the unpolished proof");
        }
        *hp = original;
    }
    finish(cx, hp);
}

fn debug() -> bool {
    std::env::var_os("HP_BAREM").is_some()
}

type SentenceEdit<'f> = dyn Fn(&Ctx, &[Block], usize, &Sentence) -> Option<Sentence> + 'f;

fn per_sentence(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof, keeps_ends: bool, f: &SentenceEdit) {
    if cx.timed_out() {
        return;
    }
    let cited = lemma_cited(hp);
    let mut edits: Vec<(usize, usize, Sentence)> = Vec::new();
    for (bi, b) in hp.blocks.iter().enumerate() {
        if b.kind == BlockKind::Raw {
            continue;
        }
        for (si, s) in b.body.iter().enumerate() {
            if !keeps_ends && cited.contains(&(b.id, si as u16)) {
                continue;
            }
            if let Some(n) = f(cx, &hp.blocks, bi, s) {
                if n != *s {
                    edits.push((bi, si, n));
                }
            }
        }
    }
    if edits.is_empty() {
        return;
    }
    let mut all = hp.clone();
    for (bi, si, n) in &edits {
        all.blocks[*bi].body[*si] = n.clone();
    }
    if check::violations_with(ch, &all).is_empty() {
        *hp = all;
        return;
    }
    for (bi, si, n) in edits {
        if cx.timed_out() {
            return;
        }
        let mut one = hp.clone();
        one.blocks[bi].body[si] = n;
        let v = check::violations_with(ch, &one);
        if v.is_empty() {
            *hp = one;
        } else if debug() {
            eprintln!("barem edit rejected in block {}: {:?}", hp.blocks[bi].id, v.first().map(|x| &x.detail));
        }
    }
}

fn lemma_cited(hp: &HumanProof) -> BTreeSet<(u16, u16)> {
    let mut out = BTreeSet::new();
    for b in &hp.blocks {
        for s in &b.body {
            visit(s, &mut |r| {
                if let Reason::Lemma { block, sentence, .. } = r {
                    out.insert((*block, *sentence));
                }
            });
        }
    }
    out
}

pub fn visit(s: &Sentence, f: &mut dyn FnMut(&Reason)) {
    fn go(r: &Reason, f: &mut dyn FnMut(&Reason)) {
        f(r);
        if let Reason::Fact { because, .. } = r {
            for x in because {
                go(x, f);
            }
        }
    }
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => {
            for l in links {
                for r in &l.reasons {
                    go(r, f);
                }
            }
        }
        Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => {
            for r in reasons {
                go(r, f);
            }
        }
        Sentence::Raw { .. } => {}
    }
}

fn visit_mut(hp: &mut HumanProof, f: &mut dyn FnMut(&mut Reason)) {
    for b in hp.blocks.iter_mut() {
        for s in b.body.iter_mut() {
            super::claims::sentence_reasons_mut(s, f);
        }
    }
}

fn angle_table(cx: &Ctx, e: &Expr) -> bool {
    eval(cx.t, e).is_some_and(|(tb, _)| tb == Table::Angle)
}

fn undirect_expr(e: &Expr) -> bool {
    match e {
        Expr::Angle { .. } | Expr::Const { .. } => true,
        Expr::Lin { terms } => terms.iter().all(|(_, x)| matches!(x, Expr::Angle { .. } | Expr::Const { .. })),
        _ => false,
    }
}

fn undirected(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    match s {
        Sentence::Chain { terms, links, then, directed: true } => undirected_chain(cx, blocks, terms, links, then.clone()),
        Sentence::Because { stmt, reasons, combination } if !combination.is_empty() => {
            let (lhs, rhs) = match stmt {
                Stmt::EqAngle { lhs, rhs } => (lhs.clone(), rhs.clone()),
                Stmt::AngleConst { angle, degrees } => (angle.clone(), Expr::Const { degrees: degrees.clone() }),
                _ => return None,
            };
            if !is_directed(&lhs) {
                return None;
            }
            let link = oriented_link(cx, blocks, &lhs, &rhs, reasons, combination, true)?;
            undirected_chain(cx, blocks, &[lhs, rhs], &[link], None)
        }
        _ => None,
    }
}

fn oriented_link(cx: &Ctx, blocks: &[Block], lhs: &Expr, rhs: &Expr, reasons: &[Reason], combination: &[Term], directed: bool) -> Option<Link> {
    let link = Link { reasons: reasons.to_vec(), combination: combination.to_vec() };
    let (tb, a) = eval(cx.t, lhs)?;
    let (_, b) = eval(cx.t, rhs)?;
    let sum = link_sum(cx, blocks, &link, !directed)?;
    let diff = &a - &b;
    if check::residual_ok(cx, tb, &diff, &sum, directed) {
        Some(link)
    } else if check::residual_ok(cx, tb, &diff, &sum.negated(), directed) {
        Some(negate_link(link))
    } else {
        None
    }
}

fn negate_link(l: Link) -> Link {
    Link { reasons: l.reasons, combination: l.combination.into_iter().map(|t| Term { coef: -&t.coef, ..t }).collect() }
}

fn term_score(e: &Expr) -> Option<i64> {
    let ninety = Rat::from_int(90);
    let one_eighty = Rat::from_int(180);
    match e {
        Expr::Angle { .. } => Some(0),
        Expr::Const { degrees } => (*degrees == ninety || *degrees == one_eighty).then_some(1),
        Expr::Lin { terms } => {
            if lin_terms(e) > 2 {
                return None;
            }
            let konst: Rat = terms.iter().filter_map(|(k, x)| if let Expr::Const { degrees } = x { Some(degrees * k) } else { None }).fold(Rat::zero(), |a, b| &a + &b);
            let mut s = if konst.is_zero() {
                0
            } else if konst == ninety || konst == one_eighty || konst == -&ninety {
                1
            } else {
                return None;
            };
            let positive_const = !konst.is_negative() && !konst.is_zero();
            for (k, x) in terms {
                match x {
                    Expr::Const { .. } => {}
                    Expr::Angle { .. } => {
                        let a = k.abs();
                        s += if a.is_one() {
                            0
                        } else if a == Rat::from_int(2) || a == Rat::new(1, 2) {
                            1
                        } else {
                            return None;
                        };
                        if k.is_negative() {
                            s += if positive_const { 1 } else { 4 };
                        }
                    }
                    _ => return None,
                }
            }
            Some(s)
        }
        _ => None,
    }
}

fn undirected_chain(cx: &Ctx, blocks: &[Block], terms: &[Expr], links: &[Link], then: Option<Stmt>) -> Option<Sentence> {
    if !angle_table(cx, &terms[0]) {
        return None;
    }
    let t = cx.t;
    let (_, raw0) = eval(t, &terms[0])?;
    let mut sums: Vec<LinComb> = Vec::new();
    for l in links {
        sums.push(link_sum(cx, blocks, l, true)?);
    }
    let mut best: Option<(i64, Vec<Expr>, bool)> = None;
    for neg in [false, true] {
        let mut node = if neg { raw0.negated() } else { raw0.clone() };
        let v0 = t.value(Table::Angle, &node);
        let k = -v0.floor() as i64;
        if (v0 - v0.round()).abs() < 1e-9 {
            return None;
        }
        node.add_term(crate::elimination::ANGLE_UNIT, Rat::from_int(k));
        let mut nodes = vec![node.clone()];
        for sum in &sums {
            node = LinComb::combine(&node, sum, &Rat::from_int(if neg { 1 } else { -1 }));
            nodes.push(node.clone());
        }
        let mut out: Vec<Expr> = Vec::new();
        let mut score = 0i64;
        let mut ok = true;
        for (j, n) in nodes.iter().enumerate() {
            let near_next = links.get(j).map(link_points).unwrap_or_default();
            let near_prev = if j > 0 { link_points(&links[j - 1]) } else { Vec::new() };
            let focus: BTreeSet<PointId> = super::view::expr_points(&terms[j]).into_iter().collect();
            let Some(e) = display_node(cx, &cx.quot.q(n), false, &[&near_next, &near_prev], &focus) else {
                ok = false;
                break;
            };
            match term_score(&e) {
                Some(sc) if sc < 4 => score += sc,
                _ => {
                    ok = false;
                    break;
                }
            }
            out.push(e);
        }
        if ok && best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, out, neg));
        }
    }
    let (_, out, neg) = best?;
    let links: Vec<Link> = if neg { links.iter().cloned().map(negate_link).collect() } else { links.to_vec() };
    Some(Sentence::Chain { terms: out, links, then, directed: false })
}

fn lin_terms(e: &Expr) -> usize {
    match e {
        Expr::Lin { terms } => terms.iter().filter(|(_, x)| !matches!(x, Expr::Const { .. })).count(),
        _ => 1,
    }
}

fn link_points(l: &Link) -> Vec<PointId> {
    let mut v = Vec::new();
    for r in &l.reasons {
        super::view::reason_points_pub(r, &mut v);
    }
    v.sort_unstable();
    v.dedup();
    v
}

fn term_rows(cx: &Ctx, blocks: &[Block], l: &Link, exact: bool) -> Option<Vec<(u16, Table, LinComb)>> {
    let t = cx.t;
    let mut cache: BTreeMap<u16, Vec<(Table, LinComb)>> = BTreeMap::new();
    let mut out = Vec::new();
    for term in &l.combination {
        let r = l.reasons.get(term.reason as usize)?;
        if !cache.contains_key(&term.reason) {
            cache.insert(term.reason, check::reason_rows(t, blocks, r)?);
        }
        let (tb, row) = cache[&term.reason].get(term.row as usize)?.clone();
        let row = if exact { t.exact(tb, &row) } else { row };
        let mut x = LinComb::zero();
        x.iadd_mul(&row, &term.coef);
        out.push((term.reason, tb, x));
    }
    Some(out)
}

fn link_sum(cx: &Ctx, blocks: &[Block], l: &Link, exact: bool) -> Option<LinComb> {
    let mut out = LinComb::zero();
    for (_, _, x) in term_rows(cx, blocks, l, exact)? {
        out.iadd_mul(&x, &Rat::one());
    }
    Some(out)
}

fn is_coll_reason(r: &Reason) -> bool {
    matches!(r, Reason::Hyp { stmt: Stmt::Coll { .. }, .. } | Reason::Fact { stmt: Stmt::Coll { .. }, .. } | Reason::Lemma { stmt: Stmt::Coll { .. }, .. } | Reason::Atom { key: AtomKey::Radii, .. })
}

pub fn link_facts(l: &Link) -> usize {
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    used.iter().filter(|&&i| l.reasons.get(i as usize).is_some_and(|r| !is_coll_reason(r))).count()
}

fn split_sentence(cx: &Ctx, blocks: &[Block], bi: usize, s: &Sentence) -> Option<Sentence> {
    let focus: BTreeSet<PointId> = blocks[bi].points.iter().copied().collect();
    match s {
        Sentence::Chain { terms, links, then, directed } => {
            let (terms, links) = split_links(cx, blocks, terms, links, *directed, &focus)?;
            Some(Sentence::Chain { terms, links, then: then.clone(), directed: *directed })
        }
        Sentence::Computation { comp, terms, links } => {
            let (terms, links) = split_links(cx, blocks, terms, links, true, &focus)?;
            Some(Sentence::Computation { comp: *comp, terms, links })
        }
        Sentence::Because { stmt, reasons, combination } | Sentence::Pooled { stmt, reasons, combination } => {
            if combination.is_empty() {
                return None;
            }
            let link = Link { reasons: reasons.clone(), combination: combination.clone() };
            if link_facts(&link) < 2 {
                return None;
            }
            let (lhs, rhs) = match stmt {
                Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs } => (lhs.clone(), rhs.clone()),
                Stmt::AngleConst { angle, degrees } => (angle.clone(), Expr::Const { degrees: degrees.clone() }),
                Stmt::Cong { s1, s2 } => (Expr::Seg { a: s1.0, b: s1.1 }, Expr::Seg { a: s2.0, b: s2.1 }),
                _ => return None,
            };
            let directed = is_directed(&lhs) && is_directed(&rhs);
            let (tb, _) = eval(cx.t, &lhs)?;
            let link = oriented_link(cx, blocks, &lhs, &rhs, &link.reasons, &link.combination, directed)?;
            let (terms, links) = split_links(cx, blocks, &[lhs, rhs], &[link], directed, &focus)?;
            if links.iter().any(|l| link_facts(l) > 1) {
                return None;
            }
            if tb == Table::Angle {
                Some(Sentence::Chain { terms, links, then: None, directed })
            } else {
                Some(Sentence::Computation { comp: CompKind::Ratio, terms, links })
            }
        }
        _ => None,
    }
}

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn go(cur: &mut Vec<usize>, used: &mut Vec<bool>, out: &mut Vec<Vec<usize>>) {
        if cur.len() == used.len() {
            out.push(cur.clone());
            return;
        }
        for i in 0..used.len() {
            if !used[i] {
                used[i] = true;
                cur.push(i);
                go(cur, used, out);
                cur.pop();
                used[i] = false;
            }
        }
    }
    let mut out = Vec::new();
    go(&mut Vec::new(), &mut vec![false; n], &mut out);
    out
}

fn greedy_orders(cx: &Ctx, tb: Table, start: &LinComb, groups: &[(u16, LinComb)], facts: &[usize], directed: bool, focus: &BTreeSet<PointId>) -> Vec<Vec<usize>> {
    let mut node = start.clone();
    let mut left: Vec<usize> = (0..facts.len()).collect();
    let mut order: Vec<usize> = Vec::new();
    while left.len() > 1 {
        let mut best: Option<(i64, usize)> = None;
        for (li, &k) in left.iter().enumerate() {
            let next = LinComb::combine(&node, &groups[facts[k]].1, &Rat::from_int(-1));
            if let Some(e) = display(cx, tb, &next, directed, &[], focus) {
                let b = badness(&e);
                if best.is_none_or(|x| b < x.0) {
                    best = Some((b, li));
                }
            }
        }
        let Some((_, li)) = best else { return Vec::new() };
        let k = left.remove(li);
        node = LinComb::combine(&node, &groups[facts[k]].1, &Rat::from_int(-1));
        order.push(k);
    }
    order.extend(left);
    vec![order]
}

fn shows(cx: &Ctx, tb: Table, e: &Expr, node: &LinComb, directed: bool) -> bool {
    let Some((_, v)) = eval(cx.t, e) else { return false };
    check::residual_ok(cx, tb, &v, node, directed)
}

fn display(cx: &Ctx, tb: Table, node: &LinComb, directed: bool, near: &[&[PointId]], focus: &BTreeSet<PointId>) -> Option<Expr> {
    let e = if tb == Table::Angle {
        let e = display_multi(cx, &cx.quot.q(node), directed, near, focus)?;
        if !directed && (!undirect_expr(&e) || lin_terms(&e) > 2) {
            return None;
        }
        e
    } else {
        ratio_expr(cx, node)
    };
    shows(cx, tb, &e, node, directed).then_some(e)
}

fn display_multi(cx: &Ctx, node: &LinComb, directed: bool, near: &[&[PointId]], focus: &BTreeSet<PointId>) -> Option<Expr> {
    if let Some(e) = display_node(cx, node, directed, near, focus) {
        return Some(e);
    }
    let mut best: Option<(i64, Expr)> = None;
    for ts in decomps_upto(node, 3) {
        if ts.len() != 3 {
            continue;
        }
        let mut terms: Vec<(Rat, Expr)> = Vec::new();
        let mut ok = true;
        for t in &ts {
            match angle_expr(cx, &term_comb(t), directed, near, focus) {
                Some(Expr::Lin { terms: tt }) => terms.extend(tt.into_iter().filter(|(_, x)| !matches!(x, Expr::Const { .. }))),
                Some(e) => terms.push((Rat::one(), e)),
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let Some(e) = with_const(cx, terms, node, directed) else { continue };
        let bad = expr_badness(&e);
        if best.as_ref().is_none_or(|b| bad < b.0) {
            best = Some((bad, e));
        }
    }
    best.map(|b| b.1)
}

fn badness(e: &Expr) -> i64 {
    expr_badness(e) + 3 * lin_terms(e) as i64
        + match e {
            Expr::Prod { factors } => factors.len() as i64,
            _ => 0,
        }
}

#[allow(clippy::type_complexity)]
fn split_links(cx: &Ctx, blocks: &[Block], terms: &[Expr], links: &[Link], directed: bool, focus: &BTreeSet<PointId>) -> Option<(Vec<Expr>, Vec<Link>)> {
    if !links.iter().any(|l| link_facts(l) > 1) {
        return None;
    }
    let t = cx.t;
    let mut out_terms = vec![terms[0].clone()];
    let mut out_links: Vec<Link> = Vec::new();
    let mut changed = false;
    for (k, l) in links.iter().enumerate() {
        let done = |out_terms: &mut Vec<Expr>, out_links: &mut Vec<Link>| {
            out_links.push(l.clone());
            out_terms.push(terms[k + 1].clone());
        };
        if link_facts(l) < 2 || cx.timed_out() {
            done(&mut out_terms, &mut out_links);
            continue;
        }
        let exact = !directed;
        let Some(rows) = term_rows(cx, blocks, l, exact) else {
            if debug() {
                eprintln!("barem split: link rows do not resolve");
            }
            done(&mut out_terms, &mut out_links);
            continue;
        };
        let Some((tb, start)) = eval(t, &terms[k]) else {
            if debug() {
                eprintln!("barem split: term {:?} does not evaluate", terms[k]);
            }
            done(&mut out_terms, &mut out_links);
            continue;
        };
        let mut groups: Vec<(u16, LinComb)> = Vec::new();
        for (r, _, x) in &rows {
            match groups.iter_mut().find(|g| g.0 == *r) {
                Some(g) => g.1.iadd_mul(x, &Rat::one()),
                None => groups.push((*r, x.clone())),
            }
        }
        let facts: Vec<usize> = (0..groups.len()).filter(|&i| !is_coll_reason(&l.reasons[groups[i].0 as usize])).collect();
        let colls: Vec<usize> = (0..groups.len()).filter(|&i| is_coll_reason(&l.reasons[groups[i].0 as usize])).collect();
        if facts.len() < 2 || facts.len() > MAX_GREEDY {
            done(&mut out_terms, &mut out_links);
            continue;
        }
        let next_pts = links.get(k + 1).map(link_points).unwrap_or_default();
        let prev_pts = if k > 0 { link_points(&links[k - 1]) } else { Vec::new() };
        let mut best: Option<(i64, Vec<Vec<usize>>, Vec<Expr>)> = None;
        let orders: Vec<Vec<usize>> = if facts.len() <= MAX_GROUPS { permutations(facts.len()) } else { greedy_orders(cx, tb, &start, &groups, &facts, directed, focus) };
        for perm in orders {
            for coll_first in [true, false] {
                let mut parts: Vec<Vec<usize>> = perm.iter().map(|&p| vec![facts[p]]).collect();
                if !colls.is_empty() {
                    let at = if coll_first { 0 } else { parts.len() - 1 };
                    parts[at].extend(colls.iter().copied());
                } else if !coll_first {
                    continue;
                }
                let mut node = start.clone();
                let mut mids: Vec<Expr> = Vec::new();
                let mut score = 0i64;
                let mut ok = true;
                for (pi, part) in parts.iter().enumerate().take(parts.len() - 1) {
                    for &g in part {
                        node = LinComb::combine(&node, &groups[g].1, &Rat::from_int(-1));
                    }
                    let mut near: Vec<PointId> = Vec::new();
                    for &g in part {
                        super::view::reason_points_pub(&l.reasons[groups[g].0 as usize], &mut near);
                    }
                    let mut after: Vec<PointId> = Vec::new();
                    for &g in &parts[pi + 1] {
                        super::view::reason_points_pub(&l.reasons[groups[g].0 as usize], &mut after);
                    }
                    let _ = (&next_pts, &prev_pts);
                    match display(cx, tb, &node, directed, &[&after, &near], focus) {
                        Some(e) => {
                            score += badness(&e);
                            let prev = mids.last().unwrap_or(&terms[k]);
                            if *prev == e || (pi + 2 == parts.len() && e == terms[k + 1]) {
                                score += 1000;
                            }
                            mids.push(e);
                        }
                        None => {
                            ok = false;
                            break;
                        }
                    }
                }
                if ok && best.as_ref().is_none_or(|b| score < b.0) {
                    best = Some((score, parts, mids));
                }
            }
            if cx.timed_out() {
                break;
            }
        }
        let Some((_, parts, mids)) = best else {
            if debug() {
                eprintln!("barem split: no displayable order for a {}-fact link in {:?} table", facts.len(), tb);
            }
            done(&mut out_terms, &mut out_links);
            continue;
        };
        let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
        let extra: Vec<usize> = (0..l.reasons.len()).filter(|i| !used.contains(&(*i as u16))).collect();
        let theorem_part = parts.iter().position(|p| p.iter().any(|&g| matches!(&l.reasons[groups[g].0 as usize], Reason::Atom { key, .. } if super::theorems::is_theorem(*key)))).unwrap_or(0);
        for (pi, part) in parts.iter().enumerate() {
            let mut idx: Vec<usize> = part.iter().map(|&g| groups[g].0 as usize).collect();
            if pi == theorem_part {
                idx.extend(extra.iter().copied());
            }
            let mut reasons: Vec<Reason> = Vec::new();
            let mut map: BTreeMap<usize, u16> = BTreeMap::new();
            for i in idx {
                map.insert(i, reasons.len() as u16);
                reasons.push(l.reasons[i].clone());
            }
            let combination: Vec<Term> = l.combination.iter().filter(|t| map.contains_key(&(t.reason as usize)) && part.iter().any(|&g| groups[g].0 == t.reason)).map(|t| Term { reason: map[&(t.reason as usize)], row: t.row, coef: t.coef.clone() }).collect();
            out_links.push(Link { reasons, combination });
            if pi + 1 < parts.len() {
                out_terms.push(mids[pi].clone());
            }
        }
        out_terms.push(terms[k + 1].clone());
        changed = true;
    }
    let (out_terms, out_links) = merge_repeats(out_terms, out_links);
    changed.then_some((out_terms, out_links))
}

fn merge_links(a: &Link, b: &Link) -> Link {
    let mut reasons = a.reasons.clone();
    let mut combination = a.combination.clone();
    for t in &b.combination {
        let r = &b.reasons[t.reason as usize];
        let pos = match reasons.iter().position(|x| x == r) {
            Some(p) => p,
            None => {
                reasons.push(r.clone());
                reasons.len() - 1
            }
        };
        combination.push(Term { reason: pos as u16, row: t.row, coef: t.coef.clone() });
    }
    for r in &b.reasons {
        if !reasons.contains(r) {
            reasons.push(r.clone());
        }
    }
    Link { reasons, combination }
}

fn merge_repeats(mut terms: Vec<Expr>, mut links: Vec<Link>) -> (Vec<Expr>, Vec<Link>) {
    while links.len() > 1 {
        let Some(k) = (0..links.len()).find(|&k| terms[k] == terms[k + 1]) else { break };
        if k + 1 < links.len() {
            let m = merge_links(&links[k], &links[k + 1]);
            links.splice(k..k + 2, [m]);
            terms.remove(k + 1);
        } else {
            let m = merge_links(&links[k - 1], &links[k]);
            links.splice(k - 1..k + 1, [m]);
            terms.remove(k);
        }
    }
    (terms, links)
}

fn merge_sentence(s: &Sentence) -> Option<Sentence> {
    match s {
        Sentence::Chain { terms, links, then, directed } if links.len() > 1 && terms.windows(2).any(|w| w[0] == w[1]) => {
            let (terms, links) = merge_repeats(terms.clone(), links.clone());
            Some(Sentence::Chain { terms, links, then: then.clone(), directed: *directed })
        }
        Sentence::Computation { comp, terms, links } if links.len() > 1 && terms.windows(2).any(|w| w[0] == w[1]) => {
            let (terms, links) = merge_repeats(terms.clone(), links.clone());
            Some(Sentence::Computation { comp: *comp, terms, links })
        }
        _ => None,
    }
}

const PROMOTE: [AtomKey; 10] = [
    AtomKey::Thales,
    AtomKey::PerpBisector,
    AtomKey::Midline,
    AtomKey::Orthocentre,
    AtomKey::Centroid,
    AtomKey::PowerOfPoint,
    AtomKey::MedianHypotenuse,
    AtomKey::EqualTangents,
    AtomKey::Parallel,
    AtomKey::PerpBisectorLocus,
];

const ALWAYS: [AtomKey; 7] = [AtomKey::PerpBisector, AtomKey::Midline, AtomKey::Orthocentre, AtomKey::Centroid, AtomKey::MedianHypotenuse, AtomKey::EqualTangents, AtomKey::PerpBisectorLocus];

fn atom_key_args(r: &Reason) -> Option<(AtomKey, Vec<PointId>)> {
    match r {
        Reason::Atom { key, args, .. } if PROMOTE.contains(key) => Some((*key, args.clone())),
        _ => None,
    }
}

fn promote_atoms(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut tried: BTreeSet<(AtomKey, Vec<PointId>)> = BTreeSet::new();
    loop {
        if cx.timed_out() {
            return;
        }
        let mut uses: BTreeMap<(AtomKey, Vec<PointId>), (usize, usize, bool)> = BTreeMap::new();
        for (bi, b) in hp.blocks.iter().enumerate() {
            if b.kind == BlockKind::Raw {
                continue;
            }
            for s in &b.body {
                let mut seen: BTreeSet<(AtomKey, Vec<PointId>)> = BTreeSet::new();
                let crowded = match s {
                    Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().any(|l| link_facts(l) > 1),
                    Sentence::Because { reasons, .. } | Sentence::Theorem { reasons, .. } | Sentence::Pooled { reasons, .. } => reasons.len() > 1,
                    _ => false,
                };
                visit(s, &mut |r| {
                    if let Some(k) = atom_key_args(r) {
                        seen.insert(k);
                    }
                });
                for k in seen {
                    let e = uses.entry(k).or_insert((bi, 0, false));
                    e.1 += 1;
                    e.2 |= crowded;
                }
            }
        }
        let pick = uses.iter().filter(|(k, (_, n, crowded))| !tried.contains(*k) && (*n >= 2 || ALWAYS.contains(&k.0) || (*crowded && k.0 != AtomKey::Parallel))).min_by_key(|(_, (bi, _, _))| *bi).map(|(k, v)| (k.clone(), *v));
        let Some(((key, args), (first, _, _))) = pick else { return };
        tried.insert((key, args.clone()));
        let Some(cand) = promote_one(cx, hp, key, &args, first) else { continue };
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
        } else if debug() {
            eprintln!("barem promotion of {key:?} {args:?} rejected: {:?}", v.first().map(|x| &x.detail));
        }
    }
}

fn exact_rows(cx: &Ctx, rows: Vec<(Table, LinComb)>) -> Vec<(Table, LinComb)> {
    rows.into_iter().map(|(tb, r)| (tb, cx.t.exact(tb, &r))).collect()
}

fn promote_one(cx: &Ctx, hp: &HumanProof, key: AtomKey, args: &[PointId], first: usize) -> Option<HumanProof> {
    let t = cx.t;
    let stmt = super::claims::atom_stmt(key, args);
    let srows = exact_rows(cx, check::stmt_targets(t, &stmt)?);
    if srows.len() != 1 {
        return None;
    }
    let (stb, srow) = srows[0].clone();
    let arows = exact_rows(cx, check::check_rows(t, key, args)?);
    let ai = arows.iter().position(|(tb, r)| *tb == stb && (*r == srow || r.negated() == srow))?;
    let sign = if arows[ai].1 == srow { Rat::one() } else { Rat::from_int(-1) };
    let user = &hp.blocks[first];
    let deps_at = |id: u16| hp.blocks.iter().position(|b| b.id == id);
    let atom_reason = {
        let mut found: Option<Reason> = None;
        for s in &user.body {
            visit(s, &mut |r| {
                if found.is_none() && atom_key_args(r).is_some_and(|(k, a)| k == key && a == args) {
                    found = Some(r.clone());
                }
            });
        }
        found?
    };
    let body = vec![Sentence::Because { stmt: stmt.clone(), reasons: vec![atom_reason.clone()], combination: vec![Term { reason: 0, row: ai as u16, coef: sign.clone() }] }];
    let from: Vec<u16> = match &atom_reason {
        Reason::Atom { from, .. } => from.clone(),
        _ => Vec::new(),
    };
    let at = from.iter().filter_map(|&f| deps_at(f)).map(|i| i + 1).max().unwrap_or(0).min(first);
    let at = (at..=first).find(|&i| hp.blocks[i].kind != BlockKind::Raw || i == first).unwrap_or(first);
    let first = at;
    let new_id = hp.blocks[first].id;
    let block = Block {
        id: new_id,
        kind: BlockKind::Step,
        stmt: stmt.clone(),
        body,
        engine_facts: user.engine_facts.clone(),
        points: args.to_vec(),
        objects: Vec::new(),
        step: 0,
        tag: false,
        horizon: user.horizon,
    };
    let mut out = hp.clone();
    remap_ids(&mut out, &|id| if id >= new_id { id + 1 } else { id });
    out.blocks.insert(first, block);
    let lemma = Reason::Lemma { stmt: stmt.clone(), block: new_id, sentence: 0 };
    for b in out.blocks.iter_mut().skip(first + 1) {
        for s in b.body.iter_mut() {
            replace_atom(cx, s, key, args, &lemma, ai, &sign);
        }
    }
    Some(out)
}

fn replace_atom(cx: &Ctx, s: &mut Sentence, key: AtomKey, args: &[PointId], lemma: &Reason, ai: usize, sign: &Rat) {
    let _ = cx;
    let fix = |reasons: &mut Vec<Reason>, comb: &mut Vec<Term>| {
        for (i, r) in reasons.iter_mut().enumerate() {
            if atom_key_args(r).is_some_and(|(k, a)| k == key && a == args) {
                *r = lemma.clone();
                for tm in comb.iter_mut() {
                    if tm.reason as usize == i {
                        if tm.row as usize == ai {
                            tm.row = 0;
                            tm.coef = &tm.coef * sign;
                        } else {
                            tm.row = u16::MAX;
                        }
                    }
                }
            }
        }
        let mut seen: Vec<Reason> = Vec::new();
        let mut map: Vec<u16> = Vec::new();
        for r in reasons.iter() {
            match seen.iter().position(|x| x == r) {
                Some(p) => map.push(p as u16),
                None => {
                    map.push(seen.len() as u16);
                    seen.push(r.clone());
                }
            }
        }
        *reasons = seen;
        for tm in comb.iter_mut() {
            tm.reason = map[tm.reason as usize];
        }
    };
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => {
            for l in links.iter_mut() {
                fix(&mut l.reasons, &mut l.combination);
            }
        }
        Sentence::Because { reasons, combination, .. } | Sentence::Pooled { reasons, combination, .. } => fix(reasons, combination),
        Sentence::Theorem { reasons, .. } => {
            let mut none = Vec::new();
            fix(reasons, &mut none);
        }
        Sentence::Raw { .. } => {}
    }
    super::claims::sentence_reasons_mut(s, &mut |r| {
        if let Reason::Fact { because, .. } = r {
            for b in because.iter_mut() {
                if atom_key_args(b).is_some_and(|(k, a)| k == key && a == args) {
                    *b = lemma.clone();
                }
            }
        }
    });
}

fn remap_ids(hp: &mut HumanProof, f: &dyn Fn(u16) -> u16) {
    for b in hp.blocks.iter_mut() {
        b.id = f(b.id);
    }
    visit_mut(hp, &mut |r| match r {
        Reason::Claim { block, .. } | Reason::Lemma { block, .. } => *block = f(*block),
        Reason::Fact { block: Some(b), .. } => *b = f(*b),
        Reason::Atom { from, .. } => {
            for x in from.iter_mut() {
                *x = f(*x);
            }
        }
        _ => {}
    });
    for p in hp.plan.iter_mut() {
        *p = f(*p);
    }
}

const MAX_LINKS: usize = 8;

fn sentence_parts(s: &Sentence) -> Option<(&[Expr], &[Link])> {
    match s {
        Sentence::Chain { terms, links, .. } | Sentence::Computation { terms, links, .. } => Some((terms, links)),
        _ => None,
    }
}

fn with_parts(s: &Sentence, terms: Vec<Expr>, links: Vec<Link>, keep_then: bool) -> Sentence {
    match s {
        Sentence::Chain { then, directed, .. } => Sentence::Chain { terms, links, then: if keep_then { then.clone() } else { None }, directed: *directed },
        Sentence::Computation { comp, .. } => Sentence::Computation { comp: *comp, terms, links },
        _ => s.clone(),
    }
}

fn cut_at(cx: &Ctx, hp: &HumanProof, bi: usize, si: usize, k: usize, kind: usize) -> Option<HumanProof> {
    let s = &hp.blocks[bi].body[si];
    let (terms, links) = sentence_parts(s)?;
    let (lhs, rhs) = (terms[0].clone(), terms[k].clone());
    let (tb, _) = eval(cx.t, &lhs)?;
    let stmt = if tb == Table::Angle && kind % 2 == 0 { Stmt::EqAngle { lhs: lhs.clone(), rhs: rhs.clone() } } else { Stmt::Eq { lhs: lhs.clone(), rhs: rhs.clone() } };
    let coef = if kind < 2 { Rat::one() } else { Rat::from_int(-1) };
    let id = hp.blocks[bi].id;
    let mut out = hp.clone();
    visit_mut(&mut out, &mut |r| {
        if let Reason::Lemma { block, sentence, .. } = r {
            if *block == id && *sentence as usize >= si {
                *sentence += 1;
            }
        }
    });
    let first = with_parts(s, terms[..=k].to_vec(), links[..k].to_vec(), false);
    let mut rest_terms = vec![lhs, rhs];
    rest_terms.extend(terms[k + 1..].iter().cloned());
    let mut rest_links = vec![Link { reasons: vec![Reason::Lemma { stmt, block: id, sentence: si as u16 }], combination: vec![Term { reason: 0, row: 0, coef }] }];
    rest_links.extend(links[k..].iter().cloned());
    let rest = with_parts(s, rest_terms, rest_links, true);
    let body = &mut out.blocks[bi].body;
    body[si] = rest;
    body.insert(si, first);
    Some(out)
}

fn cut_long(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut guard = 0;
    'again: while guard < 20 {
        guard += 1;
        if cx.timed_out() {
            return;
        }
        for bi in 0..hp.blocks.len() {
            if hp.blocks[bi].kind == BlockKind::Raw {
                continue;
            }
            for si in 0..hp.blocks[bi].body.len() {
                let Some((terms, links)) = sentence_parts(&hp.blocks[bi].body[si]) else { continue };
                let n = links.len();
                if n <= MAX_LINKS {
                    continue;
                }
                let mut ks: Vec<usize> = (2..n - 1).collect();
                ks.sort_by_key(|&k| (k.max(n - k + 1), badness(&terms[k]), k));
                for k in ks.into_iter().take(4) {
                    for kind in 0..4 {
                        let Some(cand) = cut_at(cx, hp, bi, si, k, kind) else { continue };
                        if check::violations_with(ch, &cand).is_empty() {
                            *hp = cand;
                            continue 'again;
                        } else if debug() {
                            eprintln!("barem cut rejected at link {k} kind {kind}");
                        }
                    }
                }
            }
        }
        return;
    }
}

fn lemma_steps(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut tried: BTreeSet<(u16, u16)> = BTreeSet::new();
    loop {
        if cx.timed_out() {
            return;
        }
        let cited = lemma_cited(hp);
        let mut pick: Option<(usize, usize, Stmt)> = None;
        'outer: for (bi, b) in hp.blocks.iter().enumerate() {
            if b.kind == BlockKind::Raw || b.body.len() < 2 {
                continue;
            }
            for (si, s) in b.body.iter().enumerate() {
                if !cited.contains(&(b.id, si as u16)) || tried.contains(&(b.id, si as u16)) {
                    continue;
                }
                let self_cites = {
                    let mut x = false;
                    visit(s, &mut |r| {
                        if let Reason::Lemma { block, .. } = r {
                            x |= *block == b.id;
                        }
                    });
                    x
                };
                let standalone = matches!(s, Sentence::Chain { then: None, .. } | Sentence::Computation { .. });
                if self_cites || !standalone {
                    continue;
                }
                let mut st: Option<Stmt> = None;
                for s2 in &b.body[si + 1..] {
                    visit(s2, &mut |r| {
                        if let Reason::Lemma { stmt, block, sentence } = r {
                            if *block == b.id && *sentence as usize == si && st.is_none() {
                                st = Some(stmt.clone());
                            }
                        }
                    });
                }
                for b2 in &hp.blocks[bi + 1..] {
                    for s2 in &b2.body {
                        visit(s2, &mut |r| {
                            if let Reason::Lemma { stmt, block, sentence } = r {
                                if *block == b.id && *sentence as usize == si && st.is_none() {
                                    st = Some(stmt.clone());
                                }
                            }
                        });
                    }
                }
                if let Some(st) = st {
                    pick = Some((bi, si, st));
                    break 'outer;
                }
            }
        }
        let Some((bi, si, st)) = pick else { return };
        tried.insert((hp.blocks[bi].id, si as u16));
        let cand = lemma_out(hp, bi, si, st);
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
            tried.clear();
        } else if debug() {
            eprintln!("barem lemma step rejected: {:?}", v.first().map(|x| &x.detail));
        }
    }
}

fn lemma_out(hp: &HumanProof, bi: usize, si: usize, st: Stmt) -> HumanProof {
    let old = hp.blocks[bi].id;
    let mut out = hp.clone();
    remap_ids(&mut out, &|id| if id >= old { id + 1 } else { id });
    let parent = old + 1;
    let s = out.blocks[bi].body.remove(si);
    visit_mut(&mut out, &mut |r| {
        if let Reason::Lemma { block, sentence, .. } = r {
            if *block == parent {
                if *sentence as usize == si {
                    *block = old;
                    *sentence = 0;
                } else if *sentence as usize > si {
                    *sentence -= 1;
                }
            }
        }
    });
    let p = &out.blocks[bi];
    let mut points = super::view::sentence_points(&s);
    points.sort_unstable();
    points.dedup();
    let block = Block {
        id: old,
        kind: BlockKind::Step,
        stmt: st,
        body: vec![s],
        engine_facts: p.engine_facts.clone(),
        points,
        objects: Vec::new(),
        step: 0,
        tag: false,
        horizon: p.horizon,
    };
    out.blocks.insert(bi, block);
    out
}

fn result_stmt(s: &Sentence) -> Option<Stmt> {
    match s {
        Sentence::Because { stmt, .. } | Sentence::Pooled { stmt, .. } => Some(stmt.clone()),
        Sentence::Theorem { stmt, .. } if !matches!(stmt, Stmt::Formula { .. }) => Some(stmt.clone()),
        Sentence::Chain { then: Some(st), .. } => Some(st.clone()),
        _ => None,
    }
}

fn cites_fact(ss: &[Sentence], st: &Stmt) -> bool {
    let mut hit = false;
    for s in ss {
        visit(s, &mut |r| {
            if let Reason::Fact { stmt, block: None, .. } = r {
                hit |= stmt == st;
            }
        });
    }
    hit
}

fn prefix_steps(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut tried: BTreeSet<(u16, u16)> = BTreeSet::new();
    loop {
        if cx.timed_out() {
            return;
        }
        let cited = lemma_cited(hp);
        let mut pick: Option<(usize, usize, Stmt)> = None;
        'outer: for (bi, b) in hp.blocks.iter().enumerate() {
            if b.kind == BlockKind::Raw || b.body.len() < 2 {
                continue;
            }
            for si in 0..b.body.len() - 1 {
                let inline = matches!(&b.body[si], Sentence::Because { reasons, combination, stmt } if reasons.is_empty() && combination.is_empty() && cites_fact(&b.body[si + 1..], stmt));
                if !(cited.contains(&(b.id, si as u16)) || inline) || tried.contains(&(b.id, si as u16)) {
                    continue;
                }
                if let Some(st) = result_stmt(&b.body[si]) {
                    if st != b.stmt {
                        pick = Some((bi, si, st));
                        break 'outer;
                    }
                }
            }
        }
        let Some((bi, si, st)) = pick else { return };
        tried.insert((hp.blocks[bi].id, si as u16));
        let cand = prefix_out(hp, bi, si, st);
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
            tried.clear();
        } else if debug() {
            eprintln!("barem prefix step rejected: {:?}", v.first().map(|x| &x.detail));
        }
    }
}

fn prefix_out(hp: &HumanProof, bi: usize, j: usize, st: Stmt) -> HumanProof {
    let old = hp.blocks[bi].id;
    let mut out = hp.clone();
    remap_ids(&mut out, &|id| if id >= old { id + 1 } else { id });
    let parent = old + 1;
    let head: Vec<Sentence> = out.blocks[bi].body.drain(..=j).collect();
    for sent in out.blocks[bi].body.iter_mut() {
        super::claims::sentence_reasons_mut(sent, &mut |r| {
            if let Reason::Fact { stmt, block, .. } = r {
                if block.is_none() && *stmt == st {
                    *block = Some(old);
                }
            }
        });
    }
    visit_mut(&mut out, &mut |r| {
        if let Reason::Lemma { block, sentence, .. } = r {
            if *block == parent {
                if *sentence as usize <= j {
                    *block = old;
                } else {
                    *sentence -= (j + 1) as u16;
                }
            }
        }
    });
    let p = &out.blocks[bi];
    let mut points: Vec<PointId> = head.iter().flat_map(super::view::sentence_points).collect();
    points.sort_unstable();
    points.dedup();
    let block = Block {
        id: old,
        kind: BlockKind::Step,
        stmt: st,
        body: head,
        engine_facts: p.engine_facts.clone(),
        points,
        objects: Vec::new(),
        step: 0,
        tag: false,
        horizon: p.horizon,
    };
    out.blocks.insert(bi, block);
    out
}

pub fn cited_blocks(b: &Block) -> BTreeSet<u16> {
    let mut out = BTreeSet::new();
    for s in &b.body {
        visit(s, &mut |r| match r {
            Reason::Claim { block, .. } | Reason::Lemma { block, .. } => {
                out.insert(*block);
            }
            Reason::Fact { block: Some(x), .. } => {
                out.insert(*x);
            }
            Reason::Atom { from, .. } => out.extend(from.iter().copied()),
            _ => {}
        });
    }
    out.remove(&b.id);
    out
}

fn has_angle_chain(hp: &HumanProof, cx: &Ctx, directed: bool) -> bool {
    hp.blocks.iter().flat_map(|b| b.body.iter()).any(|s| match s {
        Sentence::Chain { terms, directed: d, .. } => *d == directed && !hides_terms(s) && terms.iter().any(|e| angle_table(cx, e) && super::expr::has_angle(e)),
        Sentence::Pooled { stmt, .. } => directed && matches!(stmt, Stmt::Coll { .. } | Stmt::Cyclic { .. } | Stmt::EqAngle { .. } | Stmt::AngleConst { .. } | Stmt::Para { .. } | Stmt::Perp { .. }),
        _ => false,
    })
}

pub fn hides_terms(s: &Sentence) -> bool {
    matches!(s, Sentence::Chain { terms, links, then: Some(Stmt::Coll { .. }), .. } if links.len() == 1 && matches!(terms.last(), Some(Expr::Const { degrees }) if degrees.is_zero()))
}

fn plan_worthy(b: &Block) -> bool {
    if b.kind == BlockKind::Raw {
        return false;
    }
    let promoted = matches!(b.body.as_slice(), [Sentence::Because { reasons, .. }] if reasons.len() == 1 && atom_key_args(&reasons[0]).is_some());
    let sum = matches!(&b.stmt, Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs } if lin_terms(lhs) + lin_terms(rhs) > 2);
    !promoted && !sum && !matches!(b.stmt, Stmt::Formula { .. })
}

pub fn finish(cx: &Ctx, hp: &mut HumanProof) {
    for (i, b) in hp.blocks.iter_mut().enumerate() {
        b.step = i as u16 + 1;
    }
    let mut cited: BTreeSet<u16> = BTreeSet::new();
    for b in &hp.blocks {
        cited.extend(cited_blocks(b).into_iter().filter(|&x| x < b.id));
    }
    for b in hp.blocks.iter_mut() {
        b.tag = cited.contains(&b.id) && b.kind != BlockKind::Conclusion;
    }
    hp.plan.clear();
    if hp.blocks.len() >= PLAN_MIN_STEPS {
        if let Some(c) = hp.blocks.last().filter(|b| b.kind == BlockKind::Conclusion) {
            let direct: Vec<u16> = cited_blocks(c).into_iter().filter(|x| hp.blocks.iter().any(|b| b.id == *x && plan_worthy(b))).collect();
            let skip = direct.len().saturating_sub(PLAN_MAX);
            hp.plan = direct.into_iter().skip(skip).collect();
        }
    }
    hp.goal = Some(goal_words(cx));
    let directed = has_angle_chain(hp, cx, true);
    let undirected = has_angle_chain(hp, cx, false);
    hp.setup.retain(|s| !matches!(s, SetupLine::DirectedAngles | SetupLine::FigureAngles));
    let mut conv: Vec<SetupLine> = Vec::new();
    if undirected {
        conv.push(SetupLine::FigureAngles);
    }
    if directed {
        conv.push(SetupLine::DirectedAngles);
    }
    let at = hp.setup.iter().position(|s| !matches!(s, SetupLine::Notation { .. })).unwrap_or(hp.setup.len());
    for (k, c) in conv.into_iter().enumerate() {
        hp.setup.insert(at + k, c);
    }
    hp.as_drawn = undirected;
}

pub fn goal_words(cx: &Ctx) -> GoalWords {
    let g = &cx.goal;
    let p = &g.points;
    match g.name.as_str() {
        "coll" if p.len() == 3 && p.iter().filter(|&&x| x < 3).count() == 2 => {
            let last = *p.iter().max().unwrap();
            let rest: Vec<PointId> = p.iter().copied().filter(|&x| x != last).collect();
            GoalWords::OnLine { p: last, line: (rest[0], rest[1]) }
        }
        "coll" => GoalWords::Collinear { pts: p.clone() },
        "cyclic" => GoalWords::Concyclic { pts: p.clone() },
        "eqangle" if p.len() == 8 => {
            let (l1, l2, l3, l4) = ((p[0], p[1]), (p[2], p[3]), (p[4], p[5]), (p[6], p[7]));
            let same = |a: (PointId, PointId), b: (PointId, PointId)| (a.0 == b.0 && a.1 == b.1) || (a.0 == b.1 && a.1 == b.0);
            let shared = |a: (PointId, PointId), b: (PointId, PointId)| [a.0, a.1].into_iter().find(|x| *x == b.0 || *x == b.1);
            if same(l2, l3) {
                if let (Some(v), Some(v2)) = (shared(l1, l2), shared(l3, l4)) {
                    if v == v2 && shared(l1, l4) == Some(v) {
                        let other = |a: (PointId, PointId)| if a.0 == v { a.1 } else { a.0 };
                        return GoalWords::Bisects { line: (v, other(l2)), angle: (other(l1), v, other(l4)) };
                    }
                }
            }
            GoalWords::Stmt
        }
        _ => GoalWords::Stmt,
    }
}

pub fn chain_count(hp: &HumanProof, cx: &Ctx) -> (usize, usize, usize) {
    let mut multi = 0;
    let mut und = 0;
    let mut dir = 0;
    for b in &hp.blocks {
        for s in &b.body {
            match s {
                Sentence::Chain { links, directed, terms, .. } => {
                    multi += links.iter().filter(|l| link_facts(l) > 1).count();
                    if !hides_terms(s) && terms.iter().any(|e| angle_table(cx, e) && super::expr::has_angle(e)) {
                        if *directed {
                            dir += 1;
                        } else {
                            und += 1;
                        }
                    }
                }
                Sentence::Computation { links, .. } => multi += links.iter().filter(|l| link_facts(l) > 1).count(),
                _ => {}
            }
        }
    }
    (multi, und, dir)
}
