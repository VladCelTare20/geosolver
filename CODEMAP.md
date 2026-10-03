# geosolver — olympiad geometry prover: DDAR engine + web app, MCP server and CLI

A photographed or described geometry problem becomes a `.geo` program (via the
local `claude` CLI), the Rust DDAR engine proves it or refutes it numerically,
and the proof plus figure come back as a page, PDF or PNG. Cargo workspace of
two crates; `flake.nix` packages both binaries (`agstudio`, `ddar`).

## Commands
- `cargo build --release -p ag-studio` — the `agstudio` binary
- `cargo test --release --workspace` — all tests (~260)
- `cargo run --release -p alphageometry-rs --bin ddar -- --bench` — must stay 26/26
- `ddar --corpus corpus/imo_ag_30.txt --budget 120 --jobs 4 --threads 3 --out r.tsv` — honest solve rate on the AG1 corpora (DDAR + aux search, hard per-problem deadline); `--corpus-check` translates only. Keep jobs × threads ≤ 12 on the shared box.
- `nix build .#geosolver`, `nix develop`

## Invariants
- **Never report a false statement as proved.** Goals DDAR cannot express are a
  `CompileError::MetricGoal`, never a placeholder; any NaN/refutation is
  `proved: false`.
- **Strictly Euclidean.** `proved` means a theorem-citing proof (DDAR, or
  `metric::solve` → `Ok`). A goal that only holds in sampled figures is
  `MetricError::NoProof` / `status: "holds-numerically"`, `proved: false`. Each prover change needs
  a FALSE-statement regression test (`alphageometry-rs/tests/soundness.rs`).
- Every solve reachable from outside has a wall-clock deadline (`solve_within`).
- Panic output is muted only through `ddar::quiet_panic`; never swap the hook.
- In `mcp` mode stdout is the protocol channel.
- State is one SQLite file (`AGSTUDIO_DB`); deploy files put it on a volume.

## Subfolders
- [alphageometry-rs/src/](alphageometry-rs/src/CODEMAP.md) — the engine.
- [ag-studio/src/](ag-studio/src/CODEMAP.md) — the product binary.
- [ag-studio/assets/](ag-studio/assets/CODEMAP.md) — the web pages (app, landing, sign-in), design system, fonts.
- `deploy/` — Docker, nginx, systemd; `DEPLOY.md` mirrors the env table in `ag-studio/src/security.rs`.
- `corpus/` — AlphaGeometry problem sets (data, not code).
