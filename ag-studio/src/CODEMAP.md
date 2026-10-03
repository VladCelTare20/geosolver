# ag-studio/src — the `agstudio` binary: CLI, web app, MCP server

Up: [../../CODEMAP.md](../../CODEMAP.md) · Web assets: [../assets/](../assets/CODEMAP.md)

## Files
- `main.rs` — subcommands `render`/`best`/`translate`/`serve`/`mcp`; unknown flags exit 2.
- `engine.rs` — solve wrapper: routes DDAR vs metric goals, reconciles a proof against the numeric check, `solve_within` deadline. `Status` (`proved` / `holds-numerically` / `refuted` / `not-proved`) is set only in `reconcile`; `proof` is `Some` only when proved, numeric reports go in `numeric_evidence`. `Solution.figure` (never serialized) carries the problem the figure is drawn from, search-augmented when aux points were added.
- `present.rs` — presentation only, never changes a verdict: readable names for anonymous points, typed facts (`Fact {kind, args, points}`), the proof as numbered cited steps, classified notes, numeric counterexamples, compile-error diagnosis, and `solution_json` (engine fields + `view` + `title`).
- `figure.rs` — the web/export figure SVG: framed to the points and labels (a large circle is clipped rather than allowed to shrink the construction), every element classed and tagged with `data-p` (its points); light colours as attributes so exports render without CSS.
- `render.rs` — SVG→PNG/PDF and the report built from `solution_json` (verdict tone per status, localized, verified steps only): A4 pages for PDF (`report_pdf_from_json`, assembled with `pdf-writer` from one svg2pdf chunk per page), one tall page for PNG.
- `translate.rs` — photo/text → `.geo` via `claude -p`, locked down (no tools or Read-one-file, no settings/MCP/hooks, empty cwd, no API-key env).
- `web.rs` — axum routes and handlers; static assets table; recent-solution cache for export/reopen.
- `security.rs` — env config (the env table is the source for `deploy/DEPLOY.md`), Basic auth, guest mode, rate limits, Host/Origin guards, CSP.
- `auth.rs` — argon2 (gated, dummy-hash for unknown users), session ids, cookies.
- `db.rs` — SQLite: users, sessions (hashed), history (verdict `status`, display `goal`, full `solution` JSON; older rows have `NULL`s). Solving the same input again with the same title, method and verdict replaces the old row rather than stacking a duplicate; `replace_history` swaps a row's result in place (same input only); listings carry the stored note key (`json_extract`); files created 0600.
- `mcp.rs` — stdio JSON-RPC MCP server; exports confined to `AGSTUDIO_EXPORT_DIR`.
- `i18n.rs` — server-side EN/RO strings: errors (full sentences with an action), compile diagnoses, report text; `tf` fills `{placeholders}`, `tp` picks `.one/.few/.other` (Romanian CLDR rules); `prose_ro`/`theorem_ro` translate the Euclidean prover's closed set of English sentence templates and theorem names.

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
- `engine.rs:solve_max_until` — mirrors `ddar::aux_search::solve_max` (same depths and run budgets) but hands the deadline to every level, so a time-limited search stops starting DDAR runs at the deadline. Measured: server CPU flat within 5 s of a 60 s time-limit answer (was still climbing 20 min later).
- `engine.rs:with_keepalive` — every `run_until` worker holds a clone of the caller's guard; `web.rs` passes its heavy-work permit, so a search abandoned at the deadline keeps its slot until it really exits. A client disconnect still cannot stop a solve before its deadline: the engine has no cancel flag yet.
- `web.rs:page` — HTML pages are `Cache-Control: no-store`, so Back after logout re-asks the server (302 to `/auth`) instead of showing the previous user's page from cache.
- `web.rs:api_solve` — a `metric prover: bad number` note is answered as a compile error (`bad_number`), never as a verdict. Recorded solves return `history_id`; `PUT /api/history/{id} {id: <cached solution id>}` replaces that row (the client does this when "Shortest proof" swaps in a shorter proof).
- `present.rs:disp` — names written with a capital keep their case (`Ma` stays `Ma`, the client sets the lower-case suffix as a subscript); low-level lower-case names are upper-cased as before.
- `present.rs:transfer_fact` — the engine's `|AN| ↔ |CN|` (a fixed length ratio moved between its tables) is shown as `AN = CN` or `AC : AN = 2`; the constant is read from the figure and accepted only as a small-denominator rational (p/q, q ≤ 12) matching to 1e-6.
- `present.rs:three_on_a_circle` — a three-point "concyclic" step names the circle's centre among the figure's points (`F, B, A lie on a circle centered at Mc`).
- `present.rs:as_drawn` — the metric prover reads betweenness off the sampled figure; the step says "(as drawn)" instead of letting it pass as a hypothesis.
- `present.rs:Fact.ro` / `Step.rule_name_ro` — Romanian text for Euclidean prose steps and cited theorems; client and report pick them by language.
- `render.rs:Typeset` — point names in report lines are set in "GeoSolver Math" italic. A font change inside one `<text>` makes resvg reshape the whole run with a fallback font, so the bundled export fonts (`assets/fonts/LICENSES.md`) carry the math symbols themselves rather than mixing families.
