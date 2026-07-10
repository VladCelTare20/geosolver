//! Command-line runner for the DDAR engine.
//!
//! Modes:
//!   ddar --bench                 solve the bundled IMO set with timings
//!   ddar --max <prog|file>       UNIVERSAL solver: throw maximum effort at any
//!                                problem (DDAR, else iterative-deepening aux
//!                                search across all cores) — assumes nothing
//!                                about how hard it is
//!   ddar --geo <prog|file>       compile a high-level description and solve it
//!                                (same solver as --max, quieter)
//!   ddar --metric <prog|file>    prove a metric (length) goal with a classical
//!                                EUCLIDEAN proof citing named theorems
//!   ddar --geo-show <prog|file>  compile and print the low-level form
//!   ddar --constructions         list the high-level language vocabulary
//!   ddar --theorems              list the classical-theorem proof library
//!   ddar --aux "<problem>"       solve a low-level problem at MAX (aux search)
//!   ddar --explore               report which bundled aux points are load-bearing
//!   ddar --batch [dir]           run the universal MAX solver on every problem
//!                                in dir, in parallel across all cores
//!   ddar --bench-aux             leave-one-out aux rediscovery benchmark
//!   ddar --verify-warm           check warm-start == full solve over the corpus
//!   ddar "<problem>"             solve a single low-level problem string
//!
//! Options (combine with any solving mode):
//!   --proof                      print a numbered proof after solving
//!   --svg <file>                 write an SVG figure (figure + construction panel)
//!   --theme dark|light           figure color scheme (default: dark, like AG1)
//!   --title <text>               title shown atop the figure's construction panel
//!
//! Env:
//!   AUX_NO_WARM=1   disable warm-start candidate evaluation (for A/B timing)
//!
//! For `--geo`/`--geo-show`, the argument may be an inline program or a path to
//! a `.geo` file. Inline programs may use `;` to separate statements.

use std::time::Instant;

use ddar::aux_search::{
    apply_constructions, candidates, solve_max, solve_with_aux, strip_point, AuxProof, WarmBase,
};
use ddar::geo::{self, CONSTRUCTIONS, RELATIONS};
use ddar::metric;
use ddar::runner::{parse_dataset, solve_problem, solve_problem_with_proof};
use ddar::svg::{self, FigureOptions, Theme};
use ddar::Problem;

const DATASET: &str = include_str!("../../problems.tsv");

struct Opts {
    proof: bool,
    svg_path: Option<String>,
    theme: Theme,
    title: Option<String>,
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();

    // Split flags/options from positional arguments.
    let mut opts = Opts {
        proof: false,
        svg_path: None,
        theme: Theme::Dark,
        title: None,
    };
    let mut mode: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut it = raw.into_iter().peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--proof" => opts.proof = true,
            "--svg" => {
                opts.svg_path = Some(it.next().unwrap_or_else(|| fail("--svg needs a file path")))
            }
            "--theme" => {
                opts.theme = match it.next().as_deref() {
                    Some("dark") => Theme::Dark,
                    Some("light") => Theme::Light,
                    _ => fail("--theme needs 'dark' or 'light'"),
                }
            }
            "--title" => opts.title = Some(it.next().unwrap_or_else(|| fail("--title needs text"))),
            "--bench" | "--geo" | "--geo-show" | "--aux" | "--explore" | "--constructions"
            | "--bench-aux" | "--verify-warm" | "--metric" | "--batch" | "--max" | "--theorems" => {
                mode = Some(arg)
            }
            other => positional.push(other.to_string()),
        }
    }

    match mode.as_deref() {
        None if positional.is_empty() => bench(&opts),
        None => solve_low_level(&positional[0], &opts),
        Some("--bench") => bench(&opts),
        Some("--constructions") => list_constructions(&opts),
        Some("--theorems") => list_theorems(),
        Some("--metric") => metric_solve(&read_program(positional.first())),
        Some("--geo") => run_geo(&read_program(positional.first()), false, false, &opts),
        Some("--geo-show") => run_geo(&read_program(positional.first()), true, false, &opts),
        Some("--max") => run_geo(&read_program(positional.first()), false, true, &opts),
        Some("--aux") => aux(
            positional
                .first()
                .unwrap_or_else(|| fail("usage: ddar --aux \"<problem>\"")),
            &opts,
        ),
        Some("--explore") => explore(&opts),
        Some("--bench-aux") => bench_aux(),
        Some("--verify-warm") => verify_warm(),
        Some("--batch") => batch(
            positional
                .first()
                .map(String::as_str)
                .unwrap_or("examples/olympiad"),
        ),
        _ => unreachable!(),
    }
}

/// Warn about options that a mode cannot honor (instead of ignoring them).
fn warn_unsupported(opts: &Opts, mode: &str) {
    if opts.proof {
        eprintln!("warning: --proof is not supported with {mode}; ignoring");
    }
    if opts.svg_path.is_some() {
        eprintln!("warning: --svg is not supported with {mode}; ignoring");
    }
    if opts.title.is_some() {
        eprintln!("warning: --title is not supported with {mode}; ignoring");
    }
}

/// After a successful aux search, print the proof of the augmented problem.
fn print_aux_proof(problem: &Problem, proof: &AuxProof) {
    let augmented = apply_constructions(problem, &proof.constructions);
    match solve_problem_with_proof(&augmented) {
        Ok(Some(report)) => println!("\n{report}"),
        Ok(None) => eprintln!("warning: proof extraction failed on the augmented problem"),
        Err(e) => eprintln!("warning: {e}"),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

/// Read a program argument that is either an inline string or a file path.
fn read_program(arg: Option<&String>) -> String {
    let arg = arg.unwrap_or_else(|| fail("usage: ddar --geo <program-or-file>"));
    if std::path::Path::new(arg).is_file() {
        std::fs::read_to_string(arg).unwrap_or_else(|e| fail(&format!("reading {arg}: {e}")))
    } else {
        arg.clone()
    }
}

fn write_svg_if_requested(problem: &Problem, opts: &Opts) {
    if let Some(path) = &opts.svg_path {
        let fig = FigureOptions {
            theme: opts.theme,
            title: opts.title.clone(),
            panel: true,
            status: None,
            aux_from: None,
        };
        let doc = svg::render_with(problem, &fig);
        std::fs::write(path, doc).unwrap_or_else(|e| fail(&format!("writing {path}: {e}")));
        println!("Figure written to {path}");
    }
}

/// Solve a low-level problem, honoring --proof / --svg.
fn solve_and_report(problem: &Problem, opts: &Opts) -> bool {
    write_svg_if_requested(problem, opts);
    let start = Instant::now();
    if opts.proof {
        match solve_problem_with_proof(problem) {
            Ok(Some(report)) => {
                println!("Proven :-)  ({:.3}s)\n", start.elapsed().as_secs_f64());
                println!("{report}");
                true
            }
            Ok(None) => false,
            Err(e) => fail(&e),
        }
    } else {
        match solve_problem(problem) {
            Ok(true) => {
                println!("Proven :-)  ({:.3}s)", start.elapsed().as_secs_f64());
                true
            }
            Ok(false) => false,
            Err(e) => fail(&e),
        }
    }
}

fn solve_low_level(problem_str: &str, opts: &Opts) {
    let problem = Problem::parse(problem_str).unwrap_or_else(|e| fail(&e));
    if !solve_and_report(&problem, opts) {
        println!("Not proven (missing an auxiliary point?)");
        std::process::exit(1);
    }
}

fn list_constructions(opts: &Opts) {
    warn_unsupported(opts, "--constructions");
    println!("Constructions (value expressions):\n");
    for c in CONSTRUCTIONS {
        println!("  {}", c.summary);
    }
    println!("\nRelations (in `point:` constraints and `prove`):\n");
    for r in RELATIONS {
        println!("  {r}");
    }
    println!(
        "\nStatements (separated by newlines or `;`, `#` starts a comment):\n\
         \x20 name[, name ...] = expr        bind point(s) / object(s)\n\
         \x20 name = point: c1, c2, ...      a point solved to satisfy constraints\n\
         \x20 prove <relation>              the goal (also `goal:` or `? <relation>`)\n\
         \nExpressions nest: foot(A, line(B,C)), midpoint(midpoint(A,B), C), meet(l, c)."
    );
}

/// List the classical-theorem library, grouped by category.
fn list_theorems() {
    use ddar::synthetic::THEOREMS;
    println!("Classical Euclidean theorem library:\n");
    let mut current = "";
    for t in THEOREMS {
        if t.category != current {
            println!("\n  ── {} ──", t.category);
            current = t.category;
        }
        println!("    {:<42} {}", t.name, t.statement);
    }
    let count = |tag: &str| THEOREMS.iter().filter(|t| t.statement.contains(tag)).count();
    let (additive, ratio, ddar, catalogued) = (
        count("[additive"),
        count("[ratio"),
        count("[DDAR"),
        count("[catalogued"),
    );
    let derived = THEOREMS.iter().filter(|t| t.statement.contains("derived")).count();
    println!(
        "\n  {} theorems catalogued.\n    • {additive} operational in the additive (squared-length) engine\n    \
         • {ratio} operational in the multiplicative (ratio) engine — the famous results DERIVED\n      \
         from similar triangles ({derived} explicitly marked [derived], never self-cited)\n    \
         • {ddar} provable by the DDAR deductive closure as an angle/ratio goal (--proof)\n    \
         • {catalogued} catalogued (recognised; detector not yet wired).\n  \
         A named theorem is never used to prove itself: when a result IS the goal, the engine\n  \
         derives it from elementary steps instead of citing it.",
        THEOREMS.len()
    );
}

/// Verify a *metric* goal (specific lengths/angles/areas, sums of squares, any
/// algebraic relation) that DDAR cannot express — see [`metric`]. The program is
/// a coordinate-free construction plus a `prove <expr> = <expr>` line, e.g.
///   O = free; A = point: dist(O,A)=6; ... ; prove dist(A,C)^2 + dist(B,D)^2 = 144
fn metric_solve(src: &str) {
    let mut cons: Vec<String> = Vec::new();
    let mut goal: Option<String> = None;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or(""); // strip trailing comment
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
    let goal = goal.unwrap_or_else(|| {
        fail("--metric needs a `prove <equation>` line, e.g. `prove dist(A,C)^2 + dist(B,D)^2 = 144`")
    });
    let start = Instant::now();
    match metric::solve(&cons.join("\n"), &goal, 48) {
        Ok(report) => println!("{report}\n  ({:.3}s)", start.elapsed().as_secs_f64()),
        Err(e) => fail(&e),
    }
}

fn run_geo(src: &str, show_only: bool, verbose: bool, opts: &Opts) {
    let compiled = geo::compile(src).unwrap_or_else(|e| fail(&e));

    if show_only {
        println!("{}", compiled.problem.to_ag_string());
        write_svg_if_requested(&compiled.problem, opts);
        return;
    }

    if compiled.goal_numerically_holds == Some(false) {
        println!(
            "Warning: the goal does not hold in the sampled figure — the statement \
             appears to be false. Proceeding anyway."
        );
    }

    write_svg_if_requested(&compiled.problem, opts);

    let start = Instant::now();
    if opts.proof {
        match solve_problem_with_proof(&compiled.problem) {
            Ok(Some(report)) => {
                println!("Proven :-)  ({:.3}s)\n", start.elapsed().as_secs_f64());
                println!("{report}");
                return;
            }
            Ok(None) => {}
            Err(e) => fail(&e),
        }
    } else if solve_problem(&compiled.problem).unwrap_or_else(|e| fail(&e)) {
        println!("Proven :-)  ({:.3}s)", start.elapsed().as_secs_f64());
        return;
    }

    println!(
        "DDAR alone did not prove it; escalating the universal MAX search across all cores..."
    );
    let (result, stats) = solve_max(&compiled.problem, verbose);
    match result {
        Some(proof) => {
            println!(
                "Proven with {} auxiliary construction(s) in {} DDAR runs ({:.3}s):",
                proof.constructions.len(),
                stats.runs,
                start.elapsed().as_secs_f64()
            );
            for c in &proof.constructions {
                println!("  + {} = {}", c.name, c.desc);
            }
            if opts.proof {
                print_aux_proof(&compiled.problem, &proof);
            }
        }
        None => {
            println!(
                "No proof found (tried {} DDAR runs, {:.3}s).",
                stats.runs,
                start.elapsed().as_secs_f64()
            );
            std::process::exit(1);
        }
    }
}

/// One problem's outcome in a parallel batch run.
struct BatchResult {
    name: String,
    solved: bool,
    constructions: Vec<String>,
    runs: usize,
    secs: f64,
    note: String,
}

/// Batch mode: run the **universal MAX solver** ([`solve_max`]) on every `.geo`
/// problem in `dir`, fanning the problems across all CPU cores. Every problem
/// gets the same maximum effort — nothing is assumed easy or hard. Reports
/// per-problem timing and the wall-clock vs. summed-CPU speedup.
fn batch(dir: &str) {
    use rayon::prelude::*;
    let mut paths: Vec<std::path::PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("geo"))
            .collect(),
        Err(e) => fail(&format!("reading {dir}: {e}")),
    };
    paths.sort();
    if paths.is_empty() {
        fail(&format!("no .geo problems found in {dir}"));
    }

    // Suppress panic noise from degenerate candidate constructions across all
    // worker threads for the duration of the batch.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        if std::env::var_os("DDAR_DEBUG_PANICS").is_some_and(|v| !v.is_empty()) {
            eprintln!("[ddar panic] {info}");
        }
    }));

    println!(
        "Universal MAX solve: {} problem(s), across {} cores.\n",
        paths.len(),
        rayon::current_num_threads()
    );

    let wall = Instant::now();
    let mut results: Vec<BatchResult> = paths.par_iter().map(|p| solve_file(p)).collect();
    let wall = wall.elapsed().as_secs_f64();
    std::panic::set_hook(prev);

    results.sort_by(|a, b| a.name.cmp(&b.name));
    let (mut solved, mut cpu) = (0usize, 0.0f64);
    for r in &results {
        cpu += r.secs;
        let status = if r.solved {
            solved += 1;
            "SOLVED"
        } else {
            "!!! UNSOLVED"
        };
        println!(
            "  {:<18} {:>9}  {:>12}   {}",
            r.name,
            format!("{:.2}s", r.secs),
            format!("{} DDAR runs", r.runs),
            status
        );
        for c in &r.constructions {
            println!("        + {c}");
        }
        if !r.note.is_empty() {
            println!("        ({})", r.note);
        }
    }
    println!(
        "\n{solved}/{} solved  |  wall {:.2}s  |  CPU {:.2}s  |  parallel speedup {:.1}×",
        results.len(),
        wall,
        cpu,
        cpu / wall.max(1e-9)
    );
    if solved != results.len() {
        std::process::exit(1);
    }
}

/// Solve one `.geo` problem file with the universal MAX solver (DDAR directly,
/// else the iterative-deepening auxiliary search).
fn solve_file(path: &std::path::Path) -> BatchResult {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("?")
        .to_string();
    let start = Instant::now();
    let mk = |solved, constructions, runs, note: String| BatchResult {
        name: name.clone(),
        solved,
        constructions,
        runs,
        secs: start.elapsed().as_secs_f64(),
        note,
    };
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => return mk(false, vec![], 0, format!("read error: {e}")),
    };
    let compiled = match geo::compile(&src) {
        Ok(c) => c,
        Err(e) => return mk(false, vec![], 0, format!("compile error: {e}")),
    };
    if quiet_solve(&compiled.problem) {
        return mk(true, vec![], 1, "DDAR (no auxiliary point)".into());
    }
    let (res, stats) = solve_max(&compiled.problem, false);
    match res {
        Some(proof) => mk(
            true,
            proof
                .constructions
                .iter()
                .map(|c| format!("{} = {}", c.name, c.desc))
                .collect(),
            stats.runs,
            String::new(),
        ),
        None => mk(
            false,
            vec![],
            stats.runs,
            "no construction found in budget".into(),
        ),
    }
}

fn bench(opts: &Opts) {
    warn_unsupported(opts, "--bench");
    let entries = parse_dataset(DATASET);
    println!(
        "Running DDAR (Rust) on {} bundled IMO problems.\n",
        entries.len()
    );
    let mut total = std::time::Duration::ZERO;
    let mut proven = 0usize;
    let mut rows: Vec<(f64, String)> = Vec::new();
    for e in &entries {
        let problem = Problem::parse(e.problem).unwrap();
        let start = Instant::now();
        let ok = solve_problem(&problem).unwrap_or_else(|err| panic!("{}: {err}", e.name));
        let dt = start.elapsed();
        total += dt;
        if ok {
            proven += 1;
        }
        rows.push((dt.as_secs_f64(), e.name.to_string()));
        println!(
            "{:>10}  {:<10} {}",
            format!("{:.3}s", dt.as_secs_f64()),
            e.name,
            if ok { "Proven :-)" } else { "!!! NOT PROVEN" }
        );
    }
    println!(
        "\n{proven}/{} proven, total {:.3}s",
        entries.len(),
        total.as_secs_f64()
    );
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    println!("\nSlowest problems:");
    for (dt, name) in rows.iter().take(5) {
        println!("  {:>8.3}s  {name}", dt);
    }

    // Re-solve the whole set in parallel across all cores to show the wall-clock
    // an actual batch run achieves (the per-problem times above are serial).
    use rayon::prelude::*;
    let wall = Instant::now();
    let all_ok = entries
        .par_iter()
        .map(|e| solve_problem(&Problem::parse(e.problem).unwrap()).unwrap_or(false))
        .all(|ok| ok);
    let wall = wall.elapsed().as_secs_f64();
    println!(
        "\nParallel batch ({} cores): {:.3}s wall  ({:.1}× vs. serial {:.3}s)",
        rayon::current_num_threads(),
        wall,
        total.as_secs_f64() / wall.max(1e-9),
        total.as_secs_f64()
    );
    if proven != entries.len() || !all_ok {
        std::process::exit(1);
    }
}

fn aux(problem_str: &str, opts: &Opts) {
    let problem = Problem::parse(problem_str).unwrap_or_else(|e| fail(&e));
    write_svg_if_requested(&problem, opts);
    let start = Instant::now();
    let (result, stats) = solve_max(&problem, true);
    let dt = start.elapsed();
    match result {
        Some(proof) if proof.constructions.is_empty() => {
            println!(
                "Proven with no auxiliary points ({:.3}s).",
                dt.as_secs_f64()
            );
            if opts.proof {
                print_aux_proof(&problem, &proof);
            }
        }
        Some(proof) => {
            println!(
                "Proven with {} auxiliary construction(s) in {} DDAR runs ({:.3}s):",
                proof.constructions.len(),
                stats.runs,
                dt.as_secs_f64()
            );
            for c in &proof.constructions {
                println!("  + {} = {}", c.name, c.desc);
            }
            if opts.proof {
                print_aux_proof(&problem, &proof);
            }
        }
        None => {
            println!(
                "No proof found within budget ({} DDAR runs, {:.3}s).",
                stats.runs,
                dt.as_secs_f64()
            );
            std::process::exit(1);
        }
    }
}

/// Leave-one-out rediscovery benchmark: for every load-bearing point of the
/// bundled problems, strip it and measure whether the depth-1 aux search finds a
/// restoring construction, plus the total DDAR runs (a proxy for ranking
/// quality — fewer runs = the answer ranked higher). A single, honest number for
/// tracking the ranking heuristic.
fn bench_aux() {
    let entries = parse_dataset(DATASET);
    let start = Instant::now();
    let (mut total, mut solved, mut runs) = (0usize, 0usize, 0usize);
    for e in &entries {
        let p = match Problem::parse(e.problem) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if p.points.len() > 16 || !quiet_solve(&p) {
            continue;
        }
        let goal_pts: std::collections::HashSet<u32> = p
            .goal
            .as_ref()
            .map(|g| g.points.iter().copied().collect())
            .unwrap_or_default();
        for idx in 0..p.points.len() {
            if goal_pts.contains(&(idx as u32)) || p.points[idx].name.starts_with('_') {
                continue;
            }
            let stripped = strip_point(&p, &p.points[idx].name.clone());
            if stripped.goal.is_none() || quiet_solve(&stripped) {
                continue; // not load-bearing
            }
            total += 1;
            // A tight budget makes this a sharp *ranking* test: an instance is
            // only restored if its construction ranks within a few thousand
            // evaluations, so better ranking → more restored / fewer runs.
            let (res, stats) = solve_with_aux(&stripped, 1, 4_000, false);
            if res.is_some() {
                solved += 1;
            }
            runs += stats.runs;
        }
    }
    println!(
        "Leave-one-out rediscovery: {solved}/{total} load-bearing points restored\n\
         {runs} total DDAR runs ({:.1} avg/instance), {:.1}s",
        runs as f64 / total.max(1) as f64,
        start.elapsed().as_secs_f64()
    );
}

/// Exhaustively verify that warm-start candidate evaluation ([`WarmBase`]) gives
/// the SAME verdict as a full from-scratch solve, over every leave-one-out
/// candidate of the bundled + example problems. Warm-start is only sound to
/// enable if this reports zero mismatches.
fn verify_warm() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        if std::env::var_os("DDAR_DEBUG_PANICS").is_some_and(|v| !v.is_empty()) {
            eprintln!("[ddar panic] {info}");
        }
    }));
    let mut examples: Vec<Problem> = Vec::new();
    for e in parse_dataset(DATASET) {
        if let Ok(p) = Problem::parse(e.problem) {
            if p.points.len() <= 13 {
                examples.push(p);
            }
        }
    }
    for dir in ["examples", "examples/imo"] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for ent in rd.flatten() {
                if ent.path().extension().and_then(|s| s.to_str()) == Some("geo") {
                    if let Ok(src) = std::fs::read_to_string(ent.path()) {
                        if let Ok(c) = geo::compile(&src) {
                            examples.push(c.problem);
                        }
                    }
                }
            }
        }
    }
    let (mut checked, mut mism, mut warm_solves) = (0usize, 0usize, 0usize);
    for p in &examples {
        if p.points.len() > 14 || !quiet_solve(p) {
            continue;
        }
        let goal_pts: std::collections::HashSet<u32> = p
            .goal
            .as_ref()
            .map(|g| g.points.iter().copied().collect())
            .unwrap_or_default();
        for idx in 0..p.points.len() {
            if goal_pts.contains(&(idx as u32)) || p.points[idx].name.starts_with('_') {
                continue;
            }
            let stripped = strip_point(p, &p.points[idx].name.clone());
            if stripped.goal.is_none() {
                continue;
            }
            let cands = candidates(&stripped, stripped.points.len() <= 14);
            let Some(warm) = WarmBase::new(&stripped) else {
                continue;
            };
            for cand in cands.iter().take(200) {
                let full = quiet_solve(&apply_constructions(&stripped, std::slice::from_ref(cand)));
                let w = warm.check(cand);
                checked += 1;
                if w {
                    warm_solves += 1;
                }
                if w != full {
                    mism += 1;
                    if mism <= 12 {
                        eprintln!("MISMATCH warm={w} full={full}: {}", cand.desc);
                    }
                }
            }
        }
        eprint!("\r{checked} checks, {mism} mismatches, {warm_solves} warm-solves   ");
    }
    std::panic::set_hook(prev);
    println!(
        "\nverify-warm: {checked} candidate checks, {mism} mismatches ({} agreement)",
        if mism == 0 {
            "100%".to_string()
        } else {
            format!(
                "{:.4}%",
                100.0 * (checked - mism) as f64 / checked.max(1) as f64
            )
        }
    );
    if mism != 0 {
        std::process::exit(1);
    }
}

fn quiet_solve(p: &Problem) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        solve_problem(p).unwrap_or(false)
    }))
    .unwrap_or(false)
}

/// For each bundled problem, remove each point in turn and report which ones are
/// "load-bearing" (their removal makes DDAR fail) — good aux-search targets.
fn explore(opts: &Opts) {
    use ddar::aux_search::strip_point;
    warn_unsupported(opts, "--explore");
    let entries = parse_dataset(DATASET);
    for e in &entries {
        let problem = Problem::parse(e.problem).unwrap();
        if !solve_problem(&problem).unwrap_or(false) {
            continue;
        }
        let names: Vec<String> = problem.points.iter().map(|p| p.name.clone()).collect();
        let mut load_bearing = Vec::new();
        for name in &names {
            let stripped = strip_point(&problem, name);
            if stripped.goal.is_none() {
                continue;
            }
            let solved = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                solve_problem(&stripped).unwrap_or(false)
            }))
            .unwrap_or(false);
            if !solved {
                load_bearing.push(name.clone());
            }
        }
        if !load_bearing.is_empty() {
            println!("{:<10} load-bearing: {}", e.name, load_bearing.join(", "));
        }
    }
}
