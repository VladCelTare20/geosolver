//! Security configuration and middleware for the `serve` web app.
//!
//! All controls are configured from environment variables so the same binary is
//! safe to run locally *and* to expose (behind a reverse proxy) on a public
//! Linux server:
//!
//! | Env var | Default | Effect |
//! |---|---|---|
//! | `AGSTUDIO_BIND` | `127.0.0.1:<port>` | interface/port to bind |
//! | `AGSTUDIO_BASIC_AUTH` | (none) | `user:pass` — require HTTP Basic auth |
//! | `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
//! | `AGSTUDIO_MAX_CONCURRENT` | ~CPUs | simultaneous heavy requests (excess → 503) |
//! | `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests per minute (0 = off) |
//! | `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP `/api/translate` + `/api/explain` per minute (0 = off) |
//! | `AGSTUDIO_AUTH_PER_MIN` | 15 | per-IP `/api/auth/login` + `register` per minute (0 = off) |
//! | `AGSTUDIO_MAX_BODY_KB` | 8192 | request body size limit |
//! | `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
//! | `AGSTUDIO_DISABLE_TRANSLATE` | off | turn off the `/api/translate` endpoint |
//! | `AGSTUDIO_TRUST_PROXY` | on | read the client IP from `X-Forwarded-For` |
//! | `AGSTUDIO_DB` | `./agstudio.db` | SQLite file for accounts/sessions/history |
//! | `AGSTUDIO_SECURE_COOKIES` | off | add the `Secure` flag to the session cookie (needs HTTPS) |

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, HeaderName, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine as _;
use tokio::sync::Semaphore;

/// Parsed, validated server configuration.
#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    /// The expected `Authorization` header value (`Basic <b64>`), if auth is on.
    pub basic_auth: Option<String>,
    pub max_concurrent: usize,
    pub rate_per_min: u32,
    pub translate_per_min: u32,
    pub auth_per_min: u32,
    pub max_body_bytes: usize,
    pub max_input_chars: usize,
    pub trust_proxy: bool,
    pub enable_translate: bool,
    pub db_path: PathBuf,
    pub secure_cookies: bool,
}

fn env_u32(k: &str, d: u32) -> u32 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn env_usize(k: &str, d: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}
fn env_flag(k: &str) -> bool {
    matches!(
        std::env::var(k).ok().as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

impl Config {
    /// Build from the environment. Returns an error (refusing to start) if a
    /// non-loopback interface would be exposed without authentication.
    pub fn from_env(default_port: u16) -> Result<Config, String> {
        let bind_str =
            std::env::var("AGSTUDIO_BIND").unwrap_or_else(|_| format!("127.0.0.1:{default_port}"));
        let bind: SocketAddr = bind_str
            .parse()
            .map_err(|_| format!("invalid AGSTUDIO_BIND `{bind_str}` (want e.g. 0.0.0.0:8787)"))?;

        let basic_auth = std::env::var("AGSTUDIO_BASIC_AUTH")
            .ok()
            .filter(|s| s.contains(':') && s.len() >= 3)
            .map(|creds| {
                format!(
                    "Basic {}",
                    base64::engine::general_purpose::STANDARD.encode(creds)
                )
            });

        // Fail closed: never expose a public interface unauthenticated by accident.
        if !bind.ip().is_loopback() && basic_auth.is_none() && !env_flag("AGSTUDIO_ALLOW_INSECURE") {
            return Err(format!(
                "refusing to bind the non-loopback interface {bind} with no authentication.\n  \
                 Set AGSTUDIO_BASIC_AUTH=\"user:pass\" to require a login, OR front the app with a \
                 reverse proxy (nginx) and set AGSTUDIO_ALLOW_INSECURE=1 only if the app port is not \
                 reachable from outside the host."
            ));
        }

        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Ok(Config {
            bind,
            basic_auth,
            max_concurrent: env_usize("AGSTUDIO_MAX_CONCURRENT", cpus.clamp(2, 8)),
            rate_per_min: env_u32("AGSTUDIO_RATE_PER_MIN", 120),
            translate_per_min: env_u32("AGSTUDIO_TRANSLATE_PER_MIN", 12),
            auth_per_min: env_u32("AGSTUDIO_AUTH_PER_MIN", 15),
            max_body_bytes: env_usize("AGSTUDIO_MAX_BODY_KB", 8192).saturating_mul(1024),
            max_input_chars: env_usize("AGSTUDIO_MAX_INPUT_CHARS", 16384),
            trust_proxy: !matches!(
                std::env::var("AGSTUDIO_TRUST_PROXY").ok().as_deref(),
                Some("0") | Some("false") | Some("off")
            ),
            enable_translate: !env_flag("AGSTUDIO_DISABLE_TRANSLATE"),
            db_path: std::env::var("AGSTUDIO_DB")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("./agstudio.db")),
            secure_cookies: env_flag("AGSTUDIO_SECURE_COOKIES"),
        })
    }

    /// A one-line human summary for the startup banner.
    pub fn summary(&self) -> String {
        format!(
            "auth: {} · concurrency: {} · rate: {}/min (translate {}/min) · body ≤ {} KB",
            if self.basic_auth.is_some() {
                "Basic (required)"
            } else {
                "none (open)"
            },
            self.max_concurrent,
            self.rate_per_min,
            self.translate_per_min,
            self.max_body_bytes / 1024,
        )
    }
}

/// Shared server state.
pub struct AppState {
    pub config: Config,
    /// Concurrency gate for the CPU/subprocess-heavy endpoints.
    pub heavy: Arc<Semaphore>,
    pub rate: Mutex<RateLimiter>,
    pub db: crate::db::Db,
    /// Last time expired sessions were swept from the DB (see [`sweep_expired_sessions`]).
    session_sweep: Mutex<Option<Instant>>,
}

pub type Shared = Arc<AppState>;

impl AppState {
    /// Open (and migrate) `config.db_path` and build the shared state. Fails
    /// closed — a broken/unwritable DB path stops `serve` before it binds,
    /// rather than surfacing as opaque 500s on first login.
    pub fn new(config: Config) -> Result<Shared, String> {
        let db = crate::db::open(&config.db_path).map_err(|e| {
            format!(
                "could not open database at {}: {e}",
                config.db_path.display()
            )
        })?;
        Ok(Arc::new(AppState {
            heavy: Arc::new(Semaphore::new(config.max_concurrent)),
            rate: Mutex::new(RateLimiter::default()),
            db,
            config,
            session_sweep: Mutex::new(None),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_state_new_creates_schema_at_db_path() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("agstudio.db");
        let mut config = Config::from_env(0).unwrap();
        config.db_path = db_path.clone();
        let state = AppState::new(config).expect("AppState::new should open and migrate the DB");
        assert!(db_path.exists());
        // Schema is usable immediately (round-trips through the shared handle).
        let conn = state.db.lock().unwrap();
        crate::db::create_user(&conn, "alice", "hash").unwrap();
        assert!(crate::db::find_user_by_name(&conn, "alice").unwrap().is_some());
    }

    #[test]
    fn sweep_expired_sessions_runs_once_then_debounces() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("agstudio.db");
        let state = AppState::new(config).unwrap();
        let row_count = |state: &Shared| -> i64 {
            state
                .db
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
                .unwrap()
        };
        let uid = {
            let conn = state.db.lock().unwrap();
            crate::db::create_user(&conn, "alice", "hash").unwrap()
        };
        {
            let conn = state.db.lock().unwrap();
            crate::db::insert_session(&conn, "dead", uid, -1).unwrap();
        }
        sweep_expired_sessions(&state); // first call is always due -> physically deletes it
        assert_eq!(row_count(&state), 0);

        {
            let conn = state.db.lock().unwrap();
            crate::db::insert_session(&conn, "dead2", uid, -1).unwrap();
        }
        sweep_expired_sessions(&state); // within the 5-minute debounce window -> no-op
        assert_eq!(row_count(&state), 1);
        // Still correctly invisible to callers even though it hasn't been swept yet.
        let conn = state.db.lock().unwrap();
        assert!(crate::db::lookup_session(&conn, "dead2").unwrap().is_none());
    }
}

// ---------------------------------------------------------------- rate limit --

struct Window {
    count: u32,
    start: Instant,
}

/// A simple per-IP fixed-window limiter with three buckets (general, translate,
/// and credential-guessing-sensitive auth), memory bounded by a periodic sweep.
#[derive(Default)]
pub struct RateLimiter {
    general: HashMap<IpAddr, Window>,
    translate: HashMap<IpAddr, Window>,
    auth: HashMap<IpAddr, Window>,
    last_sweep: Option<Instant>,
}

impl RateLimiter {
    fn hit(map: &mut HashMap<IpAddr, Window>, ip: IpAddr, limit: u32, now: Instant) -> bool {
        if limit == 0 {
            return true;
        }
        let w = map.entry(ip).or_insert(Window {
            count: 0,
            start: now,
        });
        if now.duration_since(w.start) >= Duration::from_secs(60) {
            w.count = 0;
            w.start = now;
        }
        w.count = w.count.saturating_add(1);
        w.count <= limit
    }

    fn sweep(&mut self, now: Instant) {
        if self
            .last_sweep
            .map_or(true, |t| now.duration_since(t) > Duration::from_secs(300))
        {
            let cut = Duration::from_secs(60);
            self.general.retain(|_, w| now.duration_since(w.start) < cut);
            self.translate.retain(|_, w| now.duration_since(w.start) < cut);
            self.auth.retain(|_, w| now.duration_since(w.start) < cut);
            self.last_sweep = Some(now);
        }
    }
}

fn client_ip(cfg: &Config, req: &Request<Body>) -> IpAddr {
    if cfg.trust_proxy {
        if let Some(first) = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.split(',').next())
        {
            if let Ok(ip) = first.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
}

// --------------------------------------------------------------- middleware --

/// Require HTTP Basic auth when configured (constant-time comparison).
pub async fn auth(State(state): State<Shared>, req: Request<Body>, next: Next) -> Response {
    // The health probe is always reachable (no secret, no side effects).
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }
    if let Some(expected) = &state.config.basic_auth {
        let ok = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|got| ct_eq(got.as_bytes(), expected.as_bytes()));
        if !ok {
            return (
                StatusCode::UNAUTHORIZED,
                [(
                    header::WWW_AUTHENTICATE,
                    "Basic realm=\"GeoSolver\", charset=\"UTF-8\"",
                )],
                "Authentication required.",
            )
                .into_response();
        }
    }
    next.run(req).await
}

/// Delete expired session rows from the DB at most once per 5 minutes,
/// piggybacked on request traffic — same cadence and rationale as
/// `RateLimiter::sweep`, just for the DB-backed session table instead of the
/// in-memory rate-limit maps.
fn sweep_expired_sessions(state: &Shared) {
    let now = Instant::now();
    let due = {
        let mut last = state.session_sweep.lock().unwrap();
        let due = last.map_or(true, |t| now.duration_since(t) > Duration::from_secs(300));
        if due {
            *last = Some(now);
        }
        due
    };
    if due {
        if let Ok(conn) = state.db.lock() {
            let _ = crate::db::sweep_expired_sessions(&conn);
        }
    }
}

/// Per-IP rate limiting for `/api/*` (with a stricter bucket for translate and
/// explain, which both drive the same costly `claude` CLI subprocess).
pub async fn rate_limit(State(state): State<Shared>, req: Request<Body>, next: Next) -> Response {
    sweep_expired_sessions(&state);
    let path = req.uri().path();
    if path.starts_with("/api/") {
        let is_translate = path == "/api/translate" || path == "/api/explain";
        // Login and register get their own tight bucket: they are the two
        // routes an attacker can hammer for credential stuffing / spam signups.
        let is_auth = path == "/api/auth/login" || path == "/api/auth/register";
        let ip = client_ip(&state.config, &req);
        let now = Instant::now();
        let allowed = {
            let mut r = state.rate.lock().unwrap();
            r.sweep(now);
            let general = RateLimiter::hit(&mut r.general, ip, state.config.rate_per_min, now);
            let special = !is_translate
                || RateLimiter::hit(&mut r.translate, ip, state.config.translate_per_min, now);
            let auth_ok =
                !is_auth || RateLimiter::hit(&mut r.auth, ip, state.config.auth_per_min, now);
            general && special && auth_ok
        };
        if !allowed {
            let lang = crate::i18n::lang_from_headers(req.headers());
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "30")],
                Json(serde_json::json!({ "error": crate::i18n::t(lang, "rate.exceeded") })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

/// Cross-origin write protection. The session cookie is `SameSite=Lax`, which
/// already blocks classic CSRF; this adds a second, independent layer — a
/// browser-sent `Origin` on a state-changing request must match the `Host` the
/// request arrived on, or the request is refused. Requests without an `Origin`
/// header (curl, same-origin GETs, MCP) pass through untouched.
pub async fn same_origin(req: Request<Body>, next: Next) -> Response {
    let writes = matches!(
        *req.method(),
        axum::http::Method::POST | axum::http::Method::PUT | axum::http::Method::DELETE
    );
    if writes {
        let origin_host = req
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .filter(|o| *o != "null")
            .map(|o| o.strip_prefix("https://").or_else(|| o.strip_prefix("http://")).unwrap_or(o));
        if let Some(origin_host) = origin_host {
            let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok());
            if host != Some(origin_host) {
                let lang = crate::i18n::lang_from_headers(req.headers());
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": crate::i18n::t(lang, "origin.refused") })),
                )
                    .into_response();
            }
        }
    }
    next.run(req).await
}

/// Add defensive response headers to every response.
pub async fn security_headers(
    State(state): State<Shared>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let is_api = req.uri().path().starts_with("/api/");
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    let set = |h: &mut axum::http::HeaderMap, name: HeaderName, val: &'static str| {
        h.insert(name, HeaderValue::from_static(val));
    };
    set(h, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(h, header::X_FRAME_OPTIONS, "DENY");
    set(h, header::REFERRER_POLICY, "no-referrer");
    set(
        h,
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; \
         base-uri 'none'; form-action 'self'",
    );
    set(
        h,
        HeaderName::from_static("permissions-policy"),
        "geolocation=(), microphone=(), camera=(self)",
    );
    // API responses are personal (history, session state) — never cache them.
    // `/api/export` sets its own stricter `no-store` before this runs.
    if is_api && !h.contains_key(header::CACHE_CONTROL) {
        set(h, header::CACHE_CONTROL, "no-store");
    }
    // Only meaningful when the deployment actually serves HTTPS (the same
    // condition under which cookies are marked `Secure`).
    if state.config.secure_cookies {
        set(
            h,
            header::STRICT_TRANSPORT_SECURITY,
            "max-age=31536000; includeSubDomains",
        );
    }
    resp
}

/// Constant-time byte comparison (avoids auth timing side-channels).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Apply process-wide, server-safe resource limits (idempotent; only sets what
/// the operator has not already chosen). Call once at startup, before solving.
pub fn apply_process_limits() {
    // Bound the default (non-`best`) solve's auxiliary search so a crafted
    // problem cannot peg the CPU indefinitely (the `best` search is already
    // wall-clock bounded).
    if std::env::var_os("AUX_MAX_RUNS").is_none() {
        std::env::set_var("AUX_MAX_RUNS", "200000");
    }
    if std::env::var_os("AUX_MAX_DEPTH").is_none() {
        std::env::set_var("AUX_MAX_DEPTH", "2");
    }
    // Leave a core for the async runtime; caps total rayon parallelism shared
    // across all concurrent solves. Must be set before rayon initialises.
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        let n = std::thread::available_parallelism()
            .map(|c| c.get().saturating_sub(1).max(1))
            .unwrap_or(2);
        std::env::set_var("RAYON_NUM_THREADS", n.to_string());
    }
    // Contain the engine's expected degenerate-candidate panics quietly (they
    // are already caught by `catch_unwind`); avoids per-request hook races.
    std::panic::set_hook(Box::new(|_| {}));
}
