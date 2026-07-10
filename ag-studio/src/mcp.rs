//! A Model Context Protocol (MCP) server over stdio, so "this PC's Claude"
//! (Claude Desktop or Claude Code) can drive the prover directly: Claude reads
//! the photo/prose and writes a `.geo` program, then calls these tools to prove
//! it, draw the figure, and export a PDF/PNG.
//!
//! The transport is the MCP stdio convention: newline-delimited JSON-RPC 2.0.
//! stdout carries the protocol exclusively — all diagnostics go to stderr.

use std::io::{BufRead, Write};

use base64::Engine;
use serde_json::{json, Value};

use crate::engine::{self, InputKind, SolveOptions};
use crate::render;
use crate::translate;
use ddar::svg::Theme;

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the MCP server, reading requests from stdin and writing responses to
/// stdout, until stdin closes.
pub fn serve() -> anyhow::Result<()> {
    // Keep degenerate-figure panics from the aux search off the (shared) stderr
    // as noisy backtraces; they are already contained by `engine::solve`.
    std::panic::set_hook(Box::new(|_| {}));
    eprintln!("geosolver MCP server ready (stdio)");

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("ignoring malformed JSON-RPC line: {e}");
                continue;
            }
        };
        if let Some(resp) = handle(&req) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Dispatch one JSON-RPC message. Returns `None` for notifications (no `id`).
fn handle(req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    // Notifications carry no id and get no reply.
    let id = req.get("id").cloned()?;

    let result = match method {
        "initialize" => Ok(initialize(&params)),
        "tools/list" => Ok(tools_list()),
        "tools/call" => tools_call(&params),
        "ping" => Ok(json!({})),
        other => Err((-32601, format!("method not found: {other}"))),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err((code, message)) => json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": code, "message": message }
        }),
    })
}

fn initialize(params: &Value) -> Value {
    let protocol = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION)
        .to_string();
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "geosolver", "version": env!("CARGO_PKG_VERSION") }
    })
}

fn tools_list() -> Value {
    json!({ "tools": [
        {
            "name": "geo_reference",
            "description": "Return the full .geo language reference (constructions, relations, \
                metric goals, worked examples). Call this first when you need to write a .geo \
                program for solve_geometry or export_report.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "solve_geometry",
            "description": "Prove an olympiad plane-geometry problem written in the .geo language \
                and return a numbered proof plus a rendered figure (PNG). Relational goals go \
                through the DDAR closure and an auxiliary-point search; absolute-length goals \
                (e.g. AC^2+BD^2=144) get a classical Euclidean proof. To solve a problem given as \
                a photo or prose, first read it yourself and write the .geo program (use \
                geo_reference for the grammar), then call this.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "program": { "type": "string", "description": "The .geo program, ending in a `prove ...` line." },
                    "kind": { "type": "string", "enum": ["auto", "geo", "lowlevel"], "description": "Input language (default auto-detect)." },
                    "theme": { "type": "string", "enum": ["light", "dark"], "description": "Figure colour scheme (default light)." },
                    "title": { "type": "string", "description": "Optional title for the figure." },
                    "best": { "type": "boolean", "description": "Spend a time budget finding the SHORTEST proof (fewest steps; the number of auxiliary constructions does not matter) instead of the first one found." },
                    "budget_secs": { "type": "number", "description": "Time budget in seconds for `best` (default 20)." }
                },
                "required": ["program"]
            }
        },
        {
            "name": "export_report",
            "description": "Solve a .geo program and write a one-page report (figure + numbered \
                proof) to a PDF or PNG file on disk, returning the file path. Use when the user \
                wants a downloadable/printable document.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "program": { "type": "string" },
                    "format": { "type": "string", "enum": ["pdf", "png"], "description": "Default pdf." },
                    "out_path": { "type": "string", "description": "Absolute output path (default: a file in the system temp dir)." },
                    "kind": { "type": "string", "enum": ["auto", "geo", "lowlevel"] },
                    "title": { "type": "string" }
                },
                "required": ["program"]
            }
        }
    ]})
}

fn tools_call(params: &Value) -> Result<Value, (i64, String)> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match name {
        "geo_reference" => Ok(text_result(translate::grammar(), false)),
        "solve_geometry" => Ok(tool_solve(&args)),
        "export_report" => Ok(tool_export(&args)),
        other => Err((-32602, format!("unknown tool: {other}"))),
    }
}

fn opts_from(args: &Value, default_theme: Theme) -> (String, SolveOptions) {
    let program = args
        .get("program")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let kind = match args.get("kind").and_then(Value::as_str) {
        Some("geo") => InputKind::Geo,
        Some("lowlevel") | Some("low-level") => InputKind::LowLevel,
        _ => InputKind::detect(&program),
    };
    let theme = match args.get("theme").and_then(Value::as_str) {
        Some("dark") => Theme::Dark,
        Some("light") => Theme::Light,
        _ => default_theme,
    };
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string);
    (
        program,
        SolveOptions {
            kind,
            theme,
            want_proof: true,
            title,
            panel: true,
        },
    )
}

fn tool_solve(args: &Value) -> Value {
    let (program, opts) = opts_from(args, Theme::Light);
    if program.trim().is_empty() {
        return text_result("error: `program` is required", true);
    }
    let best = args.get("best").and_then(Value::as_bool).unwrap_or(false);
    let result = if best {
        let secs = args
            .get("budget_secs")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(20.0)
            .clamp(0.5, 120.0);
        engine::solve_best(&program, &opts, std::time::Duration::from_secs_f64(secs))
    } else {
        engine::solve(&program, &opts)
    };
    let sol = match result {
        Ok(s) => s,
        Err(e) => return text_result(&format!("error: {e}"), true),
    };

    // Text: verdict, method, proof, and the compiled low-level form.
    let mut text = String::new();
    text.push_str(if sol.proved {
        "PROVEN\n"
    } else {
        "NOT PROVEN (within the search budget)\n"
    });
    text.push_str(&format!(
        "method: {} | {} | {:.3}s\n",
        method_name(sol.method),
        sol.note,
        sol.elapsed_secs
    ));
    if let Some(false) = sol.goal_holds_numerically {
        text.push_str("warning: the goal does not hold in the sampled figure — the statement appears to be false.\n");
    }
    if !sol.aux_constructions.is_empty() {
        text.push_str("auxiliary constructions:\n");
        for c in &sol.aux_constructions {
            text.push_str(&format!("  + {c}\n"));
        }
    }
    if let Some(proof) = &sol.proof {
        text.push('\n');
        text.push_str(proof);
    }
    text.push_str(&format!("\n\nlow-level: {}", sol.low_level));

    // Image: the figure as a PNG.
    let mut content = vec![json!({ "type": "text", "text": text })];
    if let Ok(png) = render::svg_to_png(&sol.svg, 1.5) {
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        content.push(json!({ "type": "image", "data": b64, "mimeType": "image/png" }));
    }
    json!({ "content": content, "isError": !sol.proved })
}

fn tool_export(args: &Value) -> Value {
    // Documents export on a light, print-friendly page.
    let (program, opts) = opts_from(args, Theme::Light);
    if program.trim().is_empty() {
        return text_result("error: `program` is required", true);
    }
    let format = args
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("pdf")
        .to_lowercase();
    let sol = match engine::solve(&program, &opts) {
        Ok(s) => s,
        Err(e) => return text_result(&format!("error: {e}"), true),
    };
    let report = render::report_svg(&sol, opts.title.as_deref(), true);
    let (bytes, ext) = if format == "png" {
        match render::svg_to_png(&report, 2.0) {
            Ok(b) => (b, "png"),
            Err(e) => return text_result(&format!("error rendering PNG: {e}"), true),
        }
    } else {
        match render::svg_to_pdf(&report) {
            Ok(b) => (b, "pdf"),
            Err(e) => return text_result(&format!("error rendering PDF: {e}"), true),
        }
    };
    let path = match args.get("out_path").and_then(Value::as_str) {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::temp_dir().join(format!("geosolver-proof.{ext}")),
    };
    match std::fs::write(&path, bytes) {
        Ok(()) => text_result(
            &format!("Wrote {} report to {}", ext.to_uppercase(), path.display()),
            false,
        ),
        Err(e) => text_result(&format!("error writing {}: {e}", path.display()), true),
    }
}

fn method_name(m: engine::Method) -> &'static str {
    match m {
        engine::Method::Ddar => "DDAR",
        engine::Method::AuxSearch => "DDAR + auxiliary search",
        engine::Method::Euclidean => "classical Euclidean prover",
    }
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [ { "type": "text", "text": text } ], "isError": is_error })
}
