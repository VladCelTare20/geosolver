//! Killable solves: the web and MCP servers run each solve in a short-lived
//! `agstudio __solve-worker` child process ([`run`] parent side,
//! [`worker_main`] child side). Design notes: `src/CODEMAP.md`.

use std::io::{Read, Write};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::OwnedSemaphorePermit;

use crate::engine::{self, InputKind, Method, Solution, SolveOptions, Status};
use crate::render;
use ddar::svg::Theme;

/// The hidden subcommand that runs one solve and exits.
pub const SUBCOMMAND: &str = "__solve-worker";

/// Slack past a solve's own deadline before it is killed: room for the figure,
/// proof extraction and report rendering that follow the last cooperative check.
pub const GRACE: Duration = if cfg!(test) {
    Duration::from_millis(1500)
} else {
    Duration::from_secs(5)
};

/// Exit status of a process stopped by its own hard-deadline watchdog (the
/// coreutils `timeout` convention).
pub const EXIT_HARD_LIMIT: i32 = 124;

const DEFAULT_MEM_MB: u64 = 2048;
const MAX_REQUEST_BYTES: u64 = 16 << 20;
const MAX_REPLY_BYTES: u64 = 256 << 20;
/// Workers run below the server's priority, so `/healthz` and page loads stay
/// fast while every slot is busy.
#[cfg(unix)]
const WORKER_NICE: libc::c_int = 10;
/// How long to wait for a SIGKILLed child to be reaped before handing it to a
/// background reaper (which keeps holding the permit until it is gone).
const REAP_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    /// `engine::solve_within` with `secs` as the deadline.
    Solve,
    /// `engine::solve_best` with `secs` as the budget.
    Best,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Format {
    Pdf,
    Png,
}

/// One solve, as sent to the worker on stdin.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub input: String,
    pub low_level: bool,
    pub light: bool,
    pub want_proof: bool,
    pub title: Option<String>,
    pub panel: bool,
    pub mode: Mode,
    /// The cooperative deadline (`Solve`) or search budget (`Best`), seconds.
    pub secs: f64,
    /// Also render the light report page (figure + proof) in this format.
    #[serde(default)]
    pub report: Option<Format>,
    /// Also render the figure alone as a PNG at this scale.
    #[serde(default)]
    pub figure_png_scale: Option<f32>,
}

impl Request {
    pub fn new(input: &str, opts: &SolveOptions, mode: Mode, limit: Duration) -> Request {
        Request {
            input: input.to_string(),
            low_level: opts.kind == InputKind::LowLevel,
            light: opts.theme == Theme::Light,
            want_proof: opts.want_proof,
            title: opts.title.clone(),
            panel: opts.panel,
            mode,
            secs: limit.as_secs_f64(),
            report: None,
            figure_png_scale: None,
        }
    }

    fn options(&self) -> SolveOptions {
        SolveOptions {
            kind: if self.low_level { InputKind::LowLevel } else { InputKind::Geo },
            theme: if self.light { Theme::Light } else { Theme::Dark },
            want_proof: self.want_proof,
            title: self.title.clone(),
            panel: self.panel,
        }
    }

    /// The solve's own (cooperative) deadline or budget.
    pub fn limit(&self) -> Duration {
        let s = if self.secs.is_finite() { self.secs.clamp(0.0, 3600.0) } else { 0.0 };
        Duration::from_secs_f64(s)
    }

    /// When the parent kills the worker: the solve's limit plus [`GRACE`].
    pub fn hard_limit(&self) -> Duration {
        self.limit() + GRACE
    }
}

/// The worker's answer, one JSON line on stdout. Binary outputs are base64.
#[derive(Serialize, Deserialize)]
pub struct Reply {
    pub result: Result<Solution, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<Result<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure_png: Option<String>,
}

impl Reply {
    /// The rendered report, when one was requested and the solve succeeded.
    pub fn report_bytes(&self) -> Option<Result<Vec<u8>, String>> {
        self.report.as_ref().map(|r| match r {
            Ok(b64) => decode(b64).ok_or_else(|| "the report could not be decoded".to_string()),
            Err(e) => Err(e.clone()),
        })
    }
}

fn encode(bytes: Vec<u8>) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn decode(b64: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(b64).ok()
}

/// How a worker run ended.
pub enum Outcome {
    /// The worker answered (its `result` may still be an input error).
    Done(Box<Reply>),
    /// The worker overran its hard limit and was killed.
    TimedOut,
    /// The worker could not start, crashed, hit a resource limit, or answered
    /// garbage. Operator-facing detail.
    Failed(String),
}

/// The honest answer for a solve that had to be killed: not proved, with a
/// note saying why. No figure — drawing it is part of what overran.
pub fn time_limit_solution(input: &str, limit: Duration) -> Solution {
    Solution {
        input: input.to_string(),
        low_level: String::new(),
        proved: false,
        status: Status::NotProved,
        method: Method::Ddar,
        proof: None,
        numeric_evidence: None,
        numeric_samples: None,
        svg: String::new(),
        aux_constructions: Vec::new(),
        constructions: Vec::new(),
        goal: None,
        goal_holds_numerically: None,
        elapsed_secs: (limit + GRACE).as_secs_f64(),
        note: format!(
            "not proved — stopped at the {:.0}s time limit (the solver overran it and was terminated)",
            limit.as_secs_f64()
        ),
        proof_steps: None,
        examined: None,
    }
}

// ------------------------------------------------------------- parent ----

/// The child process and the caller's permit, released together: the permit
/// is dropped only after the child is reaped.
struct Slot {
    child: Option<tokio::process::Child>,
    /// Process-group id (= the child's pid); cleared once reaped so a recycled
    /// pid is never signalled.
    pgid: Option<i32>,
    permit: Option<OwnedSemaphorePermit>,
}

impl Slot {
    fn kill_group(&self) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid.filter(|p| *p > 1) {
            // SAFETY: plain syscall; the group is ours and its leader is not
            // reaped yet, so the id cannot have been recycled.
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }
    }

    /// Kill whatever is left of the group, reap the child, free the permit.
    async fn finish(&mut self) -> String {
        self.kill_group();
        let Some(child) = self.child.as_mut() else {
            return "no child".into();
        };
        match tokio::time::timeout(REAP_WAIT, child.wait()).await {
            Ok(status) => {
                self.child = None;
                self.pgid = None;
                self.permit = None;
                match status {
                    Ok(s) => s.to_string(),
                    Err(e) => format!("wait failed: {e}"),
                }
            }
            // Unkillable for now (e.g. stuck in the kernel): `Drop` hands it to
            // a background reaper that keeps the permit until it is gone.
            Err(_) => "not reaped yet".into(),
        }
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        self.kill_group();
        let permit = self.permit.take();
        if let Ok(Some(_)) = child.try_wait() {
            drop(permit);
            return;
        }
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let _ = child.wait().await;
                    drop(permit);
                });
            }
            // No runtime to reap on: `kill_on_drop` plus tokio's orphan reaper.
            Err(_) => {
                drop(child);
                drop(permit);
            }
        }
    }
}

/// Run one solve in a fresh worker process, holding `permit` (if any) until
/// the worker is gone. Never takes longer than `req.hard_limit()` plus the
/// time to reap a SIGKILLed process.
pub async fn run(req: &Request, permit: Option<OwnedSemaphorePermit>) -> Outcome {
    let body = match serde_json::to_vec(req) {
        Ok(b) => b,
        Err(e) => return Outcome::Failed(format!("could not encode the solve request: {e}")),
    };
    let hard = req.hard_limit();
    let mut cmd = match worker_command() {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(format!("could not locate the solver executable: {e}")),
    };
    configure(&mut cmd, hard);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Outcome::Failed(format!("could not start the solver process: {e}")),
    };
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let mut slot = Slot {
        pgid: child.id().and_then(|p| i32::try_from(p).ok()),
        child: Some(child),
        permit,
    };
    let exchange = async move {
        if let Some(mut w) = stdin {
            w.write_all(&body).await?;
            w.shutdown().await?;
        }
        let mut out = Vec::new();
        if let Some(r) = stdout {
            r.take(MAX_REPLY_BYTES).read_to_end(&mut out).await?;
        }
        Ok::<_, std::io::Error>(out)
    };
    let out = match tokio::time::timeout(hard, exchange).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            let status = slot.finish().await;
            return Outcome::Failed(format!("talking to the solver process failed: {e} ({status})"));
        }
        Err(_) => {
            slot.finish().await;
            return Outcome::TimedOut;
        }
    };
    let status = slot.finish().await;
    match parse_reply(&out) {
        Some(reply) => Outcome::Done(Box::new(reply)),
        None => Outcome::Failed(format!("the solver process ended without a result ({status})")),
    }
}

/// The last non-empty stdout line is the reply (the test harness prints its
/// own banner before it).
fn parse_reply(out: &[u8]) -> Option<Reply> {
    let line = out
        .split(|b| *b == b'\n')
        .rev()
        .find(|l| !l.iter().all(u8::is_ascii_whitespace))?;
    serde_json::from_slice(line).ok()
}

fn configure(cmd: &mut tokio::process::Command, hard: Duration) {
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        cmd.process_group(0);
        let mem = mem_limit_bytes();
        let cpu = cpu_limit_secs(hard);
        // SAFETY: only async-signal-safe syscalls run between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                cap(libc::RLIMIT_CORE, 0, 0)?;
                if let Some(bytes) = mem {
                    cap(libc::RLIMIT_AS, bytes, bytes)?;
                }
                cap(libc::RLIMIT_CPU, cpu, cpu + 5)?;
                libc::setpriority(libc::PRIO_PROCESS, 0, WORKER_NICE);
                Ok(())
            });
        }
    }
    #[cfg(not(unix))]
    let _ = hard;
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
type Resource = libc::__rlimit_resource_t;
#[cfg(all(unix, not(all(target_os = "linux", target_env = "gnu"))))]
type Resource = libc::c_int;

/// Lower a resource limit (never above the inherited hard limit).
#[cfg(unix)]
fn cap(resource: Resource, soft: u64, hard: u64) -> std::io::Result<()> {
    let mut cur = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: plain syscalls on a stack value.
    unsafe {
        if libc::getrlimit(resource, &mut cur) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let max = cur.rlim_max;
        let new = libc::rlimit {
            rlim_cur: (soft as libc::rlim_t).min(max),
            rlim_max: (hard as libc::rlim_t).min(max),
        };
        if libc::setrlimit(resource, &new) != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// `AGSTUDIO_WORKER_MEM_MB` (default 2048; 0 = no limit) in bytes.
fn mem_limit_bytes() -> Option<u64> {
    let mb = std::env::var("AGSTUDIO_WORKER_MEM_MB")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_MEM_MB);
    (mb > 0).then(|| mb.saturating_mul(1 << 20))
}

/// CPU seconds a worker may burn: every solver thread busy for the whole hard
/// limit, plus slack. A backstop behind the wall-clock kill, never the bound.
fn cpu_limit_secs(hard: Duration) -> u64 {
    let threads = std::env::var("RAYON_NUM_THREADS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(4, |n| n.get() as u64)
        });
    (hard.as_secs_f64() * (threads + 1) as f64).ceil() as u64 + 10
}

/// The production worker: this same executable. `/proc/self/exe` keeps working
/// even if the binary on disk is replaced while the server runs.
#[cfg(not(test))]
fn worker_command() -> std::io::Result<tokio::process::Command> {
    #[cfg(target_os = "linux")]
    let exe = {
        let proc_exe = std::path::Path::new("/proc/self/exe");
        if proc_exe.exists() {
            proc_exe.to_path_buf()
        } else {
            std::env::current_exe()?
        }
    };
    #[cfg(not(target_os = "linux"))]
    let exe = std::env::current_exe()?;
    let mut cmd = tokio::process::Command::new(exe);
    #[cfg(unix)]
    cmd.arg0("agstudio");
    cmd.arg(SUBCOMMAND);
    Ok(cmd)
}

#[cfg(test)]
const TEST_ENTRY: &str = "worker::tests::worker_process_entry";
#[cfg(test)]
const TEST_ENTRY_ENV: &str = "AGSTUDIO_TEST_WORKER_PROCESS";

#[cfg(test)]
fn worker_command() -> std::io::Result<tokio::process::Command> {
    let mut cmd = tokio::process::Command::new(std::env::current_exe()?);
    cmd.args([TEST_ENTRY, "--exact", "--nocapture", "--test-threads=1", "-q"])
        .env(TEST_ENTRY_ENV, "1");
    Ok(cmd)
}

// ------------------------------------------------------------- worker ----

/// `agstudio __solve-worker`: read one [`Request`] from stdin, solve, write one
/// [`Reply`] line to stdout. Returns the process exit status.
pub fn worker_main() -> i32 {
    let mut raw = Vec::new();
    if let Err(e) = std::io::stdin().take(MAX_REQUEST_BYTES).read_to_end(&mut raw) {
        eprintln!("solve worker: reading the request failed: {e}");
        return 2;
    }
    let req: Request = match serde_json::from_slice(&raw) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("solve worker: bad request: {e}");
            return 2;
        }
    };
    crate::security::apply_process_limits();
    // The parent kills at `hard_limit`; this covers a parent that is gone.
    arm_watchdog(
        req.hard_limit() + GRACE,
        format!("solve worker {}", std::process::id()),
    );
    #[cfg(all(test, unix))]
    tests::hooks(&req.input);
    let reply = match catch_unwind(AssertUnwindSafe(|| execute(&req))) {
        Ok(r) => r,
        Err(_) => {
            eprintln!("solve worker: the solver panicked");
            return 101;
        }
    };
    let mut line = match serde_json::to_vec(&reply) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("solve worker: could not encode the reply: {e}");
            return 1;
        }
    };
    line.push(b'\n');
    let mut out = std::io::stdout().lock();
    match out.write_all(&line).and_then(|()| out.flush()) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn execute(req: &Request) -> Reply {
    let opts = req.options();
    let result = match req.mode {
        Mode::Solve => engine::solve_within(&req.input, &opts, Some(req.limit())),
        Mode::Best => engine::solve_best(&req.input, &opts, req.limit()),
    };
    let report = match (&result, req.report) {
        (Ok(sol), Some(format)) => {
            let page = render::report_svg(sol, req.title.as_deref(), true);
            let bytes = match format {
                Format::Pdf => render::svg_to_pdf(&page),
                Format::Png => render::svg_to_png(&page, 2.0),
            };
            Some(bytes.map(encode).map_err(|e| format!("{e:#}")))
        }
        _ => None,
    };
    let figure_png = match (&result, req.figure_png_scale) {
        (Ok(sol), Some(scale)) => render::svg_to_png(&sol.svg, scale).ok().map(encode),
        _ => None,
    };
    Reply { result, report, figure_png }
}

/// Exit the whole process with [`EXIT_HARD_LIMIT`] once `limit` has passed,
/// whatever the other threads are doing. Used by the worker and by the CLI's
/// `render`/`best`/`translate --solve`.
pub fn arm_watchdog(limit: Duration, what: String) {
    let spawned = std::thread::Builder::new()
        .name("hard-deadline".into())
        .spawn(move || {
            std::thread::sleep(limit);
            let _ = writeln!(
                std::io::stderr(),
                "error: {what} overran its hard time limit of {:.1}s and was stopped",
                limit.as_secs_f64()
            );
            hard_exit(EXIT_HARD_LIMIT);
        });
    if let Err(e) = spawned {
        eprintln!("warning: could not start the hard-deadline watchdog: {e}");
    }
}

/// Leave now: no atexit handlers or locks that a stuck thread might hold.
fn hard_exit(code: i32) -> ! {
    #[cfg(unix)]
    // SAFETY: `_exit` is always safe to call; it does not return.
    unsafe {
        libc::_exit(code)
    }
    #[cfg(not(unix))]
    std::process::exit(code)
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use std::time::Instant;
    use tokio::sync::Semaphore;

    const WATCHDOG_ENTRY: &str = "worker::tests::watchdog_process_entry";
    const WATCHDOG_ENV: &str = "AGSTUDIO_TEST_WATCHDOG_PROCESS";

    /// Test-only inputs, honoured by the worker only in `cfg(test)` builds:
    /// the production binary has no code path that reads them.
    /// `HANG <pidfile>`: write the pid, then sleep forever.
    pub const HANG: &str = "__agstudio_test_hang__";
    /// `LIMITS <file>`: write `/proc/self/limits`, the nice value and the
    /// process group, then answer normally.
    const LIMITS: &str = "__agstudio_test_limits__";
    /// Allocate far past `RLIMIT_AS`.
    const ALLOC: &str = "__agstudio_test_alloc__";

    pub const CIRCUMCENTER: &str =
        "A B C = triangle\nO = circumcenter(A, B, C)\nprove cong(O, A, O, B)";

    pub(super) fn hooks(input: &str) {
        if let Some(path) = input.strip_prefix(HANG) {
            write_pid(path.trim());
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
        if let Some(path) = input.strip_prefix(LIMITS) {
            let limits = std::fs::read_to_string("/proc/self/limits").unwrap_or_default();
            // SAFETY: plain syscalls.
            let (nice, pgrp) = unsafe {
                (libc::getpriority(libc::PRIO_PROCESS, 0), libc::getpgrp())
            };
            let pid = std::process::id();
            let text = format!("{limits}\nnice={nice}\npgrp={pgrp}\npid={pid}\n");
            std::fs::write(path.trim(), text).unwrap();
        }
        if input.starts_with(ALLOC) {
            let big = vec![1u8; 8 << 30];
            std::hint::black_box(&big);
        }
    }

    fn write_pid(path: &str) {
        if !path.is_empty() {
            let tmp = format!("{path}.tmp");
            std::fs::write(&tmp, std::process::id().to_string()).unwrap();
            std::fs::rename(tmp, path).unwrap();
        }
    }

    #[test]
    fn worker_process_entry() {
        if std::env::var_os(TEST_ENTRY_ENV).is_some() {
            std::process::exit(worker_main());
        }
    }

    #[test]
    fn watchdog_process_entry() {
        if std::env::var_os(WATCHDOG_ENV).is_some() {
            arm_watchdog(Duration::from_millis(300), "the test command".into());
            std::thread::sleep(Duration::from_secs(60));
            std::process::exit(0);
        }
    }

    /// Has `pid` been reaped? (A zombie still answers signal 0.)
    pub fn reaped(pid: i32) -> bool {
        // SAFETY: signal 0 only checks for existence.
        unsafe { libc::kill(pid, 0) != 0 }
    }

    /// Wait (bounded) for a hung test worker to report its pid.
    pub async fn wait_for_pid(path: &std::path::Path) -> i32 {
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(s) = std::fs::read_to_string(path) {
                if let Ok(pid) = s.trim().parse() {
                    return pid;
                }
            }
            assert!(Instant::now() < until, "the test worker never started");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Wait (bounded) until `pid` is reaped; returns how long that took.
    pub async fn wait_reaped(pid: i32, within: Duration) -> Duration {
        let t = Instant::now();
        while !reaped(pid) {
            assert!(t.elapsed() < within, "worker {pid} still exists after {within:?}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        t.elapsed()
    }

    pub fn hang_input(dir: &std::path::Path, name: &str) -> (String, std::path::PathBuf) {
        let pidfile = dir.join(name);
        (format!("{HANG} {}", pidfile.display()), pidfile)
    }

    fn request(input: &str, secs: f64) -> Request {
        Request::new(input, &SolveOptions::default(), Mode::Solve, Duration::from_secs_f64(secs))
    }

    async fn bounded<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(60), f)
            .await
            .expect("test exceeded 60 s")
    }

    #[tokio::test]
    async fn a_real_solve_round_trips_through_a_worker_process() {
        let mut req = request(CIRCUMCENTER, 30.0);
        req.report = Some(Format::Pdf);
        req.figure_png_scale = Some(1.0);
        let Outcome::Done(reply) = bounded(run(&req, None)).await else {
            panic!("the worker did not answer");
        };
        let sol = reply.result.as_ref().expect("solve error");
        assert!(sol.proved, "{}", sol.note);
        assert_eq!(sol.status, Status::Proved);
        assert!(reply.report_bytes().unwrap().unwrap().starts_with(b"%PDF"));
        assert!(decode(reply.figure_png.as_deref().unwrap()).unwrap().starts_with(b"\x89PNG"));
    }

    #[tokio::test]
    async fn input_errors_come_back_as_errors_not_failures() {
        let Outcome::Done(reply) = bounded(run(&request("this is not geo", 30.0), None)).await else {
            panic!("the worker did not answer");
        };
        assert!(reply.result.is_err());
    }

    #[tokio::test]
    async fn hard_deadline_kills_a_hung_worker_and_frees_the_permit() {
        let dir = tempfile::tempdir().unwrap();
        let (input, pidfile) = hang_input(dir.path(), "pid");
        let sem = std::sync::Arc::new(Semaphore::new(1));
        let permit = sem.clone().try_acquire_owned().unwrap();
        let req = request(&input, 0.5);
        let t = Instant::now();
        let outcome = bounded(run(&req, Some(permit))).await;
        let took = t.elapsed();
        assert!(matches!(outcome, Outcome::TimedOut), "expected a timeout");
        assert!(took >= req.hard_limit(), "killed early: {took:?}");
        assert!(took < req.hard_limit() + Duration::from_secs(2), "killed late: {took:?}");
        let pid = wait_for_pid(&pidfile).await;
        assert!(reaped(pid), "the hung worker {pid} is still around");
        assert_eq!(sem.available_permits(), 1, "the permit was not released");
    }

    #[tokio::test]
    async fn dropping_the_caller_kills_the_worker_and_frees_the_permit_only_after_reaping() {
        let dir = tempfile::tempdir().unwrap();
        let (input, pidfile) = hang_input(dir.path(), "pid");
        let sem = std::sync::Arc::new(Semaphore::new(1));
        let permit = sem.clone().try_acquire_owned().unwrap();
        let req = request(&input, 60.0);
        let task = tokio::spawn(async move { run(&req, Some(permit)).await });
        let pid = wait_for_pid(&pidfile).await;
        assert!(!reaped(pid));
        assert_eq!(sem.available_permits(), 0);
        task.abort();
        let t = Instant::now();
        bounded(async {
            loop {
                if sem.available_permits() == 1 {
                    assert!(reaped(pid), "permit released before worker {pid} was reaped");
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(t.elapsed() < Duration::from_secs(2), "slow release: {:?}", t.elapsed());
    }

    #[tokio::test]
    async fn workers_run_isolated_niced_and_resource_limited() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("limits");
        let input = format!("{LIMITS} {}", file.display());
        let Outcome::Done(_) = bounded(run(&request(&input, 30.0), None)).await else {
            panic!("the worker did not answer");
        };
        let text = std::fs::read_to_string(&file).unwrap();
        let row = |name: &str| -> Vec<String> {
            let line = text.lines().find(|l| l.starts_with(name)).unwrap_or_else(|| panic!("{name}: {text}"));
            line[name.len()..].split_whitespace().map(str::to_string).collect()
        };
        let mem = (DEFAULT_MEM_MB << 20).to_string();
        assert_eq!(row("Max address space")[..2], [mem.clone(), mem], "{text}");
        assert_ne!(row("Max cpu time")[0], "unlimited", "{text}");
        assert_eq!(row("Max core file size")[0], "0", "{text}");
        assert!(text.contains(&format!("nice={WORKER_NICE}")), "{text}");
        let field = |k: &str| text.lines().find_map(|l| l.strip_prefix(k)).unwrap().to_string();
        assert_eq!(field("pgrp="), field("pid="), "the worker must lead its own process group");
    }

    #[tokio::test]
    async fn a_worker_that_exceeds_its_memory_cap_fails_fast() {
        let t = Instant::now();
        let outcome = bounded(run(&request(ALLOC, 30.0), None)).await;
        assert!(matches!(outcome, Outcome::Failed(_)), "an 8 GiB allocation must kill the worker");
        assert!(t.elapsed() < Duration::from_secs(10), "{:?}", t.elapsed());
    }

    #[test]
    fn the_watchdog_ends_an_overrunning_process_with_a_clear_message() {
        use wait_timeout::ChildExt;
        let t = Instant::now();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([WATCHDOG_ENTRY, "--exact", "--nocapture", "--test-threads=1", "-q"])
            .env(WATCHDOG_ENV, "1")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let status = match child.wait_timeout(Duration::from_secs(20)).unwrap() {
            Some(s) => s,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the watchdog did not fire");
            }
        };
        let mut err = String::new();
        child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
        assert_eq!(status.code(), Some(EXIT_HARD_LIMIT), "{err}");
        assert!(err.contains("overran its hard time limit"), "{err}");
        assert!(t.elapsed() < Duration::from_secs(15));
    }

    #[test]
    fn requests_survive_the_json_round_trip_and_bad_limits_are_clamped() {
        let mut req = request(CIRCUMCENTER, 7.5);
        req.report = Some(Format::Png);
        let back: Request = serde_json::from_slice(&serde_json::to_vec(&req).unwrap()).unwrap();
        assert_eq!(back.limit(), Duration::from_secs_f64(7.5));
        assert_eq!(back.report, Some(Format::Png));
        req.secs = f64::NAN;
        assert_eq!(req.limit(), Duration::ZERO);
        req.secs = 1e12;
        assert_eq!(req.limit(), Duration::from_secs(3600));
    }

    #[test]
    fn the_reply_is_the_last_line_of_output() {
        let reply = Reply {
            result: Ok(time_limit_solution("x", Duration::from_secs(3))),
            report: None,
            figure_png: None,
        };
        let mut out = b"\nrunning 1 test\n".to_vec();
        out.extend(serde_json::to_vec(&reply).unwrap());
        out.extend(b"\n\n");
        let parsed = parse_reply(&out).unwrap();
        assert!(parsed.result.unwrap().note.contains("time limit"));
        assert!(parse_reply(b"garbage\n").is_none());
    }
}
