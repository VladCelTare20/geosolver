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
mod i18n;
mod mcp;
mod render;
mod security;
mod translate;
mod web;

use std::process::ExitCode;

use ddar::svg::Theme;
use engine::{parse_theme, solve, solve_best, InputKind, Solution, SolveOptions};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("render") => cmd_render(&args[1..]),
        Some("translate") => cmd_translate(&args[1..]),
        Some("best") => cmd_best(&args[1..]),
        Some("serve") => cmd_serve(&args[1..]),
        Some("mcp") => match mcp::serve() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("mcp server error: {e}");
                ExitCode::from(1)
            }
        },
        Some("-h") | Some("--help") | None => {
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
         agstudio render <program|file> [--low-level] [--theme dark|light] [--title T] [--no-proof] [--svg F] [--png F] [--pdf F]\n  \
         agstudio best   <program|file> [--budget SECS] [--theme dark|light] [--title T] [--svg F] [--png F] [--pdf F]\n  \
         agstudio translate <text> | --image <path>  [--solve] [--svg F] [--png F] [--pdf F] [--theme dark|light] [--title T]\n  \
         agstudio serve [--port N]\n  \
         agstudio mcp\n\n\
         `render`    solves a .geo program (or file), falling back to the aux search.\n\
         `best`      spends a time budget (default 10s) finding the shortest, most elegant proof.\n\
         `translate` turns a photo/description into .geo via the local `claude` CLI\n\
              (your Claude subscription — no API key); add --solve/--png/--pdf to go end to end.\n"
    );
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
    let mut port: u16 = 8787;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--port" {
            if let Some(p) = it.next().and_then(|s| s.parse().ok()) {
                port = p;
            } else {
                eprintln!("error: --port needs a number");
                return ExitCode::from(2);
            }
        }
    }
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

fn cmd_best(args: &[String]) -> ExitCode {
    let mut opts = SolveOptions::default();
    let mut out = OutFlags::default();
    let mut positional: Option<String> = None;
    let mut budget_secs = 20.0f64;
    let mut theme_explicit = false;
    let mut light = false;
    let mut low_level_flag = false;
    let mut it = args.iter().cloned().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--low-level" => {
                opts.kind = InputKind::LowLevel;
                low_level_flag = true;
            }
            "--budget" => budget_secs = it.next().and_then(|s| s.parse().ok()).unwrap_or(20.0),
            "--theme" => {
                let t = it.next().unwrap_or_default();
                opts.theme = parse_theme(&t);
                light = t.eq_ignore_ascii_case("light");
                theme_explicit = true;
            }
            "--title" => opts.title = it.next(),
            "--svg" => out.svg = it.next(),
            "--png" => out.png = it.next(),
            "--pdf" => out.pdf = it.next(),
            other => positional = Some(other.to_string()),
        }
    }
    let Some(program) = positional else {
        eprintln!("error: best needs a .geo program or file path");
        return ExitCode::from(2);
    };
    let source = match load_program(&program) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if !low_level_flag {
        opts.kind = InputKind::detect(&source);
    }
    if out.exporting() && !theme_explicit {
        opts.theme = Theme::Light;
        light = true;
    }
    let secs = if budget_secs.is_finite() { budget_secs } else { 20.0 };
    let budget = std::time::Duration::from_secs_f64(secs.clamp(0.1, 600.0));
    eprintln!(
        "Searching up to {:.0}s for the shortest proof…",
        budget.as_secs_f64()
    );
    match solve_best(&source, &opts, budget) {
        Ok(sol) => emit_solution(&sol, &out, opts.title.as_deref(), light),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_render(args: &[String]) -> ExitCode {
    let mut opts = SolveOptions::default();
    let mut out = OutFlags::default();
    let mut positional: Option<String> = None;
    let mut theme_explicit = false;
    let mut light = false;
    let mut low_level_flag = false;
    let mut it = args.iter().cloned().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--low-level" => {
                opts.kind = InputKind::LowLevel;
                low_level_flag = true;
            }
            "--no-proof" => opts.want_proof = false,
            "--theme" => {
                let t = it.next().unwrap_or_default();
                opts.theme = parse_theme(&t);
                light = t.eq_ignore_ascii_case("light");
                theme_explicit = true;
            }
            "--title" => opts.title = it.next(),
            "--svg" => out.svg = it.next(),
            "--png" => out.png = it.next(),
            "--pdf" => out.pdf = it.next(),
            other => positional = Some(other.to_string()),
        }
    }
    let Some(program) = positional else {
        eprintln!("error: render needs a .geo program or file path");
        return ExitCode::from(2);
    };
    let source = match load_program(&program) {
        Ok(s) => s,
        Err(code) => return code,
    };
    if !low_level_flag {
        opts.kind = InputKind::detect(&source);
    }
    if out.exporting() && !theme_explicit {
        opts.theme = Theme::Light;
        light = true;
    }

    match solve(&source, &opts) {
        Ok(sol) => emit_solution(&sol, &out, opts.title.as_deref(), light),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_translate(args: &[String]) -> ExitCode {
    let mut out = OutFlags::default();
    let mut image: Option<String> = None;
    let mut text_parts: Vec<String> = Vec::new();
    let mut do_solve = false;
    let mut theme = Theme::Dark;
    let mut theme_explicit = false;
    let mut light = false;
    let mut title: Option<String> = None;
    let mut it = args.iter().cloned().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--image" => image = it.next(),
            "--solve" => do_solve = true,
            "--theme" => {
                let t = it.next().unwrap_or_default();
                theme = parse_theme(&t);
                light = t.eq_ignore_ascii_case("light");
                theme_explicit = true;
            }
            "--title" => title = it.next(),
            "--svg" => out.svg = it.next(),
            "--png" => out.png = it.next(),
            "--pdf" => out.pdf = it.next(),
            other => text_parts.push(other.to_string()),
        }
    }

    let source = match &image {
        Some(path) => translate::Source::Image(std::path::PathBuf::from(path)),
        None if !text_parts.is_empty() => translate::Source::Text(text_parts.join(" ")),
        None => {
            eprintln!("error: translate needs a problem description or --image <path>");
            return ExitCode::from(2);
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
            eprintln!("translation failed: {e}");
            return ExitCode::from(1);
        }
    };

    println!("{}\n", translation.geo);

    if !do_solve && !out.exporting() && out.svg.is_none() {
        return ExitCode::SUCCESS;
    }

    // Solve the translated program end to end.
    let mut opts = SolveOptions {
        kind: InputKind::detect(&translation.geo),
        theme,
        want_proof: true,
        title: title.or(translation.title),
        panel: true,
    };
    if out.exporting() && !theme_explicit {
        opts.theme = Theme::Light;
        light = true;
    }
    match solve(&translation.geo, &opts) {
        Ok(sol) => emit_solution(&sol, &out, opts.title.as_deref(), light),
        Err(e) => {
            eprintln!("error solving the translated program: {e}");
            ExitCode::from(1)
        }
    }
}

/// Read a program from a file path, or return it verbatim if it is inline text.
fn load_program(program: &str) -> Result<String, ExitCode> {
    if std::path::Path::new(program).is_file() {
        std::fs::read_to_string(program).map_err(|e| {
            eprintln!("error: reading {program}: {e}");
            ExitCode::from(1)
        })
    } else {
        Ok(program.to_string())
    }
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
            match render::svg_to_png(&report, 2.0).and_then(|b| Ok(std::fs::write(path, b)?)) {
                Ok(()) => println!("PNG written to {path}"),
                Err(e) => eprintln!("warning: PNG export failed: {e}"),
            }
        }
        if let Some(path) = &out.pdf {
            match render::svg_to_pdf(&report).and_then(|b| Ok(std::fs::write(path, b)?)) {
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
        println!("Not proven ({:.3}s)  [{}]", sol.elapsed_secs, sol.note);
        ExitCode::from(1)
    }
}
