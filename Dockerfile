# syntax=docker/dockerfile:1
# Portable multi-stage build of GeoSolver for Linux (amd64/arm64).
# The web UI, fonts, and grammar are baked into the binary, so the runtime image
# needs no assets and no system fonts. Accounts/sessions/history live in SQLite
# under /data, which must be a volume or every rebuild wipes them.
#
# Targets: `runtime` (default, last stage) has no AI; `ai` adds the claude CLI
# for Describe/Photo/AI explanations (docker build --target ai, or
# GEOSOLVER_TARGET=ai with compose) and needs CLAUDE_CODE_OAUTH_TOKEN at run time.
# Base images are pinned by digest; bump tag and digest together.

ARG RUST_IMAGE=rust:1.98-slim-bookworm@sha256:ff521445a372125ed4f76e1453a1f8098f2d05332d1601d30db1c1f62757e730
ARG DEBIAN_IMAGE=debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587
ARG NODE_IMAGE=node:22-bookworm-slim@sha256:c3de60bf2f9dd0ac6370e6117950ff62d6e339527e7472301c9c78a017978392

# ---- build ----
FROM ${RUST_IMAGE} AS build
ARG GEOSOLVER_REVISION=unknown
ARG CARGO_BUILD_JOBS=
WORKDIR /src
# The context is an allowlist (.dockerignore): sources, Cargo files and the
# corpus only, never deploy/.env, TLS keys, databases or local worktrees.
COPY . .
# Portable build: drop the host-CPU tuning so the binary runs on any CPU of the
# target architecture (the `.cargo/config.toml` files pin target-cpu=native).
# The registry and target dir are BuildKit caches, so a source edit recompiles
# only the workspace crates.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    rm -f .cargo/config.toml alphageometry-rs/.cargo/config.toml \
 && GEOSOLVER_REVISION="$GEOSOLVER_REVISION" cargo build --release --locked -p ag-studio ${CARGO_BUILD_JOBS:+-j $CARGO_BUILD_JOBS} \
 && install -m 0755 target/release/agstudio /agstudio

# ---- claude CLI (for the `ai` target only) ----
FROM ${NODE_IMAGE} AS claude
ARG CLAUDE_CODE_VERSION=latest
RUN npm install -g --omit=dev "@anthropic-ai/claude-code@${CLAUDE_CODE_VERSION}" \
 && npm cache clean --force \
 && readlink -f /usr/local/bin/claude > /claude-target

# ---- shared runtime base ----
FROM ${DEBIAN_IMAGE} AS base
ARG GEOSOLVER_REVISION=unknown
LABEL org.opencontainers.image.title="GeoSolver" \
      org.opencontainers.image.description="Olympiad geometry prover: web app, MCP server and CLI" \
      org.opencontainers.image.source="https://github.com/VladCelTare20/geosolver" \
      org.opencontainers.image.licenses="Apache-2.0" \
      org.opencontainers.image.version="0.1.0" \
      org.opencontainers.image.revision="${GEOSOLVER_REVISION}"
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --home /home/agstudio --shell /usr/sbin/nologin agstudio \
 && install -d -o agstudio -g agstudio -m 0700 /data
COPY --from=build /agstudio /usr/local/bin/agstudio
COPY LICENSE NOTICE THIRD_PARTY_LICENSES.md ag-studio/assets/fonts/FONT-LICENSES.txt /usr/share/doc/geosolver/
WORKDIR /home/agstudio
# Bind all interfaces *inside* the container; expose it via a published port or,
# preferably, a reverse proxy. A non-loopback bind REQUIRES AGSTUDIO_BASIC_AUTH
# (or AGSTUDIO_ALLOW_INSECURE=1) — the app refuses to start otherwise.
ENV AGSTUDIO_BIND=0.0.0.0:8787 \
    AGSTUDIO_DB=/data/agstudio.db
VOLUME ["/data"]
EXPOSE 8787
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8787/healthz || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/agstudio"]
CMD ["serve"]

# ---- with the claude CLI: docker build --target ai ----
FROM base AS ai
COPY --from=claude /usr/local/bin/node /usr/local/bin/node
COPY --from=claude /usr/local/lib/node_modules/@anthropic-ai /usr/local/lib/node_modules/@anthropic-ai
COPY --from=claude /claude-target /usr/local/share/claude-target
RUN ln -s "$(cat /usr/local/share/claude-target)" /usr/local/bin/claude \
 && claude --version
ENV DISABLE_AUTOUPDATER=1 \
    CLAUDE_CONFIG_DIR=/home/agstudio/.claude
USER agstudio

# ---- default: no AI ----
FROM base AS runtime
USER agstudio
