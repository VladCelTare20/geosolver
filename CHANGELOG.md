# Changelog

Versions follow the `version` of `ag-studio/Cargo.toml`; every build also
reports its git commit (`agstudio --version`, `/api/status`).

## Unreleased

### Deployment
- `agstudio backup <dest.db>`: a consistent, self-contained copy of the live
  database (`VACUUM INTO`, integrity-checked, never overwrites). The server
  checkpoints the WAL into the main file every 10 minutes and on
  `SIGTERM`/Ctrl-C, so a stopped server's `agstudio.db` is complete on its own.
- Database schema version (`PRAGMA user_version`): an older build refuses a
  database written by a newer one instead of damaging it.
- `agstudio --version`; `/api/status` and MCP `serverInfo` carry the commit.
- `/healthz` answers any `Host` header, so a load balancer's plain-HTTP check
  works with `AGSTUDIO_PUBLIC_HOST` set.
- `/licenses` serves the NOTICE and the licences of the bundled fonts (Inter,
  STIX Two Text, DejaVu); `THIRD_PARTY_LICENSES.md` lists the Rust crates.
- Docker: base images pinned by digest, `--locked`, BuildKit caches, OCI
  labels, licence files in `/usr/share/doc/geosolver`, and an `ai` target with
  the Claude Code CLI (token route). Compose: CPU/memory/pids limits,
  read-only root, no capabilities, health checks for both containers, nginx
  re-resolving the app (no 502s after the app container is recreated), ACME
  webroot and a certbot deploy hook.
- Native route: Caddyfile and host nginx config; the env file is installed
  mode 600; `deploy/build-linux.sh` is executable.
- Nix: `nixosModules.default` (`services.geosolver`: generated password,
  Funnel, health check, daily backups, optional AI with a self-test);
  the package no longer runs the test suite on every build (`nix flake check`
  does).
- CI workflow, `SECURITY.md`, this changelog.

- Graceful shutdown: `SIGTERM` stops accepting, lets requests in flight
  finish for up to `AGSTUDIO_SHUTDOWN_GRACE_SECS` (default: solve deadline +
  10 s), checkpoints the database and exits 0. The systemd unit and the NixOS
  module use `KillMode=mixed`, `TimeoutStopSec=90`, `OOMPolicy=continue`;
  compose waits 90 s.
- `/readyz` (database writable, a solver slot free, not draining) and one
  access-log line per request (`AGSTUDIO_ACCESS_LOG=0` turns it off).
- `agstudio doctor` checks the AI setup with one tiny model call; a token the
  API rejects turns the AI pills to "not set up" for 10 minutes instead of
  failing every request.

### Security
- Fail-closed configuration: flags accept only 1/0, true/false, yes/no, on/off;
  a number that does not parse (or overflows) refuses to start; a password that
  still contains the template's `CHANGE-ME` refuses to start.
- Secrets from files: `AGSTUDIO_BASIC_AUTH_FILE`, `AGSTUDIO_GATE_KEY_FILE`,
  `CLAUDE_CODE_OAUTH_TOKEN_FILE`.
- A cached solution is exported or recorded only by the caller who created it.
- Guest AI explanations count against `AGSTUDIO_GUEST_AI_PER_DAY` together
  with Describe/Photo.
- The Docker build context is an allowlist; nginx sends one HSTS header and
  no version.

### Engine
- Low-level input errors are 400s that name the line and column (unknown
  relation, arity, false hypothesis, NaN/inf coordinates); more than 100
  points is refused before any allocation.
- A false low-level `cong`/`midp` goal is refuted at once instead of searching
  until the deadline; the trapezoid corpus `midp` goal proves again.
- Best mode rejects invalid input like a normal solve and proves IMO 2023 P2
  within a 20 s budget; render with many free points finishes in its budget.
- Solver threads are shared among the solves running at once; the lemma phase
  of the aux search starts by 18 s; the solve deadline
  (`AGSTUDIO_SOLVE_DEADLINE_SECS`) is configurable and a time-limit verdict
  offers "Keep searching" (`AGSTUDIO_EXTENDED_DEADLINE_SECS`, one at a time).

### Human proofs
- Rewritten writer in the Romanian *barem* layout: numbered steps with cited
  facts "(k)", a plan line, named theorems (inscribed and central angles,
  power of a point, radical axis, midline, perpendicular bisector, vertical
  angles, SAS/AA criteria), directed angles only in the steps that need them.
- `triangle` is drawn as a scalene acute textbook triangle first, so figure
  angles read as a textbook would.
- The proof shown (page, Copy proof, PDF/PNG) is always written against the
  figure drawn; a circle is named only when the hypotheses give it, and its
  centre only when the equal radii are given.
- Romanian proofs print ∠ for figure angles (∡ stays the directed angle).

### Interface
- Describe/Photo results are marked as the AI's reading, with "Check or edit it
  in .geo"; a refuted AI reading says so.
- The Syntax panel lists every construction and relation in EN and RO;
  compile errors link to it.
- Typeset example titles that follow the language; branded EN/RO 404 page;
  the manifest follows the visitor's language.
- PDF/PNG exports survive emoji, control characters and noncharacters.

### Fixed
- Release convergence: the Docker build context admits the licence files the
  image installs; the Nix package source keeps `deploy/.env.example` (read by
  a test), so `nix flake check` passes; the workspace is `cargo fmt` clean and
  CI now fails on formatting or any clippy warning.
- The debug `cargo test` no longer overflows the stack in the soundness suite
  and skips the searches that only fit their time budget in release.
- Documentation: deployment guide rewritten (backup/restore, upgrades and
  rollback, sizing, TLS renewal, AI on a server, NixOS); results tables
  updated; README example image regenerated; examples that used passwords the
  server refuses.

## 0.1.0

First public version: the DDAR engine, the human-style proof writer, the web
app (Describe/Photo/AI explanations, PWA, EN/RO), the MCP server and the CLI.
