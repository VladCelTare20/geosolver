# Deploying GeoSolver on a Linux server

The whole app is a **single binary** (`agstudio`: the web UI, fonts and grammar
are baked in) plus **one SQLite database** (`AGSTUDIO_DB`) holding accounts,
sessions, solve history and the gate-cookie key. That database is the only
state. It runs in WAL mode, so while the server runs recent writes live in
`agstudio.db-wal` beside it: **never back up `agstudio.db` alone** — use
`agstudio backup` (see [Backup and restore](#backup-and-restore)).

Pick one route:

- **A. Docker Compose + nginx (TLS)** — recommended for a public host.
- **B. Plain `docker run`** — behind your own proxy / Cloudflare Tunnel.
- **C. Native binary + systemd** — no Docker; Caddy or nginx in front.
- **D. NixOS module** — `nixosModules.default` of this flake, optionally on
  the internet through Tailscale Funnel (how the reference instance runs).

Everything is **secure by default**: the server *refuses to start* on a
non-loopback interface unless you set `AGSTUDIO_BASIC_AUTH` (or explicitly opt
out with `AGSTUDIO_ALLOW_INSECURE=1` behind a proxy), and refuses to start if
`AGSTUDIO_BASIC_AUTH` is set but too weak to use (a password under 8
characters). With a shared password on, **guest mode** lets anyone with the
password solve, export and use the AI features without an account — share a
link and a password, nothing else. It also has per-IP rate limits, a
concurrency cap, a request-body limit, and CSP/security headers.

> **Behind any TLS proxy you run yourself** (routes B and C, Caddy, nginx,
> Traefik, Cloudflare Tunnel, Tailscale Funnel) set all three:
> `AGSTUDIO_PUBLIC_HOST=<your.domain>` (otherwise every proxied request gets
> 403 `unknown host`), `AGSTUDIO_TRUST_PROXY=1` (otherwise every visitor shares
> the proxy's IP, so one person's wrong passwords lock everyone out for a
> minute) and `AGSTUDIO_SECURE_COOKIES=1` (`Secure` `__Host-` cookies and
> HSTS). Keep the app's own bind on loopback (or a private network only the
> proxy can reach): `TRUST_PROXY` believes `X-Forwarded-For` from any
> loopback/private peer. Route A's compose file sets the last two for you.

### The shared password: the gate page

Browsers get a password page (`/gate`) instead of the HTTP Basic dialog. The
right password sets an `HttpOnly` cookie (`__Host-gate` with
`AGSTUDIO_SECURE_COOKIES=1`) signed with HMAC-SHA256 over the password, valid
for `AGSTUDIO_GATE_DAYS` (180) and renewed on visits after 7 days, so a phone —
including the app added to its Home Screen — asks once, not on every cold
start. Changing the password, or `AGSTUDIO_GATE_KEY`, signs every device out;
"Forget this device" in the app footer signs out just that one. The cookie key
is generated once and stored in the database (table `server_secrets`), so
restarts and deploys keep everyone signed in — and a restore from a backup
keeps them signed in too.

HTTP Basic still works for API clients (`curl -u x:PASSWORD …`): requests
without a `Sec-Fetch-Mode` header still get the `WWW-Authenticate` challenge.
Browser API calls without the password get `401 {"code": "gate"}` and the app
goes to the gate page. Wrong passwords on the form and on Basic share one
per-IP budget (`AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN`). `AGSTUDIO_GATE=basic`
restores the Basic-only behaviour.

Without the password only these are served: `/healthz`, `/readyz`, `/gate`,
`/manifest.webmanifest`, the icons (`/favicon.svg`, `/favicon.ico`,
`/apple-touch-icon*.png`, `/icons/*`, `/splash/*`), `/assets/app.css`, the
fonts, `/licenses` (the NOTICE and the bundled fonts' licences) and
`/robots.txt` — static files with nothing secret, which iOS fetches without
cookies when a page is added to the Home Screen. `/healthz` also answers
whatever `Host` header a proxy or load balancer sends.

### Guests, solver slots and sizing

The number of simultaneous solves ("slots", `AGSTUDIO_MAX_CONCURRENT`)
defaults to the CPUs the process may use, clamped to 2..8 — so a container
limited to 2 CPUs gets 2 slots, a 16-thread host 8. Each solve runs in its own
worker process capped at `AGSTUDIO_WORKER_MEM_MB` (2048 MB of committed
memory), so **RAM ≥ slots × WORKER_MEM_MB + ~300 MB**: 2 slots fit in 4 GB,
8 slots want 16 GB. Lower the slots or the per-worker cap on a small VPS.

Guests are keyed by their `gid` cookie, and all guests behind one address
share every slot but one (slots − 1). Friends on one Wi-Fi network or behind
one mobile carrier's NAT therefore compete: with 4 slots, a fourth
simultaneous guest solve from that address is told the server is busy and
retries once by itself. That is deliberate — the `gid` cookie is not signed,
so a client could mint new ids to take every slot — and fine for a small
audience; raise `AGSTUDIO_MAX_CONCURRENT`, or have regular users create
accounts (accounts are not limited per address).

---

## A. Docker Compose + nginx (recommended)

```sh
install -m 600 deploy/.env.example deploy/.env
#   edit deploy/.env: uncomment AGSTUDIO_BASIC_AUTH with your own password,
#   e.g. ":$(openssl rand -hex 16)", and set AGSTUDIO_PUBLIC_HOST=your.domain
#   (both required). Left commented or still CHANGE-ME, the app refuses to start.

mkdir -p deploy/certs deploy/acme
#   first start: a throw-away self-signed pair so nginx can start
openssl req -x509 -newkey rsa:2048 -nodes -days 7 \
  -keyout deploy/certs/privkey.pem -out deploy/certs/fullchain.pem -subj "/CN=localhost"

GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) \
  docker compose -f deploy/docker-compose.yml up -d --build
```

Then get the real certificate (next section). Open `https://your.domain`.
Only nginx is exposed (ports 80/443); the app runs on the private container
network. nginx terminates TLS and forwards the client IP so rate limiting
works, and looks the app up through Docker's DNS on every request, so a
recreated app container is found without restarting nginx. nginx alone sends `Strict-Transport-Security` (the app's copy is
hidden, so the header appears once).

The build context is an allowlist (`.dockerignore`): only `Cargo.toml`,
`Cargo.lock`, `ag-studio/`, `alphageometry-rs/` and `corpus/`,
minus `target/`, `node_modules/`, `.env`, `*.db*` and logs. `deploy/.env`,
`deploy/certs/` and a local `agstudio.db` never reach the builder, and editing
them does not invalidate the cargo build layer.

The app container runs read-only with all capabilities dropped,
`no-new-privileges`, a `pids` limit, and a CPU and memory limit
(`GEOSOLVER_CPUS`, default 2; `GEOSOLVER_MEM_LIMIT`, default 4g — set them in
`deploy/.env` following [sizing](#guests-solver-slots-and-sizing)). Both
containers have health checks; `docker compose ps` shows them.

### TLS certificates (Let's Encrypt)

nginx serves `/.well-known/acme-challenge/` on port 80 from `deploy/acme`, so
certbot's webroot mode works while nginx keeps running, for the first
certificate and for every renewal:

```sh
sudo certbot certonly --webroot -w "$PWD/deploy/acme" -d your.domain \
  --deploy-hook "$PWD/deploy/certbot-deploy-hook.sh"
```

The deploy hook copies the certificate into `deploy/certs/` as real files
(the nginx container cannot follow certbot's symlinks into
`/etc/letsencrypt`, so do not symlink them by hand) and reloads nginx. The
certbot package's own timer (`systemctl list-timers | grep certbot`, or its
cron entry) runs `certbot renew` twice a day, which reuses the webroot and the
hook; check it with `sudo certbot renew --dry-run`. Do not use
`--standalone`: it needs port 80, which nginx holds.

## B. Plain `docker run`

```sh
docker build -t geosolver --build-arg GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) .
pw=$(openssl rand -hex 16); echo "shared password: $pw"
docker run -d --name geosolver -p 127.0.0.1:8787:8787 \
  -e AGSTUDIO_BASIC_AUTH=":$pw" \
  -e AGSTUDIO_PUBLIC_HOST=your.domain -e AGSTUDIO_TRUST_PROXY=1 -e AGSTUDIO_SECURE_COOKIES=1 \
  -v geosolver-data:/data \
  --read-only --tmpfs /tmp --tmpfs /home/agstudio:uid=10001,gid=10001,mode=0700 \
  --cap-drop ALL --security-opt no-new-privileges --cpus 2 --memory 4g --pids-limit 512 \
  --restart unless-stopped geosolver
```

Then front it with your own TLS proxy (see the box at the top). The
`geosolver-data` volume keeps accounts and history across `docker rm` and
rebuilds. To keep the data in a host directory instead, the directory must
belong to the container's user, uid 10001:
`sudo install -d -o 10001 -g 10001 -m 700 /srv/geosolver` and
`-v /srv/geosolver:/data` (otherwise the server stops with
`cannot create /data/agstudio.db owner-only: Permission denied`).

## C. Native binary + systemd

```sh
./deploy/build-linux.sh                      # needs Rust 1.88+; builds for this CPU
sudo install -Dm755 target/release/agstudio /opt/agstudio/agstudio
sudo cp deploy/agstudio.service /etc/systemd/system/
sudo install -m 600 -o root -g root deploy/.env.example /etc/agstudio.env
#   edit /etc/agstudio.env: the password, plus the three proxy variables above
sudo systemctl daemon-reload
sudo systemctl enable --now agstudio
```

The unit is hardened (DynamicUser, seccomp, ProtectSystem=strict, no
capabilities) and listens on `127.0.0.1:8787`; its only writable path is
`StateDirectory=/var/lib/agstudio`, where the database lives. systemd reads
`/etc/agstudio.env` as root before dropping privileges, so the file stays
`0600 root`.

Put a TLS proxy in front:

- **Caddy** (simplest; gets and renews the certificate itself):
  `deploy/Caddyfile` — replace `geo.example.com`, copy it to
  `/etc/caddy/Caddyfile`, `systemctl reload caddy`.
- **nginx on the host**: `deploy/nginx-host.conf` (its header lists the
  certbot `--webroot` command; the distribution's certbot timer renews and
  `certbot renew --deploy-hook "systemctl reload nginx"` reloads).

## D. NixOS (flake module)

```nix
# flake.nix of the machine
inputs.geosolver.url = "github:VladCelTare20/geosolver";
# in nixosSystem modules:
geosolver.nixosModules.default
({ pkgs, ... }: {
  services.geosolver = {
    enable = true;
    publicHost = "box.tail1234.ts.net";   # or your domain behind Caddy/nginx
    funnel.enable = true;                 # Tailscale Funnel on :10000 (needs services.tailscale)
    maxConcurrent = 4;
    memoryMax = "10G";
    cpuQuota = "800%";
    environment.AGSTUDIO_GUEST_AI_PER_DAY = "60";
    ai.package = pkgs.claude-code;        # optional; unfree, needs allowUnfree
  };
})
```

The module runs `agstudio serve` as a DynamicUser with the same sandbox as
the unit in route C, generates a 128-bit shared password on first start
(`sudo geosolver-password` prints the link and password,
`--rotate` replaces it), restarts the app when `/healthz` stops answering
(every minute), and writes a daily backup to `/var/backup/geosolver`
(14 kept; `sudo geosolver-backup [dest]` takes one now). With `ai.package`
set, `sudo geosolver-claude-token` installs a `claude setup-token` token and
runs a self-test inside the service sandbox. Funnel is per port: everything
served on the funnel port is public, so give GeoSolver a port nothing else
uses (`funnel.port`: 443, 8443 or 10000), and `systemctl stop
geosolver-funnel` takes it off the internet while it keeps running on
loopback.

The flake's package (`nix build .#geosolver`) skips the test suite so a
deploy is not held up by load-sensitive timing tests; run it with
`nix flake check` (or `nix build .#checks.x86_64-linux.tests`) before
releasing.

---

## Backup and restore

`agstudio backup <dest.db>` writes a consistent copy of the live database
(SQLite `VACUUM INTO`, safe while the server runs), checks its integrity,
prints the number of users and history rows, and never overwrites an existing
file. The copy is one self-contained file, mode 0600.

```sh
# A (compose): inside the container, then out of the volume
docker compose -f deploy/docker-compose.yml exec app agstudio backup /data/backup-$(date +%F).db
docker compose -f deploy/docker-compose.yml cp app:/data/backup-$(date +%F).db ./
# B (docker run)
docker exec geosolver agstudio backup /data/backup-$(date +%F).db && docker cp geosolver:/data/backup-$(date +%F).db ./
# C (systemd)
sudo /opt/agstudio/agstudio backup /root/geosolver-$(date +%F).db --db /var/lib/agstudio/agstudio.db
# D (NixOS)
sudo geosolver-backup
```

Schedule it (a cron line or systemd timer per route; route D does it daily)
and copy the files off the machine. The server also checkpoints the WAL into
the main file every 10 minutes and on a clean stop (`SIGTERM`/Ctrl-C), but a
copy of the bare `.db` taken while it runs can still miss recent writes — a
filesystem or volume snapshot must include `agstudio.db-wal` and
`agstudio.db-shm` too.

**Restore** (the server must be stopped; the backup replaces everything,
including the gate key, so signed-in devices stay signed in):

```sh
# A (B: the same with docker stop/run/start on the geosolver container's volume)
docker compose -f deploy/docker-compose.yml stop app
docker compose -f deploy/docker-compose.yml run --rm --no-deps -T --entrypoint sh app -c \
  'set -e; cd /data; cat > restore.tmp; chmod 600 restore.tmp; rm -f agstudio.db-wal agstudio.db-shm; mv restore.tmp agstudio.db' < ./backup.db
docker compose -f deploy/docker-compose.yml start app
# C
sudo systemctl stop agstudio
sudo rm -f /var/lib/agstudio/agstudio.db-wal /var/lib/agstudio/agstudio.db-shm
sudo install -m 600 ./backup.db /var/lib/agstudio/agstudio.db
sudo chown --reference=/var/lib/private/agstudio /var/lib/agstudio/agstudio.db
sudo systemctl start agstudio
```

(Route D: the same as C with `geosolver` for `agstudio` in the unit and the
paths.) Check it with `journalctl -u agstudio -n 20` and a sign-in.

## Updating and rolling back

1. **Back up first** (above).
2. Update: `git pull`, then
   `GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) docker compose -f deploy/docker-compose.yml up -d --build`
   (route C: rebuild, `install`, `systemctl restart agstudio`; route D: update
   the flake input and `nixos-rebuild switch`).
3. Check `/api/status` → `"version"` and `"revision"`, or
   `agstudio --version`.

The database records its schema version (`PRAGMA user_version`). An upgrade
migrates it forward in place; an older build refuses to open a database
written by a newer one (`schema version N, newer than this build's M`) rather
than damaging it. So to roll back: check out the previous release, rebuild,
and restore the backup taken in step 1. For quick rollbacks of route A, tag
the running image before updating
(`docker tag deploy-app:latest geosolver:previous`).

**`docker compose down -v` deletes the data volume** — accounts, history and
the gate key. `down` without `-v`, `stop`, `restart` and `up --build` keep it.

## Health, logs, version

- Liveness: `GET /healthz` → `ok` (no password, any `Host`, never waits for a
  solve slot). Docker health checks use it for both containers.
- Readiness: `GET /readyz` (unauthenticated) → 200 `{"ready":true,"db":"ok","solver_free":n,…}`
  when the database takes a write, a solver slot is free and no shutdown is
  under way; otherwise 503 with `Retry-After: 10`. It turns 503 whenever every
  slot is busy, so point load balancers at it, never a restart-on-unhealthy
  health check.
- Logs: `docker compose -f deploy/docker-compose.yml logs -f app` /
  `journalctl -u agstudio -f`. The banner names the version
  (`GeoSolver 0.1.0 → …`), then one access line per request:
  `2026-10-07T01:02:03Z 203.0.113.5 POST /api/solve 200 1834ms`.
- Stopping: SIGTERM (or Ctrl-C) stops accepting connections, lets requests in
  flight finish for up to `AGSTUDIO_SHUTDOWN_GRACE_SECS` (70 s: a solve's 60 s
  plus its worker's grace), checkpoints the SQLite WAL and exits 0. The service
  manager must wait longer than that and must signal only the server, not its
  solve workers: `deploy/agstudio.service` and the NixOS module set
  `KillMode=mixed` and `TimeoutStopSec=90`, `deploy/docker-compose.yml` sets
  `stop_grace_period: 90s` (Docker's default of 10 s would cut every solve in
  flight). Workers notice a dead server within 100 ms and exit on their own.
  While running, the server also checkpoints the database every 10 minutes.
- Memory: each solve worker is capped at `AGSTUDIO_WORKER_MEM_MB`. When the
  service has a memory limit (`MemoryMax=`, a container limit), keep
  `AGSTUDIO_MAX_CONCURRENT × AGSTUDIO_WORKER_MEM_MB` under 90 % of it (6 GB and
  4 slots: at most 1382 MB each); the banner warns when it does not. Also set
  `OOMPolicy=continue` (the shipped unit and the NixOS module do), so a kernel
  OOM kill of one worker fails that solve instead of stopping the whole
  service. Low-level and `.geo` input is capped at 100 points before the
  engine allocates anything.
- Version: `agstudio --version` → `agstudio 0.1.0 (<git revision>)`; the same
  in `/api/status` (`version`, `revision`) and the MCP `serverInfo`. Docker
  builds take the revision from the `GEOSOLVER_REVISION` build argument, Nix
  builds from the flake.

## Environment variables

| Env var | Default | Effect |
|---|---|---|
| `AGSTUDIO_BIND` | `127.0.0.1:<port>` | interface/port to bind (`0.0.0.0:8787` in the Docker image) |
| `AGSTUDIO_BASIC_AUTH` | (none) | the shared site password. `user:pass` checks both; `:pass` or a bare `pass` (no colon) accepts **any** username with that password. Set-but-unusable (empty, a password under 8 chars, or one still containing the template's `CHANGE-ME`) refuses to start. `AGSTUDIO_BASIC_AUTH_FILE` reads it from a file instead |
| `AGSTUDIO_GATE` | `form` | how browsers enter the shared password: `form` = the `/gate` password page, which sets a signed `gate` cookie (HTTP Basic is still accepted, for API clients and curl); `basic` = only the HTTP Basic dialog (the behaviour before the gate page) |
| `AGSTUDIO_GATE_KEY` | (generated, stored in the DB) | 64 hex chars: the HMAC key of the gate cookie. Malformed refuses to start. Rotating it (or the password) signs every device out. `AGSTUDIO_GATE_KEY_FILE` reads it from a file instead |
| `AGSTUDIO_GATE_DAYS` | 180 | how long the gate cookie lasts (1–400 days; renewed on visits after 7 days) |
| `AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN` | 10 | per-IP *wrong* shared passwords per minute (Basic and the gate form together) before 429 (0 = off) |
| `AGSTUDIO_GUEST_MODE` | on if `AGSTUDIO_BASIC_AUTH` is set, else off | `1`/`0` override. Visitors past the shared password may solve, export and humanize without an account (no history). `1` without a password refuses to start |
| `AGSTUDIO_GUEST_AI` | same as guest mode | `0` keeps Describe and Photo (`/api/translate`) for accounts only; guests still get AI explanations |
| `AGSTUDIO_GUEST_AI_PER_DAY` | 0 | per-IP guest `/api/translate` + `/api/humanize` calls per 24 h (one budget), counted once the request is valid (0 = no daily cap; the per-minute limit always applies; accounts are never counted) |
| `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
| `AGSTUDIO_MAX_CONCURRENT` | CPUs available, clamped to 2..8 | simultaneous heavy requests; each solve/export is one worker process |
| `AGSTUDIO_QUEUE_WAIT_SECS` | 5 | how long a heavy request waits for a free slot before 503 + `Retry-After` (max 60) |
| `AGSTUDIO_WORKER_MEM_MB` | 2048 | memory cap (`RLIMIT_DATA`: heap and thread stacks, not merely reserved address space) of each solve worker process; 0 = none |
| `AGSTUDIO_HUMAN_PROOFS` | 1 | 0 turns off the human-style proof writer (the Proof tab, MCP and reports then show only the full derivation) |
| `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests per minute (0 = off) |
| `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP `/api/translate` + `/api/humanize` per minute (0 = off) |
| `AGSTUDIO_AUTH_PER_MIN` | 15 | per-IP `/api/auth/login` + `register` per minute (0 = off) |
| `AGSTUDIO_MAX_BODY_KB` | 8192 | request body size limit |
| `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
| `AGSTUDIO_DISABLE_TRANSLATE` | off | turn off `/api/translate` and `/api/humanize` (no `claude` subprocess at all) |
| `AGSTUDIO_CLAUDE_BIN` | `claude` on `PATH`, then `~/.local/bin` | the Claude Code CLI used for Describe/Photo/explanations. When set it is authoritative: a wrong path reports "not installed" rather than falling back |
| `AGSTUDIO_CLAUDE_MODEL` | the CLI's default | `--model` for Describe/Photo translations |
| `AGSTUDIO_HUMANIZE_MODEL` | `claude-opus-4-8` | `--model` for AI explanations (`/api/humanize`) |
| `CLAUDE_CODE_OAUTH_TOKEN` | (none) | subscription token from `claude setup-token`; counts as signed in. Never an `ANTHROPIC_API_KEY` (it is stripped from the CLI's environment). `CLAUDE_CODE_OAUTH_TOKEN_FILE` reads it from a file instead (set one, not both) |
| `CLAUDE_CONFIG_DIR` | `~/.claude` | where the CLI keeps its state; must be writable by the service |
| `AGSTUDIO_TRUST_PROXY` | off | client IP = **rightmost** `X-Forwarded-For` entry, honoured only when the TCP peer is loopback/private/CGNAT (i.e. the proxy). Turn on behind `tailscale funnel`/nginx/Caddy, or every visitor shares one rate-limit bucket |
| `AGSTUDIO_PUBLIC_HOST` | (none) | comma list of hostnames (a `:port` is ignored) the app is served as (e.g. the funnel `*.ts.net` name). With a loopback bind and none set, only `localhost`/`127.0.0.1`/`[::1]` `Host` headers are accepted (DNS-rebinding guard); `/healthz` accepts any |
| `AGSTUDIO_DB` | `./agstudio.db` | SQLite file for accounts/sessions/history (`/data/agstudio.db` in the Docker image) |
| `AGSTUDIO_SECURE_COOKIES` | off | session cookie becomes `__Host-sid` + `Secure`, and HSTS is sent (needs HTTPS) |
| `AGSTUDIO_SHUTDOWN_GRACE_SECS` | 70 | on SIGTERM/SIGINT: stop accepting, let requests in flight finish for at most this long (default = solve deadline + worker grace + 5 s), checkpoint the DB, exit 0. Keep the service manager's stop timeout above it |
| `AGSTUDIO_ACCESS_LOG` | on | one stdout line per request: UTC time, client IP, method, path (no query), status, milliseconds; `/healthz` and `/readyz` are not logged. `0` turns it off |
| `AGSTUDIO_EXPORT_DIR` | `$XDG_DATA_HOME/geosolver/exports` (default `~/.local/share/geosolver/exports`) | MCP `export_report` writes only here (bare filenames, no overwrite unless asked) |
| `AUX_MAX_RUNS`, `AUX_MAX_DEPTH`, `RAYON_NUM_THREADS` | server-safe | solver effort caps (set automatically; override to tune). The run/depth caps bound only the legacy fallback search; the default search runs until each solve's 60 s wall-clock limit, on `RAYON_NUM_THREADS` threads per solve |

Flags take `1`/`0`, `true`/`false`, `yes`/`no` or `on`/`off` in any case;
any other value, or a number that does not parse, refuses to start (an empty
value means unset). The startup banner shows the effective settings, including
`secure cookies: on/off` and the version.

### Secrets in files

`AGSTUDIO_BASIC_AUTH_FILE`, `AGSTUDIO_GATE_KEY_FILE` and
`CLAUDE_CODE_OAUTH_TOKEN_FILE` read the value from a file (trailing newlines
are ignored), so it never appears in `docker inspect` or a unit file.
Setting both a variable and its `_FILE` refuses to start. With compose:

```yaml
services:
  app:
    environment:
      AGSTUDIO_BASIC_AUTH_FILE: /run/secrets/agstudio_basic_auth
    secrets: [agstudio_basic_auth]
secrets:
  agstudio_basic_auth:
    file: ./basic_auth.txt   # keep it out of version control, mode 600
```

With systemd: `LoadCredential=basic_auth:/etc/agstudio/basic_auth` and
`Environment=AGSTUDIO_BASIC_AUTH_FILE=%d/basic_auth`.

Compose-only (read by `docker compose` from `deploy/.env`, not by the app):
`GEOSOLVER_TARGET` (`runtime` or `ai`), `GEOSOLVER_CPUS`,
`GEOSOLVER_MEM_LIMIT`, `GEOSOLVER_REVISION`.

## AI on a server: Describe, Photo and explanations

Typing/pasting `.geo` programs and solving works everywhere with no setup. The
**Describe / Photo** feature and AI explanations (open to guests too, unless
`AGSTUDIO_GUEST_AI=0`) drive the `claude` CLI on *your Claude subscription*,
never an API key. A server needs the CLI and a subscription token:

1. On any machine with a browser and the CLI: `claude setup-token`. It prints
   a long-lived `sk-ant-oat…` token.
2. Give the service `CLAUDE_CODE_OAUTH_TOKEN=<token>` in a file only it reads:
   - **A (compose):** set `GEOSOLVER_TARGET=ai` and the token in
     `deploy/.env` (mode 600), then `up -d --build`. The `ai` image target
     adds Node 22 and the CLI; its config lives on a tmpfs, so nothing is
     written to the read-only image. Mounting a host `~/.claude` does **not**
     work (different uid, and macOS keeps the login in the Keychain).
   - **B:** `docker build --target ai …` and `-e CLAUDE_CODE_OAUTH_TOKEN=…`
     (or `--env-file`).
   - **C (systemd):** install the CLI, add the token and
     `AGSTUDIO_CLAUDE_BIN=<absolute path>` to `/etc/agstudio.env`, and apply
     the override in the unit's comment (`MemoryDenyWriteExecute=no`, `HOME`
     and `CLAUDE_CONFIG_DIR` inside `/var/lib/agstudio`): the CLI's JIT needs
     writable+executable memory, and DynamicUser has no home of its own.
   - **D (NixOS):** `ai.package = pkgs.claude-code;` then
     `sudo geosolver-claude-token`.
3. Restart. The startup log says `AI translation: on (…)`, `/api/status`
   reports `"can_translate": true`, and the app's pill turns green.

Until a token exists the log says `AI translation: installed — run claude auth
login to enable`, `/api/status` reports `translate_block: "not_logged_in"`, and
the pill reads "AI not set up yet" (not "off", which means
`AGSTUDIO_DISABLE_TRANSLATE=1`). If you don't want the AI at all, set
`AGSTUDIO_DISABLE_TRANSLATE=1`: Describe and Photo then say why they are
unavailable and point to the `.geo` tab, and AI explanations are not offered.

## Builds

- `deploy/build-linux.sh` and `run.sh` build with `.cargo/config.toml`'s
  `target-cpu=native`: fastest on that machine, may not run on an older CPU.
- The Docker image and the Nix package are **portable** (baseline x86-64 or
  aarch64). The Dockerfile removes the native setting; in Nix, nixpkgs'
  cargo hook writes a `target.<triple>.rustflags` entry that takes precedence
  over `build.rustflags`, so the native flag is not applied. On the IMO
  benchmark the two measured the same within noise
  (`hyperfine 'ddar --bench'`: 356 ms ± 43 native vs 360 ms ± 53 portable).
- Builds are reproducible from `Cargo.lock` (`--locked` everywhere). The
  Dockerfile pins its base images by digest (bump tag and digest together)
  and caches the cargo registry and `target/` with BuildKit, so a source edit
  rebuilds only the two workspace crates. `ARG CARGO_BUILD_JOBS` limits
  compile parallelism on a shared builder.
- Licences: `LICENSE`, `NOTICE`, `THIRD_PARTY_LICENSES.md` (regenerate with
  `deploy/third-party-licenses.sh`) and the fonts' licences are installed in
  the image under `/usr/share/doc/geosolver/` and served at `/licenses`.

## Mobile / LAN access

The UI is responsive and touch-friendly. To use it from a phone on your LAN
without a full deployment:

```sh
pw=$(openssl rand -hex 16); echo "password: $pw"
AGSTUDIO_BIND=0.0.0.0:8787 AGSTUDIO_BASIC_AUTH=":$pw" ./target/release/agstudio serve
# then browse to  http://<your-computer-LAN-IP>:8787  and enter that password
```

For anything
reachable from the internet, use TLS (routes A–D) — the password page and
Basic auth both send the password in the clear without HTTPS.

On an iPhone, Safari's Share → **Add to Home Screen** installs GeoSolver as an
app (icon, name and launch screen come from the server). The Home Screen app
has its own cookies, separate from Safari, so it asks for the shared password
once more on first launch.
