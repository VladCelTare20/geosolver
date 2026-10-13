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
  the internet through Tailscale Funnel. Import the flake's module as shown;
  a hand-copied service definition does not get its hardening (KillMode,
  OOMPolicy, the daily backup timer, the worker memory cap).

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
> `AGSTUDIO_PUBLIC_HOST=<your.domain>` (on a loopback bind, routes C and D,
> every proxied request otherwise gets 403 `unknown host`; on the containers'
> `0.0.0.0` bind, routes A and B, the Host check — the DNS-rebinding guard —
> is simply off without it, and the banner warns), `AGSTUDIO_TRUST_PROXY=1` (otherwise every visitor shares
> the proxy's IP, so one person's wrong passwords lock everyone out for a
> minute) and `AGSTUDIO_SECURE_COOKIES=1` (`Secure` `__Host-` cookies and
> HSTS). Keep the app's own bind on loopback (or a private network only the
> proxy can reach): `TRUST_PROXY` believes `X-Forwarded-For` from any
> loopback/private peer. Route A's compose file sets the last two for you.
>
> `TRUST_PROXY` uses the **rightmost** `X-Forwarded-For` entry, so the proxy
> must overwrite the header or append the real client address to it — never
> pass a client's value through untouched, or every per-IP limit (rate, AI
> budget, wrong-password lockout) is keyed on a value the client chose. The
> shipped configs do: `deploy/nginx.conf` and `deploy/nginx-host.conf` set it
> to `$remote_addr`, Caddy replaces it unless `trusted_proxies` says
> otherwise, Cloudflare appends the address it saw, and Tailscale
> Serve/Funnel overwrites it with the connection's source address (checked in
> tailscale 1.102.2, `ipn/ipnlocal/serve.go:addProxyForwardedHeaders`; Go's
> `ReverseProxy` with `Rewrite` also drops the incoming `X-Forwarded-*`
> headers first). Any other proxy: check before turning `TRUST_PROXY` on.

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
memory), so **RAM ≥ slots × WORKER_MEM_MB + ~300 MB**: 2 slots want 4.4 GB
(the compose file's and route B's default limit is 5 GB), 8 slots want 17 GB.
On a 4 GB box set `AGSTUDIO_WORKER_MEM_MB=1843` (2 slots), or lower the slots.

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
#   run `openssl rand -hex 16`, then edit deploy/.env: uncomment
#   AGSTUDIO_BASIC_AUTH=:<that output> (paste the output itself: an env file
#   does not run `$(...)`), and set AGSTUDIO_PUBLIC_HOST=your.domain
#   (both required). Without the password (or still CHANGE-ME) the app refuses
#   to start; without PUBLIC_HOST it starts with the Host check off and says so
#   in its banner (`Warning: AGSTUDIO_PUBLIC_HOST is not set ...`).

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
`Cargo.lock`, `ag-studio/`, `alphageometry-rs/`, `corpus/` and the licence files,
minus `target/`, `node_modules/`, `.env`, `*.db*` and logs. `deploy/.env`,
`deploy/certs/` and a local `agstudio.db` never reach the builder, and editing
them does not invalidate the cargo build layer.

The app container runs read-only with all capabilities dropped,
`no-new-privileges`, a `pids` limit, and a CPU and memory limit
(`GEOSOLVER_CPUS`, default 2; `GEOSOLVER_MEM_LIMIT`, default 5g — set them in
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
# once: the password goes in an owner-only env file that every `docker run` reuses
install -m 600 /dev/null geosolver.env
echo "AGSTUDIO_BASIC_AUTH=:$(openssl rand -hex 16)" >> geosolver.env
docker run -d --name geosolver -p 127.0.0.1:8787:8787 \
  --env-file geosolver.env \
  -e AGSTUDIO_PUBLIC_HOST=your.domain -e AGSTUDIO_TRUST_PROXY=1 -e AGSTUDIO_SECURE_COOKIES=1 \
  -v geosolver-data:/data \
  --read-only --tmpfs /tmp --tmpfs /home/agstudio:uid=10001,gid=10001,mode=0700 \
  --cap-drop ALL --security-opt no-new-privileges --cpus 2 --memory 5g --pids-limit 512 \
  --stop-timeout 90 --restart unless-stopped geosolver
```

Keep `geosolver.env` (the password is in it: `grep BASIC_AUTH geosolver.env`)
and reuse it when you recreate the container; a new password signs every device
out. `--stop-timeout 90` lets `docker stop` wait for solves in flight (Docker's
default is 10 s).

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
    environment.AGSTUDIO_AI_PER_DAY_TOTAL = "400";
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
runs a self-test inside the service sandbox. With `memoryMax` ("9G") and
`maxConcurrent` set, each worker is capped at 90 % of the limit divided by the
slots (at most 2048 MB; `workerMemoryMb` overrides), so the app's own
overcommit warning stays quiet; the unit also gets `KillMode=mixed`,
`TimeoutStopSec=90`, `OOMPolicy=continue` and `LimitNOFILE=65536`. The funnel
unit takes `flock /run/tailscale-serve.lock` around every `tailscale
funnel`/`serve` call and clears the port with `serve off` on stop, so it
cannot race another unit that publishes on the same tailnet node. Funnel is per port: everything
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
# A (compose): written to the container's /tmp (a tmpfs), streamed out, then deleted
(umask 077; docker compose -f deploy/docker-compose.yml exec -T app sh -c \
  'agstudio backup /tmp/b.db >&2 && cat /tmp/b.db; s=$?; rm -f /tmp/b.db; exit $s' \
  > geosolver-$(date +%F-%H%M%S).db)
# B (docker run): the same through docker exec
(umask 077; docker exec geosolver sh -c \
  'agstudio backup /tmp/b.db >&2 && cat /tmp/b.db; s=$?; rm -f /tmp/b.db; exit $s' \
  > geosolver-$(date +%F-%H%M%S).db)
# C (systemd)
sudo /opt/agstudio/agstudio backup /root/geosolver-$(date +%F-%H%M%S).db --db /var/lib/agstudio/agstudio.db
# D (NixOS)
sudo geosolver-backup
```

Nothing is left inside the data volume, and the timestamp makes a second run
on the same day a new file (`agstudio backup` never overwrites one).

Schedule it (a cron line or systemd timer per route; route D does it daily)
and copy the files off the machine. The server also checkpoints the WAL into
the main file every 10 minutes and on a clean stop (`SIGTERM`/Ctrl-C), but a
copy of the bare `.db` taken while it runs can still miss recent writes — a
filesystem or volume snapshot must include `agstudio.db-wal` and
`agstudio.db-shm` too.

**Restore** (the server must be stopped; the backup replaces everything,
including the gate key, so signed-in devices stay signed in):

```sh
# A
docker compose -f deploy/docker-compose.yml stop app
docker compose -f deploy/docker-compose.yml run --rm --no-deps -T --entrypoint sh app -c \
  'set -e; cd /data; cat > restore.tmp; chmod 600 restore.tmp; rm -f agstudio.db-wal agstudio.db-shm; mv restore.tmp agstudio.db' < ./backup.db
docker compose -f deploy/docker-compose.yml start app
# B
docker stop geosolver
docker run --rm -i -v geosolver-data:/data --entrypoint sh geosolver -c \
  'set -e; cd /data; cat > restore.tmp; chmod 600 restore.tmp; rm -f agstudio.db-wal agstudio.db-shm; mv restore.tmp agstudio.db' < ./backup.db
docker start geosolver
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

1. **Back up first** (above). On a release older than `agstudio backup`, see
   [Upgrading from a release without `agstudio backup`](#upgrading-from-a-release-without-agstudio-backup).
2. Update: `git pull`, then
   `GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) docker compose -f deploy/docker-compose.yml up -d --build`
   (route C: rebuild, `install`, `systemctl restart agstudio`; route D: see
   below).
3. Check `/api/status` → `"version"` and `"revision"`, or
   `agstudio --version`.

Route D: a flake input locks one commit of one branch or tag. If the input is
`git+file:///path/to/geosolver?ref=<branch>`, the release you want must be on
that branch: merge or fast-forward it there (or change `ref`), then
`nix flake update geosolver` in the machine's flake, commit, and
`nixos-rebuild build`, `test`, `switch`. Updating a lock that still names the
old branch deploys nothing new. After the switch, `agstudio --version` must
print the commit you released.

The database records its schema version (`PRAGMA user_version`). An upgrade
migrates it forward in place. Builds from this release on refuse to open a
database written by a newer one (`schema version N, newer than this build's
M`) rather than damaging it; older builds have no such check, but the one
migration so far (version 0 → 1) only adds to the schema, and the previous
release runs on a migrated database unchanged. So to roll back: check out the
previous release, rebuild, and restore the backup taken in step 1.

Quick rollback without rebuilding — tag the running image before updating,
then start the tagged one if the update misbehaves:

```sh
# A: compose builds the image as geosolver-app:${GEOSOLVER_IMAGE_TAG:-latest}
docker tag geosolver-app:latest geosolver-app:previous           # before `up -d --build`
GEOSOLVER_IMAGE_TAG=previous docker compose -f deploy/docker-compose.yml up -d --no-build app
# B
docker tag geosolver geosolver:previous                          # before `docker build`
docker stop -t 90 geosolver && docker rm geosolver
#   then the route B `docker run` line with `geosolver:previous` as the image
```

If the newer build migrated the schema, restore the step 1 backup as well
(see [Backup and restore](#backup-and-restore)).

### Upgrading from a release without `agstudio backup`

Releases before this one (for example `0a6e67b`) have no `backup`,
`--version` or `doctor` subcommand, so step 1 above fails with `unknown
subcommand: backup`. For that one upgrade:

1. **Password.** If your env file still has the template's
   `AGSTUDIO_BASIC_AUTH=:CHANGE-ME-run-openssl-rand-hex-16` (the old
   `.env.example` shipped it uncommented, and the old build accepted it), the
   new build refuses to start. That password was published in this
   repository, so treat it as compromised: set a new one
   (run `openssl rand -hex 16` and paste its output after
   `AGSTUDIO_BASIC_AUTH=:`) before upgrading, and tell your users.
2. **Back up with the new build, the service stopped.** Never copy
   `agstudio.db` alone: the old release leaves its data in `agstudio.db-wal`
   even after a clean stop (seen: a 4 KB `agstudio.db` beside a 129 KB WAL).
   Build the new release first, without starting it, and let its
   `agstudio backup` read the old database, WAL included:

   ```sh
   # A: the old container keeps running until `stop`
   git pull
   docker tag deploy-app:latest geosolver-app:previous   # the old image, for a quick rollback
   GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) docker compose -f deploy/docker-compose.yml build app
   docker compose -f deploy/docker-compose.yml stop app
   (umask 077; docker compose -f deploy/docker-compose.yml run --rm --no-deps -T --entrypoint sh app -c \
     'agstudio backup /tmp/b.db >&2 && cat /tmp/b.db' > geosolver-pre-upgrade.db)
   # B
   git pull
   docker build -t geosolver:new --build-arg GEOSOLVER_REVISION=$(git rev-parse --short=12 HEAD) .
   docker stop -t 90 geosolver
   (umask 077; docker run --rm -v geosolver-data:/data --entrypoint sh geosolver:new -c \
     'agstudio backup /tmp/b.db >&2 && cat /tmp/b.db' > geosolver-pre-upgrade.db)
   docker tag geosolver geosolver:previous && docker tag geosolver:new geosolver
   # C: the new binary, built (deploy/build-linux.sh prints its path) but not installed yet
   sudo systemctl stop agstudio
   sudo ./target/release/agstudio backup /root/geosolver-pre-upgrade.db --db /var/lib/agstudio/agstudio.db
   #    or with the sqlite3 shell, which also reads the WAL:
   #    sudo sqlite3 /var/lib/agstudio/agstudio.db ".backup /root/geosolver-pre-upgrade.db"
   ```

   Each `agstudio backup` prints the number of users and history rows and
   `integrity ok`. Then update as in step 2 of the list above (B: `docker rm
   geosolver` and the route B `docker run` again).
3. **Route C:** the unit and the env file changed too. Re-copy
   `deploy/agstudio.service` to `/etc/systemd/system/`, run
   `sudo systemctl daemon-reload`, and `sudo chmod 600 /etc/agstudio.env`
   (the old instructions left it world-readable), then install the new binary
   and `sudo systemctl start agstudio`. `systemctl show agstudio -p KillMode
   -p OOMPolicy` should print `KillMode=mixed` and `OOMPolicy=continue`.
4. **Route A:** compose now waits 90 s on stop and limits memory to 5 GB by
   default (`GEOSOLVER_MEM_LIMIT`); `up -d --build` applies both.

Sessions, history, accounts and gate cookies survive the upgrade.

**`docker compose down -v` deletes the data volume** — accounts, history and
the gate key. `down` without `-v`, `stop`, `restart` and `up --build` keep it.

## Health, logs, version

- Liveness: `GET /healthz` → `ok` (no password, any `Host`, never waits for a
  solve slot). Docker health checks use it for both containers.
- Readiness: `GET /readyz` (unauthenticated) → 200 `{"ready":true}` when the
  database takes a write, a solver slot is free and no shutdown is under way;
  otherwise 503 `{"ready":false}` with `Retry-After: 10` (no slot counts or
  other detail, since anyone can call it). It turns 503 whenever every
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
  `stop_grace_period: 90s` and route B's `docker run` passes
  `--stop-timeout 90` (Docker's default of 10 s would cut every solve in
  flight). A "Keep searching" solve (up to 180 s) still running when the grace
  ends is cut off; its user sees a failed request and can run it again. Workers notice a dead server within 100 ms and exit on their own.
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
| `AGSTUDIO_GUEST_AI_PER_DAY` | 60 with guest mode, else 0 | per-IP `/api/translate` + `/api/humanize` calls per 24 h (one budget), counted once the request is valid, for guests and accounts alike, so registering an account does not reset it (0 = no daily cap; the per-minute limit always applies) |
| `AGSTUDIO_AI_PER_DAY_TOTAL` | 0 | server-wide `/api/translate` + `/api/humanize` calls per 24 h, every caller and address together: the ceiling on what the subscription spends (0 = none) |
| `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
| `AGSTUDIO_MAX_CONCURRENT` | CPUs available, clamped to 2..8 | simultaneous heavy requests; each solve/export is one worker process |
| `AGSTUDIO_QUEUE_WAIT_SECS` | 5 | how long a heavy request waits for a free slot before 503 + `Retry-After` (max 60) |
| `AGSTUDIO_SOLVE_DEADLINE_SECS` | 60 | wall-clock limit of one web solve or export (10–600); the request timeout grows to fit. Your proxy's read timeout must exceed max(this, `EXTENDED_DEADLINE`) + 40 s (the shipped nginx and Caddy files wait 240 s) |
| `AGSTUDIO_EXTENDED_DEADLINE_SECS` | 180 | limit of the "Keep searching" solve offered after a time-limit verdict (one at a time, server-wide; 0 = not offered; at most 900). Raising it means raising the proxy timeout too; a restart drains for `SHUTDOWN_GRACE_SECS` (70 s) only, so it cuts off a longer extended solve |
| `RAYON_NUM_THREADS` | unset | solver threads per worker; unset, the CPUs (cgroup quota included) minus one are shared among the workers running at once |
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
| `AGSTUDIO_MAX_CONNECTIONS` | 0 = half the soft open-files limit (64–4096) | simultaneous client connections; one over the cap is closed at once, so an idle-connection flood cannot exhaust file descriptors and wedge `accept`. Give the service `LimitNOFILE=65536` |
| `AGSTUDIO_MAX_ACCOUNTS` | 2000 | registered accounts; past it `/api/auth/register` answers 503 (guests and existing accounts are unaffected; 0 = no limit) |
| `AGSTUDIO_MAX_DB_MB` | 4096 | database size at which new history rows are stored without their full solution (they reopen by re-solving) and a warning is logged once (0 = no limit) |
| `AGSTUDIO_ACCESS_LOG` | on | one stdout line per request: UTC time, client IP, method, path (no query), status, milliseconds; `/healthz` and `/readyz` are not logged. `0` turns it off |
| `AGSTUDIO_EXPORT_DIR` | `$XDG_DATA_HOME/geosolver/exports` (default `~/.local/share/geosolver/exports`) | MCP `export_report` writes only here (bare filenames, no overwrite unless asked) |
| `AUX_MAX_RUNS`, `AUX_MAX_DEPTH` | server-safe | solver effort caps (set automatically; override to tune). They bound only the legacy fallback search; the default search runs until each solve's wall-clock limit |

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
    file: ./basic_auth.txt   # keep it out of version control
```

Outside Swarm, compose bind-mounts the file as it is (the long syntax's
`uid`/`mode` are ignored), and the app runs as uid 10001, so the file must
belong to that user or the app exits with `Permission denied`:
`sudo chown 10001:10001 basic_auth.txt && sudo chmod 400 basic_auth.txt`. The
same goes for a gate-key or Claude-token file.

With systemd: `LoadCredential=basic_auth:/etc/agstudio/basic_auth` and
`Environment=AGSTUDIO_BASIC_AUTH_FILE=%d/basic_auth`.

Compose-only (read by `docker compose` from `deploy/.env`, not by the app):
`GEOSOLVER_TARGET` (`runtime` or `ai`), `GEOSOLVER_CPUS`,
`GEOSOLVER_MEM_LIMIT`, `GEOSOLVER_REVISION`, `GEOSOLVER_IMAGE_TAG` (the tag
of the `geosolver-app` image compose builds and runs; default `latest`), and
the build arguments
`GEOSOLVER_BUILD_JOBS` (compile parallelism; empty = all cores) and
`CLAUDE_CODE_VERSION` (the CLI in the `ai` image; empty = the version pinned
in the Dockerfile).

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
   - **C (systemd):** install the CLI system-wide, e.g.
     `sudo npm install -g @anthropic-ai/claude-code` (giving
     `/usr/local/bin/claude`) or a copy under `/opt` — never under `/home` or
     `/root`, which the unit cannot see (`ProtectHome=yes`; the CLI's own
     per-user installer puts it in `~/.local/bin`). Add the token and
     `AGSTUDIO_CLAUDE_BIN=/usr/local/bin/claude` to `/etc/agstudio.env`, and
     install the drop-in `deploy/agstudio-ai.conf`
     (`sudo install -Dm644 deploy/agstudio-ai.conf /etc/systemd/system/agstudio.service.d/ai.conf`,
     then `sudo systemctl daemon-reload`): `MemoryDenyWriteExecute=no`, since
     the CLI's JIT needs writable+executable memory, and `HOME`/`CLAUDE_CONFIG_DIR`
     inside `/var/lib/agstudio`, since DynamicUser has no home of its own.
     Without the drop-in, `claude --version` still runs, so the banner says AI
     is on, but every model call dies once its JavaScript is compiled; each
     such failure is logged with `hint: … install deploy/agstudio-ai.conf`.
     `agstudio doctor` below catches it before users do.
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
  rebuilds only the two workspace crates. `ARG BUILD_JOBS` limits
  compile parallelism on a shared builder (`docker build --build-arg
  BUILD_JOBS=4 …`; with compose, `GEOSOLVER_BUILD_JOBS=4` in `deploy/.env`;
  it is deliberately not named `CARGO_BUILD_JOBS`, which cargo reads itself
  and rejects when empty), and the `ai` target installs a
  pinned Claude Code CLI (`ARG CLAUDE_CODE_VERSION`); compose forwards both
  from `deploy/.env`.
- Licences: `LICENSE`, `NOTICE`, `THIRD_PARTY_LICENSES.md` (regenerate with
  `deploy/third-party-licenses.sh`) and the fonts' licences are installed in
  the image under `/usr/share/doc/geosolver/` and served at `/licenses`.

Neither `claude auth status` nor the startup log can tell a valid token from an
expired or revoked one: any non-empty `CLAUDE_CODE_OAUTH_TOKEN` counts as
signed in until a model call is made. Check it with **`agstudio doctor`**, run
with the service's environment: it makes one tiny model call and prints
`AI: on`, or the CLI's own words (`Failed to authenticate. API Error: 401 …`)
and exits 1. Run it inside the service's environment, not from a login shell:

```sh
# A
docker compose -f deploy/docker-compose.yml exec app agstudio doctor
# B
docker exec geosolver agstudio doctor
# C: the unit's env file, an unprivileged user and the unit's own
#    MemoryDenyWriteExecute (so a missing agstudio-ai.conf shows up here). A
#    plain `sudo …/agstudio doctor` sees no env file and reports "the claude CLI
#    was not found". HOME is a private /tmp, so the running service's state
#    directory is left alone.
sudo systemd-run --quiet --wait --pipe --collect \
  -p EnvironmentFile=/etc/agstudio.env -p DynamicUser=yes -p PrivateTmp=yes \
  -p ProtectHome=yes -p ProtectSystem=strict \
  -p MemoryDenyWriteExecute=$(systemctl show agstudio -p MemoryDenyWriteExecute --value) \
  -E HOME=/tmp -E CLAUDE_CONFIG_DIR=/tmp/claude -E DISABLE_AUTOUPDATER=1 \
  /opt/agstudio/agstudio doctor
# D
sudo geosolver-claude-token   # installs a token and runs the same self-test
```

At run time, a call the API refuses is logged with what the CLI
printed (`translate error: the claude CLI is not signed in … (claude said
stdout: Failed to authenticate …)`), and for the next 10 minutes `/api/status`
reports `translate_block: "not_logged_in"` so the app stops offering Describe
and Photo; the next successful call clears it.

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
