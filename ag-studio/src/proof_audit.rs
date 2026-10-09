use std::sync::Mutex;
use std::time::Duration;

use ddar::human::{Expr, HumanProof, Reason, Sentence, SetupLine, Stmt};
use ddar::{Predicate, Problem};

use crate::engine::{solve_within, InputKind, Solution, SolveOptions};

pub struct Case {
    pub name: String,
    pub input: String,
    pub kind: InputKind,
    pub limit: Duration,
}

pub fn app_examples() -> Vec<Case> {
    let js = include_str!("../assets/app.js");
    let mut out = Vec::new();
    let mut rest = js;
    while let Some(i) = rest.find("{ id: \"") {
        rest = &rest[i + 7..];
        let id = rest.split('"').next().unwrap_or_default().to_string();
        let Some(b) = rest.find("body: \"") else {
            break;
        };
        let body_start = &rest[b + 6..];
        let mut end = 1;
        let bytes = body_start.as_bytes();
        while end < bytes.len() && bytes[end] != b'"' {
            end += if bytes[end] == b'\\' { 2 } else { 1 };
        }
        let body: String =
            serde_json::from_str(&body_start[..=end]).expect("example body is a JSON string");
        out.push(Case {
            name: format!("example {id}"),
            input: body,
            kind: InputKind::Geo,
            limit: Duration::from_secs(60),
        });
        rest = &body_start[end..];
    }
    out
}

pub fn fixtures() -> Vec<Case> {
    crate::human_view::tests::ALL
        .iter()
        .map(|n| Case {
            name: format!("fixture {n}"),
            input: crate::human_view::tests::load(n).program,
            kind: InputKind::Geo,
            limit: Duration::from_secs(60),
        })
        .collect()
}

pub fn corpus(file: &str, limit: Duration) -> Vec<Case> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus")
        .join(file);
    ddar::corpus::read_corpus(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .into_iter()
        .map(|(name, body)| Case {
            name: format!("{file} {name}"),
            input: body,
            kind: InputKind::Corpus,
            limit,
        })
        .collect()
}

pub fn solve_all(cases: &[Case], threads: usize) -> Vec<(usize, Result<Solution, String>)> {
    let next = Mutex::new(0usize);
    let out = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = {
                    let mut n = next.lock().unwrap();
                    let i = *n;
                    *n += 1;
                    i
                };
                let Some(c) = cases.get(i) else { break };
                let opts = SolveOptions {
                    kind: c.kind,
                    ..SolveOptions::default()
                };
                let r = solve_within(&c.input, &opts, Some(c.limit));
                out.lock().unwrap().push((i, r));
            });
        }
    });
    let mut v = out.into_inner().unwrap();
    v.sort_by_key(|x| x.0);
    v
}

pub struct Shown {
    pub sol: Solution,
    pub view: crate::present::View,
}

pub fn shown(sol: &Solution) -> Shown {
    let d = crate::present::displayed(sol, None);
    let redrawn = d.is_some();
    let sol = d.unwrap_or_else(|| sol.clone());
    let view = crate::present::build_shown(&sol, redrawn);
    Shown { sol, view }
}

pub fn drawn_badly(sol: &Solution) -> Option<Solution> {
    let fig = sol.figure.as_ref()?;
    let goal = fig.problem.goal.as_ref()?;
    let instances = ddar::geo::build_instances(&sol.input, 64).ok()?;
    for inst in instances {
        let at: std::collections::HashMap<&str, ddar::numerics::Vec2> =
            inst.iter().map(|(n, v)| (n.as_str(), *v)).collect();
        let mut p = fig.problem.clone();
        let placed = p
            .points
            .iter_mut()
            .all(|q| at.get(q.name.as_str()).map(|v| q.value = *v).is_some());
        if !placed
            || p.preds
                .iter()
                .chain(std::iter::once(goal))
                .any(|pr| ddar::geo::numeric_holds(&p, pr) != Some(true))
        {
            continue;
        }
        let Some(redrawn) =
            crate::spread::respread(&p, fig.aux_from, &sol.input, crate::spread::Goal::Holds)
        else {
            continue;
        };
        let opts = ddar::human::Opts {
            deadline: None,
            strict: false,
        };
        let Some((hp, trace, deps, proof)) = ddar::human::for_problem(&p, &[], &opts) else {
            continue;
        };
        if !hp.available || angle_violations(&hp, &trace.names, &redrawn).is_empty() {
            continue;
        }
        let mut out = sol.clone();
        out.proof_steps = Some(crate::engine::proof_size(&proof).0);
        out.proof = Some(proof);
        out.human = Some(crate::engine::HumanSource {
            closure: trace.closure(&deps),
            names: trace.names.clone(),
            proof: hp,
        });
        out.figure = Some(crate::engine::FigureSource {
            problem: p,
            aux_from: fig.aux_from,
        });
        return Some(out);
    }
    None
}

fn xy(fig: &Problem, names: &[String], id: u32) -> Option<(f64, f64)> {
    let n = names.get(id as usize)?;
    fig.points
        .iter()
        .find(|p| &p.name == n)
        .map(|p| (p.value.x, p.value.y))
}

fn dir(p: (f64, f64), q: (f64, f64)) -> f64 {
    (q.1 - p.1).atan2(q.0 - p.0).to_degrees()
}

fn lcm(a: i64, b: i64) -> i64 {
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    a / x * b
}

fn eval(e: &Expr, at: &dyn Fn(u32) -> Option<(f64, f64)>) -> Option<(f64, i64)> {
    match e {
        Expr::Angle { a, b, c, directed } => {
            let (pa, pb, pc) = (at(*a)?, at(*b)?, at(*c)?);
            if *directed {
                Some(((dir(pb, pc) - dir(pb, pa)).rem_euclid(180.0), 1))
            } else {
                let (u, w) = ((pa.0 - pb.0, pa.1 - pb.1), (pc.0 - pb.0, pc.1 - pb.1));
                Some((
                    (u.0 * w.1 - u.1 * w.0)
                        .abs()
                        .atan2(u.0 * w.0 + u.1 * w.1)
                        .to_degrees(),
                    0,
                ))
            }
        }
        Expr::LineAngle { l1, l2, .. } => Some((
            (dir(at(l2.0)?, at(l2.1)?) - dir(at(l1.0)?, at(l1.1)?)).rem_euclid(180.0),
            1,
        )),
        Expr::Const { degrees } => Some((degrees.to_f64(), 0)),
        Expr::Lin { terms } => {
            let mut v = 0.0;
            let mut div = 0;
            for (k, x) in terms {
                let (xv, xd) = eval(x, at)?;
                v += k.to_f64() * xv;
                if xd > 0 {
                    let d = xd * k.denom_i64().unwrap_or(1).abs();
                    div = if div == 0 { d } else { lcm(div, d) };
                }
            }
            Some((v, div))
        }
        _ => None,
    }
}

fn holds(l: (f64, i64), r: (f64, i64)) -> bool {
    const TOL: f64 = 1e-4;
    let div = match (l.1, r.1) {
        (0, 0) => return (l.0 - r.0).abs() < TOL,
        (0, d) | (d, 0) => d,
        (a, b) => lcm(a, b),
    };
    let m = 180.0 / div as f64;
    let d = (l.0 - r.0).rem_euclid(m);
    d < TOL || d > m - TOL
}

fn stmts_of_reason<'a>(r: &'a Reason, out: &mut Vec<&'a Stmt>) {
    match r {
        Reason::Hyp { stmt, .. } | Reason::Atom { stmt, .. } | Reason::Lemma { stmt, .. } => {
            out.push(stmt)
        }
        Reason::Fact { stmt, because, .. } => {
            out.push(stmt);
            for b in because {
                stmts_of_reason(b, out);
            }
        }
        Reason::Claim { .. } | Reason::Engine { .. } => {}
    }
}

fn all_stmts(hp: &HumanProof) -> Vec<&Stmt> {
    let mut stmts: Vec<&Stmt> = Vec::new();
    for b in &hp.blocks {
        stmts.push(&b.stmt);
        for s in &b.body {
            match s {
                Sentence::Chain { links, then, .. } => {
                    stmts.extend(then.iter());
                    for r in links.iter().flat_map(|l| &l.reasons) {
                        stmts_of_reason(r, &mut stmts);
                    }
                }
                Sentence::Computation { links, .. } => {
                    for r in links.iter().flat_map(|l| &l.reasons) {
                        stmts_of_reason(r, &mut stmts);
                    }
                }
                Sentence::Because { stmt, reasons, .. }
                | Sentence::Pooled { stmt, reasons, .. }
                | Sentence::Theorem { stmt, reasons, .. } => {
                    stmts.push(stmt);
                    for r in reasons {
                        stmts_of_reason(r, &mut stmts);
                    }
                }
                Sentence::Raw { .. } => {}
            }
        }
    }
    stmts
}

fn interior(fig: &Problem, names: &[String], p: u32, v: u32, q: u32) -> Option<f64> {
    let (a, o, b) = (xy(fig, names, p)?, xy(fig, names, v)?, xy(fig, names, q)?);
    let (u, w) = ((a.0 - o.0, a.1 - o.1), (b.0 - o.0, b.1 - o.1));
    let n = (u.0 * u.0 + u.1 * u.1).sqrt() * (w.0 * w.0 + w.1 * w.1).sqrt();
    (n > 1e-12).then(|| {
        ((u.0 * w.0 + u.1 * w.1) / n)
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    })
}

pub fn bisector_violations(hp: &HumanProof, names: &[String], fig: &Problem) -> Vec<String> {
    let mut bad = Vec::new();
    let name = |id: u32| names.get(id as usize).cloned().unwrap_or_default();
    for st in all_stmts(hp) {
        let (
            Some(shape),
            Stmt::EqAngle {
                lhs: Expr::Angle { a, b: v, c, .. },
                rhs: Expr::Angle { c: c2, .. },
            },
        ) = (ddar::human::barem::bisector_shape(st), st)
        else {
            continue;
        };
        let (Some(x), Some(y)) = (
            interior(fig, names, *a, *v, *c),
            interior(fig, names, *c, *v, *c2),
        ) else {
            continue;
        };
        let equal = (x - y).abs() < 1e-4;
        let worded = match shape {
            (false, false) if !equal => "bisects",
            (true, true) if equal => "is the external bisector of",
            _ => continue,
        };
        bad.push(format!(
            "{}{} {worded} ∠{}{}{} but the figure reads {x:.2}° and {y:.2}°",
            name(*v),
            name(*c),
            name(*a),
            name(*v),
            name(*c2)
        ));
    }
    if let Some(ddar::human::GoalWords::Bisects { line, angle }) = &hp.goal {
        if let (Some(x), Some(y)) = (
            interior(fig, names, angle.0, line.0, line.1),
            interior(fig, names, line.1, line.0, angle.2),
        ) {
            if (x - y).abs() >= 1e-4 {
                bad.push(format!(
                    "the conclusion says {}{} bisects ∠{}{}{} but the figure reads {x:.2}° and {y:.2}°",
                    name(line.0),
                    name(line.1),
                    name(angle.0),
                    name(angle.1),
                    name(angle.2)
                ));
            }
        }
    }
    bad
}

pub fn angle_violations(hp: &HumanProof, names: &[String], fig: &Problem) -> Vec<String> {
    let at = |id: u32| xy(fig, names, id);
    let show = |e: &Expr| format!("{e:?}");
    let mut bad = Vec::new();
    let mut check = |l: &Expr, r: &Expr, what: &str| {
        if let (Some(a), Some(b)) = (eval(l, &at), eval(r, &at)) {
            if !holds(a, b) {
                bad.push(format!(
                    "{what}: {} = {} reads {:.3}° vs {:.3}°",
                    show(l),
                    show(r),
                    a.0,
                    b.0
                ));
            }
        }
    };
    let mut stmts: Vec<&Stmt> = Vec::new();
    let mut chains: Vec<&Vec<Expr>> = Vec::new();
    for b in &hp.blocks {
        stmts.push(&b.stmt);
        for s in &b.body {
            match s {
                Sentence::Chain {
                    terms, links, then, ..
                } => {
                    chains.push(terms);
                    stmts.extend(then.iter());
                    for l in links {
                        for r in &l.reasons {
                            stmts_of_reason(r, &mut stmts);
                        }
                    }
                }
                Sentence::Computation { terms, links, .. } => {
                    chains.push(terms);
                    for l in links {
                        for r in &l.reasons {
                            stmts_of_reason(r, &mut stmts);
                        }
                    }
                }
                Sentence::Because { stmt, reasons, .. }
                | Sentence::Pooled { stmt, reasons, .. }
                | Sentence::Theorem { stmt, reasons, .. } => {
                    stmts.push(stmt);
                    for r in reasons {
                        stmts_of_reason(r, &mut stmts);
                    }
                }
                Sentence::Raw { .. } => {}
            }
        }
    }
    for terms in chains {
        for w in terms.windows(2) {
            check(&w[0], &w[1], "chain link");
        }
    }
    for s in stmts {
        match s {
            Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs } => check(lhs, rhs, "statement"),
            Stmt::AngleConst { angle, degrees } => check(
                angle,
                &Expr::Const {
                    degrees: degrees.clone(),
                },
                "statement",
            ),
            _ => {}
        }
    }
    bad
}

pub fn setup_violations(
    hp: &HumanProof,
    names: &[String],
    fig: &Problem,
    goal: &Predicate,
) -> Vec<String> {
    let name_of = |id: u32| names.get(id as usize).cloned().unwrap_or_default();
    let goal_names: Vec<String> = goal
        .points
        .iter()
        .map(|&p| fig.point_name(p).to_string())
        .collect();
    let has =
        |set: &[u32], want: &[String]| want.iter().all(|w| set.iter().any(|&p| &name_of(p) == w));
    let mut bad = Vec::new();
    for s in &hp.setup {
        let SetupLine::Circle {
            name,
            through,
            centre,
            diameter,
        } = s
        else {
            continue;
        };
        let line = format!(
            "{name} through {:?} centre {:?} diameter {:?}",
            through.iter().map(|&p| name_of(p)).collect::<Vec<_>>(),
            centre.map(name_of),
            diameter.map(|d| (name_of(d.0), name_of(d.1)))
        );
        match goal.name.as_str() {
            "cyclic" if has(through, &goal_names) => bad.push(format!(
                "the setup line {line} states the goal concyclicity"
            )),
            "cong" => {
                let g = &goal_names;
                let pairs = [(0, 1, 2, 3), (0, 1, 3, 2), (1, 0, 2, 3), (1, 0, 3, 2)];
                for (o1, x, o2, y) in pairs {
                    if g[o1] == g[o2]
                        && centre.is_some_and(|c| name_of(c) == g[o1])
                        && has(through, &[g[x].clone(), g[y].clone()])
                    {
                        bad.push(format!(
                            "the setup line {line} states the goal {}{} = {}{}",
                            g[o1], g[x], g[o2], g[y]
                        ));
                    }
                }
            }
            "perp" => {
                let g = &goal_names;
                let (l1, l2) = ((&g[0], &g[1]), (&g[2], &g[3]));
                let vertex = [l1.0, l1.1].into_iter().find(|p| *p == l2.0 || *p == l2.1);
                if let (Some(v), Some(d)) = (vertex, diameter) {
                    let other =
                        |l: (&String, &String)| if l.0 == v { l.1.clone() } else { l.0.clone() };
                    let ends = [other(l1), other(l2)];
                    let dia = [name_of(d.0), name_of(d.1)];
                    if has(through, std::slice::from_ref(v)) && dia.iter().all(|x| ends.contains(x))
                    {
                        bad.push(format!(
                            "the setup line {line} states the goal right angle at {v}"
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    bad
}

type Pt = (f64, f64);

pub fn svg_draws(svg: &str, fig: &Problem) -> Result<usize, String> {
    let names = crate::present::Names::build(fig);
    let mut pairs: Vec<(Pt, Pt, String)> = Vec::new();
    for p in &fig.points {
        let shown = names.get(&p.name);
        let key = format!("data-p=\"{shown}\"");
        let Some(l) = svg
            .lines()
            .find(|l| l.contains("f-dot") && l.contains(&key))
        else {
            continue;
        };
        let num = |k: &str| -> Option<f64> {
            let i = l.find(&format!(" {k}=\""))? + k.len() + 3;
            l[i..].split('"').next()?.parse().ok()
        };
        if let (Some(x), Some(y)) = (num("cx"), num("cy")) {
            pairs.push(((p.value.x, p.value.y), (x, y), shown));
        }
    }
    if pairs.len() < 3 {
        return Ok(pairs.len());
    }
    let d = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let mut far = (0, 1, 0.0);
    for i in 0..pairs.len() {
        for j in i + 1..pairs.len() {
            let x = d(pairs[i].1, pairs[j].1);
            if x > far.2 {
                far = (i, j, x);
            }
        }
    }
    let scale = far.2 / d(pairs[far.0].0, pairs[far.1].0);
    for i in 0..pairs.len() {
        for j in i + 1..pairs.len() {
            let want = d(pairs[i].0, pairs[j].0) * scale;
            let got = d(pairs[i].1, pairs[j].1);
            if (want - got).abs() > 0.5 + 0.01 * want {
                return Err(format!(
                    "{}{} is drawn {got:.2} long, the figure says {want:.2}",
                    pairs[i].2, pairs[j].2
                ));
            }
        }
    }
    Ok(pairs.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(case: &Case, sol: &Solution) -> Option<Vec<String>> {
        if !sol.proved {
            return None;
        }
        sol.human.as_ref()?;
        let sh = shown(sol);
        let h = sh.sol.human.as_ref()?;
        let fig = &sh
            .sol
            .figure
            .as_ref()
            .expect("a proved solution has a figure")
            .problem;
        let goal = fig.goal.as_ref()?;
        let mut bad: Vec<String> = Vec::new();
        bad.extend(angle_violations(&h.proof, &h.names, fig));
        bad.extend(bisector_violations(&h.proof, &h.names, fig));
        bad.extend(setup_violations(&h.proof, &h.names, fig, goal));
        if let Err(e) = svg_draws(&sh.view.svg, fig) {
            bad.push(format!(
                "the page's figure is not the figure the proof reads: {e}"
            ));
        }
        Some(
            bad.into_iter()
                .map(|b| format!("{}: {b}", case.name))
                .collect(),
        )
    }

    #[test]
    fn the_audit_rejects_a_false_figure_angle_and_a_setup_line_that_states_the_goal() {
        let src = "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))";
        let sol = crate::engine::solve(src, &SolveOptions::default()).unwrap();
        let sh = shown(&sol);
        let h = sh.sol.human.as_ref().expect("a human proof");
        let fig = &sh.sol.figure.as_ref().unwrap().problem;
        let goal = fig.goal.as_ref().unwrap();
        assert!(angle_violations(&h.proof, &h.names, fig).is_empty());
        assert!(setup_violations(&h.proof, &h.names, fig, goal).is_empty());
        assert!(svg_draws(&sh.view.svg, fig).unwrap() >= 4);

        let bad = drawn_badly(&sol).expect("an instance of this program that the page redraws");
        let engine = bad.figure.as_ref().unwrap();
        let respread = crate::spread::respread(
            &engine.problem,
            engine.aux_from,
            &sol.input,
            crate::spread::Goal::Holds,
        )
        .expect("the badly drawn figure is redrawn");
        let engine_proof = bad.human.as_ref().unwrap();
        assert!(
            angle_violations(&engine_proof.proof, &engine_proof.names, &engine.problem).is_empty()
        );
        assert!(
            !angle_violations(&engine_proof.proof, &engine_proof.names, &respread).is_empty(),
            "the proof written on the engine's figure misreads the redrawn one (∠C = ∠HBC − 90°)"
        );
        let redrawn = shown(&bad);
        let rh = redrawn.sol.human.as_ref().unwrap();
        let rfig = &redrawn.sol.figure.as_ref().unwrap().problem;
        assert!(
            angle_violations(&rh.proof, &rh.names, rfig).is_empty(),
            "the page rewrites the proof for the figure it redraws"
        );
        assert!(svg_draws(&redrawn.view.svg, rfig).is_ok());

        let mut flipped = h.proof.clone();
        let chain = flipped
            .blocks
            .iter_mut()
            .flat_map(|b| b.body.iter_mut())
            .find_map(|s| match s {
                Sentence::Chain {
                    terms,
                    directed: false,
                    ..
                } if terms.len() >= 2 => Some(terms),
                _ => None,
            })
            .expect("an undirected chain");
        let second = chain[1].clone();
        chain[1] = Expr::Lin {
            terms: vec![(ddar::rational::Rat::from_int(-1), second)],
        };
        let bad = angle_violations(&flipped, &h.names, fig);
        assert!(
            !bad.is_empty(),
            "a chain link that is false in the drawn figure must be reported"
        );

        let mut stating = h.proof.clone();
        stating.setup.push(SetupLine::Circle {
            name: "ω".into(),
            through: goal.points.clone(),
            centre: None,
            diameter: None,
        });
        assert!(
            !setup_violations(&stating, &h.names, fig, goal).is_empty(),
            "a circle line through all four goal points states the goal"
        );

        let mut moved = fig.clone();
        moved.points[0].value.x += 0.3;
        assert!(
            svg_draws(&sh.view.svg, &moved).is_err(),
            "a figure other than the drawn one must not pass"
        );
    }

    #[test]
    fn an_external_bisector_is_never_said_to_bisect() {
        for (name, src, must) in [
            (
                "excenter_bisector",
                "A B C = triangle\nJ = excenter(A, B, C)\nprove eqangle(B, A, B, J, B, J, B, C)",
                "BJ is the external bisector of \u{2220}ABC",
            ),
            (
                "arc_midpoint_bisector",
                include_str!("../../alphageometry-rs/examples/named/arc_midpoint_bisector.geo"),
                "\u{2221}BAM = \u{2221}MAC",
            ),
            (
                "incenter_excenter_cyclic",
                include_str!("../../alphageometry-rs/examples/named/incenter_excenter_cyclic.geo"),
                "is the external bisector of",
            ),
        ] {
            let sol = crate::engine::solve(src, &SolveOptions::default()).unwrap();
            assert!(sol.proved, "{name}: {}", sol.note);
            let sh = shown(&sol);
            let h = sh
                .sol
                .human
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: a human proof"));
            let fig = &sh.sol.figure.as_ref().unwrap().problem;
            let bad = bisector_violations(&h.proof, &h.names, fig);
            assert!(bad.is_empty(), "{name}: {bad:?}");
            let v = crate::present::solution_json(&sh.sol, Some(name));
            let hv = crate::human_view::of_view(&v["view"])
                .unwrap_or_else(|| panic!("{name}: a human view"));
            for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Ro] {
                let text = crate::human_view::text(&hv, &v["view"], lang);
                if lang == crate::i18n::Lang::En {
                    assert!(text.contains(must), "{name}: {text}");
                }
                for wrong in [
                    "BJ bisects",
                    "AM bisects",
                    "BI\u{2090} bisects",
                    "J este bisectoarea \u{2220}",
                ] {
                    assert!(!text.contains(wrong), "{name} {lang:?}: {text}");
                }
            }
        }
        let sol = crate::engine::solve(
            "A B C = triangle\nJ = excenter(A, B, C)\nprove eqangle(B, A, B, J, B, J, B, C)",
            &SolveOptions::default(),
        )
        .unwrap();
        let sh = shown(&sol);
        let h = sh.sol.human.as_ref().unwrap();
        let fig = &sh.sol.figure.as_ref().unwrap().problem;
        let mut forged = h.proof.clone();
        forged.goal = Some(ddar::human::GoalWords::Bisects {
            line: (1, 3),
            angle: (0, 1, 2),
        });
        let ids: Vec<&str> = h.names.iter().map(String::as_str).collect();
        assert_eq!(&ids[..4], ["A", "B", "C", "J"], "{ids:?}");
        assert!(
            !bisector_violations(&forged, &h.names, fig).is_empty(),
            "the audit catches a conclusion that calls an external bisector a bisector"
        );
    }

    #[test]
    fn every_shown_proof_holds_in_the_shown_figure_and_never_assumes_its_goal() {
        let _heavy = crate::engine::heavy_search_lock();
        let mut cases = app_examples();
        let examples = cases.len();
        cases.extend(fixtures());
        let imo_from = cases.len();
        cases.extend(corpus("imo_ag_30.txt", Duration::from_secs(90)));
        let jgex_from = cases.len();
        cases.extend(corpus("jgex_ag_231.txt", Duration::from_secs(20)));
        assert!(examples >= 10, "app.js examples not found: {examples}");
        assert_eq!(cases.len() - imo_from, 261);
        let results = solve_all(&cases, 4);
        let (mut checked, mut imo, mut jgex) = (0usize, 0usize, 0usize);
        let mut bad: Vec<String> = Vec::new();
        for (i, r) in &results {
            let Ok(sol) = r else { continue };
            let Some(v) = audit(&cases[*i], sol) else {
                continue;
            };
            checked += 1;
            if *i >= jgex_from {
                jgex += 1;
            } else if *i >= imo_from {
                imo += 1;
            }
            bad.extend(v);
        }
        eprintln!("audited {checked} shown proofs ({imo} IMO, {jgex} JGEX)");
        assert!(
            imo >= 22 && jgex >= 190,
            "too few proofs audited: {imo} IMO, {jgex} JGEX"
        );
        assert!(
            bad.is_empty(),
            "{} problems:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }
}
