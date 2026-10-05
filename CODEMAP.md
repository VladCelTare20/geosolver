# geosolver — olympiad geometry prover: DDAR engine + web app, MCP server and CLI

A photographed or described geometry problem becomes a `.geo` program (via the
local `claude` CLI), the Rust DDAR engine proves it or refutes it numerically,
and the proof plus figure come back as a page, PDF or PNG. Cargo workspace of
two crates; `flake.nix` packages both binaries (`agstudio`, `ddar`).

## Commands
- `cargo build --release -p ag-studio` — the `agstudio` binary
- `RAYON_NUM_THREADS=6 cargo test --release --workspace -- --test-threads=6` — all tests (~400; the thread caps keep it inside the shared box's budget)
- `cargo run --release -p alphageometry-rs --bin ddar -- --bench` — must stay 26/26
- `ddar --corpus corpus/imo_ag_30.txt --budget 120 --jobs 4 --threads 3 --out r.tsv` — honest solve rate on the AG1 corpora (DDAR + aux search, hard per-problem deadline); `--corpus-check` translates only. Keep jobs × threads ≤ 12 on the shared box. `AUX_VERBOSE=1` prints the aux pool and rollout progress; `AUX_LEGACY=1` runs the old aux search.
- `nix build .#geosolver`, `nix develop`

## Invariants
- **Never report a false statement as proved.** Goals DDAR cannot express are a
  `CompileError::MetricGoal`, never a placeholder; any NaN/refutation is
  `proved: false`.
- **Strictly Euclidean.** `proved` means a theorem-citing proof (DDAR, or
  `metric::solve` → `Ok`). A goal that only holds in sampled figures is
  `MetricError::NoProof` / `status: "holds-numerically"`, `proved: false`. Each prover change needs
  a FALSE-statement regression test (`alphageometry-rs/tests/soundness.rs`).
- Every solve reachable from outside has a wall-clock deadline (`solve_within`), and the web/MCP servers run it in a killable worker process (`worker::run`) that is SIGKILLed at deadline + grace, on client disconnect and on MCP cancel, and exits by itself within 100 ms if its server dies; the CLI's `--timeout`/`--budget` is enforced by a watchdog. No solve runs in-process in a server.
- **One search everywhere.** The product (`agstudio render`/`best`, `/api/solve`, MCP) and the corpus benchmark run the same aux pipeline, `aux_search::solve_max_until` (DDAR, full depth-1 sweep over the coincidence-ranked pool, randomised rollouts until the deadline); a capability measured with `ddar --corpus` is what the app delivers.
- Panic output is muted only through `ddar::quiet_panic`; never swap the hook.
- In `mcp` mode stdout is the protocol channel.
- State is one SQLite file (`AGSTUDIO_DB`); deploy files put it on a volume.

## Subfolders
- [alphageometry-rs/src/](alphageometry-rs/src/CODEMAP.md) — the engine.
- [ag-studio/src/](ag-studio/src/CODEMAP.md) — the product binary.
- [ag-studio/assets/](ag-studio/assets/CODEMAP.md) — the web pages (app, landing, sign-in), design system, fonts.
- [ag-studio/tests/](ag-studio/tests/CODEMAP.md) — binary integration tests, iPhone CSS invariants, the WebKit iPhone check.
- `deploy/` — Docker, nginx, systemd; `DEPLOY.md` mirrors the env table in `ag-studio/src/security.rs`.
- `corpus/` — AlphaGeometry problem sets (data, not code).
- `docs/HUMAN_PROOFS.md` — design of the deterministic human proof writer; engine side in `alphageometry-rs/src/human/` (status and data-model changes at the end of the spec), wired into ag-studio (`engine.rs`, `present.rs:human_view`, Proof tab, MCP, reports); goldens and the measurement prototype patch in `docs/human-proofs/`.
