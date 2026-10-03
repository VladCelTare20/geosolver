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
//! Only a Euclidean proof sets `proved`. A metric goal no prover reaches is
//! checked numerically in many sampled figures and reported as
//! [`Status::HoldsNumerically`] — evidence, never a proof.
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

/// What a solve established. Serialized as `"proved"`, `"holds-numerically"`,
/// `"refuted"` or `"not-proved"`; only `Proved` comes with `proved: true`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// A classical Euclidean proof: a DDAR deduction or a theorem-citing
    /// metric proof.
    Proved,
    /// No Euclidean proof was found, but the goal held in every one of
    /// `numeric_samples` independently sampled figures. Evidence, not a proof.
    HoldsNumerically,
    /// The goal fails in a sampled figure: the statement appears to be false.
    Refuted,
    /// Neither proved nor refuted (search budget or time limit reached, or the
    /// metric prover could not process the goal).
    NotProved,
}

impl Status {
    /// A short human label for the verdict.
    pub fn label(self) -> &'static str {
        match self {
            Status::Proved => "Proven",
            Status::HoldsNumerically => "Not proven — holds numerically",
            Status::Refuted => "Refuted",
            Status::NotProved => "Not proven",
        }
    }
}

/// The full outcome of a solve: proof, figure, and metadata.
#[derive(Clone, serde::Serialize)]
pub struct Solution {
    /// The original input program.
    pub input: String,
    /// The compiled low-level AlphaGeometry form (coordinates + predicates).
    pub low_level: String,
    /// Whether the goal was proved — by a Euclidean proof, nothing else.
    pub proved: bool,
    /// The verdict; `proved` is `status == Status::Proved`.
    pub status: Status,
    /// Which prover path was taken.
    pub method: Method,
    /// The numbered, citation-annotated proof. `Some` only when proved (and
    /// `want_proof`).
    pub proof: Option<String>,
    /// The numeric check of a metric goal, when one ran: a counterexample
    /// (`Refuted`) or the agreement report (`HoldsNumerically`). Never a proof.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_evidence: Option<String>,
    /// How many independently sampled figures the numeric check used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric_samples: Option<usize>,
    /// The figure as a standalone SVG document.
    pub svg: String,
    /// Human-readable auxiliary constructions the search introduced, if any.
    pub aux_constructions: Vec<String>,
    /// The hypotheses in natural notation (the figure legend, as text).
    pub constructions: Vec<String>,
    /// The goal in natural notation.
    pub goal: Option<String>,
    /// `Some(false)` if the goal does not hold in a sampled figure (the
    /// statement appears to be false); `Some(true)` if it holds there; `None`
    /// if not numerically checkable. Never a proof either way.
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
    /// The problem the figure was drawn from — search-augmented when the proof
    /// needed auxiliary points — for the web app's presentation layer, which
    /// draws its own interactive figure. Never serialized.
    #[serde(skip)]
    pub figure: Option<FigureSource>,
}

/// What a figure is drawn from: the (possibly augmented) problem, and the index
/// of its first auxiliary point when the search added some.
#[derive(Clone)]
pub struct FigureSource {
    pub problem: Problem,
    pub aux_from: Option<usize>,
}

/// Solve one problem end to end. Never panics: the engine's degenerate-figure
/// panics (which the aux search normally catches internally) are contained
/// here too, so a bad input yields an `Err`, not a crash.
///
/// No wall-clock bound beyond the aux search's run caps (`AUX_MAX_RUNS`); use
/// [`solve_within`] where a caller needs a hard deadline.
#[cfg(test)]
pub fn solve(input: &str, opts: &SolveOptions) -> Result<Solution, String> {
    solve_within(input, opts, None)
}

/// Default wall-clock limit for the CLI and MCP solve paths.
pub const DEFAULT_SOLVE_TIMEOUT: Duration = Duration::from_secs(60);

/// [`solve`] with an optional wall-clock `timeout`. When it expires the result
/// is `proved: false` with a "time limit" note; the abandoned search thread
/// keeps running until its run cap, so long-lived callers should also bound
/// `AUX_MAX_RUNS` (see `security::apply_process_limits`).
pub fn solve_within(
    input: &str,
    opts: &SolveOptions,
    timeout: Option<Duration>,
) -> Result<Solution, String> {
    let deadline = timeout.map(|t| Instant::now() + t);
    match opts.kind {
        InputKind::LowLevel => {
            let problem = Problem::parse(input).map_err(|e| format!("parse error: {e}"))?;
            deductive_flow(problem, input, None, opts, deadline)
        }
        InputKind::Geo => {
            // Metric equations DDAR cannot express (absolute lengths, squares,
            // sums of products, …) get the classical Euclidean prover; plain
            // relations go through the DDAR closure.
            if extract_goal(input).map(|g| is_metric_goal(&g)).unwrap_or(false) {
                return euclidean_flow(input, opts);
            }
            let compiled = match geo::compile(input) {
                Ok(c) => c,
                Err(e) if e.is_metric_goal() => return euclidean_flow(input, opts),
                Err(e) => return Err(format!("compile error: {e}")),
            };
            // Safety net: a goal the compiler lowered to a trivially-true
            // placeholder would be "proved" vacuously by DDAR.
            if compiled.problem.goal.as_ref().is_some_and(is_placeholder_goal) {
                return euclidean_flow(input, opts);
            }
            deductive_flow(
                compiled.problem,
                input,
                compiled.goal_numerically_holds,
                opts,
                deadline,
            )
        }
    }
}

/// The compiler's stand-in for goals with no DDAR predicate: `cong a b a b`.
fn is_placeholder_goal(goal: &ddar::Predicate) -> bool {
    goal.name == "cong" && goal.points.len() == 4 && goal.points[..2] == goal.points[2..]
}

/// Run `f` to completion, or until `deadline` passes. `None` means the deadline
/// expired first (the worker thread is abandoned, not killed); `Some(Err(_))`
/// means `f` panicked.
fn run_until<T: Send + 'static>(
    deadline: Option<Instant>,
    f: impl FnOnce() -> T + Send + 'static,
) -> Option<std::thread::Result<T>> {
    let Some(deadline) = deadline else {
        return Some(catch_unwind(AssertUnwindSafe(f)));
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("solve-deadline".into())
        .spawn(move || {
            let _ = tx.send(catch_unwind(AssertUnwindSafe(f)));
        });
    if spawned.is_err() {
        return Some(Err(Box::new("could not start the solver thread")));
    }
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
}

/// Final consistency checks applied to every solve result: a numerically
/// refuted goal is never reported proved, only a proved result carries proof
/// text (and always some, when one was requested), and `status` agrees with
/// both.
fn reconcile(sol: &mut Solution, want_proof: bool) {
    if sol.proved && sol.goal_holds_numerically == Some(false) {
        sol.proved = false;
        sol.proof = None;
        sol.proof_steps = None;
        sol.note = format!(
            "not proved — the prover derived the goal, but it does not hold in the sampled \
             figure, so the derivation is rejected as unsound (likely a degenerate figure); {}",
            sol.note
        );
    }
    if sol.proved && sol.proof.is_none() && want_proof {
        let aux = if sol.aux_constructions.is_empty() {
            String::new()
        } else {
            format!(" using {}", sol.aux_constructions.join("; "))
        };
        sol.proof = Some(format!(
            "The goal was proved{aux} ({}), but the step-by-step proof could not be extracted.",
            sol.note
        ));
    }
    if !sol.proved {
        sol.proof = None;
        sol.proof_steps = None;
    }
    sol.status = if sol.proved {
        Status::Proved
    } else if sol.goal_holds_numerically == Some(false) {
        Status::Refuted
    } else if sol.goal_holds_numerically == Some(true) && sol.numeric_samples.is_some() {
        Status::HoldsNumerically
    } else {
        Status::NotProved
    };
}

/// Sentinel from [`to_problem`]: the goal has no DDAR form (route it to the
/// Euclidean prover).
const METRIC_GOAL: &str = "metric goal";

/// Compile a high-level `.geo` program, or parse a low-level problem, into a
/// `Problem` plus any numeric goal check from the compiler.
fn to_problem(input: &str, kind: InputKind) -> Result<(Problem, Option<bool>), String> {
    match kind {
        InputKind::Geo => {
            let compiled = geo::compile(input).map_err(|e| {
                if e.is_metric_goal() {
                    METRIC_GOAL.to_string()
                } else {
                    format!("compile error: {e}")
                }
            })?;
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
    let euclid = |opts: &SolveOptions| -> Result<Solution, String> {
        let mut sol = euclidean_flow(input, opts)?;
        if sol.proved {
            sol.note = format!(
                "classical Euclidean proof — already minimal (cites named theorems); {}",
                sol.note
            );
        }
        sol.examined = Some(1);
        Ok(sol)
    };
    // Length goals already get the minimal, theorem-citing Euclidean proof.
    if opts.kind == InputKind::Geo
        && extract_goal(input).map(|g| is_metric_goal(&g)).unwrap_or(false)
    {
        return euclid(opts);
    }

    let (problem, goal_holds) = match to_problem(input, opts.kind) {
        Err(e) if e == METRIC_GOAL => return euclid(opts),
        other => other?,
    };
    if opts.kind == InputKind::Geo && problem.goal.as_ref().is_some_and(is_placeholder_goal) {
        return euclid(opts);
    }
    let mut sol = ddar::quiet_panic::quiet(|| best_deductive(input, opts, budget, problem, goal_holds))?;
    reconcile(&mut sol, true);
    Ok(sol)
}

fn best_deductive(
    input: &str,
    opts: &SolveOptions,
    budget: Duration,
    problem: Problem,
    goal_holds: Option<bool>,
) -> Result<Solution, String> {
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
            status: Status::NotProved,
            method,
            proof,
            numeric_evidence: None,
            numeric_samples: None,
            svg: svg.clone(),
            aux_constructions: aux,
            constructions: legend_cons.clone(),
            goal: legend_goal.clone(),
            goal_holds_numerically: goal_holds,
            elapsed_secs: start.elapsed().as_secs_f64(),
            note,
            proof_steps,
            examined: Some(examined),
            figure: Some(FigureSource { problem: problem.clone(), aux_from: None }),
        }
    };

    // The globally shortest proof so far: (aux constructions, proof, steps, facts).
    let mut best: Option<(Vec<Construction>, String, usize, usize)> = None;
    let mut examined = 0usize;


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
                sol.figure = Some(FigureSource { problem: aug, aux_from: Some(problem.points.len()) });
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
    let run_budget = ((rate * remaining.as_secs_f64()) as usize).clamp(1, 5_000_000);
    // The run estimate is only a rate guess; the wall clock is the real bound.
    let search_problem = problem.clone();
    let searched = run_until(Some(Instant::now() + remaining), move || {
        solve_with_aux(&search_problem, 3, run_budget, false)
    });
    let Some(searched) = searched else {
        return Ok(build(
            false,
            Method::AuxSearch,
            None,
            vec![],
            examined,
            format!(
                "no proof found within the {:.0}s budget (deeper search hit the time limit)",
                budget.as_secs_f64()
            ),
        ));
    };
    let (res, stats) = searched.map_err(|_| "auxiliary search panicked".to_string())?;
    match res {
        Some(auxproof) => {
            let Some(aug) = safe_apply(&problem, &auxproof.constructions) else {
                return Ok(build(
                    false,
                    Method::AuxSearch,
                    None,
                    vec![],
                    examined,
                    "the auxiliary search reported a proof, but its constructions could not be \
                     replayed (degenerate figure)"
                        .to_string(),
                ));
            };
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
            sol.figure = Some(FigureSource { problem: aug, aux_from: Some(problem.points.len()) });
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
        .is_none_or(|(_, _, bs, bf)| (steps, facts) < (*bs, *bf))
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
    deadline: Option<Instant>,
) -> Result<Solution, String> {
    let start = Instant::now();
    let mut svg = render_figure(&problem, opts, None)?;
    let mut figure = FigureSource { problem: problem.clone(), aux_from: None };
    let (legend_cons, legend_goal) = ddar::svg::legend_lines(&problem);
    let limit_note = || {
        let secs = deadline.map_or(0.0, |d| d.duration_since(start).as_secs_f64());
        format!("not proved — stopped at the {secs:.0}s time limit")
    };

    let direct_problem = problem.clone();
    let direct = run_until(deadline, move || solve_problem_with_proof(&direct_problem));
    let direct = match direct {
        Some(r) => r.map_err(|_| "solver panicked".to_string())?,
        None => Ok(None),
    };
    let timed_out = || deadline.is_some_and(|d| Instant::now() >= d);
    let (proved, method, proof, aux_constructions, note) = match direct {
        Ok(None) if timed_out() => (false, Method::Ddar, None, Vec::new(), limit_note()),
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
            let search_problem = problem.clone();
            let searched = run_until(deadline, move || solve_max(&search_problem, false));
            let (res, stats) = match searched {
                Some(r) => r.map_err(|_| "auxiliary search panicked".to_string())?,
                None => (None, ddar::aux_search::SearchStats { runs: 0 }),
            };
            let augmented = res
                .as_ref()
                .and_then(|a| safe_apply(&problem, &a.constructions));
            match (res, augmented) {
                (None, _) if timed_out() => {
                    (false, Method::AuxSearch, None, Vec::new(), limit_note())
                }
                (Some(_), None) => (
                    false,
                    Method::AuxSearch,
                    None,
                    Vec::new(),
                    "the auxiliary search reported a proof, but its constructions could not be \
                     replayed (degenerate figure)"
                        .to_string(),
                ),
                (Some(auxproof), Some(augmented)) => {
                    let aux: Vec<String> = auxproof
                        .constructions
                        .iter()
                        .map(|c| format!("{} = {}", c.name, c.desc))
                        .collect();
                    // Redraw the figure from the augmented problem so the
                    // auxiliary constructions appear on the drawing (dashed,
                    // in the aux color) rather than only as text.
                    if let Ok(aux_svg) =
                        render_figure(&augmented, opts, Some(problem.points.len()))
                    {
                        svg = aux_svg;
                    }
                    figure = FigureSource {
                        problem: augmented.clone(),
                        aux_from: Some(problem.points.len()),
                    };
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
                (None, _) => (
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
    let mut sol = Solution {
        input: input.to_string(),
        low_level: problem.to_ag_string(),
        proved,
        status: Status::NotProved,
        method,
        proof,
        numeric_evidence: None,
        numeric_samples: None,
        svg,
        aux_constructions,
        constructions: legend_cons,
        goal: legend_goal,
        goal_holds_numerically: goal_holds,
        elapsed_secs: start.elapsed().as_secs_f64(),
        note,
        proof_steps,
        examined: None,
        figure: Some(figure),
    };
    reconcile(&mut sol, opts.want_proof);
    Ok(sol)
}

/// The classical-Euclidean path for absolute-length goals: draw the figure from
/// the construction lines, then prove the length goal with named theorems.
fn euclidean_flow(program: &str, opts: &SolveOptions) -> Result<Solution, String> {
    let start = Instant::now();
    let (cons, goal) = split_metric_program(program)?;

    // Figure + low-level form: compile the construction lines alone (the metric
    // goal is not a DDAR predicate, so it is dropped from the drawing).
    let (svg, low_level, goal_holds, legend_cons, figure) = match geo::compile(&cons) {
        Ok(c) => {
            let svg = render_figure(&c.problem, opts, None).unwrap_or_default();
            let (lc, _) = ddar::svg::legend_lines(&c.problem);
            let ll = c.problem.to_ag_string();
            let fig = FigureSource { problem: c.problem, aux_from: None };
            (svg, ll, c.goal_numerically_holds, lc, Some(fig))
        }
        Err(_) => (String::new(), String::new(), None, Vec::new(), None),
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        ddar::metric::solve(&cons, &goal, ddar::metric::DEFAULT_SAMPLES)
    }))
    .map_err(|_| "metric prover panicked".to_string())?;
    let mut goal_holds = goal_holds;
    let (mut numeric_evidence, mut numeric_samples) = (None, None);
    let (proved, proof, note) = match result {
        Ok(proof) => (
            true,
            Some(proof),
            "proved by the classical Euclidean prover".to_string(),
        ),
        Err(ddar::metric::MetricError::NoProof { reason, evidence }) => {
            goal_holds = Some(true);
            numeric_samples = Some(evidence.samples);
            numeric_evidence = Some(evidence.report);
            (
                false,
                None,
                format!(
                    "not proved — no classical Euclidean proof was found ({reason}); the goal \
                     holds numerically in {} sampled figures, which is evidence, not a proof",
                    evidence.samples
                ),
            )
        }
        Err(ddar::metric::MetricError::Refuted(report)) => {
            goal_holds = Some(false);
            numeric_evidence = Some(report);
            (
                false,
                None,
                "not provable — the equation fails in a sampled figure (the statement \
                 appears to be false)"
                    .to_string(),
            )
        }
        Err(e) => (false, None, format!("metric prover: {}", e.message())),
    };

    let proof_steps = proof.as_deref().map(proof_size).map(|(s, _)| s);
    let mut sol = Solution {
        input: program.to_string(),
        low_level,
        proved,
        status: Status::NotProved,
        method: Method::Euclidean,
        proof,
        numeric_evidence,
        numeric_samples,
        svg,
        aux_constructions: Vec::new(),
        constructions: legend_cons,
        goal: Some(goal),
        goal_holds_numerically: goal_holds,
        elapsed_secs: start.elapsed().as_secs_f64(),
        note,
        proof_steps,
        examined: None,
        figure,
    };
    reconcile(&mut sol, opts.want_proof);
    Ok(sol)
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

/// Decide whether a goal must go to the Euclidean (metric) prover rather than
/// DDAR.
///
/// Mirrors `ddar::geo`'s goal classification: a metric equation stays on DDAR
/// only when it lowers to a real DDAR predicate — `dist = dist` (cong),
/// `angle = <deg>` / `angle = angle`, an equality of products of bare distances
/// (power of a point), a zero-constant linear sum of distances, or a linear sum
/// of angles. Everything else (absolute lengths, squares, roots, areas, sums of
/// products such as Ptolemy, ratios with a constant) has no DDAR predicate and
/// would otherwise compile to a vacuous placeholder.
fn is_metric_goal(goal: &str) -> bool {
    let g = goal.trim();
    let head = g.split('(').next().unwrap_or("").trim();
    if matches!(
        head,
        "coll" | "cyclic" | "cong" | "perp" | "para" | "eqangle" | "eqratio" | "on"
    ) {
        return false;
    }
    let Some((lhs, rhs)) = g.split_once('=') else {
        return false;
    };
    match (MExpr::parse(lhs), MExpr::parse(rhs)) {
        (Some(l), Some(r)) => !ddar_expressible(&l, &r),
        // Unparseable here (e.g. nested past MAX_GOAL_DEPTH): anything metric
        // goes to the Euclidean prover, which reports its own error; the rest is
        // left to `geo::compile`.
        _ => ["dist(", "area(", "sqrt", "√", "^"].iter().any(|k| g.contains(k)),
    }
}

/// A metric expression, as far as goal routing needs to see it.
#[derive(Clone, Debug, PartialEq)]
enum MExpr {
    Num(f64),
    Dist(String, String),
    Angle(String, String, String),
    /// sqrt/sin/cos/tan/area — never DDAR-expressible.
    Opaque,
    Neg(Box<MExpr>),
    Add(Box<MExpr>, Box<MExpr>),
    Sub(Box<MExpr>, Box<MExpr>),
    Mul(Box<MExpr>, Box<MExpr>),
    Div(Box<MExpr>, Box<MExpr>),
    Pow(Box<MExpr>),
}

#[derive(Clone, Debug, PartialEq)]
enum MTok {
    Num(f64),
    Ident(String),
    Op(char),
}

impl MExpr {
    fn parse(src: &str) -> Option<MExpr> {
        let toks = mtokens(src)?;
        let mut pos = 0;
        let e = parse_sum(&toks, &mut pos, 0)?;
        (pos == toks.len()).then_some(e)
    }
}

const MAX_GOAL_DEPTH: usize = 64;

fn mtokens(src: &str) -> Option<Vec<MTok>> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let s = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            out.push(MTok::Num(chars[s..i].iter().collect::<String>().parse().ok()?));
        } else if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '\'')) {
                i += 1;
            }
            out.push(MTok::Ident(chars[s..i].iter().collect()));
        } else if "+-*/^(),".contains(c) {
            out.push(MTok::Op(c));
            i += 1;
        } else {
            return None;
        }
    }
    Some(out)
}

fn parse_sum(t: &[MTok], p: &mut usize, d: usize) -> Option<MExpr> {
    let mut e = parse_product(t, p, d)?;
    while let Some(MTok::Op(op @ ('+' | '-'))) = t.get(*p) {
        *p += 1;
        let r = Box::new(parse_product(t, p, d)?);
        e = if *op == '+' { MExpr::Add(Box::new(e), r) } else { MExpr::Sub(Box::new(e), r) };
    }
    Some(e)
}

fn parse_product(t: &[MTok], p: &mut usize, d: usize) -> Option<MExpr> {
    let mut e = parse_power(t, p, d)?;
    while let Some(MTok::Op(op @ ('*' | '/'))) = t.get(*p) {
        *p += 1;
        let r = Box::new(parse_power(t, p, d)?);
        e = if *op == '*' { MExpr::Mul(Box::new(e), r) } else { MExpr::Div(Box::new(e), r) };
    }
    Some(e)
}

fn parse_power(t: &[MTok], p: &mut usize, d: usize) -> Option<MExpr> {
    let base = parse_unary(t, p, d)?;
    if t.get(*p) == Some(&MTok::Op('^')) {
        *p += 1;
        match t.get(*p) {
            Some(MTok::Num(_)) => *p += 1,
            _ => return None,
        }
        return Some(MExpr::Pow(Box::new(base)));
    }
    Some(base)
}

fn parse_unary(t: &[MTok], p: &mut usize, d: usize) -> Option<MExpr> {
    if d > MAX_GOAL_DEPTH {
        return None;
    }
    if t.get(*p) == Some(&MTok::Op('-')) {
        *p += 1;
        return Some(MExpr::Neg(Box::new(parse_unary(t, p, d + 1)?)));
    }
    match t.get(*p)?.clone() {
        MTok::Num(v) => {
            *p += 1;
            Some(MExpr::Num(v))
        }
        MTok::Op('(') => {
            *p += 1;
            let e = parse_sum(t, p, d + 1)?;
            (t.get(*p) == Some(&MTok::Op(')'))).then_some(())?;
            *p += 1;
            Some(e)
        }
        MTok::Ident(name) => {
            *p += 1;
            let low = name.to_ascii_lowercase();
            if low == "pi" {
                return Some(MExpr::Num(std::f64::consts::PI));
            }
            (t.get(*p) == Some(&MTok::Op('('))).then_some(())?;
            *p += 1;
            if matches!(low.as_str(), "sqrt" | "sin" | "cos" | "tan") {
                parse_sum(t, p, d + 1)?;
                (t.get(*p) == Some(&MTok::Op(')'))).then_some(())?;
                *p += 1;
                return Some(MExpr::Opaque);
            }
            let mut args = Vec::new();
            loop {
                match t.get(*p)? {
                    MTok::Ident(a) => args.push(a.clone()),
                    _ => return None,
                }
                *p += 1;
                match t.get(*p)? {
                    MTok::Op(',') => *p += 1,
                    MTok::Op(')') => {
                        *p += 1;
                        break;
                    }
                    _ => return None,
                }
            }
            match (low.as_str(), args.as_slice()) {
                ("dist", [a, b]) => Some(MExpr::Dist(a.clone(), b.clone())),
                ("angle", [a, b, c]) => Some(MExpr::Angle(a.clone(), b.clone(), c.clone())),
                ("area", [_, _, _]) => Some(MExpr::Opaque),
                _ => None,
            }
        }
        MTok::Op(_) => None,
    }
}

/// Would `ddar::geo` lower `lhs = rhs` to a real DDAR predicate?
fn ddar_expressible(lhs: &MExpr, rhs: &MExpr) -> bool {
    use MExpr::*;
    match (lhs, rhs) {
        (Dist(..), Dist(..)) | (Angle(..), Num(_)) | (Angle(..), Angle(..)) => return true,
        (Dist(..), Num(_)) | (Num(_), Dist(..)) => return false,
        _ => {}
    }
    if let (Some(l), Some(r)) = (dist_product(lhs), dist_product(rhs)) {
        if l >= 2 || r >= 2 {
            return true;
        }
    }
    if let (Some((lt, lk)), Some((rt, rk))) = (lincomb(lhs, false), lincomb(rhs, false)) {
        if (lk - rk).abs() <= 1e-12 && net_terms(lt, rt) >= 2 {
            return true;
        }
    }
    if let (Some((lt, _)), Some((rt, _))) = (lincomb(lhs, true), lincomb(rhs, true)) {
        if net_terms(lt, rt) >= 1 {
            return true;
        }
    }
    false
}

/// Number of factors if `e` is a product of bare distances.
fn dist_product(e: &MExpr) -> Option<usize> {
    match e {
        MExpr::Dist(..) => Some(1),
        MExpr::Mul(x, y) => Some(dist_product(x)? + dist_product(y)?),
        _ => None,
    }
}

/// `Σ cᵢ·termᵢ + k` over distances (or, with `angles`, over angles): the
/// signed terms (keyed by their unordered endpoints) and the constant.
type LinTerms = Vec<(f64, Vec<String>)>;

fn lincomb(e: &MExpr, angles: bool) -> Option<(LinTerms, f64)> {
    use MExpr::*;
    let scale = |(t, k): (LinTerms, f64), c: f64| {
        (t.into_iter().map(|(x, n)| (x * c, n)).collect::<LinTerms>(), k * c)
    };
    match e {
        Num(v) => Some((Vec::new(), *v)),
        Dist(a, b) if !angles => {
            let mut key = vec![a.clone(), b.clone()];
            key.sort();
            Some((vec![(1.0, key)], 0.0))
        }
        Angle(a, b, c) if angles => {
            let (x, z) = if a <= c { (a, c) } else { (c, a) };
            Some((vec![(1.0, vec![x.clone(), b.clone(), z.clone()])], 0.0))
        }
        Neg(x) => Some(scale(lincomb(x, angles)?, -1.0)),
        Add(x, y) | Sub(x, y) => {
            let (mut t, k1) = lincomb(x, angles)?;
            let sign = if matches!(e, Sub(..)) { -1.0 } else { 1.0 };
            let (t2, k2) = scale(lincomb(y, angles)?, sign);
            t.extend(t2);
            Some((t, k1 + k2))
        }
        Mul(x, y) => match (&**x, &**y) {
            (Num(c), o) | (o, Num(c)) => Some(scale(lincomb(o, angles)?, *c)),
            _ => None,
        },
        Div(x, y) => match &**y {
            Num(c) if *c != 0.0 => Some(scale(lincomb(x, angles)?, 1.0 / c)),
            _ => None,
        },
        _ => None,
    }
}

/// Count the terms of `lhs - rhs` that survive merging and cancellation.
fn net_terms(lhs: LinTerms, rhs: LinTerms) -> usize {
    let mut merged: LinTerms = Vec::new();
    for (c, key) in lhs.into_iter().chain(rhs.into_iter().map(|(c, k)| (-c, k))) {
        match merged.iter_mut().find(|m| m.1 == key) {
            Some(m) => m.0 += c,
            None => merged.push((c, key)),
        }
    }
    merged.iter().filter(|m| m.0.abs() > 1e-12).count()
}

/// Parse a theme name; defaults to dark (AlphaGeometry's original look).
pub fn parse_theme(s: &str) -> Theme {
    match s.to_ascii_lowercase().as_str() {
        "light" => Theme::Light,
        _ => Theme::Dark,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIST_CONST_FREE: &str = "A = free\nB = free\nprove dist(A,B) = 3";
    /// A median halves the area: true, but no theorem in the library speaks of
    /// areas, so it has no Euclidean proof here.
    const NUMERIC_ONLY: &str = "A B C = triangle\nM = midpoint(B, C)\nprove area(A,B,M) = area(A,M,C)";
    const PYTHAGORAS_GENERIC: &str =
        "A B C = triangle\nprove dist(A,B)^2 = dist(B,C)^2 + dist(A,C)^2";
    const THALES_ANGLE: &str = "A = free\nO = free\nB = reflect(A, O)\n\
                                C = point: on(C, circle(O, A))\nprove angle(A,C,B) = 90";
    const GENERIC_RIGHT_ANGLE: &str = "A B C = triangle\nprove angle(A,C,B) = 90";
    // Needs a two-point auxiliary search (seconds, not milliseconds).
    const BUTTERFLY: &str = "O = free\nP = free\nQ = on_circle(O, P)\nM = midpoint(P, Q)\n\
        A = on_circle(O, P)\nB = meet(line(A, M), circle(O, P))\nC = on_circle(O, P)\n\
        D = meet(line(C, M), circle(O, P))\nX = meet(line(A, D), line(P, Q))\n\
        Y = meet(line(B, C), line(P, Q))\nprove cong(M, X, M, Y)";

    #[test]
    fn refuted_metric_goal_is_not_proved() {
        let sol = solve(DIST_CONST_FREE, &SolveOptions::default()).unwrap();
        assert_eq!(sol.method, Method::Euclidean);
        assert!(!sol.proved, "a refuted goal must not count as proved: {}", sol.note);
        assert_eq!(sol.goal_holds_numerically, Some(false));
    }

    #[test]
    fn pythagoras_on_a_generic_triangle_is_not_proved() {
        let sol = solve(PYTHAGORAS_GENERIC, &SolveOptions::default()).unwrap();
        assert_eq!(sol.method, Method::Euclidean);
        assert!(!sol.proved, "{}", sol.note);
    }

    #[test]
    fn pythagoras_on_a_generic_triangle_is_not_proved_by_solve_best() {
        let sol = solve_best(PYTHAGORAS_GENERIC, &SolveOptions::default(), Duration::from_secs(2))
            .unwrap();
        assert!(!sol.proved, "{}", sol.note);
    }

    #[test]
    fn a_true_metric_theorem_is_still_proved() {
        let src = include_str!("../../alphageometry-rs/examples/metric/stewart.geo");
        let sol = solve(src, &SolveOptions::default()).unwrap();
        assert_eq!(sol.method, Method::Euclidean);
        assert!(sol.proved, "{}", sol.note);
        assert_ne!(sol.goal_holds_numerically, Some(false));
    }

    #[test]
    fn numeric_angle_goal_goes_to_ddar() {
        let sol = solve(THALES_ANGLE, &SolveOptions::default()).unwrap();
        assert_ne!(sol.method, Method::Euclidean, "{}", sol.note);
        assert!(sol.proved, "{}", sol.note);
    }

    #[test]
    fn false_numeric_angle_goal_is_not_proved() {
        let sol = solve(GENERIC_RIGHT_ANGLE, &SolveOptions::default()).unwrap();
        assert!(!sol.proved, "{}", sol.note);
    }

    #[test]
    fn goal_routing() {
        for g in [
            "dist(A,B) = 3",
            "dist(A,B)^2 = dist(B,C)^2 + dist(A,C)^2",
            "dist(A,C)^2 + dist(B,D)^2 = 144",
            "dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D) + dist(A,D)*dist(B,C)",
            "dist(A,B)/dist(C,D) = 2",
            "dist(A,B) = 3*sqrt(3)",
            "area(A,B,C) = area(A,B,D)",
            "dist(A,B) + dist(B,C) = 10",
        ] {
            assert!(is_metric_goal(g), "{g} should go to the Euclidean prover");
        }
        for g in [
            "cong(A,B,C,D)",
            "perp(A,B,C,D)",
            "coll(A, B, C)",
            "cyclic(A,B,C,D)",
            "eqangle(A,B,A,X,A,X,A,C)",
            "dist(A,B) = dist(C,D)",
            "angle(A,B,C) = 90",
            "angle(A,B,C) = angle(D,E,F)",
            "angle(A,B,C) + angle(B,C,A) + angle(C,A,B) = 180",
            "dist(P,A)*dist(P,B) = dist(P,C)*dist(P,D)",
            "dist(A,M) + dist(M,B) = dist(A,B)",
        ] {
            assert!(!is_metric_goal(g), "{g} should stay on DDAR");
        }
    }

    #[test]
    fn reconcile_rejects_a_proof_the_figure_refutes() {
        let mut sol = blank_solution();
        sol.proved = true;
        sol.proof = Some("001. something".into());
        sol.goal_holds_numerically = Some(false);
        reconcile(&mut sol, true);
        assert!(!sol.proved);
        assert!(sol.proof.is_none());
        assert!(sol.note.contains("does not hold"), "{}", sol.note);
    }

    #[test]
    fn reconcile_fills_in_a_missing_proof_text() {
        let mut sol = blank_solution();
        sol.proved = true;
        reconcile(&mut sol, true);
        assert!(sol.proved);
        assert!(sol.proof.as_deref().unwrap_or("").contains("could not be extracted"));

        let mut quiet = blank_solution();
        quiet.proved = true;
        reconcile(&mut quiet, false);
        assert!(quiet.proof.is_none());
    }

    /// A metric goal that holds in every sampled figure but has no Euclidean
    /// proof is NOT proved: it gets its own status, the numeric evidence, and
    /// no proof text.
    #[test]
    fn numeric_only_metric_goal_is_not_proved() {
        let sol = solve(NUMERIC_ONLY, &SolveOptions::default()).unwrap();
        assert_eq!(sol.method, Method::Euclidean);
        assert!(!sol.proved, "{}", sol.note);
        assert_eq!(sol.status, Status::HoldsNumerically, "{}", sol.note);
        assert_eq!(sol.goal_holds_numerically, Some(true));
        assert!(sol.numeric_samples.is_some_and(|n| n >= 8), "{:?}", sol.numeric_samples);
        assert!(sol.proof.is_none(), "{:?}", sol.proof);
        assert!(sol.proof_steps.is_none());
        let ev = sol.numeric_evidence.as_deref().unwrap_or("");
        assert!(ev.contains("not a proof"), "{ev}");
        assert!(sol.note.contains("not proved"), "{}", sol.note);

        let json = serde_json::to_value(&sol).unwrap();
        assert_eq!(json["proved"], false);
        assert_eq!(json["status"], "holds-numerically");
        assert_eq!(json["goal_holds_numerically"], true);
        assert!(json["numeric_samples"].as_u64().is_some_and(|n| n >= 8));
        assert!(json["proof"].is_null());
    }

    #[test]
    fn numeric_only_metric_goal_is_not_proved_by_solve_best() {
        let sol = solve_best(NUMERIC_ONLY, &SolveOptions::default(), Duration::from_secs(2)).unwrap();
        assert!(!sol.proved, "{}", sol.note);
        assert_eq!(sol.status, Status::HoldsNumerically);
    }

    #[test]
    fn statuses_agree_with_proved() {
        let proved = solve(include_str!("../../alphageometry-rs/examples/metric/stewart.geo"), &SolveOptions::default()).unwrap();
        assert_eq!(proved.status, Status::Proved);
        assert!(proved.numeric_evidence.is_none());
        let refuted = solve(DIST_CONST_FREE, &SolveOptions::default()).unwrap();
        assert_eq!(refuted.status, Status::Refuted);
        assert!(refuted.proof.is_none());
        assert!(refuted.numeric_evidence.as_deref().unwrap_or("").contains("counterexample"));
        let ddar_false = solve(GENERIC_RIGHT_ANGLE, &SolveOptions::default()).unwrap();
        assert!(!ddar_false.proved);
        assert_ne!(ddar_false.status, Status::Proved);
        assert_ne!(ddar_false.status, Status::HoldsNumerically);
        let ddar_true = solve(THALES_ANGLE, &SolveOptions::default()).unwrap();
        assert_eq!(ddar_true.status, Status::Proved);
    }

    #[test]
    fn reconcile_strips_proof_text_from_an_unproved_result() {
        let mut sol = blank_solution();
        sol.proof = Some("NUMERICAL CHECK ...".into());
        sol.proof_steps = Some(3);
        sol.goal_holds_numerically = Some(true);
        reconcile(&mut sol, true);
        assert!(sol.proof.is_none());
        assert!(sol.proof_steps.is_none());
        assert_eq!(sol.status, Status::NotProved, "no numeric sampling ran");
        sol.numeric_samples = Some(48);
        reconcile(&mut sol, true);
        assert_eq!(sol.status, Status::HoldsNumerically);
    }

    #[test]
    fn solve_within_honours_the_deadline() {
        let t = Instant::now();
        let sol =
            solve_within(BUTTERFLY, &SolveOptions::default(), Some(Duration::from_millis(300)))
                .unwrap();
        let took = t.elapsed();
        assert!(took < Duration::from_secs(2), "took {took:?}");
        assert!(!sol.proved);
        assert!(sol.note.contains("time limit"), "{}", sol.note);
    }

    fn blank_solution() -> Solution {
        Solution {
            input: String::new(),
            low_level: String::new(),
            proved: false,
            status: Status::NotProved,
            method: Method::AuxSearch,
            proof: None,
            numeric_evidence: None,
            numeric_samples: None,
            svg: String::new(),
            aux_constructions: Vec::new(),
            constructions: Vec::new(),
            goal: None,
            goal_holds_numerically: None,
            elapsed_secs: 0.0,
            note: String::new(),
            proof_steps: None,
            examined: None,
            figure: None,
        }
    }
}
