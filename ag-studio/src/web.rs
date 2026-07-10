//! The local web app: an axum server exposing a single-page UI plus a small
//! JSON API that wraps the solver, the translator, and the PDF/PNG exporter.
//!
//! Hardened for exposure behind a reverse proxy (see [`crate::security`]): body
//! limits, a concurrency gate, per-IP rate limiting, optional HTTP Basic auth,
//! security headers, gzip compression, and a request timeout.

use std::io::Write;
use std::net::SocketAddr;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;

use crate::engine::{self, InputKind, SolveOptions};
use crate::security::{self, AppState, Config, Shared};
use crate::{auth, db, i18n, render, translate};
use ddar::svg::Theme;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const AUTH_HTML: &str = include_str!("../assets/auth.html");
const LANDING_HTML: &str = include_str!("../assets/landing.html");
const I18N_JS: &str = include_str!("../assets/i18n.js");
/// Hard ceiling on a decoded upload, independent of the body limit.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

/// Run the web app until the process is stopped, using the given configuration.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind = config.bind;
    let translate_note = if !config.enable_translate {
        "disabled by configuration".to_string()
    } else {
        match translate::status() {
            s if s.logged_in => "on (local Claude subscription)".into(),
            s if s.installed => "installed — run `claude auth login` to enable".into(),
            _ => "off — install the `claude` CLI to enable".into(),
        }
    };
    let summary = config.summary();
    let state = AppState::new(config).map_err(|e| anyhow::anyhow!(e))?;
    let app = app_router(state);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    println!("\n  GeoSolver  →  http://{bind}\n");
    println!("  AI translation: {translate_note}");
    println!("  Security: {summary}");
    if bind.ip().is_loopback() {
        println!("  (loopback only — put a reverse proxy in front to expose it)");
    }
    println!("  Press Ctrl-C to stop.\n");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Assemble the full application router (routes + middleware) for `state`.
/// Extracted from [`serve`] so integration tests can drive it without a socket.
fn app_router(state: Shared) -> Router {
    let body_limit = state.config.max_body_bytes;
    Router::new()
        .route("/", get(index))
        .route("/app", get(app_page))
        .route("/auth", get(auth_page))
        .route("/assets/i18n.js", get(i18n_js))
        .route("/healthz", get(healthz))
        .route("/api/status", get(api_status))
        .route("/api/solve", post(api_solve))
        .route("/api/translate", post(api_translate))
        .route("/api/humanize", post(api_humanize))
        .route("/api/export", post(api_export))
        .route("/api/history", get(api_history))
        .route("/api/history/{id}", axum::routing::delete(api_history_delete))
        .route("/api/auth/register", post(api_register))
        .route("/api/auth/login", post(api_login))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/auth/me", get(api_me))
        // Innermost first; the last layer added is the outermost.
        .layer(axum::middleware::from_fn(security::same_origin))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::rate_limit,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::auth,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::security_headers,
        ))
        .layer(CompressionLayer::new())
        // Covers a translate (75 s) or an Opus proof rewrite (~10–30 s, 90 s cap);
        // ordinary requests still return in well under a second.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::GATEWAY_TIMEOUT,
            Duration::from_secs(120),
        ))
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

/// `/`: signed-in visitors go straight to the app; everyone else sees the
/// public landing page pitching the product.
async fn index(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if auth::current_user(&state.db, &headers).is_some() {
        Redirect::to("/app").into_response()
    } else {
        Html(LANDING_HTML).into_response()
    }
}

/// `/app`: the solver SPA, gated on a valid session (else 302 to `/auth`).
async fn app_page(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if auth::current_user(&state.db, &headers).is_some() {
        Html(INDEX_HTML).into_response()
    } else {
        Redirect::to("/auth").into_response()
    }
}

/// The shared client-side i18n engine + string catalog (English/Romanian).
async fn i18n_js() -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/javascript; charset=utf-8"),
        )],
        I18N_JS,
    )
        .into_response()
}

/// The login/register page (public — its own JS calls the `/api/auth/*` routes).
async fn auth_page() -> Html<&'static str> {
    Html(AUTH_HTML)
}

/// Unauthenticated liveness probe for container / load-balancer health checks.
async fn healthz() -> &'static str {
    "ok"
}

#[derive(Serialize)]
struct Status {
    translate_installed: bool,
    translate_logged_in: bool,
    version: &'static str,
}

async fn api_status(State(state): State<Shared>) -> Json<Status> {
    let s = translate::status();
    let enabled = state.config.enable_translate;
    Json(Status {
        translate_installed: enabled && s.installed,
        translate_logged_in: enabled && s.logged_in,
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ------------------------------------------------------------------ auth ----
//
// Public routes (no session gate) that manage accounts: register/login mint a
// session cookie, logout clears it, `me` reports the current user. All password
// hashing + SQLite work runs on the blocking pool. These sit behind the same
// rate-limit / Basic-auth / security-header layers as everything else.

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
}

enum RegisterErr {
    Taken,
    Internal,
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation)
}

/// Validate a username (returning its trimmed, canonical form) and password length.
#[allow(clippy::result_large_err)]
fn valid_credentials(username: &str, password: &str, lang: i18n::Lang) -> Result<String, Response> {
    let u = username.trim();
    let ok_name = (3..=32).contains(&u.chars().count())
        && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !ok_name {
        return Err(err(
            StatusCode::BAD_REQUEST,
            i18n::t(lang, "auth.username_rule"),
        ));
    }
    if !(8..=128).contains(&password.chars().count()) {
        return Err(err(StatusCode::BAD_REQUEST, i18n::t(lang, "auth.pw_len")));
    }
    Ok(u.to_string())
}

/// A `{ "username": … }` body plus a session `Set-Cookie`.
fn auth_ok(username: &str, sid: &str, secure: bool) -> Response {
    (
        [(header::SET_COOKIE, auth::set_cookie_header(sid, secure))],
        Json(serde_json::json!({ "username": username })),
    )
        .into_response()
}

async fn api_register(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<Credentials>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let username = match valid_credentials(&req.username, &req.password, lang) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let secure = state.config.secure_cookies;
    let db = state.db.clone();
    let outcome = tokio::task::spawn_blocking(move || -> Result<(String, String), RegisterErr> {
        let hash = auth::hash_password(&req.password).map_err(|_| RegisterErr::Internal)?;
        let conn = db.lock().map_err(|_| RegisterErr::Internal)?;
        let uid = db::create_user(&conn, &username, &hash).map_err(|e| {
            if is_unique_violation(&e) {
                RegisterErr::Taken
            } else {
                RegisterErr::Internal
            }
        })?;
        let sid = auth::new_session_id();
        db::insert_session(&conn, &sid, uid, auth::SESSION_TTL_SECS)
            .map_err(|_| RegisterErr::Internal)?;
        Ok((username, sid))
    })
    .await;
    match outcome {
        Ok(Ok((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(Err(RegisterErr::Taken)) => err(StatusCode::CONFLICT, i18n::t(lang, "auth.user_taken")),
        Ok(Err(RegisterErr::Internal)) => {
            eprintln!("register failed: internal error hashing/creating the account");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.create_fail"))
        }
        Err(e) => {
            eprintln!("register panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.create_fail"))
        }
    }
}

async fn api_login(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<Credentials>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let secure = state.config.secure_cookies;
    let db = state.db.clone();
    // `None` is returned for every failure mode (unknown user, bad password) so
    // the client can't distinguish them; the argon2 verify runs off the DB lock.
    let outcome = tokio::task::spawn_blocking(move || -> Option<(String, String)> {
        let (uid, username, hash) = {
            let conn = db.lock().ok()?;
            let u = db::find_user_by_name(&conn, req.username.trim()).ok()??;
            (u.id, u.username, u.password_hash)
        };
        if !auth::verify_password(&req.password, &hash) {
            return None;
        }
        let sid = auth::new_session_id();
        let conn = db.lock().ok()?;
        db::insert_session(&conn, &sid, uid, auth::SESSION_TTL_SECS).ok()?;
        Some((username, sid))
    })
    .await;
    match outcome {
        Ok(Some((username, sid))) => auth_ok(&username, &sid, secure),
        Ok(None) => err(StatusCode::UNAUTHORIZED, i18n::t(lang, "auth.bad_creds")),
        Err(e) => {
            eprintln!("login panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "auth.login_fail"))
        }
    }
}

async fn api_logout(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if let Some(sid) = auth::parse_sid(&headers) {
        if let Ok(conn) = state.db.lock() {
            let _ = db::delete_session(&conn, &sid);
        }
    }
    (
        [(header::SET_COOKIE, auth::clear_cookie_header())],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

async fn api_me(State(state): State<Shared>, headers: HeaderMap) -> Response {
    match auth::current_user(&state.db, &headers) {
        Some(u) => Json(serde_json::json!({ "username": u.username })).into_response(),
        None => err(
            StatusCode::UNAUTHORIZED,
            i18n::t(i18n::lang_from_headers(&headers), "auth.not_signed_in"),
        ),
    }
}

fn kind_of(input: &str, kind: &str) -> InputKind {
    match kind {
        "lowlevel" | "low-level" => InputKind::LowLevel,
        "geo" => InputKind::Geo,
        _ => InputKind::detect(input),
    }
}

/// A JSON `{ "error": "…" }` response with a status code.
fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

/// Reject empty or over-long programs early. (The `Response` error is axum's
/// own large type — boxing it everywhere would only add noise.)
#[allow(clippy::result_large_err)]
fn check_input(state: &Shared, headers: &HeaderMap, input: &str) -> Result<(), Response> {
    let lang = i18n::lang_from_headers(headers);
    if input.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, i18n::t(lang, "solve.empty")));
    }
    if input.len() > state.config.max_input_chars {
        return Err(err(
            StatusCode::PAYLOAD_TOO_LARGE,
            i18n::t(lang, "err.too_large"),
        ));
    }
    Ok(())
}

/// Require a valid session cookie, or return 401 JSON. Guards the solver,
/// translate, and export APIs — the account routes above stay public.
#[allow(clippy::result_large_err)]
fn require_session(state: &Shared, headers: &HeaderMap) -> Result<(), Response> {
    if auth::current_user(&state.db, headers).is_some() {
        Ok(())
    } else {
        let lang = i18n::lang_from_headers(headers);
        Err(err(
            StatusCode::UNAUTHORIZED,
            i18n::t(lang, "auth.sign_in_required"),
        ))
    }
}

/// Acquire a heavy-work slot, or return 503 if the server is at capacity.
#[allow(clippy::result_large_err)]
fn heavy_permit(
    state: &Shared,
    headers: &HeaderMap,
) -> Result<tokio::sync::OwnedSemaphorePermit, Response> {
    state.heavy.clone().try_acquire_owned().map_err(|_| {
        let lang = i18n::lang_from_headers(headers);
        err(StatusCode::SERVICE_UNAVAILABLE, i18n::t(lang, "server.busy"))
    })
}

// ---------------------------------------------------------------- solve ----

#[derive(Deserialize)]
struct SolveReq {
    input: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    theme: String,
    #[serde(default)]
    title: Option<String>,
    /// Search for the shortest proof within `budget_secs`.
    #[serde(default)]
    best: bool,
    #[serde(default)]
    budget_secs: Option<f64>,
}

async fn api_solve(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<SolveReq>,
) -> Response {
    let user = match auth::current_user(&state.db, &headers) {
        Some(u) => u,
        None => return err(StatusCode::UNAUTHORIZED, i18n::t(i18n::lang_from_headers(&headers), "auth.sign_in_required")),
    };
    if let Err(e) = check_input(&state, &headers, &req.input) {
        return e;
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let history_title = req.title.clone();
    let opts = SolveOptions {
        kind: kind_of(&req.input, &req.kind),
        theme: engine::parse_theme(&req.theme),
        want_proof: true,
        title: req.title,
        // The UI lays the legend out in HTML below the figure, so the drawing
        // gets the whole card (matters on phones).
        panel: false,
    };
    let input = req.input;
    let best = req.best;
    // Reject non-finite budgets; clamp to a server-safe ceiling.
    let budget = Duration::from_secs_f64(
        req.budget_secs
            .filter(|v| v.is_finite())
            .unwrap_or(20.0)
            .clamp(0.5, 60.0),
    );
    let task = tokio::task::spawn_blocking(move || {
        let _permit = permit; // hold the slot until the work actually finishes
        if best {
            engine::solve_best(&input, &opts, budget)
        } else {
            engine::solve(&input, &opts)
        }
    });
    match task.await {
        Ok(Ok(sol)) => {
            save_history(&state, user.id, &sol, history_title.as_deref()).await;
            Json(sol).into_response()
        }
        Ok(Err(e)) => {
            eprintln!("solve error: {e}"); // detail to the operator's log, not the client
            err(StatusCode::BAD_REQUEST, e) // compile/parse errors describe the user's input
        }
        Err(e) => {
            eprintln!("solve panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "solve.failed"))
        }
    }
}

fn method_str(m: engine::Method) -> &'static str {
    match m {
        engine::Method::Ddar => "ddar",
        engine::Method::AuxSearch => "aux-search",
        engine::Method::Euclidean => "euclidean",
    }
}

/// Record a completed solve (proved or not) in the caller's history. Best-effort:
/// a storage error here must never fail the solve response itself.
async fn save_history(state: &Shared, user_id: i64, sol: &engine::Solution, title: Option<&str>) {
    let db = state.db.clone();
    let (input, proved, method) = (sol.input.clone(), sol.proved, method_str(sol.method));
    let title = title.map(str::to_string);
    let _ = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| ())?;
        db::insert_history(&conn, user_id, &input, title.as_deref(), proved, Some(method))
            .map_err(|_| ())
    })
    .await;
}

// -------------------------------------------------------------- history ----

#[derive(Serialize)]
struct HistoryItem {
    id: i64,
    input: String,
    title: Option<String>,
    proved: bool,
    method: Option<String>,
    created_at: i64,
}

impl From<db::HistoryEntry> for HistoryItem {
    fn from(h: db::HistoryEntry) -> Self {
        HistoryItem { id: h.id, input: h.input, title: h.title, proved: h.proved, method: h.method, created_at: h.created_at }
    }
}

/// The signed-in user's past solves, newest first.
async fn api_history(State(state): State<Shared>, headers: HeaderMap) -> Response {
    let user = match auth::current_user(&state.db, &headers) {
        Some(u) => u,
        None => return err(StatusCode::UNAUTHORIZED, i18n::t(i18n::lang_from_headers(&headers), "auth.sign_in_required")),
    };
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| ())?;
        db::list_history(&conn, user.id).map_err(|_| ())
    })
    .await;
    match res {
        Ok(Ok(rows)) => Json(rows.into_iter().map(HistoryItem::from).collect::<Vec<_>>()).into_response(),
        Ok(Err(())) => {
            eprintln!("history load failed: db error");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.load_fail"))
        }
        Err(e) => {
            eprintln!("history load panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.load_fail"))
        }
    }
}

/// Delete one of the signed-in user's history entries (others' ids are 404).
async fn api_history_delete(
    State(state): State<Shared>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    headers: HeaderMap,
) -> Response {
    let user = match auth::current_user(&state.db, &headers) {
        Some(u) => u,
        None => return err(StatusCode::UNAUTHORIZED, i18n::t(i18n::lang_from_headers(&headers), "auth.sign_in_required")),
    };
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = db.lock().map_err(|_| ())?;
        db::delete_history(&conn, user.id, id).map_err(|_| ())
    })
    .await;
    match res {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => err(StatusCode::NOT_FOUND, i18n::t(i18n::lang_from_headers(&headers), "history.none")),
        Ok(Err(())) => {
            eprintln!("history delete failed: db error");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.del_fail"))
        }
        Err(e) => {
            eprintln!("history delete panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "history.del_fail"))
        }
    }
}

// ------------------------------------------------------------ translate ----

#[derive(Deserialize)]
struct TranslateReq {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    image_base64: Option<String>,
    #[serde(default)]
    filename: Option<String>,
}

async fn api_translate(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<TranslateReq>,
) -> Response {
    if let Err(e) = require_session(&state, &headers) {
        return e;
    }
    if !state.config.enable_translate || !translate::available() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            "AI translation is not enabled on this server",
        );
    }
    if let Some(t) = &req.text {
        if t.len() > state.config.max_input_chars {
            return err(StatusCode::PAYLOAD_TOO_LARGE, i18n::t(i18n::lang_from_headers(&headers), "translate.too_long"));
        }
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // Build the source. An uploaded image goes to a private, auto-deleted temp
    // file (random name, mode 0600) via `tempfile`.
    enum Held {
        Text(String),
        Image(tempfile::NamedTempFile),
    }
    let held = match (req.image_base64, req.text) {
        (Some(b64), _) => match decode_image(&b64, req.filename.as_deref()) {
            Ok(tmp) => Held::Image(tmp),
            Err(e) => {
                eprintln!("translate: bad image upload: {e}");
                return err(StatusCode::BAD_REQUEST, i18n::t(i18n::lang_from_headers(&headers), "translate.bad_image"));
            }
        },
        (None, Some(t)) if !t.trim().is_empty() => Held::Text(t),
        _ => return err(StatusCode::BAD_REQUEST, i18n::t(i18n::lang_from_headers(&headers), "translate.need_input")),
    };

    let res = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let source = match &held {
            Held::Text(t) => translate::Source::Text(t.clone()),
            Held::Image(tmp) => translate::Source::Image(tmp.path().to_path_buf()),
        };
        let out = translate::translate(&source);
        drop(held); // RAII: the temp image is deleted here, on every path
        out
    })
    .await;

    match res {
        Ok(Ok(t)) => Json(t).into_response(),
        Ok(Err(e)) => {
            eprintln!("translate error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::BAD_GATEWAY,
                "translation failed — could not turn that into a geometry problem",
            )
        }
        Err(e) => {
            eprintln!("translate panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "translate.failed"))
        }
    }
}

// ------------------------------------------------------------ humanize ----

#[derive(Deserialize)]
struct HumanizeReq {
    /// The readable problem (given hypotheses + goal, and/or the compiled `.geo`).
    #[serde(default)]
    problem: String,
    /// The machine (DDAR) proof to rewrite.
    #[serde(default)]
    proof: String,
    /// Auxiliary constructions the search introduced, if any.
    #[serde(default)]
    aux: Vec<String>,
    /// Output language ("en" or "ro"); defaults to English.
    #[serde(default)]
    lang: String,
}

/// Rewrite a machine (DDAR) proof into a flowing, human-readable proof in the
/// chosen language, via the local Claude Opus subscription. Same
/// session/availability/rate-limit posture as `/api/translate`; any failure
/// degrades to a 503 so the client can fall back to the machine proof.
async fn api_humanize(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<HumanizeReq>,
) -> Response {
    if let Err(e) = require_session(&state, &headers) {
        return e;
    }
    let lang = i18n::lang_from_headers(&headers);
    if req.proof.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, i18n::t(lang, "humanize.need_proof"));
    }
    let over_limit = req.problem.len() > state.config.max_input_chars
        || req.proof.len() > 4 * state.config.max_input_chars;
    if over_limit {
        return err(StatusCode::PAYLOAD_TOO_LARGE, i18n::t(lang, "err.too_long"));
    }
    if !state.config.enable_translate || !translate::available() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            i18n::t(lang, "humanize.unavailable"),
        );
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let tlang = translate::Lang::from_code(&req.lang);
    let HumanizeReq {
        problem,
        proof,
        aux,
        ..
    } = req;
    let res = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        translate::humanize_proof(&problem, &proof, &aux, tlang)
    })
    .await;
    match res {
        Ok(Ok(text)) => Json(serde_json::json!({ "proof": text })).into_response(),
        Ok(Err(e)) => {
            eprintln!("humanize error: {e}"); // detail to the operator's log, not the client
            err(
                StatusCode::SERVICE_UNAVAILABLE,
                i18n::t(lang, "humanize.failed"),
            )
        }
        Err(e) => {
            eprintln!("humanize panicked: {e}");
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                i18n::t(lang, "humanize.failed"),
            )
        }
    }
}

/// Decode a base64 (optionally data-URL) image into a private temp file.
fn decode_image(b64: &str, filename: Option<&str>) -> anyhow::Result<tempfile::NamedTempFile> {
    use base64::Engine;
    let data = b64.rsplit(',').next().unwrap_or(b64).trim();
    let bytes = base64::engine::general_purpose::STANDARD.decode(data)?;
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("image out of range");
    }
    let ext = filename
        .and_then(|f| std::path::Path::new(f).extension())
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .filter(|e| matches!(e.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp"))
        .unwrap_or_else(|| "png".to_string());
    let mut f = tempfile::Builder::new()
        .prefix("agstudio_upload_")
        .suffix(&format!(".{ext}"))
        .tempfile()?;
    f.write_all(&bytes)?;
    f.flush()?;
    Ok(f)
}

// --------------------------------------------------------------- export ----

#[derive(Deserialize)]
struct ExportReq {
    input: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    format: String,
}

async fn api_export(
    State(state): State<Shared>,
    headers: HeaderMap,
    Json(req): Json<ExportReq>,
) -> Response {
    if let Err(e) = require_session(&state, &headers) {
        return e;
    }
    if let Err(e) = check_input(&state, &headers, &req.input) {
        return e;
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };
    // Documents export on a light, print-friendly page regardless of on-screen theme.
    let opts = SolveOptions {
        kind: kind_of(&req.input, &req.kind),
        theme: Theme::Light,
        want_proof: true,
        title: req.title.clone(),
        panel: true,
    };
    let input = req.input;
    let title = req.title;
    let want_pdf = req.format.eq_ignore_ascii_case("pdf");

    let res = tokio::task::spawn_blocking(
        move || -> anyhow::Result<(Vec<u8>, &'static str, &'static str)> {
            let _permit = permit;
            let sol = engine::solve(&input, &opts).map_err(|e| anyhow::anyhow!(e))?;
            let report = render::report_svg(&sol, title.as_deref(), true);
            if want_pdf {
                Ok((
                    render::svg_to_pdf(&report)?,
                    "application/pdf",
                    "attachment; filename=\"geosolver-proof.pdf\"",
                ))
            } else {
                Ok((
                    render::svg_to_png(&report, 2.0)?,
                    "image/png",
                    "attachment; filename=\"geosolver-proof.png\"",
                ))
            }
        },
    )
    .await;

    match res {
        Ok(Ok((bytes, content_type, disposition))) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_DISPOSITION, disposition)
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed"))),
        Ok(Err(e)) => {
            eprintln!("export error: {e}");
            err(StatusCode::BAD_REQUEST, format!("{e}"))
        }
        Err(e) => {
            eprintln!("export panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "export.failed"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt; // for `oneshot`

    /// A fresh server state backed by a throwaway on-disk DB (kept alive by the
    /// returned `TempDir`).
    fn test_state() -> (Shared, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        (AppState::new(config).unwrap(), dir)
    }

    /// Drive one request through the full router; return (status, `Set-Cookie`, body-json).
    async fn call(
        state: &Shared,
        method: &str,
        uri: &str,
        cookie: Option<&str>,
        body: serde_json::Value,
    ) -> (StatusCode, Option<String>, serde_json::Value) {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let req = b.body(Body::from(body.to_string())).unwrap();
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        let status = resp.status();
        let set_cookie = resp
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, set_cookie, json)
    }

    /// The `sid=…` pair from a `Set-Cookie`, ready to re-send as a `Cookie` header.
    fn cookie_of(set: &str) -> String {
        set.split(';').next().unwrap().to_string()
    }

    fn creds(user: &str, pass: &str) -> serde_json::Value {
        serde_json::json!({ "username": user, "password": pass })
    }

    #[tokio::test]
    async fn register_me_logout_flow() {
        let (state, _dir) = test_state();
        let (st, set, body) =
            call(&state, "POST", "/api/auth/register", None, creds("alice", "hunter2hunter")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["username"], "alice");
        let cookie = cookie_of(&set.expect("register sets a session cookie"));
        assert!(cookie.starts_with("sid="));

        // `me` sees the session; without the cookie it is 401.
        let (st, _, body) =
            call(&state, "GET", "/api/auth/me", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["username"], "alice");
        let (st, _, _) =
            call(&state, "GET", "/api/auth/me", None, serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // Logout clears the cookie and invalidates the session.
        let (st, set, _) =
            call(&state, "POST", "/api/auth/logout", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert!(set.unwrap().contains("Max-Age=0"));
        let (st, _, _) =
            call(&state, "GET", "/api/auth/me", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn login_verifies_password() {
        let (state, _dir) = test_state();
        call(&state, "POST", "/api/auth/register", None, creds("carol", "correcthorse")).await;
        let (st, _, _) =
            call(&state, "POST", "/api/auth/login", None, creds("carol", "wrongwrongwrong")).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (st, _, _) =
            call(&state, "POST", "/api/auth/login", None, creds("nobody", "correcthorse")).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED); // unknown user is indistinguishable
        let (st, set, body) =
            call(&state, "POST", "/api/auth/login", None, creds("carol", "correcthorse")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["username"], "carol");
        assert!(set.unwrap().starts_with("sid="));
    }

    #[tokio::test]
    async fn duplicate_registration_conflicts() {
        let (state, _dir) = test_state();
        let (st, _, _) =
            call(&state, "POST", "/api/auth/register", None, creds("bob", "password123")).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _, _) =
            call(&state, "POST", "/api/auth/register", None, creds("bob", "password123")).await;
        assert_eq!(st, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn registration_validates_input() {
        let (state, _dir) = test_state();
        for (user, pass) in [("dave", "short"), ("ab", "longenoughpw"), ("has space", "longenoughpw")]
        {
            let (st, _, _) =
                call(&state, "POST", "/api/auth/register", None, creds(user, pass)).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "expected {user:?}/{pass:?} rejected");
        }
    }

    #[tokio::test]
    async fn auth_page_is_public() {
        let (state, _dir) = test_state();
        let req = Request::builder().uri("/auth").body(Body::empty()).unwrap();
        let resp = app_router(state).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("Create account"));
    }

    /// Drive a cookie-less/bodyless GET through the full router.
    async fn get_page(state: &Shared, uri: &str, cookie: Option<&str>) -> (StatusCode, HeaderMap, String) {
        let mut b = Request::builder().uri(uri);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let resp = app_router(state.clone()).oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Register a throwaway user and return their session cookie (`sid=…`).
    async fn register_cookie(state: &Shared, user: &str) -> String {
        let (_, set, _) =
            call(state, "POST", "/api/auth/register", None, creds(user, "password123")).await;
        cookie_of(&set.expect("register sets a session cookie"))
    }

    #[tokio::test]
    async fn root_redirects_signed_in_users_else_shows_landing() {
        let (state, _dir) = test_state();
        let (st, _, body) = get_page(&state, "/", None).await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.contains("Create a free account"), "logged-out `/` should show the landing page");

        let cookie = register_cookie(&state, "erin").await;
        let (st, headers, _) = get_page(&state, "/", Some(&cookie)).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(headers.get(header::LOCATION).unwrap(), "/app");
    }

    #[tokio::test]
    async fn app_page_requires_session() {
        let (state, _dir) = test_state();
        let (st, headers, _) = get_page(&state, "/app", None).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(headers.get(header::LOCATION).unwrap(), "/auth");

        let cookie = register_cookie(&state, "frank").await;
        let (st, _, body) = get_page(&state, "/app", Some(&cookie)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.contains("GeoSolver"));
    }

    const ISOSCELES_GEO: &str =
        "B C = segment\nA = point: dist(A, B) = dist(A, C)\nprove eqangle(B, C, B, A, C, A, C, B)";

    #[tokio::test]
    async fn history_is_recorded_and_scoped_per_user() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "helen").await;

        for title in ["first", "second"] {
            let (st, _, _) = call(
                &state,
                "POST",
                "/api/solve",
                Some(&cookie),
                serde_json::json!({"input": ISOSCELES_GEO, "title": title}),
            )
            .await;
            assert_eq!(st, StatusCode::OK);
        }

        let (st, _, body) =
            call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        let rows = body.as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["title"], "second"); // newest first
        assert_eq!(rows[1]["title"], "first");
        assert_eq!(rows[0]["proved"], true);
        assert_eq!(rows[0]["method"], "ddar");
        // Reopening from history must restore the exact multi-line .geo program
        // (newlines, indentation, everything) the user originally submitted.
        assert_eq!(rows[0]["input"], ISOSCELES_GEO);
        assert_eq!(rows[1]["input"], ISOSCELES_GEO);

        // A second user has their own, empty history.
        let other_cookie = register_cookie(&state, "ivan").await;
        let (st, _, body) =
            call(&state, "GET", "/api/history", Some(&other_cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert!(body.as_array().unwrap().is_empty());

        let (st, _, _) =
            call(&state, "GET", "/api/history", None, serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    /// A `.geo` program with 500 levels of `reflect(...)` nesting in a
    /// construction is well under the 16,384-char body limit (so it reaches
    /// the parser), and its goal (`coll(...)`) is not a metric goal, so it
    /// routes through `geo::compile` — where it must be rejected as a clean
    /// 400 by the depth cap added in geo.rs — not hang, time out, or (in the
    /// pre-fix world) risk a stack overflow.
    #[tokio::test]
    async fn deeply_nested_solve_input_is_rejected_cleanly() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "sam").await;
        let mut input = String::from("A = free\nB = ");
        for _ in 0..500 {
            input.push_str("reflect(");
        }
        input.push('A');
        for _ in 0..500 {
            input.push(')');
        }
        input.push_str("\nprove coll(A, A, B)");
        assert!(input.len() < 16384, "test input must stay under the body cap");
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "got: {body:?}");
    }

    /// A metric goal (e.g. `dist(A,B) = <deeply nested>`) routes through
    /// `euclidean_flow`, not `geo::compile` — so it hits Task 2's `metric.rs`
    /// depth cap, not Task 1's `geo.rs` cap. That path's pre-existing design
    /// folds every prover failure into a normal 200/`proved:false` response
    /// (not a 400) — this test locks in that the cap still fires safely and
    /// fast through that path, not that it produces a 400.
    #[tokio::test]
    async fn deeply_nested_metric_goal_is_rejected_safely_not_accepted_or_crashed() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "vic").await;
        let mut input = String::from("prove dist(A, B) = ");
        input.push_str(&"-".repeat(500));
        input.push('5');
        assert!(input.len() < 16384, "test input must stay under the body cap");
        let start = std::time::Instant::now();
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "must reject fast, not hang"
        );
        assert_eq!(
            st,
            StatusCode::OK,
            "the euclidean-flow path folds prover failures into 200/proved:false, not a 400 — got: {body:?}"
        );
        assert_eq!(body["proved"], false);
        assert!(
            body["note"].as_str().unwrap_or("").contains("nested too deeply"),
            "expected the metric.rs cap's message in the note, got: {body:?}"
        );
    }

    /// A `.geo` program declaring 500 points is well under the body limit,
    /// but must be rejected quickly by the point-count cap added in geo.rs —
    /// not accepted and left to blow up `Ddar::new`'s O(n^2) allocation.
    #[tokio::test]
    async fn too_many_points_solve_input_is_rejected_quickly() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "tara").await;
        let mut input = String::new();
        for i in 0..500 {
            input.push_str(&format!("p{i} = free\n"));
        }
        input.push_str("prove coll(p0, p0, p0)");
        assert!(input.len() < 16384, "test input must stay under the body cap");
        let start = std::time::Instant::now();
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "rejection must be fast, not run the expensive instance-search loop first"
        );
        assert_eq!(st, StatusCode::BAD_REQUEST, "got: {body:?}");
    }

    /// Firing several adversarial and several valid solves concurrently must
    /// not let one request's failure affect another's success — each request
    /// gets its own `spawn_blocking` task and its own response.
    #[tokio::test]
    async fn concurrent_adversarial_and_valid_solves_do_not_interfere() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "uma").await;
        let mut bad_input = String::from("A = free\nB = ");
        for _ in 0..500 {
            bad_input.push_str("reflect(");
        }
        bad_input.push('A');
        for _ in 0..500 {
            bad_input.push(')');
        }
        bad_input.push_str("\nprove coll(A, A, B)");

        let mut tasks = Vec::new();
        for i in 0..8 {
            let state = state.clone();
            let cookie = cookie.clone();
            let input = if i % 2 == 0 {
                bad_input.clone()
            } else {
                ISOSCELES_GEO.to_string()
            };
            let expect_ok = i % 2 != 0;
            tasks.push(tokio::spawn(async move {
                let (st, _, body) = call(
                    &state,
                    "POST",
                    "/api/solve",
                    Some(&cookie),
                    serde_json::json!({"input": input}),
                )
                .await;
                let expected = if expect_ok { StatusCode::OK } else { StatusCode::BAD_REQUEST };
                assert_eq!(st, expected, "request {i} got: {body:?}");
            }));
        }
        for t in tasks {
            t.await.expect("request task panicked");
        }
    }

    #[tokio::test]
    async fn solve_translate_export_require_session() {
        let (state, _dir) = test_state();
        for uri in ["/api/solve", "/api/translate", "/api/export", "/api/humanize"] {
            let (st, _, _) =
                call(&state, "POST", uri, None, serde_json::json!({"input": "", "step": ""})).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "expected {uri} to require a session");
        }
        // Empty input still fails validation *after* the gate — this confirms the
        // gate actually passed rather than coincidentally also returning 401.
        let cookie = register_cookie(&state, "gail").await;
        let (st, _, _) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ""})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    /// `AGSTUDIO_BASIC_AUTH` layers *in front of* session auth: it gates every
    /// route (including the public `/auth` page and `/api/auth/*`), and a
    /// session cookie alone is not enough to skip it.
    #[tokio::test]
    async fn basic_auth_layers_over_session_auth() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.basic_auth = Some("Basic b3A6c2VjcmV0".to_string()); // "op:secret"
        let state = AppState::new(config).unwrap();

        // No Basic auth at all: rejected before the route (or any session
        // cookie) is even considered — even the public `/auth` page.
        let req = Request::builder().uri("/auth").body(Body::empty()).unwrap();
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        assert!(resp.headers().contains_key(header::WWW_AUTHENTICATE));

        // `/healthz` is the sole exception (unauthenticated liveness probe).
        let req = Request::builder().uri("/healthz").body(Body::empty()).unwrap();
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // With correct Basic auth, register/login/me still work exactly as
        // without it — the two layers are independent and additive.
        let basic = "Basic b3A6c2VjcmV0"; // "op:secret"
        let req = Request::builder()
            .method("POST")
            .uri("/api/auth/register")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::AUTHORIZATION, basic)
            .body(Body::from(creds("kay", "password123").to_string()))
            .unwrap();
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let cookie = cookie_of(
            resp.headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap(),
        );

        // Correct Basic auth but no session cookie: the API route still
        // demands a session (the two gates are independent, not either/or).
        let req = Request::builder()
            .uri("/api/auth/me")
            .header(header::AUTHORIZATION, basic)
            .body(Body::empty())
            .unwrap();
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Both Basic auth and the session cookie present: succeeds.
        let req = Request::builder()
            .uri("/api/auth/me")
            .header(header::AUTHORIZATION, basic)
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap();
        let resp = app_router(state).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    /// Security headers apply uniformly, including on the new `/auth`, `/app`,
    /// and landing-page `/` routes (not just the old `/api/*` surface).
    #[tokio::test]
    async fn security_headers_present_on_new_routes() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "leo").await;
        for (uri, cookie_hdr) in [("/", None), ("/auth", None), ("/app", Some(cookie.as_str()))] {
            let (_, headers, _) = get_page(&state, uri, cookie_hdr).await;
            assert_eq!(
                headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
                "nosniff",
                "missing on {uri}"
            );
            assert_eq!(headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY", "missing on {uri}");
            assert!(
                headers.get(header::CONTENT_SECURITY_POLICY).is_some(),
                "missing CSP on {uri}"
            );
        }
    }

    #[tokio::test]
    async fn humanize_is_503_when_translate_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.enable_translate = false;
        let state = AppState::new(config).unwrap();
        let cookie = register_cookie(&state, "judy").await;
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": "prove coll A B C", "proof": "001. assumption: coll A B C"}),
        )
        .await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
        assert!(body["error"].as_str().unwrap().contains("not enabled"));
    }

    /// Input validation must run before the `translate::available()` check:
    /// a malformed request is rejected the same way whether or not the CLI
    /// happens to be configured on this server.
    #[tokio::test]
    async fn humanize_rejects_blank_proof_before_availability_check() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "nia").await;
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": "prove coll A B C", "proof": "   "}),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn humanize_rejects_oversized_input_before_availability_check() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "otto").await;
        let too_long = "x".repeat(state.config.max_input_chars + 1);

        let (st, _, _) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": too_long, "proof": "001. coll A B C"}),
        )
        .await;
        assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// Login/register share a tight per-IP bucket against credential stuffing.
    #[tokio::test]
    async fn auth_routes_have_their_own_rate_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.auth_per_min = 3;
        let state = AppState::new(config).unwrap();

        for i in 0..3 {
            let (st, _, _) = call(
                &state,
                "POST",
                "/api/auth/login",
                None,
                creds(&format!("ghost{i}"), "password123"),
            )
            .await;
            assert_eq!(st, StatusCode::UNAUTHORIZED); // wrong creds, but not throttled yet
        }
        let (st, _, _) =
            call(&state, "POST", "/api/auth/register", None, creds("ghost9", "password123")).await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS); // register shares the bucket

        // Other API routes are unaffected by the exhausted auth bucket.
        let (st, _, _) =
            call(&state, "GET", "/api/status", None, serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
    }

    /// A browser-sent `Origin` that doesn't match `Host` is refused on writes;
    /// a matching one (or none at all) passes.
    #[tokio::test]
    async fn cross_origin_writes_are_refused() {
        let (state, _dir) = test_state();
        let body = creds("mallory", "password123").to_string();
        let mk = |origin: Option<&str>| {
            let mut b = Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header(header::HOST, "geo.example:8787")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(o) = origin {
                b = b.header(header::ORIGIN, o);
            }
            b.body(Body::from(body.clone())).unwrap()
        };
        let resp = app_router(state.clone()).oneshot(mk(Some("https://evil.example"))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let resp = app_router(state.clone()).oneshot(mk(Some("http://geo.example:8787"))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // No Origin header (curl / non-browser clients): allowed. Duplicate
        // username here proves the request reached the handler itself.
        let resp = app_router(state).oneshot(mk(None)).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    /// API responses carry `Cache-Control: no-store` (they are per-user data).
    #[tokio::test]
    async fn api_responses_are_not_cacheable() {
        let (state, _dir) = test_state();
        let (_, headers, _) = get_page(&state, "/api/status", None).await;
        assert_eq!(headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    }

    /// `/api/translate` and `/api/explain` drive the same costly `claude` CLI
    /// subprocess, so `security::rate_limit` must count hits to either one
    /// against a single shared per-IP bucket (not two independent ones).
    #[tokio::test]
    async fn explain_and_translate_share_rate_limit_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.translate_per_min = 2;
        // Keep hits cheap (a 503, no CLI spawn): rate-limiting is a middleware
        // layer that runs before the handler, so it counts regardless.
        config.enable_translate = false;
        let state = AppState::new(config).unwrap();
        let cookie = register_cookie(&state, "priya").await;

        // The first two hits to /api/translate consume the shared bucket.
        for _ in 0..2 {
            let (st, _, _) = call(
                &state,
                "POST",
                "/api/translate",
                Some(&cookie),
                serde_json::json!({"text": "x"}),
            )
            .await;
            assert_ne!(st, StatusCode::TOO_MANY_REQUESTS);
        }
        // A third hit to /api/translate is now rate-limited.
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/translate",
            Some(&cookie),
            serde_json::json!({"text": "x"}),
        )
        .await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);

        // /api/explain shares that same bucket: it's already exhausted, so
        // this is 429 without any hits of its own against /api/explain.
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/explain",
            Some(&cookie),
            serde_json::json!({"step": "AB = CD"}),
        )
        .await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["error"], "rate limit exceeded — please slow down");
    }
}
