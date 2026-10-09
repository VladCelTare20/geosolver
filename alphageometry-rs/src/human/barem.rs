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
    median_hypotenuse(cx, &mut ch, hp);
    per_sentence(cx, &mut ch, hp, false, &|cx, blocks, _bi, s| undirected(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| split_sentence(cx, blocks, bi, s));
    promote_atoms(cx, &mut ch, hp);
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| split_sentence(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|_, _, _, s| merge_sentence(s));
    per_sentence(cx, &mut ch, hp, true, &|_, _, _, s| merge_same_reasons(s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| dedupe_restated(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| absorb_by_triangles(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| restated_line_fact(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| vertical_angles(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| redisplay_middles(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| super::cyclic::merge_central(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| super::cyclic::central_isosceles(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| super::cyclic::radius_chord(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| split_sentence(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| expose_addition(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, bi, s| because_chain(cx, blocks, bi, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, _, _, s| undirected_hyps(cx, s));
    per_block(cx, &mut ch, hp, &congruence_side);
    per_block(cx, &mut ch, hp, &super::triangles::arc_chord_isosceles);
    per_block_with(cx, &mut ch, hp, &super::triangles::hence_premises);
    per_sentence(cx, &mut ch, hp, true, &|_, blocks, bi, s| restated_conclusion(blocks, bi, s));
    let named: Vec<(PointId, Vec<PointId>)> = hp.setup.iter().filter_map(|l| if let SetupLine::Circle { name, through, centre: Some(o), .. } = l { (!name.is_empty()).then(|| (*o, through.clone())) } else { None }).collect();
    per_sentence(cx, &mut ch, hp, true, &|_, _, _, s| drop_setup_radii(&named, s));
    per_sentence(cx, &mut ch, hp, true, &|_, _, _, s| trim_parallel_extras(s));
    per_sentence(cx, &mut ch, hp, false, &|cx, blocks, _, s| redisplay_drawn(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, _, _, s| stable_names(cx, s));
    per_sentence(cx, &mut ch, hp, true, &|cx, blocks, _, s| super::cyclic::cyclic_quads(cx, blocks, s));
    per_sentence(cx, &mut ch, hp, false, &|_, _, _, s| constants_first(s));
    cut_long(cx, &mut ch, hp);
    lemma_steps(cx, &mut ch, hp);
    prefix_steps(cx, &mut ch, hp);
    per_block(cx, &mut ch, hp, &super::triangles::triangle_names);
    per_block_with(cx, &mut ch, hp, &super::triangles::common_angle);
    super::cyclic::converse_steps(cx, &mut ch, hp);
    if !check::violations_with(&mut ch, hp).is_empty() {
        if std::env::var_os("HP_DEBUG").is_some() {
            eprintln!("barem polish produced violations; keeping the unpolished proof");
        }
        *hp = original;
    }
    finish(cx, hp);
}

pub(crate) fn debug() -> bool {
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

pub(crate) fn negate_link(l: Link) -> Link {
    Link { reasons: l.reasons, combination: l.combination.into_iter().map(|t| Term { coef: -&t.coef, ..t }).collect() }
}

pub fn term_score(e: &Expr) -> Option<i64> {
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
            let positive_angle = terms.iter().any(|(k, x)| matches!(x, Expr::Angle { .. }) && !k.is_negative());
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
                            s += if positive_const { 1 } else if positive_angle { 2 } else { 4 };
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
                if debug() {
                    eprintln!("undirected_chain neg {neg} node {j}: no display");
                }
                ok = false;
                break;
            };
            if debug() {
                eprintln!("undirected_chain neg {neg} node {j}: {:?} score {:?}", e, term_score(&e));
            }
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

fn triangle_claim(blocks: &[Block], r: &Reason) -> bool {
    match r {
        Reason::Claim { block, .. } => blocks.iter().any(|b| b.id == *block && matches!(b.stmt, Stmt::Sim { .. } | Stmt::Congruent { .. })),
        Reason::Fact { stmt, .. } | Reason::Lemma { stmt, .. } => matches!(stmt, Stmt::Sim { .. } | Stmt::Congruent { .. }),
        _ => false,
    }
}

fn single_reason_link(cx: &Ctx, blocks: &[Block], tb: Table, diff: &LinComb, r: &Reason, directed: bool) -> Option<Link> {
    let rows = check::reason_rows(cx.t, blocks, r)?;
    let idx: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].0 == tb).collect();
    if debug() {
        eprintln!("single_reason_link: tb {:?} diff {:?} q {:?} rows {:?}", tb, diff, cx.quot.q(diff), rows.iter().map(|(t, r)| (t, cx.quot.q(r))).collect::<Vec<_>>());
    }
    let one = Rat::one();
    let mut cands: Vec<Vec<Term>> = Vec::new();
    for &i in &idx {
        cands.push(vec![Term { reason: 0, row: i as u16, coef: one.clone() }]);
    }
    for (x, &i) in idx.iter().enumerate() {
        for &j in &idx[x + 1..] {
            for sj in [one.clone(), -&one] {
                cands.push(vec![Term { reason: 0, row: i as u16, coef: one.clone() }, Term { reason: 0, row: j as u16, coef: sj }]);
            }
        }
    }
    for combination in cands {
        let link = Link { reasons: vec![r.clone()], combination };
        let Some(sum) = link_sum(cx, blocks, &link, !directed) else { continue };
        if check::residual_ok(cx, tb, diff, &sum, directed) {
            return Some(link);
        }
        if check::residual_ok(cx, tb, diff, &sum.negated(), directed) {
            return Some(negate_link(link));
        }
    }
    None
}

fn absorb_by_triangles(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, then, directed } = s else { return None };
    if links.len() < 2 {
        return None;
    }
    for i in 0..links.len() - 1 {
        let (Some((tb, a)), Some((_, b))) = (eval(cx.t, &terms[i]), eval(cx.t, &terms[i + 2])) else { continue };
        let diff = &a - &b;
        for r in links[i].reasons.iter().chain(links[i + 1].reasons.iter()) {
            if debug() {
                eprintln!("absorb: link {i} reason {:?} triangle {} rows {:?}", r, triangle_claim(blocks, r), check::reason_rows(cx.t, blocks, r).map(|v| v.len()));
            }
            if !triangle_claim(blocks, r) {
                continue;
            }
            if let Some(l) = single_reason_link(cx, blocks, tb, &diff, r, *directed) {
                let mut nt = terms.clone();
                nt.remove(i + 1);
                let mut nl = links.clone();
                nl.splice(i..i + 2, [l]);
                return Some(Sentence::Chain { terms: nt, links: nl, then: then.clone(), directed: *directed });
            }
        }
    }
    None
}

fn trim_parallel_link(l: &Link) -> Option<Link> {
    let Some(Reason::Atom { key: AtomKey::Parallel, args, .. }) = l.reasons.first() else { return None };
    if args.len() != 6 || l.reasons.len() <= 3 {
        return None;
    }
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    let extras: Vec<Reason> = l.reasons[1..].to_vec();
    let mut keep: Vec<usize> = (0..l.reasons.len()).filter(|&i| used.contains(&(i as u16))).collect();
    for i in 0..2 {
        if let Some(k) = super::text::parallel_justification(args, &extras, i) {
            keep.push(k + 1);
        }
    }
    keep.sort_unstable();
    keep.dedup();
    if keep.len() == l.reasons.len() {
        return None;
    }
    let map: BTreeMap<u16, u16> = keep.iter().enumerate().map(|(n, &o)| (o as u16, n as u16)).collect();
    Some(Link { reasons: keep.iter().map(|&i| l.reasons[i].clone()).collect(), combination: l.combination.iter().map(|t| Term { reason: map[&t.reason], ..t.clone() }).collect() })
}

fn trim_parallel_extras(s: &Sentence) -> Option<Sentence> {
    match s {
        Sentence::Because { stmt, reasons, combination } => {
            let l = trim_parallel_link(&Link { reasons: reasons.clone(), combination: combination.clone() })?;
            Some(Sentence::Because { stmt: stmt.clone(), reasons: l.reasons, combination: l.combination })
        }
        Sentence::Chain { terms, links, then, directed } => {
            let mut changed = false;
            let out: Vec<Link> = links
                .iter()
                .map(|l| match trim_parallel_link(l) {
                    Some(n) => {
                        changed = true;
                        n
                    }
                    None => l.clone(),
                })
                .collect();
            changed.then(|| Sentence::Chain { terms: terms.clone(), links: out, then: then.clone(), directed: *directed })
        }
        _ => None,
    }
}

fn setup_radius(named: &[(PointId, Vec<PointId>)], r: &Reason) -> bool {
    let st = match r {
        Reason::Hyp { stmt, .. } | Reason::Fact { stmt, because: _, .. } => stmt,
        Reason::Atom { key: AtomKey::Radii, stmt, .. } => stmt,
        _ => return false,
    };
    let Stmt::Cong { s1, s2 } = st else { return false };
    named.iter().any(|(o, through)| {
        let end = |s: &(PointId, PointId)| if s.0 == *o { Some(s.1) } else if s.1 == *o { Some(s.0) } else { None };
        matches!((end(s1), end(s2)), (Some(a), Some(b)) if through.contains(&a) && through.contains(&b))
    })
}

fn drop_setup_radii(named: &[(PointId, Vec<PointId>)], s: &Sentence) -> Option<Sentence> {
    if let Sentence::Because { stmt, reasons, combination } = s {
        if combination.is_empty() {
            return None;
        }
        let l = drop_setup_radii(named, &Sentence::Chain { terms: Vec::new(), links: vec![Link { reasons: reasons.clone(), combination: combination.clone() }], then: None, directed: false })?;
        let Sentence::Chain { links, .. } = l else { return None };
        let l = links.into_iter().next()?;
        return Some(Sentence::Because { stmt: stmt.clone(), reasons: l.reasons, combination: l.combination });
    }
    let Sentence::Chain { terms, links, then, directed } = s else { return None };
    if terms.iter().any(super::expr::has_angle) {
        return None;
    }
    let mut changed = false;
    let mut out: Vec<Link> = Vec::new();
    for l in links {
        let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
        let keep: Vec<usize> = (0..l.reasons.len()).filter(|&i| used.contains(&(i as u16)) || !setup_radius(named, &l.reasons[i])).collect();
        if keep.len() == l.reasons.len() {
            out.push(l.clone());
            continue;
        }
        changed = true;
        let map: BTreeMap<u16, u16> = keep.iter().enumerate().map(|(n, &o)| (o as u16, n as u16)).collect();
        out.push(Link { reasons: keep.iter().map(|&i| l.reasons[i].clone()).collect(), combination: l.combination.iter().map(|t| Term { reason: map[&t.reason], ..t.clone() }).collect() });
    }
    changed.then(|| Sentence::Chain { terms: terms.clone(), links: out, then: then.clone(), directed: *directed })
}

fn doubled_expr(e: &Expr) -> bool {
    matches!(e, Expr::Lin { terms } if terms.iter().any(|(k, x)| matches!(x, Expr::Angle { directed: true, .. } | Expr::LineAngle { directed: true, .. }) && !k.abs().is_one()))
}

fn doubled_directed(s: &Sentence) -> bool {
    match s {
        Sentence::Chain { terms, directed: true, .. } => terms.iter().any(doubled_expr),
        Sentence::Because { stmt: Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs }, .. } => doubled_expr(lhs) || doubled_expr(rhs),
        _ => false,
    }
}

fn median_hypotenuse_body(cx: &Ctx, b: &Block) -> Option<Vec<Sentence>> {
    let t = cx.t;
    let Stmt::Cong { s1, s2 } = &b.stmt else { return None };
    if !b.body.iter().any(doubled_directed) {
        return None;
    }
    let o = [s1.0, s1.1].into_iter().find(|p| *p == s2.0 || *p == s2.1)?;
    let x = if s1.0 == o { s1.1 } else { s1.0 };
    let y = if s2.0 == o { s2.1 } else { s2.0 };
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9 * a.abs().max(b.abs()).max(1.0);
    let right = |v: PointId, p: PointId, q: PointId| {
        let ((vx, vy), (px, py), (qx, qy)) = (t.coord(v), t.coord(p), t.coord(q));
        let dot = (px - vx) * (qx - vx) + (py - vy) * (qy - vy);
        dot.abs() < 1e-9 * (t.dist(v, p) * t.dist(v, q)).max(1e-12)
    };
    for (xx, other) in [(x, y), (y, x)] {
        for q in 0..t.n as PointId {
            if [o, xx, other].contains(&q) || t.name(q).starts_with('_') {
                continue;
            }
            let ((ox, oy), (px, py), (qx, qy)) = (t.coord(o), t.coord(other), t.coord(q));
            if !near(ox * 2.0, px + qx) || !near(oy * 2.0, py + qy) || !right(xx, other, q) {
                continue;
            }
            let args = vec![o, other, q, xx];
            let stmt = super::claims::atom_stmt(AtomKey::MedianHypotenuse, &args);
            let coef = if s1.0 == xx || s1.1 == xx { Rat::one() } else { Rat::from_int(-1) };
            let reason = Reason::Atom { key: AtomKey::MedianHypotenuse, stmt, args, from: Vec::new() };
            return Some(vec![Sentence::Because { stmt: b.stmt.clone(), reasons: vec![reason], combination: vec![Term { reason: 0, row: 0, coef }] }]);
        }
    }
    None
}

fn median_hypotenuse(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    for bi in 0..hp.blocks.len() {
        if cx.timed_out() {
            return;
        }
        if hp.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        let Some(body) = median_hypotenuse_body(cx, &hp.blocks[bi]) else { continue };
        let mut cand = hp.clone();
        cand.blocks[bi].body = body;
        let v = check::violations_with(ch, &cand);
        if v.is_empty() {
            *hp = cand;
        } else if debug() {
            eprintln!("barem median to the hypotenuse in block {bi} rejected: {:?}", v.first().map(|x| &x.detail));
        }
    }
}

fn vertical_angles(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let owned: Link;
    let (l, st, goal) = match s {
        Sentence::Chain { links, then: Some(st @ Stmt::Coll { pts }), .. } if links.len() == 1 => (&links[0], st, pts),
        Sentence::Because { stmt: st @ Stmt::Coll { pts }, reasons, combination } if !combination.is_empty() => {
            owned = Link { reasons: reasons.clone(), combination: combination.clone() };
            (&owned, st, pts)
        }
        _ => return None,
    };
    let core: Vec<(usize, &Reason)> = l.reasons.iter().enumerate().filter(|(_, r)| !matches!(super::claims::reason_stmt(r), Some(Stmt::Coll { .. }))).collect();
    let [(ri, r)] = core.as_slice() else { return None };
    let (t1, t2) = match super::claims::reason_stmt(r)? {
        Stmt::Sim { t1, t2, .. } | Stmt::Congruent { t1, t2, .. } => (*t1, *t2),
        _ => return None,
    };
    let a = [t1.0, t1.1, t1.2];
    let b = [t2.0, t2.1, t2.2];
    let i = (0..3).find(|&i| a[i] == b[i] && goal.contains(&a[i]))?;
    let v = a[i];
    let others: Vec<usize> = (0..3).filter(|&j| j != i).collect();
    let in_goal = |j: usize| goal.contains(&a[j]) && goal.contains(&b[j]) && a[j] != b[j];
    let (g, p) = if in_goal(others[0]) { (others[0], others[1]) } else if in_goal(others[1]) { (others[1], others[0]) } else { return None };
    let (x, y, pp, qq) = (a[g], b[g], a[p], b[p]);
    if pp == qq || [pp, qq].iter().any(|z| goal.contains(z)) {
        return None;
    }
    let trio = [pp, v, qq];
    let hyp_line = cx.hyp_pred.iter().filter(|(f, p)| p.name == "coll" && matches!(cx.class.get(**f as usize), Some(super::ctx::FactClass::Hyp)) && trio.iter().all(|z| p.points.contains(z))).map(|(f, _)| *f).min();
    let coll_reason = if let Some(fact) = hyp_line {
        Reason::Hyp { stmt: Stmt::Coll { pts: trio.to_vec() }, fact }
    } else {
        let blk = blocks.iter().find(|bk| matches!(&bk.stmt, Stmt::Coll { pts } if trio.iter().all(|z| pts.contains(z))))?;
        Reason::Fact { stmt: blk.stmt.clone(), fact: *blk.engine_facts.first()?, block: Some(blk.id), because: Vec::new() }
    };
    let terms = vec![Expr::Angle { a: pp, b: v, c: x, directed: false }, Expr::Angle { a: qq, b: v, c: y, directed: false }];
    let (_, e0) = super::expr::eval(cx.t, &terms[0])?;
    let (_, e1) = super::expr::eval(cx.t, &terms[1])?;
    let diff = &e0 - &e1;
    let rows = check::reason_rows(cx.t, blocks, r)?;
    let ang: Vec<(usize, &LinComb)> = rows.iter().enumerate().filter(|(_, (tb, _))| *tb == Table::Angle).map(|(k, (_, row))| (k, row)).collect();
    let signs = [Rat::one(), Rat::from_int(-1)];
    let mut combination: Option<Vec<Term>> = None;
    'search: for (ai, (k1, r1)) in ang.iter().enumerate() {
        for c1 in &signs {
            let mut x = (*r1).clone();
            x.mul_assign_scalar(c1);
            if check::residual_ok(cx, Table::Angle, &diff, &x, false) {
                combination = Some(vec![Term { reason: 0, row: *k1 as u16, coef: c1.clone() }]);
                break 'search;
            }
            for (k2, r2) in ang.iter().skip(ai + 1) {
                for c2 in &signs {
                    let y = LinComb::combine(&x, r2, c2);
                    if check::residual_ok(cx, Table::Angle, &diff, &y, false) {
                        combination = Some(vec![Term { reason: 0, row: *k1 as u16, coef: c1.clone() }, Term { reason: 0, row: *k2 as u16, coef: c2.clone() }]);
                        break 'search;
                    }
                }
            }
        }
    }
    let _ = ri;
    if debug() {
        eprintln!("vertical angles: trio {trio:?} combination {combination:?}");
    }
    let link = Link { reasons: vec![(*r).clone(), coll_reason], combination: combination? };
    Some(Sentence::Chain { terms, links: vec![link], then: Some(st.clone()), directed: false })
}

fn restated_line_fact(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, then: Some(st @ (Stmt::Para { .. } | Stmt::Perp { .. })), .. } = s else { return None };
    let ([_, Expr::Const { degrees }], [l]) = (terms.as_slice(), links.as_slice()) else { return None };
    if !degrees.is_zero() && *degrees != Rat::from_int(90) {
        return None;
    }
    let [r] = l.reasons.as_slice() else { return None };
    let rs = super::claims::reason_stmt(r)?;
    let rows = check::stmt_targets(cx.t, st)?;
    let rrows = check::stmt_targets(cx.t, rs)?;
    let same = rows.len() == 1 && rrows.len() == 1 && (cx.quot.q(&cx.t.exact(Table::Angle, &rows[0].1)) == cx.quot.q(&cx.t.exact(Table::Angle, &rrows[0].1)) || cx.quot.q(&cx.t.exact(Table::Angle, &rows[0].1)) == cx.quot.q(&cx.t.exact(Table::Angle, &rrows[0].1)).negated());
    if !same {
        return None;
    }
    let _ = blocks;
    Some(Sentence::Because { stmt: st.clone(), reasons: l.reasons.clone(), combination: l.combination.clone() })
}

fn chain_score(terms: &[Expr]) -> Option<i64> {
    terms.iter().map(|e| term_score(e).filter(|s| *s < 4)).sum()
}

fn soft_score(terms: &[Expr]) -> i64 {
    terms.iter().map(|e| term_score(e).unwrap_or(8)).sum()
}

fn negated_expr(e: &Expr) -> Option<Expr> {
    let terms: Vec<(Rat, Expr)> = match e {
        Expr::Angle { .. } => vec![(Rat::one(), e.clone())],
        Expr::Const { degrees } => return Some(Expr::Const { degrees: -degrees }),
        Expr::Lin { terms } => terms.clone(),
        _ => return None,
    };
    let mut out: Vec<(Rat, Expr)> = Vec::new();
    let mut konst = Rat::zero();
    for (k, x) in terms {
        match x {
            Expr::Const { degrees } => konst = &konst - &(&degrees * &k),
            Expr::Angle { .. } | Expr::LineAngle { .. } => out.push((-&k, x)),
            _ => return None,
        }
    }
    out.sort_by_key(|(k, _)| k.is_negative());
    if !konst.is_zero() {
        let lead = !konst.is_negative() && out.iter().all(|(k, _)| k.is_negative());
        let c = (Rat::one(), Expr::Const { degrees: konst });
        if lead {
            out.insert(0, c);
        } else {
            out.push(c);
        }
    }
    Some(match out.as_slice() {
        [(k, x)] if k.is_one() => x.clone(),
        _ => Expr::Lin { terms: out },
    })
}

fn negate_drawn(s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, then, directed: false } = s else { return None };
    let nt: Vec<Expr> = terms.iter().map(negated_expr).collect::<Option<Vec<_>>>()?;
    let leading_minus = |ts: &[Expr]| ts.iter().filter(|e| matches!(e, Expr::Lin { terms } if terms.first().is_some_and(|(k, _)| k.is_negative()))).count();
    (soft_score(&nt) < soft_score(terms) && leading_minus(&nt) < leading_minus(terms)).then(|| Sentence::Chain { terms: nt, links: links.iter().cloned().map(negate_link).collect(), then: then.clone(), directed: false })
}

fn opaque_count(cx: &Ctx, blocks: &[Block], s: &Sentence) -> usize {
    let Sentence::Chain { terms, links, directed, .. } = s else { return 0 };
    links.iter().enumerate().filter(|(k, l)| link_facts(l) == 1 && !identity_only(l) && opaque_link(cx, blocks, &terms[*k], &terms[k + 1], l, *directed && is_directed(&terms[*k]) && is_directed(&terms[k + 1])).is_some()).count()
}

fn redisplay_drawn(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let n = redisplay_drawn_inner(cx, blocks, s)?;
    (opaque_count(cx, blocks, &n) <= opaque_count(cx, blocks, s)).then_some(n)
}

fn redisplay_drawn_inner(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    if let Some(n) = negate_drawn(s) {
        return Some(n);
    }
    let Sentence::Chain { terms, links, then, directed: false } = s else { return None };
    let old = chain_score(terms).unwrap_or(i64::MAX);
    if old == 0 {
        return None;
    }
    let mut best: Option<(i64, Sentence)> = None;
    if let Some(n @ Sentence::Chain { .. }) = undirected_chain(cx, blocks, terms, links, then.clone()) {
        if let Sentence::Chain { terms: nt, .. } = &n {
            if let Some(sc) = chain_score(nt).filter(|sc| *sc < old) {
                best = Some((sc, n.clone()));
            }
        }
    }
    let flipped: Option<Vec<Expr>> = terms.iter().map(supplement).collect();
    let plain_ends = |ts: &[Expr]| usize::from(matches!(ts.first(), Some(Expr::Angle { .. }))) + usize::from(matches!(ts.last(), Some(Expr::Angle { .. })));
    if let Some(ft) = flipped {
        let better_ends = plain_ends(&ft) > plain_ends(terms);
        if let Some(sc) = chain_score(&ft).filter(|sc| (*sc < old || (*sc == old && better_ends)) && best.as_ref().is_none_or(|b| *sc < b.0)) {
            best = Some((sc, Sentence::Chain { terms: ft, links: links.iter().cloned().map(negate_link).collect(), then: then.clone(), directed: false }));
        }
    }
    best.map(|b| b.1)
}

fn supplement(e: &Expr) -> Option<Expr> {
    let one_eighty = Rat::from_int(180);
    let terms: Vec<(Rat, Expr)> = match e {
        Expr::Angle { .. } => vec![(Rat::one(), e.clone())],
        Expr::Const { degrees } => vec![(Rat::one(), Expr::Const { degrees: degrees.clone() })],
        Expr::Lin { terms } => terms.clone(),
        _ => return None,
    };
    let mut konst = one_eighty.clone();
    let mut out: Vec<(Rat, Expr)> = Vec::new();
    for (k, x) in terms {
        match x {
            Expr::Const { degrees } => konst = &konst - &(&degrees * &k),
            Expr::Angle { .. } => out.push((-&k, x)),
            _ => return None,
        }
    }
    out.sort_by_key(|(k, _)| k.is_negative());
    if !konst.is_zero() {
        out.push((Rat::one(), Expr::Const { degrees: konst }));
    }
    Some(match out.as_slice() {
        [(k, x)] if k.is_one() => x.clone(),
        [(k, Expr::Const { degrees })] if k.is_one() => Expr::Const { degrees: degrees.clone() },
        _ => Expr::Lin { terms: out },
    })
}

fn restated_conclusion(blocks: &[Block], bi: usize, s: &Sentence) -> Option<Sentence> {
    let b = &blocks[bi];
    if b.kind != BlockKind::Conclusion || b.body.last() != Some(s) {
        return None;
    }
    let Sentence::Chain { links, then, .. } = s else { return None };
    if links.is_empty() || links.len() > 3 || !matches!(b.stmt, Stmt::EqAngle { .. }) || then.as_ref().is_some_and(|t| !matches!(t, Stmt::EqAngle { .. })) {
        return None;
    }
    let coll_claim = |r: &Reason| matches!(r, Reason::Claim { block, .. } if blocks.iter().any(|x| x.id == *block && matches!(x.stmt, Stmt::Coll { .. })));
    let plain = links.iter().all(|l| l.reasons.iter().all(|r| is_coll_reason(r) || coll_claim(r) || matches!(r, Reason::Hyp { .. })));
    let restates = links.iter().any(|l| l.reasons.iter().any(|r| coll_claim(r) || matches!(r, Reason::Lemma { stmt: Stmt::Coll { .. }, .. } | Reason::Fact { stmt: Stmt::Coll { .. }, block: Some(_), .. })));
    if !plain || !restates {
        return None;
    }
    let merged = links[1..].iter().fold(links[0].clone(), |a, l| merge_links(&a, l));
    Some(Sentence::Because { stmt: b.stmt.clone(), reasons: merged.reasons, combination: merged.combination })
}

fn per_block(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof, f: &dyn Fn(&Ctx, &Block) -> Option<Block>) {
    if cx.timed_out() {
        return;
    }
    let mut all = hp.clone();
    let mut any = false;
    for bi in 0..all.blocks.len() {
        if all.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        if let Some(nb) = f(cx, &all.blocks[bi]) {
            all.blocks[bi] = nb;
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
        if hp.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        let Some(nb) = f(cx, &hp.blocks[bi]) else { continue };
        let mut one = hp.clone();
        one.blocks[bi] = nb;
        let v = check::violations_with(ch, &one);
        if v.is_empty() {
            *hp = one;
        } else if debug() {
            eprintln!("barem block edit rejected in block {}: {:?}", hp.blocks[bi].id, v.first().map(|x| &x.detail));
        }
    }
}

fn per_block_with(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof, f: &dyn Fn(&Ctx, &[Block], &Block) -> Option<Block>) {
    if cx.timed_out() {
        return;
    }
    let mut all = hp.clone();
    let mut any = false;
    for bi in 0..all.blocks.len() {
        if all.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        if let Some(nb) = f(cx, &all.blocks[..bi], &all.blocks[bi]) {
            all.blocks[bi] = nb;
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
        if hp.blocks[bi].kind == BlockKind::Raw {
            continue;
        }
        let Some(nb) = f(cx, &hp.blocks[..bi], &hp.blocks[bi]) else { continue };
        let mut one = hp.clone();
        one.blocks[bi] = nb;
        let v = check::violations_with(ch, &one);
        if v.is_empty() {
            *hp = one;
        } else if debug() {
            eprintln!("barem block edit rejected in block {}: {:?}", hp.blocks[bi].id, v.first().map(|x| &x.detail));
        }
    }
}

fn shows_equal_side(body: &[Sentence]) -> bool {
    body.iter().any(|s| match s {
        Sentence::Because { stmt: Stmt::Cong { .. }, .. } => true,
        Sentence::Because { reasons, .. } => reasons.iter().any(|r| matches!(r, Reason::Hyp { stmt: Stmt::Cong { .. }, .. } | Reason::Fact { stmt: Stmt::Cong { .. }, .. } | Reason::Atom { stmt: Stmt::Cong { .. }, .. })),
        Sentence::Chain { terms, links, .. } => {
            links.len() == 1 && matches!(terms.as_slice(), [Expr::Seg { .. }, Expr::Seg { .. }])
                || links.iter().any(|l| l.reasons.iter().any(|r| matches!(r, Reason::Hyp { stmt: Stmt::Cong { .. }, .. } | Reason::Fact { stmt: Stmt::Cong { .. }, .. })))
        }
        _ => false,
    })
}

fn congruence_side(cx: &Ctx, b: &Block) -> Option<Block> {
    let Stmt::Congruent { t1, t2, .. } = &b.stmt else { return None };
    let last = b.body.len().checked_sub(1)?;
    let Sentence::Because { stmt: Stmt::Congruent { .. }, reasons, combination } = &b.body[last] else { return None };
    if shows_equal_side(&b.body) {
        return None;
    }
    let t = cx.t;
    let h = *b.engine_facts.first()?;
    let (a, c) = ([t1.0, t1.1, t1.2], [t2.0, t2.1, t2.2]);
    for (i, j) in [(0, 1), (1, 2), (0, 2)] {
        let (s1, s2) = ((a[i], a[j]), (c[i], c[j]));
        let row = &t.dm(s1.0, s1.1)? - &t.dm(s2.0, s2.1)?;
        let canon = t.canon_ratio(&row);
        if canon.is_zero() {
            continue;
        }
        let hit = cx.closure.iter().copied().filter(|&f| f < h && matches!(cx.class.get(f as usize), Some(super::ctx::FactClass::Hyp))).find(|&f| {
            t.rows_of(Table::Ratio, f).any(|r| t.canon_ratio(&(r - &row)).is_zero() || t.canon_ratio(&(r + &row)).is_zero())
        });
        if let Some(f) = hit {
            let mut nb = b.clone();
            let mut rs = reasons.clone();
            rs.push(Reason::Hyp { stmt: Stmt::Cong { s1, s2 }, fact: f });
            nb.body[last] = Sentence::Because { stmt: b.stmt.clone(), reasons: rs, combination: combination.clone() };
            return Some(nb);
        }
    }
    None
}

fn undirected_stmt(cx: &Ctx, st: &Stmt) -> Option<Stmt> {
    let Stmt::EqAngle { lhs: Expr::Angle { a, b, c, directed: true }, rhs: Expr::Angle { a: d, b: e, c: f, directed: true } } = st else { return None };
    let v1 = super::theorems::interior_degrees(cx.t, *a, *b, *c);
    let v2 = super::theorems::interior_degrees(cx.t, *d, *e, *f);
    if !(1e-6..180.0 - 1e-6).contains(&v1) {
        return None;
    }
    let lhs = Expr::Angle { a: *a, b: *b, c: *c, directed: false };
    let rhs = Expr::Angle { a: *d, b: *e, c: *f, directed: false };
    if (v1 - v2).abs() <= 1e-6 {
        return Some(Stmt::EqAngle { lhs, rhs });
    }
    if (v1 + v2 - 180.0).abs() <= 1e-6 {
        return Some(Stmt::Eq { lhs, rhs: Expr::Lin { terms: vec![(Rat::one(), Expr::Const { degrees: Rat::from_int(180) }), (Rat::from_int(-1), rhs)] } });
    }
    None
}

fn undirected_hyps(cx: &Ctx, s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed: false, then } = s else { return None };
    let mut changed = false;
    let links: Vec<Link> = links
        .iter()
        .map(|l| {
            let mut l = l.clone();
            for r in l.reasons.iter_mut() {
                if let Reason::Hyp { stmt, fact } = r {
                    if let Some(st) = undirected_stmt(cx, stmt) {
                        *r = Reason::Hyp { stmt: st, fact: *fact };
                        changed = true;
                    }
                }
            }
            l
        })
        .collect();
    changed.then(|| Sentence::Chain { terms: terms.clone(), links, directed: false, then: then.clone() })
}

#[derive(Clone)]
pub(crate) struct TermInfo {
    pub coef: Rat,
    pub expr: Expr,
    pub vertex: Option<PointId>,
    pub pts: Vec<PointId>,
    pub key: String,
}

pub(crate) fn term_infos(cx: &Ctx, e: &Expr, directed: bool) -> Option<Vec<TermInfo>> {
    let t = cx.t;
    let parts: Vec<(Rat, Expr)> = match e {
        Expr::Lin { terms } => terms.clone(),
        x => vec![(Rat::one(), x.clone())],
    };
    let mut out = Vec::new();
    for (k, x) in parts {
        let (vertex, pts) = match &x {
            Expr::Const { .. } => continue,
            Expr::Angle { a, b, c, .. } => (Some(*b), vec![*a, *b, *c]),
            Expr::LineAngle { l1, l2, .. } => (None, vec![l1.0, l1.1, l2.0, l2.1]),
            _ => return None,
        };
        let (tb, row) = eval(t, &x)?;
        if tb != Table::Angle {
            return None;
        }
        let mut q = cx.quot.q(&row);
        if directed {
            let c = q.get(crate::elimination::ANGLE_UNIT);
            q.add_term(crate::elimination::ANGLE_UNIT, &c.mod_one() - &c);
        }
        out.push(TermInfo { coef: k, expr: x, vertex, pts, key: format!("{q:?}") });
    }
    Some(out)
}

fn key_map(infos: &[TermInfo]) -> BTreeMap<String, Rat> {
    let mut m: BTreeMap<String, Rat> = BTreeMap::new();
    for ti in infos {
        let e = m.entry(ti.key.clone()).or_insert_with(Rat::zero);
        *e = &*e + &ti.coef;
    }
    m.retain(|_, v| !v.is_zero());
    m
}

pub(crate) fn diff_terms(cx: &Ctx, a: &Expr, b: &Expr, directed: bool) -> Option<Vec<(Rat, TermInfo)>> {
    let (ia, ib) = (term_infos(cx, a, directed)?, term_infos(cx, b, directed)?);
    let (ma, mb) = (key_map(&ia), key_map(&ib));
    let keys: BTreeSet<&String> = ma.keys().chain(mb.keys()).collect();
    let mut out = Vec::new();
    for k in keys {
        let d = &ma.get(k).cloned().unwrap_or_else(Rat::zero) - &mb.get(k).cloned().unwrap_or_else(Rat::zero);
        if d.is_zero() {
            continue;
        }
        let rep = ia.iter().chain(ib.iter()).find(|x| &x.key == k)?.clone();
        out.push((d, rep));
    }
    Some(out)
}

pub fn row_delta(cx: &Ctx, a: &Expr, b: &Expr, directed: bool) -> Option<usize> {
    diff_terms(cx, a, b, directed).map(|d| d.len())
}

pub(crate) fn near_line(cx: &Ctx, x: PointId, y: PointId, p: PointId) -> bool {
    let t = cx.t;
    let (a, b, c) = (t.coord(x), t.coord(y), t.coord(p));
    let (u, w) = ((b.0 - a.0, b.1 - a.1), (c.0 - a.0, c.1 - a.1));
    let nu = (u.0 * u.0 + u.1 * u.1).sqrt();
    let nw = (w.0 * w.0 + w.1 * w.1).sqrt();
    if nu < 1e-12 {
        return false;
    }
    nw < 1e-12 || (u.0 * w.1 - u.1 * w.0).abs() <= 1e-7 * nu * nw.max(1.0)
}

pub fn same_ray(cx: &Ctx, v: PointId, a: PointId, p: PointId) -> bool {
    let (o, x, y) = (cx.t.coord(v), cx.t.coord(a), cx.t.coord(p));
    a == p || (near_line(cx, v, a, p) && (x.0 - o.0) * (y.0 - o.0) + (x.1 - o.1) * (y.1 - o.1) > 0.0)
}

pub fn opposite_ray(cx: &Ctx, v: PointId, a: PointId, p: PointId) -> bool {
    let (o, x, y) = (cx.t.coord(v), cx.t.coord(a), cx.t.coord(p));
    a != v && p != v && near_line(cx, v, a, p) && (x.0 - o.0) * (y.0 - o.0) + (x.1 - o.1) * (y.1 - o.1) < 0.0
}

fn angle_rays(e: &Expr) -> Option<(PointId, PointId, PointId)> {
    match e {
        Expr::Angle { a, b, c, .. } => Some((*a, *b, *c)),
        _ => None,
    }
}

fn renamed_pair(cx: &Ctx, a: &Expr, b: &Expr) -> Option<(AtomKey, PointId)> {
    let (ia, ib) = (term_infos(cx, a, false)?, term_infos(cx, b, false)?);
    let mut found: Option<(AtomKey, PointId)> = None;
    let mut used = vec![false; ib.len()];
    for x in &ia {
        let j = (0..ib.len()).find(|&j| !used[j] && ib[j].key == x.key && ib[j].coef == x.coef)?;
        used[j] = true;
        if ib[j].expr == x.expr {
            continue;
        }
        let ((p, v, q), (r, w, u)) = (angle_rays(&x.expr)?, angle_rays(&ib[j].expr)?);
        if v != w || found.is_some() {
            return None;
        }
        let vertical = (opposite_ray(cx, v, p, r) && opposite_ray(cx, v, q, u)) || (opposite_ray(cx, v, p, u) && opposite_ray(cx, v, q, r));
        let same = (same_ray(cx, v, p, r) && same_ray(cx, v, q, u)) || (same_ray(cx, v, p, u) && same_ray(cx, v, q, r));
        found = Some((if vertical { AtomKey::VerticalAngles } else if same { AtomKey::CommonAngle } else { return None }, v));
    }
    if used.iter().any(|u| !u) {
        return None;
    }
    found
}

fn supplementary(cx: &Ctx, d: &[(Rat, TermInfo)]) -> Option<Vec<PointId>> {
    let [(k1, t1), (k2, t2)] = d else { return None };
    if !k1.abs().is_one() || k1 != k2 {
        return None;
    }
    let ((p, v, q), (r, w, u)) = (angle_rays(&t1.expr)?, angle_rays(&t2.expr)?);
    if v != w {
        return None;
    }
    for (x, y, rest1, rest2) in [(p, r, q, u), (p, u, q, r), (q, r, p, u), (q, u, p, r)] {
        if opposite_ray(cx, v, x, y) && same_ray(cx, v, rest1, rest2) {
            let sum = super::theorems::interior_degrees(cx.t, p, v, q) + super::theorems::interior_degrees(cx.t, r, v, u);
            return ((sum - 180.0).abs() < 1e-6).then(|| vec![v, x, y]);
        }
    }
    None
}

pub fn identity_label(cx: &Ctx, a: &Expr, b: &Expr, directed: bool) -> Option<(AtomKey, Vec<PointId>)> {
    let d = diff_terms(cx, a, b, directed)?;
    if d.is_empty() {
        if directed {
            return None;
        }
        return renamed_pair(cx, a, b).map(|(k, v)| (k, vec![v]));
    }
    let mut vs: Vec<PointId> = Vec::new();
    for (_, ti) in &d {
        let Some(v) = ti.vertex else { return Some((AtomKey::AngleAddition, Vec::new())) };
        if !vs.contains(&v) {
            vs.push(v);
        }
    }
    if vs.len() == 1 {
        if !directed {
            if let Some(args) = supplementary(cx, &d) {
                return Some((AtomKey::Supplementary, args));
            }
        }
        return Some((AtomKey::AngleAddition, vs));
    }
    if vs.len() == 3 {
        let (x, y, z) = (vs[0], vs[1], vs[2]);
        let tri = !near_line(cx, x, y, z);
        let sides = d.iter().all(|(_, ti)| {
            let v = ti.vertex.unwrap_or(x);
            let others: Vec<PointId> = vs.iter().copied().filter(|&w| w != v).collect();
            [ti.pts[0], ti.pts[2]].iter().all(|&p| p != v && others.iter().any(|&w| near_line(cx, v, w, p)))
        });
        if tri && sides {
            if d.len() == 3 && !directed {
                let neg: Vec<usize> = (0..3).filter(|&i| d[i].0.is_negative()).collect();
                let odd = match neg.len() {
                    1 => Some(neg[0]),
                    2 => (0..3).find(|i| !neg.contains(i)),
                    _ => None,
                };
                if let Some(i) = odd {
                    let v = d[i].1.vertex.unwrap_or(x);
                    let rest: Vec<PointId> = vs.iter().copied().filter(|&w| w != v).collect();
                    let interior = super::theorems::interior_degrees(cx.t, rest[0], v, rest[1]);
                    if let Expr::Angle { a: p, b: q, c: r, .. } = d[i].1.expr {
                        let val = super::theorems::interior_degrees(cx.t, p, q, r);
                        if (val + interior - 180.0).abs() < 1e-6 {
                            return Some((AtomKey::ExteriorAngle, vec![v, rest[0], rest[1]]));
                        }
                    }
                }
            }
            return Some((AtomKey::AngleSum, vs));
        }
    }
    Some((AtomKey::AngleAddition, Vec::new()))
}

pub fn identity_link(cx: &Ctx, a: &Expr, b: &Expr, directed: bool) -> Option<Link> {
    let (key, args) = identity_label(cx, a, b, directed)?;
    let stmt = Stmt::Formula { text: String::new(), pts: args.clone() };
    Some(Link { reasons: vec![Reason::Atom { key, stmt, args, from: Vec::new() }], combination: Vec::new() })
}

fn tidy(e: Expr) -> Expr {
    match e {
        Expr::Lin { terms } if terms.len() == 1 && terms[0].0.is_one() => terms.into_iter().next().map(|x| x.1).unwrap_or(Expr::Const { degrees: Rat::zero() }),
        e => e,
    }
}

fn nice_mid(cx: &Ctx, x: &Expr, directed: bool) -> bool {
    if lin_terms(x) > 3 {
        return false;
    }
    let parts: Vec<(Rat, Expr)> = match x {
        Expr::Lin { terms } => terms.clone(),
        e => vec![(Rat::one(), e.clone())],
    };
    for (k, e) in &parts {
        match e {
            Expr::Const { degrees } => {
                let c = degrees * k;
                if ![0, 90, 180, -90, -180].iter().any(|v| c == Rat::from_int(*v)) {
                    return false;
                }
            }
            Expr::Angle { .. } | Expr::LineAngle { .. } => {
                let a = k.abs();
                if directed && !a.is_one() {
                    return false;
                }
                if !(a.is_one() || a == Rat::from_int(2) || a == Rat::new(1, 2)) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    if directed {
        !super::chain::fractional_angle(x)
    } else {
        undirect_expr(x) && !super::chain::degenerate_angle(cx, x)
    }
}

fn subst_candidates(cx: &Ctx, e1: &Expr, e2: &Expr, l: &Link, directed: bool, focus: &BTreeSet<PointId>) -> Vec<(bool, Expr)> {
    let t = cx.t;
    let mut out = Vec::new();
    let (Some((_, r1)), Some((_, r2))) = (eval(t, e1), eval(t, e2)) else { return out };
    let (q1, q2) = (cx.quot.q(&r1), cx.quot.q(&r2));
    let delta = LinComb::combine(&q2, &q1, &Rat::from_int(-1));
    let fpts = link_points(l);
    for (fact_first, base, target) in [(true, e1, &q2), (false, e2, &q1)] {
        let Some(infos) = term_infos(cx, base, directed) else { continue };
        let sign = if fact_first { Rat::one() } else { Rat::from_int(-1) };
        for (i, ti) in infos.iter().enumerate() {
            let Some((_, tr)) = eval(t, &ti.expr) else { continue };
            let mut nr = cx.quot.q(&tr);
            nr.iadd_mul(&delta, &(&sign / &ti.coef));
            let cl = super::chain::classes(&nr);
            if cl.len() != 2 || !cl[0].1.abs().is_one() || cl[0].1 != -&cl[1].1 {
                continue;
            }
            let near: [&[PointId]; 2] = [&fpts, &ti.pts];
            let Some(ne) = angle_expr(cx, &nr, directed, &near, focus) else { continue };
            let mut xs: Vec<(Rat, Expr)> = Vec::new();
            for (j, tj) in infos.iter().enumerate() {
                if j != i {
                    xs.push((tj.coef.clone(), tj.expr.clone()));
                    continue;
                }
                match &ne {
                    Expr::Lin { terms } => {
                        for (k, x) in terms {
                            if !matches!(x, Expr::Const { .. }) {
                                xs.push((k * &ti.coef, x.clone()));
                            }
                        }
                    }
                    x => xs.push((ti.coef.clone(), x.clone())),
                }
            }
            let Some(x) = with_const(cx, xs, target, directed) else { continue };
            let x = tidy(x);
            if x == *e1 || x == *e2 || !nice_mid(cx, &x, directed) || cancelling(cx, &x) || !shows(cx, Table::Angle, &x, target, directed) {
                continue;
            }
            out.push((fact_first, x));
        }
    }
    out
}

fn expose_link(cx: &Ctx, blocks: &[Block], e1: &Expr, e2: &Expr, l: &Link, directed: bool, focus: &BTreeSet<PointId>) -> Option<(i64, Expr, bool, Link)> {
    let mut best: Option<(i64, Expr, bool, Link)> = None;
    for (fact_first, x) in subst_candidates(cx, e1, e2, l, directed, focus) {
        let (ia, ib) = if fact_first { (&x, e2) } else { (e1, &x) };
        let Some(idelta) = row_delta(cx, ia, ib, directed) else { continue };
        if idelta == 0 && !matches!(identity_label(cx, ia, ib, directed), Some((AtomKey::VerticalAngles, _))) {
            continue;
        }
        let fdelta = if fact_first { row_delta(cx, e1, &x, directed) } else { row_delta(cx, &x, e2, directed) }.unwrap_or(9);
        if fdelta > 2 {
            continue;
        }
        let (fa, fb) = if fact_first { (e1, &x) } else { (&x, e2) };
        if opaque_link(cx, blocks, fa, fb, l, directed).is_some() {
            continue;
        }
        let Some(label) = identity_link(cx, ia, ib, directed) else { continue };
        let generic = matches!(label.reasons.first(), Some(Reason::Atom { key: AtomKey::AngleAddition, args, .. }) if args.is_empty());
        if generic && !directed {
            continue;
        }
        let score = 10 * idelta as i64 + 3 * fdelta as i64 + badness(&x) + if generic { 25 } else { 0 };
        if best.as_ref().is_none_or(|b| score < b.0) {
            best = Some((score, x, fact_first, label));
        }
    }
    best
}

fn link_cost(cx: &Ctx, blocks: &[Block], a: &Expr, b: &Expr, l: &Link, directed: bool) -> usize {
    if identity_only(l) || link_facts(l) != 1 {
        return 0;
    }
    opaque_link(cx, blocks, a, b, l, directed).unwrap_or(0)
}

fn redisplay_middles(cx: &Ctx, blocks: &[Block], bi: usize, s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    if terms.len() < 3 || !terms.first().is_some_and(|e| angle_table(cx, e)) {
        return None;
    }
    let focus: BTreeSet<PointId> = blocks[bi].points.iter().copied().collect();
    let mut terms = terms.clone();
    let mut changed = false;
    for k in 1..terms.len() - 1 {
        let d1 = *directed && is_directed(&terms[k - 1]) && is_directed(&terms[k]);
        let d2 = *directed && is_directed(&terms[k]) && is_directed(&terms[k + 1]);
        if d1 != d2 {
            continue;
        }
        let cost = |x: &Expr| link_cost(cx, blocks, &terms[k - 1], x, &links[k - 1], d1) + link_cost(cx, blocks, x, &terms[k + 1], &links[k], d2);
        if cost(&terms[k]) == 0 {
            continue;
        }
        let mut cands: Vec<Expr> = subst_candidates(cx, &terms[k - 1], &terms[k], &links[k - 1], d1, &focus).into_iter().filter(|c| c.0).map(|c| c.1).collect();
        cands.extend(subst_candidates(cx, &terms[k], &terms[k + 1], &links[k], d2, &focus).into_iter().filter(|c| !c.0).map(|c| c.1));
        let best = cands.into_iter().map(|x| (cost(&x), badness(&x), x)).filter(|c| c.0 == 0).min_by_key(|c| (c.0, c.1));
        if let Some((_, _, x)) = best {
            terms[k] = x;
            changed = true;
        }
    }
    changed.then(|| Sentence::Chain { terms, links: links.clone(), directed: *directed, then: then.clone() })
}

fn cancelling(cx: &Ctx, e: &Expr) -> bool {
    let parts = plain_terms(e);
    for i in 0..parts.len() {
        for j in i + 1..parts.len() {
            let (Some((_, a)), Some((_, b))) = (eval(cx.t, &parts[i].1), eval(cx.t, &parts[j].1)) else { continue };
            let mut sum = cx.quot.q(&a);
            sum.iadd_mul(&cx.quot.q(&b), &(&parts[j].0 / &parts[i].0));
            if super::chain::classes(&sum).is_empty() {
                return true;
            }
        }
    }
    false
}

fn vertex_count(cx: &Ctx, a: &Expr, b: &Expr, directed: bool) -> usize {
    let mut vs: Vec<Option<PointId>> = Vec::new();
    for (_, ti) in diff_terms(cx, a, b, directed).unwrap_or_default() {
        if !vs.contains(&ti.vertex) {
            vs.push(ti.vertex);
        }
    }
    vs.len()
}

fn plain_terms(e: &Expr) -> Vec<(Rat, Expr)> {
    match e {
        Expr::Lin { terms } => terms.iter().filter(|(_, x)| !matches!(x, Expr::Const { .. })).cloned().collect(),
        Expr::Const { .. } => Vec::new(),
        x => vec![(Rat::one(), x.clone())],
    }
}

fn expose_middle(cx: &Ctx, e1: &Expr, e2: &Expr, l: &Link, directed: bool, focus: &BTreeSet<PointId>) -> Option<(i64, Vec<Expr>, Vec<Link>)> {
    let t = cx.t;
    let (_, r1) = eval(t, e1)?;
    let (_, r2) = eval(t, e2)?;
    let (q1, q2) = (cx.quot.q(&r1), cx.quot.q(&r2));
    let delta = LinComb::combine(&q2, &q1, &Rat::from_int(-1));
    let mut dc = LinComb::zero();
    for (v, k) in delta.terms.iter() {
        if *v != crate::elimination::ANGLE_UNIT {
            dc.add_term(*v, k.clone());
        }
    }
    let fpts = link_points(l);
    let near: [&[PointId]; 1] = [&fpts];
    let mut best: Option<(i64, Vec<Expr>, Vec<Link>)> = None;
    for ts in super::chain::decomps(&dc) {
        if ts.len() != 2 || !ts.iter().all(|x| x.0.abs().is_one()) {
            if std::env::var("HP_OPAQUE").is_ok_and(|v| v == "2") {
                eprintln!("  middle: decomposition {:?} skipped", ts.iter().map(|x| x.0.clone()).collect::<Vec<_>>());
            }
            continue;
        }
        for (ia, ib) in [(0usize, 1usize), (1, 0)] {
            let mut a_comb = term_comb(&ts[ia]);
            a_comb.mul_assign_scalar(&Rat::from_int(-1));
            let b_comb = term_comb(&ts[ib]);
            let (Some(alpha), Some(beta)) = (angle_expr(cx, &a_comb, directed, &near, focus), angle_expr(cx, &b_comb, directed, &near, focus)) else {
                if std::env::var("HP_OPAQUE").is_ok_and(|v| v == "2") {
                    eprintln!("  middle: alpha/beta not displayable");
                }
                continue;
            };
            let Some((_, ra)) = eval(t, &alpha) else { continue };
            let rest = LinComb::combine(&q1, &cx.quot.q(&ra), &Rat::from_int(-1));
            let rest_terms: Vec<(Rat, Expr)> = if super::chain::classes(&rest).is_empty() {
                Vec::new()
            } else {
                match display(cx, Table::Angle, &rest, directed, &[&fpts], focus) {
                    Some(r) => plain_terms(&r),
                    None => {
                        if std::env::var("HP_OPAQUE").is_ok_and(|v| v == "2") {
                            eprintln!("  middle: rest not displayable for alpha {:?}", alpha);
                        }
                        continue;
                    }
                }
            };
            let ys: Vec<(Rat, Expr)> = rest_terms.iter().cloned().chain(plain_terms(&alpha)).collect();
            let xs: Vec<(Rat, Expr)> = rest_terms.iter().cloned().chain(plain_terms(&beta)).collect();
            let (Some(y), Some(x)) = (with_const(cx, ys, &q1, directed), with_const(cx, xs, &q2, directed)) else { continue };
            let (y, x) = (tidy(y), tidy(x));
            if y == *e1 || x == *e2 || y == x || !nice_mid(cx, &y, directed) || !nice_mid(cx, &x, directed) || cancelling(cx, &y) || cancelling(cx, &x) {
                if std::env::var("HP_OPAQUE").is_ok_and(|v| v == "2") {
                    eprintln!("  middle: y {:?} x {:?} rejected (nice {} {})", y, x, nice_mid(cx, &y, directed), nice_mid(cx, &x, directed));
                }
                continue;
            }
            let triangle = |a: &Expr, b: &Expr| matches!(identity_label(cx, a, b, directed), Some((AtomKey::ExteriorAngle | AtomKey::AngleSum, _)));
            if (vertex_count(cx, e1, &y, directed) > 2 && !triangle(e1, &y)) || (vertex_count(cx, &x, e2, directed) > 2 && !triangle(&x, e2)) {
                if std::env::var("HP_OPAQUE").is_ok_and(|v| v == "2") {
                    eprintln!("  middle: y {:?} x {:?} vertex count", y, x);
                }
                continue;
            }
            if !shows(cx, Table::Angle, &y, &q1, directed) || !shows(cx, Table::Angle, &x, &q2, directed) {
                continue;
            }
            let (Some(d1), Some(d2), Some(fd)) = (row_delta(cx, e1, &y, directed), row_delta(cx, &x, e2, directed), row_delta(cx, &y, &x, directed)) else { continue };
            if fd > 2 || d1 == 0 || d2 == 0 {
                continue;
            }
            let (Some(l1), Some(l2)) = (identity_link(cx, e1, &y, directed), identity_link(cx, &x, e2, directed)) else { continue };
            let generic = |k: &Link| matches!(k.reasons.first(), Some(Reason::Atom { key: AtomKey::AngleAddition, args, .. }) if args.is_empty());
            if !directed && (generic(&l1) || generic(&l2)) {
                continue;
            }
            let score = 10 * (d1 + d2) as i64 + badness(&y) + badness(&x) + if generic(&l1) || generic(&l2) { 25 } else { 0 };
            if best.as_ref().is_none_or(|b| score < b.0) {
                best = Some((score, vec![y, x], vec![l1, l.clone(), l2]));
            }
        }
    }
    best
}

fn split_term(cx: &Ctx, infos: &[TermInfo], alpha: &Expr, v: PointId, directed: bool, focus: &BTreeSet<PointId>, fpts: &[PointId]) -> Vec<Vec<(Rat, Expr)>> {
    let t = cx.t;
    let mut out = Vec::new();
    let Some((_, ar)) = eval(t, alpha) else { return out };
    let qa = cx.quot.q(&ar);
    for (i, ti) in infos.iter().enumerate() {
        if ti.vertex != Some(v) || !ti.coef.abs().is_one() {
            continue;
        }
        let Some((_, tr)) = eval(t, &ti.expr) else { continue };
        let mut nr = cx.quot.q(&tr);
        nr.mul_assign_scalar(&ti.coef);
        let nr = LinComb::combine(&nr, &qa, &Rat::from_int(-1));
        let cl = super::chain::classes(&nr);
        if cl.len() != 2 || !cl[0].1.abs().is_one() || cl[0].1 != -&cl[1].1 {
            continue;
        }
        let near: [&[PointId]; 2] = [fpts, &ti.pts];
        let Some(ne) = angle_expr(cx, &nr, directed, &near, focus) else { continue };
        if plain_terms(&ne).iter().any(|(_, x)| !matches!(x, Expr::Angle { b, .. } if *b == v)) {
            continue;
        }
        let mut rest: Vec<(Rat, Expr)> = infos.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, x)| (x.coef.clone(), x.expr.clone())).collect();
        rest.extend(plain_terms(&ne));
        out.push(rest);
    }
    out
}

fn expose_split(cx: &Ctx, e1: &Expr, e2: &Expr, l: &Link, directed: bool, focus: &BTreeSet<PointId>) -> Option<(i64, Vec<Expr>, Vec<Link>)> {
    let t = cx.t;
    let (_, r1) = eval(t, e1)?;
    let (_, r2) = eval(t, e2)?;
    let (q1, q2) = (cx.quot.q(&r1), cx.quot.q(&r2));
    let delta = LinComb::combine(&q2, &q1, &Rat::from_int(-1));
    let mut dc = LinComb::zero();
    for (v, k) in delta.terms.iter() {
        if *v != crate::elimination::ANGLE_UNIT {
            dc.add_term(*v, k.clone());
        }
    }
    let fpts = link_points(l);
    let near: [&[PointId]; 1] = [&fpts];
    let (i1, i2) = (term_infos(cx, e1, directed)?, term_infos(cx, e2, directed)?);
    let mut best: Option<(i64, Vec<Expr>, Vec<Link>)> = None;
    for ts in super::chain::decomps(&dc) {
        if ts.len() != 2 || !ts.iter().all(|x| x.0.abs().is_one()) {
            continue;
        }
        for (ia, ib) in [(0usize, 1usize), (1, 0)] {
            let mut a_comb = term_comb(&ts[ia]);
            a_comb.mul_assign_scalar(&Rat::from_int(-1));
            let b_comb = term_comb(&ts[ib]);
            let (Some(alpha), Some(beta)) = (angle_expr(cx, &a_comb, directed, &near, focus), angle_expr(cx, &b_comb, directed, &near, focus)) else { continue };
            let (Some((_, va, _)), Some((_, vb, _))) = (angle_rays(&alpha), angle_rays(&beta)) else { continue };
            let r1 = split_term(cx, &i1, &alpha, va, directed, focus, &fpts);
            let r2 = split_term(cx, &i2, &beta, vb, directed, focus, &fpts);
            let mut pairs: Vec<(Vec<(Rat, Expr)>, Vec<(Rat, Expr)>)> = Vec::new();
            for a in &r1 {
                for b in &r2 {
                    pairs.push((a.clone(), b.clone()));
                }
                pairs.push((a.clone(), a.clone()));
            }
            for b in &r2 {
                pairs.push((b.clone(), b.clone()));
            }
            let triangle = |a: &Expr, b: &Expr| matches!(identity_label(cx, a, b, directed), Some((AtomKey::ExteriorAngle | AtomKey::AngleSum, _)));
            for (ys, xs) in pairs {
                {
                    let ys: Vec<(Rat, Expr)> = ys.iter().cloned().chain([(Rat::one(), alpha.clone())]).collect();
                    let xs: Vec<(Rat, Expr)> = xs.iter().cloned().chain([(Rat::one(), beta.clone())]).collect();
                    let (Some(y), Some(x)) = (with_const(cx, ys, &q1, directed), with_const(cx, xs, &q2, directed)) else { continue };
                    let (y, x) = (tidy(y), tidy(x));
                    if y == *e1 || x == *e2 || y == x || !nice_mid(cx, &y, directed) || !nice_mid(cx, &x, directed) || cancelling(cx, &y) || cancelling(cx, &x) {
                        continue;
                    }
                    let (c1, c2) = (vertex_count(cx, e1, &y, directed), vertex_count(cx, &x, e2, directed));
                    if (c1 != 1 && !triangle(e1, &y)) || (c2 != 1 && !triangle(&x, e2)) {
                        continue;
                    }
                    if !shows(cx, Table::Angle, &y, &q1, directed) || !shows(cx, Table::Angle, &x, &q2, directed) {
                        continue;
                    }
                    let (Some(d1), Some(d2), Some(fd)) = (row_delta(cx, e1, &y, directed), row_delta(cx, &x, e2, directed), row_delta(cx, &y, &x, directed)) else { continue };
                    if fd > 2 || d1 == 0 || d2 == 0 {
                        continue;
                    }
                    let (Some(l1), Some(l2)) = (identity_link(cx, e1, &y, directed), identity_link(cx, &x, e2, directed)) else { continue };
                    let score = 10 * (d1 + d2) as i64 + badness(&y) + badness(&x);
                    if best.as_ref().is_none_or(|b| score < b.0) {
                        best = Some((score, vec![y, x], vec![l1, l.clone(), l2]));
                    }
                }
            }
        }
    }
    best
}

fn lead_constant(e: &Expr) -> Option<Expr> {
    let Expr::Lin { terms } = e else { return None };
    let konst = terms.iter().position(|(_, x)| matches!(x, Expr::Const { .. }))?;
    let all_negative = terms.iter().filter(|(_, x)| !matches!(x, Expr::Const { .. })).all(|(k, _)| k.is_negative());
    let positive = matches!(&terms[konst], (k, Expr::Const { degrees }) if !(k * degrees).is_negative());
    if konst == 0 || !all_negative || !positive {
        return None;
    }
    let mut out = terms.clone();
    let c = out.remove(konst);
    out.insert(0, c);
    Some(Expr::Lin { terms: out })
}

pub fn constants_first(s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    let mut changed = false;
    let out: Vec<Expr> = terms
        .iter()
        .map(|e| match lead_constant(e) {
            Some(n) => {
                changed = true;
                n
            }
            None => e.clone(),
        })
        .collect();
    changed.then(|| Sentence::Chain { terms: out, links: links.clone(), directed: *directed, then: then.clone() })
}

fn stable_names(cx: &Ctx, s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    if !terms.first().is_some_and(|e| angle_table(cx, e)) {
        return None;
    }
    let mut out = terms.clone();
    let mut changed = false;
    for k in 1..out.len().saturating_sub(1) {
        if links[k - 1].reasons.iter().any(|r| matches!(r, Reason::Atom { key: AtomKey::VerticalAngles | AtomKey::CommonAngle, .. })) {
            continue;
        }
        let d = *directed && is_directed(&out[k - 1]) && is_directed(&out[k]);
        let (Some(prev), Some(cur)) = (term_infos(cx, &out[k - 1], d), term_infos(cx, &out[k], d)) else { continue };
        let mut parts: Vec<(Rat, Expr)> = match &out[k] {
            Expr::Lin { terms } => terms.clone(),
            x => vec![(Rat::one(), x.clone())],
        };
        let mut hit = false;
        for c in &cur {
            let Some(p) = prev.iter().find(|p| p.key == c.key && p.expr != c.expr && p.coef == c.coef) else { continue };
            if let Some(slot) = parts.iter_mut().find(|(k2, x)| *x == c.expr && *k2 == c.coef) {
                slot.1 = p.expr.clone();
                hit = true;
            }
        }
        if hit {
            out[k] = tidy(Expr::Lin { terms: parts });
            changed = true;
        }
    }
    changed.then(|| Sentence::Chain { terms: out, links: links.clone(), directed: *directed, then: then.clone() })
}

fn because_chain(cx: &Ctx, blocks: &[Block], bi: usize, s: &Sentence) -> Option<Sentence> {
    let Sentence::Because { stmt: Stmt::EqAngle { lhs, rhs }, reasons, combination } = s else { return None };
    if combination.is_empty() || is_directed(lhs) || is_directed(rhs) {
        return None;
    }
    let link = oriented_link(cx, blocks, lhs, rhs, reasons, combination, false)?;
    if link_facts(&link) != 1 {
        return None;
    }
    opaque_link(cx, blocks, lhs, rhs, &link, false)?;
    let chain = Sentence::Chain { terms: vec![lhs.clone(), rhs.clone()], links: vec![link], directed: false, then: None };
    let exposed = expose_addition(cx, blocks, bi, &chain)?;
    Some(exposed)
}

fn opaque_link(cx: &Ctx, blocks: &[Block], e1: &Expr, e2: &Expr, l: &Link, directed: bool) -> Option<usize> {
    let d = diff_terms(cx, e1, e2, directed)?;
    if d.len() >= OPAQUE {
        return Some(d.len());
    }
    if d.len() < 2 {
        return None;
    }
    let mut fp = link_points(l);
    for r in &l.reasons {
        if let Reason::Claim { block, .. } | Reason::Fact { block: Some(block), .. } | Reason::Lemma { block, .. } = r {
            if let Some(b) = blocks.iter().find(|b| b.id == *block) {
                fp.extend(super::view::stmt_points(&b.stmt));
            }
        }
    }
    let ray = |v: PointId, a: PointId| -> Option<bool> {
        if fp.contains(&a) || fp.iter().any(|&p| p != v && same_ray(cx, v, a, p)) {
            return Some(false);
        }
        fp.iter().any(|&p| p != v && near_line(cx, v, a, p)).then_some(true)
    };
    let mut flips: Vec<bool> = Vec::new();
    let mut outside = false;
    for (_, ti) in &d {
        let Some(v) = ti.vertex else { continue };
        if ti.pts.len() != 3 {
            continue;
        }
        match (fp.contains(&v), ray(v, ti.pts[0]), ray(v, ti.pts[2])) {
            (true, Some(true), Some(true)) if !directed => outside = true,
            (true, Some(f0), Some(f2)) => flips.push(f0 != f2),
            _ => outside = true,
        }
    }
    let labelled = !directed && super::cyclic::cyclic_kind(cx, blocks, e1, e2, l).is_some();
    let cyclic = !directed && super::cyclic::cyclic_reason(blocks, l).is_some();
    let outside = outside || (!labelled && flips.windows(2).any(|w| w[0] != w[1])) || (cyclic && !labelled && flips.iter().any(|f| *f));
    outside.then_some(d.len())
}

const OPAQUE: usize = 3;
const EXPOSED_LINKS: usize = 12;

fn expose_addition(cx: &Ctx, blocks: &[Block], bi: usize, s: &Sentence) -> Option<Sentence> {
    let Sentence::Chain { terms, links, directed, then } = s else { return None };
    if !terms.first().is_some_and(|e| angle_table(cx, e)) || links.len() >= EXPOSED_LINKS {
        return None;
    }
    let focus: BTreeSet<PointId> = blocks[bi].points.iter().copied().collect();
    let mut cands: Vec<(usize, usize, Vec<Expr>, Vec<Link>)> = Vec::new();
    for (k, l) in links.iter().enumerate() {
        let (e1, e2) = (&terms[k], &terms[k + 1]);
        if link_facts(l) != 1 || l.reasons.iter().any(|r| matches!(r, Reason::Atom { key, .. } if key.is_identity_label())) {
            continue;
        }
        let d = *directed && is_directed(e1) && is_directed(e2);
        let Some(delta) = opaque_link(cx, blocks, e1, e2, l, d) else { continue };
        if let Some((_, x, fact_first, label)) = expose_link(cx, blocks, e1, e2, l, d, &focus) {
            let ls = if fact_first { vec![l.clone(), label] } else { vec![label, l.clone()] };
            cands.push((delta, k, vec![x], ls));
        } else if let Some((_, xs, ls)) = expose_middle(cx, e1, e2, l, d, &focus) {
            cands.push((delta, k, xs, ls));
        } else if let Some((_, xs, ls)) = expose_split(cx, e1, e2, l, d, &focus) {
            cands.push((delta, k, xs, ls));
        } else if std::env::var_os("HP_OPAQUE").is_some() {
            eprintln!("OPAQUE delta {delta} directed {d}: {e1:?} -> {e2:?}");
        }
    }
    if cands.is_empty() {
        return None;
    }
    cands.sort_by_key(|c| (std::cmp::Reverse(c.0), c.1));
    let mut room = EXPOSED_LINKS - links.len();
    cands.retain(|c| {
        let extra = c.3.len() - 1;
        if extra <= room {
            room -= extra;
            true
        } else {
            false
        }
    });
    let mut out_terms = vec![terms[0].clone()];
    let mut out_links: Vec<Link> = Vec::new();
    for (k, l) in links.iter().enumerate() {
        if let Some((_, _, xs, ls)) = cands.iter().find(|c| c.1 == k) {
            out_links.extend(ls.iter().cloned());
            out_terms.extend(xs.iter().cloned());
        } else {
            out_links.push(l.clone());
        }
        out_terms.push(terms[k + 1].clone());
    }
    Some(Sentence::Chain { terms: out_terms, links: out_links, directed: *directed, then: then.clone() })
}

pub fn lin_terms(e: &Expr) -> usize {
    match e {
        Expr::Lin { terms } => terms.iter().filter(|(_, x)| !matches!(x, Expr::Const { .. })).count(),
        _ => 1,
    }
}

pub(crate) fn link_points(l: &Link) -> Vec<PointId> {
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

pub(crate) fn link_sum(cx: &Ctx, blocks: &[Block], l: &Link, exact: bool) -> Option<LinComb> {
    let mut out = LinComb::zero();
    for (_, _, x) in term_rows(cx, blocks, l, exact)? {
        out.iadd_mul(&x, &Rat::one());
    }
    Some(out)
}

pub(crate) fn is_coll_reason(r: &Reason) -> bool {
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
        if (directed && super::chain::fractional_angle(&e)) || (!directed && super::chain::degenerate_angle(cx, &e)) {
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
        let free = |r: &Reason| is_coll_reason(r) || (tb == Table::Ratio && matches!(r, Reason::Fact { stmt: Stmt::AngleConst { degrees, .. }, .. } | Reason::Hyp { stmt: Stmt::AngleConst { degrees, .. }, .. } if *degrees == Rat::from_int(90)));
        let facts: Vec<usize> = (0..groups.len()).filter(|&i| !free(&l.reasons[groups[i].0 as usize])).collect();
        let colls: Vec<usize> = (0..groups.len()).filter(|&i| free(&l.reasons[groups[i].0 as usize])).collect();
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
        let part_pts: Vec<Option<Vec<PointId>>> = parts
            .iter()
            .map(|p| {
                let mut v: Vec<PointId> = Vec::new();
                let mut any = false;
                for &g in p {
                    let r = &l.reasons[groups[g].0 as usize];
                    if matches!(r, Reason::Atom { key, .. } if super::theorems::is_theorem(*key)) {
                        any = true;
                        super::view::reason_points_pub(r, &mut v);
                    }
                }
                any.then_some(v)
            })
            .collect();
        let mut assigned = vec![0i64; parts.len()];
        let home: Vec<usize> = extra
            .iter()
            .map(|&i| {
                let mut v: Vec<PointId> = Vec::new();
                super::view::reason_points_pub(&l.reasons[i], &mut v);
                let mut best = (i64::MIN, theorem_part);
                for (pi, pts) in part_pts.iter().enumerate() {
                    if let Some(pts) = pts {
                        let n = v.iter().filter(|x| pts.contains(x)).count() as i64;
                        let score = 10 * n - assigned[pi];
                        if n > 0 && score > best.0 {
                            best = (score, pi);
                        }
                    }
                }
                assigned[best.1] += 1;
                best.1
            })
            .collect();
        for (pi, part) in parts.iter().enumerate() {
            let mut idx: Vec<usize> = part.iter().map(|&g| groups[g].0 as usize).collect();
            idx.extend(extra.iter().zip(&home).filter(|(_, h)| **h == pi).map(|(i, _)| *i));
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

fn merge_same_reasons(s: &Sentence) -> Option<Sentence> {
    let (terms, links) = sentence_parts(s)?;
    if links.len() < 2 || !links.windows(2).any(|w| !w[0].reasons.is_empty() && w[0].reasons == w[1].reasons) {
        return None;
    }
    let mut terms = terms.to_vec();
    let mut links = links.to_vec();
    while let Some(k) = (0..links.len().saturating_sub(1)).find(|&k| !links[k].reasons.is_empty() && links[k].reasons == links[k + 1].reasons) {
        let m = merge_links(&links[k], &links[k + 1]);
        links.splice(k..k + 2, [m]);
        terms.remove(k + 1);
    }
    Some(with_parts(s, terms, links, true))
}

const PROMOTE: [AtomKey; 11] = [
    AtomKey::Miquel,
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

const ALWAYS: [AtomKey; 8] = [AtomKey::PerpBisector, AtomKey::Midline, AtomKey::Orthocentre, AtomKey::Centroid, AtomKey::MedianHypotenuse, AtomKey::EqualTangents, AtomKey::PerpBisectorLocus, AtomKey::Miquel];

fn atom_key_args(r: &Reason) -> Option<(AtomKey, Vec<PointId>)> {
    match r {
        Reason::Atom { key, args, .. } if PROMOTE.contains(key) => Some((*key, args.clone())),
        _ => None,
    }
}

fn promote_atoms(cx: &Ctx, ch: &mut Checker, hp: &mut HumanProof) {
    let mut tried: BTreeSet<(AtomKey, Vec<PointId>)> = BTreeSet::new();
    for b in hp.blocks.iter().filter(|b| b.kind != BlockKind::Raw) {
        if let [Sentence::Because { stmt, reasons, .. }] = b.body.as_slice() {
            if let ([r], true) = (reasons.as_slice(), *stmt == b.stmt) {
                if let Some(k) = atom_key_args(r) {
                    tried.insert(k);
                }
            }
        }
    }
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
        let mut justified: BTreeSet<(AtomKey, Vec<PointId>)> = BTreeSet::new();
        for b in hp.blocks.iter().filter(|b| b.kind != BlockKind::Raw) {
            for s in &b.body {
                if let Sentence::Chain { links, .. } | Sentence::Computation { links, .. } = s {
                    for l in links {
                        if !parallel_extras(l).is_empty() {
                            if let Some(Reason::Atom { key, args, .. }) = parallel_reason(l) {
                                let shared = args.len() >= 4 && (args[0] == args[2] || args[0] == args[3] || args[1] == args[2] || args[1] == args[3]);
                                if !shared {
                                    justified.insert((*key, args.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
        let pick = uses
            .iter()
            .filter(|(k, (_, n, crowded))| !tried.contains(*k) && (*n >= 2 || ALWAYS.contains(&k.0) || justified.contains(*k) || (*crowded && k.0 != AtomKey::Parallel)))
            .min_by_key(|(_, (bi, _, _))| *bi)
            .map(|(k, v)| (k.clone(), *v));
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

fn parallel_reason(l: &Link) -> Option<&Reason> {
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    let par: Vec<&Reason> = used.iter().filter_map(|&i| l.reasons.get(i as usize)).filter(|r| matches!(r, Reason::Atom { key: AtomKey::Parallel, .. })).collect();
    match par.as_slice() {
        [r] => Some(*r),
        _ => None,
    }
}

fn parallel_extras(l: &Link) -> Vec<usize> {
    let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
    if parallel_reason(l).is_none() || used.len() != 1 {
        return Vec::new();
    }
    (0..l.reasons.len()).filter(|i| !used.contains(&(*i as u16))).collect()
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
    let mut carried: Vec<Reason> = Vec::new();
    if key == AtomKey::Parallel {
        for s in &user.body {
            if let Sentence::Chain { links, .. } | Sentence::Computation { links, .. } = s {
                for l in links {
                    if parallel_reason(l).is_some_and(|r| atom_key_args(r).is_some_and(|(k, a)| k == key && a == args)) {
                        for i in parallel_extras(l) {
                            if !carried.contains(&l.reasons[i]) {
                                carried.push(l.reasons[i].clone());
                            }
                        }
                    }
                }
            }
        }
    }
    let mut from: Vec<u16> = match &atom_reason {
        Reason::Atom { from, .. } => from.clone(),
        _ => Vec::new(),
    };
    for r in &carried {
        match r {
            Reason::Lemma { block, .. } | Reason::Claim { block, .. } | Reason::Fact { block: Some(block), .. } => from.push(*block),
            Reason::Atom { from: f, .. } => from.extend(f.iter().copied()),
            _ => {}
        }
    }
    let at = from.iter().filter_map(|&f| deps_at(f)).map(|i| i + 1).max().unwrap_or(0).min(first);
    let at = (at..=first).find(|&i| hp.blocks[i].kind != BlockKind::Raw || i == first).unwrap_or(first);
    let first = at;
    let new_id = hp.blocks[first].id;
    let shift = |id: u16| if id >= new_id { id + 1 } else { id };
    let carried: Vec<Reason> = carried
        .into_iter()
        .map(|mut r| {
            match &mut r {
                Reason::Lemma { block, .. } | Reason::Claim { block, .. } | Reason::Fact { block: Some(block), .. } => *block = shift(*block),
                Reason::Atom { from, .. } => from.iter_mut().for_each(|x| *x = shift(*x)),
                _ => {}
            }
            r
        })
        .collect();
    let atom_reason = match atom_reason {
        Reason::Atom { key, stmt, args, from } => Reason::Atom { key, stmt, args, from: from.into_iter().map(shift).collect() },
        r => r,
    };
    let mut reasons = vec![atom_reason.clone()];
    reasons.extend(carried.iter().cloned());
    let body = vec![Sentence::Because { stmt: stmt.clone(), reasons, combination: vec![Term { reason: 0, row: ai as u16, coef: sign.clone() }] }];
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
            if !carried.is_empty() {
                drop_carried(s, &lemma, &carried);
            }
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

fn drop_carried(s: &mut Sentence, lemma: &Reason, carried: &[Reason]) {
    let (Sentence::Chain { links, .. } | Sentence::Computation { links, .. }) = s else { return };
    for l in links.iter_mut() {
        if !l.reasons.contains(lemma) {
            continue;
        }
        let used: BTreeSet<u16> = l.combination.iter().map(|t| t.reason).collect();
        let keep: Vec<usize> = (0..l.reasons.len()).filter(|&i| used.contains(&(i as u16)) || !carried.contains(&l.reasons[i])).collect();
        if keep.len() == l.reasons.len() {
            continue;
        }
        let map: BTreeMap<usize, u16> = keep.iter().enumerate().map(|(k, &i)| (i, k as u16)).collect();
        l.reasons = keep.iter().map(|&i| l.reasons[i].clone()).collect();
        for t in l.combination.iter_mut() {
            t.reason = map[&(t.reason as usize)];
        }
    }
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

pub fn identity_only(l: &Link) -> bool {
    l.combination.is_empty() && l.reasons.iter().all(|r| matches!(r, Reason::Atom { key, .. } if key.is_identity_label()))
}

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
                if links.iter().filter(|l| !identity_only(l)).count() <= MAX_LINKS && n <= EXPOSED_LINKS {
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
    hp.blocks.iter().any(|b| block_has_angle_chain(b, cx, directed))
}

fn block_has_angle_chain(b: &Block, cx: &Ctx, directed: bool) -> bool {
    b.body.iter().any(|s| match s {
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
            let all_used = hp.blocks.iter().all(|b| b.tag || b.kind == BlockKind::Conclusion || b.kind == BlockKind::Raw);
            if direct.len() <= PLAN_MAX && all_used {
                hp.plan = direct;
            }
        }
    }
    hp.goal = Some(goal_words(cx));
    let directed = has_angle_chain(hp, cx, true);
    let undirected = has_angle_chain(hp, cx, false);
    hp.setup.retain(|s| !matches!(s, SetupLine::DirectedAngles | SetupLine::FigureAngles | SetupLine::DirectedSteps { .. }));
    let mut conv: Vec<SetupLine> = Vec::new();
    if undirected {
        conv.push(SetupLine::FigureAngles);
    }
    if directed {
        conv.push(SetupLine::DirectedAngles);
        if undirected {
            let steps: Vec<u16> = hp.blocks.iter().filter(|b| block_has_angle_chain(b, cx, true)).map(|b| b.step).collect();
            if !steps.is_empty() {
                conv.push(SetupLine::DirectedSteps { steps });
            }
        }
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

fn dedupe_link(cx: &Ctx, blocks: &[Block], l: &Link) -> Option<Link> {
    let t = cx.t;
    let used: BTreeSet<u16> = l.combination.iter().map(|x| x.reason).collect();
    for &ri in &used {
        let r = &l.reasons[ri as usize];
        let Some(rs) = super::claims::reason_stmt(r) else { continue };
        if !matches!(r, Reason::Atom { key: AtomKey::Radii | AtomKey::Isosceles, from, .. } if from.is_empty()) {
            continue;
        }
        let Some(rrows) = check::reason_rows(t, blocks, r) else { continue };
        for (si, s2) in l.reasons.iter().enumerate() {
            if si == ri as usize || super::claims::reason_stmt(s2).is_none_or(|x| !super::claims::same_cong(x, rs)) {
                continue;
            }
            let Some(srows) = check::reason_rows(t, blocks, s2) else { continue };
            let map: Option<Vec<u16>> = l
                .combination
                .iter()
                .filter(|x| x.reason == ri)
                .map(|x| {
                    let row = &rrows.get(x.row as usize)?.1;
                    srows.iter().position(|(_, y)| y == row || *y == row.negated()).map(|k| k as u16)
                })
                .collect();
            let Some(map) = map else { continue };
            let mut it = map.into_iter();
            let mut combination: Vec<Term> = Vec::new();
            for x in &l.combination {
                if x.reason == ri {
                    let k = it.next()?;
                    let flip = srows[k as usize].1 != rrows[x.row as usize].1;
                    combination.push(Term { reason: si as u16, row: k, coef: if flip { -x.coef.clone() } else { x.coef.clone() } });
                } else {
                    combination.push(x.clone());
                }
            }
            let mut reasons = l.reasons.clone();
            reasons.remove(ri as usize);
            for x in combination.iter_mut() {
                if x.reason > ri {
                    x.reason -= 1;
                }
            }
            return Some(Link { reasons, combination });
        }
    }
    None
}

fn dedupe_restated(cx: &Ctx, blocks: &[Block], s: &Sentence) -> Option<Sentence> {
    let fix = |links: &[Link]| -> Option<Vec<Link>> {
        let mut out = links.to_vec();
        let mut changed = false;
        for l in out.iter_mut() {
            while let Some(n) = dedupe_link(cx, blocks, l) {
                *l = n;
                changed = true;
            }
        }
        changed.then_some(out)
    };
    match s {
        Sentence::Chain { terms, links, then, directed } => Some(Sentence::Chain { terms: terms.clone(), links: fix(links)?, then: then.clone(), directed: *directed }),
        Sentence::Computation { comp, terms, links } => Some(Sentence::Computation { comp: *comp, terms: terms.clone(), links: fix(links)? }),
        _ => None,
    }
}
