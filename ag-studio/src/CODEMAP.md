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
- `worker.rs:run` — child in its own process group, `kill_on_drop`, nice +10 (so `/healthz` stays fast under load), `RLIMIT_DATA` = `AGSTUDIO_WORKER_MEM_MB` (default 2048), `RLIMIT_CPU` = threads × hard limit (backstop only), no core dumps. Hard limit = the solve's deadline/budget + `GRACE` (5 s); past it the group is SIGKILLed and the caller gets `Outcome::TimedOut`, reported as `not-proved` with a "time limit" note (web export: 504; MCP export: error).
- `worker.rs:Slot` — the caller's permit lives in the same guard as the child and is dropped only after the child is reaped: on success, on timeout, and when the future is dropped (HTTP client gone, `TimeoutLayer`, MCP `notifications/cancelled`), where a spawned task reaps the SIGKILLed child and then frees the slot.
- `worker.rs:run` — the cap is `RLIMIT_DATA`, not `RLIMIT_AS`: glibc reserves 64 MB of address space per malloc arena (8 × cores) plus a stack per rayon thread, so under a 2 GB `RLIMIT_AS` an IMO solve aborted at ≥ 31 threads and at 128 rayon could not start its pool (swallowed by the aux search as "no proof"). `RLIMIT_DATA` counts only committed writable memory; 64-thread IMO P2 is a test.
- `worker.rs:worker_main` — before solving: `PR_SET_DUMPABLE` 0 (`RLIMIT_CORE` 0 alone still pipes every crash to systemd-coredump), a `parent-watch` thread that exits 125 within 100 ms once `getppid()` is no longer the server (pid in `AGSTUDIO_WORKER_PARENT`; workers are in their own process group, so Ctrl-C/SIGTERM/SIGKILL of the server never reached them; `PR_SET_PDEATHSIG` is avoided because it fires when the forking tokio *thread* exits), its own watchdog at hard limit + `GRACE`, and the rayon pool built up front (exit 3 if it cannot start, so a thread failure is `Outcome::Failed`, never a not-proved answer). Exits 101 on a panic, 124 from the watchdog. Dying messages go through `last_words` (≤ 200 ms), since stderr may be an undrained pipe.
- `worker.rs:worker_command` — production execs `/proc/self/exe` (survives the binary being replaced on disk). Under `cargo test` it re-runs the test harness filtered to `tests::worker_process_entry`, which is why the reply is read as the *last* stdout line; the test-only inputs (`__agstudio_test_hang__` etc.) exist only in `cfg(test)` builds.
- `web.rs:serve_on` — binds and answers before the `claude` probe (up to 15 s per candidate) finishes; it reports in the background. `accept_loop` replaces `axum::serve` only to get a header-read timeout (`Config::header_timeout`, 30 s; axum sets none). HTTP/1 `half_close` stays off on purpose: hyper cannot tell a half-closed client from one that left, and read-EOF is what kills a disconnected client's worker.
- `web.rs:heavy_permit` — waits up to `AGSTUDIO_QUEUE_WAIT_SECS` (5) for a slot, then 503 + `Retry-After: 10`. `/healthz` and pages never touch the semaphore.
- `security.rs:SOLVE_DEADLINE` — 60 s (`Config::solve_deadline`), so hard limit 65 s, inside the 120 s request timeout.
- `mcp.rs:serve_io` — tool calls run as tasks (at most `AGSTUDIO_MAX_CONCURRENT`, default 2) while the loop keeps reading; a cancelled request gets no response; an id already in flight is refused (-32600) so a cancel always reaches its call. A call waits for a slot at most its own limit (≥ `MIN_QUEUE_WAIT`, 5 s), then gets a "busy, retry" tool error. At EOF (MCP shutdown) calls still waiting for a slot are answered "not started"; running ones finish, then the last replies get `FINAL_FLUSH` (60 s) to be read.
- `mcp.rs:spawn_writer`, `note!` — stdout replies (~400 KB with the figure) and stderr diagnostics are written by their own threads: a blocking write on a runtime thread starved the timer that kills overrunning workers when the client was slow to read.
- `main.rs:arm_hard_limit` — `render`/`best`/`translate --solve` exit 124 with a message if the solve overruns `--timeout`/`--budget` + `GRACE`.
- `translate.rs:run_with_timeout` — after the child exits, its output is awaited only 2 s; a process it left holding the pipes gets its group killed, then the call fails rather than hanging.
- `security.rs:client_ip` — rightmost X-Forwarded-For, honoured only from a loopback/private peer with `AGSTUDIO_TRUST_PROXY=1`.
- `translate.rs` — `--bare` is deliberately absent: it limits auth to `ANTHROPIC_API_KEY`, breaking subscription login.
