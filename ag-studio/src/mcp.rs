//! A Model Context Protocol (MCP) server over stdio, so "this PC's Claude"
//! (Claude Desktop or Claude Code) can drive the prover directly: Claude reads
//! the photo/prose and writes a `.geo` program, then calls these tools to prove
//! it, draw the figure, and export a PDF/PNG.
//!
//! The transport is the MCP stdio convention: newline-delimited JSON-RPC 2.0.
//! stdout carries the protocol exclusively — all diagnostics go to stderr.

use std::collections::HashMap;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};

use crate::engine::{self, InputKind, SolveOptions};
use crate::translate;
use crate::worker::{self, Outcome};
use ddar::svg::Theme;

/// Protocol revisions this server speaks, newest first. `initialize` echoes the
/// client's version when it is listed here, else offers the newest.
const SUPPORTED_PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;

/// Shortest time a tool call waits for a free solver slot before it is
/// answered "busy"; otherwise it waits up to its own time limit.
const MIN_QUEUE_WAIT: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(5)
};
/// After stdin closes, how long the last responses may take to be read.
const FINAL_FLUSH: Duration = Duration::from_secs(60);
/// Diagnostics queued for stderr before new ones are dropped.
const NOTE_BACKLOG: usize = 256;

/// `eprintln!` that never blocks the caller (see [`note`]).
macro_rules! note {
    ($($arg:tt)*) => { note(format!($($arg)*)) };
}

/// Run the MCP server, reading requests from stdin and writing responses to
/// stdout, until stdin closes and every in-flight tool call has answered.
pub fn serve() -> anyhow::Result<()> {
    // Keep degenerate-figure panics from the aux search off the (shared) stderr
    // as noisy backtraces; they are already contained by `engine::solve`.
    std::panic::set_hook(Box::new(|_| {}));
    eprintln!("geosolver MCP server ready (stdio)");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let input = tokio::io::BufReader::new(tokio::io::stdin());
    let result = rt.block_on(serve_io(input, std::io::stdout(), max_concurrent()));
    rt.shutdown_timeout(std::time::Duration::from_secs(1));
    result
}

type Inflight = Arc<Mutex<HashMap<String, (u64, tokio::task::AbortHandle)>>>;

fn relock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Responses, one JSON line each, for the stdout writer thread.
type Output = std::sync::mpsc::Sender<String>;

fn spawn_writer<W: Write + Send + 'static>(
    mut out: W,
) -> std::io::Result<(Output, std::thread::JoinHandle<()>)> {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let handle = std::thread::Builder::new()
        .name("mcp-stdout".into())
        .spawn(move || {
            for line in rx {
                if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                    return;
                }
            }
        })?;
    Ok((tx, handle))
}

/// Queue one response for stdout; fails only once stdout is broken.
fn emit(out: &Output, resp: &Value) -> std::io::Result<()> {
    out.send(resp.to_string())
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "stdout is closed"))
}

/// Wait (bounded by [`FINAL_FLUSH`]) for the writer to deliver what is queued.
async fn finish_writer(writer: std::thread::JoinHandle<()>) {
    let joined = tokio::task::spawn_blocking(move || writer.join());
    if tokio::time::timeout(FINAL_FLUSH, joined).await.is_err() {
        note!("stdout was not read for {}s after stdin closed; exiting", FINAL_FLUSH.as_secs());
    }
}

/// One diagnostic line to stderr, written by its own thread; dropped when
/// [`NOTE_BACKLOG`] lines are already queued.
fn note(line: String) {
    static TX: OnceLock<Option<std::sync::mpsc::SyncSender<String>>> = OnceLock::new();
    let tx = TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(NOTE_BACKLOG);
        std::thread::Builder::new()
            .name("mcp-stderr".into())
            .spawn(move || {
                for line in rx {
                    let _ = writeln!(std::io::stderr(), "{line}");
                }
            })
            .ok()
            .map(|_| tx)
    });
    if let Some(tx) = tx {
        let _ = tx.try_send(line);
    }
}

/// Simultaneous solve workers: `AGSTUDIO_MAX_CONCURRENT`, default 2.
fn max_concurrent() -> usize {
    std::env::var("AGSTUDIO_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 64)
}

/// The protocol loop. Solves run as tasks, so the loop keeps reading while
/// they work; `notifications/cancelled` aborts one, which kills its worker.
async fn serve_io<R, W>(mut input: R, out: W, slots: usize) -> anyhow::Result<()>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: Write + Send + 'static,
{
    let (out, writer) = spawn_writer(out)?;
    let inflight: Inflight = Arc::default();
    let slots = Arc::new(Semaphore::new(slots));
    let (closing, closed) = watch::channel(false);
    let mut tasks = tokio::task::JoinSet::new();
    let mut seq = 0u64;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input.read_until(b'\n', &mut buf).await? == 0 {
            break;
        }
        while tasks.try_join_next().is_some() {}
        match route(&buf) {
            Route::Reply(Some(resp)) => emit(&out, &resp)?,
            Route::Reply(None) => {}
            Route::Cancel(key) => {
                if let Some((_, task)) = relock(&inflight).remove(&key) {
                    task.abort();
                    note!("request {key} cancelled; its solver was stopped");
                }
            }
            Route::Tool { id, name, args } => {
                let key = id.to_string();
                let mut map = relock(&inflight);
                if map.contains_key(&key) {
                    drop(map);
                    let msg = "invalid request: this id is already used by a call in progress";
                    emit(&out, &error_response(id, INVALID_REQUEST, msg))?;
                    continue;
                }
                seq += 1;
                let me = seq;
                let (out, inflight_ref, slots) = (out.clone(), inflight.clone(), slots.clone());
                let closed = closed.clone();
                let task_key = key.clone();
                let handle = tasks.spawn(async move {
                    let resp = run_tool(id, &name, args, slots, closed).await;
                    {
                        let mut map = relock(&inflight_ref);
                        if map.get(&task_key).is_some_and(|(s, _)| *s == me) {
                            map.remove(&task_key);
                        }
                    }
                    if let Err(e) = emit(&out, &resp) {
                        note!("could not write a response: {e}");
                    }
                });
                map.insert(key, (me, handle));
            }
        }
    }
    let _ = closing.send(true);
    while tasks.join_next().await.is_some() {}
    drop(out);
    finish_writer(writer).await;
    Ok(())
}

/// What to do with one input line.
enum Route {
    /// Answer now (or say nothing).
    Reply(Option<Value>),
    /// `notifications/cancelled` for the request with this (JSON-encoded) id.
    Cancel(String),
    /// A slow tool call, run as a cancellable task.
    Tool { id: Value, name: String, args: Value },
}

fn route(raw: &[u8]) -> Route {
    let req = match validate(raw) {
        Ok(req) => req,
        Err(reply) => return Route::Reply(reply),
    };
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let id = req.get("id").cloned();
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    if method == "notifications/cancelled" && id.is_none() {
        return match params.get("requestId") {
            Some(target) => Route::Cancel(target.to_string()),
            None => Route::Reply(None),
        };
    }
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if let (Some(id), "tools/call", true) = (id.clone(), method, is_slow_tool(name)) {
        return Route::Tool {
            id,
            name: name.to_string(),
            args: params.get("arguments").cloned().unwrap_or(Value::Null),
        };
    }
    Route::Reply(catch_panics(id, || handle(&req)))
}

fn is_slow_tool(name: &str) -> bool {
    matches!(name, "solve_geometry" | "export_report")
}

async fn run_tool(
    id: Value,
    name: &str,
    args: Value,
    slots: Arc<Semaphore>,
    closed: watch::Receiver<bool>,
) -> Value {
    let work = async {
        let permit = match acquire_slot(slots, queue_wait(name, &args), closed).await {
            Ok(p) => p,
            Err(why) => return text_result(&why, true),
        };
        match name {
            "solve_geometry" => tool_solve(&args, Some(permit)).await,
            _ => tool_export(&args, Some(permit)).await,
        }
    };
    match CatchUnwind(Box::pin(work)).await {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(_) => {
            note!("tool call panicked; returning an error instead of crashing");
            json!({
                "jsonrpc": "2.0", "id": id,
                "error": { "code": -32000, "message": "internal error while handling this request" }
            })
        }
    }
}

/// How long a call may wait for a slot: its own time limit, at least
/// [`MIN_QUEUE_WAIT`].
fn queue_wait(name: &str, args: &Value) -> Duration {
    let limit = match name {
        "solve_geometry" => solve_limit(args).1,
        _ => timeout_from(args),
    };
    limit.max(MIN_QUEUE_WAIT)
}

/// A solver slot, or the error text for a call that never started: the
/// server stayed busy for `wait`, or stdin closed while the call was queued.
async fn acquire_slot(
    slots: Arc<Semaphore>,
    wait: Duration,
    mut closed: watch::Receiver<bool>,
) -> Result<OwnedSemaphorePermit, String> {
    const HUNG_UP: &str = "error: not started — the client closed stdin while this call \
                           was waiting for a free solver slot";
    if let Ok(permit) = slots.clone().try_acquire_owned() {
        return Ok(permit);
    }
    let hung_up = async {
        if closed.wait_for(|c| *c).await.is_err() {
            std::future::pending::<()>().await;
        }
    };
    tokio::select! {
        biased;
        got = tokio::time::timeout(wait, slots.acquire_owned()) => match got {
            Ok(Ok(permit)) => Ok(permit),
            Ok(Err(_)) => Err(HUNG_UP.to_string()),
            Err(_) => Err(format!(
                "error: the server is busy — every solver slot stayed in use for {:.0}s, so \
                 this call was not started; retry it later",
                wait.as_secs_f64()
            )),
        },
        () = hung_up => Err(HUNG_UP.to_string()),
    }
}

/// `catch_unwind` for a future: a panic in one tool call becomes that call's
/// error response instead of a lost reply.
struct CatchUnwind<F>(Pin<Box<F>>);

impl<F: Future> Future for CatchUnwind<F> {
    type Output = std::thread::Result<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = self.0.as_mut();
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fut.poll(cx))) {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(v)) => Poll::Ready(Ok(v)),
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Validate and dispatch one raw line synchronously (no slow tools). Returns
/// the response to write, or `None` for blank lines, notifications, and
/// responses sent by the client.
#[cfg(test)]
fn handle_line(raw: &[u8]) -> Option<Value> {
    match validate(raw) {
        Ok(req) => catch_panics(req.get("id").cloned(), || handle(&req)),
        Err(reply) => reply,
    }
}

/// Parse and validate one raw line as a JSON-RPC 2.0 request or notification.
/// `Err` carries the reply for an invalid line (or `None`: say nothing).
fn validate(raw: &[u8]) -> Result<Value, Option<Value>> {
    let Ok(text) = std::str::from_utf8(raw) else {
        note!("rejecting a JSON-RPC line that is not valid UTF-8");
        return Err(Some(error_response(Value::Null, PARSE_ERROR, "parse error: invalid UTF-8")));
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(None);
    }
    let req: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            note!("rejecting malformed JSON-RPC line: {e}");
            return Err(Some(error_response(Value::Null, PARSE_ERROR, &format!("parse error: {e}"))));
        }
    };
    let Some(obj) = req.as_object() else {
        let what = if req.is_array() {
            "batch requests are not supported"
        } else {
            "a request must be a JSON object"
        };
        return Err(Some(error_response(Value::Null, INVALID_REQUEST, what)));
    };
    let id = obj.get("id");
    let id_ok = matches!(id, None | Some(Value::String(_)) | Some(Value::Number(_)));
    if obj.get("method").is_none() && (obj.contains_key("result") || obj.contains_key("error")) {
        // A response from the client (we never send requests); nothing to say.
        return Err(None);
    }
    if !id_ok {
        return Err(Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            "invalid request: `id` must be a string or a number",
        )));
    }
    let reply_id = id.cloned().unwrap_or(Value::Null);
    if obj.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(id.map(|_| error_response(reply_id, INVALID_REQUEST, "invalid request: jsonrpc must be \"2.0\"")));
    }
    if !obj.get("method").is_some_and(Value::is_string) {
        return Err(id.map(|_| error_response(reply_id, INVALID_REQUEST, "invalid request: missing `method`")));
    }
    Ok(req)
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
            note!("request handler panicked; returning an error instead of crashing");
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
                    "filename": { "type": "string", "description": "A bare file name (no directories) inside the export directory ($AGSTUDIO_EXPORT_DIR, else $XDG_DATA_HOME/geosolver/exports, else ~/.local/share/geosolver/exports). Its extension must match `format`; it is added if missing. Default: a fresh unique name." },
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
        "geo_reference" => {
            let _ = args;
            Ok(text_result(translate::grammar(), false))
        }
        slow if is_slow_tool(slow) => Err((-32603, format!("{slow} is dispatched asynchronously"))),
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

async fn tool_solve(args: &Value, permit: Option<OwnedSemaphorePermit>) -> Value {
    let (program, opts) = opts_from(args, Theme::Light);
    if program.trim().is_empty() {
        return text_result("error: `program` is required", true);
    }
    let (mode, limit) = solve_limit(args);
    let mut job = worker::Request::new(&program, &opts, mode, limit);
    job.figure_png_scale = Some(1.5);
    job.human_text = Some(crate::i18n::Lang::En);
    let (sol, figure_png, human) = match worker::run(&job, permit).await {
        Outcome::Done(reply) => match reply.result {
            Ok(sol) => (sol, reply.figure_png, reply.human),
            Err(e) => return text_result(&format!("error: {e}"), true),
        },
        Outcome::TimedOut => (worker::time_limit_solution(&program, job.limit()), None, None),
        Outcome::Failed(e) => {
            note!("solve worker failed: {e}");
            return text_result("error: the solver process failed (crashed or ran out of memory)", true);
        }
    };

    // Text: verdict, method, proof, and the compiled low-level form.
    let mut text = String::new();
    text.push_str(&match sol.status {
        engine::Status::Proved => "PROVEN — classical Euclidean proof\n".to_string(),
        engine::Status::HoldsNumerically => format!(
            "NOT PROVEN — no Euclidean proof was found. The goal holds numerically in {} \
             sampled figures; that is evidence, not a proof.\n",
            sol.numeric_samples.unwrap_or(0)
        ),
        engine::Status::Refuted => {
            "REFUTED — the goal fails in a sampled figure (the statement appears to be false)\n"
                .to_string()
        }
        engine::Status::NotProved => "NOT PROVEN (within the search budget)\n".to_string(),
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
    match (&human, &sol.proof) {
        (Some(h), Some(proof)) => {
            text.push_str("\nPROOF\n\n");
            text.push_str(h);
            text.push_str(&format!("\n\nFULL DERIVATION ({} machine-checked steps)\n", sol.proof_steps.unwrap_or(0)));
            text.push_str(proof);
        }
        (None, Some(proof)) => {
            text.push('\n');
            text.push_str(proof);
        }
        _ => {}
    }
    if let Some(evidence) = &sol.numeric_evidence {
        text.push('\n');
        text.push_str(evidence);
    }
    text.push_str(&format!("\n\nlow-level: {}", sol.low_level));

    // Image: the figure as a PNG.
    let mut content = vec![json!({ "type": "text", "text": text })];
    if let Some(b64) = figure_png {
        content.push(json!({ "type": "image", "data": b64, "mimeType": "image/png" }));
    }
    // "Not proven" is a valid answer, not a tool failure.
    json!({ "content": content, "isError": false })
}
/// `solve_geometry`'s mode and limit: the `best` search with `budget_secs`
/// (default 20, 0.5..=120), else a plain solve with `timeout_secs`.
fn solve_limit(args: &Value) -> (worker::Mode, Duration) {
    if args.get("best").and_then(Value::as_bool).unwrap_or(false) {
        let secs = args
            .get("budget_secs")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .unwrap_or(20.0)
            .clamp(0.5, 120.0);
        (worker::Mode::Best, Duration::from_secs_f64(secs))
    } else {
        (worker::Mode::Solve, timeout_from(args))
    }
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
/// `$XDG_DATA_HOME/geosolver/exports`, else the XDG default
/// `~/.local/share/geosolver/exports` (`%USERPROFILE%\\geosolver-exports` on Windows).
fn export_dir() -> Result<PathBuf, String> {
    let nonempty = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty());
    let dir = if let Some(d) = nonempty("AGSTUDIO_EXPORT_DIR") {
        PathBuf::from(d)
    } else if let Some(x) = nonempty("XDG_DATA_HOME") {
        Path::new(&x).join("geosolver").join("exports")
    } else if let Some(h) = nonempty("HOME") {
        Path::new(&h).join(".local/share/geosolver/exports")
    } else if let Some(h) = nonempty("USERPROFILE") {
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

async fn tool_export(args: &Value, permit: Option<OwnedSemaphorePermit>) -> Value {
    match export_dir() {
        Ok(dir) => tool_export_in(args, &dir, permit).await,
        Err(e) => text_result(&format!("error: {e}"), true),
    }
}

async fn tool_export_in(args: &Value, dir: &Path, permit: Option<OwnedSemaphorePermit>) -> Value {
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
    let mut job = worker::Request::new(&program, &opts, worker::Mode::Solve, timeout_from(args));
    job.report = Some(if format == "png" { worker::Format::Png } else { worker::Format::Pdf });
    let reply = match worker::run(&job, permit).await {
        Outcome::Done(reply) => reply,
        Outcome::TimedOut => {
            return text_result(
                &format!(
                    "error: the solve overran its {:.0}s time limit and was stopped; nothing was written",
                    job.limit().as_secs_f64()
                ),
                true,
            )
        }
        Outcome::Failed(e) => {
            note!("export worker failed: {e}");
            return text_result("error: the solver process failed (crashed or ran out of memory)", true);
        }
    };
    let sol = match &reply.result {
        Ok(s) => s,
        Err(e) => return text_result(&format!("error: {e}"), true),
    };
    let bytes = match reply.report_bytes() {
        Some(Ok(b)) => b,
        Some(Err(e)) => return text_result(&format!("error rendering {format}: {e}"), true),
        None => return text_result(&format!("error rendering {format}: no output"), true),
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
                sol.status.label().to_lowercase()
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

    #[tokio::test]
    async fn numeric_only_goal_is_reported_not_proven_with_its_evidence() {
        let r = tool_solve(&json!({"program": "A B C = triangle\nM = midpoint(B, C)\nprove area(A,B,M) = area(A,M,C)"}), None).await;
        assert_eq!(r["isError"], false, "{r}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("NOT PROVEN — no Euclidean proof"), "{text}");
        assert!(!text.starts_with("PROVEN"), "{text}");
        assert!(text.contains("not a proof"), "{text}");
    }

    #[tokio::test]
    async fn not_proven_is_a_result_not_a_tool_error() {
        let r = tool_solve(&json!({"program": "A B C = triangle\nprove perp(A, B, A, C)"}), None).await;
        assert_eq!(r["isError"], false, "{r}");
        // False in the sampled figure: refuted, which is still a normal result.
        assert!(r["content"][0]["text"].as_str().unwrap().starts_with("REFUTED"), "{r}");
        assert_eq!(r["content"][1]["mimeType"], "image/png", "the figure comes back too");
        let r = tool_solve(&json!({"program": "this is not geo"}), None).await;
        assert_eq!(r["isError"], true);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_solve_that_overruns_its_timeout_is_killed_and_reported_not_proven() {
        use crate::worker::tests::{hang_input, wait_for_pid, wait_reaped};
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let t = std::time::Instant::now();
        let r = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tool_solve(&json!({"program": program, "timeout_secs": 1}), None),
        )
        .await
        .expect("the tool call must end at its hard limit");
        assert!(t.elapsed() < std::time::Duration::from_secs(1) + worker::GRACE + std::time::Duration::from_secs(2));
        assert_eq!(r["isError"], false, "{r}");
        let text = r["content"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("NOT PROVEN"), "{text}");
        assert!(text.contains("time limit"), "{text}");
        let pid = wait_for_pid(&pidfile).await;
        wait_reaped(pid, std::time::Duration::from_secs(1)).await;
    }

    /// A `Write` the test can read back while the server task owns it.
    #[derive(Clone, Default)]
    struct Captured(Arc<Mutex<Vec<u8>>>);

    impl Write for Captured {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            relock(&self.0).extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Captured {
        fn responses(&self) -> Vec<Value> {
            String::from_utf8_lossy(&relock(&self.0))
                .lines()
                .filter_map(|l| serde_json::from_str(l).ok())
                .collect()
        }

        async fn wait_for(&self, id: Value) -> Value {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                if let Some(r) = self.responses().into_iter().find(|r| r["id"] == id) {
                    return r;
                }
                assert!(std::time::Instant::now() < until, "no response for id {id}");
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelling_a_running_solve_kills_its_worker_while_the_server_keeps_answering() {
        use crate::worker::tests::{hang_input, wait_for_pid, wait_reaped};
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let (mut client, server_end) = tokio::io::duplex(1 << 16);
        let out = Captured::default();
        let server = tokio::spawn(serve_io(tokio::io::BufReader::new(server_end), out.clone(), 2));
        let send = |v: Value| format!("{v}\n");

        let call = json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": {"name": "solve_geometry", "arguments": {"program": program}}});
        client.write_all(send(call).as_bytes()).await.unwrap();
        let pid = wait_for_pid(&pidfile).await;

        let ping = json!({"jsonrpc": "2.0", "id": 8, "method": "ping"});
        client.write_all(send(ping).as_bytes()).await.unwrap();
        let t = std::time::Instant::now();
        assert_eq!(out.wait_for(json!(8)).await["result"], json!({}));
        assert!(t.elapsed() < std::time::Duration::from_secs(2), "ping waited on the solve");

        let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled",
            "params": {"requestId": 7, "reason": "user gave up"}});
        client.write_all(send(cancel).as_bytes()).await.unwrap();
        let took = wait_reaped(pid, std::time::Duration::from_secs(3)).await;
        eprintln!("cancelled worker reaped {took:?} after the notification");

        drop(client);
        tokio::time::timeout(std::time::Duration::from_secs(10), server)
            .await
            .expect("the server must stop at EOF")
            .unwrap()
            .unwrap();
        assert!(
            out.responses().iter().all(|r| r["id"] != 7),
            "a cancelled request gets no response: {:?}",
            out.responses()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tool_calls_answer_after_stdin_closes() {
        let input = format!(
            "{}\n{}\n",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": "solve_geometry", "arguments": {"program": crate::worker::tests::CIRCUMCENTER}}}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"})
        );
        let out = Captured::default();
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            serve_io(tokio::io::BufReader::new(input.as_bytes()), out.clone(), 2),
        )
        .await
        .unwrap()
        .unwrap();
        let rs = out.responses();
        assert_eq!(rs.len(), 2, "{rs:?}");
        let solve = rs.iter().find(|r| r["id"] == 1).unwrap();
        assert!(solve["result"]["content"][0]["text"].as_str().unwrap().starts_with("PROVEN"), "{solve}");
    }

    /// A stdout nobody reads: every write blocks until [`Stalled::open`].
    #[derive(Clone, Default)]
    struct Stalled {
        gate: Arc<(Mutex<bool>, std::sync::Condvar)>,
        data: Captured,
    }

    impl Write for Stalled {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let (open, cv) = &*self.gate;
            let mut is_open = relock(open);
            while !*is_open {
                is_open = cv.wait(is_open).unwrap_or_else(PoisonError::into_inner);
            }
            drop(is_open);
            self.data.write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Stalled {
        fn open(&self) {
            *relock(&self.gate.0) = true;
            self.gate.1.notify_all();
        }
    }

    fn call(id: Value, name: &str, args: Value) -> String {
        format!("{}\n", json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}}))
    }

    async fn stopped<T>(server: tokio::task::JoinHandle<T>, within: Duration) -> T {
        tokio::time::timeout(within, server)
            .await
            .expect("the server must stop at EOF")
            .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn an_undrained_stdout_never_stalls_the_runtime_or_its_kill_timers() {
        use crate::worker::tests::{hang_input, wait_for_pid, wait_reaped};
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let out = Stalled::default();
        let (mut client, server_end) = tokio::io::duplex(1 << 16);
        let server_out = out.clone();
        let server = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(serve_io(tokio::io::BufReader::new(server_end), server_out, 2))
        });
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
            client.write_all(format!("{ping}\n").as_bytes()).await.unwrap();
            let hang = call(json!(2), "solve_geometry", json!({"program": program, "timeout_secs": 1}));
            client.write_all(hang.as_bytes()).await.unwrap();
            let pid = wait_for_pid(&pidfile).await;
            let hard = Duration::from_secs(1) + worker::GRACE;
            let took = wait_reaped(pid, hard + Duration::from_millis(500)).await;
            eprintln!("worker {pid} killed {took:?} after start with stdout blocked");
        });
        out.open();
        drop(client);
        let until = std::time::Instant::now() + Duration::from_secs(20);
        while !server.is_finished() {
            assert!(std::time::Instant::now() < until, "the server did not stop at EOF");
            std::thread::sleep(Duration::from_millis(20));
        }
        server.join().unwrap().unwrap();
        let rs = out.data.responses();
        assert_eq!(rs.iter().find(|r| r["id"] == 1).unwrap()["result"], json!({}), "{rs:?}");
        let solve = rs.iter().find(|r| r["id"] == 2).expect("the killed solve is still answered");
        assert!(solve["result"]["content"][0]["text"].as_str().unwrap().contains("time limit"), "{solve}");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_queued_call_waits_at_most_its_own_limit_then_is_told_the_server_is_busy() {
        use crate::worker::tests::{hang_input, wait_for_pid, wait_reaped, CIRCUMCENTER};
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let (mut client, server_end) = tokio::io::duplex(1 << 16);
        let out = Captured::default();
        let server = tokio::spawn(serve_io(tokio::io::BufReader::new(server_end), out.clone(), 1));
        client.write_all(call(json!(1), "solve_geometry", json!({"program": program, "timeout_secs": 60})).as_bytes()).await.unwrap();
        let pid = wait_for_pid(&pidfile).await;

        let t = std::time::Instant::now();
        client.write_all(call(json!(2), "solve_geometry", json!({"program": CIRCUMCENTER, "timeout_secs": 1})).as_bytes()).await.unwrap();
        let busy = out.wait_for(json!(2)).await;
        let waited = t.elapsed();
        assert_eq!(busy["result"]["isError"], true, "{busy}");
        assert!(busy["result"]["content"][0]["text"].as_str().unwrap().contains("busy"), "{busy}");
        assert!(waited >= MIN_QUEUE_WAIT, "{waited:?}");
        assert!(waited < MIN_QUEUE_WAIT + Duration::from_secs(2), "{waited:?}");

        let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 1}});
        client.write_all(format!("{cancel}\n").as_bytes()).await.unwrap();
        wait_reaped(pid, Duration::from_secs(3)).await;
        drop(client);
        stopped(server, Duration::from_secs(10)).await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn at_eof_queued_calls_are_not_started_and_running_ones_still_answer() {
        use crate::worker::tests::{hang_input, wait_for_pid, CIRCUMCENTER};
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let (mut client, server_end) = tokio::io::duplex(1 << 16);
        let out = Captured::default();
        let server = tokio::spawn(serve_io(tokio::io::BufReader::new(server_end), out.clone(), 1));
        client.write_all(call(json!(1), "solve_geometry", json!({"program": program, "timeout_secs": 2})).as_bytes()).await.unwrap();
        wait_for_pid(&pidfile).await;
        client.write_all(call(json!(2), "export_report", json!({"program": CIRCUMCENTER, "timeout_secs": 60})).as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        drop(client);
        let t = std::time::Instant::now();
        let queued = out.wait_for(json!(2)).await;
        assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
        assert_eq!(queued["result"]["isError"], true, "{queued}");
        assert!(queued["result"]["content"][0]["text"].as_str().unwrap().contains("not started"), "{queued}");
        let hard = Duration::from_secs(2) + worker::GRACE;
        stopped(server, hard + Duration::from_secs(3)).await.unwrap();
        let running = out.responses().into_iter().find(|r| r["id"] == 1).expect("the running call answers");
        assert!(running["result"]["content"][0]["text"].as_str().unwrap().contains("time limit"), "{running}");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_reused_id_is_refused_so_cancel_still_reaches_the_first_call() {
        use crate::worker::tests::{hang_input, wait_for_pid, wait_reaped, CIRCUMCENTER};
        use tokio::io::AsyncWriteExt;
        let dir = tempfile::tempdir().unwrap();
        let (program, pidfile) = hang_input(dir.path(), "pid");
        let (mut client, server_end) = tokio::io::duplex(1 << 16);
        let out = Captured::default();
        let server = tokio::spawn(serve_io(tokio::io::BufReader::new(server_end), out.clone(), 2));
        client.write_all(call(json!(7), "solve_geometry", json!({"program": program, "timeout_secs": 60})).as_bytes()).await.unwrap();
        let pid = wait_for_pid(&pidfile).await;
        client.write_all(call(json!(7), "solve_geometry", json!({"program": CIRCUMCENTER})).as_bytes()).await.unwrap();
        let dup = out.wait_for(json!(7)).await;
        assert_eq!(dup["error"]["code"], INVALID_REQUEST, "{dup}");

        let cancel = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}});
        client.write_all(format!("{cancel}\n").as_bytes()).await.unwrap();
        wait_reaped(pid, Duration::from_secs(3)).await;
        drop(client);
        stopped(server, Duration::from_secs(10)).await.unwrap();
        assert_eq!(out.responses().iter().filter(|r| r["id"] == 7).count(), 1, "{:?}", out.responses());
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

    #[tokio::test]
    async fn export_rejects_unknown_formats_and_writes_inside_the_dir() {
        let dir = tempfile::tempdir().unwrap();
        let prog = "A B C = triangle\nO = circumcenter(A, B, C)\nprove cong(O, A, O, B)";
        let r = tool_export_in(&json!({"program": prog, "format": "svg"}), dir.path(), None).await;
        assert_eq!(r["isError"], true, "{r}");
        let r = tool_export_in(&json!({"program": prog, "filename": "../escape.pdf"}), dir.path(), None).await;
        assert_eq!(r["isError"], true, "{r}");
        let r = tool_export_in(&json!({"program": prog, "filename": "ok.pdf"}), dir.path(), None).await;
        assert_eq!(r["isError"], false, "{r}");
        let bytes = std::fs::read(dir.path().join("ok.pdf")).unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }
}
