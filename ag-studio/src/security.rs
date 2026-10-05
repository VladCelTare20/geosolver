//! Security configuration and middleware for the `serve` web app.
//!
//! All controls are configured from environment variables so the same binary is
//! safe to run locally *and* to expose (behind a reverse proxy) on a public
//! Linux server:
//!
//! | Env var | Default | Effect |
//! |---|---|---|
//! | `AGSTUDIO_BIND` | `127.0.0.1:<port>` | interface/port to bind |
//! | `AGSTUDIO_BASIC_AUTH` | (none) | require HTTP Basic auth. `user:pass` checks both; `:pass` or a bare `pass` (no colon) accepts **any** username with that password. Set-but-unusable (empty, or a password under 8 chars) refuses to start |
//! | `AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN` | 10 | per-IP *wrong* Basic credentials per minute before 429 (0 = off) |
//! | `AGSTUDIO_GUEST_MODE` | on if `AGSTUDIO_BASIC_AUTH` is set, else off | `1`/`0` override. Visitors past Basic auth may solve, export and humanize without an account (no history). `1` without Basic auth refuses to start |
//! | `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
//! | `AGSTUDIO_MAX_CONCURRENT` | ~CPUs | simultaneous heavy requests; each solve/export is one worker process |
//! | `AGSTUDIO_QUEUE_WAIT_SECS` | 5 | how long a heavy request waits for a free slot before 503 + `Retry-After` (max 60) |
//! | `AGSTUDIO_WORKER_MEM_MB` | 2048 | memory cap (`RLIMIT_DATA`: heap and thread stacks, not merely reserved address space) of each solve worker process; 0 = none |
//! | `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests per minute (0 = off) |
//! | `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP `/api/translate` + `/api/humanize` per minute (0 = off) |
//! | `AGSTUDIO_AUTH_PER_MIN` | 15 | per-IP `/api/auth/login` + `register` per minute (0 = off) |
//! | `AGSTUDIO_MAX_BODY_KB` | 8192 | request body size limit |
//! | `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
//! | `AGSTUDIO_DISABLE_TRANSLATE` | off | turn off `/api/translate` and `/api/humanize` (no `claude` subprocess at all) |
//! | `AGSTUDIO_TRUST_PROXY` | off | client IP = **rightmost** `X-Forwarded-For` entry, honoured only when the TCP peer is loopback/private/CGNAT (i.e. the proxy). Turn on behind `tailscale funnel`/nginx, or every visitor shares one rate-limit bucket |
//! | `AGSTUDIO_PUBLIC_HOST` | (none) | comma list of hostnames the app is served as (e.g. the funnel `*.ts.net` name). With a loopback bind and none set, only `localhost`/`127.0.0.1`/`[::1]` `Host` headers are accepted (DNS-rebinding guard) |
//! | `AGSTUDIO_DB` | `./agstudio.db` | SQLite file for accounts/sessions/history |
//! | `AGSTUDIO_SECURE_COOKIES` | off | session cookie becomes `__Host-sid` + `Secure`, and HSTS is sent (needs HTTPS) |

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
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
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::translate;

/// Simultaneous argon2 hash/verify operations (each costs ~19 MiB and tens of
/// ms of CPU). Login/register beyond this queue rather than pile onto the pool.
pub const ARGON2_PERMITS: usize = 2;

/// Minimum length of the shared Basic-auth password.
const MIN_BASIC_PASSWORD_CHARS: usize = 8;

/// The configured HTTP Basic credentials, kept only as SHA-256 digests so a
/// comparison is constant-time and length-independent.
#[derive(Clone)]
pub struct BasicAuth {
    /// `None` = password-only mode: any username is accepted.
    user: Option<[u8; 32]>,
    pass: [u8; 32],
}

fn sha256(b: &[u8]) -> [u8; 32] {
    Sha256::digest(b).into()
}

impl BasicAuth {
    /// Parse `AGSTUDIO_BASIC_AUTH`: `user:pass`, `:pass`, or a bare `pass`.
    /// The split is at the first `:`, so a password may itself contain colons
    /// only when a username (or the leading `:`) is given.
    pub fn parse(spec: &str) -> Result<BasicAuth, String> {
        let (user, pass) = match spec.split_once(':') {
            Some(("", p)) => (None, p),
            Some((u, p)) => (Some(u), p),
            None => (None, spec),
        };
        if pass.chars().count() < MIN_BASIC_PASSWORD_CHARS {
            return Err(format!(
                "AGSTUDIO_BASIC_AUTH is set but unusable: the password must be at least \
                 {MIN_BASIC_PASSWORD_CHARS} characters (forms: `user:pass`, `:pass`, or `pass`)"
            ));
        }
        Ok(BasicAuth {
            user: user.map(|u| sha256(u.as_bytes())),
            pass: sha256(pass.as_bytes()),
        })
    }

    /// Whether an `Authorization` header value carries these credentials.
    pub fn matches(&self, header_value: &str) -> bool {
        let Some((scheme, b64)) = header_value.trim().split_once(' ') else {
            return false;
        };
        if !scheme.eq_ignore_ascii_case("basic") {
            return false;
        }
        let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(b64.trim()) else {
            return false;
        };
        let (user, pass) = match decoded.iter().position(|&b| b == b':') {
            Some(i) => (&decoded[..i], &decoded[i + 1..]),
            None => (&decoded[..], &[][..]),
        };
        let pass_ok = ct_eq(&sha256(pass), &self.pass);
        let user_ok = self.user.as_ref().is_none_or(|u| ct_eq(&sha256(user), u));
        pass_ok & user_ok
    }
}

/// Parsed, validated server configuration.
#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub basic_auth: Option<BasicAuth>,
    pub basic_auth_fails_per_min: u32,
    /// Visitors past Basic auth may use the solver without an account.
    pub guest_mode: bool,
    pub max_concurrent: usize,
    pub rate_per_min: u32,
    pub translate_per_min: u32,
    pub auth_per_min: u32,
    pub max_body_bytes: usize,
    pub max_input_chars: usize,
    pub trust_proxy: bool,
    /// Lowercase hostnames (no port) the app is publicly served as.
    pub public_hosts: Vec<String>,
    pub enable_translate: bool,
    pub db_path: PathBuf,
    pub secure_cookies: bool,
    /// How long a heavy request waits for a slot before 503 + `Retry-After`.
    pub queue_wait: Duration,
    /// Wall-clock deadline of one web solve or export (its worker is killed
    /// [`crate::worker::GRACE`] later).
    pub solve_deadline: Duration,
    /// How long a client has to send a request's headers before the
    /// connection is closed.
    pub header_timeout: Duration,
}

/// Default web solve deadline, well inside the 120 s request timeout.
pub const SOLVE_DEADLINE: Duration = Duration::from_secs(60);
/// Default [`Config::header_timeout`].
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(30);

fn is_on(v: Option<&str>) -> bool {
    matches!(v, Some("1") | Some("true") | Some("yes") | Some("on"))
}
fn is_off(v: Option<&str>) -> bool {
    matches!(v, Some("0") | Some("false") | Some("no") | Some("off"))
}

impl Config {
    /// Build from the environment. Returns an error (refusing to start) if a
    /// non-loopback interface would be exposed without authentication, or an
    /// auth setting is present but unusable.
    pub fn from_env(default_port: u16) -> Result<Config, String> {
        Config::from_lookup(default_port, |k| std::env::var(k).ok())
    }

    /// [`Config::from_env`] over an arbitrary variable source (tests).
    pub fn from_lookup(
        default_port: u16,
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<Config, String> {
        let num = |k: &str, d: usize| get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let flag = |k: &str| is_on(get(k).as_deref());

        let bind_str = get("AGSTUDIO_BIND").unwrap_or_else(|| format!("127.0.0.1:{default_port}"));
        let bind: SocketAddr = bind_str
            .parse()
            .map_err(|_| format!("invalid AGSTUDIO_BIND `{bind_str}` (want e.g. 0.0.0.0:8787)"))?;

        // Fail closed: a set-but-broken value must never silently mean "no auth".
        let basic_auth = match get("AGSTUDIO_BASIC_AUTH") {
            None => None,
            Some(spec) => Some(BasicAuth::parse(&spec)?),
        };

        if !bind.ip().is_loopback() && basic_auth.is_none() && !flag("AGSTUDIO_ALLOW_INSECURE") {
            return Err(format!(
                "refusing to bind the non-loopback interface {bind} with no authentication.\n  \
                 Set AGSTUDIO_BASIC_AUTH=\"user:pass\" to require a login, OR front the app with a \
                 reverse proxy (nginx) and set AGSTUDIO_ALLOW_INSECURE=1 only if the app port is not \
                 reachable from outside the host."
            ));
        }

        let guest_raw = get("AGSTUDIO_GUEST_MODE");
        let guest_mode = if is_on(guest_raw.as_deref()) {
            true
        } else if is_off(guest_raw.as_deref()) {
            false
        } else {
            basic_auth.is_some()
        };
        if guest_mode && basic_auth.is_none() {
            return Err("AGSTUDIO_GUEST_MODE=1 needs AGSTUDIO_BASIC_AUTH: guest access is \
                        \"link + shared password\", never fully open"
                .into());
        }

        let public_hosts = get("AGSTUDIO_PUBLIC_HOST")
            .unwrap_or_default()
            .split(',')
            .map(|h| host_only(h.trim()))
            .filter(|h| !h.is_empty())
            .collect();

        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        Ok(Config {
            bind,
            basic_auth,
            basic_auth_fails_per_min: num("AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN", 10) as u32,
            guest_mode,
            max_concurrent: num("AGSTUDIO_MAX_CONCURRENT", cpus.clamp(2, 8)).max(1),
            rate_per_min: num("AGSTUDIO_RATE_PER_MIN", 120) as u32,
            translate_per_min: num("AGSTUDIO_TRANSLATE_PER_MIN", 12) as u32,
            auth_per_min: num("AGSTUDIO_AUTH_PER_MIN", 15) as u32,
            max_body_bytes: num("AGSTUDIO_MAX_BODY_KB", 8192).saturating_mul(1024),
            max_input_chars: num("AGSTUDIO_MAX_INPUT_CHARS", 16384),
            trust_proxy: flag("AGSTUDIO_TRUST_PROXY"),
            public_hosts,
            enable_translate: !flag("AGSTUDIO_DISABLE_TRANSLATE"),
            db_path: get("AGSTUDIO_DB")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./agstudio.db")),
            secure_cookies: flag("AGSTUDIO_SECURE_COOKIES"),
            queue_wait: Duration::from_secs(num("AGSTUDIO_QUEUE_WAIT_SECS", 5).min(60) as u64),
            solve_deadline: SOLVE_DEADLINE,
            header_timeout: HEADER_TIMEOUT,
        })
    }

    /// Guest access is only ever granted behind the shared Basic password.
    pub fn guest_allowed(&self) -> bool {
        self.guest_mode && self.basic_auth.is_some()
    }

    /// A one-line human summary for the startup banner.
    pub fn summary(&self) -> String {
        format!(
            "auth: {}{} · concurrency: {} · rate: {}/min (translate {}/min) · body ≤ {} KB · proxy IPs: {}",
            if self.basic_auth.is_some() {
                "Basic (required)"
            } else {
                "none (open)"
            },
            if self.guest_allowed() { ", guests allowed" } else { "" },
            self.max_concurrent,
            self.rate_per_min,
            self.translate_per_min,
            self.max_body_bytes / 1024,
            if self.trust_proxy { "trusted" } else { "ignored" },
        )
    }
}

type StatusProbe = fn() -> translate::Status;

/// How long a `claude auth status` result is reused by `/api/status`.
pub const TRANSLATE_STATUS_TTL: Duration = Duration::from_secs(60);

/// Shared server state.
pub struct AppState {
    pub config: Config,
    /// Concurrency gate for the CPU/subprocess-heavy endpoints.
    pub heavy: Arc<Semaphore>,
    pub render: Arc<Semaphore>,
    pub inflight: Arc<Mutex<HashMap<String, usize>>>,
    /// Concurrency gate for argon2 (login/register).
    pub argon: Arc<Semaphore>,
    pub rate: Mutex<RateLimiter>,
    pub db: crate::db::Db,
    /// Spawns `claude auth status`; swappable so tests need no CLI.
    pub translate_probe: StatusProbe,
    /// Last probe result. An async mutex, so concurrent misses wait for one
    /// probe instead of each spawning their own.
    pub translate_status: tokio::sync::Mutex<Option<(Instant, translate::Status)>>,
    /// The same result for non-blocking readers, plus whether a background
    /// refresh is already running.
    status_snapshot: Mutex<(Option<(Instant, translate::Status)>, bool)>,
    /// Last time expired sessions were swept from the DB (see [`sweep_expired_sessions`]).
    session_sweep: Mutex<Option<Instant>>,
}

pub type Shared = Arc<AppState>;

fn relock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    /// Open (and migrate) `config.db_path` and build the shared state. Fails
    /// closed — a broken/unwritable DB path stops `serve` before it binds,
    /// rather than surfacing as opaque 500s on first login.
    pub fn new(config: Config) -> Result<Shared, String> {
        AppState::with_translate_probe(config, translate::status)
    }

    pub fn with_translate_probe(config: Config, probe: StatusProbe) -> Result<Shared, String> {
        let db = crate::db::open(&config.db_path).map_err(|e| {
            format!(
                "could not open database at {}: {e}",
                config.db_path.display()
            )
        })?;
        Ok(Arc::new(AppState {
            heavy: Arc::new(Semaphore::new(config.max_concurrent)),
            render: Arc::new(Semaphore::new(config.max_concurrent)),
            inflight: Arc::new(Mutex::new(HashMap::new())),
            argon: Arc::new(Semaphore::new(ARGON2_PERMITS)),
            rate: Mutex::new(RateLimiter::default()),
            db,
            config,
            translate_probe: probe,
            translate_status: tokio::sync::Mutex::new(None),
            status_snapshot: Mutex::new((None, false)),
            session_sweep: Mutex::new(None),
        }))
    }

    /// Translation status for `/api/status`: never spawns anything when
    /// translation is disabled, otherwise probes on the blocking pool at most
    /// once per [`TRANSLATE_STATUS_TTL`].
    pub async fn translate_status(&self) -> translate::Status {
        let off = translate::Status { installed: false, logged_in: false };
        if !self.config.enable_translate {
            return off;
        }
        let mut cached = self.translate_status.lock().await;
        if let Some((at, s)) = *cached {
            if at.elapsed() < TRANSLATE_STATUS_TTL {
                return s;
            }
        }
        let probe = self.translate_probe;
        let s = tokio::task::spawn_blocking(probe).await.unwrap_or(off);
        let now = Instant::now();
        *cached = Some((now, s));
        relock(&self.status_snapshot).0 = Some((now, s));
        s
    }

    /// The last probe result without waiting: `None` until the first probe
    /// finishes. A missing or stale result starts one background refresh.
    pub fn translate_status_now(self: &Arc<Self>) -> Option<translate::Status> {
        if !self.config.enable_translate {
            return Some(translate::Status { installed: false, logged_in: false });
        }
        let mut snap = relock(&self.status_snapshot);
        let (current, stale) = match snap.0 {
            Some((at, s)) => (Some(s), at.elapsed() >= TRANSLATE_STATUS_TTL),
            None => (None, true),
        };
        if stale && !snap.1 {
            snap.1 = true;
            drop(snap);
            let me = self.clone();
            tokio::spawn(async move {
                me.translate_status().await;
                relock(&me.status_snapshot).1 = false;
            });
        }
        current
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

    fn cfg(vars: &[(&str, &str)]) -> Result<Config, String> {
        let vars: HashMap<String, String> =
            vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Config::from_lookup(8787, move |k| vars.get(k).cloned())
    }

    #[test]
    fn unusable_basic_auth_refuses_to_start() {
        for bad in ["", ":", "short", ":short", "user:short", "user:"] {
            assert!(cfg(&[("AGSTUDIO_BASIC_AUTH", bad)]).is_err(), "{bad:?} must fail closed");
        }
        assert!(cfg(&[("AGSTUDIO_BASIC_AUTH", "longenough")]).unwrap().basic_auth.is_some());
    }

    fn hdr(user: &str, pass: &str) -> String {
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
        )
    }

    #[test]
    fn basic_auth_forms() {
        for spec in [":hunter2hunter2", "hunter2hunter2"] {
            let a = BasicAuth::parse(spec).unwrap();
            assert!(a.matches(&hdr("", "hunter2hunter2")));
            assert!(a.matches(&hdr("whoever", "hunter2hunter2")));
            assert!(a.matches(&format!("basic {}", &hdr("x", "hunter2hunter2")[6..])));
            assert!(!a.matches(&hdr("whoever", "hunter2hunter3")));
            assert!(!a.matches("Bearer hunter2hunter2"));
            assert!(!a.matches("Basic !!!notbase64"));
        }
        let a = BasicAuth::parse("op:pa:ss:word").unwrap();
        assert!(a.matches(&hdr("op", "pa:ss:word")));
        assert!(!a.matches(&hdr("other", "pa:ss:word")));
        assert!(!a.matches(&hdr("op", "pa")));
    }

    #[test]
    fn guest_mode_defaults_follow_basic_auth() {
        assert!(!cfg(&[]).unwrap().guest_allowed());
        assert!(cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough")]).unwrap().guest_allowed());
        let off = cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough"), ("AGSTUDIO_GUEST_MODE", "0")]);
        assert!(!off.unwrap().guest_allowed());
        assert!(cfg(&[("AGSTUDIO_GUEST_MODE", "1")]).is_err(), "guest mode with no password");
    }

    #[test]
    fn trust_proxy_defaults_off_and_public_hosts_parse() {
        let c = cfg(&[]).unwrap();
        assert!(!c.trust_proxy);
        assert!(c.public_hosts.is_empty());
        let c = cfg(&[
            ("AGSTUDIO_TRUST_PROXY", "1"),
            ("AGSTUDIO_PUBLIC_HOST", " Geo.Tail1.ts.net:443 , other.example ,"),
        ])
        .unwrap();
        assert!(c.trust_proxy);
        assert_eq!(c.public_hosts, ["geo.tail1.ts.net", "other.example"]);
    }

    fn req_from(peer: &str, xff: &[&str]) -> Request<Body> {
        let mut b = Request::builder().uri("/api/status");
        for v in xff {
            b = b.header("x-forwarded-for", *v);
        }
        let mut r = b.body(Body::empty()).unwrap();
        r.extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        r
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn client_ip_uses_the_rightmost_xff_from_a_trusted_peer_only() {
        let mut c = cfg(&[]).unwrap();
        let spoofed = req_from("127.0.0.1:4000", &["6.6.6.6, 203.0.113.5"]);
        assert_eq!(client_ip(&c, &spoofed), ip("127.0.0.1"), "trust is off by default");
        c.trust_proxy = true;
        assert_eq!(client_ip(&c, &spoofed), ip("203.0.113.5"));
        let split = req_from("127.0.0.1:4000", &["6.6.6.6", "203.0.113.5"]);
        assert_eq!(client_ip(&c, &split), ip("203.0.113.5"));
        let tailnet = req_from("100.101.102.103:4000", &["203.0.113.5"]);
        assert_eq!(client_ip(&c, &tailnet), ip("203.0.113.5"));
        let direct = req_from("198.51.100.20:4000", &["203.0.113.5"]);
        assert_eq!(client_ip(&c, &direct), ip("198.51.100.20"), "untrusted peer: XFF ignored");
        let garbage = req_from("10.0.0.2:4000", &["203.0.113.5, not-an-ip"]);
        assert_eq!(client_ip(&c, &garbage), ip("10.0.0.2"), "invalid XFF: the peer, per request");
    }

    #[test]
    fn ipv6_clients_are_keyed_by_their_64() {
        let c = cfg(&[]).unwrap();
        let a = client_ip(&c, &req_from("[2001:db8:1:2:aaaa::1]:1", &[]));
        let b = client_ip(&c, &req_from("[2001:db8:1:2:bbbb::9]:1", &[]));
        let other = client_ip(&c, &req_from("[2001:db8:1:3::1]:1", &[]));
        assert_eq!(a, b);
        assert_ne!(a, other);
        let mapped = client_ip(&c, &req_from("[::ffff:198.51.100.7]:1", &[]));
        assert_eq!(mapped, ip("198.51.100.7"));
    }

    #[test]
    fn host_only_strips_ports_and_case() {
        assert_eq!(host_only("LocalHost:8787"), "localhost");
        assert_eq!(host_only("[::1]:8787"), "[::1]");
        assert_eq!(host_only("[::1]"), "[::1]");
        assert_eq!(host_only("geo.example"), "geo.example");
    }

    #[test]
    fn rate_limiter_and_sweep_survive_poison() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::from_env(0).unwrap();
        config.db_path = dir.path().join("p.db");
        let state = AppState::new(config).unwrap();
        let s2 = state.clone();
        let _ = std::thread::spawn(move || {
            let _a = s2.rate.lock().unwrap();
            let _b = s2.session_sweep.lock().unwrap();
            panic!("poison");
        })
        .join();
        sweep_expired_sessions(&state);
        assert!(relock(&state.rate).general.is_empty());
    }
}

// ---------------------------------------------------------------- rate limit --

struct Window {
    count: u32,
    start: Instant,
}

/// A simple per-IP fixed-window limiter with four buckets (general, translate,
/// credential-guessing-sensitive auth, and failed Basic auth), memory bounded
/// by a periodic sweep. Keys come from [`client_ip`] (IPv6 already cut to /64).
#[derive(Default)]
pub struct RateLimiter {
    general: HashMap<IpAddr, Window>,
    translate: HashMap<IpAddr, Window>,
    auth: HashMap<IpAddr, Window>,
    basic_fail: HashMap<IpAddr, Window>,
    last_sweep: Option<Instant>,
}

const WINDOW: Duration = Duration::from_secs(60);

impl RateLimiter {
    fn hit(map: &mut HashMap<IpAddr, Window>, ip: IpAddr, limit: u32, now: Instant) -> bool {
        if limit == 0 {
            return true;
        }
        let w = map.entry(ip).or_insert(Window {
            count: 0,
            start: now,
        });
        if now.duration_since(w.start) >= WINDOW {
            w.count = 0;
            w.start = now;
        }
        w.count = w.count.saturating_add(1);
        w.count <= limit
    }

    /// Whether `ip` has already used up `limit` in the current window (no hit).
    fn exhausted(map: &HashMap<IpAddr, Window>, ip: IpAddr, limit: u32, now: Instant) -> bool {
        limit != 0
            && map
                .get(&ip)
                .is_some_and(|w| now.duration_since(w.start) < WINDOW && w.count >= limit)
    }

    fn sweep(&mut self, now: Instant) {
        if self
            .last_sweep
            .is_none_or(|t| now.duration_since(t) > Duration::from_secs(300))
        {
            for m in [&mut self.general, &mut self.translate, &mut self.auth, &mut self.basic_fail] {
                m.retain(|_, w| now.duration_since(w.start) < WINDOW);
            }
            self.last_sweep = Some(now);
        }
    }
}

/// Peers allowed to speak for someone else via `X-Forwarded-For`: the proxy is
/// on this host (`tailscale funnel`, nginx), a private LAN, or the tailnet.
fn is_trusted_proxy(ip: IpAddr) -> bool {
    match canonical(ip) {
        IpAddr::V4(v4) => {
            let cgnat = v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 64;
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || cgnat
        }
        IpAddr::V6(v6) => {
            let seg0 = v6.segments()[0];
            v6.is_loopback() || (seg0 & 0xfe00) == 0xfc00 || (seg0 & 0xffc0) == 0xfe80
        }
    }
}

fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    }
}

/// The rate-limit key: IPv4 as is, IPv6 cut to its /64 (one subscriber's
/// allocation, which an attacker can otherwise rotate through for free).
fn rate_key(ip: IpAddr) -> IpAddr {
    match canonical(ip) {
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

/// Client address for rate limiting. `X-Forwarded-For` is honoured only when
/// enabled *and* the TCP peer is a trusted proxy, and then only its rightmost
/// entry — the one that proxy appended; everything left of it is whatever the
/// client chose to send.
fn client_ip(cfg: &Config, req: &Request<Body>) -> IpAddr {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    if cfg.trust_proxy && peer.is_some_and(is_trusted_proxy) {
        let rightmost = req
            .headers()
            .get_all("x-forwarded-for")
            .iter()
            .next_back()
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next())
            .and_then(|s| s.trim().parse::<IpAddr>().ok());
        if let Some(ip) = rightmost {
            return rate_key(ip);
        }
    }
    rate_key(peer.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)))
}

/// Lowercased host with any `:port` removed (`[v6]` brackets kept).
fn host_only(h: &str) -> String {
    let h = h.trim();
    let bare = if let Some(end) = h.strip_prefix('[').and_then(|r| r.find(']')) {
        &h[..end + 2]
    } else {
        match h.rsplit_once(':') {
            Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => name,
            _ => h,
        }
    };
    bare.to_ascii_lowercase()
}

fn json_error(code: StatusCode, msg: &str) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

// --------------------------------------------------------------- middleware --

/// DNS-rebinding guard. A page on `attacker.example` whose DNS flips to
/// 127.0.0.1 reaches a loopback-bound app with `Host: attacker.example`; only
/// the names this app is really served as are accepted. Applies when bound to
/// loopback or when `AGSTUDIO_PUBLIC_HOST` is set; a request with no `Host` at
/// all (not a browser) passes.
pub async fn host_guard(State(state): State<Shared>, req: Request<Body>, next: Next) -> Response {
    let cfg = &state.config;
    if !cfg.bind.ip().is_loopback() && cfg.public_hosts.is_empty() {
        return next.run(req).await;
    }
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| req.uri().authority().map(|a| a.to_string()));
    if let Some(host) = host {
        let h = host_only(&host);
        let ok = matches!(h.as_str(), "localhost" | "127.0.0.1" | "[::1]")
            || cfg.public_hosts.contains(&h);
        if !ok {
            eprintln!("refused request for unknown Host `{host}`");
            return json_error(StatusCode::FORBIDDEN, "unknown host");
        }
    }
    next.run(req).await
}

/// Require HTTP Basic auth when configured. Wrong credentials are counted per
/// client IP; past `basic_auth_fails_per_min` the IP gets 429 without its
/// credentials even being checked, so the shared password can't be brute-forced.
/// A request with no `Authorization` at all (a browser's first, pre-prompt
/// request) gets the 401 challenge but is not counted.
pub async fn auth(State(state): State<Shared>, req: Request<Body>, next: Next) -> Response {
    // The health probe is always reachable (no secret, no side effects).
    if req.uri().path() == "/healthz" {
        return next.run(req).await;
    }
    if let Some(expected) = &state.config.basic_auth {
        let presented = req
            .headers()
            .get(header::AUTHORIZATION)
            .map(|v| v.to_str().unwrap_or(""));
        let ok = if let Some(got) = presented {
            let ip = client_ip(&state.config, &req);
            let limit = state.config.basic_auth_fails_per_min;
            let now = Instant::now();
            if RateLimiter::exhausted(&relock(&state.rate).basic_fail, ip, limit, now) {
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    [(header::RETRY_AFTER, "60")],
                    "Too many failed logins; try again in a minute.",
                )
                    .into_response();
            }
            let ok = expected.matches(got);
            if !ok {
                let mut r = relock(&state.rate);
                r.sweep(now);
                RateLimiter::hit(&mut r.basic_fail, ip, limit, now);
            }
            ok
        } else {
            false
        };
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

/// Claim the 5-minute session sweep slot, if it is due.
fn sweep_due(state: &Shared) -> bool {
    let now = Instant::now();
    let mut last = relock(&state.session_sweep);
    let due = last.is_none_or(|t| now.duration_since(t) > Duration::from_secs(300));
    if due {
        *last = Some(now);
    }
    due
}

/// Delete expired session rows from the DB at most once per 5 minutes,
/// piggybacked on request traffic — same cadence and rationale as
/// `RateLimiter::sweep`, just for the DB-backed session table instead of the
/// in-memory rate-limit maps. Blocking; the middleware runs it on the pool.
#[cfg(test)]
fn sweep_expired_sessions(state: &Shared) {
    if sweep_due(state) {
        purge_expired_sessions(&state.db);
    }
}

fn purge_expired_sessions(db: &crate::db::Db) {
    let _ = crate::db::sweep_expired_sessions(&crate::db::lock(db));
}

/// Per-IP rate limiting for `/api/*` (with a stricter bucket for translate and
/// humanize, which both drive the same costly `claude` CLI subprocess).
#[derive(Clone, Copy, Debug)]
pub struct ClientIp(pub IpAddr);

pub struct CallerSlot {
    key: String,
    map: Arc<Mutex<HashMap<String, usize>>>,
}

impl Drop for CallerSlot {
    fn drop(&mut self) {
        let mut m = relock(&self.map);
        if let Some(n) = m.get_mut(&self.key) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&self.key);
            }
        }
    }
}

impl AppState {
    pub fn per_caller_limit(&self) -> usize {
        (self.config.max_concurrent / 2).max(1)
    }

    /// Count one more solve for `key`, or `None` when that caller already has
    /// [`AppState::per_caller_limit`] solves (cancelled ones included) running.
    pub fn claim_caller(&self, key: String) -> Option<CallerSlot> {
        self.claim_within(key, self.per_caller_limit())
    }

    /// How many solves all guests behind one address may run together: every
    /// slot but one, so one network cannot lock everyone else out.
    pub fn guest_ip_limit(&self) -> usize {
        self.config.max_concurrent.saturating_sub(1).max(self.per_caller_limit())
    }

    pub fn claim_within(&self, key: String, limit: usize) -> Option<CallerSlot> {
        let mut m = relock(&self.inflight);
        let n = m.entry(key.clone()).or_insert(0);
        if *n >= limit {
            return None;
        }
        *n += 1;
        Some(CallerSlot { key, map: self.inflight.clone() })
    }
}

pub async fn rate_limit(State(state): State<Shared>, mut req: Request<Body>, next: Next) -> Response {
    if sweep_due(&state) {
        let db = state.db.clone();
        tokio::task::spawn_blocking(move || purge_expired_sessions(&db));
    }
    let path = req.uri().path();
    if path.starts_with("/api/") {
        let is_translate = path == "/api/translate" || path == "/api/humanize";
        // Login and register get their own tight bucket: they are the two
        // routes an attacker can hammer for credential stuffing / spam signups.
        let is_auth = path == "/api/auth/login" || path == "/api/auth/register";
        let ip = client_ip(&state.config, &req);
        req.extensions_mut().insert(ClientIp(ip));
        let now = Instant::now();
        let allowed = {
            let mut r = relock(&state.rate);
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
/// browser-sent `Origin` on a state-changing request must name the host the
/// request arrived on (ports ignored: nginx's `$host` drops them), and the
/// opaque `Origin: null` (sandboxed frames, `file:`) is refused outright.
/// Requests without an `Origin` header (curl, same-origin GETs, MCP) pass.
pub async fn same_origin(req: Request<Body>, next: Next) -> Response {
    let writes = matches!(
        *req.method(),
        axum::http::Method::POST
            | axum::http::Method::PUT
            | axum::http::Method::PATCH
            | axum::http::Method::DELETE
    );
    if writes {
        if let Some(origin) = req.headers().get(header::ORIGIN) {
            let origin = origin.to_str().unwrap_or("null");
            let origin_host = origin
                .strip_prefix("https://")
                .or_else(|| origin.strip_prefix("http://"))
                .map(host_only);
            let host = req
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                .map(host_only);
            if origin_host.is_none() || origin_host != host {
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

/// The bodies of every `<script>` without a `src` in `html`.
pub fn inline_scripts(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<script") {
        rest = &rest[i..];
        let Some(open_end) = rest.find('>') else { break };
        let tag = &rest[..open_end];
        let body_start = open_end + 1;
        let Some(close) = rest[body_start..].find("</script>") else { break };
        if !tag.contains("src=") {
            out.push(&rest[body_start..body_start + close]);
        }
        rest = &rest[body_start + close..];
    }
    out
}

/// A CSP source expression allowing exactly this inline script.
pub fn csp_hash(script: &str) -> String {
    format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(sha256(script.as_bytes()))
    )
}

/// The page CSP: inline scripts are pinned by hash instead of `'unsafe-inline'`,
/// so injected markup can't run script. Styles keep `'unsafe-inline'` (the
/// pages use `style=` attributes throughout).
fn content_security_policy() -> &'static str {
    static CSP: OnceLock<String> = OnceLock::new();
    CSP.get_or_init(|| {
        let pages = [
            include_str!("../assets/index.html"),
            include_str!("../assets/auth.html"),
            include_str!("../assets/landing.html"),
        ];
        let hashes: Vec<String> = pages
            .iter()
            .flat_map(|p| inline_scripts(p))
            .map(csp_hash)
            .collect();
        format!(
            "default-src 'self'; script-src 'self' {}; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; \
             base-uri 'none'; form-action 'self'",
            hashes.join(" ")
        )
    })
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
    set(h, header::CONTENT_SECURITY_POLICY, content_security_policy());
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

/// Constant-time comparison of two equal-length digests.
fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// Apply process-wide, server-safe resource limits (idempotent; only sets what
/// the operator has not already chosen). Call once at startup, before solving.
pub fn apply_process_limits() {
    // Run caps of the legacy aux search (the fallback when the rollout search
    // cannot close the base figure); every solve is also wall-clock bounded.
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
