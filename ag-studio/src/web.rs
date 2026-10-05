//! The local web app: an axum server exposing a single-page UI plus a small
//! JSON API that wraps the solver, the translator, and the PDF/PNG exporter.
//!
//! Hardened for exposure behind a reverse proxy (see [`crate::security`]): body
//! limits, a concurrency gate, per-IP rate limiting, an optional shared
//! password (the `/gate` page or HTTP Basic, with an account-free guest mode
//! behind it), a Host allow-list, security headers, gzip compression, and a
//! request timeout.

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

use crate::engine::{self, InputKind, SolveOptions};
use crate::security::{self, AppState, Config, Shared};
use crate::worker::{self, Outcome};
use crate::{auth, db, gate, i18n, present, pwa, render, translate};
use ddar::svg::Theme;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const AUTH_HTML: &str = include_str!("../assets/auth.html");
const LANDING_HTML: &str = include_str!("../assets/landing.html");

fn index_html() -> &'static str {
    static P: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    P.get_or_init(|| pwa::with_splash(INDEX_HTML))
}

fn auth_html() -> &'static str {
    static P: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    P.get_or_init(|| pwa::with_splash(AUTH_HTML))
}

fn landing_html() -> &'static str {
    static P: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    P.get_or_init(|| pwa::with_splash(LANDING_HTML))
}

/// Static files under `/assets/`: (name, content type, cache policy, bytes).
const ASSETS: &[(&str, &str, &str, &[u8])] = &[
    ("i18n.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/i18n.js")),
    ("app.css", "text/css; charset=utf-8", "no-cache", include_bytes!("../assets/app.css")),
    ("site.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/site.js")),
    ("app.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/app.js")),
    ("auth.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/auth.js")),
    ("landing.js", "application/javascript; charset=utf-8", "no-cache", include_bytes!("../assets/landing.js")),
    ("showcase.json", "application/json", "no-cache", include_bytes!("../assets/showcase.json")),
    ("fonts/stix-two-text.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/stix-two-text.woff2")),
    ("fonts/stix-two-text-italic.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/stix-two-text-italic.woff2")),
    ("fonts/inter.woff2", "font/woff2", "public, max-age=604800", include_bytes!("../assets/fonts/inter.woff2")),
];

/// Recent solutions by id, so export and history reopen use exactly what the
/// reader saw instead of solving again.
const SOLUTION_TTL: Duration = Duration::from_secs(30 * 60);
const SOLUTION_CACHE_MAX: usize = 256;
const SOLUTION_CACHE_PER_OWNER: usize = 24;
/// Solutions larger than this are not stored in history (reopen re-solves).
const MAX_STORED_SOLUTION_BYTES: usize = 512 * 1024;

struct CachedSolution {
    id: String,
    owner: String,
    at: std::time::Instant,
    value: std::sync::Arc<serde_json::Value>,
}

struct SolutionCache {
    entries: Vec<CachedSolution>,
}

fn solution_cache() -> &'static std::sync::Mutex<SolutionCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<SolutionCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(SolutionCache { entries: Vec::new() }))
}

impl SolutionCache {
    fn evict_for(&mut self, owner: &str, per_owner: usize, max: usize) {
        self.entries.retain(|e| e.at.elapsed() < SOLUTION_TTL);
        let mine = self.entries.iter().filter(|e| e.owner == owner).count();
        if mine >= per_owner {
            if let Some(i) = self.entries.iter().position(|e| e.owner == owner) {
                self.entries.remove(i);
            }
        }
        if self.entries.len() >= max {
            let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            for e in &self.entries {
                *counts.entry(e.owner.as_str()).or_insert(0) += 1;
            }
            let top = counts.iter().max_by_key(|(_, n)| **n).map(|(o, _)| o.to_string());
            if let Some(top) = top {
                if let Some(i) = self.entries.iter().position(|e| e.owner == top) {
                    self.entries.remove(i);
                }
            }
        }
    }
}

fn cache_put(owner: &str, value: serde_json::Value) -> (String, std::sync::Arc<serde_json::Value>) {
    let id = auth::new_session_id()[..32].to_string();
    let mut value = value;
    value["id"] = serde_json::Value::String(id.clone());
    let arc = std::sync::Arc::new(value);
    let mut c = solution_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    c.evict_for(owner, SOLUTION_CACHE_PER_OWNER, SOLUTION_CACHE_MAX);
    c.entries.push(CachedSolution {
        id: id.clone(),
        owner: owner.to_string(),
        at: std::time::Instant::now(),
        value: arc.clone(),
    });
    (id, arc)
}

fn cache_get(id: &str) -> Option<std::sync::Arc<serde_json::Value>> {
    let c = solution_cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    c.entries
        .iter()
        .find(|e| e.id == id && e.at.elapsed() < SOLUTION_TTL)
        .map(|e| e.value.clone())
}

const SOLUTION_SIG_HEADER: &str = "x-solution-sig";

fn signing_key() -> &'static [u8; 32] {
    static KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        let hex = auth::new_session_id();
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or(0);
        }
        k
    })
}

fn solution_sig(body: &str) -> String {
    security::hmac_sha256(signing_key(), body.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn signed_solution(body: &str, sig: &str) -> Option<serde_json::Value> {
    let want = solution_sig(body);
    let same = want.len() == sig.len() && want.bytes().zip(sig.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    if !same {
        return None;
    }
    serde_json::from_str(body).ok()
}

fn signed_json(value: &serde_json::Value) -> Response {
    let body = value.to_string();
    let sig = solution_sig(&body);
    let mut resp = ([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body).into_response();
    if let Ok(v) = HeaderValue::from_str(&sig) {
        resp.headers_mut().insert(SOLUTION_SIG_HEADER, v);
    }
    resp
}


/// Hard ceiling on a decoded upload, independent of the body limit.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Titles are display labels; anything longer is truncated before use/storage.
const MAX_TITLE_CHARS: usize = 200;
/// Auxiliary constructions accepted by `/api/humanize`.
const MAX_AUX_ITEMS: usize = 64;
const HISTORY_PAGE_DEFAULT: i64 = 100;
const HISTORY_PAGE_MAX: i64 = 200;
/// Body limit of the `/gate` password form.
const GATE_BODY_LIMIT: usize = 4096;
/// How long login/register wait for an argon2 slot before answering 503.
const ARGON2_QUEUE_WAIT: Duration = Duration::from_secs(10);

/// Run the web app until the process is stopped, using the given configuration.
pub async fn serve(config: Config) -> anyhow::Result<()> {
    let bind = config.bind;
    let state = AppState::new(config).map_err(|e| anyhow::anyhow!(e))?;
    let listener = tokio::net::TcpListener::bind(bind).await?;
    serve_on(listener, state).await
}

/// Serve `state` on an already-bound listener. Answers at once: the
/// translation probe (which can take seconds) reports in the background.
async fn serve_on(listener: tokio::net::TcpListener, state: Shared) -> anyhow::Result<()> {
    let bind = listener.local_addr()?;
    println!("\n  GeoSolver  →  http://{bind}\n");
    println!("  Security: {}", state.config.summary());
    if bind.ip().is_loopback() {
        println!("  (loopback only — put a reverse proxy in front to expose it)");
    }
    println!("  Press Ctrl-C to stop.\n");
    report_translate_status(state.clone());
    let header_timeout = state.config.header_timeout;
    accept_loop(listener, app_router(state), header_timeout).await
}

fn report_translate_status(state: Shared) {
    if !state.config.enable_translate {
        println!("  AI translation: disabled by configuration");
        return;
    }
    tokio::spawn(async move {
        let who = if state.config.guest_ai_allowed() { "accounts + guests" } else { "accounts only" };
        let on = format!("on ({who}; local Claude subscription)");
        let note = match state.translate_status().await {
            s if s.logged_in => on.as_str(),
            s if s.installed => "installed — run `claude auth login` to enable",
            _ => "off — install the `claude` CLI to enable",
        };
        println!("  AI translation: {note}");
    });
}

/// HTTP/1 connections with a header-read timeout; each request carries its
/// peer as `ConnectInfo<SocketAddr>`.
async fn accept_loop(
    listener: tokio::net::TcpListener,
    app: Router,
    header_timeout: Duration,
) -> anyhow::Result<()> {
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(hyper_util::rt::TokioTimer::new())
        .header_read_timeout(header_timeout);
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) if is_connection_error(&e) => continue,
            Err(e) => {
                eprintln!("accept failed: {e}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        let conn = http.serve_connection(
            hyper_util::rt::TokioIo::new(stream),
            WithPeer { app: app.clone(), peer },
        );
        tokio::spawn(async move {
            let _ = conn.await;
        });
    }
}

fn is_connection_error(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(e.kind(), ConnectionRefused | ConnectionAborted | ConnectionReset)
}

#[derive(Clone)]
struct WithPeer {
    app: Router,
    peer: SocketAddr,
}

impl hyper::service::Service<hyper::Request<hyper::body::Incoming>> for WithPeer {
    type Response = Response;
    type Error = std::convert::Infallible;
    type Future = axum::routing::future::RouteFuture<std::convert::Infallible>;

    fn call(&self, mut req: hyper::Request<hyper::body::Incoming>) -> Self::Future {
        req.extensions_mut().insert(axum::extract::ConnectInfo(self.peer));
        tower::Service::call(&mut self.app.clone(), req)
    }
}

/// Assemble the full application router (routes + middleware) for `state`.
/// Extracted from [`serve`] so integration tests can drive it without a socket.
fn app_router(state: Shared) -> Router {
    let body_limit = state.config.max_body_bytes;
    Router::new()
        .route("/", get(index))
        .route("/app", get(app_page))
        .route("/auth", get(auth_page))
        .route(
            "/gate",
            get(gate::page).post(gate::submit).layer(DefaultBodyLimit::max(GATE_BODY_LIMIT)),
        )
        .route("/gate/forget", post(gate::forget))
        .route("/manifest.webmanifest", get(pwa::manifest))
        .route("/favicon.svg", get(pwa::favicon_svg))
        .route("/favicon.ico", get(pwa::favicon_ico))
        .route("/apple-touch-icon.png", get(pwa::apple_touch_icon))
        .route("/apple-touch-icon-precomposed.png", get(pwa::apple_touch_icon))
        .route("/icons/{file}", get(pwa::icon_file))
        .route("/splash/{file}", get(pwa::splash_file))
        .route("/robots.txt", get(pwa::robots))
        .route("/assets/{file}", get(asset))
        .route("/assets/fonts/{file}", get(font_asset))
        .route("/healthz", get(healthz))
        .route("/api/status", get(api_status))
        .route("/api/solve", post(api_solve))
        .route("/api/translate", post(api_translate))
        .route("/api/humanize", post(api_humanize))
        .route("/api/export", post(api_export))
        .route("/api/history", get(api_history))
        .route(
            "/api/history/{id}",
            axum::routing::delete(api_history_delete).get(api_history_get).put(api_history_replace),
        )
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
        with_guest_id(&state, &headers, page(index_html()))
    } else if session_user(&state, &headers).await.is_some() {
        Redirect::to("/app").into_response()
    } else {
        page(landing_html())
    }
}

fn page(html: &'static str) -> Response {
    ([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Html(html)).into_response()
}

/// In guest mode, give a browser without one its own guest id, so guests
/// sharing an address do not share one caller's solve slots.
pub(crate) fn with_guest_id(state: &Shared, headers: &HeaderMap, mut resp: Response) -> Response {
    let secure = state.config.secure_cookies;
    if !state.config.guest_allowed() || auth::parse_guest(headers, secure).is_some() {
        return resp;
    }
    if let Ok(v) = HeaderValue::from_str(&auth::set_guest_cookie_header(&auth::new_session_id(), secure)) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
    resp
}

/// `/app`: the solver SPA, gated on a valid session (else 302 to `/auth`)
/// unless guest mode lets Basic auth alone in.
async fn app_page(State(state): State<Shared>, headers: HeaderMap) -> Response {
    if state.config.guest_allowed() || session_user(&state, &headers).await.is_some() {
        with_guest_id(&state, &headers, page(index_html()))
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
async fn auth_page() -> Response {
    page(auth_html())
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
    /// `cookie` when this browser is past the shared password by the gate
    /// cookie (the app then offers "Forget this device").
    #[serde(skip_serializing_if = "Option::is_none")]
    gate: Option<&'static str>,
    solve_deadline_secs: u64,
    /// Whether a solver slot is free right now (else a solve waits in line).
    solver_free: bool,
    /// How long a solve waits in line for a slot before the server says busy.
    queue_wait_secs: f64,
    version: &'static str,
}

/// Never waits on the `claude` probe: a stale or missing result is refreshed
/// in the background and reported as `translate_checking`.
async fn api_status(
    State(state): State<Shared>,
    via: Option<axum::Extension<security::GateVia>>,
    headers: HeaderMap,
) -> Response {
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
    } else if user.is_none() && !state.config.guest_ai_allowed() {
        Some("sign_in")
    } else {
        None
    };
    let guest = user.is_none() && state.config.guest_allowed();
    let status = Json(Status {
        translate_installed: s.installed,
        translate_logged_in: s.logged_in,
        translate_checking: checking,
        can_translate: block.is_none(),
        translate_block: block,
        signed_in: user.is_some(),
        guest,
        username: user.map(|u| u.username),
        gate: via.filter(|v| v.0 == security::GateVia::Cookie).map(|_| "cookie"),
        solve_deadline_secs: state.config.solve_deadline.as_secs(),
        solver_free: state.heavy.available_permits() > 0,
        queue_wait_secs: state.config.queue_wait.as_secs_f64(),
        version: env!("CARGO_PKG_VERSION"),
    })
    .into_response();
    if guest {
        with_guest_id(&state, &headers, status)
    } else {
        status
    }
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
/// Basic-auth layer has already admitted the request — an anonymous guest,
/// with the browser's guest id when it sent one.
enum Caller {
    User(db::User),
    Guest(Option<String>),
}

impl FromRequestParts<Shared> for Caller {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Response> {
        match SessionUser::from_request_parts(parts, state).await {
            Ok(SessionUser(u)) => Ok(Caller::User(u)),
            Err(_) if state.config.guest_allowed() => {
                Ok(Caller::Guest(auth::parse_guest(&parts.headers, state.config.secure_cookies)))
            }
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
        return Err(err_code(StatusCode::BAD_REQUEST, "username_rule", i18n::t(lang, "auth.username_rule")));
    }
    if !(8..=128).contains(&password.chars().count()) {
        return Err(err_code(StatusCode::BAD_REQUEST, "pw_len", i18n::t(lang, "auth.pw_len")));
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
        Ok(Err(RegisterErr::Taken)) => err_code(StatusCode::CONFLICT, "user_taken", i18n::t(lang, "auth.user_taken")),
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
        Ok(None) => err_code(StatusCode::UNAUTHORIZED, "bad_creds", i18n::t(lang, "auth.bad_creds")),
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

/// [`err`] plus a machine-readable `code` the client maps to a field.
fn err_code(status: StatusCode, code: &str, msg: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": msg.into(), "code": code }))).into_response()
}

/// A compile/parse error: a localized sentence, where it is, and the engine's
/// own words as `detail`.
fn compile_err(lang: i18n::Lang, input: &str, raw: &str) -> Response {
    let d = present::diagnose(input, raw);
    let msg = i18n::compile_message(lang, &d);
    let detail = compile_detail(&d, raw);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg, "code": "compile", "diagnosis": d, "detail": detail })),
    )
        .into_response()
}

/// The engine's words for the "technical details" toggle, when they add
/// something: none for an empty `no attempt`, and the relation compiler's
/// message rather than the metric fallback's when the relation is unknown.
fn compile_detail(d: &present::Diagnosis, raw: &str) -> Option<String> {
    let bare = raw.trim_start_matches("compile error: ").trim_start_matches("metric prover: ").trim();
    if bare.is_empty() || bare == "no attempt" {
        return None;
    }
    if d.key == "unknown_relation" && bare.contains("is not a metric value here") {
        return d.token.as_ref().map(|t| format!("compile error: unknown relation `{t}`"));
    }
    Some(present::readable_engine_message(raw))
}

/// Where a goal divides by a literal zero: (line, column), both 1-based.
fn division_by_zero(input: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = input.lines().collect();
    let i = lines.iter().rposition(|l| l.split('#').next().unwrap_or("").trim_start().starts_with("prove"))?;
    let code: Vec<char> = lines[i].split('#').next().unwrap_or("").chars().collect();
    (0..code.len()).find_map(|k| {
        if code[k] != '/' {
            return None;
        }
        let mut j = k + 1;
        while j < code.len() && code[j] == ' ' {
            j += 1;
        }
        let num: String = code[j..].iter().take_while(|c| c.is_ascii_digit() || **c == '.').collect();
        let next = code.get(j + num.chars().count());
        let zero = !num.is_empty() && num.parse::<f64>().is_ok_and(|v| v == 0.0);
        (zero && next.is_none_or(|c| !c.is_ascii_alphanumeric() && *c != '_' && *c != '(' && *c != '^')).then_some((i + 1, k + 1))
    })
}

fn division_by_zero_err(lang: i18n::Lang, at: (usize, usize)) -> Response {
    let d = present::Diagnosis { key: "div_zero", line: at.0, col: at.1, len: 1, token: None, expected: None, got: None };
    let msg = i18n::compile_message(lang, &d);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg, "code": "compile", "diagnosis": d, "detail": "the goal divides by zero" })),
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
    let t = t.trim();
    if t.chars().count() <= MAX_TITLE_CHARS {
        return (!t.is_empty()).then(|| t.to_string());
    }
    let cut: String = t.chars().take(MAX_TITLE_CHARS - 1).collect();
    let at_word = match cut.rfind(char::is_whitespace) {
        Some(i) if cut[..i].chars().count() > MAX_TITLE_CHARS / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    Some(format!("{}\u{2026}", at_word.trim_end_matches(|c: char| c.is_whitespace() || ",;:-".contains(c))))
}

/// A time-limit verdict states the limit the caller was promised (the engine
/// measures from after compiling, so it would say 58 s for a 60 s limit).
fn advertise_limit(mut value: serde_json::Value, limit: Duration) -> serde_json::Value {
    if value["view"]["note"]["key"] == "time_limit" {
        value["view"]["note"]["secs"] = serde_json::json!(limit.as_secs_f64().round());
    }
    value
}

/// Seconds a client is told to wait after a 503 for a full server.
const BUSY_RETRY_AFTER: &str = "10";

/// Acquire a heavy-work slot, waiting at most `queue_wait`; past that, 503
/// with `Retry-After`.
#[allow(clippy::result_large_err)]
async fn heavy_permit(
    state: &Shared,
    headers: &HeaderMap,
) -> Result<tokio::sync::OwnedSemaphorePermit, Response> {
    let wait = state.config.queue_wait;
    match tokio::time::timeout(wait, state.heavy.clone().acquire_owned()).await {
        Ok(Ok(permit)) => Ok(permit),
        _ => Err(busy(headers)),
    }
}

fn busy(headers: &HeaderMap) -> Response {
    let lang = i18n::lang_from_headers(headers);
    let mut resp = err_code(StatusCode::SERVICE_UNAVAILABLE, "busy", i18n::t(lang, "server.busy"));
    resp.headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static(BUSY_RETRY_AFTER));
    resp
}

/// The client-facing JSON of a solve: the worker's presentation (it alone holds
/// the figure source), or, for a solve the worker never answered, the same
/// presentation built here from the bare time-limit solution.
fn view_of(sol: &engine::Solution, reply: Option<&worker::Reply>, title: Option<&str>) -> serde_json::Value {
    match reply.and_then(|r| r.view.clone()) {
        Some(v) => v,
        None => present::solution_json(sol, title),
    }
}

/// A worker outcome as a solution, or the error response for the client:
/// input errors become a located `compile` error, a killed worker the honest
/// time-limit verdict.
#[allow(clippy::result_large_err)]
fn solution_of(
    outcome: Outcome,
    req: &worker::Request,
    headers: &HeaderMap,
) -> Result<(engine::Solution, Option<Box<worker::Reply>>), Response> {
    match outcome {
        Outcome::Done(mut reply) => {
            let result = std::mem::replace(&mut reply.result, Err(String::new()));
            let lang = i18n::lang_from_headers(headers);
            match result {
                Ok(sol) if !sol.proved && input_error_note(&sol.note) => {
                    Err(compile_err(lang, &req.input, &sol.note))
                }
                Ok(sol) => Ok((sol, Some(reply))),
                Err(e) => {
                    eprintln!("solve error: {e}");
                    Err(compile_err(lang, &req.input, &e))
                }
            }
        }
        Outcome::TimedOut => {
            eprintln!("solve worker killed at its {:.0}s hard limit", req.hard_limit().as_secs_f64());
            Ok((worker::time_limit_solution(&req.input, req.limit()), None))
        }
        Outcome::Failed(e) => {
            eprintln!("solve worker failed: {e}");
            Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                i18n::t(i18n::lang_from_headers(headers), "solve.failed"),
            ))
        }
    }
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

fn input_error_note(note: &str) -> bool {
    let Some(m) = note.strip_prefix("metric prover: ") else { return false };
    [
        "bad number",
        "trailing tokens",
        "unexpected token",
        "unexpected character",
        "expected ",
        "unknown point",
        "unknown name",
        "unknown construction",
        "unknown relation",
    ]
    .iter()
    .any(|p| m.starts_with(p))
        || m.contains(" expects ")
        || (m.contains(" got ") && (m.ends_with(" arguments") || m.ends_with(" argument")))
}

fn split_args(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth) = (Vec::new(), String::new(), 0i32);
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur.trim().to_string());
    out
}

/// The point a predicate goal repeats where that makes it vacuous or
/// degenerate (`cyclic(A, B, C, A)`, `perp(A, A, B, C)`, `cong(A, B, B, A)`).
fn degenerate_goal(input: &str) -> Option<String> {
    let goal = present::source_goal(input)?;
    let open = goal.find('(')?;
    let name = goal[..open].trim();
    let mut depth = 0i32;
    let close = goal[open..].char_indices().find_map(|(k, c)| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        (depth == 0).then_some(open + k)
    })?;
    if !goal[close + 1..].trim().is_empty() {
        return None;
    }
    let args = split_args(&goal[open + 1..close]);
    let is_pt = |a: &str| a.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && a.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '\'');
    let repeated = |xs: &[String]| {
        xs.iter().enumerate().find_map(|(i, a)| (is_pt(a) && xs[..i].contains(a)).then(|| a.clone()))
    };
    let pairs = |xs: &[String]| xs.chunks(2).find_map(|p| (p.len() == 2 && is_pt(&p[0]) && p[0] == p[1]).then(|| p[0].clone()));
    let same_pairs = |xs: &[String], k: usize| {
        let key = |c: &[String]| {
            let mut c = c.to_vec();
            c.sort();
            c
        };
        let chunks: Vec<&[String]> = xs.chunks(k).collect();
        (chunks.len() == 2 && chunks[0].iter().chain(chunks[1]).all(|a| is_pt(a)) && key(chunks[0]) == key(chunks[1]))
            .then(|| chunks[0][0].clone())
    };
    match (name, args.len()) {
        ("cyclic" | "coll" | "collinear" | "concyclic", _) => repeated(&args),
        ("midp" | "midpoint", 3) => repeated(&args),
        ("perp" | "para" | "cong", 4) => pairs(&args).or_else(|| same_pairs(&args, 2)),
        ("eqangle" | "eqratio", 8) => pairs(&args).or_else(|| same_pairs(&args, 4)),
        _ => None,
    }
}

fn degenerate_goal_err(lang: i18n::Lang, input: &str, point: &str) -> Response {
    let line = input.lines().collect::<Vec<_>>().iter().rposition(|l| l.split('#').next().unwrap_or("").trim_start().starts_with("prove"));
    let mut d = present::Diagnosis { key: "degenerate_goal", line: 0, col: 0, len: 0, token: Some(point.to_string()), expected: None, got: None };
    if let Some(i) = line {
        let code: Vec<char> = input.lines().nth(i).unwrap_or("").chars().collect();
        let p: Vec<char> = point.chars().collect();
        let hits: Vec<usize> = (0..code.len().saturating_sub(p.len() - 1))
            .filter(|&k| {
                code[k..k + p.len()] == p[..]
                    && (k == 0 || !code[k - 1].is_ascii_alphanumeric())
                    && code.get(k + p.len()).is_none_or(|c| !c.is_ascii_alphanumeric() && *c != '\'')
            })
            .collect();
        d.line = i + 1;
        d.col = hits.get(1).or(hits.first()).map_or(1, |k| k + 1);
        d.len = p.len();
    }
    let msg = i18n::compile_message(lang, &d);
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg, "code": "compile", "diagnosis": d, "detail": format!("the goal repeats point {point}") })),
    )
        .into_response()
}

fn caller_key(caller: &Caller, ip: Option<&axum::Extension<security::ClientIp>>) -> String {
    match caller {
        Caller::User(u) => format!("u{}", u.id),
        Caller::Guest(Some(id)) => format!("g{id}"),
        Caller::Guest(None) => format!("g{}", ip_text(ip)),
    }
}

fn ip_text(ip: Option<&axum::Extension<security::ClientIp>>) -> String {
    ip.map_or_else(|| "?".to_string(), |e| e.0 .0.to_string())
}

/// Per-caller slot, then, for guests, the per-address ceiling. Only the first
/// is the caller's own doing (`busy_self`, with the limit); a full address is
/// the plain server-busy answer, since other people behind it hold the slots.
#[allow(clippy::result_large_err)]
fn claim_solve_slots(
    state: &Shared,
    caller: &Caller,
    ip: Option<&axum::Extension<security::ClientIp>>,
    headers: &HeaderMap,
) -> Result<(security::CallerSlot, Option<security::CallerSlot>), Response> {
    let Some(own) = state.claim_caller(caller_key(caller, ip)) else {
        let lang = i18n::lang_from_headers(headers);
        let limit = state.per_caller_limit();
        let msg = i18n::tp(lang, "server.busy_self", limit as u64, &[("n", limit.to_string())]);
        let mut resp = (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": msg, "code": "busy_self", "limit": limit })),
        )
            .into_response();
        resp.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from_static(BUSY_RETRY_AFTER));
        return Err(resp);
    };
    let shared = match caller {
        Caller::Guest(_) => match state.claim_within(format!("i{}", ip_text(ip)), state.guest_ip_limit()) {
            Some(s) => Some(s),
            None => return Err(busy(headers)),
        },
        Caller::User(_) => None,
    };
    Ok((own, shared))
}

async fn api_solve(
    State(state): State<Shared>,
    caller: Caller,
    ip: Option<axum::Extension<security::ClientIp>>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<SolveReq>,
) -> Response {
    if let Err(e) = check_input(&state, &headers, &req.input) {
        return e;
    }
    if let Some(p) = degenerate_goal(&req.input) {
        return degenerate_goal_err(i18n::lang_from_headers(&headers), &req.input, &p);
    }
    if let Some(at) = division_by_zero(&req.input) {
        return division_by_zero_err(i18n::lang_from_headers(&headers), at);
    }
    let slots = match claim_solve_slots(&state, &caller, ip.as_ref(), &headers) {
        Ok(s) => s,
        Err(resp) => return resp,
    };
    let permit = match heavy_permit(&state, &headers).await {
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
    // Reject non-finite budgets; clamp to a server-safe ceiling.
    let budget = Duration::from_secs_f64(
        req.budget_secs
            .filter(|v| v.is_finite())
            .unwrap_or(20.0)
            .clamp(0.5, 60.0),
    )
    .min(state.config.solve_deadline);
    let mut job = if req.best {
        worker::Request::new(&req.input, &opts, worker::Mode::Best, budget)
    } else {
        worker::Request::new(&req.input, &opts, worker::Mode::Solve, state.config.solve_deadline)
    };
    job.present = true;
    // The permit (and the per-caller slot) live with the worker process: freed
    // when it is reaped, also when this future is dropped because the client
    // went away or cancelled.
    let outcome = worker::run(&job, Some(permit)).await;
    drop(slots);
    match solution_of(outcome, &job, &headers) {
        Ok((sol, reply)) => {
            let mut value = advertise_limit(view_of(&sol, reply.as_deref(), history_title.as_deref()), job.limit());
            if req.best {
                value["search"] = serde_json::json!("shortest");
            }
            let (_, value) = cache_put(&caller_key(&caller, ip.as_ref()), value);
            let mut out = value.as_ref().clone();
            if let (Caller::User(user), true) = (&caller, req.record) {
                if let Some(id) = save_history(&state, user.id, &sol, history_title.as_deref(), &value).await {
                    out["history_id"] = serde_json::json!(id);
                }
            }
            signed_json(&out)
        }
        Err(resp) => resp,
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
) -> Option<i64> {
    if sol.input.len() > state.config.max_input_chars {
        return None;
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
    tokio::task::spawn_blocking(move || {
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
    .await
    .ok()
    .and_then(Result::ok)
}

#[derive(Deserialize)]
struct ReplaceReq {
    id: String,
}

/// `PUT /api/history/{id}` `{id: <cached solution id>}`: the client swapped a
/// shorter proof in for the one it recorded, so the entry must reopen as shown.
async fn api_history_replace(
    State(state): State<Shared>,
    SessionUser(user): SessionUser,
    axum::extract::Path(row): axum::extract::Path<i64>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<ReplaceReq>,
) -> Response {
    let lang = i18n::lang_from_headers(&headers);
    let Some(value) = cache_get(&req.id) else {
        return err(StatusCode::GONE, i18n::t(lang, "export.expired"));
    };
    let mut stored = value.as_ref().clone();
    if let Some(o) = stored.as_object_mut() {
        o.remove("id");
        o.remove("history_id");
    }
    let solution = stored.to_string();
    if solution.len() > MAX_STORED_SOLUTION_BYTES {
        return err(StatusCode::PAYLOAD_TOO_LARGE, i18n::t(lang, "err.too_large"));
    }
    let s = |k: &str| stored[k].as_str().map(str::to_string);
    let (input, title, method, status) = (s("input").unwrap_or_default(), s("title"), s("method"), s("status"));
    let proved = stored["proved"].as_bool().unwrap_or(false);
    let goal = stored["view"].get("goal").filter(|g| !g.is_null()).map(|g| g.to_string());
    let db = state.db.clone();
    let res = tokio::task::spawn_blocking(move || {
        db::replace_history(
            &db::lock(&db),
            user.id,
            row,
            &db::NewHistory {
                input: &input,
                title: title.as_deref(),
                proved,
                method: method.as_deref(),
                status: status.as_deref(),
                goal: goal.as_deref(),
                solution: Some(&solution),
            },
        )
    })
    .await;
    match res {
        Ok(Ok(true)) => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(false)) => err(StatusCode::NOT_FOUND, i18n::t(lang, "history.none")),
        _ => err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "history.load_fail")),
    }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    as_drawn: bool,
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
            note: h.note,
            as_drawn: h.as_drawn,
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
                let (_, v) = cache_put(&format!("u{}", user.id), v);
                signed_json(&v)
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
    caller: Caller,
    ip: Option<axum::Extension<security::ClientIp>>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<TranslateReq>,
) -> Response {
    if let Caller::Guest(_) = caller {
        if !state.config.guest_ai {
            let lang = i18n::lang_from_headers(&headers);
            return err_code(StatusCode::UNAUTHORIZED, "sign_in", i18n::t(lang, "auth.sign_in_ai"));
        }
    }
    if !state.config.enable_translate || !translate::available() {
        return err(
            StatusCode::SERVICE_UNAVAILABLE,
            i18n::t(i18n::lang_from_headers(&headers), "translate.disabled"),
        );
    }
    if let (Caller::Guest(_), Some(ip)) = (&caller, ip.as_ref()) {
        if !state.guest_translation_allowed(ip.0 .0) {
            let lang = i18n::lang_from_headers(&headers);
            return err_code(StatusCode::TOO_MANY_REQUESTS, "daily", i18n::t(lang, "translate.daily"));
        }
    }
    if let Some(t) = &req.text {
        if t.len() > state.config.max_input_chars {
            return err(StatusCode::PAYLOAD_TOO_LARGE, i18n::t(i18n::lang_from_headers(&headers), "translate.too_long"));
        }
    }
    let permit = match heavy_permit(&state, &headers).await {
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
    let permit = match heavy_permit(&state, &headers).await {
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
    /// A solve answer exactly as this server sent it, with its
    /// `x-solution-sig`: rendered as is when the cached copy has expired.
    #[serde(default)]
    signed: Option<String>,
    #[serde(default)]
    sig: Option<String>,
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
    let id = req.id.as_deref().filter(|s| !s.is_empty());
    let signed = req.signed.as_deref().filter(|s| !s.is_empty());
    if id.is_some() || signed.is_some() {
        let value = id.and_then(cache_get).or_else(|| {
            signed.and_then(|body| signed_solution(body, req.sig.as_deref().unwrap_or(""))).map(std::sync::Arc::new)
        });
        let Some(value) = value else {
            return (
                StatusCode::GONE,
                Json(serde_json::json!({ "error": i18n::t(lang, "export.expired"), "code": "expired" })),
            )
                .into_response();
        };
        let Ok(permit) = state.render.clone().try_acquire_owned() else {
            return err(StatusCode::SERVICE_UNAVAILABLE, i18n::t(lang, "server.busy"));
        };
        let res = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
            let _permit = permit;
            if want_pdf {
                render::report_pdf_from_json(&value, lang)
            } else {
                render::report_png_from_json(&value, lang)
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
    let permit = match heavy_permit(&state, &headers).await {
        Ok(p) => p,
        Err(e) => return e,
    };
    let title = clean_title(req.title);
    // Documents export on a light, print-friendly page regardless of on-screen theme.
    let opts = SolveOptions {
        kind: kind_of(&req.input, &req.kind),
        theme: Theme::Light,
        want_proof: true,
        title,
        panel: true,
    };
    let mut job =
        worker::Request::new(&req.input, &opts, worker::Mode::Solve, state.config.solve_deadline);
    job.report = Some(if want_pdf { worker::Format::Pdf } else { worker::Format::Png });
    job.lang = lang;
    let failed = || err(StatusCode::INTERNAL_SERVER_ERROR, i18n::t(lang, "export.failed"));

    let outcome = worker::run(&job, Some(permit)).await;
    if matches!(outcome, Outcome::TimedOut) {
        eprintln!("export worker killed at its {:.0}s hard limit", job.hard_limit().as_secs_f64());
        return err(
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                "the solve overran its {:.0}s time limit and was stopped; nothing to export",
                job.limit().as_secs_f64()
            ),
        );
    }
    let reply = match solution_of(outcome, &job, &headers) {
        Ok((_, Some(reply))) => reply,
        Ok((_, None)) => return failed(),
        Err(resp) => return resp,
    };
    match reply.report_bytes() {
        Some(Ok(bytes)) => export_response(bytes, want_pdf),
        Some(Err(e)) => {
            eprintln!("export error: {e}");
            failed()
        }
        None => failed(),
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

    #[tokio::test]
    async fn html_pages_are_never_cached() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "gina").await;
        for (uri, c) in [("/", None), ("/auth", None), ("/app", Some(cookie.as_str()))] {
            let (st, headers, _) = get_page(&state, uri, c).await;
            assert_eq!(st, StatusCode::OK, "{uri}");
            assert_eq!(headers.get(header::CACHE_CONTROL).map(|v| v.to_str().unwrap()), Some("no-store"), "{uri}");
        }
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
    async fn a_malformed_number_is_a_compile_error_not_a_verdict() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "noether2").await;
        let input = "B = free\nC = point: dist(B,C)=6.2.3\nprove dist(B,C)^2 = 36";
        let (st, _, body) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": input})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["code"], "compile", "{body:?}");
        assert_eq!(body["diagnosis"]["key"], "bad_number", "{body:?}");
        assert_eq!(body["diagnosis"]["line"], 2, "{body:?}");
        let (_, _, rows) =
            call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(rows.as_array().map(Vec::len), Some(0), "{rows:?}");
    }

    #[tokio::test]
    async fn malformed_metric_goals_are_compile_errors_not_verdicts() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "noether3").await;
        let cases = [
            ("A B C = triangle\nprove dist(A,B) = 2)", "trailing", 2, 20),
            ("A B C = triangle\nprove dist(A,B) = 2 3", "trailing", 2, 21),
            ("A B C = triangle\nprove dist(A,B) == 2", "unexpected_token", 2, 18),
            ("B = free\nC = point: dist(B,C)=6\nprove dist(B,C)^x = 36", "expect_exponent", 3, 17),
            ("A B C D = cyclic_quad\nprove dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D) + dist(A,D)*dist(B,C)", "unknown_shape", 1, 11),
            ("A B C D = cyclic_quad\nprove cyclic(A, B, C, D)", "unknown_shape", 1, 11),
        ];
        for (input, key, line, col) in cases {
            let (st, _, body) =
                call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": input})).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{input}: {body:?}");
            assert_eq!(body["code"], "compile", "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["key"], key, "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["line"], line, "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["col"], col, "{input}: {body:?}");
        }
        let (_, _, rows) =
            call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(rows.as_array().map(Vec::len), Some(0), "{rows:?}");
    }

    #[tokio::test]
    async fn goals_that_repeat_a_point_are_refused_not_proved() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "noether4").await;
        for (input, point) in [
            ("A B C = triangle\nprove cyclic(A, B, C, A)", "A"),
            ("A B C = triangle\nprove coll(A, B, A)", "A"),
            ("A B C = triangle\nprove perp(A, A, B, C)", "A"),
            ("A B C = triangle\nprove cong(A, B, B, A)", "A"),
            ("A B C = triangle\nprove para(A, B, B, A)", "A"),
        ] {
            let (st, _, body) =
                call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": input})).await;
            assert_eq!(st, StatusCode::BAD_REQUEST, "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["key"], "degenerate_goal", "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["token"], point, "{input}: {body:?}");
            assert_eq!(body["diagnosis"]["line"], 2, "{input}: {body:?}");
        }
        for input in [
            "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))",
            "A B C = triangle\nI = incenter(A, B, C)\nprove eqangle(A, B, A, I, A, I, A, C)",
        ] {
            let (st, _, body) =
                call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": input, "record": false})).await;
            assert_eq!(st, StatusCode::OK, "{input}: {body:?}");
            assert_eq!(body["status"], "proved", "{input}: {body:?}");
        }
    }

    #[tokio::test]
    async fn one_caller_cannot_take_every_solver_slot() {
        let (state, _dir) = test_state();
        let limit = state.per_caller_limit();
        assert!(limit < state.config.max_concurrent || state.config.max_concurrent == 1, "a caller must leave slots for others");
        let held: Vec<_> = (0..limit).map(|_| state.claim_caller("u1".into()).expect("within the limit")).collect();
        assert!(state.claim_caller("u1".into()).is_none(), "over the per-caller limit");
        assert!(state.claim_caller("u2".into()).is_some(), "another caller is unaffected");
        drop(held);
        assert!(state.claim_caller("u1".into()).is_some(), "slots come back when the solves finish");
    }

    #[tokio::test]
    async fn auth_errors_carry_a_field_code() {
        let (state, _dir) = test_state();
        let _ = register_cookie(&state, "takenname").await;
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/auth/register",
            None,
            serde_json::json!({"username": "takenname", "password": "password123"}),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT, "{body:?}");
        assert_eq!(body["code"], "user_taken", "{body:?}");
        let (st, _, body) = call(
            &state,
            "POST",
            "/api/auth/register",
            None,
            serde_json::json!({"username": "newname", "password": "x".repeat(129)}),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["code"], "pw_len", "{body:?}");
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
        assert_eq!(body["error"], i18n::t(i18n::Lang::En, "rate.exceeded"));
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
    async fn guests_behind_one_address_are_not_told_their_own_solves_are_running() {
        let (state, _dir) = state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse(":shared-secret").unwrap());
            c.guest_mode = true;
            c.max_concurrent = 2;
        });
        let auth = basic("", "shared-secret");
        let (st, h, _) = send(&state, build("GET", "/", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        let set = h.get(header::SET_COOKIE).and_then(|v| v.to_str().ok()).expect("a guest id for a new browser");
        assert!(set.starts_with("gid=") && set.contains("HttpOnly"), "{set}");
        let browser_b = cookie_of(set);
        let (_, h, _) = send(&state, build("GET", "/", &[("authorization", &auth), ("cookie", &browser_b)], None)).await;
        assert!(h.get(header::SET_COOKIE).is_none(), "a browser keeps the id it has");

        let solve = serde_json::json!({"input": ISOSCELES_GEO}).to_string();
        let as_b = [("authorization", auth.as_str()), ("cookie", browser_b.as_str())];
        let ip = std::net::Ipv4Addr::UNSPECIFIED;
        let guest_a = state.claim_caller(format!("g{ip}")).unwrap();
        let guest_a_addr = state.claim_within(format!("i{ip}"), state.guest_ip_limit()).unwrap();
        let (st, h, text) = send(&state, build("POST", "/api/solve", &as_b, Some(&solve))).await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{text}");
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["code"], "busy", "another guest's solve is not this browser's: {text}");
        assert!(h.get(header::RETRY_AFTER).is_some());
        drop((guest_a, guest_a_addr));

        let own = state.claim_caller(format!("g{}", &browser_b[4..])).unwrap();
        let (st, h, text) = send(&state, build("POST", "/api/solve", &as_b, Some(&solve))).await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["code"], "busy_self", "{text}");
        assert_eq!(body["limit"], 1);
        assert!(body["error"].as_str().unwrap().contains("one solve at a time"), "{text}");
        assert!(h.get(header::RETRY_AFTER).is_some());
        drop(own);

        let (st, _, text) = send(&state, build("POST", "/api/solve", &as_b, Some(&solve))).await;
        assert_eq!(st, StatusCode::OK, "{text}");
    }

    #[tokio::test]
    async fn an_expired_result_exports_from_its_signed_copy_without_solving_again() {
        let (state, _dir) = guest_state(true);
        let auth = basic("", "shared-secret");
        let solve = serde_json::json!({"input": ISOSCELES_GEO}).to_string();
        let (st, h, text) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&solve))).await;
        assert_eq!(st, StatusCode::OK);
        let sig = h.get(SOLUTION_SIG_HEADER).and_then(|v| v.to_str().ok()).expect("signed answer").to_string();
        let export = |body: &str, sig: &str| serde_json::json!({"id": "0".repeat(32), "signed": body, "sig": sig, "format": "png"}).to_string();
        let (st, h, _) = send(&state, build("POST", "/api/export", &[("authorization", &auth)], Some(&export(&text, &sig)))).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h.get(header::CONTENT_TYPE).unwrap(), "image/png");
        let forged = text.replacen("\"status\":\"proved\"", "\"status\":\"refuted\"", 1);
        assert_ne!(forged, text);
        let (st, _, _) = send(&state, build("POST", "/api/export", &[("authorization", &auth)], Some(&export(&forged, &sig)))).await;
        assert_eq!(st, StatusCode::GONE, "an edited answer is not rendered");
        let (st, _, _) = send(&state, build("POST", "/api/export", &[("authorization", &auth)], Some(&export(&text, &"0".repeat(64))))).await;
        assert_eq!(st, StatusCode::GONE);
    }

    #[tokio::test]
    async fn shortest_proof_answers_say_their_time_is_the_search_budget() {
        let (state, _dir) = guest_state(true);
        let auth = basic("", "shared-secret");
        let best = serde_json::json!({"input": ISOSCELES_GEO, "best": true, "budget_secs": 1}).to_string();
        let (st, _, text) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&best))).await;
        assert_eq!(st, StatusCode::OK, "{text}");
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["search"], "shortest");
        let plain = serde_json::json!({"input": ISOSCELES_GEO}).to_string();
        let (_, _, text) = send(&state, build("POST", "/api/solve", &[("authorization", &auth)], Some(&plain))).await;
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(body.get("search").is_none());
        let (_, _, text) = send(&state, build("GET", "/api/status", &[("authorization", &auth)], None)).await;
        let status: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(status["solver_free"], true);
        assert!(status["queue_wait_secs"].as_f64().is_some());
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

    #[cfg(unix)]
    mod preemption {
        use super::*;
        use crate::worker::tests::{hang_input, reaped, wait_for_pid, wait_reaped};
        use std::time::Instant;
        use tokio::io::AsyncWriteExt;

        async fn bounded<T>(f: impl std::future::Future<Output = T>) -> T {
            tokio::time::timeout(Duration::from_secs(60), f).await.expect("test step hung")
        }

        fn post(uri: &str, cookie: &str, body: serde_json::Value) -> Request<Body> {
            build("POST", uri, &[("cookie", cookie)], Some(&body.to_string()))
        }

        async fn wait_permits(state: &Shared, n: usize) {
            bounded(async {
                while state.heavy.available_permits() != n {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await;
        }

        #[tokio::test]
        async fn a_solve_past_its_deadline_is_killed_reported_not_proven_and_frees_its_slot() {
            let (state, _dir) = state_with(|c| {
                c.max_concurrent = 1;
                c.solve_deadline = Duration::from_secs(1);
            });
            let cookie = register_cookie(&state, "deadline").await;
            let tmp = tempfile::tempdir().unwrap();
            let (input, pidfile) = hang_input(tmp.path(), "pid");
            let t = Instant::now();
            let (st, _, body) =
                bounded(send(&state, post("/api/solve", &cookie, serde_json::json!({"input": input})))).await;
            let took = t.elapsed();
            assert_eq!(st, StatusCode::OK, "{body}");
            let sol: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(sol["proved"], false);
            assert_eq!(sol["status"], "not-proved");
            assert!(sol["note"].as_str().unwrap().contains("time limit"), "{sol}");
            assert_eq!(sol["view"]["note"]["secs"], 1.0, "the verdict states the configured limit: {sol}");
            assert!(took < Duration::from_secs(1) + worker::GRACE + Duration::from_secs(2), "{took:?}");
            let pid = wait_for_pid(&pidfile).await;
            assert!(reaped(pid), "worker {pid} outlived its request");
            assert_eq!(state.heavy.available_permits(), 1, "the slot leaked");

            let (st, _, body) =
                bounded(send(&state, post("/api/solve", &cookie, serde_json::json!({"input": ISOSCELES_GEO})))).await;
            assert_eq!(st, StatusCode::OK, "{body}");
            assert!(body.contains("\"proved\":true"), "{body}");
        }

        #[tokio::test]
        async fn an_export_past_its_deadline_is_killed_and_answers_504() {
            let (state, _dir) = state_with(|c| {
                c.max_concurrent = 1;
                c.solve_deadline = Duration::from_secs(1);
            });
            let cookie = register_cookie(&state, "exporter").await;
            let tmp = tempfile::tempdir().unwrap();
            let (input, pidfile) = hang_input(tmp.path(), "pid");
            let body = serde_json::json!({"input": input, "format": "pdf"});
            let (st, _, text) = bounded(send(&state, post("/api/export", &cookie, body))).await;
            assert_eq!(st, StatusCode::GATEWAY_TIMEOUT, "{text}");
            assert!(text.contains("time limit"), "{text}");
            assert!(reaped(wait_for_pid(&pidfile).await));
            assert_eq!(state.heavy.available_permits(), 1);
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn a_saturated_server_answers_503_with_retry_after_and_stays_responsive() {
            let (state, _dir) = state_with(|c| {
                c.max_concurrent = 2;
                c.queue_wait = Duration::from_millis(300);
            });
            let cookie = register_cookie(&state, "busy").await;
            let tmp = tempfile::tempdir().unwrap();
            let mut hogs = Vec::new();
            let mut pids = Vec::new();
            for i in 0..2 {
                let (input, pidfile) = hang_input(tmp.path(), &format!("pid{i}"));
                // One user per hog: a single caller may only hold half the slots.
                let cookie = register_cookie(&state, &format!("hog{i}")).await;
                let state = state.clone();
                hogs.push(tokio::spawn(async move {
                    send(&state, post("/api/solve", &cookie, serde_json::json!({"input": input}))).await
                }));
                pids.push(wait_for_pid(&pidfile).await);
            }
            assert_eq!(state.heavy.available_permits(), 0);

            let t = Instant::now();
            let (st, headers, body) =
                bounded(send(&state, post("/api/solve", &cookie, serde_json::json!({"input": ISOSCELES_GEO})))).await;
            let queued = t.elapsed();
            assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "{body}");
            assert_eq!(headers.get(header::RETRY_AFTER).unwrap(), BUSY_RETRY_AFTER);
            assert!(queued >= Duration::from_millis(300), "did not queue: {queued:?}");
            assert!(queued < Duration::from_secs(2), "queued too long: {queued:?}");
            let (st, _, _) = bounded(send(&state, post("/api/export", &cookie, serde_json::json!({"input": ISOSCELES_GEO})))).await;
            assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);

            for (uri, cookie) in [("/healthz", None), ("/", None), ("/auth", None), ("/app", Some(cookie.as_str()))] {
                let t = Instant::now();
                let headers: Vec<(&str, &str)> = cookie.map(|c| ("cookie", c)).into_iter().collect();
                let (st, _, _) = bounded(send(&state, build("GET", uri, &headers, None))).await;
                assert_eq!(st, StatusCode::OK, "{uri}");
                assert!(t.elapsed() < Duration::from_millis(500), "{uri} took {:?} under saturation", t.elapsed());
            }

            for hog in &hogs {
                hog.abort();
            }
            for pid in pids {
                wait_reaped(pid, Duration::from_secs(3)).await;
            }
            wait_permits(&state, 2).await;
        }

        async fn bind_test_port() -> tokio::net::TcpListener {
            for port in 18800..18850 {
                if let Ok(l) = tokio::net::TcpListener::bind(("127.0.0.1", port)).await {
                    return l;
                }
            }
            panic!("no free port in 18800-18849");
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn a_client_that_disconnects_mid_solve_gets_its_worker_killed_and_slot_freed() {
            let (state, _dir) = state_with(|c| c.max_concurrent = 1);
            let cookie = register_cookie(&state, "leaver").await;
            let listener = bind_test_port().await;
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(serve_on(listener, state.clone()));

            let tmp = tempfile::tempdir().unwrap();
            let (input, pidfile) = hang_input(tmp.path(), "pid");
            let body = serde_json::json!({"input": input}).to_string();
            let request = format!(
                "POST /api/solve HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nCookie: {cookie}\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                addr.port(),
                body.len()
            );
            let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
            sock.write_all(request.as_bytes()).await.unwrap();
            let pid = wait_for_pid(&pidfile).await;
            assert_eq!(state.heavy.available_permits(), 0);

            drop(sock);
            let took = wait_reaped(pid, Duration::from_secs(3)).await;
            wait_permits(&state, 1).await;
            eprintln!("worker reaped {took:?} after the client hung up");
            server.abort();
        }

        async fn http_get(addr: SocketAddr, path: &str) -> String {
            use tokio::io::AsyncReadExt;
            let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
            let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", addr.port());
            sock.write_all(req.as_bytes()).await.unwrap();
            let mut out = String::new();
            sock.read_to_string(&mut out).await.unwrap();
            out
        }

        fn slow_probe() -> crate::translate::Status {
            std::thread::sleep(Duration::from_secs(3));
            crate::translate::Status { installed: true, logged_in: false }
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn the_server_answers_before_a_slow_translate_probe_finishes() {
            let dir = tempfile::tempdir().unwrap();
            let mut config = Config::from_env(0).unwrap();
            config.db_path = dir.path().join("test.db");
            config.enable_translate = true;
            let state = AppState::with_translate_probe(config, slow_probe).unwrap();
            let listener = bind_test_port().await;
            let addr = listener.local_addr().unwrap();
            let t = Instant::now();
            let server = tokio::spawn(serve_on(listener, state));
            let reply = bounded(http_get(addr, "/healthz")).await;
            assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
            assert!(t.elapsed() < Duration::from_secs(1), "healthz waited {:?} for the probe", t.elapsed());
            server.abort();
        }

        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        async fn a_client_that_never_finishes_its_headers_is_dropped() {
            use tokio::io::AsyncReadExt;
            let (state, _dir) = state_with(|c| c.header_timeout = Duration::from_secs(1));
            let listener = bind_test_port().await;
            let addr = listener.local_addr().unwrap();
            let server = tokio::spawn(serve_on(listener, state));

            let status = bounded(http_get(addr, "/api/status")).await;
            assert!(status.starts_with("HTTP/1.1 200"), "the peer address reaches the app: {status}");

            let mut sock = tokio::net::TcpStream::connect(addr).await.unwrap();
            sock.write_all(b"POST /api/solve HTTP/1.1\r\nHost: 127.0.0.1\r\n").await.unwrap();
            let t = Instant::now();
            let mut buf = [0u8; 256];
            let closed = bounded(async {
                loop {
                    tokio::select! {
                        n = sock.read(&mut buf) => break n.map_or(true, |n| n == 0 || buf[..n].starts_with(b"HTTP/1.1 408")),
                        _ = tokio::time::sleep(Duration::from_millis(300)) => {
                            if sock.write_all(b"X-Slow: 1\r\n").await.is_err() {
                                break true;
                            }
                        }
                    }
                }
            })
            .await;
            assert!(closed);
            assert!(t.elapsed() < Duration::from_secs(3), "dropped after {:?}", t.elapsed());
            server.abort();
        }
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
    async fn a_swapped_in_shorter_proof_replaces_the_history_entry() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "euler").await;
        let (_, _, first) =
            call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ORTHO_REFLECTION})).await;
        let hid = first["history_id"].as_i64().expect("a recorded solve says which row it wrote");
        let (_, _, best) = call(
            &state,
            "POST",
            "/api/solve",
            Some(&cookie),
            serde_json::json!({"input": ORTHO_REFLECTION, "best": true, "budget_secs": 2, "record": false}),
        )
        .await;
        assert!(best["history_id"].is_null());
        let body = serde_json::json!({"id": best["id"]});
        let other = register_cookie(&state, "lagrange").await;
        let (st, _, _) = call(&state, "PUT", &format!("/api/history/{hid}"), Some(&other), body.clone()).await;
        assert_eq!(st, StatusCode::NOT_FOUND, "only the owner can replace a row");
        let (st, _, _) = call(&state, "PUT", &format!("/api/history/{hid}"), Some(&cookie), body).await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        let (_, _, again) =
            call(&state, "GET", &format!("/api/history/{hid}"), Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(again["view"], best["view"]);
        assert_eq!(again["examined"], best["examined"]);
        let (_, _, rows) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(rows.as_array().map(Vec::len), Some(1));
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
    async fn dividing_by_zero_is_an_input_error_not_a_verdict() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "zero").await;
        let src = "A B C = triangle\nprove dist(A,B) = 1/0";
        let (st, _, body) = call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": src})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["diagnosis"]["key"], "div_zero", "{body}");
        assert_eq!(body["error"], "Division by zero in the goal.");
        assert!(division_by_zero("A B C = triangle\nprove dist(A,B) / 0.5 = 2").is_none());
        assert!(division_by_zero("A B C = triangle\nprove dist(A,B) = 1/ 0.0").is_some());
    }

    #[tokio::test]
    async fn metric_typos_are_compile_errors_in_the_users_words() {
        let (state, _dir) = test_state();
        let cookie = register_cookie(&state, "typo").await;
        let arity = "A B C = triangle\nprove dist(B,C) = sin(angle(A,B))";
        let (st, _, body) = call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": arity})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(body["diagnosis"]["key"], "arity", "{body}");
        assert_eq!(body["diagnosis"]["line"], 2, "{body}");
        assert_eq!(body["error"], "“angle” takes 3 arguments, but 2 were given.", "{body}");
        let ops = "A B C = triangle\nprove dist(B,C) = 2**dist(A,B)";
        let (st, _, body) = call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": ops})).await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        let msg = body["error"].as_str().unwrap();
        assert!(!msg.contains("Op(") && msg.contains("*"), "{body}");
        assert_eq!((body["diagnosis"]["line"].as_u64(), body["diagnosis"]["col"].as_u64()), (Some(2), Some(21)), "{body}");
        let (_, _, hist) = call(&state, "GET", "/api/history", Some(&cookie), serde_json::Value::Null).await;
        assert_eq!(hist.as_array().map(Vec::len), Some(0), "typos are not history: {hist}");
        let degenerate = "A B C = triangle\nM = midpoint(A, A)\nprove coll(A, B, M)";
        let (_, _, body) = call(&state, "POST", "/api/solve", Some(&cookie), serde_json::json!({"input": degenerate})).await;
        assert!(body["detail"].as_str().is_none_or(|d| !d.contains("no attempt")), "{body}");
    }

    #[test]
    fn long_titles_are_cut_at_a_word_with_an_ellipsis() {
        let long = "In every non-degenerate triangle consider the reflection of the orthocentre in each side; ".repeat(4);
        let t = clean_title(Some(long)).unwrap();
        assert!(t.chars().count() <= MAX_TITLE_CHARS, "{t}");
        assert!(t.ends_with('\u{2026}'), "{t}");
        let word = t.trim_end_matches('\u{2026}').rsplit(' ').next().unwrap();
        assert!(["In", "every", "non-degenerate", "triangle", "consider", "the", "reflection", "of", "orthocentre", "in", "each", "side"].contains(&word), "{t}");
        assert_eq!(clean_title(Some("  Euler line ".into())).as_deref(), Some("Euler line"));
        assert_eq!(clean_title(Some("   ".into())), None);
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
        let (state, _dir) = ai_guest_state(false, true);
        let auth = basic("", GATE_PW);
        let (st, _, text) = send(&state, build("GET", "/api/status", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        let body: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["guest"], true);
        assert_eq!(body["signed_in"], false);
        assert_eq!(body["can_translate"], false);
        assert!(body["translate_block"].is_string(), "{body}");
        assert!(body.get("gate").is_none(), "Basic, not the cookie: {body}");
    }

    #[test]
    fn one_caller_cannot_evict_anothers_cached_results() {
        let mut c = SolutionCache { entries: Vec::new() };
        let mut push = |owner: &str, id: String| {
            c.evict_for(owner, SOLUTION_CACHE_PER_OWNER, SOLUTION_CACHE_MAX);
            c.entries.push(CachedSolution {
                id,
                owner: owner.to_string(),
                at: std::time::Instant::now(),
                value: std::sync::Arc::new(serde_json::json!({})),
            });
        };
        push("u1", "mine".into());
        for i in 0..1000 {
            push("u2", format!("flood{i}"));
        }
        for k in 0..20 {
            for i in 0..30 {
                push(&format!("g{k}"), format!("g{k}-{i}"));
            }
        }
        assert!(c.entries.iter().any(|e| e.id == "mine"), "a flood of other callers' solves evicted u1's result");
        assert!(c.entries.iter().filter(|e| e.owner == "u2").count() <= SOLUTION_CACHE_PER_OWNER);
        assert!(c.entries.len() <= SOLUTION_CACHE_MAX);
    }

    #[test]
    fn a_goal_joining_two_relations_is_not_called_degenerate() {
        assert_eq!(degenerate_goal("A B C = triangle\nM = midpoint(A, B)\nprove coll(A, M, B) \u{2227} coll(A, B, M)"), None);
        assert_eq!(degenerate_goal("A B C = triangle\nprove cyclic(A, B, C, A)").as_deref(), Some("A"));
    }

    // ------------------------------------------------------------ gate page --

    const GATE_PW: &str = "correct-horse-9";

    fn gate_state(f: impl FnOnce(&mut Config)) -> (Shared, tempfile::TempDir) {
        state_with(|c| {
            c.basic_auth = Some(security::BasicAuth::parse(&format!(":{GATE_PW}")).unwrap());
            c.guest_mode = true;
            c.guest_ai = true;
            c.gate = security::GateMode::Form;
            f(c);
        })
    }

    fn set_cookies(h: &HeaderMap) -> Vec<String> {
        h.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_string()).collect()
    }

    fn gate_set_cookie(h: &HeaderMap) -> Option<String> {
        set_cookies(h).into_iter().find(|c| c.starts_with("gate=") || c.starts_with("__Host-gate="))
    }

    fn form_body(pairs: &[(&str, &str)]) -> String {
        pairs
            .iter()
            .map(|(k, v)| format!("{k}={}", gate::percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn post_gate(pairs: &[(&str, &str)], extra: &[(&str, &str)]) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/gate")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        for (k, v) in extra {
            b = b.header(*k, *v);
        }
        b.body(Body::from(form_body(pairs))).unwrap()
    }

    async fn gate_cookie(state: &Shared) -> String {
        let (st, h, _) = send(state, post_gate(&[("password", GATE_PW), ("next", "/")], &[])).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        cookie_of(&gate_set_cookie(&h).expect("the gate sets its cookie"))
    }

    const NAV: (&str, &str) = ("sec-fetch-mode", "navigate");

    #[tokio::test]
    async fn page_loads_without_the_password_go_to_the_gate() {
        let (state, _dir) = gate_state(|_| {});
        let (st, h, _) = send(&state, build("GET", "/", &[NAV], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(h[header::LOCATION], "/gate?next=%2F");
        assert_eq!(h[header::CACHE_CONTROL], "no-store");
        assert!(!h.contains_key(header::WWW_AUTHENTICATE));
        let (st, h, _) = send(&state, build("GET", "/app?x=1", &[("accept", "text/html,application/xhtml+xml")], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "Accept: text/html without Sec-Fetch is a navigation");
        assert_eq!(h[header::LOCATION], "/gate?next=%2Fapp%3Fx%3D1");
        let (st, h, body) = send(&state, build("GET", "/gate?next=%2Fapp", &[NAV], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(
            h[header::REFERRER_POLICY], "same-origin",
            "under no-referrer WebKit posts the form with `Origin: null`, which same_origin refuses"
        );
        assert_eq!(h["x-robots-tag"], "noindex");
        assert!(body.contains("action=\"/gate\""));
        assert!(body.contains("name=\"next\" value=\"/app\""));
    }

    #[tokio::test]
    async fn api_calls_without_the_password_get_json_and_curl_keeps_its_challenge() {
        let (state, _dir) = gate_state(|_| {});
        let (st, h, _) = send(&state, build("GET", "/api/status", &[], None)).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(h[header::WWW_AUTHENTICATE].to_str().unwrap().starts_with("Basic realm=\"GeoSolver\""));
        let (st, h, body) = send(&state, build("GET", "/api/status", &[("sec-fetch-mode", "cors")], None)).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(!h.contains_key(header::WWW_AUTHENTICATE), "a browser fetch must not pop the Basic sheet");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["code"], "gate");
        assert_eq!(v["error"], i18n::t(i18n::Lang::En, "gate.required"));
        let (_, _, body) = send(
            &state,
            build("POST", "/api/solve", &[("sec-fetch-mode", "cors"), ("accept-language", "ro-RO,ro;q=0.9")], Some("{}")),
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["error"], i18n::t(i18n::Lang::Ro, "gate.required"));
        let (st, h, _) = send(&state, build("GET", "/assets/app.js", &[("sec-fetch-mode", "no-cors")], None)).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(!h.contains_key(header::WWW_AUTHENTICATE));
    }

    #[tokio::test]
    async fn public_paths_are_open_and_their_siblings_are_not() {
        let (state, _dir) = gate_state(|_| {});
        for p in security::PUBLIC_PATHS {
            let path = match *p {
                "/icons/{file}" => "/icons/icon-192.png",
                "/splash/{file}" => "/splash/750x1334-light.png",
                "/assets/fonts/{file}" => "/assets/fonts/inter.woff2",
                p => p,
            };
            let (st, _, _) = call_raw(&state, build("GET", path, &[], None)).await;
            if path == "/gate/forget" {
                assert_eq!(st, StatusCode::METHOD_NOT_ALLOWED, "{path}: POST only, but past the gate");
            } else {
                assert_eq!(st, StatusCode::OK, "{path}");
            }
            let (st, _, _) = call_raw(&state, build("HEAD", path, &[], None)).await;
            assert_ne!(st, StatusCode::UNAUTHORIZED, "HEAD {path}");
        }
        let (st, _, _) = call_raw(&state, build("GET", "/icons/nope.png", &[], None)).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        let (st, _, _) = call_raw(&state, build("GET", "/splash/1x1-light.png", &[], None)).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
        for gated in [
            "/", "/app", "/auth", "/assets/app.js", "/assets/site.js", "/assets/i18n.js", "/assets/auth.js",
            "/assets/landing.js", "/assets/showcase.json", "/api/status", "/api/history", "/icons/a/b.png",
            "/assets/fonts/x/y.woff2",
        ] {
            let (st, _, _) = call_raw(&state, build("GET", gated, &[], None)).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "{gated}");
            let (st, _, _) = call_raw(&state, build("GET", gated, &[NAV], None)).await;
            assert_eq!(st, StatusCode::SEE_OTHER, "{gated}");
        }
        let (st, _, _) = call_raw(&state, build("POST", "/manifest.webmanifest", &[], Some("{}"))).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED, "only GET/HEAD are public");
    }

    #[tokio::test]
    async fn the_right_password_sets_the_cookie_and_the_cookie_opens_the_app() {
        let (state, _dir) = gate_state(|_| {});
        let (st, h, _) = send(&state, post_gate(&[("password", GATE_PW), ("next", "/app?x=1"), ("username", "GeoSolver")], &[])).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(h[header::LOCATION], "/app?x=1");
        assert_eq!(h[header::CACHE_CONTROL], "no-store");
        let set = gate_set_cookie(&h).unwrap();
        assert!(set.starts_with("gate=v1."), "{set}");
        for attr in ["HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=15552000"] {
            assert!(set.contains(attr), "{attr} missing in {set}");
        }
        assert!(!set.contains("Secure"));
        assert!(set_cookies(&h).iter().any(|c| c.starts_with("gid=")), "guests get their id at once");
        let cookie = cookie_of(&set);
        let (st, _, page) = send(&state, build("GET", "/app", &[NAV, ("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(page.contains("id=\"solve\""));
        let (st, _, body) = send(&state, build("GET", "/api/status", &[("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["gate"], "cookie");
        let (st, h, _) = send(&state, build("GET", "/gate?next=%2Fapp", &[NAV, ("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "already past the gate");
        assert_eq!(h[header::LOCATION], "/app");
        let twice = format!("{cookie}; {cookie}");
        let (st, _, _) = send(&state, build("GET", "/app", &[NAV, ("cookie", &twice)], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "a duplicated cookie is no cookie");

        let (secure, _d2) = gate_state(|c| c.secure_cookies = true);
        let (_, h, _) = send(&secure, post_gate(&[("password", GATE_PW)], &[])).await;
        let set = gate_set_cookie(&h).unwrap();
        assert!(set.starts_with("__Host-gate=v1."), "{set}");
        for attr in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/", "Max-Age=15552000"] {
            assert!(set.contains(attr), "{attr} missing in {set}");
        }
        assert!(!set.contains("Domain"));
        let (st, _, _) = send(&secure, build("GET", "/api/status", &[("cookie", &cookie_of(&set))], None)).await;
        assert_eq!(st, StatusCode::OK);
    }

    #[tokio::test]
    async fn wrong_passwords_are_rate_limited_together_with_basic_failures() {
        let (state, _dir) = gate_state(|c| c.basic_auth_fails_per_min = 10);
        let (st, h, body) = send(&state, post_gate(&[("password", "nope-nope-nope"), ("next", "/app")], &[])).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert!(!h.contains_key(header::WWW_AUTHENTICATE));
        assert!(gate_set_cookie(&h).is_none());
        assert!(body.contains(&gate::html_escape(i18n::t(i18n::Lang::En, "gate.wrong"))));
        assert!(body.contains("aria-invalid=\"true\""));
        assert!(body.contains("name=\"next\" value=\"/app\""), "the return path survives a wrong try");
        let (st, _, body) = send(&state, post_gate(&[("password", ""), ("next", "/")], &[])).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert!(body.contains(&gate::html_escape(i18n::t(i18n::Lang::En, "gate.empty"))));
        for _ in 1..10 {
            send(&state, post_gate(&[("password", "nope-nope-nope")], &[])).await;
        }
        let (st, h, body) = send(&state, post_gate(&[("password", GATE_PW)], &[])).await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "the right password is not even checked");
        assert_eq!(h[header::RETRY_AFTER], "60");
        assert!(gate_set_cookie(&h).is_none());
        assert!(body.contains(&gate::html_escape(i18n::t(i18n::Lang::En, "gate.rate"))));

        let (state, _dir) = gate_state(|c| c.basic_auth_fails_per_min = 10);
        let wrong = basic("x", "nope-nope-nope");
        for _ in 0..5 {
            let (st, _, _) = send(&state, build("GET", "/api/status", &[("authorization", &wrong)], None)).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED);
        }
        for _ in 0..5 {
            let (st, _, _) = send(&state, post_gate(&[("password", "nope-nope-nope")], &[])).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED);
        }
        let (st, _, _) = send(&state, post_gate(&[("password", GATE_PW)], &[])).await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS, "Basic and form failures share one budget");
        let right = basic("x", GATE_PW);
        let (st, _, _) = send(&state, build("GET", "/api/status", &[("authorization", &right)], None)).await;
        assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn next_is_sanitized_before_the_redirect() {
        let (state, _dir) = gate_state(|c| c.basic_auth_fails_per_min = 0);
        for (next, want) in [
            ("/app?x=1", "/app?x=1"),
            ("//evil.com", "/"),
            ("/\\evil.com", "/"),
            ("https://evil.com", "/"),
            ("/gate", "/"),
            ("/app\r\nSet-Cookie: a=b", "/"),
        ] {
            let (st, h, _) = send(&state, post_gate(&[("password", GATE_PW), ("next", next)], &[])).await;
            assert_eq!(st, StatusCode::SEE_OTHER, "{next:?}");
            assert_eq!(h[header::LOCATION], want, "{next:?}");
        }
        let req = Request::builder()
            .method("POST")
            .uri("/gate")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(format!("password={GATE_PW}&next=%2F%2Fevil")))
            .unwrap();
        let (_, h, _) = send(&state, req).await;
        assert_eq!(h[header::LOCATION], "/");
        let cookie = gate_cookie(&state).await;
        let (_, h, _) = send(&state, build("GET", "/gate?next=%2F%2Fevil.com", &[("cookie", &cookie)], None)).await;
        assert_eq!(h[header::LOCATION], "/");
        let (_, h, _) = send(&state, build("GET", "/gate?next=%2Fapp%3Fq%3D%C4%83", &[("cookie", &cookie)], None)).await;
        assert_eq!(h[header::LOCATION], "/app?q=%C4%83", "non-ASCII is escaped, not dropped");
    }

    #[tokio::test]
    async fn the_gate_form_refuses_a_foreign_origin() {
        let (state, _dir) = gate_state(|_| {});
        let (st, h, _) = send(
            &state,
            post_gate(&[("password", GATE_PW)], &[("host", "localhost:8787"), ("origin", "https://evil.example")]),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert!(gate_set_cookie(&h).is_none());
        let (st, _, _) = send(
            &state,
            post_gate(&[("password", GATE_PW)], &[("host", "localhost:8787"), ("origin", "http://localhost:8787")]),
        )
        .await;
        assert_eq!(st, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn the_gate_speaks_the_visitors_language() {
        let (state, _dir) = gate_state(|_| {});
        let ro_title = gate::html_escape(i18n::t(i18n::Lang::Ro, "gate.title"));
        let en_title = gate::html_escape(i18n::t(i18n::Lang::En, "gate.title"));
        let (_, _, body) = send(&state, build("GET", "/gate", &[("accept-language", "ro-RO,ro;q=0.9,en;q=0.8")], None)).await;
        assert!(body.contains(&ro_title));
        assert!(body.contains("<html lang=\"ro\">"));
        let (_, _, body) =
            send(&state, build("GET", "/gate", &[("accept-language", "ro-RO"), ("cookie", "lang=en")], None)).await;
        assert!(body.contains(&en_title), "a chosen language wins over the browser's");
        let (_, h, body) = send(&state, build("GET", "/gate?lang=ro&next=%2Fapp", &[("cookie", "lang=en")], None)).await;
        assert!(body.contains(&ro_title));
        let lang = set_cookies(&h).into_iter().find(|c| c.starts_with("lang=")).unwrap();
        assert!(lang.starts_with("lang=ro;"), "{lang}");
        assert!(lang.contains("Max-Age=31536000") && lang.contains("SameSite=Lax") && lang.contains("Path=/"));
        assert!(!lang.contains("HttpOnly"), "i18n.js reads it");
        assert!(body.contains("href=\"/gate?lang=en&amp;next=%2Fapp\""));
        assert!(body.contains("hreflang=\"ro\" lang=\"ro\" aria-current=\"true\""));
        let (_, h, body) = send(&state, post_gate(&[("password", "x-wrong-x")], &[("accept-language", "ro")])).await;
        assert!(body.contains(&gate::html_escape(i18n::t(i18n::Lang::Ro, "gate.wrong"))));
        assert!(gate_set_cookie(&h).is_none());
    }

    #[tokio::test]
    async fn gate_basic_keeps_the_old_challenge_everywhere() {
        let (state, _dir) = gate_state(|c| c.gate = security::GateMode::Basic);
        for path in ["/", "/manifest.webmanifest", "/gate", "/apple-touch-icon.png"] {
            let (st, h, _) = send(&state, build("GET", path, &[NAV], None)).await;
            assert_eq!(st, StatusCode::UNAUTHORIZED, "{path}");
            assert!(h.contains_key(header::WWW_AUTHENTICATE), "{path}");
        }
        let (st, _, _) = send(&state, build("GET", "/healthz", &[], None)).await;
        assert_eq!(st, StatusCode::OK);
        let auth = basic("x", GATE_PW);
        let (st, h, _) = send(&state, build("GET", "/", &[NAV, ("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(gate_set_cookie(&h).is_none(), "no cookie under the Basic-only gate");
        let (st, _, _) = send(&state, post_gate(&[("password", GATE_PW)], &[("authorization", &auth)])).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn basic_auth_still_works_and_moves_browsers_onto_the_cookie() {
        let (state, _dir) = gate_state(|_| {});
        let auth = basic("anyone", GATE_PW);
        let (st, h, _) = send(&state, build("GET", "/api/status", &[("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(gate_set_cookie(&h).is_none(), "API clients get no cookie");
        let (st, h, _) = send(&state, build("GET", "/", &[NAV, ("authorization", &auth)], None)).await;
        assert_eq!(st, StatusCode::OK);
        let cookie = cookie_of(&gate_set_cookie(&h).expect("a Basic page load is given the gate cookie"));
        let (st, _, _) = send(&state, build("GET", "/api/status", &[("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::OK);
    }

    #[tokio::test]
    async fn forgetting_the_device_clears_the_cookie() {
        let (state, _dir) = gate_state(|_| {});
        let cookie = gate_cookie(&state).await;
        let (st, h, _) = send(&state, build("POST", "/gate/forget", &[("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER);
        assert_eq!(h[header::LOCATION], "/gate");
        let cleared = gate_set_cookie(&h).unwrap();
        assert!(cleared.starts_with("gate=;") && cleared.contains("Max-Age=0") && cleared.contains("Path=/"), "{cleared}");
    }

    #[tokio::test]
    async fn the_gate_key_persists_across_restarts_and_rotation_revokes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("gate.db");
        let mk = |pw: &str, key: Option<[u8; 32]>| {
            let mut c = Config::from_env(0).unwrap();
            c.db_path = db.clone();
            c.basic_auth = Some(security::BasicAuth::parse(pw).unwrap());
            c.guest_mode = true;
            c.gate = security::GateMode::Form;
            c.gate_key = key;
            AppState::new(c).unwrap()
        };
        let first = mk(&format!(":{GATE_PW}"), None);
        let cookie = gate_cookie(&first).await;
        let second = mk(&format!(":{GATE_PW}"), None);
        assert_eq!(first.gate_key, second.gate_key, "the generated key is stored, not per process");
        let (st, _, _) = send(&second, build("GET", "/app", &[NAV, ("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::OK, "a restart keeps every device signed in");
        let rotated_pw = mk(":a-new-password", None);
        let (st, h, _) = send(&rotated_pw, build("GET", "/app", &[NAV, ("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "a new password revokes old cookies");
        assert!(h[header::LOCATION].to_str().unwrap().starts_with("/gate?next="));
        let rotated_key = mk(&format!(":{GATE_PW}"), Some([9u8; 32]));
        assert_eq!(rotated_key.gate_key, [9u8; 32]);
        let (st, _, _) = send(&rotated_key, build("GET", "/app", &[NAV, ("cookie", &cookie)], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "a new AGSTUDIO_GATE_KEY revokes old cookies");
    }

    #[tokio::test]
    async fn old_cookies_are_renewed_on_page_loads() {
        let (state, _dir) = gate_state(|_| {});
        let cred = state.config.basic_auth.as_ref().unwrap().cred_digest();
        let at = |days: u64| format!("gate={}", gate::mint(&state.gate_key, &cred, gate::now_secs() - days * 86_400));
        let (st, h, _) = send(&state, build("GET", "/app", &[NAV, ("cookie", &at(8))], None)).await;
        assert_eq!(st, StatusCode::OK);
        let fresh = gate_set_cookie(&h).expect("an 8-day-old cookie is renewed");
        let iat: u64 = fresh.split('.').nth(1).unwrap().parse().unwrap();
        assert!(gate::now_secs() - iat < 60);
        let (st, h, _) = send(&state, build("GET", "/app", &[NAV, ("cookie", &at(1))], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert!(gate_set_cookie(&h).is_none(), "a 1-day-old cookie is left alone");
        let (_, h, _) = send(&state, build("GET", "/api/status", &[("cookie", &at(8))], None)).await;
        assert!(gate_set_cookie(&h).is_none(), "only page loads renew");
        let (st, _, _) = send(&state, build("GET", "/app", &[NAV, ("cookie", &at(181))], None)).await;
        assert_eq!(st, StatusCode::SEE_OTHER, "past AGSTUDIO_GATE_DAYS");
    }

    // ------------------------------------------------------ home screen ----

    #[tokio::test]
    async fn manifest_and_icons_are_served_with_their_types() {
        let (state, _dir) = gate_state(|_| {});
        let (st, h, body) = call_raw(&state, build("GET", "/manifest.webmanifest", &[], None)).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(h[header::CONTENT_TYPE], "application/manifest+json");
        assert_eq!(h[header::CACHE_CONTROL], "public, max-age=86400");
        let m: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(m["display"], "standalone");
        assert_eq!(m["start_url"], "/");
        for icon in m["icons"].as_array().unwrap() {
            let src = icon["src"].as_str().unwrap();
            let (st, h, body) = call_raw(&state, build("GET", src, &[], None)).await;
            assert_eq!(st, StatusCode::OK, "{src}");
            assert_eq!(h[header::CONTENT_TYPE], icon["type"].as_str().unwrap(), "{src}");
            assert_eq!(h[header::CACHE_CONTROL], "public, max-age=604800", "{src}");
            if icon["type"] == "image/png" {
                let (w, hgt) = (u32::from_be_bytes(body[16..20].try_into().unwrap()), u32::from_be_bytes(body[20..24].try_into().unwrap()));
                assert_eq!(icon["sizes"], format!("{w}x{hgt}"), "{src}");
            }
        }
        for touch in ["/apple-touch-icon.png", "/apple-touch-icon-precomposed.png"] {
            let (st, h, body) = call_raw(&state, build("GET", touch, &[], None)).await;
            assert_eq!(st, StatusCode::OK);
            assert_eq!(h[header::CONTENT_TYPE], "image/png");
            assert_eq!(&body[16..24], &[0, 0, 0, 180, 0, 0, 0, 180]);
            assert_eq!(body[25], 2, "RGB: iOS fills transparent pixels with black");
        }
        let (_, h, _) = call_raw(&state, build("GET", "/favicon.ico", &[], None)).await;
        assert_eq!(h[header::CONTENT_TYPE], "image/x-icon");
        let (_, h, body) = call_raw(&state, build("GET", "/robots.txt", &[], None)).await;
        assert!(h[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/plain"));
        assert_eq!(std::str::from_utf8(&body).unwrap(), "User-agent: *\nDisallow: /\n");
    }

    #[test]
    fn every_page_carries_the_home_screen_head() {
        let gate_page = {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let (state, _dir) = gate_state(|_| {});
            rt.block_on(send(&state, build("GET", "/gate", &[], None))).2
        };
        for (name, html) in [
            ("index", index_html()),
            ("auth", auth_html()),
            ("landing", landing_html()),
            ("gate", gate_page.as_str()),
        ] {
            for needle in [
                "<link rel=\"manifest\" href=\"/manifest.webmanifest\">",
                "<link rel=\"icon\" href=\"/favicon.svg\" type=\"image/svg+xml\">",
                "<link rel=\"apple-touch-icon\" href=\"/apple-touch-icon.png\">",
                "<meta name=\"apple-mobile-web-app-title\" content=\"GeoSolver\">",
                "<meta name=\"apple-mobile-web-app-capable\" content=\"yes\">",
                "<meta name=\"mobile-web-app-capable\" content=\"yes\">",
                "<meta name=\"apple-mobile-web-app-status-bar-style\" content=\"default\">",
                "<meta name=\"theme-color\" media=\"(prefers-color-scheme: light)\" content=\"#f7f7f5\">",
                "<meta name=\"theme-color\" media=\"(prefers-color-scheme: dark)\" content=\"#121417\">",
            ] {
                assert!(html.contains(needle), "{name} lacks {needle}");
            }
            assert_eq!(html.matches("rel=\"apple-touch-startup-image\"").count(), 16, "{name}");
            assert!(!html.contains("<!--pwa-splash-->"), "{name}");
            assert!(!html.contains("data:image/svg+xml"), "{name} still has the data: favicon");
            assert_eq!(html.matches("name=\"theme-color\"").count(), 2, "{name}");
        }
    }

    // ----------------------------------------------------------- guest AI ----

    fn logged_in_probe() -> translate::Status {
        translate::Status { installed: true, logged_in: true }
    }

    fn ai_guest_state(guest_ai: bool, enable_translate: bool) -> (Shared, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut c = Config::from_env(0).unwrap();
        c.db_path = dir.path().join("test.db");
        c.basic_auth = Some(security::BasicAuth::parse(&format!(":{GATE_PW}")).unwrap());
        c.guest_mode = true;
        c.guest_ai = guest_ai;
        c.enable_translate = enable_translate;
        (AppState::with_translate_probe(c, logged_in_probe).unwrap(), dir)
    }

    async fn settled_status(state: &Shared, cookie: &str) -> serde_json::Value {
        let mut body = serde_json::Value::Null;
        for _ in 0..40 {
            let (_, _, text) = send(state, build("GET", "/api/status", &[("cookie", cookie)], None)).await;
            body = serde_json::from_str(&text).unwrap();
            if body["translate_checking"] == false {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        body
    }

    #[tokio::test]
    async fn guests_past_the_password_can_use_describe_and_photo() {
        let (state, _dir) = ai_guest_state(true, true);
        let cookie = gate_cookie(&state).await;
        let body = settled_status(&state, &cookie).await;
        assert_eq!(body["guest"], true, "{body}");
        assert_eq!(body["translate_logged_in"], true, "{body}");
        assert_eq!(body["can_translate"], true, "{body}");
        assert!(body.get("translate_block").is_none(), "{body}");

        let (state, _dir) = ai_guest_state(false, true);
        let cookie = gate_cookie(&state).await;
        let body = settled_status(&state, &cookie).await;
        assert_eq!(body["can_translate"], false, "{body}");
        assert_eq!(body["translate_block"], "sign_in", "AGSTUDIO_GUEST_AI=0 keeps it for accounts");
    }

    #[tokio::test]
    async fn guest_translate_requests_get_past_the_account_check() {
        let (state, _dir) = ai_guest_state(true, false);
        let cookie = gate_cookie(&state).await;
        let (st, _, body) = send(&state, build("POST", "/api/translate", &[("cookie", &cookie)], Some("{\"text\":\"x\"}"))).await;
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE, "reached the handler's availability check: {body}");
        let (state, _dir) = ai_guest_state(false, false);
        let cookie = gate_cookie(&state).await;
        let (st, _, body) = send(&state, build("POST", "/api/translate", &[("cookie", &cookie), ("accept-language", "ro")], Some("{\"text\":\"x\"}"))).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["code"], "sign_in");
        assert_eq!(v["error"], i18n::t(i18n::Lang::Ro, "auth.sign_in_ai"));
    }

    #[test]
    fn the_guest_daily_translation_cap_counts_per_address() {
        let (state, _dir) = ai_guest_state(true, false);
        let a: std::net::IpAddr = "203.0.113.5".parse().unwrap();
        for _ in 0..100 {
            assert!(state.guest_translation_allowed(a), "0 = no daily cap");
        }
        let (state, _dir) = state_with(|c| c.guest_ai_per_day = 2);
        let b: std::net::IpAddr = "203.0.113.6".parse().unwrap();
        assert!(state.guest_translation_allowed(a));
        assert!(state.guest_translation_allowed(a));
        assert!(!state.guest_translation_allowed(a));
        assert!(state.guest_translation_allowed(b));
    }
}
