# ag-studio/src — the `agstudio` binary: CLI, web app, MCP server

Up: [../../CODEMAP.md](../../CODEMAP.md)

## Files
- `main.rs` — subcommands `render`/`best`/`translate`/`serve`/`mcp`, plus the hidden `__solve-worker`; unknown flags exit 2.
- `engine.rs` — solve wrapper: routes DDAR vs metric goals, reconciles a proof against the numeric check, `solve_within` deadline. `Status` (`proved` / `holds-numerically` / `refuted` / `not-proved`) is set only in `reconcile`; `proof` is `Some` only when proved, numeric reports go in `numeric_evidence`.
- `worker.rs` — killable solves: `run` (parent) spawns `agstudio __solve-worker`, `worker_main` (child) solves one JSON request and prints one JSON reply line; `arm_watchdog` is the hard-exit timer shared with the CLI.
- `render.rs` — SVG→PNG/PDF and the combined report page; scales long proofs down to fit pixel/page limits.
- `translate.rs` — photo/text → `.geo` via `claude -p`, locked down (no tools or Read-one-file, no settings/MCP/hooks, empty cwd, no API-key env).
- `web.rs` — axum routes and handlers.
- `security.rs` — env config (the env table is the source for `deploy/DEPLOY.md`), Basic auth, guest mode, rate limits, Host/Origin guards, CSP.
- `auth.rs` — argon2 (gated, dummy-hash for unknown users), session ids, cookies.
- `db.rs` — SQLite: users, sessions (hashed), history (with the verdict `status`; `NULL` on rows from before it existed); files created 0600.
- `mcp.rs` — stdio JSON-RPC MCP server; exports confined to `AGSTUDIO_EXPORT_DIR`.
- `i18n.rs` — server-side EN/RO strings.

## Notes
- `worker.rs` — why processes: a DDAR closure is not interruptible and `solve_within` checks its deadline only between closures, so an in-process timeout abandoned a thread that kept the CPU (and, in `web.rs`, its semaphore permit). Every web/MCP solve and export now runs in a worker; only the CLI solves in-process.
- `worker.rs:run` — child in its own process group, `kill_on_drop`, nice +10 (so `/healthz` stays fast under load), `RLIMIT_AS` = `AGSTUDIO_WORKER_MEM_MB` (default 2048; a 2-aux IMO solve peaks at ~1.2 GB virtual / ~140 MB RSS, the rest is glibc arenas), `RLIMIT_CPU` = threads × hard limit (backstop only), no core dumps. Hard limit = the solve's deadline/budget + `GRACE` (5 s); past it the group is SIGKILLed and the caller gets `Outcome::TimedOut`, reported as `not-proved` with a "time limit" note (web export: 504; MCP export: error).
- `worker.rs:Slot` — the caller's permit lives in the same guard as the child and is dropped only after the child is reaped: on success, on timeout, and when the future is dropped (HTTP client gone, `TimeoutLayer`, MCP `notifications/cancelled`), where a spawned task reaps the SIGKILLed child and then frees the slot.
- `worker.rs:worker_main` — arms its own watchdog at hard limit + `GRACE`, so an orphaned worker still exits; exits 101 on a panic, 124 from the watchdog.
- `worker.rs:worker_command` — production execs `/proc/self/exe` (survives the binary being replaced on disk). Under `cargo test` it re-runs the test harness filtered to `tests::worker_process_entry`, which is why the reply is read as the *last* stdout line; the test-only inputs (`__agstudio_test_hang__` etc.) exist only in `cfg(test)` builds.
- `web.rs:heavy_permit` — waits up to `AGSTUDIO_QUEUE_WAIT_SECS` (5) for a slot, then 503 + `Retry-After: 10`. `/healthz` and pages never touch the semaphore.
- `security.rs:SOLVE_DEADLINE` — 60 s (`Config::solve_deadline`), so hard limit 65 s, inside the 120 s request timeout.
- `mcp.rs:serve_io` — tool calls run as tasks (at most `AGSTUDIO_MAX_CONCURRENT`, default 2) while the loop keeps reading; a cancelled request gets no response. At EOF it waits for in-flight calls before exiting.
- `main.rs:arm_hard_limit` — `render`/`best`/`translate --solve` exit 124 with a message if the solve overruns `--timeout`/`--budget` + `GRACE`.
- `translate.rs:run_with_timeout` — after the child exits, its output is awaited only 2 s; a process it left holding the pipes gets its group killed, then the call fails rather than hanging.
- `security.rs:client_ip` — rightmost X-Forwarded-For, honoured only from a loopback/private peer with `AGSTUDIO_TRUST_PROXY=1`.
- `translate.rs` — `--bare` is deliberately absent: it limits auth to `ANTHROPIC_API_KEY`, breaking subscription login.
