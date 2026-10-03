//! A Model Context Protocol (MCP) server over stdio, so "this PC's Claude"
//! (Claude Desktop or Claude Code) can drive the prover directly: Claude reads
//! the photo/prose and writes a `.geo` program, then calls these tools to prove
//! it, draw the figure, and export a PDF/PNG.
//!
//! The transport is the MCP stdio convention: newline-delimited JSON-RPC 2.0.
//! stdout carries the protocol exclusively — all diagnostics go to stderr.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Value};

use crate::engine::{self, InputKind, SolveOptions};
use crate::render;
use crate::translate;
use ddar::svg::Theme;

/// Protocol revisions this server speaks, newest first. `initialize` echoes the
/// client's version when it is listed here, else offers the newest.
const SUPPORTED_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;

/// Run the MCP server, reading requests from stdin and writing responses to
/// stdout, until stdin closes.
pub fn serve() -> anyhow::Result<()> {
    // Keep degenerate-figure panics from the aux search off the (shared) stderr
    // as noisy backtraces; they are already contained by `engine::solve`.
    std::panic::set_hook(Box::new(|_| {}));
    eprintln!("geosolver MCP server ready (stdio)");

    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut stdout = std::io::stdout();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        if input.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        if let Some(resp) = handle_line(&buf) {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Validate and dispatch one raw line. Returns the response to write, or
/// `None` for blank lines, notifications, and responses sent by the client.
fn handle_line(raw: &[u8]) -> Option<Value> {
    let Ok(text) = std::str::from_utf8(raw) else {
        eprintln!("rejecting a JSON-RPC line that is not valid UTF-8");
        return Some(error_response(Value::Null, PARSE_ERROR, "parse error: invalid UTF-8"));
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let req: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rejecting malformed JSON-RPC line: {e}");
            return Some(error_response(Value::Null, PARSE_ERROR, &format!("parse error: {e}")));
        }
    };
    let Some(obj) = req.as_object() else {
        let what = if req.is_array() {
            "batch requests are not supported"
        } else {
            "a request must be a JSON object"
        };
        return Some(error_response(Value::Null, INVALID_REQUEST, what));
    };
    let id = obj.get("id");
    let id_ok = matches!(id, None | Some(Value::String(_)) | Some(Value::Number(_)));
    if obj.get("method").is_none() && (obj.contains_key("result") || obj.contains_key("error")) {
        // A response from the client (we never send requests); nothing to say.
        return None;
    }
    if !id_ok {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: `id` must be a string or a number",
        ));
    }
    let reply_id = id.cloned().unwrap_or(Value::Null);
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return id.map(|_| error_response(reply_id, INVALID_REQUEST, "invalid request: jsonrpc must be \"2.0\""));
    }
    if !obj.get("method").is_some_and(Value::is_string) {
        return id.map(|_| error_response(reply_id, INVALID_REQUEST, "invalid request: missing `method`"));
    }
    catch_panics(id.cloned(), || handle(&req))
}

/// Runs `f`, converting any panic into a JSON-RPC error response instead of
/// letting it propagate — which would otherwise kill this entire per-session
/// MCP process (unlike `ag-studio`'s web server, nothing here runs inside a
/// `tokio::task::spawn_blocking`, so there is no panic-isolation boundary
/// above this one). Mirrors the `catch_unwind(AssertUnwindSafe(...))`
/// convention used throughout `engine.rs`/`aux_search.rs`.
fn catch_panics(id: Option<Value>, f: impl FnOnce() -> Option<Value>) -> Option<Value> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(resp) => resp,
        Err(_) => {
            eprintln!("request handler panicked; returning an error instead of crashing");
            id.map(|id| {
                json!({
                    "jsonrpc": "2.0", "id": id,
                    "error": { "code": -32000, "message": "internal error while handling this request" }
                })
            })
        }
    }
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
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let protocol = requested
        .and_then(|r| SUPPORTED_PROTOCOLS.iter().find(|s| **s == r))
        .unwrap_or(&SUPPORTED_PROTOCOLS[0]);
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
                    "budget_secs": { "type": "number", "description": "Time budget in seconds for `best` (default 20)." },
                    "timeout_secs": { "type": "number", "description": "Wall-clock limit for the default (non-`best`) solve, in seconds (default 60, max 300)." }
                },
                "required": ["program"]
            }
        },
        {
            "name": "export_report",
            "description": "Solve a .geo program and write a one-page report (figure + numbered \
                proof) to a PDF or PNG file in the GeoSolver export directory, returning the file \
                path. Use when the user wants a downloadable/printable document.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "program": { "type": "string" },
                    "format": { "type": "string", "enum": ["pdf", "png"], "description": "Default pdf." },
                    "filename": { "type": "string", "description": "A bare file name (no directories) inside the export directory ($AGSTUDIO_EXPORT_DIR, else $XDG_DATA_HOME/geosolver/exports, else ~/geosolver-exports). Its extension must match `format`; it is added if missing. Default: a fresh unique name." },
                    "overwrite": { "type": "boolean", "description": "Replace an existing file of that name (default false)." },
                    "timeout_secs": { "type": "number", "description": "Wall-clock solve limit in seconds (default 60, max 300)." },
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
        engine::solve_within(&program, &opts, Some(timeout_from(args)))
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
    // "Not proven" is a valid answer, not a tool failure.
    json!({ "content": content, "isError": false })
}
/// The solve wall-clock limit from a tool's `timeout_secs` (default 60s,
/// clamped to 1..=300s).
fn timeout_from(args: &Value) -> std::time::Duration {
    let secs = args
        .get("timeout_secs")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .unwrap_or(engine::DEFAULT_SOLVE_TIMEOUT.as_secs_f64())
        .clamp(1.0, 300.0);
    std::time::Duration::from_secs_f64(secs)
}

/// Where `export_report` may write: `AGSTUDIO_EXPORT_DIR`, else
/// `$XDG_DATA_HOME/geosolver/exports`, else `~/geosolver-exports`.
fn export_dir() -> Result<PathBuf, String> {
    let nonempty = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let dir = if let Some(d) = nonempty("AGSTUDIO_EXPORT_DIR") {
        PathBuf::from(d)
    } else if let Some(x) = nonempty("XDG_DATA_HOME") {
        Path::new(&x).join("geosolver").join("exports")
    } else if let Some(h) = nonempty("HOME").or_else(|| nonempty("USERPROFILE")) {
        Path::new(&h).join("geosolver-exports")
    } else {
        return Err("no export directory: set AGSTUDIO_EXPORT_DIR".to_string());
    };
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&dir)
        .map_err(|e| format!("cannot create export directory {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Resolve the file `export_report` will write, confined to `dir`: a bare file
/// name (or an absolute path directly inside `dir`), no hidden names, an
/// extension matching `ext` (added when missing), and no clobbering an
/// existing file unless `overwrite` — and never through a symlink.
fn resolve_export_path(
    dir: &Path,
    name: Option<&str>,
    ext: &str,
    overwrite: bool,
) -> Result<PathBuf, String> {
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("export directory {}: {e}", dir.display()))?;
    let file_name = match name {
        None => {
            static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            format!("geosolver-proof-{secs}-{}-{n}.{ext}", std::process::id())
        }
        Some(raw) => {
            let p = Path::new(raw);
            let base = if p.is_absolute() {
                let parent = p.parent().and_then(|d| d.canonicalize().ok());
                if parent.as_deref() != Some(dir.as_path()) {
                    return Err(format!(
                        "refusing to write outside the export directory {}",
                        dir.display()
                    ));
                }
                p.file_name().and_then(|f| f.to_str()).unwrap_or("")
            } else {
                raw
            };
            if base.is_empty()
                || base.starts_with('.')
                || base.contains(['/', '\\', '\0'])
                || base.contains("..")
            {
                return Err(format!(
                    "`filename` must be a plain file name inside {} (got {raw:?})",
                    dir.display()
                ));
            }
            match Path::new(base).extension().and_then(|e| e.to_str()) {
                Some(e) if e.eq_ignore_ascii_case(ext) => base.to_string(),
                Some(e) => {
                    return Err(format!(
                        "`filename` has extension .{e}, but the format is {ext}"
                    ))
                }
                None => format!("{base}.{ext}"),
            }
        }
    };
    let path = dir.join(file_name);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() => {
            Err(format!("refusing to write through the symlink {}", path.display()))
        }
        Ok(m) if !m.is_file() => Err(format!("{} exists and is not a file", path.display())),
        Ok(_) if !overwrite => Err(format!(
            "{} already exists; pass `overwrite: true` to replace it",
            path.display()
        )),
        _ => Ok(path),
    }
}

fn tool_export(args: &Value) -> Value {
    match export_dir() {
        Ok(dir) => tool_export_in(args, &dir),
        Err(e) => text_result(&format!("error: {e}"), true),
    }
}

fn tool_export_in(args: &Value, dir: &Path) -> Value {
    // Documents export on a light, print-friendly page.
    let (program, opts) = opts_from(args, Theme::Light);
    if program.trim().is_empty() {
        return text_result("error: `program` is required", true);
    }
    let name = args
        .get("filename")
        .or_else(|| args.get("out_path"))
        .and_then(Value::as_str);
    let format = match args.get("format").and_then(Value::as_str) {
        Some(f) => f.to_ascii_lowercase(),
        None => match name.and_then(|n| Path::new(n).extension()).and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("png") => "png".to_string(),
            _ => "pdf".to_string(),
        },
    };
    if format != "pdf" && format != "png" {
        return text_result(&format!("error: unknown format {format:?} (use \"pdf\" or \"png\")"), true);
    }
    let overwrite = args.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
    // Check the destination before spending time on the solve.
    if let Err(e) = resolve_export_path(dir, name, &format, overwrite) {
        return text_result(&format!("error: {e}"), true);
    }
    let sol = match engine::solve_within(&program, &opts, Some(timeout_from(args))) {
        Ok(s) => s,
        Err(e) => return text_result(&format!("error: {e}"), true),
    };
    let report = render::report_svg(&sol, opts.title.as_deref(), true);
    let rendered = if format == "png" {
        render::svg_to_png(&report, 2.0)
    } else {
        render::svg_to_pdf(&report)
    };
    let bytes = match rendered {
        Ok(b) => b,
        Err(e) => return text_result(&format!("error rendering {format}: {e}"), true),
    };
    let path = match resolve_export_path(dir, name, &format, overwrite) {
        Ok(p) => p,
        Err(e) => return text_result(&format!("error: {e}"), true),
    };
    let mut open = std::fs::OpenOptions::new();
    open.write(true);
    if overwrite {
        open.create(true).truncate(true);
    } else {
        open.create_new(true);
    }
    let written = open.open(&path).and_then(|mut f| f.write_all(&bytes));
    match written {
        Ok(()) => text_result(
            &format!(
                "Wrote {} report to {} ({})",
                format.to_uppercase(),
                path.display(),
                if sol.proved { "proven" } else { "not proven" }
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_panics_converts_a_panic_into_a_jsonrpc_error_response() {
        let id = Some(json!(7));
        let resp = catch_panics(id, || panic!("simulated engine panic"));
        let resp = resp.expect("a panicking request with an id must still get a response");
        assert_eq!(resp["error"]["code"], -32000);
        assert_eq!(resp["id"], 7);
    }

    #[test]
    fn catch_panics_passes_through_normal_results_unchanged() {
        let resp = catch_panics(Some(json!(1)), || Some(json!({"ok": true})));
        assert_eq!(resp, Some(json!({"ok": true})));
    }

    #[test]
    fn catch_panics_returns_none_for_a_panicking_notification() {
        // Notifications (no id) get no reply even when they panic — mirrors
        // `handle`'s existing behavior of returning `None` for notifications.
        let resp = catch_panics(None, || panic!("simulated panic on a notification"));
        assert_eq!(resp, None);
    }

    fn line(s: &str) -> Option<Value> {
        handle_line(s.as_bytes())
    }

    #[test]
    fn malformed_json_gets_a_parse_error_with_null_id() {
        let r = line("{not json").unwrap();
        assert_eq!(r["error"]["code"], -32700);
        assert_eq!(r["id"], Value::Null);
    }

    #[test]
    fn invalid_utf8_gets_a_parse_error_instead_of_killing_the_server() {
        let r = handle_line(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\xff\"}").unwrap();
        assert_eq!(r["error"]["code"], -32700);
    }

    #[test]
    fn batches_and_non_objects_are_invalid_requests() {
        for s in ["[]", r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#, "42", "\"x\""] {
            let r = line(s).unwrap();
            assert_eq!(r["error"]["code"], -32600, "{s}");
            assert_eq!(r["id"], Value::Null);
        }
    }

    #[test]
    fn bad_ids_and_versions_are_invalid_requests() {
        for s in [
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":{"a":1},"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":[1],"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":true,"method":"ping"}"#,
        ] {
            let r = line(s).unwrap();
            assert_eq!(r["error"]["code"], -32600, "{s}");
            assert_eq!(r["id"], Value::Null, "{s}");
        }
        let r = line(r#"{"jsonrpc":"1.0","id":5,"method":"ping"}"#).unwrap();
        assert_eq!(r["error"]["code"], -32600);
        assert_eq!(r["id"], 5);
        let r = line(r#"{"id":6,"method":"ping"}"#).unwrap();
        assert_eq!(r["error"]["code"], -32600);
    }

    #[test]
    fn client_responses_and_notifications_get_no_reply() {
        assert!(line(r#"{"jsonrpc":"2.0","id":3,"result":{}}"#).is_none());
        assert!(line(r#"{"jsonrpc":"2.0","id":"x","error":{"code":1,"message":"m"}}"#).is_none());
        assert!(line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        let r = line(r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#).unwrap();
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"], json!({}));
    }

    #[test]
    fn initialize_negotiates_the_protocol_version() {
        let v = initialize(&json!({"protocolVersion": "2024-11-05"}));
        assert_eq!(v["protocolVersion"], "2024-11-05");
        let v = initialize(&json!({"protocolVersion": "1999-01-01"}));
        assert_eq!(v["protocolVersion"], SUPPORTED_PROTOCOLS[0]);
        let v = initialize(&json!({}));
        assert_eq!(v["protocolVersion"], SUPPORTED_PROTOCOLS[0]);
    }

    #[test]
    fn not_proven_is_a_result_not_a_tool_error() {
        let r = tool_solve(&json!({"program": "A B C = triangle\nprove perp(A, B, A, C)"}));
        assert_eq!(r["isError"], false, "{r}");
        assert!(r["content"][0]["text"].as_str().unwrap().contains("NOT PROVEN"));
        let r = tool_solve(&json!({"program": "this is not geo"}));
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn export_paths_are_confined_to_the_export_dir() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        let ok = resolve_export_path(d, Some("proof.pdf"), "pdf", false).unwrap();
        assert_eq!(ok, d.canonicalize().unwrap().join("proof.pdf"));
        let added = resolve_export_path(d, Some("proof"), "png", false).unwrap();
        assert!(added.ends_with("proof.png"));
        for bad in ["../x.pdf", "/etc/passwd", "a/b.pdf", "..", ".hidden.pdf", "x.png", ""] {
            assert!(resolve_export_path(d, Some(bad), "pdf", false).is_err(), "{bad}");
        }
        let inside = d.canonicalize().unwrap().join("abs.pdf");
        assert!(resolve_export_path(d, Some(inside.to_str().unwrap()), "pdf", false).is_ok());
        let default = resolve_export_path(d, None, "pdf", false).unwrap();
        assert!(default.starts_with(d.canonicalize().unwrap()));
        assert_eq!(default.extension().unwrap(), "pdf");
    }

    #[test]
    fn export_refuses_to_overwrite_unless_asked() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.pdf"), b"precious").unwrap();
        assert!(resolve_export_path(dir.path(), Some("keep.pdf"), "pdf", false).is_err());
        assert!(resolve_export_path(dir.path(), Some("keep.pdf"), "pdf", true).is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink("/etc/passwd", dir.path().join("link.pdf")).unwrap();
            assert!(resolve_export_path(dir.path(), Some("link.pdf"), "pdf", true).is_err());
        }
    }

    #[test]
    fn export_rejects_unknown_formats_and_writes_inside_the_dir() {
        let dir = tempfile::tempdir().unwrap();
        let prog = "A B C = triangle\nO = circumcenter(A, B, C)\nprove cong(O, A, O, B)";
        let r = tool_export_in(&json!({"program": prog, "format": "svg"}), dir.path());
        assert_eq!(r["isError"], true, "{r}");
        let r = tool_export_in(&json!({"program": prog, "filename": "../escape.pdf"}), dir.path());
        assert_eq!(r["isError"], true, "{r}");
        let r = tool_export_in(&json!({"program": prog, "filename": "ok.pdf"}), dir.path());
        assert_eq!(r["isError"], false, "{r}");
        let bytes = std::fs::read(dir.path().join("ok.pdf")).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }
}
