# ag-studio/src — the `agstudio` binary: CLI, web app, MCP server

Up: [../../CODEMAP.md](../../CODEMAP.md) · Web assets: [../assets/](../assets/CODEMAP.md)

## Files
- `main.rs` — subcommands `render`/`best`/`translate`/`serve`/`mcp`; unknown flags exit 2.
- `engine.rs` — solve wrapper: routes DDAR vs metric goals, reconciles a proof against the numeric check, `solve_within` deadline. `Status` (`proved` / `holds-numerically` / `refuted` / `not-proved`) is set only in `reconcile`; `proof` is `Some` only when proved, numeric reports go in `numeric_evidence`. `Solution.figure` (never serialized) carries the problem the figure is drawn from, search-augmented when aux points were added.
- `present.rs` — presentation only, never changes a verdict: readable names for anonymous points, typed facts (`Fact {kind, args, points}`), the proof as numbered cited steps, classified notes, numeric counterexamples, compile-error diagnosis, and `solution_json` (engine fields + `view` + `title`).
- `figure.rs` — the web/export figure SVG: framed to all content, every element classed and tagged with `data-p` (its points); light colours as attributes so exports render without CSS.
- `render.rs` — SVG→PNG/PDF and the report page built from `solution_json` (verdict tone per status, localized, verified steps only).
- `translate.rs` — photo/text → `.geo` via `claude -p`, locked down (no tools or Read-one-file, no settings/MCP/hooks, empty cwd, no API-key env).
- `web.rs` — axum routes and handlers; static assets table; recent-solution cache for export/reopen.
- `security.rs` — env config (the env table is the source for `deploy/DEPLOY.md`), Basic auth, guest mode, rate limits, Host/Origin guards, CSP.
- `auth.rs` — argon2 (gated, dummy-hash for unknown users), session ids, cookies.
- `db.rs` — SQLite: users, sessions (hashed), history (verdict `status`, display `goal`, full `solution` JSON; older rows have `NULL`s). Solving the same input again with the same title, method and verdict replaces the old row rather than stacking a duplicate; files created 0600.
- `mcp.rs` — stdio JSON-RPC MCP server; exports confined to `AGSTUDIO_EXPORT_DIR`.
- `i18n.rs` — server-side EN/RO strings: errors, compile diagnoses, report text; `tf` fills `{placeholders}`.

## Notes
- `security.rs:client_ip` — rightmost X-Forwarded-For, honoured only from a loopback/private peer with `AGSTUDIO_TRUST_PROXY=1`.
- `security.rs:translate_status_now` — `/api/status` never waits for the `claude` probe: it answers from the last result (or `translate_checking`) and refreshes in the background; `serve` warms it before binding.
- `web.rs:SOLVE_DEADLINE` — 60 s, inside the 120 s request timeout.
- `web.rs:cache_put` — every solve answer gets a random `id`; `/api/export {id}` renders exactly that result (no re-solve) for 30 min, 128 entries. History rows store the full JSON (≤ 512 KB) and `GET /api/history/{id}` re-registers it.
- `web.rs:compile_err` — `{error, code: "compile", diagnosis: {key, line, col, len, token}, detail}`; `error` is the localized sentence, `detail` the engine's words.
- `present.rs:Names::build` — `_5` from `reflect(H, …)` becomes `H′`, a midpoint `M`/`N`, else `P₁, P₂…`; the same map renames figure, facts, steps and aux text, so engine names never reach the page (raw `proof`/`low_level` keep them).
- `present.rs:drop_restatements` — the engine re-states hypotheses (`collinear: A I M [003]` after `assumption: coll A I M`); those steps are folded into the cited one and citations renumbered. The step count shown is the folded one; `proof_steps` stays the engine's.
- `present.rs:diagnose` — the engine's compile errors carry no position: by-name errors are found by token; parse errors by compiling growing prefixes until the same message appears (≤ 1.5 s).
- `translate.rs` — `--bare` is deliberately absent: it limits auth to `ANTHROPIC_API_KEY`, breaking subscription login.
