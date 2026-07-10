# syntax=docker/dockerfile:1
# Portable multi-stage build of AlphaGeometry Studio for Linux (amd64/arm64).
# The web UI, fonts, and grammar are baked into the binary, so the runtime image
# needs no assets and no system fonts.

# ---- build ----
FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
# Portable build: drop the host-CPU tuning so the binary runs on any CPU of the
# target architecture (the `.cargo/config.toml` files pin target-cpu=native).
RUN rm -f .cargo/config.toml alphageometry-rs/.cargo/config.toml \
 && cargo build --release -p ag-studio
# (the release profile strips symbols, so no separate `strip` step is needed)

# ---- runtime ----
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --home /home/agstudio --shell /usr/sbin/nologin agstudio
COPY --from=build /src/target/release/agstudio /usr/local/bin/agstudio
USER agstudio
WORKDIR /home/agstudio
# Bind all interfaces *inside* the container; expose it via a published port or,
# preferably, a reverse proxy. A non-loopback bind REQUIRES AGSTUDIO_BASIC_AUTH
# (or AGSTUDIO_ALLOW_INSECURE=1) — the app refuses to start otherwise.
ENV AGSTUDIO_BIND=0.0.0.0:8787
EXPOSE 8787
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8787/healthz || exit 1
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/agstudio"]
CMD ["serve"]
