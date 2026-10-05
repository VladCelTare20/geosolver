# Deploying GeoSolver on a Linux server

The whole app is a **single binary** — the web UI, fonts, and grammar are baked
in — plus **one SQLite file** (`AGSTUDIO_DB`) holding accounts, sessions and
solve history. That file is the only state: put it on persistent storage and
back it up. Each option below does that for you. Pick one of:

- **A. Docker Compose + nginx (TLS)** — recommended for a public host.
- **B. Plain `docker run`** — behind your own proxy / Cloudflare Tunnel.
- **C. Native binary + systemd** — no Docker.

Everything is **secure by default**: the server *refuses to start* on a
non-loopback interface unless you set `AGSTUDIO_BASIC_AUTH` (or explicitly opt
out with `AGSTUDIO_ALLOW_INSECURE=1` behind a proxy), and refuses to start if
`AGSTUDIO_BASIC_AUTH` is set but too weak to use. With a shared password on,
**guest mode** lets anyone with the password solve, export and use the AI
features without an account — share a link and a password, nothing else. It
also has a per-IP rate limit, a concurrency cap, a request-body limit, and
CSP/security headers.

### The shared password: the gate page

Browsers get a password page (`/gate`) instead of the HTTP Basic dialog. The
right password sets an `HttpOnly` cookie (`__Host-gate` with
`AGSTUDIO_SECURE_COOKIES=1`) signed with HMAC-SHA256 over the password, valid
for `AGSTUDIO_GATE_DAYS` (180) and renewed on visits after 7 days, so a phone —
including the app added to its Home Screen — asks once, not on every cold
start. Changing the password, or `AGSTUDIO_GATE_KEY`, signs every device out;
"Forget this device" in the app footer signs out just that one. The cookie key
is generated once and stored in the database (table `server_secrets`), so
restarts and deploys keep everyone signed in.

HTTP Basic still works for API clients (`curl -u x:PASSWORD …`): requests
without a `Sec-Fetch-Mode` header still get the `WWW-Authenticate` challenge.
Browser API calls without the password get `401 {"code": "gate"}` and the app
goes to the gate page. Wrong passwords on the form and on Basic share one
per-IP budget (`AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN`). `AGSTUDIO_GATE=basic`
restores the Basic-only behaviour.

Without the password only these are served: `/healthz`, `/gate`,
`/manifest.webmanifest`, the icons (`/favicon.svg`, `/favicon.ico`,
`/apple-touch-icon*.png`, `/icons/*`, `/splash/*`), `/assets/app.css`, the
fonts and `/robots.txt` — static files with nothing secret, which iOS fetches
without cookies when a page is added to the Home Screen.

### Guests and solver slots

Guests are keyed by their `gid` cookie, and all guests behind one address
share every solver slot but one (`AGSTUDIO_MAX_CONCURRENT` − 1). Friends on one
Wi-Fi network or behind one mobile carrier's NAT therefore compete: with the
default 4 slots, a fourth simultaneous guest solve from that address is told
the server is busy and retries once by itself. That is deliberate — the `gid`
cookie is not signed, so a client could mint new ids to take every slot —
and fine for a small audience; raise `AGSTUDIO_MAX_CONCURRENT`, or have
regular users create accounts (accounts are not limited per address).

---

## A. Docker Compose + nginx (recommended)

```sh
cp deploy/.env.example deploy/.env
#   edit deploy/.env — set AGSTUDIO_BASIC_AUTH to "user:strong-password"

mkdir -p deploy/certs
#   put your certificate at deploy/certs/fullchain.pem and key at deploy/certs/privkey.pem
#   (for Let's Encrypt: certbot certonly --standalone -d your.domain, then copy or symlink)

docker compose -f deploy/docker-compose.yml up -d --build
```

Open `https://your.domain`. Only nginx is exposed (ports 80/443); the app runs
on the private container network. nginx terminates TLS and forwards the client
IP so rate limiting works.

No domain/cert yet? Generate a self-signed pair to test:

```sh
openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
  -keyout deploy/certs/privkey.pem -out deploy/certs/fullchain.pem -subj "/CN=localhost"
```

## B. Plain `docker run`

```sh
docker build -t agstudio .
docker run -d --name agstudio -p 127.0.0.1:8787:8787 \
  -e AGSTUDIO_BASIC_AUTH="user:strong-password" \
  -v agstudio-data:/data \
  --restart unless-stopped agstudio
```

Then front it with your own TLS proxy (nginx, Caddy, Traefik, Cloudflare
Tunnel). Set `AGSTUDIO_TRUST_PROXY=1` so the real client IP is read from
`X-Forwarded-For` — only when a proxy is the sole way in, otherwise clients can
forge the header and dodge the rate limits. The `-v agstudio-data:/data` volume
keeps accounts and history across `docker rm`/rebuilds.

## C. Native binary + systemd

```sh
./deploy/build-linux.sh
sudo install -Dm755 target/release/agstudio /opt/agstudio/agstudio
sudo cp deploy/agstudio.service /etc/systemd/system/
sudo cp deploy/.env.example /etc/agstudio.env      # then edit it
sudo systemctl daemon-reload
sudo systemctl enable --now agstudio
```

Requires Rust 1.88+ to build. The unit is hardened (DynamicUser, seccomp,
ProtectSystem=strict, no capabilities); its only writable path is
`StateDirectory=/var/lib/agstudio`, where the SQLite DB lives.

---

## Environment variables

| Env var | Default | Effect |
|---|---|---|
| `AGSTUDIO_BIND` | `127.0.0.1:<port>` | interface/port to bind |
| `AGSTUDIO_BASIC_AUTH` | (none) | the shared site password. `user:pass` checks both; `:pass` or a bare `pass` (no colon) accepts **any** username with that password. Set-but-unusable (empty, or a password under 8 chars) refuses to start |
| `AGSTUDIO_GATE` | `form` | how browsers enter the shared password: `form` = the `/gate` password page, which sets a signed `gate` cookie (HTTP Basic is still accepted, for API clients and curl); `basic` = only the HTTP Basic dialog (the behaviour before the gate page) |
| `AGSTUDIO_GATE_KEY` | (generated, stored in the DB) | 64 hex chars: the HMAC key of the gate cookie. Malformed refuses to start. Rotating it (or the password) signs every device out |
| `AGSTUDIO_GATE_DAYS` | 180 | how long the gate cookie lasts (1–400 days; renewed on visits after 7 days) |
| `AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN` | 10 | per-IP *wrong* shared passwords per minute (Basic and the gate form together) before 429 (0 = off) |
| `AGSTUDIO_GUEST_MODE` | on if `AGSTUDIO_BASIC_AUTH` is set, else off | `1`/`0` override. Visitors past the shared password may solve, export and humanize without an account (no history). `1` without a password refuses to start |
| `AGSTUDIO_GUEST_AI` | same as guest mode | `0` keeps Describe and Photo (`/api/translate`) for accounts only; guests still get AI explanations |
| `AGSTUDIO_GUEST_AI_PER_DAY` | 0 | per-IP guest `/api/translate` calls per 24 h, counted once the request is valid (0 = no daily cap; the per-minute limit always applies; accounts and `/api/humanize` are never counted) |
| `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
| `AGSTUDIO_MAX_CONCURRENT` | ~CPUs | simultaneous heavy requests; each solve/export is one worker process |
| `AGSTUDIO_QUEUE_WAIT_SECS` | 5 | how long a heavy request waits for a free slot before 503 + `Retry-After` (max 60) |
| `AGSTUDIO_WORKER_MEM_MB` | 2048 | memory cap (`RLIMIT_DATA`: heap and thread stacks, not merely reserved address space) of each solve worker process; 0 = none |
| `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests per minute (0 = off) |
| `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP `/api/translate` + `/api/humanize` per minute (0 = off) |
| `AGSTUDIO_AUTH_PER_MIN` | 15 | per-IP `/api/auth/login` + `register` per minute (0 = off) |
| `AGSTUDIO_MAX_BODY_KB` | 8192 | request body size limit |
| `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
| `AGSTUDIO_DISABLE_TRANSLATE` | off | turn off `/api/translate` and `/api/humanize` (no `claude` subprocess at all) |
| `AGSTUDIO_TRUST_PROXY` | off | client IP = **rightmost** `X-Forwarded-For` entry, honoured only when the TCP peer is loopback/private/CGNAT (i.e. the proxy). Turn on behind `tailscale funnel`/nginx, or every visitor shares one rate-limit bucket |
| `AGSTUDIO_PUBLIC_HOST` | (none) | comma list of hostnames the app is served as (e.g. the funnel `*.ts.net` name). With a loopback bind and none set, only `localhost`/`127.0.0.1`/`[::1]` `Host` headers are accepted (DNS-rebinding guard) |
| `AGSTUDIO_DB` | `./agstudio.db` | SQLite file for accounts/sessions/history |
| `AGSTUDIO_SECURE_COOKIES` | off | session cookie becomes `__Host-sid` + `Secure`, and HSTS is sent (needs HTTPS) |
| `AGSTUDIO_EXPORT_DIR` | `$XDG_DATA_HOME/geosolver/exports` (default `~/.local/share/geosolver/exports`) | MCP `export_report` writes only here (bare filenames, no overwrite unless asked) |
| `AUX_MAX_RUNS`, `AUX_MAX_DEPTH`, `RAYON_NUM_THREADS` | server-safe | solver effort caps (set automatically; override to tune). The run/depth caps bound only the legacy fallback search; the default search runs until each solve's 60 s wall-clock limit, on `RAYON_NUM_THREADS` threads per solve |

## Health, logs, updates

- Liveness: `GET /healthz` → `ok` (unauthenticated, for load balancers).
- Logs: `docker compose logs -f app` / `journalctl -u agstudio -f`.
- Update: `git pull && docker compose -f deploy/docker-compose.yml up -d --build`.

## Photo / natural-language translation on a server

Typing/pasting `.geo` programs and solving works everywhere with no setup. The
**Describe / photo** feature (open to guests too, unless `AGSTUDIO_GUEST_AI=0`)
drives the local `claude` CLI on *your Claude
subscription*, so it needs that CLI installed and signed in on the host:

- **Docker:** the default image does not include `claude`, so translation is off.
  To enable it, build a variant that installs `@anthropic-ai/claude-code` and
  mount a signed-in `~/.claude` into the container (`-v ~/.claude:/home/agstudio/.claude:ro`).
- **Native:** install `claude`, run `claude auth login` as the service user, and
  relax `ProtectHome=` in the unit.

If you don't need it, leave it off (`AGSTUDIO_DISABLE_TRANSLATE=1`): Describe
and Photo then say why they are unavailable and point to the `.geo` tab, and
AI explanations are not offered.

To turn it on for a deployment that has it off: remove
`AGSTUDIO_DISABLE_TRANSLATE`, make sure `claude auth status` reports
`"loggedIn": true` for the service user (or set `CLAUDE_CODE_OAUTH_TOKEN` in
the service environment), and restart. `/api/status` then reports
`"can_translate": true` for guests (with guest mode and `AGSTUDIO_GUEST_AI`
on) and for signed-in users.

## Mobile / LAN access

The UI is responsive and touch-friendly. To use it from a phone on your LAN
without a full deployment:

```sh
AGSTUDIO_BIND=0.0.0.0:8787 AGSTUDIO_BASIC_AUTH="me:secret" ./target/release/agstudio serve
# then browse to  http://<your-computer-LAN-IP>:8787  from the phone
```

For anything reachable from the internet, use TLS (option A) — the password
page and Basic auth both send the password in the clear without HTTPS.

On an iPhone, Safari's Share → **Add to Home Screen** installs GeoSolver as an
app (icon, name and launch screen come from the server). The Home Screen app
has its own cookies, separate from Safari, so it asks for the shared password
once more on first launch.
