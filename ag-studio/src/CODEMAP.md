# ag-studio/src — the `agstudio` binary: CLI, web app, MCP server

Up: [../../CODEMAP.md](../../CODEMAP.md)

## Files
- `main.rs` — subcommands `render`/`best`/`translate`/`serve`/`mcp`; unknown flags exit 2.
- `engine.rs` — solve wrapper: routes DDAR vs metric goals, reconciles a proof against the numeric check, `solve_within` deadline.
- `render.rs` — SVG→PNG/PDF and the combined report page; scales long proofs down to fit pixel/page limits.
- `translate.rs` — photo/text → `.geo` via `claude -p`, locked down (no tools or Read-one-file, no settings/MCP/hooks, empty cwd, no API-key env).
- `web.rs` — axum routes and handlers.
- `security.rs` — env config (the env table is the source for `deploy/DEPLOY.md`), Basic auth, guest mode, rate limits, Host/Origin guards, CSP.
- `auth.rs` — argon2 (gated, dummy-hash for unknown users), session ids, cookies.
- `db.rs` — SQLite: users, sessions (hashed), history; files created 0600.
- `mcp.rs` — stdio JSON-RPC MCP server; exports confined to `AGSTUDIO_EXPORT_DIR`.
- `i18n.rs` — server-side EN/RO strings.

## Notes
- `security.rs:client_ip` — rightmost X-Forwarded-For, honoured only from a loopback/private peer with `AGSTUDIO_TRUST_PROXY=1`.
- `web.rs:SOLVE_DEADLINE` — 60 s, inside the 120 s request timeout.
- `translate.rs` — `--bare` is deliberately absent: it limits auth to `ANTHROPIC_API_KEY`, breaking subscription login.
