//! Thin, in-process wrapper over the `ddar` engine.
//!
//! Turns a problem — a high-level `.geo` program, a low-level AlphaGeometry
//! string, or a length/metric program — into a numbered proof plus an SVG
//! figure. Three solver paths, chosen automatically:
//!
//!   * **relational goals** (coll, perp, cyclic, cong, eqangle, products/ratios
//!     of lengths, …) → the DDAR deductive closure, falling back to the
//!     universal auxiliary-point search;
//!   * **absolute-length goals** (specific lengths, sums of squares such as
//!     `AC² + BD² = 144`) → the classical *Euclidean* prover, which writes a
//!     numbered proof citing named theorems (Pythagoras, Thales, …);
//!   * **low-level** problems → DDAR directly.
//!
//! This is the single entry point shared by the `render` CLI, the web server,
//! and the MCP server.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use ddar::aux_search::{
    apply_constructions, candidates, solve_max, solve_with_aux, Construction, WarmBase,
};
use ddar::geo;
use ddar::runner::{solve_problem, solve_problem_with_proof};
use ddar::svg::{render_with, FigureOptions, Theme};
use ddar::Problem;

/// How to interpret the input program text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    /// High-level `.geo` construction language (compiled to a `Problem`).
    Geo,
    /// Low-level AlphaGeometry problem string (`a@x_y b@... = preds ? goal`).
    LowLevel,
}

impl InputKind {
    /// Guess the input kind: low-level problems pin every point with an `@`
    /// coordinate; `.geo` programs never do.
    pub fn detect(input: &str) -> InputKind {
        if input.contains('@') {
            InputKind::LowLevel
        } else {
            InputKind::Geo
        }
    }
}

/// Options controlling a solve.
#[derive(Clone)]
pub struct SolveOptions {
    pub kind: InputKind,
    pub theme: Theme,
    /// Extract and return the numbered proof (a few % slower than a yes/no solve).
    pub want_proof: bool,
    /// Title drawn atop the figure's construction panel.
    pub title: Option<String>,
    /// Bake the construction/goal side panel into the SVG. The web UI turns
    /// this off and lays the legend out in HTML instead (responsive, selectable);
    /// exports and the CLI keep it for a self-contained document.
    pub panel: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        SolveOptions {
            kind: InputKind::Geo,
            theme: Theme::Dark,
            want_proof: true,
            title: None,
            panel: true,
        }
    }
}

/// Which prover produced the result.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// DDAR deductive closure (no auxiliary point).
    Ddar,
    /// DDAR plus the auxiliary-point search.
    AuxSearch,
    /// The classical Euclidean length prover.
    Euclidean,
}

/// The full outcome of a solve: proof, figure, and metadata.
#[derive(Clone, serde::Serialize)]
pub struct Solution {
    /// The original input program.
    pub input: String,
    /// The compiled low-level AlphaGeometry form (coordinates + predicates).
    pub low_level: String,
    /// Whether the goal was proved.
    pub proved: bool,
    /// Which prover path was taken.
    pub method: Method,
    /// The numbered, citation-annotated proof (when `want_proof` and proved).
    pub proof: Option<String>,
    /// The figure as a standalone SVG document.
    pub svg: String,
    /// Human-readable auxiliary constructions the search introduced, if any.
    pub aux_constructions: Vec<String>,
    /// The hypotheses in natural notation (the figure legend, as text).
    pub constructions: Vec<String>,
    /// The goal in natural notation.
    pub goal: Option<String>,
    /// `Some(false)` if the goal does not hold in the sampled figure (the
    /// statement appears to be false); `Some(true)` if it holds; `None` if not
    /// numerically checkable.
    pub goal_holds_numerically: Option<bool>,
    /// Wall-clock solve time in seconds.
    pub elapsed_secs: f64,
    /// A short status note (e.g. how many auxiliary points / DDAR runs).
    pub note: String,
    /// Number of proof steps in the returned proof, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_steps: Option<usize>,
    /// How many distinct working solutions the elegant search compared
    /// (`solve_best` only; `None` for a plain solve).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub examined: Option<usize>,
}

/// Solve one problem end to end. Never panics: the engine's degenerate-figure
/// panics (which the aux search normally catches internally) are contained
/// here too, so a bad input yields an `Err`, not a crash.
pub fn solve(input: &str, opts: &SolveOptions) -> Result<Solution, String> {
    match opts.kind {
        InputKind::LowLevel => {
            let problem = Problem::parse(input).map_err(|e| format!("parse error: {e}"))?;
            deductive_flow(problem, input, None, opts)
        }
        InputKind::Geo => {
            // Absolute-length / sum-of-squares goals get the classical Euclidean
            // prover; everything else goes through the DDAR closure (which also
            // handles product/ratio-of-length goals like power of a point).
            if extract_goal(input).map(|g| is_metric_goal(&g)).unwrap_or(false) {
                euclidean_flow(input, opts)
            } else {
                let compiled =
                    geo::compile(input).map_err(|e| format!("compile error: {e}"))?;
                deductive_flow(
                    compiled.problem,
                    input,
                    compiled.goal_numerically_holds,
                    opts,
                )
            }
        }
    }
}

/// Compile a high-level `.geo` program, or parse a low-level problem, into a
/// `Problem` plus any numeric goal check from the compiler.
fn to_problem(input: &str, kind: InputKind) -> Result<(Problem, Option<bool>), String> {
    match kind {
        InputKind::Geo => {
            let compiled = geo::compile(input).map_err(|e| format!("compile error: {e}"))?;
            Ok((compiled.problem, compiled.goal_numerically_holds))
        }
        InputKind::LowLevel => {
            let p = Problem::parse(input).map_err(|e| format!("parse error: {e}"))?;
            Ok((p, None))
        }
    }
}

/// Search, within a wall-clock `budget`, for the **shortest** proof — the one
/// with the fewest steps, *regardless of how many auxiliary constructions it
/// uses* (auxiliary count is not a factor).
///
/// The default solver returns the *first* (highest-ranked) proof it finds. This
/// spends the budget comparing every proof it can reach and keeps the shortest:
///   * the direct DDAR proof (if any) and every single auxiliary construction
///     that lets DDAR prove the goal are all proof-extracted and compared purely
///     by length;
///   * with budget left, it deepens from the shortest working bases (adding a
///     second construction) to hunt for an even shorter proof;
///   * if nothing at depth ≤ 1 even *solves*, the deepening MAX search finds a
///     feasible proof (rank-first — from-scratch depth-≥2 minimisation is
///     intractable), clearly labelled.
pub fn solve_best(input: &str, opts: &SolveOptions, budget: Duration) -> Result<Solution, String> {
    // Length goals already get the minimal, theorem-citing Euclidean proof.
    if opts.kind == InputKind::Geo
        && extract_goal(input).map(|g| is_metric_goal(&g)).unwrap_or(false)
    {
        let mut sol = euclidean_flow(input, opts)?;
        sol.note = format!(
            "classical Euclidean proof — already minimal (cites named theorems); {}",
            sol.note
        );
        sol.examined = Some(1);
        return Ok(sol);
    }

    let (problem, goal_holds) = to_problem(input, opts.kind)?;
    let svg = render_figure(&problem, opts, None)?;
    let (legend_cons, legend_goal) = ddar::svg::legend_lines(&problem);
    let start = Instant::now();

    let build = |proved: bool,
                 method: Method,
                 proof: Option<String>,
                 aux: Vec<String>,
                 examined: usize,
                 note: String|
     -> Solution {
        let proof_steps = proof.as_deref().map(proof_size).map(|(s, _)| s);
        Solution {
            input: input.to_string(),
            low_level: problem.to_ag_string(),
            proved,
            method,
            proof,
            svg: svg.clone(),
            aux_constructions: aux,
            constructions: legend_cons.clone(),
            goal: legend_goal.clone(),
            goal_holds_numerically: goal_holds,
            elapsed_secs: start.elapsed().as_secs_f64(),
            note,
            proof_steps,
            examined: Some(examined),
        }
    };

    // The globally shortest proof so far: (aux constructions, proof, steps, facts).
    let mut best: Option<(Vec<Construction>, String, usize, usize)> = None;
    let mut examined = 0usize;

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    // Depth 0: the direct proof (0 aux) is a candidate — NOT returned early, since
    // an auxiliary construction may yield a shorter proof.
    if let Ok(Ok(Some(proof))) =
        catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&problem)))
    {
        consider(&mut best, &mut examined, Vec::new(), proof);
    }
    // If the figure shows the goal is false and DDAR cannot prove it directly, no
    // auxiliary construction can either — fail fast.
    if best.is_none() && goal_holds == Some(false) {
        std::panic::set_hook(prev_hook);
        return Ok(build(
            false,
            Method::Ddar,
            None,
            vec![],
            examined,
            "not provable — the goal does not hold in the figure (the statement appears to be false)"
                .to_string(),
        ));
    }

    // Depth 1: every single auxiliary construction that solves, compared by
    // length. Collect the working ones as bases for the depth-2 hunt.
    let full = problem.points.len() <= 14;
    let cands = catch_unwind(AssertUnwindSafe(|| candidates(&problem, full))).unwrap_or_default();
    let warm = WarmBase::new(&problem);
    let mut bases: Vec<(Construction, usize)> = Vec::new();
    for cand in &cands {
        if start.elapsed() >= budget {
            break;
        }
        if !solves_with(&warm, &problem, cand) {
            continue;
        }
        let Some(aug) = safe_apply(&problem, std::slice::from_ref(cand)) else {
            continue;
        };
        if let Ok(Ok(Some(proof))) = catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&aug)))
        {
            bases.push((cand.clone(), proof_size(&proof).0));
            consider(&mut best, &mut examined, vec![cand.clone()], proof);
        }
    }

    // Depth 2 (opportunistic): from the shortest working bases first, add a second
    // construction and keep any strictly-shorter proof — spending the rest of the
    // budget genuinely hunting for a shorter path.
    if !bases.is_empty() {
        bases.sort_by_key(|(_, steps)| *steps);
        'outer: for (base, _) in &bases {
            if start.elapsed() >= budget {
                break;
            }
            let Some(base_problem) = safe_apply(&problem, std::slice::from_ref(base)) else {
                continue;
            };
            let full2 = base_problem.points.len() <= 14;
            let cands2 = catch_unwind(AssertUnwindSafe(|| candidates(&base_problem, full2)))
                .unwrap_or_default();
            let warm2 = catch_unwind(AssertUnwindSafe(|| WarmBase::new(&base_problem)))
                .ok()
                .flatten();
            for c2 in &cands2 {
                if start.elapsed() >= budget {
                    break 'outer;
                }
                if !solves_with(&warm2, &base_problem, c2) {
                    continue;
                }
                let Some(aug2) = safe_apply(&base_problem, std::slice::from_ref(c2)) else {
                    continue;
                };
                if let Ok(Ok(Some(proof))) =
                    catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&aug2)))
                {
                    consider(&mut best, &mut examined, vec![base.clone(), c2.clone()], proof);
                }
            }
        }
    }
    std::panic::set_hook(prev_hook);

    if let Some((aux, proof, steps, _)) = best {
        let method = if aux.is_empty() {
            Method::Ddar
        } else {
            Method::AuxSearch
        };
        let aux_desc = match aux.len() {
            0 => "no auxiliary point".to_string(),
            1 => "1 auxiliary point".to_string(),
            k => format!("{k} auxiliary points"),
        };
        let aux_strs = aux
            .iter()
            .map(|c| format!("{} = {}", c.name, c.desc))
            .collect();
        let mut sol = build(
            true,
            method,
            Some(proof),
            aux_strs,
            examined,
            format!(
                "shortest proof found: {steps} steps ({aux_desc}); examined {examined} distinct proof(s)"
            ),
        );
        // Redraw from the augmented problem so the winning proof's auxiliary
        // constructions appear on the drawing (dashed, in the aux color).
        if !aux.is_empty() {
            if let Some(aug) = safe_apply(&problem, &aux) {
                if let Ok(aux_svg) = render_figure(&aug, opts, Some(problem.points.len())) {
                    sol.svg = aux_svg;
                }
            }
        }
        return Ok(sol);
    }

    // Nothing at depth ≤ 1 even solves — find a feasible proof with the deepening
    // MAX search (rank-first). Convert remaining time to a run budget from the
    // observed rate.
    let elapsed = start.elapsed();
    let remaining = budget.saturating_sub(elapsed);
    if remaining.is_zero() {
        return Ok(build(
            false,
            Method::AuxSearch,
            None,
            vec![],
            examined,
            "no proof at depth ≤ 1 within the budget (deeper search skipped — out of time)"
                .to_string(),
        ));
    }
    let rate = (cands.len().max(1) as f64 / elapsed.as_secs_f64().max(1e-3)).max(1_000.0);
    let run_budget = ((rate * remaining.as_secs_f64()) as usize).clamp(20_000, 5_000_000);
    let (res, stats) =
        catch_unwind(AssertUnwindSafe(|| solve_with_aux(&problem, 3, run_budget, false)))
            .map_err(|_| "auxiliary search panicked".to_string())?;
    match res {
        Some(auxproof) => {
            let aug = apply_constructions(&problem, &auxproof.constructions);
            let proof = catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&aug)))
                .ok()
                .and_then(|r| r.ok().flatten());
            let n = auxproof.constructions.len();
            let steps = proof.as_deref().map(proof_size).map(|(s, _)| s).unwrap_or(0);
            let aux = auxproof
                .constructions
                .iter()
                .map(|c| format!("{} = {}", c.name, c.desc))
                .collect();
            let mut sol = build(
                true,
                Method::AuxSearch,
                proof,
                aux,
                examined,
                format!(
                    "proved with {n} auxiliary construction(s), {steps} steps ({} DDAR runs; rank-first at depth ≥ 2)",
                    stats.runs
                ),
            );
            // Same redraw: put the auxiliary constructions on the figure.
            if let Ok(aux_svg) = render_figure(&aug, opts, Some(problem.points.len())) {
                sol.svg = aux_svg;
            }
            Ok(sol)
        }
        None => Ok(build(
            false,
            Method::AuxSearch,
            None,
            vec![],
            examined,
            format!("no proof found within the {:.0}s budget", budget.as_secs_f64()),
        )),
    }
}

/// Record a proof if it is strictly shorter than the best so far (fewer steps,
/// then fewer facts). Auxiliary count is intentionally not a factor.
fn consider(
    best: &mut Option<(Vec<Construction>, String, usize, usize)>,
    examined: &mut usize,
    aux: Vec<Construction>,
    proof: String,
) {
    let (steps, facts) = proof_size(&proof);
    *examined += 1;
    if best
        .as_ref()
        .map_or(true, |(_, _, bs, bf)| (steps, facts) < (*bs, *bf))
    {
        *best = Some((aux, proof, steps, facts));
    }
}

/// Apply constructions, containing any panic from a degenerate configuration
/// (returns `None` to skip that candidate rather than crash the search).
fn safe_apply(problem: &Problem, cons: &[Construction]) -> Option<Problem> {
    catch_unwind(AssertUnwindSafe(|| apply_constructions(problem, cons))).ok()
}

/// Does adding `cand` to `problem` let DDAR prove the goal? Uses the warm-start
/// fast path when available, else a full solve; degenerate candidates are caught.
fn solves_with(warm: &Option<WarmBase>, problem: &Problem, cand: &Construction) -> bool {
    match warm {
        Some(w) => catch_unwind(AssertUnwindSafe(|| w.check(cand))).unwrap_or(false),
        None => catch_unwind(AssertUnwindSafe(|| {
            solve_problem(&apply_constructions(problem, std::slice::from_ref(cand))).unwrap_or(false)
        }))
        .unwrap_or(false),
    }
}

/// Parse `(steps, facts)` from a proof: the header's trailing "(N steps, M
/// facts …)" when present, else a count of numbered step lines.
fn proof_size(proof: &str) -> (usize, usize) {
    if let Some(first) = proof.lines().next() {
        if let Some(open) = first.rfind('(') {
            let nums: Vec<usize> = first[open..]
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|s| s.parse().ok())
                .collect();
            match nums.as_slice() {
                [s, f, ..] => return (*s, *f),
                [s] => return (*s, *s),
                _ => {}
            }
        }
    }
    let steps = proof.lines().filter(|l| is_step_line(l)).count();
    (steps, steps)
}

/// Does a line look like a numbered proof step, e.g. `001.` or `3.`?
fn is_step_line(line: &str) -> bool {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    (1..=3).contains(&digits) && t[digits..].starts_with('.')
}

/// The DDAR path: figure, direct deduction, then the auxiliary-point search.
fn deductive_flow(
    problem: Problem,
    input: &str,
    goal_holds: Option<bool>,
    opts: &SolveOptions,
) -> Result<Solution, String> {
    let start = Instant::now();
    let mut svg = render_figure(&problem, opts, None)?;
    let (legend_cons, legend_goal) = ddar::svg::legend_lines(&problem);

    let direct = catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&problem)))
        .map_err(|_| "solver panicked".to_string())?;
    let (proved, method, proof, aux_constructions, note) = match direct {
        Ok(Some(p)) => (
            true,
            Method::Ddar,
            Some(p),
            Vec::new(),
            "proved by DDAR (no auxiliary point)".to_string(),
        ),
        // The figure already shows the goal is false — the auxiliary search
        // cannot prove a false statement, so don't burn minutes on it.
        Ok(None) if goal_holds == Some(false) => (
            false,
            Method::Ddar,
            None,
            Vec::new(),
            "not provable — the goal does not hold in the figure (the statement appears to be false)"
                .to_string(),
        ),
        Ok(None) => {
            let (res, stats) = catch_unwind(AssertUnwindSafe(|| solve_max(&problem, false)))
                .map_err(|_| "auxiliary search panicked".to_string())?;
            match res {
                Some(auxproof) => {
                    let aux: Vec<String> = auxproof
                        .constructions
                        .iter()
                        .map(|c| format!("{} = {}", c.name, c.desc))
                        .collect();
                    let augmented = apply_constructions(&problem, &auxproof.constructions);
                    // Redraw the figure from the augmented problem so the
                    // auxiliary constructions appear on the drawing (dashed,
                    // in the aux color) rather than only as text.
                    if let Ok(aux_svg) =
                        render_figure(&augmented, opts, Some(problem.points.len()))
                    {
                        svg = aux_svg;
                    }
                    let proof = if opts.want_proof {
                        catch_unwind(AssertUnwindSafe(|| solve_problem_with_proof(&augmented)))
                            .ok()
                            .and_then(|r| r.ok().flatten())
                    } else {
                        None
                    };
                    let note = format!(
                        "proved with {} auxiliary construction(s) in {} DDAR runs",
                        auxproof.constructions.len(),
                        stats.runs
                    );
                    (true, Method::AuxSearch, proof, aux, note)
                }
                None => (
                    false,
                    Method::AuxSearch,
                    None,
                    Vec::new(),
                    format!("no proof found within budget ({} DDAR runs)", stats.runs),
                ),
            }
        }
        Err(e) => return Err(e),
    };

    let proof_steps = proof.as_deref().map(proof_size).map(|(s, _)| s);
    Ok(Solution {
        input: input.to_string(),
        low_level: problem.to_ag_string(),
        proved,
        method,
        proof,
        svg,
        aux_constructions,
        constructions: legend_cons,
        goal: legend_goal,
        goal_holds_numerically: goal_holds,
        elapsed_secs: start.elapsed().as_secs_f64(),
        note,
        proof_steps,
        examined: None,
    })
}

/// The classical-Euclidean path for absolute-length goals: draw the figure from
/// the construction lines, then prove the length goal with named theorems.
fn euclidean_flow(program: &str, opts: &SolveOptions) -> Result<Solution, String> {
    let start = Instant::now();
    let (cons, goal) = split_metric_program(program)?;

    // Figure + low-level form: compile the construction lines alone (the metric
    // goal is not a DDAR predicate, so it is dropped from the drawing).
    let (svg, low_level, goal_holds, legend_cons) = match geo::compile(&cons) {
        Ok(c) => {
            let svg = render_figure(&c.problem, opts, None).unwrap_or_default();
            let (lc, _) = ddar::svg::legend_lines(&c.problem);
            (svg, c.problem.to_ag_string(), c.goal_numerically_holds, lc)
        }
        Err(_) => (String::new(), String::new(), None, Vec::new()),
    };

    let result = catch_unwind(AssertUnwindSafe(|| ddar::metric::solve(&cons, &goal, 48)))
        .map_err(|_| "metric prover panicked".to_string())?;
    let (proved, proof, note) = match result {
        Ok(report) => (
            true,
            Some(report),
            "proved by the classical Euclidean prover".to_string(),
        ),
        Err(e) => (false, None, format!("metric prover: {e}")),
    };

    let proof_steps = proof.as_deref().map(proof_size).map(|(s, _)| s);
    Ok(Solution {
        input: program.to_string(),
        low_level,
        proved,
        method: Method::Euclidean,
        proof,
        svg,
        aux_constructions: Vec::new(),
        constructions: legend_cons,
        goal: Some(goal),
        goal_holds_numerically: goal_holds,
        elapsed_secs: start.elapsed().as_secs_f64(),
        note,
        proof_steps,
        examined: None,
    })
}

/// Render the figure; `aux_from` marks the index of the first auxiliary point
/// (from a search-augmented problem), drawn in the distinct aux style.
fn render_figure(
    problem: &Problem,
    opts: &SolveOptions,
    aux_from: Option<usize>,
) -> Result<String, String> {
    let fig = FigureOptions {
        theme: opts.theme,
        title: opts.title.clone(),
        panel: opts.panel,
        status: None,
        aux_from,
    };
    catch_unwind(AssertUnwindSafe(|| render_with(problem, &fig)))
        .map_err(|_| "figure rendering panicked".to_string())
}

/// Pull the goal expression out of a program (`prove …` / `goal: …` / `? …`).
fn extract_goal(src: &str) -> Option<String> {
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            if let Some(g) = s
                .strip_prefix("prove ")
                .or_else(|| s.strip_prefix("goal:"))
                .or_else(|| s.strip_prefix("?"))
            {
                return Some(g.trim().to_string());
            }
        }
    }
    None
}

/// Split a metric program into its construction text and the goal equation
/// (mirrors the CLI's `--metric` parsing).
fn split_metric_program(src: &str) -> Result<(String, String), String> {
    let mut cons: Vec<String> = Vec::new();
    let mut goal: Option<String> = None;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            match s
                .strip_prefix("prove ")
                .or_else(|| s.strip_prefix("goal:"))
                .or_else(|| s.strip_prefix("?"))
            {
                Some(g) => goal = Some(g.trim().to_string()),
                None => cons.push(s.to_string()),
            }
        }
    }
    let goal = goal.ok_or_else(|| "program needs a `prove <equation>` line".to_string())?;
    Ok((cons.join("\n"), goal))
}

/// Decide whether a goal is an *absolute-length* equation (→ Euclidean prover)
/// rather than a relation or a scale-invariant product/ratio (→ DDAR).
///
/// The distinguishing feature is an absolute numeric constant: `= 144`,
/// `AC² = 27`, `3·√3`. A purely product/ratio identity (Ptolemy:
/// `AC·BD = AB·CD + AD·BC`) has no bare constant and stays on the DDAR path.
fn is_metric_goal(goal: &str) -> bool {
    if goal.contains("sqrt") || goal.contains('√') {
        return true;
    }
    // A term with only digits/`.`/`*`/spaces (and at least one digit) — i.e. no
    // point names — is an absolute constant like `144`.
    for side in goal.split('=') {
        for term in side.split(['+', '-']) {
            let t = term.trim();
            if !t.is_empty()
                && t.chars().any(|c| c.is_ascii_digit())
                && t.chars()
                    .all(|c| c.is_ascii_digit() || c == '.' || c == '*' || c.is_whitespace())
            {
                return true;
            }
        }
    }
    false
}

/// Parse a theme name; defaults to dark (AlphaGeometry's original look).
pub fn parse_theme(s: &str) -> Theme {
    match s.to_ascii_lowercase().as_str() {
        "light" => Theme::Light,
        _ => Theme::Dark,
    }
}
