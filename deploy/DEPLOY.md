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
`AGSTUDIO_BASIC_AUTH` is set but too weak to use. With Basic auth on, **guest
mode** lets anyone with the password solve and export without an account —
share a link and a password, nothing else. It also has a per-IP rate
limit, a concurrency cap, a request-body limit, and CSP/security headers.

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
| `AGSTUDIO_BASIC_AUTH` | (none) | require HTTP Basic auth. `user:pass` checks both; `:pass` or a bare `pass` (no colon) accepts **any** username with that password. Set-but-unusable (empty, or a password under 8 chars) refuses to start |
| `AGSTUDIO_BASIC_AUTH_FAILS_PER_MIN` | 10 | per-IP *wrong* Basic credentials per minute before 429 (0 = off) |
| `AGSTUDIO_GUEST_MODE` | on if `AGSTUDIO_BASIC_AUTH` is set, else off | `1`/`0` override. Visitors past Basic auth may solve, export and humanize without an account (no history). `1` without Basic auth refuses to start |
| `AGSTUDIO_ALLOW_INSECURE` | off | permit a public bind with no auth (proxy only) |
| `AGSTUDIO_MAX_CONCURRENT` | ~CPUs | simultaneous heavy requests (excess → 503) |
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
| `AUX_MAX_RUNS`, `AUX_MAX_DEPTH`, `RAYON_NUM_THREADS` | server-safe | solver effort caps (set automatically; override to tune). Each web solve is also capped at 60 s wall clock |

## Health, logs, updates

- Liveness: `GET /healthz` → `ok` (unauthenticated, for load balancers).
- Logs: `docker compose logs -f app` / `journalctl -u agstudio -f`.
- Update: `git pull && docker compose -f deploy/docker-compose.yml up -d --build`.

## Photo / natural-language translation on a server

Typing/pasting `.geo` programs and solving works everywhere with no setup. The
**Describe / photo** feature drives the local `claude` CLI on *your Claude
subscription*, so it needs that CLI installed and signed in on the host:

- **Docker:** the default image does not include `claude`, so translation is off.
  To enable it, build a variant that installs `@anthropic-ai/claude-code` and
  mount a signed-in `~/.claude` into the container (`-v ~/.claude:/home/agstudio/.claude:ro`).
- **Native:** install `claude`, run `claude auth login` as the service user, and
  relax `ProtectHome=` in the unit.

If you don't need it, leave it off — the UI simply hides the Describe mode.

## Mobile / LAN access

The UI is responsive and touch-friendly. To use it from a phone on your LAN
without a full deployment:

```sh
AGSTUDIO_BIND=0.0.0.0:8787 AGSTUDIO_BASIC_AUTH="me:secret" ./target/release/agstudio serve
# then browse to  http://<your-computer-LAN-IP>:8787  from the phone
```

For anything reachable from the internet, use TLS (option A) — Basic auth
without HTTPS sends the password in the clear.
