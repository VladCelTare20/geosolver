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
use ddar::{bench, corpus};
use ddar::metric;
use ddar::runner::{parse_dataset, solve_problem, solve_problem_with_proof};
use ddar::svg::{self, FigureOptions, Theme};
use ddar::Problem;

const DATASET: &str = include_str!("../../problems.tsv");

const USAGE: &str = "\
usage: ddar [mode] [options] [<problem|program|file>]

Modes:
  --bench                 solve the bundled IMO set with timings (default)
  --max <prog|file>       universal solver: DDAR, else the aux search at max effort
  --geo <prog|file>       compile a high-level .geo program and solve it
  --metric <prog|file>    prove a metric (length) goal with a Euclidean proof
  --geo-show <prog|file>  compile and print the low-level form
  --constructions         list the high-level language vocabulary
  --theorems              list the classical-theorem proof library
  --aux \"<problem>\"       solve a low-level problem with the aux search
  --explore               report which bundled aux points are load-bearing
  --batch [dir]           run the universal solver on every problem in dir
  --bench-aux             leave-one-out aux rediscovery benchmark
  --verify-warm           check warm-start == full solve over the corpus
  --corpus <file>         solve-rate benchmark over an AG1-format corpus
                          (corpus/imo_ag_30.txt, corpus/jgex_ag_231.txt):
                          DDAR then the aux search per problem, each in its
                          own process with a hard wall-clock deadline
  --corpus-check <file>   translate every corpus problem (no solving); report
                          translation errors and numerically false goals
  --corpus-show <file> <name>  print one translated problem in low-level form
  --corpus-one <file> <name>   solve one corpus problem in-process (--proof)
  --fuzz-false <file|dir> soundness fuzzer: false variants of every corpus
                          problem (mutated goal, dropped hypothesis, degenerate
                          figure) through DDAR + aux search, one process each;
                          a directory fuzzes the metric goals of its .geo
                          programs through the classical provers instead;
                          exits 2 if any is proved
  --fuzz-one <cases> <name>    run one fuzz case in-process (child of --fuzz-false)
  \"<problem>\"             solve a single low-level problem string

Options:
  --proof                 print a numbered proof after solving
  --human                 print the human-style proof (EN) after solving
  --human-json            print the human-style proof as JSON after solving
  --human-stats <file>    (--corpus) write the human-proof writer's metrics per proof
  --svg <file>            write an SVG figure
  --theme dark|light      figure color scheme (default dark)
  --title <text>          title atop the figure's construction panel
  --trig <mode>           law of sines in the DDAR closure: off, fallback
                          (default: only when a length goal is left unproved
                          by the base closure), lazy (every closure, after its
                          trig-free fixpoint), always; also env GEO_TRIG
  -h, --help              print this help

Corpus options:
  --budget <secs>         wall-clock budget per problem (default 60)
  --jobs <n>              problems solved concurrently (default 4)
  --threads <n>           solver threads per problem (default 3); keep
                          jobs x threads within the machine's thread budget
  --out <file.tsv>        write per-problem results (default: stdout only)
  --only <a,b,...>        run only these problem names
  --proofs <dir>          write each proof found to <dir>/<name>.txt
  --mem-mb <n>            kill a problem whose memory exceeds n MiB (default 4096)

Fuzz options (--fuzz-false; also --budget [default 15], --jobs, --threads,
--only, --mem-mb, --out <file.tsv> [cases and proofs are written beside it]):
  --per <n>               goal mutations per problem (default 8); hypothesis
                          drops get n/2, degenerate figures n/4
  --seed <k>              generation seed (default 1)
  --samples <n>           figures a variant must be false on (default 5)
";

const MODES: &[&str] = &[
    "--bench", "--geo", "--geo-show", "--aux", "--explore", "--constructions", "--bench-aux",
    "--verify-warm", "--metric", "--batch", "--max", "--theorems", "--corpus", "--corpus-one",
    "--corpus-check", "--corpus-show", "--fuzz-false", "--fuzz-one",
];

enum ArgKind {
    Help,
    Mode,
    Unknown,
    Positional,
}

/// Classify an argument that is not an option's value.
fn classify_arg(arg: &str) -> ArgKind {
    match arg {
        "-h" | "--help" => ArgKind::Help,
        a if MODES.contains(&a) => ArgKind::Mode,
        a if a.starts_with("--") || (a.starts_with('-') && a.len() == 2) => ArgKind::Unknown,
        _ => ArgKind::Positional,
    }
}

struct Opts {
    proof: bool,
    human: bool,
    human_json: bool,
    human_stats: Option<String>,
    svg_path: Option<String>,
    theme: Theme,
    title: Option<String>,
    corpus: CorpusOpts,
}

struct CorpusOpts {
    budget: f64,
    jobs: usize,
    threads: usize,
    out: Option<String>,
    only: Option<Vec<String>>,
    proofs: Option<String>,
    mem_mb: u64,
    budget_given: bool,
    per: usize,
    seed: u64,
    samples: usize,
}

fn num_arg<T: std::str::FromStr>(v: Option<String>, flag: &str) -> T {
    v.as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| fail(&format!("{flag} needs a number")))
}

fn main() {
    let raw: Vec<String> = std::env::args().skip(1).collect();

    // Split flags/options from positional arguments.
    let mut opts = Opts {
        proof: false,
        human: false,
        human_json: false,
        human_stats: None,
        svg_path: None,
        theme: Theme::Dark,
        title: None,
        corpus: CorpusOpts {
            budget: 60.0,
            jobs: 4,
            threads: 3,
            out: None,
            only: None,
            proofs: None,
            mem_mb: 4096,
            budget_given: false,
            per: 8,
            seed: 1,
            samples: 5,
        },
    };
    let mut mode: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut it = raw.into_iter().peekable();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--proof" => opts.proof = true,
            "--human" => opts.human = true,
            "--human-json" => opts.human_json = true,
            "--human-stats" => {
                opts.human_stats = Some(it.next().unwrap_or_else(|| fail("--human-stats needs a file (or - in a child)")))
            }
            "--trig" => {
                let m = it.next().unwrap_or_else(|| fail("--trig needs off|fallback|lazy|always"));
                if !matches!(m.as_str(), "off" | "fallback" | "lazy" | "always") {
                    fail("--trig needs off|fallback|lazy|always");
                }
                std::env::set_var("GEO_TRIG", m);
            }
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
            "--budget" => {
                opts.corpus.budget = num_arg(it.next(), "--budget");
                opts.corpus.budget_given = true;
            }
            "--per" => opts.corpus.per = num_arg::<usize>(it.next(), "--per").max(1),
            "--seed" => opts.corpus.seed = num_arg(it.next(), "--seed"),
            "--samples" => opts.corpus.samples = num_arg::<usize>(it.next(), "--samples").max(2),
            "--jobs" => opts.corpus.jobs = num_arg::<usize>(it.next(), "--jobs").max(1),
            "--threads" => opts.corpus.threads = num_arg::<usize>(it.next(), "--threads").max(1),
            "--mem-mb" => opts.corpus.mem_mb = num_arg(it.next(), "--mem-mb"),
            "--out" => opts.corpus.out = Some(it.next().unwrap_or_else(|| fail("--out needs a file"))),
            "--proofs" => {
                opts.corpus.proofs = Some(it.next().unwrap_or_else(|| fail("--proofs needs a directory")))
            }
            "--only" => {
                opts.corpus.only = Some(
                    it.next()
                        .unwrap_or_else(|| fail("--only needs a comma-separated list"))
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                )
            }
            other => match classify_arg(other) {
                ArgKind::Help => {
                    print!("{USAGE}");
                    return;
                }
                ArgKind::Mode => mode = Some(arg),
                ArgKind::Unknown => fail(&format!("unknown option {other} (see ddar --help)")),
                ArgKind::Positional => positional.push(arg),
            },
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
        Some("--corpus") => corpus_bench(positional.first(), &opts),
        Some("--corpus-check") => corpus_check(positional.first(), &opts),
        Some("--corpus-show") => corpus_show(positional.first(), positional.get(1)),
        Some("--corpus-one") => corpus_one(positional.first(), positional.get(1), &opts),
        Some("--fuzz-false") => fuzz_false(positional.first(), &opts),
        Some("--fuzz-one") => fuzz_one(positional.first(), positional.get(1), &opts),
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

/// Prove a *metric* goal (specific lengths/angles/areas, sums of squares, any
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
    match metric::solve(&cons.join("\n"), &goal, metric::DEFAULT_SAMPLES) {
        Ok(report) => println!("{report}\n  ({:.3}s)", start.elapsed().as_secs_f64()),
        Err(e @ (metric::MetricError::Refuted(_) | metric::MetricError::NoProof { .. })) => {
            println!("{}\n  ({:.3}s)", e.report(), start.elapsed().as_secs_f64());
            std::process::exit(1);
        }
        Err(e) => fail(e.message()),
    }
}

fn run_geo(src: &str, show_only: bool, verbose: bool, opts: &Opts) {
    let compiled = match geo::compile(src) {
        Ok(c) => c,
        Err(e) if e.is_metric_goal() && !show_only => {
            println!("Metric goal (not a DDAR predicate) — using the metric prover.");
            return metric_solve(src);
        }
        Err(e) => fail(e.message()),
    };

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
    let want_human = opts.human || opts.human_json;
    if opts.proof || want_human {
        match solve_problem_with_proof(&compiled.problem) {
            Ok(Some(report)) => {
                println!("Proven :-)  ({:.3}s)\n", start.elapsed().as_secs_f64());
                if opts.proof {
                    println!("{report}");
                }
                if want_human {
                    human_report("geo", 0, &compiled.problem, &[], compiled.problem.points.len(), opts);
                }
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
            if want_human {
                let aug = apply_constructions(&compiled.problem, &proof.constructions);
                let aux: Vec<String> = proof.constructions.iter().map(|c| format!("{} = {}", c.name, c.desc)).collect();
                human_report("geo", 0, &aug, &aux, compiled.problem.points.len(), opts);
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
    // Mute panic noise from degenerate candidate constructions across all
    // worker threads for the duration of the batch.
    ddar::quiet_panic::quiet(|| batch_quiet(dir))
}

fn batch_quiet(dir: &str) {
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


    println!(
        "Universal MAX solve: {} problem(s), across {} cores.\n",
        paths.len(),
        rayon::current_num_threads()
    );

    let wall = Instant::now();
    let mut results: Vec<BatchResult> = paths.par_iter().map(|p| solve_file(p)).collect();
    let wall = wall.elapsed().as_secs_f64();

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
    ddar::quiet_panic::quiet(verify_warm_quiet)
}

fn verify_warm_quiet() {
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

fn load_corpus(file: Option<&String>, only: Option<&Vec<String>>) -> Vec<(String, String)> {
    let file = file.unwrap_or_else(|| fail("usage: ddar --corpus <file>"));
    let text =
        std::fs::read_to_string(file).unwrap_or_else(|e| fail(&format!("reading {file}: {e}")));
    let all = corpus::read_corpus(&text).unwrap_or_else(|e| fail(&e));
    match only {
        None => all,
        Some(names) => {
            for n in names {
                if !all.iter().any(|(m, _)| m == n) {
                    fail(&format!("--only: no problem named `{n}` in {file}"));
                }
            }
            all.into_iter().filter(|(n, _)| names.contains(n)).collect()
        }
    }
}

fn corpus_bench(file: Option<&String>, opts: &Opts) {
    let c = &opts.corpus;
    let problems = load_corpus(file, c.only.as_ref());
    let file = file.expect("checked by load_corpus");
    let cfg = bench::RunConfig {
        exe: std::env::current_exe().unwrap_or_else(|e| fail(&format!("current_exe: {e}"))),
        corpus: file.into(),
        budget: std::time::Duration::from_secs_f64(c.budget),
        jobs: c.jobs,
        threads: c.threads,
        mem_mb: c.mem_mb,
        grace: std::time::Duration::from_secs_f64((c.budget * 0.1).max(5.0)),
        proofs_dir: c.proofs.as_ref().map(Into::into),
        child_flag: "--corpus-one",
        human_stats: opts.human_stats.is_some(),
    };
    eprintln!(
        "corpus {file}: {} problems, budget {}s, {} jobs x {} threads",
        problems.len(),
        c.budget,
        c.jobs,
        c.threads
    );
    let start = Instant::now();
    let progress = |k: usize, n: usize, o: &bench::Outcome| {
        eprintln!(
            "[{k:>3}/{n}] {:<44} {:<15} {:>7.2}s {}{}",
            o.name,
            o.status,
            o.secs,
            if o.proved { format!("{} ", o.method) } else { String::new() },
            if o.aux.is_empty() { String::new() } else { format!("[{}]", o.aux.join("; ")) }
        )
    };
    let results = bench::run_corpus(&cfg, &problems, &progress);
    let mut tsv = String::from(bench::TSV_HEADER);
    tsv.push('\n');
    for o in &results {
        tsv.push_str(&o.to_tsv());
        tsv.push('\n');
    }
    if let Some(out) = &c.out {
        std::fs::write(out, &tsv).unwrap_or_else(|e| fail(&format!("writing {out}: {e}")));
    } else {
        print!("{tsv}");
    }
    if let Some(hs) = &opts.human_stats {
        let mut body = String::from(HUMAN_HEADER);
        body.push('\n');
        for o in &results {
            for l in &o.human {
                body.push_str(l.strip_prefix("HUMAN\t").unwrap_or(l));
                body.push('\n');
            }
        }
        std::fs::write(hs, body).unwrap_or_else(|e| fail(&format!("writing {hs}: {e}")));
    }
    let count = |f: &dyn Fn(&bench::Outcome) -> bool| results.iter().filter(|o| f(o)).count();
    let n = results.len();
    eprintln!("\n== {file}: {n} problems, {:.0}s wall ==", start.elapsed().as_secs_f64());
    eprintln!("  parsed (figure built):   {}/{n}", count(&|o| o.parsed));
    eprintln!("  goal holds numerically:  {}/{n}", count(&|o| o.goal_numeric == Some(true)));
    eprintln!("  PROVED:                  {}/{n}", count(&|o| o.proved));
    eprintln!("    by DDAR alone:         {}", count(&|o| o.proved && o.method == "ddar"));
    eprintln!("    with aux points:       {}", count(&|o| o.proved && o.method == "aux"));
    let mut statuses: Vec<(String, usize)> = Vec::new();
    for o in &results {
        match statuses.iter_mut().find(|(s, _)| *s == o.status) {
            Some((_, k)) => *k += 1,
            None => statuses.push((o.status.clone(), 1)),
        }
    }
    statuses.sort_by_key(|s| std::cmp::Reverse(s.1));
    let st: Vec<String> = statuses.iter().map(|(s, k)| format!("{s} {k}")).collect();
    eprintln!("  status:                  {}", st.join(", "));
    if results.iter().any(|o| o.status == "UNSOUND") {
        eprintln!("  !! UNSOUND results present: a numerically false goal was proved");
        std::process::exit(2);
    }
}

fn corpus_check(file: Option<&String>, opts: &Opts) {
    let problems = load_corpus(file, opts.corpus.only.as_ref());
    let (mut ok, mut false_goal, mut errors) = (0, 0, 0);
    for (name, text) in &problems {
        match corpus::parse_problem(name, text).and_then(|p| corpus::translate(&p, 1, 200)) {
            Ok(t) if t.goal_holds => {
                ok += 1;
                println!("ok          {name}  (seed {}, {} points)", t.seed, t.problem.points.len());
            }
            Ok(t) => {
                false_goal += 1;
                println!("GOAL-FALSE  {name}  ({} points)", t.problem.points.len());
            }
            Err(e) => {
                errors += 1;
                println!("ERROR       {name}  {e}");
            }
        }
    }
    eprintln!(
        "{} problems: {ok} ok, {false_goal} goal numerically false, {errors} not translatable",
        problems.len()
    );
}

fn corpus_show(file: Option<&String>, name: Option<&String>) {
    let name = name.unwrap_or_else(|| fail("usage: ddar --corpus-show <file> <name>"));
    let problems = load_corpus(file, Some(&vec![name.clone()]));
    let (n, text) = &problems[0];
    println!("{text}\n");
    let t = corpus::parse_problem(n, text)
        .and_then(|p| corpus::translate(&p, 1, 200))
        .unwrap_or_else(|e| fail(&e));
    println!("{}", t.problem.to_ag_string());
    for g in &t.goals {
        let names: Vec<&str> = g.points.iter().map(|&i| t.problem.point_name(i)).collect();
        println!("goal: {} {}", g.name, names.join(" "));
    }
    println!("holds numerically: {}  (seed {})", t.goal_holds, t.seed);
}

fn corpus_one(file: Option<&String>, name: Option<&String>, opts: &Opts) {
    let file = file.unwrap_or_else(|| fail("usage: ddar --corpus-one <file> <name>"));
    let name = name.unwrap_or_else(|| fail("usage: ddar --corpus-one <file> <name>"));
    let o = bench::child_main(
        std::path::Path::new(file),
        name,
        std::time::Duration::from_secs_f64(opts.corpus.budget),
        opts.corpus.proofs.as_deref().map(std::path::Path::new),
    );
    println!("RESULT\t{}", o.to_tsv());
    if opts.human || opts.human_json || opts.human_stats.is_some() {
        for (k, (p, aux, first)) in o.proved_problems.iter().enumerate() {
            human_report(&o.name, k, p, aux, *first, opts);
        }
    }
    if opts.proof {
        for a in &o.aux {
            println!("aux: {a}");
        }
        if let Some(p) = &o.proof {
            println!("\n{p}");
        }
    }
}

const HUMAN_HEADER: &str = "name\tconj\tavailable\traw_steps\tderived\tblocks\tclaims\tsentences\tchain_links\tchains\tpure_chains\tpooled\tfallback_blocks\tfallback_facts\treproved\tsilent\tpruned\ttheorem\twords_en\tlines_en\tcitations\thuman_cost\tcheck_violations\ttimed_out\tmicros\tpanicked";

fn human_report(name: &str, k: usize, p: &Problem, aux: &[String], first: usize, opts: &Opts) {
    let infos = ddar::human::aux_infos(p, first, aux);
    let hopts = ddar::human::Opts::default();
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ddar::quiet_panic::quiet(|| ddar::human::for_problem(p, &infos, &hopts))
    }));
    let (hp, trace, deps) = match res {
        Ok(Some((hp, trace, deps, _))) => (Some(hp), Some(trace), deps),
        Ok(None) => (None, None, Vec::new()),
        Err(_) => {
            if opts.human_stats.is_some() {
                println!("HUMAN\t{name}\t{k}\tno\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\tno\t0\tyes");
            }
            return;
        }
    };
    let (Some(hp), Some(trace)) = (hp, trace) else { return };
    if opts.human_stats.is_some() {
        let m = &hp.metrics;
        println!(
            "HUMAN\t{name}\t{k}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\tno",
            if hp.available { "yes" } else { "no" },
            m.raw_steps,
            m.derived,
            m.blocks,
            m.claims,
            m.sentences,
            m.chain_links,
            m.chains,
            m.pure_chains,
            m.pooled,
            m.fallback_blocks,
            m.fallback_facts,
            m.reproved,
            m.silent,
            m.pruned,
            m.theorem,
            m.words_en,
            m.lines_en,
            m.citations,
            m.human_cost,
            m.check_violations,
            if m.timed_out { "yes" } else { "no" },
            m.micros
        );
    }
    if opts.human {
        println!("\n== human proof ({name}, conjunct {k}) ==");
        print!("{}", ddar::human::render_en(&trace, &hp, &infos));
    }
    if opts.human_json {
        let closure = trace.closure(&deps);
        println!("{}", ddar::human::view::engine_json(&trace, &hp, &closure));
    }
}

fn fuzz_false(file: Option<&String>, opts: &Opts) {
    use ddar::fuzz;
    let c = &opts.corpus;
    let geo_dir = file.filter(|f| std::path::Path::new(f).is_dir());
    let problems = match geo_dir {
        Some(dir) => geo_programs(std::path::Path::new(dir)),
        None => load_corpus(file, c.only.as_ref()),
    };
    let file = file.expect("checked by load_corpus");
    let budget = if c.budget_given { c.budget } else { 15.0 };
    let out = c
        .out
        .clone()
        .unwrap_or_else(|| format!("{}/ddar-fuzz-{}.tsv", std::env::temp_dir().display(), std::process::id()));
    let cases_path = format!("{out}.cases.txt");
    let proofs_dir = std::path::PathBuf::from(format!("{out}.proofs"));
    let start = Instant::now();
    let cfg = fuzz::Config {
        per: c.per,
        seed: c.seed,
        samples: c.samples,
    };
    let gen = if geo_dir.is_some() {
        fuzz::generate_geo(&problems, &cfg)
    } else {
        fuzz::generate(&problems, &cfg)
    };
    let by_kind: Vec<String> = fuzz::Kind::ALL
        .iter()
        .map(|k| format!("{} {}", k.tag(), gen.cases.iter().filter(|x| x.kind == *k).count()))
        .collect();
    eprintln!(
        "fuzz {file}: {} problems -> {} cases ({}) in {:.1}s; {} variants held on some figure and were dropped; {} problems skipped",
        problems.len(),
        gen.cases.len(),
        by_kind.join(", "),
        start.elapsed().as_secs_f64(),
        gen.not_false,
        gen.skipped.len()
    );
    for (n, why) in &gen.skipped {
        eprintln!("  skipped {n}: {why}");
    }
    std::fs::write(&cases_path, fuzz::corpus_text(&gen.cases))
        .unwrap_or_else(|e| fail(&format!("writing {cases_path}: {e}")));
    let _ = std::fs::remove_dir_all(&proofs_dir);
    let run = bench::RunConfig {
        exe: std::env::current_exe().unwrap_or_else(|e| fail(&format!("current_exe: {e}"))),
        corpus: cases_path.clone().into(),
        budget: std::time::Duration::from_secs_f64(budget),
        jobs: c.jobs,
        threads: c.threads,
        mem_mb: c.mem_mb,
        grace: std::time::Duration::from_secs_f64((budget * 0.1).max(5.0)),
        proofs_dir: Some(proofs_dir.clone()),
        child_flag: "--fuzz-one",
        human_stats: false,
    };
    eprintln!(
        "running {} cases, budget {budget}s, {} jobs x {} threads; cases in {cases_path}",
        gen.cases.len(),
        c.jobs,
        c.threads
    );
    let kinds: std::collections::HashMap<&str, &fuzz::Case> =
        gen.cases.iter().map(|k| (k.name.as_str(), k)).collect();
    let progress = |k: usize, n: usize, o: &bench::Outcome| {
        if let Some(case) = kinds.get(o.name.as_str()) {
            let v = fuzz::verdict(case, o);
            if v != "ok" || k.is_multiple_of(50) || k == n {
                eprintln!("[{k:>4}/{n}] {:<52} {:<11} {:<15} {v}", o.name, case.kind.tag(), o.status);
            }
        }
    };
    let pairs: Vec<(String, String)> = gen.cases.iter().map(|k| (k.name.clone(), k.text.clone())).collect();
    let results = bench::run_corpus(&run, &pairs, &progress);
    let mut tsv = String::from(fuzz::TSV_HEADER);
    tsv.push('\n');
    let mut unsound = Vec::new();
    let mut crashes = 0;
    let mut statuses: std::collections::BTreeMap<String, usize> = Default::default();
    for o in &results {
        let Some(case) = kinds.get(o.name.as_str()) else { continue };
        tsv.push_str(&fuzz::tsv_row(case, o));
        tsv.push('\n');
        *statuses.entry(format!("{}:{}", case.kind.tag(), o.status)).or_default() += 1;
        match fuzz::verdict(case, o) {
            "UNSOUND" => unsound.push((*case, o)),
            "CRASH" => crashes += 1,
            _ => {}
        }
    }
    std::fs::write(&out, &tsv).unwrap_or_else(|e| fail(&format!("writing {out}: {e}")));
    eprintln!("\n== fuzz {file}: {} cases, {:.0}s wall, results in {out} ==", results.len(), start.elapsed().as_secs_f64());
    for (s, n) in &statuses {
        eprintln!("  {s:<32} {n}");
    }
    eprintln!("  crashes: {crashes}   UNSOUND: {}", unsound.len());
    for (case, o) in &unsound {
        eprintln!("\n!! UNSOUND {} [{}] {}\n   {}\n   status {} {}", case.name, case.kind.tag(), case.mutation, case.text, o.status, o.detail);
        if let Ok(p) = std::fs::read_to_string(bench::proof_path(&proofs_dir, &case.name)) {
            eprintln!("{p}");
        }
    }
    if !unsound.is_empty() {
        std::process::exit(2);
    }
    if crashes > 0 {
        std::process::exit(1);
    }
}

fn geo_programs(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("geo") {
                if let Ok(src) = std::fs::read_to_string(&p) {
                    out.push((p.display().to_string(), src));
                }
            }
        }
    }
    out.sort();
    out
}

fn fuzz_one(file: Option<&String>, name: Option<&String>, opts: &Opts) {
    let file = file.unwrap_or_else(|| fail("usage: ddar --fuzz-one <cases> <name>"));
    let name = name.unwrap_or_else(|| fail("usage: ddar --fuzz-one <cases> <name>"));
    let o = ddar::fuzz::child_main(
        std::path::Path::new(file),
        name,
        std::time::Duration::from_secs_f64(opts.corpus.budget),
        opts.corpus.proofs.as_deref().map(std::path::Path::new),
    );
    println!("RESULT\t{}", o.to_tsv());
    if opts.human || opts.human_json || opts.human_stats.is_some() {
        for (k, (p, aux, first)) in o.proved_problems.iter().enumerate() {
            human_report(&o.name, k, p, aux, *first, opts);
        }
    }
    if opts.proof {
        if let Some(p) = &o.proof {
            println!("\n{p}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_flags_are_recognised() {
        for h in ["--help", "-h"] {
            assert!(matches!(classify_arg(h), ArgKind::Help), "{h}");
        }
        assert!(USAGE.contains("--geo <prog|file>") && USAGE.contains("-h, --help"));
    }

    #[test]
    fn unknown_options_are_rejected_not_parsed_as_problems() {
        assert!(matches!(classify_arg("--bogus"), ArgKind::Unknown));
        assert!(matches!(classify_arg("a b c = triangle a b c ? perp a b a c"), ArgKind::Positional));
        assert!(matches!(classify_arg("--geo"), ArgKind::Mode));
    }
}
