# Deploying AlphaGeometry Studio on a Linux server

The whole app is a **single static binary** — the web UI, fonts, and grammar are
baked in. There is no database and no runtime assets. Pick one of:

- **A. Docker Compose + nginx (TLS)** — recommended for a public host.
- **B. Plain `docker run`** — behind your own proxy / Cloudflare Tunnel.
- **C. Native binary + systemd** — no Docker.

Everything is **secure by default**: the server *refuses to start* on a
non-loopback interface unless you set `AGSTUDIO_BASIC_AUTH` (or explicitly opt
out with `AGSTUDIO_ALLOW_INSECURE=1` behind a proxy). It also has a per-IP rate
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
docker run -d --name agstudio -p 8787:8787 \
  -e AGSTUDIO_BASIC_AUTH="user:strong-password" \
  --restart unless-stopped agstudio
```

Then front it with your own TLS proxy (nginx, Caddy, Traefik, Cloudflare
Tunnel). Set `AGSTUDIO_TRUST_PROXY=1` (default on) so the real client IP is read
from `X-Forwarded-For`.

## C. Native binary + systemd

```sh
./deploy/build-linux.sh
sudo install -Dm755 target/release/agstudio /opt/agstudio/agstudio
sudo cp deploy/agstudio.service /etc/systemd/system/
sudo cp deploy/.env.example /etc/agstudio.env      # then edit it
sudo systemctl daemon-reload
sudo systemctl enable --now agstudio
```

Requires [Rust](https://rustup.rs) to build. The unit is hardened (DynamicUser,
seccomp, ProtectSystem=strict, no capabilities).

---

## Environment variables

| Variable | Default | Purpose |
|---|---|---|
| `AGSTUDIO_BIND` | `127.0.0.1:8787` | interface:port to bind |
| `AGSTUDIO_BASIC_AUTH` | (none) | `user:pass` — require HTTP Basic auth. **Required for a public bind.** |
| `AGSTUDIO_ALLOW_INSECURE` | off | allow a non-loopback bind with no auth (only if a proxy is the sole entry point) |
| `AGSTUDIO_TRUST_PROXY` | on | read the client IP from `X-Forwarded-For` |
| `AGSTUDIO_MAX_CONCURRENT` | ~CPUs | simultaneous heavy requests (503 when full) |
| `AGSTUDIO_RATE_PER_MIN` | 120 | per-IP `/api/*` requests/min (0 = off) |
| `AGSTUDIO_TRANSLATE_PER_MIN` | 12 | per-IP translations/min |
| `AGSTUDIO_MAX_BODY_KB` | 8192 | request body limit |
| `AGSTUDIO_MAX_INPUT_CHARS` | 16384 | max program length |
| `AGSTUDIO_DISABLE_TRANSLATE` | off | disable the `/api/translate` endpoint |
| `AUX_MAX_RUNS`, `AUX_MAX_DEPTH`, `RAYON_NUM_THREADS` | server-safe | solver effort caps (set automatically; override to tune) |

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
