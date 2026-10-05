pub mod atoms;
pub mod cert;
pub mod chain;
pub mod check;
pub mod claims;
pub mod classify;
pub mod ctx;
pub mod expr;
pub mod model;
pub mod text;
pub mod trace;
pub mod view;

pub use ctx::AuxInfo;
pub use model::*;
pub use trace::EngineTrace;

use crate::predicate::{PointId, Predicate, Problem};
use crate::proof::{FactId, Reason as ER};
use claims::{Presenter, Writer};
use ctx::Ctx;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct Opts {
    pub deadline: Option<Instant>,
    pub strict: bool,
}

impl Default for Opts {
    fn default() -> Opts {
        Opts { deadline: Some(Instant::now() + Duration::from_secs(2)), strict: false }
    }
}

pub fn unavailable(raw_steps: usize) -> HumanProof {
    HumanProof {
        version: 1,
        available: false,
        setup: Vec::new(),
        blocks: Vec::new(),
        as_drawn: false,
        metrics: Metrics { raw_steps, ..Metrics::default() },
    }
}

pub fn write(trace: &EngineTrace, goal: &Predicate, deps: &[FactId], aux: &[AuxInfo], opts: &Opts) -> HumanProof {
    let start = Instant::now();
    let raw_steps = trace.closure(deps).len();
    let res = if std::env::var_os("HP_DEBUG").is_some() {
        Ok(write_inner(trace, goal, deps, aux, opts))
    } else {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::quiet_panic::quiet(|| write_inner(trace, goal, deps, aux, opts))))
    };
    let mut hp = match res {
        Ok(Some(hp)) => hp,
        _ => unavailable(raw_steps),
    };
    hp.metrics.micros = start.elapsed().as_micros() as u64;
    hp
}

fn write_inner(trace: &EngineTrace, goal: &Predicate, deps: &[FactId], aux: &[AuxInfo], opts: &Opts) -> Option<HumanProof> {
    let cx = Ctx::new(trace, goal, deps, aux, opts.deadline);
    let mut w = Writer::new(&cx);
    w.first_pass();
    w.select();
    w.recertify_with_claims();
    w.select();
    if std::env::var_os("HP_DEBUG").is_some() {
        for (k, n) in &w.nodes {
            eprintln!("node {k} kind {:?} role {:?} label {} parts {} theorem {} deps {:?}", n.kind, n.role, n.label, n.parts.as_ref().map(|p| p.len() as i64).unwrap_or(-1), n.theorem, n.deps);
        }
    }
    if w.nodes.get(&claims::GOAL).is_none_or(|g| g.parts.is_none()) {
        let mut hp = unavailable(cx.closure.len());
        hp.metrics.timed_out = cx.timed_out();
        return Some(hp);
    }
    let mut p = Presenter::new(&w);
    p.run();
    let blocks = p.blocks.clone();
    let as_drawn = p.as_drawn;
    let setup = setup_lines(&cx, &blocks);
    let mut hp = HumanProof { version: 1, available: true, setup, blocks, as_drawn, metrics: Metrics::default() };
    let violations = check::check(&cx, &mut hp, opts.strict);
    hp.metrics.check_violations = violations;
    if !hp.blocks.last().is_some_and(|b| b.kind == BlockKind::Conclusion) {
        let mut u = unavailable(cx.closure.len());
        u.metrics.check_violations = violations;
        return Some(u);
    }
    fill_metrics(&cx, &w, &mut hp);
    hp.metrics.timed_out = cx.timed_out();
    Some(hp)
}

fn helper_meaning(cx: &Ctx, p: PointId) -> HelperMeaning {
    let preds: Vec<&Predicate> = cx.hyp_pred.values().collect();
    let mut cong_centres: Vec<(PointId, PointId)> = Vec::new();
    for q in &preds {
        if q.name == "cong" && q.points.len() == 4 {
            let a = &q.points;
            let centre = if a[0] == a[2] { Some((a[0], a[1], a[3])) } else if a[1] == a[3] { Some((a[1], a[0], a[2])) } else if a[0] == a[3] { Some((a[0], a[1], a[2])) } else if a[1] == a[2] { Some((a[1], a[0], a[3])) } else { None };
            if let Some((c, x, y)) = centre {
                if c == p && x != y {
                    let on = preds.iter().any(|r| r.name == "coll" && [p, x, y].iter().all(|z| r.points.contains(z)));
                    if on {
                        return HelperMeaning::Midpoint { of: (x.min(y), x.max(y)) };
                    }
                }
                if x == p {
                    cong_centres.push((c, y));
                } else if y == p {
                    cong_centres.push((c, x));
                }
            }
        }
    }
    let mut by_src: std::collections::BTreeMap<PointId, Vec<PointId>> = std::collections::BTreeMap::new();
    for (c, s) in cong_centres {
        by_src.entry(s).or_default().push(c);
    }
    for (s, cs) in by_src {
        if cs.len() >= 2 && cs[0] != cs[1] {
            return HelperMeaning::Reflection { of: s, line: (cs[0], cs[1]) };
        }
    }
    HelperMeaning::Point
}

fn all_reasons(s: &Sentence) -> Vec<&Reason> {
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().flat_map(|l| l.reasons.iter()).collect(),
        Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => reasons.iter().collect(),
        Sentence::Raw { .. } => Vec::new(),
    }
}

fn setup_lines(cx: &Ctx, blocks: &[Block]) -> Vec<SetupLine> {
    let mut out = Vec::new();
    let directed = blocks.iter().any(|b| b.body.iter().any(|s| matches!(s, Sentence::Chain { directed: true, .. } | Sentence::Pooled { .. })));
    if directed {
        out.push(SetupLine::DirectedAngles);
    }
    let mut used: Vec<PointId> = Vec::new();
    for b in blocks {
        used.extend(b.points.iter().copied());
    }
    for (i, a) in cx.aux.iter().enumerate() {
        if used.contains(&a.point) {
            out.push(SetupLine::Aux { point: a.point, aux_index: i });
        }
    }
    let mut helpers: Vec<PointId> = used.iter().copied().filter(|&p| cx.t.name(p).starts_with('_')).collect();
    helpers.sort_unstable();
    helpers.dedup();
    for p in helpers {
        let meaning = helper_meaning(cx, p);
        out.push(SetupLine::Helper { point: p, meaning });
    }
    let _ = all_reasons;
    out
}

fn fill_metrics(cx: &Ctx, w: &Writer, hp: &mut HumanProof) {
    let m = &mut hp.metrics;
    m.raw_steps = cx.closure.len();
    m.derived = cx.closure.iter().filter(|&&f| !matches!(cx.class[f as usize], ctx::FactClass::Hyp)).count();
    m.blocks = hp.blocks.len();
    m.claims = hp.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Claim(_))).count();
    m.fallback_blocks = hp.blocks.iter().filter(|b| b.kind == BlockKind::Raw).count();
    for b in &hp.blocks {
        for s in &b.body {
            m.sentences += 1;
            match s {
                Sentence::Chain { links, terms, .. } => {
                    m.chains += 1;
                    m.chain_links += links.len();
                    if terms.iter().all(|t| !matches!(t, Expr::Lin { terms } if terms.len() > 2)) {
                        m.pure_chains += 1;
                    }
                }
                Sentence::Pooled { .. } => m.pooled += 1,
                Sentence::Computation { links, .. } => m.chain_links += links.len(),
                _ => {}
            }
            m.citations += all_reasons(s).len();
        }
    }
    m.aux_shown = hp.setup.iter().filter(|s| matches!(s, SetupLine::Aux { .. })).count();
    let mut displayed: std::collections::BTreeSet<FactId> = std::collections::BTreeSet::new();
    for b in &hp.blocks {
        displayed.extend(b.engine_facts.iter().copied());
    }
    let live = w.reachable();
    for &f in &cx.closure {
        match cx.class[f as usize] {
            ctx::FactClass::Hyp => {}
            ctx::FactClass::HypReg | ctx::FactClass::Silent => m.silent += 1,
            ctx::FactClass::TheoremReg(_) | ctx::FactClass::MergeReg(_) => m.silent += 1,
            _ => {
                let raw = hp.blocks.iter().any(|b| b.kind == BlockKind::Raw && b.engine_facts.contains(&f));
                let node = w.nodes.get(&f);
                if raw {
                    m.fallback_facts += 1;
                } else if node.is_some_and(|n| n.theorem) && live.contains(&f) {
                    m.theorem += 1;
                } else if displayed.contains(&f) || (live.contains(&f) && node.is_some()) {
                    m.reproved += 1;
                } else {
                    m.pruned += 1;
                }
            }
        }
    }
    let aux_desc: Vec<(PointId, String)> = cx.aux.iter().map(|a| (a.point, a.desc.clone())).collect();
    let r = text::render(cx.t, hp, &aux_desc, &|f| text::engine_line(cx.t, f));
    let body: Vec<&String> = r.lines.iter().filter(|l| !l.trim().is_empty()).collect();
    hp.metrics.lines_en = body.iter().map(|l| l.lines().filter(|x| !x.trim().is_empty()).count()).sum();
    hp.metrics.words_en = body.iter().map(|l| l.split_whitespace().count()).sum();
    let m = &mut hp.metrics;
    m.human_cost = 10 * m.claims + 3 * m.sentences + m.chain_links + 2 * hp.blocks.iter().flat_map(|b| b.body.iter()).filter_map(|s| if let Sentence::Pooled { reasons, .. } = s { Some(reasons.len()) } else { None }).sum::<usize>()
        + 6 * (m.chains - m.pure_chains)
        + 25 * m.fallback_blocks
        + 4 * m.aux_shown
        + m.words_en.div_ceil(20);
}

pub fn render_en(trace: &EngineTrace, hp: &HumanProof, aux: &[AuxInfo]) -> String {
    if !hp.available {
        return String::from("(no human proof: the writer could not re-prove the goal; see the full derivation)\n");
    }
    let aux_desc: Vec<(PointId, String)> = aux.iter().map(|a| (a.point, a.desc.clone())).collect();
    let r = text::render(trace, hp, &aux_desc, &|f| text::engine_line(trace, f));
    let mut out = String::new();
    for l in r.lines {
        out.push_str(&l);
        out.push('\n');
    }
    out
}

pub fn aux_infos(problem: &Problem, first_aux: usize, descs: &[String]) -> Vec<AuxInfo> {
    let mut out = Vec::new();
    for (k, d) in descs.iter().enumerate() {
        let (name, desc) = match d.split_once(" = ") {
            Some((a, b)) => (a.trim().to_string(), b.trim().to_string()),
            None => (String::new(), d.clone()),
        };
        let point = problem
            .points
            .iter()
            .enumerate()
            .skip(first_aux)
            .find(|(_, p)| p.name == name)
            .map(|(i, _)| i as PointId)
            .unwrap_or((first_aux + k) as PointId);
        out.push(AuxInfo { point, name, desc });
    }
    out
}

pub fn for_problem(problem: &Problem, aux: &[AuxInfo], opts: &Opts) -> Option<(HumanProof, EngineTrace, Vec<FactId>, String)> {
    let (proof, trace, deps) = crate::runner::solve_problem_with_trace(problem).ok()??;
    let goal = problem.goal.clone()?;
    let hp = write(&trace, &goal, &deps, aux, opts);
    Some((hp, trace, deps, proof))
}

pub fn is_hyp_reason(r: &ER) -> bool {
    matches!(r, ER::Assumption(_) | ER::Construction(_))
}
