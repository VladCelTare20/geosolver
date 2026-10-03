//! The local web app: an axum server exposing a single-page UI plus a small
//! JSON API that wraps the solver, the translator, and the PDF/PNG exporter.
//!
//! Hardened for exposure behind a reverse proxy (see [`crate::security`]): body
//! limits, a concurrency gate, per-IP rate limiting, optional HTTP Basic auth
//! (with an account-free guest mode behind it), a Host allow-list, security
//! headers, gzip compression, and a request timeout.

use std::io::Write;
use std::net::SocketAddr;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Query, Request, State},
    http::{header, request::Parts, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tower_http::compression::CompressionLayer;
use tower_http::timeout::TimeoutLayer;

/// Wall-clock cap on one solve, well inside the 120 s request timeout so the
/// client gets the not-proven result instead of a 504.
const SOLVE_DEADLINE: Duration = Duration::from_secs(60);

use crate::engine::{self, InputKind, SolveOptions};
use crate::security::{self, AppState, Config, Shared};
use crate::{auth, db, i18n, present, render, translate};
use ddar::svg::Theme;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const AUTH_HTML: &str = include_str!("../assets/auth.html");
const LANDING_HTML: &str = include_str!("../assets/landing.html");

/// Static files under `/assets/`: (name, content type, cache policy, bytes).
const ASSETS: &[(&str, &str, &str, &[u8])] = &[
    ("i18n.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/i18n.js")),
    ("app.css", "text/css; charset=utf-8", "no-cache", include_bytes!("../assets/app.css")),
    ("site.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/site.js")),
    ("app.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/app.js")),
    ("auth.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/auth.js")),
    ("landing.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/landing.js")),
    ("fonts/stix-two-text.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/stix-two-text.woff2")),
    ("fonts/stix-two-text-italic.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/stix-two-text-italic.woff2")),
    ("fonts/inter.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/inter.woff2")),
];

/// Recent solutions by id, so export and history reopen use exactly what the
/// reader saw instead of solving again.
const SOLUTION_TTL: Duration = Duration::from_secs(30 * 60);
const SOLUTION_CACHE_MAX: usize = 128;
/// Solutions larger than this are not stored in history (reopen re-solves).
const MAX_STORED_SOLUTION_BYTES: usize = 512 * 1024;

struct SolutionCache {
    entries: Vec<(String, std::time::Instant, std::sync::Arc<serde_json::Value>)>,
}

fn solution_cache() -> &'static std::sync::Mutex<SolutionCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<SolutionCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(SolutionCache { entries: Vec::new() }))
}

fn cache_put(value: serde_json::Value) -> (String, std::sync::Arc<serde_json::Value>) {
    let id = auth::new_session_id()[..32].to_string();
    let mut value = value;
    value["id"] = serde_json::Value::String(id.clone());
    let arc = std::sync::Arc::new(value);
    let mut c = solution_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    c.entries.retain(|(_, at, _)| at.elapsed() < SOLUTION_TTL);
    if c.entries.len() >= SOLUTION_CACHE_MAX {
        c.entries.remove(0);
    }
    c.entries.push((id.clone(), std::time::Instant::now(), arc.clone()));
    (id, arc)
}

fn cache_get(id: &str) -> Option<std::sync::Arc<serde_json::Value>> {
    let c = solution_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    c.entries
        .iter()
        .find(|(k, at, _)| k == id && at.elapsed() < SOLUTION_TTL)
        .map(|(_, _, v)| v.clone())
}


/// Hard ceiling on a decoded upload, independent of the body limit.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Titles are display labels; anything longer is truncated before use/storage.
const MAX_TITLE_CHARS: usize = 200;
/// Auxiliary constructions accepted by `/api/humanize`.
const MAX_AUX_ITEMS: usize = 64;
const HISTORY_PAGE_DEFAULT: i64 = 100;
const HISTORY_PAGE_MAX: i64 = 200;
/// How long login/register wait for an argon2 slot before answering 503.
const ARGON2_QUEUE_WAIT: Duration = Duration::from_secs(10);

/// Run the web app until the process is stopped, using the given configuration.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind = config.bind;
    let summary = config.summary();
    let state = AppState::new(config).map_err(|e| anyhow::anyhow!(e))?;
    let translate_note = if !state.config.enable_translate {
        "disabled by configuration".to_string()
    } else {
        match state.translate_status().await {
            s if s.logged_in => "on (local Claude subscription)".into(),
            s if s.installed => "installed — run `claude auth login` to enable".into(),
            _ => "off — install the `claude` CLI to enable".into(),
        }
    };
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
        .route("/assets/{file}", get(asset))
        .route("/assets/fonts/{file}", get(font_asset))
        .route("/healthz", get(healthz))
        .route("/api/status", get(api_status))
        .route("/api/solve", post(api_solve))
        .route("/api/translate", post(api_translate))
        .route("/api/humanize", post(api_humanize))
        .route("/api/export", post(api_export))
        .route("/api/history", get(api_history))
        .route("/api/history/{id}", axum::routing::delete(api_history_delete).get(api_history_get))
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
            security::host_guard,
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

async fn session_user(state: &Shared, headers: &HeaderMap) -> Option<db::User> {
    auth::current_user_async(&state.db, headers, state.config.secure_cookies).await
}

/// `/`: signed-in visitors (and guests, in guest mode) go straight to the app;
/// everyone else sees the public landing page pitching the product.
async fn index(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if state.config.guest_allowed() {
        Html(INDEX_HTML).into_response()
    } else if session_user(&state, &headers).await.is_some() {
        Redirect::to("/app").into_response()
    } else {
        Html(LANDING_HTML).into_response()
    }
}

/// `/app`: the solver SPA, gated on a valid session (else 302 to `/auth`)
/// unless guest mode lets Basic auth alone in.
async fn app_page(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if state.config.guest_allowed() || session_user(&state, &headers).await.is_some() {
        Html(INDEX_HTML).into_response()
    } else {
        Redirect::to("/auth").into_response()
    }
}

fn serve_asset(name: &str) -> Response {
    match ASSETS.iter().find(|(n, ..)| *n == name) {
        Some((_, ctype, cache, bytes)) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(ctype)),
                (header::CACHE_CONTROL, HeaderValue::from_static(cache)),
            ],
            *bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn asset(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    serve_asset(&file)
}

async fn font_asset(axum::extract::Path(file): axum::extract::Path<String>) -> Response {
    serve_asset(&format!("fonts/{file}"))
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
    /// The first CLI probe has not finished; ask again shortly.
    translate_checking: bool,
    /// Whether *this caller* can use `/api/translate` right now.
    can_translate: bool,
    /// Why not: `disabled`, `not_installed`, `not_logged_in`, `sign_in`, `checking`.
    #[serde(skip_serializing_if = "Option::is_none")]
    translate_block: Option<&'static str>,
    signed_in: bool,
    guest: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
    solve_deadline_secs: u64,
    version: &'static str,
}

/// Never waits on the `claude` probe: a stale or missing result is refreshed
/// in the background and reported as `translate_checking`.
async fn api_status(State(state): State<Shared>, headers: HeaderMap) -> Json<Status> {
    let user = session_user(&state, &headers).await;
    let probe = state.translate_status_now();
    let checking = probe.is_none();
    let s = probe.unwrap_or(translate::Status { installed: false, logged_in: false });
    let block = if !state.config.enable_translate {
        Some("disabled")
    } else if checking {
        Some("checking")
    } else if !s.installed {
        Some("not_installed")
    } else if !s.logged_in {
        Some("not_logged_in")
    } else if user.is_none() {
        Some("sign_in")
    } else {
        None
    };
    Json(Status {
        translate_installed: s.installed,
        translate_logged_in: s.logged_in,
        translate_checking: checking,
        can_translate: block.is_none(),
        translate_block: block,
        signed_in: user.is_some(),
        guest: user.is_none() && state.config.guest_allowed(),
        username: user.map(|u| u.username),
        solve_deadline_secs: SOLVE_DEADLINE.as_secs(),
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ------------------------------------------------------------ extractors ----

/// `Json<T>` whose rejections use the app's `{ "error": … }` shape and do not
/// echo serde's detail (struct and field names) back to the client.
struct ApiJson<T>(T);

impl<T, S> FromRequest<S> for ApiJson<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(req: Request, state: &S) -> Result<Self, Response> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(ApiJson(v)),
            Err(rejection) => {
                eprintln!("rejected request body: {rejection}");
                Err(err(rejection.status(), "invalid request body"))
            }
        }
    }
}

/// A signed-in user. Listed before the body extractor in a handler, so an
/// unauthenticated request is refused before its body is read or parsed.
struct SessionUser(db::User);

impl FromRequestParts<Shared> for SessionUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Response> {
        match session_user(state, &parts.headers).await {
            Some(u) => Ok(SessionUser(u)),
            None => Err(err(
                StatusCode::UNAUTHORIZED,
                i18n::t(i18n::lang_from_headers(&parts.headers), "auth.sign_in_required"),
            )),
        }
    }
}

/// Who may use the solver: a signed-in user, or — in guest mode, where the
/// Basic-auth layer has already admitted the request — an anonymous guest.
enum Caller {
    User(db::User),
    Guest,
}

impl FromRequestParts<Shared> for Caller {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Response> {
        match SessionUser::from_request_parts(parts, state).await {
            Ok(SessionUser(u)) => Ok(Caller::User(u)),
            Err(_) if state.config.guest_allowed() => Ok(Caller::Guest),
            Err(e) => Err(e),
        }
    }
}

// ------------------------------------------------------------------ auth ----
//
// Public routes (no session gate) that manage accounts: register/login mint a
// session cookie, logout clears it, `me` reports the current user. All password
// hashing + SQLite work runs on the blocking pool, argon2 behind its own small
// semaphore. These sit behind the same rate-limit / Basic-auth / security-header
// layers as everything else.

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

/// Usernames are case-insensitive: stored and compared lowercased.
fn canonical_username(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

/// Validate a username (returning its canonical form) and password length.
#[allow(clippy::result_large_err)]
fn valid_credentials(username: &str, password: &str, lang: i18n::Lang) -> Result<String, Response> {
    let u = canonical_username(username);
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
    Ok(u)
}

/// A `{ "username": … }` body plus a session `Set-Cookie`.
fn auth_ok(username: &str, sid: &str, secure: bool) -> Response {
    (
        [(header::SET_COOKIE, auth::set_cookie_header(sid, secure))],
        Json(serde_json::json!({ "username": username })),
    )
        .into_response()
}

/// Wait (bounded) for an argon2 slot, or answer 503.
async fn argon_permit(
    state: &Shared,
    lang: i18n::Lang,
) -> Result<tokio::sync::OwnedSemaphorePermit, Response> {
    match tokio::time::timeout(ARGON2_QUEUE_WAIT, state.argon.clone().acquire_owned()).await {
        Ok(Ok(p)) => Ok(p),
        _ => Err(err(StatusCode::SERVICE_UNAVAILABLE, i18n::t(lang, "server.busy"))),
    }
}

async fn api_register(
    State(state): State<Shared>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<Credentials>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let username = match valid_credentials(&req.username, &req.password, lang) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let permit = match argon_permit(&state, lang).await {
        Ok(p) => p,
        Err(e) => return e,
    };
    let secure = state.config.secure_cookies;
    let db = state.db.clone();
    let outcome = tokio::task::spawn_blocking(move || -> Result<(String, String), RegisterErr> {
        let hash = auth::hash_password(&req.password).map_err(|_| RegisterErr::Internal)?;
        drop(permit);
        let conn = db::lock(&db);
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
    ApiJson(req): ApiJson<Credentials>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let secure = state.config.secure_cookies;
    let permit = match argon_permit(&state, lang).await {
        Ok(p) => p,
        Err(e) => return e,
    };
    let db = state.db.clone();
    // `None` is returned for every failure mode (unknown user, bad password) so
    // the client can't distinguish them — not by status, and not by timing: an
    // unknown user is verified against a dummy hash. argon2 runs off the DB lock.
    let outcome = tokio::task::spawn_blocking(move || -> Option<(String, String)> {
        let found = {
            let conn = db::lock(&db);
            db::find_user_by_name(&conn, &canonical_username(&req.username)).ok()?
        };
        let ok = auth::verify_password_or_dummy(
            &req.password,
            found.as_ref().map(|u| u.password_hash.as_str()),
        );
        drop(permit);
        let user = found.filter(|_| ok)?;
        let sid = auth::new_session_id();
        db::insert_session(&db::lock(&db), &sid, user.id, auth::SESSION_TTL_SECS).ok()?;
        Some((user.username, sid))
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
    let secure = state.config.secure_cookies;
    if let Some(sid) = auth::parse_sid(&headers, secure) {
        let db = state.db.clone();
        let _ = tokio::task::spawn_blocking(move || db::delete_session(&db::lock(&db), &sid)).await;
    }
    (
        [(header::SET_COOKIE, auth::clear_cookie_header(secure))],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

async fn api_me(State(state): State<Shared>, headers: HeaderMap) -> Response {
    match session_user(&state, &headers).await {
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

/// A compile/parse error: a localized sentence, where it is, and the engine's
/// own words as `detail`.
fn compile_err(lang: i18n::Lang, input: &str, raw: &str) -> Response {
    let d = present::diagnose(input, raw);
    let msg = i18n::compile_message(lang, &d);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg, "code": "compile", "diagnosis": d, "detail": raw })),
    )
        .into_response()
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

/// Trim a client-supplied title and cut it to [`MAX_TITLE_CHARS`]; blank → none.
fn clean_title(title: Option<String>) -> Option<String> {
    let t = title?;
    let t: String = t.trim().chars().take(MAX_TITLE_CHARS).collect();
    (!t.is_empty()).then_some(t)
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

fn yes() -> bool {
    true
}

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
    /// `false` when the client is replaying an entry already in history.
    #[serde(default = "yes")]
    record: bool,
}

async fn api_solve(
    State(state): State<Shared>,
    caller: Caller,
    headers: HeaderMap,
    ApiJson(req): ApiJson<SolveReq>,
) -> Response {
    if let Err(e) = check_input(&state, &headers, &req.input) {
        return e;
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let title = clean_title(req.title);
    let history_title = title.clone();
    let opts = SolveOptions {
        kind: kind_of(&req.input, &req.kind),
        theme: engine::parse_theme(&req.theme),
        want_proof: true,
        title,
        // The UI lays the legend out in HTML below the figure, so the drawing
        // gets the whole card (matters on phones).
        panel: false,
    };
    let input = req.input;
    let input_for_errors = input.clone();
    let lang = i18n::lang_from_headers(&headers);
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
            engine::solve_within(&input, &opts, Some(SOLVE_DEADLINE))
        }
    });
    match task.await {
        Ok(Ok(sol)) => {
            let value = tokio::task::spawn_blocking({
                let sol = sol.clone();
                let t = history_title.clone();
                move || present::solution_json(&sol, t.as_deref())
            })
            .await
            .unwrap_or_default();
            let (_, value) = cache_put(value);
            if let (Caller::User(user), true) = (&caller, req.record) {
                save_history(&state, user.id, &sol, history_title.as_deref(), &value).await;
            }
            Json(value.as_ref().clone()).into_response()
        }
        Ok(Err(e)) => {
            eprintln!("solve error: {e}");
            compile_err(lang, &input_for_errors, &e)
        }
        Err(e) => {
            eprintln!("solve panicked: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(i18n::lang_from_headers(&headers), "solve.failed"))
        }
    }
}

/// The verdict as stored in history — the same strings the solve JSON uses.
fn status_str(s: engine::Status) -> &'static str {
    match s {
        engine::Status::Proved => "proved",
        engine::Status::HoldsNumerically => "holds-numerically",
        engine::Status::Refuted => "refuted",
        engine::Status::NotProved => "not-proved",
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
/// a storage error here must never fail the solve response itself. Inputs past
/// the request cap are never stored (the request check bounds `sol.input`'s
/// source, this bounds what the engine hands back).
async fn save_history(
    state: &Shared,
    user_id: i64,
    sol: &engine::Solution,
    title: Option<&str>,
    value: &serde_json::Value,
) {
    if sol.input.len() > state.config.max_input_chars {
        return;
    }
    let db = state.db.clone();
    let (input, proved, method) = (sol.input.clone(), sol.proved, method_str(sol.method));
    let status = status_str(sol.status);
    let title = title.map(str::to_string).or_else(|| value["title"].as_str().map(str::to_string));
    let goal = value["view"].get("goal").filter(|g| !g.is_null()).map(|g| g.to_string());
    let mut stored = value.clone();
    if let Some(o) = stored.as_object_mut() {
        o.remove("id");
    }
    let solution = Some(stored.to_string()).filter(|s| s.len() <= MAX_STORED_SOLUTION_BYTES);
    let _ = tokio::task::spawn_blocking(move || {
        db::insert_history_full(
            &db::lock(&db),
            user_id,
            &db::NewHistory {
                input: &input,
                title: title.as_deref(),
                proved,
                method: Some(method),
                status: Some(status),
                goal: goal.as_deref(),
                solution: solution.as_deref(),
            },
        )
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
    status: Option<String>,
    /// The goal as a typed fact (see `present::Fact`), when recorded.
    goal: Option<serde_json::Value>,
    has_solution: bool,
    created_at: i64,
}

impl From<db::HistoryEntry> for HistoryItem {
    fn from(h: db::HistoryEntry) -> Self {
        HistoryItem {
            id: h.id,
            input: h.input,
            title: h.title,
            proved: h.proved,
            method: h.method,
            status: h.status,
            goal: h.goal.and_then(|g| serde_json::from_str(&g).ok()),
            has_solution: h.has_solution,
            created_at: h.created_at,
        }
    }
}

/// One stored solution, exactly as it was shown, ready to export again.
async fn api_history_get(
    State(state): State<Shared>,
    SessionUser(user): SessionUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
    headers: HeaderMap,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || db::history_solution(&db::lock(&db), user.id, id)).await;
    match res {
        Ok(Ok(Some(Some(json)))) => match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(v) => {
                let (_, v) = cache_put(v);
                Json(v.as_ref().clone()).into_response()
            }
            Err(_) => err(StatusCode::GONE, i18n::t(lang, "history.no_solution")),
        },
        Ok(Ok(Some(None))) => (
            StatusCode::GONE,
            Json(serde_json::json!({ "error": i18n::t(lang, "history.no_solution"), "code": "no_solution" })),
        )
            .into_response(),
        Ok(Ok(None)) => err(StatusCode::NOT_FOUND, i18n::t(lang, "history.none")),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "history.load_fail")),
    }
}

#[derive(Deserialize)]
struct HistoryQuery {
    limit: Option<i64>,
    /// Return only entries older than this id (the last id of the previous page).
    before: Option<i64>,
}

/// A page of the signed-in user's past solves, newest first.
async fn api_history(
    State(state): State<Shared>,
    SessionUser(user): SessionUser,
    headers: HeaderMap,
    query: Result<Query<HistoryQuery>, axum::extract::rejection::QueryRejection>,
) -> Response {
    let Ok(Query(q)) = query else {
        return err(StatusCode::BAD_REQUEST, "invalid query");
    };
    let limit = q.limit.unwrap_or(HISTORY_PAGE_DEFAULT).clamp(1, HISTORY_PAGE_MAX);
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        db::list_history(&db::lock(&db), user.id, limit, q.before)
    })
    .await;
    match res {
        Ok(Ok(rows)) => Json(rows.into_iter().map(HistoryItem::from).collect::<Vec<_>>()).into_response(),
        Ok(Err(e)) => {
            eprintln!("history load failed: {e}");
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
    SessionUser(user): SessionUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
    headers: HeaderMap,
) -> Response {
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || db::delete_history(&db::lock(&db), user.id, id)).await;
    match res {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => err(StatusCode::NOT_FOUND, i18n::t(i18n::lang_from_headers(&headers), "history.none")),
        Ok(Err(e)) => {
            eprintln!("history delete failed: {e}");
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
    _user: SessionUser,
    headers: HeaderMap,
    ApiJson(req): ApiJson<TranslateReq>,
) -> Response {
    if !state.config.enable_translate || !translate::available() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            i18n::t(i18n::lang_from_headers(&headers), "translate.disabled"),
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
                i18n::t(i18n::lang_from_headers(&headers), "translate.no_problem"),
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
/// availability/rate-limit posture as `/api/translate` (guests allowed in guest
/// mode); any failure degrades to a 503 so the client can fall back to the
/// machine proof.
async fn api_humanize(
    State(state): State<Shared>,
    _caller: Caller,
    headers: HeaderMap,
    ApiJson(req): ApiJson<HumanizeReq>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    if req.proof.trim().is_empty() {
        return err(StatusCode::BAD_REQUEST, i18n::t(lang, "humanize.need_proof"));
    }
    let max = state.config.max_input_chars;
    let aux_bytes: usize = req.aux.iter().map(String::len).sum();
    let over_limit = req.problem.len() > max
        || req.proof.len() > 4 * max
        || req.aux.len() > MAX_AUX_ITEMS
        || aux_bytes > max;
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
    /// A solution id from `/api/solve` or `/api/history/{id}`: export exactly
    /// that result. Without one, `input` is solved afresh.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    input: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    format: String,
}

fn export_response(bytes: Vec<u8>, want_pdf: bool) -> Response {
    let (ctype, disposition) = if want_pdf {
        ("application/pdf", "attachment; filename=\"geosolver-proof.pdf\"")
    } else {
        ("image/png", "attachment; filename=\"geosolver-proof.png\"")
    };
    Response::builder()
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn api_export(
    State(state): State<Shared>,
    _caller: Caller,
    headers: HeaderMap,
    ApiJson(req): ApiJson<ExportReq>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let want_pdf = req.format.eq_ignore_ascii_case("pdf");
    if let Some(id) = req.id.as_deref().filter(|s| !s.is_empty()) {
        let Some(value) = cache_get(id) else {
            return (
                StatusCode::GONE,
                Json(serde_json::json!({ "error": i18n::t(lang, "export.expired"), "code": "expired" })),
            )
                .into_response();
        };
        let permit = match heavy_permit(&state, &headers) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
            let _permit = permit;
            let report = render::report_from_json(&value, lang);
            if want_pdf {
                render::svg_to_pdf(&report)
            } else {
                render::svg_to_png(&report, 2.0)
            }
        })
        .await;
        return match res {
            Ok(Ok(bytes)) => export_response(bytes, want_pdf),
            Ok(Err(e)) => {
                eprintln!("export error: {e}");
                err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "export.failed"))
            }
            Err(e) => {
                eprintln!("export panicked: {e}");
                err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "export.failed"))
            }
        };
    }
    if let Err(e) = check_input(&state, &headers, &req.input) {
        return e;
    }
    let permit = match heavy_permit(&state, &headers) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let title = clean_title(req.title);
    // Documents export on a light, print-friendly page regardless of on-screen theme.
    let opts = SolveOptions {
        kind: kind_of(&req.input, &req.kind),
        theme: Theme::Light,
        want_proof: true,
        title: title.clone(),
        panel: true,
    };
    let input = req.input;
    let input_for_errors = input.clone();

    let res = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, (bool, String)> {
        let _permit = permit;
        let sol = engine::solve_within(&input, &opts, Some(SOLVE_DEADLINE)).map_err(|e| (true, e))?;
        let value = present::solution_json(&sol, title.as_deref());
        let report = render::report_from_json(&value, lang);
        if want_pdf {
            render::svg_to_pdf(&report)
        } else {
            render::svg_to_png(&report, 2.0)
        }
        .map_err(|e| (false, e.to_string()))
    })
    .await;

    match res {
        Ok(Ok(bytes)) => export_response(bytes, want_pdf),
        Ok(Err((true, e))) => compile_err(lang, &input_for_errors, &e),
        Ok(Err((false, e))) => {
            eprintln!("export error: {e}");
            err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "export.failed"))
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
    async fn numeric_only_metric_goal_is_not_proved_over_http() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "noether").await;
        let input = "A B C = triangle\nM = midpoint(B, C)\nprove area(A,B,M) = area(A,M,C)";
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": input}),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body:?}");
        assert_eq!(body["proved"], false, "{body:?}");
        assert_eq!(body["status"], "holds-numerically", "{body:?}");
        assert_eq!(body["goal_holds_numerically"], true, "{body:?}");
        assert!(body["numeric_samples"].as_u64().is_some_and(|n| n >= 8), "{body:?}");
        assert!(body["proof"].is_null(), "a numeric check is not a proof: {body:?}");
        assert!(
            body["numeric_evidence"].as_str().is_some_and(|e| e.contains("not a proof")),
            "{body:?}"
        );

        let (_, _, rows) =
            call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(rows[0]["proved"], false);
        assert_eq!(rows[0]["status"], "holds-numerically");
    }

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
        assert_eq!(rows[0]["status"], "proved");
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
        config.basic_auth = Some(security::BasicAuth::parse("op:secret-pass").unwrap());
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
        let basic_hdr = basic("op", "secret-pass");
        let basic = basic_hdr.as_str();
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
        let (state, _dir) = state_with(|c| c.public_hosts = vec!["geo.example".into()]);
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

    /// `/api/translate` and `/api/humanize` drive the same costly `claude` CLI
    /// subprocess, so `security::rate_limit` must count hits to either one
    /// against a single shared per-IP bucket (not two independent ones).
    #[tokio::test]
    async fn humanize_and_translate_share_rate_limit_bucket() {
        let (state, _dir) = state_with(|c| {
            c.translate_per_min = 2;
            // Keep hits cheap (a 503, no CLI spawn): rate-limiting is a middleware
            // layer that runs before the handler, so it counts regardless.
            c.enable_translate = false;
        });
        let cookie = register_cookie(&state, "priya").await;
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
        // /api/humanize shares that same, now exhausted, bucket.
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": "p", "proof": "001. coll A B C"}),
        )
        .await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(body["error"], "rate limit exceeded — please slow down");
    }

    // ------------------------------------------------ public-exposure hardening --

    fn state_with(f: impl FnOnce(&mut Config)) -> (Shared, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        f(&mut config);
        (AppState::new(config).unwrap(), dir)
    }

    fn basic(user: &str, pass: &str) -> String {
        use base64::Engine;
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        )
    }

    fn build(
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> Request<Body> {
        let mut b = Request::builder().method(method).uri(uri);
        if body.is_some() {
            b = b.header(header::CONTENT_TYPE, "application/json");
        }
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(body.map_or_else(Body::empty, |s| Body::from(s.to_string())))
            .unwrap()
    }

    fn from_peer(mut req: Request<Body>, peer: &str) -> Request<Body> {
        let addr: SocketAddr = peer.parse().unwrap();
        req.extensions_mut().insert(axum::extract::ConnectInfo(addr));
        req
    }

    async fn send(state: &Shared, req: Request<Body>) -> (StatusCode, HeaderMap, String) {
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, headers, String::from_utf8_lossy(&bytes).into_owned())
    }

    static DISABLED_PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn counting_probe() -> translate::Status {
        DISABLED_PROBES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        translate::Status { installed: true, logged_in: true }
    }

    static SLOW_PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    fn slow_probe() -> translate::Status {
        SLOW_PROBES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(400));
        translate::Status { installed: true, logged_in: true }
    }

    #[tokio::test]
    async fn status_never_probes_the_cli_when_translate_is_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.enable_translate = false;
        let state = AppState::with_translate_probe(config, counting_probe).unwrap();
        let (st, _, body) = call(&state, "GET", "/api/status", None, serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["translate_installed"], false);
        assert_eq!(DISABLED_PROBES.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    /// The default `#[tokio::test]` runtime is single-threaded, so a probe run
    /// inline on the async worker would stall `/healthz` for its whole duration.
    #[tokio::test]
    async fn status_probe_runs_off_the_async_worker_and_is_cached() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("test.db");
        config.enable_translate = true;
        let state = AppState::with_translate_probe(config, slow_probe).unwrap();
        let s2 = state.clone();
        let t = std::time::Instant::now();
        let status = tokio::spawn(async move {
            call(&s2, "GET", "/api/status", None, serde_json::Value::Null).await
        });
        tokio::task::yield_now().await;
        let (st, _, _) = send(&state, build("GET", "/healthz", &[], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(t.elapsed() < Duration::from_millis(200), "healthz stalled {:?}", t.elapsed());
        let (st, _, body) = status.await.unwrap();
        assert_eq!(st, StatusCode::OK);
        assert!(t.elapsed() < Duration::from_millis(200), "/api/status waited on the probe");
        assert_eq!(body["translate_checking"], true, "{body}");
        assert_eq!(body["can_translate"], false);
        let mut body = body;
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            body = call(&state, "GET", "/api/status", None, serde_json::Value::Null).await.2;
            if body["translate_checking"] == false {
                break;
            }
        }
        assert_eq!(body["translate_logged_in"], true, "{body}");
        assert_eq!(body["translate_block"], "sign_in", "a visitor without a session cannot translate");
        call(&state, "GET", "/api/status", None, serde_json::Value::Null).await;
        assert_eq!(SLOW_PROBES.load(std::sync::atomic::Ordering::SeqCst), 1, "later calls must hit the cache");
    }

    #[tokio::test]
    async fn long_titles_are_capped_before_reaching_history() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "tess").await;
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": ISOSCELES_GEO, "title": "t".repeat(5000)}),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, _, body) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(body[0]["title"].as_str().unwrap().chars().count(), MAX_TITLE_CHARS);
    }

    #[tokio::test]
    async fn history_listing_is_paginated_and_capped() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "pam").await;
        {
            let conn = db::lock(&state.db);
            let uid = db::find_user_by_name(&conn, "pam").unwrap().unwrap().id;
            for i in 0..(HISTORY_PAGE_MAX + 20) {
                db::insert_history(&conn, uid, &format!("p{i}"), None, true, None, None).unwrap();
            }
        }
        let (st, _, body) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), HISTORY_PAGE_DEFAULT as usize);
        let (_, _, body) =
            call(&state, "GET", "/api/history?limit=100000", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(body.as_array().unwrap().len(), HISTORY_PAGE_MAX as usize);
        let first = body[0]["id"].as_i64().unwrap();
        let (_, _, body) = call(
            &state,
            "GET",
            &format!("/api/history?limit=2&before={first}"),
            Some(&cookie),
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(body.as_array().unwrap().len(), 2);
        assert!(body[0]["id"].as_i64().unwrap() < first);
        let (st, _, body) =
            call(&state, "GET", "/api/history?limit=abc", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(body["error"].is_string());
    }

    #[tokio::test]
    async fn humanize_caps_aux_count_and_size() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "abe").await;
        let many: Vec<String> = (0..(MAX_AUX_ITEMS + 1)).map(|i| format!("x{i}")).collect();
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": "p", "proof": "001. coll A B C", "aux": many}),
        )
        .await;
        assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
        let big = vec!["y".repeat(state.config.max_input_chars); 2];
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/humanize",
            Some(&cookie),
            serde_json::json!({"problem": "p", "proof": "001. coll A B C", "aux": big}),
        )
        .await;
        assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn login_waits_for_an_argon2_permit() {
        let (state, _dir) = test_state();
        let held = state
            .argon
            .clone()
            .acquire_many_owned(security::ARGON2_PERMITS as u32)
            .await
            .unwrap();
        let pending = tokio::time::timeout(
            Duration::from_millis(300),
            call(&state, "POST", "/api/auth/login", None, creds("nobody", "password123")),
        )
        .await;
        assert!(pending.is_err(), "login must not run argon2 without a permit");
        drop(held);
        let (st, _, _) = call(&state, "POST", "/api/auth/login", None, creds("nobody", "password123")).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn failed_basic_auth_is_rate_limited_per_ip() {
        let (state, _dir) = state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse(":correct-horse").unwrap());
            c.basic_auth_fails_per_min = 3;
        });
        let attacker = "203.0.113.7:5555";
        let wrong = basic("x", "wrong-guess");
        for _ in 0..3 {
            let r = from_peer(build("GET", "/auth", &[("authorization", &wrong)], None), attacker);
            assert_eq!(send(&state, r).await.0, StatusCode::UNAUTHORIZED);
        }
        let r = from_peer(build("GET", "/auth", &[("authorization", &wrong)], None), attacker);
        assert_eq!(send(&state, r).await.0, StatusCode::TOO_MANY_REQUESTS);
        // Locked out: even the right password is not evaluated from that IP.
        let right = basic("x", "correct-horse");
        let r = from_peer(build("GET", "/auth", &[("authorization", &right)], None), attacker);
        assert_eq!(send(&state, r).await.0, StatusCode::TOO_MANY_REQUESTS);
        // Another client is unaffected.
        let r = from_peer(build("GET", "/auth", &[("authorization", &right)], None), "198.51.100.9:1");
        assert_eq!(send(&state, r).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn password_only_basic_auth_accepts_any_username() {
        let (state, _dir) = state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse(":correct-horse").unwrap());
        });
        for user in ["", "anyone", "Ünïcode user"] {
            let r = build("GET", "/auth", &[("authorization", &basic(user, "correct-horse"))], None);
            assert_eq!(send(&state, r).await.0, StatusCode::OK, "user {user:?}");
        }
        let r = build("GET", "/auth", &[("authorization", &basic("anyone", "nope-nope"))], None);
        let (st, headers, _) = send(&state, r).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(headers[header::WWW_AUTHENTICATE].to_str().unwrap().contains("realm=\"GeoSolver\""));
    }

    #[tokio::test]
    async fn user_pass_basic_auth_still_checks_the_username() {
        let (state, _dir) = state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse("op:correct-horse").unwrap());
        });
        let r = build("GET", "/auth", &[("authorization", &basic("op", "correct-horse"))], None);
        assert_eq!(send(&state, r).await.0, StatusCode::OK);
        let r = build("GET", "/auth", &[("authorization", &basic("other", "correct-horse"))], None);
        assert_eq!(send(&state, r).await.0, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn secure_cookie_mode_uses_the_host_prefix_end_to_end() {
        let (state, _dir) = state_with(|c| c.secure_cookies = true);
        let (st, set, _) =
            call(&state, "POST", "/api/auth/register", None, creds("sec", "password123")).await;
        assert_eq!(st, StatusCode::OK);
        let set = set.unwrap();
        assert!(set.starts_with("__Host-sid=") && set.contains("; Secure"), "{set}");
        let cookie = cookie_of(&set);
        let (st, _, _) = call(&state, "GET", "/api/auth/me", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        let plain = cookie.replacen("__Host-sid=", "sid=", 1);
        let (st, _, _) = call(&state, "GET", "/api/auth/me", Some(&plain), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let (_, set, _) = call(&state, "POST", "/api/auth/logout", Some(&cookie), serde_json::Value::Null).await;
        assert!(set.unwrap().starts_with("__Host-sid=;"));
    }

    #[tokio::test]
    async fn loopback_bind_refuses_foreign_host_headers() {
        let (state, _dir) = test_state();
        assert!(state.config.bind.ip().is_loopback());
        for host in ["localhost:8787", "127.0.0.1:8787", "[::1]:8787", "LOCALHOST"] {
            let (st, _, _) = send(&state, build("GET", "/auth", &[("host", host)], None)).await;
            assert_eq!(st, StatusCode::OK, "{host}");
        }
        let (st, _, _) = send(&state, build("GET", "/auth", &[("host", "rebind.attacker.example")], None)).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn public_host_list_admits_the_proxied_hostname() {
        let (state, _dir) = state_with(|c| c.public_hosts = vec!["geo.tail1234.ts.net".into()]);
        for host in ["geo.tail1234.ts.net", "GEO.tail1234.ts.net:443", "localhost:8787"] {
            let (st, _, _) = send(&state, build("GET", "/auth", &[("host", host)], None)).await;
            assert_eq!(st, StatusCode::OK, "{host}");
        }
        let (st, _, _) = send(&state, build("GET", "/auth", &[("host", "evil.example")], None)).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn unauthenticated_requests_are_refused_before_the_body_is_parsed() {
        let (state, _dir) = test_state();
        for uri in ["/api/solve", "/api/translate", "/api/export", "/api/humanize"] {
            let (st, _, body) = send(&state, build("POST", uri, &[], Some("{not json"))).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "{uri}: {body}");
        }
    }

    #[tokio::test]
    async fn bad_json_gets_the_app_error_shape() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "joe").await;
        for body in ["{not json", r#"{"kind":"geo"}"#] {
            let (st, headers, text) =
                send(&state, build("POST", "/api/solve", &[("cookie", &cookie)], Some(body))).await;
            assert!(st.is_client_error(), "{st}");
            assert!(headers[header::CONTENT_TYPE].to_str().unwrap().starts_with("application/json"));
            let v: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert!(v["error"].is_string(), "{text}");
            assert!(!text.contains("SolveReq") && !text.contains("missing field"), "{text}");
        }
        let (st, _, text) = send(&state, build("POST", "/api/auth/login", &[], Some("[]"))).await;
        assert!(st.is_client_error());
        assert!(serde_json::from_str::<serde_json::Value>(&text).unwrap()["error"].is_string());
    }

    #[tokio::test]
    async fn poisoned_db_mutex_does_not_log_everyone_out() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "pia").await;
        let db = state.db.clone();
        let _ = std::thread::spawn(move || {
            let _g = db.lock().unwrap();
            panic!("poison");
        })
        .join();
        let (st, _, _) = call(&state, "GET", "/api/auth/me", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _, _) = call(&state, "GET", "/api/status", None, serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
    }

    #[tokio::test]
    async fn replayed_solve_with_record_false_adds_no_history() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "rex").await;
        call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ISOSCELES_GEO, "title": "kept"})).await;
        let (st, _, _) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": ISOSCELES_GEO, "title": "kept", "record": false}),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let (_, _, body) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["title"], "kept");
    }

    #[tokio::test]
    async fn usernames_are_case_insensitive() {
        let (state, _dir) = test_state();
        let (st, _, body) = call(&state, "POST", "/api/auth/register", None, creds("Alice", "password123")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["username"], "alice");
        let (st, _, _) = call(&state, "POST", "/api/auth/register", None, creds("ALICE", "password123")).await;
        assert_eq!(st, StatusCode::CONFLICT);
        let (st, _, body) = call(&state, "POST", "/api/auth/login", None, creds("aLiCe", "password123")).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["username"], "alice");
    }

    #[tokio::test]
    async fn origin_null_is_refused_and_ports_are_ignored() {
        let (state, _dir) = state_with(|c| c.public_hosts = vec!["geo.example".into()]);
        let body = creds("olga", "password123").to_string();
        let post = |host: &str, origin: &str| {
            build("POST", "/api/auth/register", &[("host", host), ("origin", origin)], Some(&body))
        };
        let (st, _, _) = send(&state, post("geo.example", "null")).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        // nginx `$host` drops the port the browser put in `Origin`.
        let (st, _, text) = send(&state, post("geo.example", "https://geo.example:8443")).await;
        assert_eq!(st, StatusCode::OK, "{text}");
        let (st, _, _) = send(&state, post("geo.example:8443", "https://evil.example:8443")).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn csp_allows_only_the_known_inline_scripts() {
        let (state, _dir) = test_state();
        let (_, headers, _) = get_page(&state, "/auth", None).await;
        let csp = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().to_string();
        let script_src = csp.split(';').find(|d| d.trim().starts_with("script-src")).unwrap();
        assert!(!script_src.contains("unsafe-inline"), "{script_src}");
        for html in [INDEX_HTML, AUTH_HTML, LANDING_HTML] {
            for script in security::inline_scripts(html) {
                assert!(script_src.contains(&security::csp_hash(script)), "missing hash in {script_src}");
            }
        }
    }

    fn guest_state(guest: bool) -> (Shared, tempfile::TempDir) {
        state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse(":shared-secret").unwrap());
            c.guest_mode = guest;
        })
    }

    #[tokio::test]
    async fn guest_mode_lets_basic_auth_alone_solve_export_and_humanize() {
        let (state, _dir) = guest_state(true);
        let auth = basic("", "shared-secret");
        let solve = serde_json::json!({"input": ISOSCELES_GEO}).to_string();
        let (st, _, text) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&solve))).await;
        assert_eq!(st, StatusCode::OK, "{text}");
        let export = serde_json::json!({"input": ISOSCELES_GEO, "format": "png"}).to_string();
        let (st, _, _) = send(&state, build("POST", "/api/export", &[("authorization", &auth)], Some(&export))).await;
        assert_eq!(st, StatusCode::OK);
        let hz = serde_json::json!({"problem": "p", "proof": "   "}).to_string();
        let (st, _, _) = send(&state, build("POST", "/api/humanize", &[("authorization", &auth)], Some(&hz))).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "past the session gate, into validation");
        // `/` is the app itself, not the account landing page.
        let (st, _, page) = send(&state, build("GET", "/", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(page.contains("id=\"solve\""), "guest `/` should serve the solver UI");
        let (st, _, _) = send(&state, build("GET", "/app", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        // Without the Basic password, nothing.
        let (st, _, _) = send(&state, build("POST", "/api/solve", &[], Some(&solve))).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn guest_mode_off_still_requires_a_session() {
        let (state, _dir) = guest_state(false);
        let auth = basic("", "shared-secret");
        let solve = serde_json::json!({"input": ISOSCELES_GEO}).to_string();
        let (st, _, _) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&solve))).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn guest_solves_write_no_history() {
        let (state, _dir) = guest_state(true);
        let auth = basic("", "shared-secret");
        let creds_body = creds("member", "password123").to_string();
        let (st, _, _) = send(
            &state,
            build("POST", "/api/auth/register", &[("authorization", &auth)], Some(&creds_body)),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "an account exists, so a stray insert could succeed");
        let solve = serde_json::json!({"input": ISOSCELES_GEO, "title": "g"}).to_string();
        let (st, _, _) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&solve))).await;
        assert_eq!(st, StatusCode::OK);
        let rows: i64 = db::lock(&state.db)
            .query_row("SELECT COUNT(*) FROM history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0);
        let (st, _, _) = send(&state, build("GET", "/api/history", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
    }

    const ORTHO_REFLECTION: &str =
        "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))";

    async fn call_raw(state: &Shared, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let resp = app_router(state.clone()).oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec();
        (status, headers, bytes)
    }

    #[tokio::test]
    async fn solve_answers_with_a_readable_view_and_an_export_id() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "euler").await;
        let (st, _, body) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ORTHO_REFLECTION})).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["status"], "proved");
        assert!(body["proof"].as_str().unwrap().contains("_5"), "the engine's own proof is passed through");
        for field in ["view", "svg", "title"] {
            let shown = body[field].to_string();
            assert!(!shown.contains("_5") && !shown.contains("_\u{2085}"), "internal name in {field}: {shown}");
        }
        assert!(body["view"]["proof"]["steps"].as_array().is_some_and(|s| !s.is_empty()));
        assert!(body["svg"].as_str().unwrap().contains("data-p=\""));
        let id = body["id"].as_str().expect("solution id").to_string();

        let export = serde_json::json!({"id": id, "format": "png"}).to_string();
        let req = build("POST", "/api/export", &[("cookie", &cookie)], Some(&export));
        let (st, headers, bytes) = call_raw(&state, req).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "image/png");
        assert!(bytes.starts_with(b"\x89PNG"));

        let gone = serde_json::json!({"id": "0123456789abcdef0123456789abcdef", "format": "pdf"}).to_string();
        let req = build("POST", "/api/export", &[("cookie", &cookie)], Some(&gone));
        let (st, _, bytes) = call_raw(&state, req).await;
        assert_eq!(st, StatusCode::GONE);
        let j: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(j["code"], "expired");
    }

    #[tokio::test]
    async fn history_reopens_the_stored_solution_without_solving() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "gauss").await;
        let (_, _, solved) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ORTHO_REFLECTION})).await;
        let (st, _, rows) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        let row = &rows[0];
        assert_eq!(row["has_solution"], true);
        assert_eq!(row["goal"]["kind"], "cyclic");
        let id = row["id"].as_i64().unwrap();
        let (st, _, again) =
            call(&state, "GET", &format!("/api/history/{id}"), Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(again["proof"], solved["proof"]);
        assert_eq!(again["view"], solved["view"]);
        assert_ne!(again["id"], solved["id"], "a fresh export id");
        let other = register_cookie(&state, "riemann").await;
        let (st, _, _) =
            call(&state, "GET", &format!("/api/history/{id}"), Some(&other), serde_json::Value::Null).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "another user's row is invisible");
    }

    #[tokio::test]
    async fn compile_errors_are_located_and_localized() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "hilbert").await;
        let bad = "A B C = triangle\nH = orthocenter(A B C)\nprove perp(A, H, B, C)";
        let (st, _, body) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": bad})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(body["code"], "compile");
        assert_eq!(body["diagnosis"]["line"], 2);
        assert_eq!(body["diagnosis"]["col"], 19);
        assert!(body["error"].as_str().unwrap().starts_with("Expected a comma"), "{body}");
        assert!(body["detail"].as_str().unwrap().contains("found"));
        let ro = format!("{cookie}; lang=ro");
        let (_, _, body) = call(&state, "POST", "/api/solve", Some(&ro), serde_json::json!({"input": bad})).await;
        assert!(body["error"].as_str().unwrap().starts_with("Lipsește"), "{body}");
    }

    #[tokio::test]
    async fn static_assets_are_served_with_their_types() {
        let (state, _dir) = test_state();
        for (path, ctype) in [
            ("/assets/app.css", "text/css; charset=utf-8"),
            ("/assets/app.js", "application/javascript; charset=utf-8"),
            ("/assets/i18n.js", "application/javascript; charset=utf-8"),
            ("/assets/fonts/stix-two-text.woff2", "font/woff2"),
        ] {
            let (st, headers, _) = call_raw(&state, build("GET", path, &[], None)).await;
            assert_eq!(st, StatusCode::OK, "{path}");
            assert_eq!(headers[header::CONTENT_TYPE], ctype, "{path}");
        }
        let (st, _, _) = call_raw(&state, build("GET", "/assets/../Cargo.toml", &[], None)).await;
        assert_ne!(st, StatusCode::OK);
        let (st, _, _) = call_raw(&state, build("GET", "/assets/nope.js", &[], None)).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn guests_are_told_why_they_cannot_translate() {
        let (state, _dir) = guest_state(true);
        let auth = basic("", "shared-secret");
        let (st, _, text) = send(&state, build("GET", "/api/status", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["guest"], true);
        assert_eq!(body["signed_in"], false);
        assert_eq!(body["can_translate"], false);
        assert!(body["translate_block"].is_string(), "{body}");
    }
}
