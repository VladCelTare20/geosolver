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

### Fixed
- The debug `cargo test` no longer overflows the stack in the soundness suite
  and skips the two searches that only fit their time budget in release.
- Documentation: deployment guide rewritten (backup/restore, upgrades and
  rollback, sizing, TLS renewal, AI on a server, NixOS); results tables
  updated; README example image regenerated; examples that used passwords the
  server refuses.

## 0.1.0

First public version: the DDAR engine, the human-style proof writer, the web
app (Describe/Photo/AI explanations, PWA, EN/RO), the MCP server and the CLI.
