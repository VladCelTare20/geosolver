# Changelog

Versions follow the `version` of `ag-studio/Cargo.toml`; every build also
reports its git commit (`agstudio --version`, `/api/status`).

## Unreleased

### Upgrading — action required
- If your env file still has the old template's
  `AGSTUDIO_BASIC_AUTH=:CHANGE-ME-run-openssl-rand-hex-16` (the previous
  `.env.example` shipped it uncommented and the previous build accepted it),
  this build refuses to start. That password was published in this
  repository: set a new one before upgrading.
- The previous release has no `agstudio backup`, and its data stays in
  `agstudio.db-wal` even after a clean stop, so never copy `agstudio.db` alone
  (a 4 KB main file beside a 100 KB+ WAL: the copy has no tables). Stop the
  service, build the new release without starting it, and back up with the
  NEW binary (`agstudio backup <dest> --db <path>`), or copy `agstudio.db`
  together with its `-wal` and `-shm` files (DEPLOY.md, "Upgrading from a
  release without `agstudio backup`"). Route C: re-copy
  `deploy/agstudio.service`, `systemctl daemon-reload`, and
  `chmod 600 /etc/agstudio.env`.
- `AGSTUDIO_GUEST_AI_PER_DAY` now counts accounts too (per address), and
  `AGSTUDIO_AI_PER_DAY_TOTAL` caps the whole server; `/readyz` answers only
  `{"ready": bool}`.

### Soundness
- `angle(a,b,c) = angle(d,e,f)` and `angle(a,b,c) = k`, as goals and as
  hypotheses, are undirected angles: they are no longer lowered to directed
  line angles mod 180°, which reported supplementary angles as equal and a
  120° angle as 60° (and drew "given ∠ABC = 120°" at 60°). The prover gets the
  directed form with each angle's orientation read off the figure; a straight
  or zero `angle(..)` is refused with a pointer to `coll`.
- A proof whose goal fails in another figure satisfying every hypothesis is
  marked "Proved as drawn — for the configuration drawn only" on the page,
  in the PDF/PNG, over MCP and in the CLI note.
- A directed angle fact whose `∠` reading is false in the figure is shown as
  `∡` in GIVEN/PROVE, the legend and the reports.

- The human proof says "XY bisects ∠PXQ" only when the figure has both angles
  equal and inside ∠PXQ; an external bisector is "XY is the external bisector
  of ∠PXQ" (EN and RO), and a conclusion that holds for the bisector's line but
  not the ray is stated as the directed equality. `proof_audit` checks every
  hypothesis wording, the goal wording and every GIVEN/PROVE angle statement
  against the displayed figure.
- `simtri`/`contri` goals on collinear triangles are refused (they were proved
  by `0 = 0` angle conjuncts).
- A congruence is named by a criterion (SSS/SAS/ASA) only when the facts that
  criterion needs were stated before the congruence, never counting the
  congruence's own conclusion.

### UI polish
- Phone proofs: a stacked equality chain no longer draws the angle sign over the
  "=" or "−" (every "= ∠B" read as "≠ B", "90° − ∠HBC" as "90° ∠HBC"); the
  hanging indent applies to the row only.
- Point names longer than a capital and one lower-case letter are set whole
  (`Orthocenter` was O, subscript r, "thocenter") and the points of a segment
  or angle are kept apart (`Alpha Horth ⟂ Beta Gamma`) in GIVEN, PROVE, the
  proof, copied text and the PDF/PNG; figure labels of long names no longer
  clip at the frame edge (the frame grows to the labels' real width).
- GIVEN no longer lists a length the program does not state
  (`dist(B,D) = 2*dist(D,C)` produced "BD = 2"), and a ratio or sum is stated
  once.
- Menus (Examples, Export) open on screen on short laptops and landscape phones;
  a compile error scrolls the editor to the marked token; a failed status call
  ends in "Can't check AI · Retry" instead of "Checking AI…" forever; the
  thumbnail hides less of the step it accompanies; tapping a figure point keeps
  its highlight and has a 17 px target.
- Layout: the example chips are on the first screen of a 1366×768 laptop, the
  tablet figure is at most 560 px, the header no longer shows text through,
  the proof tabs and the figure toolbar fill a 280–399 px row, the footer links
  share a row, the verdict's as-drawn note is its own line, step references are
  wider targets, 2.4K/4K screens get a larger page, the end-of-proof mark follows
  the text, the reason column of a PNG/PDF chain clears wide angle rows, the
  auxiliary colour is teal (red goal and brown aux were 38 apart under
  deuteranopia), the favicon lightens on a dark tab strip, the verdict heading
  no longer breaks inside a word at 200 % text.
- Proof tabs never overflow, clip or leave a lone second row at any width or
  text size: one row when the labels fit, three equal columns when only
  wrapped labels fit, a stack of three when they do not (checked 280-480 px x
  100-200 % x English/Romanian). Names of an angle or segment are never split
  across lines in a desktop chain (Romanian "∡SP / L =" in IMO 2023 step 7).
- Phones: the step thumbnail is dismissed when the step it shows scrolls away
  (it stayed over Solve and the verdict); the "as drawn" sentence spans the full
  card width; header buttons, tabs and fact rows are 44 px tall and step
  references are 44 px wide targets; figure labels are measured after the font
  loads (a stale width put the widest 12-character names 1.7 px from the frame)
  and two labels no longer overlap (A' over S on a landscape SE); the 1280x720
  example chips end inside the screen; the gate card, the AI pill and the
  full-screen figure fit a 180-240 px page at 125-200 % text.

### Final-attack fixes
- Soundness: an angle-sum goal is also checked as written, on the undirected
  angles in degrees with no reduction mod 180°. `angle(A,B,C) + angle(B,C,A) +
  angle(C,A,B) = 360` (or 0, 540, 720), a parallelogram's opposite angles
  differing by 180° and a quadrilateral summing to 720° were reported
  "Proved"; they are refuted, and `ddar --geo` no longer prints "Proven" for a
  goal its own figure contradicts.
- Soundness: a proof of a betweenness, ray or arc-side statement about a free
  point (`BD + DC = BC` for D on line BC or the foot of A, `angle(A,B,D) =
  angle(A,B,C)`, `angle(A,B,C) = angle(A,D,C)` for D on the circle, an
  orthocenter angle sum) is "for the configuration drawn only": the
  configuration probe now evaluates the statement as written in right and
  obtuse triangles and with free points on both sides, not the lowered
  directed predicate. A 120-statement sweep against an independent oracle had
  56 false statements proved with no caveat; it now has none.
- Proofs: a converse bisector step is typeset ∡ when its undirected angles
  differ; a directed constant is stated with the arms the proof text uses
  (`aconst a b c d K` is the turn from `cd` to `ab`); the external
  bisector theorem is named so; no midline of a collinear triangle; `distmeq`
  hypotheses are written as the length equation; a block that only restates
  the collinear or concyclic fact it cites is dropped.
- Server: connections are capped (`AGSTUDIO_MAX_CONNECTIONS`, units get
  `LimitNOFILE=65536`), `/gate` cuts a slow body after 10 s, new accounts stop
  at `AGSTUDIO_MAX_ACCOUNTS` and history stops storing solutions past
  `AGSTUDIO_MAX_DB_MB`, point names are at most 16 characters, an empty
  `AGSTUDIO_GATE_KEY` is unset, the grammar temp file no longer accumulates,
  and a shared password under 12 characters is flagged at startup.
- NixOS module: each worker's memory cap follows `memoryMax` and
  `maxConcurrent` (`workerMemoryMb` overrides), the funnel unit takes
  `flock /run/tailscale-serve.lock` and clears the port with `serve off` on stop.
- UI: the daily AI cap shows its own message (no "wait a minute"/Try again),
  the server-wide cap no longer says "on this network", a long token in a
  `.geo` error and long point names wrap, selecting and copying proof text
  gives the visible text, the exported SVG has a white background, print keeps
  the PROVE label with its statement, the gate page follows the app's theme,
  and the no-AI wording is meant for visitors.

### Human proofs
- Readability rounds 1 and 2 (merged from `wf/hp-final3`): concyclicity by a
  named converse (Thales' converse for two right angles; the inscribed-angle
  converse with its side condition), named identities (supplementary,
  vertical and common angles, alternate and corresponding angles), reflection
  and perpendicular-bisector citations, angle-bisector-theorem steps that
  cite their bisector equality, similarity steps that name their corners, and
  fewer restated hypotheses. See `docs/HUMAN_PROOFS.md` §13.12.

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
- `deploy/docker-compose.yml` had `stop_grace_period` twice, so compose
  refused to parse it; CI now runs `docker compose config`. Compose and
  route B default to a 5 GB memory limit (2 slots × 2048 MB + overhead),
  route B passes `--stop-timeout 90`, the `ai` image pins the Claude Code
  CLI, and compose forwards `GEOSOLVER_BUILD_JOBS`/`CLAUDE_CODE_VERSION`.
- DEPLOY.md: backups no longer leave copies in the data volume and can run
  twice a day; literal route B restore; secret files must belong to uid
  10001; the host nginx bootstrap (port 80 first, then certbot, then 443);
  per-route `agstudio doctor`; where to install the CLI for route C.
- `/licenses` is an HTML page that fits a phone, linked from the footers.
- When signed in, the header stays one row on phones from 300 to 440 px.
- `/api/humanize` takes at most 32 KB of proof, 16 KB of human text and
  48 KB in all.
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
