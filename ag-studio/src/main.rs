//! GeoSolver — a local front end for the `alphageometry-rs` DDAR
//! engine: translate a photographed/described problem into `.geo`, solve it, and
//! render the proof + figure as PDF/PNG.
//!
//! Subcommands:
//!   agstudio render <program|file> [opts]    solve and print a proof + figure
//!   agstudio translate <text|--image P> …    photo/NL → .geo (via the claude CLI)
//!   agstudio serve [--port N]                run the web app (later stage)
//!   agstudio mcp                             run the MCP server (later stage)

mod auth;
mod db;
mod engine;
mod figure;
mod gate;
mod i18n;
mod mcp;
mod present;
mod pwa;
mod render;
mod security;
mod spread;
mod translate;
mod web;
mod worker;

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Duration;

use ddar::svg::Theme;
use engine::{parse_theme, solve_best, solve_within, InputKind, Solution, SolveOptions};

/// Default `best` search budget, in seconds.
const DEFAULT_BUDGET_SECS: f64 = 20.0;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("render") => cmd_render(&args[1..]),
        Some("translate") => cmd_translate(&args[1..]),
        Some("best") => cmd_best(&args[1..]),
        Some("serve") => cmd_serve(&args[1..]),
        Some(worker::SUBCOMMAND) => ExitCode::from(worker::worker_main().clamp(0, 255) as u8),
        Some("mcp") => {
            if let Some(extra) = args.get(1) {
                return usage_error(&format!("mcp takes no arguments (got {extra:?})"));
            }
            // Bound what an abandoned (timed-out) search may still consume in
            // this long-lived process.
            security::apply_process_limits();
            match mcp::serve() {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("mcp server error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("-h") | Some("--help") | Some("help") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown subcommand: {other}\n");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!(
        "GeoSolver\n\n\
         USAGE:\n  \
         agstudio render <program|file> [--low-level] [--theme dark|light] [--title T] [--no-proof] [--timeout SECS] [--svg F] [--png F] [--pdf F]\n  \
         agstudio best   <program|file> [--low-level] [--budget SECS] [--theme dark|light] [--title T] [--svg F] [--png F] [--pdf F]\n  \
         agstudio translate <text> | --image <path>  [--solve] [--timeout SECS] [--svg F] [--png F] [--pdf F] [--theme dark|light] [--title T]\n  \
         agstudio serve [--port N]\n  \
         agstudio mcp\n\n\
         `render`    solves a .geo program (or file), falling back to the aux search\n\
              (gives up after --timeout seconds, default {timeout}).\n\
         `best`      spends a time budget (default {budget}s) finding the shortest, most elegant proof.\n\
         `translate` turns a photo/description into .geo via the local `claude` CLI\n\
              (your Claude subscription — no API key); add --solve/--png/--pdf to go end to end.\n",
        timeout = engine::DEFAULT_SOLVE_TIMEOUT.as_secs(),
        budget = DEFAULT_BUDGET_SECS,
    )
}

fn usage_error(msg: &str) -> ExitCode {
    eprintln!("error: {msg}\n(run `agstudio --help` for usage)");
    ExitCode::from(2)
}

/// Parsed command-line arguments for one subcommand.
#[derive(Debug, Default)]
struct Args {
    switches: Vec<String>,
    values: HashMap<String, String>,
    positional: Vec<String>,
}

impl Args {
    fn has(&self, flag: &str) -> bool {
        self.switches.iter().any(|s| s == flag)
    }
    fn get(&self, flag: &str) -> Option<&str> {
        self.values.get(flag).map(String::as_str)
    }
    fn secs(&self, flag: &str) -> Result<Option<f64>, String> {
        match self.get(flag) {
            None => Ok(None),
            Some(v) => match v.parse::<f64>() {
                Ok(s) if s.is_finite() && s > 0.0 => Ok(Some(s)),
                _ => Err(format!("{flag} needs a positive number of seconds (got {v:?})")),
            },
        }
    }
    fn theme(&self) -> Result<Option<Theme>, String> {
        match self.get("--theme") {
            None => Ok(None),
            Some(t) if t.eq_ignore_ascii_case("light") || t.eq_ignore_ascii_case("dark") => {
                Ok(Some(parse_theme(t)))
            }
            Some(t) => Err(format!("--theme must be dark or light (got {t:?})")),
        }
    }
}

/// Strict flag parsing: unknown flags and flags missing their value are
/// errors (never silently taken as program text); `--` ends the flags.
fn parse_args(args: &[String], switches: &[&str], valued: &[&str]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    let mut flags_done = false;
    while let Some(a) = it.next() {
        let is_flag = !flags_done && a.len() > 1 && a.starts_with('-');
        if !is_flag {
            out.positional.push(a.clone());
        } else if a == "--" {
            flags_done = true;
        } else if switches.contains(&a.as_str()) {
            out.switches.push(a.clone());
        } else if valued.contains(&a.as_str()) {
            match it.next() {
                Some(v) if !(v.starts_with("--") && v.len() > 2) => {
                    out.values.insert(a.clone(), v.clone());
                }
                _ => return Err(format!("{a} needs a value")),
            }
        } else {
            return Err(format!("unknown option {a}"));
        }
    }
    Ok(out)
}

fn out_flags(a: &Args) -> OutFlags {
    OutFlags {
        svg: a.get("--svg").map(str::to_string),
        png: a.get("--png").map(str::to_string),
        pdf: a.get("--pdf").map(str::to_string),
    }
}

/// Output settings collected from CLI flags.
#[derive(Default)]
struct OutFlags {
    svg: Option<String>,
    png: Option<String>,
    pdf: Option<String>,
}

impl OutFlags {
    fn exporting(&self) -> bool {
        self.png.is_some() || self.pdf.is_some()
    }
}

fn cmd_serve(args: &[String]) -> ExitCode {
    let parsed = match parse_args(args, &[], &["--port"]) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    if let Some(extra) = parsed.positional.first() {
        return usage_error(&format!("serve takes no positional arguments (got {extra:?})"));
    }
    let port: u16 = match parsed.get("--port").map(str::parse) {
        None => 8787,
        Some(Ok(p)) => p,
        Some(Err(_)) => return usage_error("--port needs a number"),
    };
    let config = match security::Config::from_env(port) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    // Apply server-safe resource limits before any solving / rayon initialisation.
    security::apply_process_limits();
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: could not start async runtime: {e}");
            return ExitCode::from(1);
        }
    };
    match rt.block_on(web::serve(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("server error: {e}");
            ExitCode::from(1)
        }
    }
}

/// Shared front half of `render` / `best`: parse flags, load the program, and
/// build the solve options.
struct Prepared {
    source: String,
    opts: SolveOptions,
    out: OutFlags,
    light: bool,
    args: Args,
}

fn prepare(cmd: &str, args: &[String], switches: &[&str], valued: &[&str]) -> Result<Prepared, ExitCode> {
    let parsed = parse_args(args, switches, valued).map_err(|e| usage_error(&e))?;
    let theme = parsed.theme().map_err(|e| usage_error(&e))?;
    let program = match parsed.positional.as_slice() {
        [p] => p.clone(),
        [] => return Err(usage_error(&format!("{cmd} needs a .geo program or file path"))),
        [_, extra, ..] => {
            return Err(usage_error(&format!(
                "{cmd} takes one program (quote it); unexpected extra argument {extra:?}"
            )))
        }
    };
    let source = load_program(&program)?;
    let mut opts = SolveOptions {
        kind: if parsed.has("--low-level") {
            InputKind::LowLevel
        } else {
            InputKind::detect(&source)
        },
        want_proof: !parsed.has("--no-proof"),
        title: parsed.get("--title").map(str::to_string),
        ..SolveOptions::default()
    };
    let out = out_flags(&parsed);
    let mut light = false;
    match theme {
        Some(t) => {
            light = t == Theme::Light;
            opts.theme = t;
        }
        None if out.exporting() => {
            opts.theme = Theme::Light;
            light = true;
        }
        None => {}
    }
    Ok(Prepared {
        source,
        opts,
        out,
        light,
        args: parsed,
    })
}

const OUT_FLAGS: [&str; 5] = ["--theme", "--title", "--svg", "--png", "--pdf"];

fn cmd_best(args: &[String]) -> ExitCode {
    let valued = [&OUT_FLAGS[..], &["--budget"]].concat();
    let p = match prepare("best", args, &["--low-level"], &valued) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let secs = match p.args.secs("--budget") {
        Ok(s) => s.unwrap_or(DEFAULT_BUDGET_SECS),
        Err(e) => return usage_error(&e),
    };
    let budget = Duration::from_secs_f64(secs.clamp(0.1, 600.0));
    arm_hard_limit("best", "--budget", budget);
    eprintln!(
        "Searching up to {:.0}s for the shortest proof…",
        budget.as_secs_f64()
    );
    match solve_best(&p.source, &p.opts, budget) {
        Ok(sol) => emit_solution(&sol, &p.out, p.opts.title.as_deref(), p.light),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_render(args: &[String]) -> ExitCode {
    let valued = [&OUT_FLAGS[..], &["--timeout"]].concat();
    let p = match prepare("render", args, &["--low-level", "--no-proof"], &valued) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let timeout = match p.args.secs("--timeout") {
        Ok(s) => s.map_or(engine::DEFAULT_SOLVE_TIMEOUT, Duration::from_secs_f64),
        Err(e) => return usage_error(&e),
    };
    arm_hard_limit("render", "--timeout", timeout);
    match solve_within(&p.source, &p.opts, Some(timeout)) {
        Ok(sol) => emit_solution(&sol, &p.out, p.opts.title.as_deref(), p.light),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_translate(args: &[String]) -> ExitCode {
    let valued = [&OUT_FLAGS[..], &["--image", "--timeout"]].concat();
    let parsed = match parse_args(args, &["--solve"], &valued) {
        Ok(p) => p,
        Err(e) => return usage_error(&e),
    };
    let (theme, timeout) = match (parsed.theme(), parsed.secs("--timeout")) {
        (Ok(t), Ok(s)) => (t, s.map_or(engine::DEFAULT_SOLVE_TIMEOUT, Duration::from_secs_f64)),
        (Err(e), _) | (_, Err(e)) => return usage_error(&e),
    };
    let out = out_flags(&parsed);
    let do_solve = parsed.has("--solve");
    let title = parsed.get("--title").map(str::to_string);

    let source = match (parsed.get("--image"), parsed.positional.is_empty()) {
        (Some(_), false) => {
            return usage_error("give either a problem description or --image <path>, not both")
        }
        (Some(path), true) => {
            if !std::path::Path::new(path).is_file() {
                eprintln!("error: image not found: {path}");
                return ExitCode::from(1);
            }
            translate::Source::Image(std::path::PathBuf::from(path))
        }
        (None, false) => translate::Source::Text(parsed.positional.join(" ")),
        (None, true) => {
            return usage_error("translate needs a problem description or --image <path>")
        }
    };

    if !translate::available() {
        eprintln!(
            "The `claude` CLI is not available, so translation is disabled.\n\
             Install it with `npm i -g @anthropic-ai/claude-code` and sign in with your\n\
             subscription (run `claude`, then `/login`). You can still solve .geo programs\n\
             directly with `agstudio render`."
        );
        return ExitCode::from(3);
    }

    eprintln!("Translating with the local Claude subscription…");
    let translation = match translate::translate(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("translation failed: {e:#}");
            return ExitCode::from(1);
        }
    };

    println!("{}\n", translation.geo);

    if !do_solve && !out.exporting() && out.svg.is_none() {
        return ExitCode::SUCCESS;
    }

    // Solve the translated program end to end.
    let mut light = theme == Some(Theme::Light);
    let mut opts = SolveOptions {
        kind: InputKind::detect(&translation.geo),
        theme: theme.unwrap_or(Theme::Dark),
        want_proof: true,
        title: title.or(translation.title),
        panel: true,
    };
    if out.exporting() && theme.is_none() {
        opts.theme = Theme::Light;
        light = true;
    }
    arm_hard_limit("translate --solve", "--timeout", timeout);
    match solve_within(&translation.geo, &opts, Some(timeout)) {
        Ok(sol) => emit_solution(&sol, &out, opts.title.as_deref(), light),
        Err(e) => {
            eprintln!("error solving the translated program: {e}");
            ExitCode::from(1)
        }
    }
}

/// Make `limit` (plus [`worker::GRACE`]) a hard bound on the rest of the command.
fn arm_hard_limit(cmd: &str, flag: &str, limit: Duration) {
    worker::arm_watchdog(
        limit + worker::GRACE,
        format!(
            "`{cmd}` ({flag} {:.0}s + {:.0}s grace)",
            limit.as_secs_f64(),
            worker::GRACE.as_secs_f64()
        ),
    );
}

/// Read a program from a file path, or return it verbatim if it is inline text.
/// A single-line argument that names a `.geo`/`.txt` file which does not exist
/// is an error, not a program.
fn load_program(program: &str) -> Result<String, ExitCode> {
    let path = std::path::Path::new(program);
    if path.is_file() {
        return std::fs::read_to_string(program).map_err(|e| {
            eprintln!("error: reading {program}: {e}");
            ExitCode::from(1)
        });
    }
    if looks_like_a_path(program) {
        eprintln!("error: file not found: {program}");
        return Err(ExitCode::from(1));
    }
    Ok(program.to_string())
}

fn looks_like_a_path(arg: &str) -> bool {
    let t = arg.trim();
    if t.contains('\n') || t.contains('=') || t.contains('@') {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    lower.ends_with(".geo") || lower.ends_with(".txt") || std::path::Path::new(t).is_dir()
}

/// Write requested outputs (figure SVG, and a combined proof+figure PNG/PDF
/// report) and print the proof to the terminal.
fn emit_solution(sol: &Solution, out: &OutFlags, title: Option<&str>, light: bool) -> ExitCode {
    if let Some(path) = &out.svg {
        match std::fs::write(path, &sol.svg) {
            Ok(()) => println!("Figure written to {path}"),
            Err(e) => eprintln!("warning: could not write {path}: {e}"),
        }
    }
    if out.exporting() {
        let report = render::report_svg(sol, title, light);
        if let Some(path) = &out.png {
            match render::svg_to_png(&report, 2.75).and_then(|b| Ok(std::fs::write(path, b)?)) {
                Ok(()) => println!("PNG written to {path}"),
                Err(e) => eprintln!("warning: PNG export failed: {e}"),
            }
        }
        if let Some(path) = &out.pdf {
            match render::report_pdf(sol, title).and_then(|b| Ok(std::fs::write(path, b)?)) {
                Ok(()) => println!("PDF written to {path}"),
                Err(e) => eprintln!("warning: PDF export failed: {e}"),
            }
        }
    }

    if let Some(false) = sol.goal_holds_numerically {
        println!(
            "Warning: the goal does not hold in the sampled figure — the statement \
             appears to be false."
        );
    }
    if sol.proved {
        let steps = sol
            .proof_steps
            .map(|s| format!(", {s} steps"))
            .unwrap_or_default();
        println!(
            "Proven :-)  ({:.3}s{})  [{}]\n",
            sol.elapsed_secs, steps, sol.note
        );
        if !sol.aux_constructions.is_empty() {
            println!("Auxiliary constructions:");
            for c in &sol.aux_constructions {
                println!("  + {c}");
            }
            println!();
        }
        if let Some(proof) = &sol.proof {
            println!("{proof}");
        }
        ExitCode::SUCCESS
    } else {
        println!(
            "{} ({:.3}s)  [{}]",
            sol.status.label(),
            sol.elapsed_secs,
            sol.note
        );
        if let Some(evidence) = &sol.numeric_evidence {
            println!("\n{evidence}");
        }
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn unknown_flags_are_errors_not_program_text() {
        let e = parse_args(&v(&["--bogus", "A = free"]), &[], &["--svg"]).unwrap_err();
        assert!(e.contains("unknown option --bogus"), "{e}");
    }

    #[test]
    fn flags_missing_a_value_are_errors() {
        assert!(parse_args(&v(&["prog", "--svg"]), &[], &["--svg"]).is_err());
        assert!(parse_args(&v(&["--svg", "--pdf", "x"]), &[], &["--svg", "--pdf"]).is_err());
        let ok = parse_args(&v(&["--title", "-5", "p"]), &[], &["--title"]).unwrap();
        assert_eq!(ok.get("--title"), Some("-5"));
    }

    #[test]
    fn double_dash_ends_flags() {
        let a = parse_args(&v(&["--", "--weird"]), &[], &[]).unwrap();
        assert_eq!(a.positional, v(&["--weird"]));
    }

    #[test]
    fn numeric_and_theme_flags_are_validated() {
        let a = parse_args(&v(&["--budget", "abc"]), &[], &["--budget"]).unwrap();
        assert!(a.secs("--budget").is_err());
        let a = parse_args(&v(&["--timeout", "0"]), &[], &["--timeout"]).unwrap();
        assert!(a.secs("--timeout").is_err());
        let a = parse_args(&v(&["--theme", "blue"]), &[], &["--theme"]).unwrap();
        assert!(a.theme().is_err());
    }

    #[test]
    fn missing_geo_file_is_not_compiled_as_text() {
        assert!(load_program("/nonexistent/problem.geo").is_err());
        assert!(load_program("problem.GEO").is_err());
        assert_eq!(load_program("A = free\nprove coll(A,A,A)").unwrap(), "A = free\nprove coll(A,A,A)");
        assert!(load_program("a@1_2 b@3_4 = ? coll a b a").is_ok());
    }
}
