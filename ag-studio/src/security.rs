//! Security configuration and middleware for the `serve` web app.
//!
//! All controls are configured from environment variables so the same binary is
//! safe to run locally *and* to expose (behind a reverse proxy) on a public
//! Linux server:
//!
//! | Env var | Default | Effect |
//! |---|---|---|
//! | `AGSTUDIO_BIND` | `127.0.0.1:<port>` | interface/port to bind |
//! | `AGSTUDIO_BASIC_AUTH` | (none) | the shared site password. `user:pass` checks both; `:pass` or a bare `pass` (no colon) accepts **any** username with that password. Set-but-unusable (empty, a password under 8 chars, or one still containing the template's `CHANGE-ME`) refuses to start. `AGSTUDIO_BASIC_AUTH_FILE` reads it from a file instead |
//! | `AGSTUDIO_GATE` | `form` | how browsers enter the shared password: `form` = the `/gate` password page, which sets a signed `gate` cookie (HTTP Basic is still accepted, for API clients); `basic` = only the HTTP Basic dialog (the behaviour before the gate page) |
//! | `AGSTUDIO_GATE_KEY` | (generated, stored in the DB) | 64 hex chars: the HMAC key of the gate cookie. Malformed refuses to start. Rotating it (or the password) signs every device out. `AGSTUDIO_GATE_KEY_FILE` reads it from a file instead |
//! | `AGSTUDIO_GATE_DAYS` | 180 | how long the gate cookie lasts (1–400 days; renewed on visits after 7 days) |
//! | `AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN` | 10 | per-IP *wrong* shared passwords per minute (Basic and the gate form together) before 429 (0 = off) |
//! | `AGSTUDIO_GUEST_MODE` | on if `AGSTUDIO_BASIC_AUTH` is set, else off | `1`/`0` override. Visitors past the shared password may solve, export and humanize without an account (no history). `1` without a password refuses to start |
//! | `AGSTUDIO_GUEST_AI` | same as guest mode | `0` keeps Describe and Photo (`/api/translate`) for accounts only; guests still get AI explanations |
//! | `AGSTUDIO_GUEST_AI_PER_DAY` | 0 | per-IP `/api/translate` + `/api/humanize` calls per 24 h (one budget), counted once the request is valid, for guests and accounts alike, so registering an account does not reset it (0 = no daily cap; the per-minute limit always applies) |
//! | `AGSTUDIO_AI_PER_DAY_TOTAL` | 0 | server-wide `/api/translate` + `/api/humanize` calls per 24 h, every caller and address together: the ceiling on what the subscription spends (0 = none) |
//! | `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
//! | `AGSTUDIO_MAX_CONCURRENT` | CPUs available, clamped to 2..8 | simultaneous heavy requests; each solve/export is one worker process |
//! | `AGSTUDIO_QUEUE_WAIT_SECS` | 5 | how long a heavy request waits for a free slot before 503 + `Retry-After` (max 60) |
//! | `AGSTUDIO_SOLVE_DEADLINE_SECS` | 60 | wall-clock limit of one web solve or export (10–600); the request timeout grows to fit |
//! | `AGSTUDIO_EXTENDED_DEADLINE_SECS` | 180 | limit of the "Keep searching" solve offered after a time-limit verdict (one at a time, server-wide; 0 = not offered; at most 900) |
//! | `RAYON_NUM_THREADS` | unset | solver threads per worker; unset, the CPUs (cgroup quota included) minus one are shared among the workers running at once |
//! | `AGSTUDIO_WORKER_MEM_MB` | 2048 | memory cap (`RLIMIT_DATA`: heap and thread stacks, not merely reserved address space) of each solve worker process; 0 = none |
//! | `AGSTUDIO_HUMAN_PROOFS` | 1 | 0 turns off the human-style proof writer (the Proof tab, MCP and reports then show only the full derivation) |
//! | `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests per minute (0 = off) |
//! | `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP `/api/translate` + `/api/humanize` per minute (0 = off) |
//! | `AGSTUDIO_AUTH_PER_MIN` | 15 | per-IP `/api/auth/login` + `register` per minute (0 = off) |
//! | `AGSTUDIO_MAX_BODY_KB` | 8192 | request body size limit |
//! | `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
//! | `AGSTUDIO_DISABLE_TRANSLATE` | off | turn off `/api/translate` and `/api/humanize` (no `claude` subprocess at all) |
//! | `AGSTUDIO_CLAUDE_BIN` | `claude` on `PATH`, then `~/.local/bin` | the Claude Code CLI; when set it is authoritative (a wrong path reports "not installed") |
//! | `AGSTUDIO_CLAUDE_MODEL` | the CLI's default | `--model` for Describe/Photo translations |
//! | `AGSTUDIO_HUMANIZE_MODEL` | `claude-opus-4-8` | `--model` for AI explanations (`/api/humanize`) |
//! | `CLAUDE_CODE_OAUTH_TOKEN` | (none) | subscription token from `claude setup-token`; counts as signed in (`CLAUDE_CODE_OAUTH_TOKEN_FILE`: from a file) |
//! | `CLAUDE_CONFIG_DIR` | `~/.claude` | the CLI's state directory; must be writable by the service |
//! | `AGSTUDIO_TRUST_PROXY` | off | client IP = **rightmost** `X-Forwarded-For` entry, honoured only when the TCP peer is loopback/private/CGNAT (i.e. the proxy). Turn on behind `tailscale funnel`/nginx, or every visitor shares one rate-limit bucket |
//! | `AGSTUDIO_PUBLIC_HOST` | (none) | comma list of hostnames (a `:port` is ignored) the app is served as (e.g. the funnel `*.ts.net` name). With a loopback bind and none set, only `localhost`/`127.0.0.1`/`[::1]` `Host` headers are accepted (DNS-rebinding guard); `/healthz` accepts any |
//! | `AGSTUDIO_DB` | `./agstudio.db` | SQLite file for accounts/sessions/history |
//! | `AGSTUDIO_SECURE_COOKIES` | off | session cookie becomes `__Host-sid` + `Secure`, and HSTS is sent (needs HTTPS) |
//! | `AGSTUDIO_SHUTDOWN_GRACE_SECS` | 70 | on SIGTERM/SIGINT: stop accepting, let requests in flight finish for at most this long (default = solve deadline + worker grace + 5 s), checkpoint the DB, exit 0. Keep the service manager's stop timeout above it |
//! | `AGSTUDIO_ACCESS_LOG` | on | one stdout line per request: UTC time, client IP, method, path (no query), status, milliseconds; `/healthz` and `/readyz` are not logged. `0` turns it off |
//!
//! Flags take `1`/`0`, `true`/`false`, `yes`/`no` or `on`/`off` in any case; any other
//! value, or a number that does not parse, refuses to start (empty = unset).
//! `AGSTUDIO_BASIC_AUTH_FILE`, `AGSTUDIO_GATE_KEY_FILE` and `CLAUDE_CODE_OAUTH_TOKEN_FILE`
//! read the secret from a file; setting a variable and its `_FILE` refuses to start.

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

/// The marker of the template password in `deploy/.env.example`.
const PLACEHOLDER_PASSWORD: &str = "change-me";

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
        if pass.to_ascii_lowercase().contains(PLACEHOLDER_PASSWORD) {
            return Err(
                "AGSTUDIO_BASIC_AUTH is still the CHANGE-ME placeholder from deploy/.env.example, \
                 which is published with the source. Set your own password, e.g. \
                 `AGSTUDIO_BASIC_AUTH=:$(openssl rand -hex 16)`"
                    .into(),
            );
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
        self.credentials_match(user, pass)
    }

    /// Whether a username/password pair (the gate form's fields) matches.
    pub fn credentials_match(&self, user: &[u8], pass: &[u8]) -> bool {
        let pass_ok = ct_eq(&sha256(pass), &self.pass);
        let user_ok = self.user.as_ref().is_none_or(|u| ct_eq(&sha256(user), u));
        pass_ok & user_ok
    }

    /// Whether the spec names a username (`user:pass`), which the gate page
    /// then asks for.
    pub fn has_user(&self) -> bool {
        self.user.is_some()
    }

    /// A digest of the configured credentials. Gate cookies are signed over
    /// it, so changing the password (or the user) revokes all of them.
    pub fn cred_digest(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.user.unwrap_or([0u8; 32]));
        h.update(self.pass);
        h.finalize().into()
    }
}

/// How a browser presents the shared password.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateMode {
    Form,
    Basic,
}

fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()?;
    }
    Some(k)
}

/// Parsed, validated server configuration.
#[derive(Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub basic_auth: Option<BasicAuth>,
    pub basic_auth_fails_per_min: u32,
    pub gate: GateMode,
    /// `AGSTUDIO_GATE_KEY`; `None` = the key stored in the DB.
    pub gate_key: Option<[u8; 32]>,
    pub gate_days: u32,
    /// Visitors past the shared password may use the solver without an account.
    pub guest_mode: bool,
    /// Guests may also use Describe and Photo.
    pub guest_ai: bool,
    pub guest_ai_per_day: u32,
    pub ai_per_day_total: u32,
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
    /// `CLAUDE_CODE_OAUTH_TOKEN`, or the contents of `CLAUDE_CODE_OAUTH_TOKEN_FILE`.
    pub claude_oauth_token: Option<String>,
    /// How long a heavy request waits for a slot before 503 + `Retry-After`.
    pub queue_wait: Duration,
    /// Wall-clock deadline of one web solve or export (its worker is killed
    /// [`crate::worker::GRACE`] later).
    pub solve_deadline: Duration,
    /// Deadline of a "keep searching" solve after a time-limit verdict
    /// (`AGSTUDIO_EXTENDED_DEADLINE_SECS`; `None` = not offered).
    pub extended_deadline: Option<Duration>,
    /// How long a client has to send a request's headers before the
    /// connection is closed.
    pub header_timeout: Duration,
    /// How long requests in flight may run on after a shutdown signal.
    pub shutdown_grace: Duration,
    pub access_log: bool,
}

/// Default web solve deadline (`AGSTUDIO_SOLVE_DEADLINE_SECS`).
pub const SOLVE_DEADLINE: Duration = Duration::from_secs(60);
/// Default [`Config::extended_deadline`].
pub const EXTENDED_DEADLINE: Duration = Duration::from_secs(180);
/// The shortest whole-request timeout; longer deadlines raise it.
pub const REQUEST_TIMEOUT_MIN: Duration = Duration::from_secs(120);
/// Default [`Config::header_timeout`].
pub const HEADER_TIMEOUT: Duration = Duration::from_secs(30);

/// A boolean setting: `1`/`true`/`yes`/`on` or `0`/`false`/`no`/`off`, any case.
pub fn parse_flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn set_value(get: &impl Fn(&str) -> Option<String>, k: &str) -> Option<String> {
    get(k).filter(|v| !v.trim().is_empty())
}

fn env_flag(get: &impl Fn(&str) -> Option<String>, k: &str) -> Result<Option<bool>, String> {
    match set_value(get, k) {
        None => Ok(None),
        Some(v) => parse_flag(&v)
            .map(Some)
            .ok_or_else(|| format!("invalid {k} `{v}` (want 1/0, true/false, yes/no or on/off)")),
    }
}

fn env_num<T: std::str::FromStr>(
    get: &impl Fn(&str) -> Option<String>,
    k: &str,
    default: T,
) -> Result<T, String> {
    match set_value(get, k) {
        None => Ok(default),
        Some(v) => v
            .trim()
            .parse()
            .map_err(|_| format!("invalid {k} `{v}` (want a whole number)")),
    }
}

fn env_secret(get: &impl Fn(&str) -> Option<String>, k: &str) -> Result<Option<String>, String> {
    let file_key = format!("{k}_FILE");
    match (get(k), set_value(get, &file_key)) {
        (Some(_), Some(_)) => Err(format!("set {k} or {file_key}, not both")),
        (Some(v), None) => Ok(Some(v)),
        (None, None) => Ok(None),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path.trim())
                .map_err(|e| format!("could not read {file_key} `{path}`: {e}"))?;
            Ok(Some(text.trim_end_matches(['\n', '\r']).to_string()))
        }
    }
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
        let flag = |k: &str| env_flag(&get, k).map(|v| v.unwrap_or(false));

        let bind_str = get("AGSTUDIO_BIND").unwrap_or_else(|| format!("127.0.0.1:{default_port}"));
        let bind: SocketAddr = bind_str
            .parse()
            .map_err(|_| format!("invalid AGSTUDIO_BIND `{bind_str}` (want e.g. 0.0.0.0:8787)"))?;

        // Fail closed: a set-but-broken value must never silently mean "no auth".
        let basic_auth = match env_secret(&get, "AGSTUDIO_BASIC_AUTH")? {
            None => None,
            Some(spec) => Some(BasicAuth::parse(&spec)?),
        };

        let allow_insecure = flag("AGSTUDIO_ALLOW_INSECURE")?;
        if !bind.ip().is_loopback() && basic_auth.is_none() && !allow_insecure {
            return Err(format!(
                "refusing to bind the non-loopback interface {bind} with no authentication.\n  \
                 Set AGSTUDIO_BASIC_AUTH=\"user:pass\" to require a login, OR front the app with a \
                 reverse proxy (nginx) and set AGSTUDIO_ALLOW_INSECURE=1 only if the app port is not \
                 reachable from outside the host."
            ));
        }

        let guest_mode = env_flag(&get, "AGSTUDIO_GUEST_MODE")?.unwrap_or(basic_auth.is_some());
        if guest_mode && basic_auth.is_none() {
            return Err(
                "AGSTUDIO_GUEST_MODE=1 needs AGSTUDIO_BASIC_AUTH: guest access is \
                        \"link + shared password\", never fully open"
                    .into(),
            );
        }
        let guest_ai = env_flag(&get, "AGSTUDIO_GUEST_AI")?.unwrap_or(guest_mode);

        let gate = match get("AGSTUDIO_GATE").as_deref().map(str::trim) {
            None | Some("") | Some("form") => GateMode::Form,
            Some("basic") => GateMode::Basic,
            Some(other) => {
                return Err(format!(
                    "invalid AGSTUDIO_GATE `{other}` (want `form` or `basic`)"
                ))
            }
        };
        let gate_key = match env_secret(&get, "AGSTUDIO_GATE_KEY")? {
            None => None,
            Some(v) => Some(parse_hex32(v.trim()).ok_or(
                "AGSTUDIO_GATE_KEY is set but unusable: it must be 64 hex characters (32 bytes), \
                 e.g. the output of `openssl rand -hex 32`",
            )?),
        };

        let public_hosts = get("AGSTUDIO_PUBLIC_HOST")
            .unwrap_or_default()
            .split(',')
            .map(|h| host_only(h.trim()))
            .filter(|h| !h.is_empty())
            .collect();

        let claude_oauth_token = env_secret(&get, "CLAUDE_CODE_OAUTH_TOKEN")?;
        env_num::<u64>(&get, "AGSTUDIO_WORKER_MEM_MB", 0)?;
        env_flag(&get, crate::engine::HUMAN_ENV)?;

        let cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        let solve_deadline = Duration::from_secs(
            env_num(
                &get,
                "AGSTUDIO_SOLVE_DEADLINE_SECS",
                SOLVE_DEADLINE.as_secs(),
            )?
            .clamp(10, 600),
        );
        let extended_deadline = match env_num(
            &get,
            "AGSTUDIO_EXTENDED_DEADLINE_SECS",
            EXTENDED_DEADLINE.as_secs(),
        )? {
            0 => None,
            s => Some(Duration::from_secs(s.min(900))).filter(|d| *d > solve_deadline),
        };
        Ok(Config {
            bind,
            basic_auth,
            basic_auth_fails_per_min: env_num(&get, "AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN", 10)?,
            gate,
            gate_key,
            gate_days: env_num(&get, "AGSTUDIO_GATE_DAYS", 180u32)?.clamp(1, 400),
            guest_mode,
            guest_ai,
            guest_ai_per_day: env_num(&get, "AGSTUDIO_GUEST_AI_PER_DAY", 0)?,
            ai_per_day_total: env_num(&get, "AGSTUDIO_AI_PER_DAY_TOTAL", 0)?,
            max_concurrent: env_num(&get, "AGSTUDIO_MAX_CONCURRENT", cpus.clamp(2, 8))?.max(1),
            rate_per_min: env_num(&get, "AGSTUDIO_RATE_PER_MIN", 120)?,
            translate_per_min: env_num(&get, "AGSTUDIO_TRANSLATE_PER_MIN", 12)?,
            auth_per_min: env_num(&get, "AGSTUDIO_AUTH_PER_MIN", 15)?,
            max_body_bytes: env_num(&get, "AGSTUDIO_MAX_BODY_KB", 8192usize)?.saturating_mul(1024),
            max_input_chars: env_num(&get, "AGSTUDIO_MAX_INPUT_CHARS", 16384)?,
            trust_proxy: flag("AGSTUDIO_TRUST_PROXY")?,
            public_hosts,
            enable_translate: !flag("AGSTUDIO_DISABLE_TRANSLATE")?,
            db_path: get("AGSTUDIO_DB")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("./agstudio.db")),
            secure_cookies: flag("AGSTUDIO_SECURE_COOKIES")?,
            claude_oauth_token,
            queue_wait: Duration::from_secs(
                env_num(&get, "AGSTUDIO_QUEUE_WAIT_SECS", 5u64)?.min(60),
            ),
            solve_deadline,
            extended_deadline,
            header_timeout: HEADER_TIMEOUT,
            shutdown_grace: match set_value(&get, "AGSTUDIO_SHUTDOWN_GRACE_SECS") {
                Some(_) => Duration::from_secs(
                    env_num(&get, "AGSTUDIO_SHUTDOWN_GRACE_SECS", 0u64)?.min(600),
                ),
                None => solve_deadline + crate::worker::GRACE + Duration::from_secs(5),
            },
            access_log: env_flag(&get, "AGSTUDIO_ACCESS_LOG")?.unwrap_or(true),
        })
    }

    /// Hand a file-supplied subscription token to the `claude` children. Call
    /// before any other thread starts.
    pub fn export_claude_token(&self) {
        if let Some(t) = &self.claude_oauth_token {
            if std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok().as_deref() != Some(t.as_str()) {
                std::env::set_var("CLAUDE_CODE_OAUTH_TOKEN", t);
            }
        }
    }

    /// The whole-request timeout: room for the longest solve, its worker's
    /// grace period and the wait for a slot.
    pub fn request_timeout(&self) -> Duration {
        let longest = self
            .extended_deadline
            .unwrap_or(self.solve_deadline)
            .max(self.solve_deadline);
        REQUEST_TIMEOUT_MIN
            .max(longest + crate::worker::GRACE + self.queue_wait + Duration::from_secs(30))
    }

    /// Guest access is only ever granted behind the shared Basic password.
    pub fn guest_allowed(&self) -> bool {
        self.guest_mode && self.basic_auth.is_some()
    }

    /// Guests may use Describe and Photo, not only AI explanations.
    pub fn guest_ai_allowed(&self) -> bool {
        self.guest_allowed() && self.guest_ai
    }

    /// The `/gate` password page (and its cookie) is in use.
    pub fn form_gate(&self) -> bool {
        self.basic_auth.is_some() && self.gate == GateMode::Form
    }

    /// A one-line human summary for the startup banner.
    pub fn summary(&self) -> String {
        format!(
            "auth: {}{} · concurrency: {} · rate: {}/min (translate {}/min) · body ≤ {} KB · proxy IPs: {} · secure cookies: {} · v{}",
            match (&self.basic_auth, self.gate) {
                (None, _) => "none (open)",
                (Some(_), GateMode::Form) => "shared password (form + Basic)",
                (Some(_), GateMode::Basic) => "shared password (Basic only)",
            },
            if self.guest_allowed() { ", guests allowed" } else { "" },
            self.max_concurrent,
            self.rate_per_min,
            self.translate_per_min,
            self.max_body_bytes / 1024,
            if self.trust_proxy { "trusted" } else { "ignored" },
            if self.secure_cookies { "on" } else { "off" },
            env!("CARGO_PKG_VERSION"),
        )
    }
}

type StatusProbe = fn() -> translate::Status;

/// How long a `claude auth status` result is reused by `/api/status`.
pub const TRANSLATE_STATUS_TTL: Duration = Duration::from_secs(60);

/// How long a credential the API rejected keeps AI reported as signed out.
pub const CREDENTIAL_REJECTED_HOLD: Duration = Duration::from_secs(600);

/// Shared server state.
pub struct AppState {
    pub config: Config,
    /// Concurrency gate for the CPU/subprocess-heavy endpoints.
    pub heavy: Arc<Semaphore>,
    pub render: Arc<Semaphore>,
    /// One "keep searching" solve at a time, server-wide.
    pub extended: Arc<Semaphore>,
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
    credential_rejected: Mutex<Option<Instant>>,
    /// HMAC key of the gate cookie: `AGSTUDIO_GATE_KEY`, else the DB's.
    pub gate_key: [u8; 32],
    /// A shutdown signal arrived; requests in flight are draining.
    pub draining: std::sync::atomic::AtomicBool,
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
        let gate_key = match config.gate_key {
            Some(k) => k,
            None => crate::db::server_secret(
                &crate::db::lock(&db),
                "gate_key",
                crate::auth::random_key(),
            )
            .map_err(|e| {
                format!(
                    "could not read the gate key from {}: {e}",
                    config.db_path.display()
                )
            })?,
        };
        Ok(Arc::new(AppState {
            heavy: Arc::new(Semaphore::new(config.max_concurrent)),
            render: Arc::new(Semaphore::new(config.max_concurrent)),
            extended: Arc::new(Semaphore::new(1)),
            inflight: Arc::new(Mutex::new(HashMap::new())),
            argon: Arc::new(Semaphore::new(ARGON2_PERMITS)),
            rate: Mutex::new(RateLimiter::default()),
            db,
            config,
            translate_probe: probe,
            translate_status: tokio::sync::Mutex::new(None),
            status_snapshot: Mutex::new((None, false)),
            session_sweep: Mutex::new(None),
            credential_rejected: Mutex::new(None),
            gate_key,
            draining: std::sync::atomic::AtomicBool::new(false),
        }))
    }

    /// Record how a call to the `claude` CLI ended: a rejected credential
    /// reports AI as signed out for [`CREDENTIAL_REJECTED_HOLD`]; an answer
    /// clears that.
    pub fn note_ai_outcome(&self, outcome: Result<(), &anyhow::Error>) {
        let mut rejected = relock(&self.credential_rejected);
        match outcome {
            Ok(()) => *rejected = None,
            Err(e) if translate::failure(e) == Some(translate::Failure::SignedOut) => {
                if rejected.is_none() {
                    eprintln!(
                        "  AI translation: off for {} min — the claude CLI is signed out or its credential was rejected (see the error above); `agstudio doctor` checks it",
                        CREDENTIAL_REJECTED_HOLD.as_secs() / 60
                    );
                }
                *rejected = Some(Instant::now());
            }
            Err(_) => {}
        }
    }

    fn with_rejection(&self, s: translate::Status) -> translate::Status {
        let held = relock(&self.credential_rejected)
            .is_some_and(|at| at.elapsed() < CREDENTIAL_REJECTED_HOLD);
        translate::Status {
            installed: s.installed,
            logged_in: s.logged_in && !held,
        }
    }

    /// Translation status for `/api/status`: never spawns anything when
    /// translation is disabled, otherwise probes on the blocking pool at most
    /// once per [`TRANSLATE_STATUS_TTL`].
    pub async fn translate_status(&self) -> translate::Status {
        let off = translate::Status {
            installed: false,
            logged_in: false,
        };
        if !self.config.enable_translate {
            return off;
        }
        let mut cached = self.translate_status.lock().await;
        if let Some((at, s)) = *cached {
            if at.elapsed() < TRANSLATE_STATUS_TTL {
                return self.with_rejection(s);
            }
        }
        let probe = self.translate_probe;
        let s = tokio::task::spawn_blocking(probe).await.unwrap_or(off);
        let now = Instant::now();
        *cached = Some((now, s));
        relock(&self.status_snapshot).0 = Some((now, s));
        self.with_rejection(s)
    }

    /// The last probe result without waiting: `None` until the first probe
    /// finishes. A missing or stale result starts one background refresh.
    pub fn translate_status_now(self: &Arc<Self>) -> Option<translate::Status> {
        if !self.config.enable_translate {
            return Some(translate::Status {
                installed: false,
                logged_in: false,
            });
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
        current.map(|s| self.with_rejection(s))
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
        assert!(crate::db::find_user_by_name(&conn, "alice")
            .unwrap()
            .is_some());
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
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(8787, move |k| vars.get(k).cloned())
    }

    #[test]
    fn the_solve_deadlines_are_configurable_and_the_request_timeout_fits_them() {
        let c = cfg(&[]).unwrap();
        assert_eq!(c.solve_deadline, Duration::from_secs(60));
        assert_eq!(c.extended_deadline, Some(Duration::from_secs(180)));
        assert!(
            c.request_timeout() >= Duration::from_secs(180) + crate::worker::GRACE + c.queue_wait
        );
        let c = cfg(&[
            ("AGSTUDIO_SOLVE_DEADLINE_SECS", "90"),
            ("AGSTUDIO_EXTENDED_DEADLINE_SECS", "0"),
        ])
        .unwrap();
        assert_eq!(c.solve_deadline, Duration::from_secs(90));
        assert_eq!(c.extended_deadline, None);
        assert_eq!(
            c.request_timeout(),
            REQUEST_TIMEOUT_MIN.max(
                Duration::from_secs(90)
                    + crate::worker::GRACE
                    + c.queue_wait
                    + Duration::from_secs(30)
            )
        );
        let c = cfg(&[
            ("AGSTUDIO_SOLVE_DEADLINE_SECS", "1"),
            ("AGSTUDIO_EXTENDED_DEADLINE_SECS", "5000"),
        ])
        .unwrap();
        assert_eq!(c.solve_deadline, Duration::from_secs(10));
        assert_eq!(c.extended_deadline, Some(Duration::from_secs(900)));
        let c = cfg(&[
            ("AGSTUDIO_SOLVE_DEADLINE_SECS", "200"),
            ("AGSTUDIO_EXTENDED_DEADLINE_SECS", "120"),
        ])
        .unwrap();
        assert_eq!(c.extended_deadline, None, "a longer search must be longer");
    }

    #[test]
    fn unusable_basic_auth_refuses_to_start() {
        for bad in ["", ":", "short", ":short", "user:short", "user:"] {
            assert!(
                cfg(&[("AGSTUDIO_BASIC_AUTH", bad)]).is_err(),
                "{bad:?} must fail closed"
            );
        }
        assert!(cfg(&[("AGSTUDIO_BASIC_AUTH", "longenough")])
            .unwrap()
            .basic_auth
            .is_some());
    }

    #[test]
    fn the_published_template_password_refuses_to_start() {
        let template = include_str!("../../deploy/.env.example");
        let shipped: Vec<&str> = template
            .lines()
            .filter_map(|l| {
                l.trim()
                    .strip_prefix("#")
                    .unwrap_or(l)
                    .trim()
                    .strip_prefix("AGSTUDIO_BASIC_AUTH=")
            })
            .collect();
        assert!(
            !shipped.is_empty(),
            "the template documents AGSTUDIO_BASIC_AUTH"
        );
        for spec in shipped {
            let err = BasicAuth::parse(spec)
                .err()
                .unwrap_or_else(|| panic!("{spec:?} must fail closed"));
            assert!(err.contains("openssl rand -hex 16"), "{err}");
        }
        assert!(
            !template
                .lines()
                .any(|l| l.trim_start().starts_with("AGSTUDIO_BASIC_AUTH=")),
            "an unedited copy must leave the password unset, so a public bind refuses to start"
        );
        for bad in [
            ":CHANGE-ME-run-openssl-rand-hex-16",
            "op:change-me-please",
            "Change-Me-12345",
        ] {
            assert!(BasicAuth::parse(bad).is_err(), "{bad:?}");
        }
        let unedited = cfg(&[
            ("AGSTUDIO_BIND", "0.0.0.0:8787"),
            ("AGSTUDIO_TRUST_PROXY", "1"),
        ]);
        assert!(unedited.err().unwrap().contains("refusing to bind"));
    }

    #[test]
    fn flags_parse_in_any_case_and_refuse_anything_else() {
        let on = cfg(&[
            ("AGSTUDIO_SECURE_COOKIES", "True"),
            ("AGSTUDIO_TRUST_PROXY", " ON "),
        ])
        .unwrap();
        assert!(on.secure_cookies && on.trust_proxy);
        let off = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GUEST_MODE", "False"),
        ])
        .unwrap();
        assert!(
            !off.guest_allowed(),
            "GUEST_MODE=False must mean off, not the default"
        );
        let no_ai = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GUEST_AI", "NO"),
        ])
        .unwrap();
        assert!(!no_ai.guest_ai_allowed());
        assert!(
            !cfg(&[("AGSTUDIO_DISABLE_TRANSLATE", "Yes")])
                .unwrap()
                .enable_translate
        );
        assert!(
            cfg(&[("AGSTUDIO_SECURE_COOKIES", "")]).is_ok(),
            "empty = unset"
        );
        for k in [
            "AGSTUDIO_SECURE_COOKIES",
            "AGSTUDIO_TRUST_PROXY",
            "AGSTUDIO_GUEST_MODE",
            "AGSTUDIO_GUEST_AI",
            "AGSTUDIO_DISABLE_TRANSLATE",
            "AGSTUDIO_ALLOW_INSECURE",
            "AGSTUDIO_HUMAN_PROOFS",
        ] {
            for bad in ["2", "enabled", "tru", "o n"] {
                let err = cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough"), (k, bad)]).err();
                assert!(
                    err.as_deref().is_some_and(|e| e.contains(k)),
                    "{k}={bad:?} must refuse to start: {err:?}"
                );
            }
        }
        assert_eq!(parse_flag("Off"), Some(false));
        assert_eq!(parse_flag("1 "), Some(true));
        assert_eq!(parse_flag("nope"), None);
    }

    #[test]
    fn numbers_refuse_to_start_when_unparseable() {
        for k in [
            "AGSTUDIO_MAX_CONCURRENT",
            "AGSTUDIO_RATE_PER_MIN",
            "AGSTUDIO_TRANSLATE_PER_MIN",
            "AGSTUDIO_AUTH_PER_MIN",
            "AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN",
            "AGSTUDIO_GUEST_AI_PER_DAY",
            "AGSTUDIO_AI_PER_DAY_TOTAL",
            "AGSTUDIO_GATE_DAYS",
            "AGSTUDIO_MAX_BODY_KB",
            "AGSTUDIO_MAX_INPUT_CHARS",
            "AGSTUDIO_QUEUE_WAIT_SECS",
            "AGSTUDIO_WORKER_MEM_MB",
        ] {
            for bad in ["four", "6O", "-1", "1.5"] {
                let err = cfg(&[(k, bad)]).err();
                assert!(
                    err.as_deref().is_some_and(|e| e.contains(k)),
                    "{k}={bad:?} must refuse to start: {err:?}"
                );
            }
        }
        assert!(
            cfg(&[("AGSTUDIO_RATE_PER_MIN", "4294967296")]).is_err(),
            "no silent wrap to 0 = off"
        );
        let c = cfg(&[
            ("AGSTUDIO_MAX_CONCURRENT", " 3 "),
            ("AGSTUDIO_RATE_PER_MIN", "60"),
            ("AGSTUDIO_MAX_BODY_KB", ""),
        ])
        .unwrap();
        assert_eq!(
            (c.max_concurrent, c.rate_per_min, c.max_body_bytes),
            (3, 60, 8192 * 1024)
        );
    }

    #[test]
    fn the_banner_shows_secure_cookies_and_the_version() {
        let off = cfg(&[]).unwrap().summary();
        assert!(off.contains("secure cookies: off"), "{off}");
        assert!(off.contains(env!("CARGO_PKG_VERSION")), "{off}");
        let on = cfg(&[("AGSTUDIO_SECURE_COOKIES", "1")]).unwrap().summary();
        assert!(on.contains("secure cookies: on"), "{on}");
    }

    #[test]
    fn secrets_can_come_from_files() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, body: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            p.to_string_lossy().into_owned()
        };
        let pw = write("pw", ":from-a-file-123\n");
        let key = write("key", &format!("{}\r\n", "ab".repeat(32)));
        let tok = write("tok", "sk-ant-oat-test\n");
        let c = cfg(&[
            ("AGSTUDIO_BASIC_AUTH_FILE", &pw),
            ("AGSTUDIO_GATE_KEY_FILE", &key),
            ("CLAUDE_CODE_OAUTH_TOKEN_FILE", &tok),
        ])
        .unwrap();
        let auth = c.basic_auth.as_ref().unwrap();
        assert!(
            auth.credentials_match(b"anyone", b"from-a-file-123"),
            "trailing newline stripped"
        );
        assert_eq!(c.gate_key, Some([0xab; 32]));
        assert_eq!(c.claude_oauth_token.as_deref(), Some("sk-ant-oat-test"));
        assert_eq!(
            cfg(&[("CLAUDE_CODE_OAUTH_TOKEN", "t")])
                .unwrap()
                .claude_oauth_token
                .as_deref(),
            Some("t")
        );

        let both = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_BASIC_AUTH_FILE", &pw),
        ]);
        assert!(both.err().unwrap().contains("not both"));
        let missing = dir.path().join("absent").to_string_lossy().into_owned();
        for k in [
            "AGSTUDIO_BASIC_AUTH_FILE",
            "AGSTUDIO_GATE_KEY_FILE",
            "CLAUDE_CODE_OAUTH_TOKEN_FILE",
        ] {
            let err = cfg(&[(k, &missing)]).err();
            assert!(
                err.as_deref().is_some_and(|e| e.contains(k)),
                "{k}: {err:?}"
            );
        }
        let weak = write("weak", "short\n");
        assert!(
            cfg(&[("AGSTUDIO_BASIC_AUTH_FILE", &weak)]).is_err(),
            "a file password is checked like any other"
        );
        let placeholder = write("ph", ":CHANGE-ME-run-openssl-rand-hex-16\n");
        assert!(cfg(&[("AGSTUDIO_BASIC_AUTH_FILE", &placeholder)]).is_err());
    }

    #[test]
    fn the_csp_names_each_inline_script_once() {
        let csp = content_security_policy();
        let hashes: Vec<&str> = csp
            .split_whitespace()
            .filter(|w| w.starts_with("'sha256-"))
            .collect();
        assert!(!hashes.is_empty(), "{csp}");
        let mut unique = hashes.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), hashes.len(), "{csp}");
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
        assert!(cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough")])
            .unwrap()
            .guest_allowed());
        let off = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GUEST_MODE", "0"),
        ]);
        assert!(!off.unwrap().guest_allowed());
        assert!(
            cfg(&[("AGSTUDIO_GUEST_MODE", "1")]).is_err(),
            "guest mode with no password"
        );
    }

    #[test]
    fn gate_settings_parse_and_fail_closed() {
        let c = cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough")]).unwrap();
        assert_eq!(c.gate, GateMode::Form, "the password page is the default");
        assert!(c.form_gate());
        assert_eq!(c.gate_days, 180);
        assert!(c.gate_key.is_none());
        assert!(
            c.summary()
                .starts_with("auth: shared password (form + Basic), guests allowed"),
            "{}",
            c.summary()
        );
        let b = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GATE", "basic"),
        ])
        .unwrap();
        assert_eq!(b.gate, GateMode::Basic);
        assert!(!b.form_gate());
        assert!(!cfg(&[]).unwrap().form_gate(), "no password, no gate");
        assert!(cfg(&[("AGSTUDIO_GATE", "cookie")]).is_err());
        let hex = "00112233445566778899aabbccddeeff00112233445566778899AABBCCDDEEFF";
        let k = cfg(&[("AGSTUDIO_GATE_KEY", hex)])
            .unwrap()
            .gate_key
            .unwrap();
        assert_eq!((k[0], k[1], k[31]), (0x00, 0x11, 0xff));
        for bad in [
            "",
            "abc",
            &hex[..63],
            &format!("{hex}0"),
            &hex.replace('0', "g"),
        ] {
            assert!(
                cfg(&[("AGSTUDIO_GATE_KEY", bad)]).is_err(),
                "{bad:?} must refuse to start"
            );
        }
        assert_eq!(cfg(&[("AGSTUDIO_GATE_DAYS", "0")]).unwrap().gate_days, 1);
        assert_eq!(
            cfg(&[("AGSTUDIO_GATE_DAYS", "9999")]).unwrap().gate_days,
            400
        );
    }

    #[test]
    fn guest_ai_follows_guest_mode_unless_set() {
        let on = cfg(&[("AGSTUDIO_BASIC_AUTH", ":longenough")]).unwrap();
        assert!(on.guest_ai_allowed());
        let off = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GUEST_AI", "0"),
        ])
        .unwrap();
        assert!(off.guest_allowed() && !off.guest_ai_allowed());
        let no_guests = cfg(&[
            ("AGSTUDIO_BASIC_AUTH", ":longenough"),
            ("AGSTUDIO_GUEST_MODE", "0"),
        ])
        .unwrap();
        assert!(!no_guests.guest_ai_allowed());
        let forced = cfg(&[("AGSTUDIO_GUEST_AI", "1")]).unwrap();
        assert!(
            !forced.guest_ai_allowed(),
            "never without the shared password"
        );
        assert_eq!(
            cfg(&[("AGSTUDIO_GUEST_AI_PER_DAY", "30")])
                .unwrap()
                .guest_ai_per_day,
            30
        );
    }

    #[test]
    fn cred_digest_changes_with_user_or_password() {
        let d = |s: &str| BasicAuth::parse(s).unwrap().cred_digest();
        assert_eq!(d(":hunter2hunter2"), d("hunter2hunter2"));
        assert_ne!(d(":hunter2hunter2"), d(":hunter2hunter3"));
        assert_ne!(d(":hunter2hunter2"), d("op:hunter2hunter2"));
        assert_ne!(d("op:hunter2hunter2"), d("ops:hunter2hunter2"));
        let a = BasicAuth::parse("op:hunter2hunter2").unwrap();
        assert!(a.has_user());
        assert!(a.credentials_match(b"op", b"hunter2hunter2"));
        assert!(!a.credentials_match(b"GeoSolver", b"hunter2hunter2"));
        let any = BasicAuth::parse(":hunter2hunter2").unwrap();
        assert!(!any.has_user());
        assert!(any.credentials_match(b"GeoSolver", b"hunter2hunter2"));
    }

    #[test]
    fn hmac_matches_rfc_4231_style_vector() {
        let key = [0x0bu8; 32];
        let mac = hmac_sha256(&key, b"Hi There");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "198a607eb44bfbc69903a0f1cf2bbdc5ba0aa3f3d9ae3c1c7a3b1696a0b68cf7"
        );
    }

    #[test]
    fn public_paths_match_exactly() {
        use axum::http::Method;
        assert!(is_public(&Method::GET, "/icons/icon-192.png"));
        assert!(!is_public(&Method::GET, "/icons/"));
        assert!(!is_public(&Method::GET, "/icons/a/b"));
        assert!(!is_public(&Method::GET, "/assets/app.js"));
        assert!(!is_public(&Method::GET, "/assets/app.css/x"));
        assert!(!is_public(&Method::GET, "/healthz/"));
        assert!(is_public(&Method::POST, "/gate"));
        assert!(!is_public(&Method::POST, "/manifest.webmanifest"));
        assert!(!is_public(&Method::DELETE, "/gate"));
    }

    #[test]
    fn trust_proxy_defaults_off_and_public_hosts_parse() {
        let c = cfg(&[]).unwrap();
        assert!(!c.trust_proxy);
        assert!(c.public_hosts.is_empty());
        let c = cfg(&[
            ("AGSTUDIO_TRUST_PROXY", "1"),
            (
                "AGSTUDIO_PUBLIC_HOST",
                " Geo.Tail1.ts.net:443 , other.example ,",
            ),
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
        assert_eq!(
            client_ip(&c, &spoofed),
            ip("127.0.0.1"),
            "trust is off by default"
        );
        c.trust_proxy = true;
        assert_eq!(client_ip(&c, &spoofed), ip("203.0.113.5"));
        let split = req_from("127.0.0.1:4000", &["6.6.6.6", "203.0.113.5"]);
        assert_eq!(client_ip(&c, &split), ip("203.0.113.5"));
        let tailnet = req_from("100.101.102.103:4000", &["203.0.113.5"]);
        assert_eq!(client_ip(&c, &tailnet), ip("203.0.113.5"));
        let direct = req_from("198.51.100.20:4000", &["203.0.113.5"]);
        assert_eq!(
            client_ip(&c, &direct),
            ip("198.51.100.20"),
            "untrusted peer: XFF ignored"
        );
        let garbage = req_from("10.0.0.2:4000", &["203.0.113.5, not-an-ip"]);
        assert_eq!(
            client_ip(&c, &garbage),
            ip("10.0.0.2"),
            "invalid XFF: the peer, per request"
        );
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

/// A simple per-IP fixed-window limiter with five buckets (general, translate,
/// credential-guessing-sensitive auth, failed shared passwords, and guest
/// translations per day), memory bounded by a periodic sweep. Keys come from
/// [`client_ip`] (IPv6 already cut to /64).
#[derive(Default)]
pub struct RateLimiter {
    general: HashMap<IpAddr, Window>,
    translate: HashMap<IpAddr, Window>,
    auth: HashMap<IpAddr, Window>,
    basic_fail: HashMap<IpAddr, Window>,
    guest_ai_day: HashMap<IpAddr, Window>,
    ai_day_total: HashMap<(), Window>,
    last_sweep: Option<Instant>,
}

const WINDOW: Duration = Duration::from_secs(60);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

impl RateLimiter {
    fn hit(map: &mut HashMap<IpAddr, Window>, ip: IpAddr, limit: u32, now: Instant) -> bool {
        Self::hit_in(map, ip, limit, now, WINDOW)
    }

    fn hit_in<K: std::hash::Hash + Eq>(
        map: &mut HashMap<K, Window>,
        ip: K,
        limit: u32,
        now: Instant,
        window: Duration,
    ) -> bool {
        if limit == 0 {
            return true;
        }
        let w = map.entry(ip).or_insert(Window {
            count: 0,
            start: now,
        });
        if now.duration_since(w.start) >= window {
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
            for m in [
                &mut self.general,
                &mut self.translate,
                &mut self.auth,
                &mut self.basic_fail,
            ] {
                m.retain(|_, w| now.duration_since(w.start) < WINDOW);
            }
            self.guest_ai_day
                .retain(|_, w| now.duration_since(w.start) < DAY);
            self.last_sweep = Some(now);
        }
    }
}

impl AppState {
    /// `ip` has used up its wrong-password budget for this minute.
    pub fn password_guesses_exhausted(&self, ip: IpAddr) -> bool {
        RateLimiter::exhausted(
            &relock(&self.rate).basic_fail,
            ip,
            self.config.basic_auth_fails_per_min,
            Instant::now(),
        )
    }

    /// Count one wrong shared password (Basic or the gate form) for `ip`.
    pub fn count_wrong_password(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut r = relock(&self.rate);
        r.sweep(now);
        RateLimiter::hit(
            &mut r.basic_fail,
            ip,
            self.config.basic_auth_fails_per_min,
            now,
        );
    }

    /// Count one translation or AI explanation from `ip`, by a guest or an
    /// account, against `AGSTUDIO_GUEST_AI_PER_DAY` and the server-wide
    /// `AGSTUDIO_AI_PER_DAY_TOTAL`; false once either day's budget is spent.
    pub fn ai_call_allowed(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut r = relock(&self.rate);
        r.sweep(now);
        RateLimiter::hit_in(
            &mut r.guest_ai_day,
            ip,
            self.config.guest_ai_per_day,
            now,
            DAY,
        ) && RateLimiter::hit_in(
            &mut r.ai_day_total,
            (),
            self.config.ai_per_day_total,
            now,
            DAY,
        )
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
pub(crate) fn client_ip(cfg: &Config, req: &Request<Body>) -> IpAddr {
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
            Some((name, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => {
                name
            }
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
    if (!cfg.bind.ip().is_loopback() && cfg.public_hosts.is_empty())
        || req.uri().path() == "/healthz"
    {
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

/// Paths anyone may fetch without the shared password (GET/HEAD; `/gate`
/// and `/gate/forget` also POST): the gate page itself and the static,
/// secret-free files a home-screen install fetches without cookies. A
/// trailing `{file}` matches one path segment.
pub const PUBLIC_PATHS: &[&str] = &[
    "/healthz",
    "/readyz",
    "/gate",
    "/gate/forget",
    "/manifest.webmanifest",
    "/favicon.svg",
    "/favicon.ico",
    "/apple-touch-icon.png",
    "/apple-touch-icon-precomposed.png",
    "/icons/{file}",
    "/splash/{file}",
    "/assets/app.css",
    "/assets/fonts/{file}",
    "/robots.txt",
    "/licenses",
];

fn is_public(method: &axum::http::Method, path: &str) -> bool {
    use axum::http::Method;
    if *method == Method::POST {
        return path == "/gate" || path == "/gate/forget";
    }
    if *method != Method::GET && *method != Method::HEAD {
        return false;
    }
    PUBLIC_PATHS.iter().any(|p| match p.strip_suffix("{file}") {
        Some(dir) => path
            .strip_prefix(dir)
            .is_some_and(|f| !f.is_empty() && !f.contains('/')),
        None => *p == path,
    })
}

/// A top-level page load, as opposed to a `fetch()` or a subresource.
pub fn is_navigation(req: &Request<Body>) -> bool {
    use axum::http::Method;
    if *req.method() != Method::GET && *req.method() != Method::HEAD {
        return false;
    }
    match req.headers().get("sec-fetch-mode") {
        Some(m) => m.as_bytes() == b"navigate",
        None => req
            .headers()
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|a| a.contains("text/html")),
    }
}

/// How a request got past the shared password, for `/api/status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateVia {
    Cookie,
    Basic,
}

const BASIC_CHALLENGE: &str = "Basic realm=\"GeoSolver\", charset=\"UTF-8\"";

fn basic_challenge() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, BASIC_CHALLENGE)],
        "Authentication required.",
    )
        .into_response()
}

/// A request that has not presented the shared password, under the form
/// gate: a page load goes to `/gate`; an API call gets JSON `code: "gate"`.
/// Only clients that send no `Sec-Fetch-Mode` (curl, scripts) get a Basic
/// challenge: on a browser `fetch()` it would pop Safari's credentials sheet.
fn gate_reject(req: &Request<Body>, nav: bool) -> Response {
    if nav {
        let target = req.uri().path_and_query().map_or("/", |pq| pq.as_str());
        if crate::gate::came_from_gate(req.headers()) {
            return crate::gate::redirect_to_gate_cookie_lost(target);
        }
        return crate::gate::redirect_to_gate(target);
    }
    let lang = crate::i18n::lang_from_headers(req.headers());
    let mut resp = if req.uri().path().starts_with("/api/") {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": crate::i18n::t(lang, "gate.required"), "code": "gate" })),
        )
            .into_response()
    } else {
        (StatusCode::UNAUTHORIZED, "Authentication required.").into_response()
    };
    if !req.headers().contains_key("sec-fetch-mode") {
        resp.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(BASIC_CHALLENGE),
        );
    }
    resp
}

/// The shared-password gate. With `AGSTUDIO_GATE=form` (the default) a valid
/// `gate` cookie passes, then HTTP Basic credentials; anything else is sent to
/// the `/gate` page (see [`gate_reject`]). With `AGSTUDIO_GATE=basic` only
/// Basic counts and every refusal is the Basic challenge. Wrong passwords
/// (Basic or the form) share one per-IP budget; past
/// `basic_auth_fails_per_min` the IP gets 429 without its credentials even
/// being checked. A request with no `Authorization` at all is not counted.
pub async fn auth(State(state): State<Shared>, mut req: Request<Body>, next: Next) -> Response {
    let cfg = &state.config;
    let Some(expected) = &cfg.basic_auth else {
        return next.run(req).await;
    };
    let form = cfg.gate == GateMode::Form;
    let path = req.uri().path();
    if path == "/healthz" || path == "/readyz" || (form && is_public(req.method(), path)) {
        return next.run(req).await;
    }
    let nav = is_navigation(&req);
    if form {
        if let Some(iat) = crate::gate::cookie_iat(&state, req.headers()) {
            req.extensions_mut().insert(GateVia::Cookie);
            let mut resp = next.run(req).await;
            if nav && crate::gate::due_for_renewal(iat) {
                crate::gate::append_cookie(&state, &mut resp);
            }
            return resp;
        }
    }
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .map(|v| v.to_str().unwrap_or("").to_string());
    if let Some(got) = presented {
        let ip = client_ip(cfg, &req);
        if state.password_guesses_exhausted(ip) {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, "60")],
                "Too many failed logins; try again in a minute.",
            )
                .into_response();
        }
        if expected.matches(&got) {
            req.extensions_mut().insert(GateVia::Basic);
            let mut resp = next.run(req).await;
            if form && nav {
                crate::gate::append_cookie(&state, &mut resp);
            }
            return resp;
        }
        state.count_wrong_password(ip);
    }
    if form {
        gate_reject(&req, nav)
    } else {
        basic_challenge()
    }
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
        self.config
            .max_concurrent
            .saturating_sub(1)
            .max(self.per_caller_limit())
    }

    pub fn claim_within(&self, key: String, limit: usize) -> Option<CallerSlot> {
        let mut m = relock(&self.inflight);
        let n = m.entry(key.clone()).or_insert(0);
        if *n >= limit {
            return None;
        }
        *n += 1;
        Some(CallerSlot {
            key,
            map: self.inflight.clone(),
        })
    }
}

pub async fn rate_limit(
    State(state): State<Shared>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
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
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..open_end];
        let body_start = open_end + 1;
        let Some(close) = rest[body_start..].find("</script>") else {
            break;
        };
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
        let mut hashes: Vec<String> = Vec::new();
        for h in pages.iter().flat_map(|p| inline_scripts(p)).map(csp_hash) {
            if !hashes.contains(&h) {
                hashes.push(h);
            }
        }
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
    set(h, header::REFERRER_POLICY, "same-origin");
    set(
        h,
        header::CONTENT_SECURITY_POLICY,
        content_security_policy(),
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

/// Constant-time comparison of two equal-length digests.
pub(crate) fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// HMAC-SHA256 (RFC 2104) with a 32-byte key.
pub(crate) fn hmac_sha256(key: &[u8; 32], msg: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for (i, b) in key.iter().enumerate() {
        ipad[i] ^= b;
        opad[i] ^= b;
    }
    let inner = Sha256::new()
        .chain_update(ipad)
        .chain_update(msg)
        .finalize();
    Sha256::new()
        .chain_update(opad)
        .chain_update(inner)
        .finalize()
        .into()
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
