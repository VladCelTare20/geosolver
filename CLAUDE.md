# GeoSolver (formerly AlphaGeometry Studio)

Local product wrapping the `alphageometry-rs` Rust DDAR engine so a photographed
or described olympiad geometry problem becomes a numbered proof + figure (PDF/PNG),
via a web app, an MCP server (Claude Desktop/Code), or a CLI.

## Layout
- `alphageometry-rs/` — the DDAR engine (library `ddar` + `ddar` binary). Do not
  break its tests; it is verified against the original AlphaGeometry2 Python.
- `ag-studio/` — this product. One binary `agstudio` with subcommands
  `render` / `best` / `translate` / `serve` / `mcp`.
  - `src/engine.rs` — in-process solve wrapper (DDAR + aux search; routes
    absolute-length goals to the classical Euclidean prover). Returns proof + SVG.
  - `src/render.rs` — SVG→PNG (resvg) / SVG→PDF (svg2pdf); the combined
    figure+proof "report" page. Bundles Inter, STIX Two and DejaVu
    (`assets/fonts/`, licences in `FONT-LICENSES.txt`, served at `/licenses`).
  - `src/translate.rs` — photo/NL→`.geo` by shelling out to the local `claude`
    CLI (subscription auth, **no API key**). `prompts/geo_grammar.md` is the prompt.
  - `src/web.rs` + `assets/index.html` — axum server + single-page UI.
  - `src/mcp.rs` — stdio MCP server (newline-delimited JSON-RPC, hand-rolled).
- `corpus/` — AlphaGeometry problem sets (imo_ag_30, jgex_ag_231) + defs/rules.

## Commands
```sh
cargo build --release -p ag-studio          # the agstudio binary
RAYON_NUM_THREADS=6 cargo test --release --workspace -- --test-threads=6   # all tests (the gate)
./run.sh  (Linux/macOS)  |  ./run.ps1 (Windows)  # build + launch the web app (:8787)
agstudio render <program|file> [--pdf F --png F --svg F --theme light|dark]
agstudio translate "<text>" | --image P [--solve --pdf F]
agstudio serve [--port N]                     # web app
agstudio mcp                                  # MCP server (stdio)
agstudio backup <dest.db>                     # consistent copy of the server DB
```

## Conventions
- No Python in this repo (it was the reference impl; the Rust is verified against it).
- Rendering is pure Rust and headless (no browser). Exports default to a light,
  print-friendly page.
- Translation must use the Claude **subscription** (the `claude` CLI), never an
  `ANTHROPIC_API_KEY`.
- MCP stdout is the protocol channel — never print to stdout in `mcp` mode.
- **No coordinates** (owner's requirement, 2026-10-03): every proof is classical
  steps — DDAR deductions (angle/ratio/distance chasing), cited theorems, and
  trigonometry (law of sines/cosines, trig Ceva — explicitly allowed). No
  coordinate/algebraic-geometry provers (`algebra.rs` was deleted on purpose), no
  Wu/Groebner/complex/barycentric bashing, and a numerical check is never a proof
  (`status: holds-numerically`). Coordinates may only propose
  candidates, pick configuration branches and check non-degeneracy.
- Measure capability with the corpus benchmark (`ddar --corpus`, results in
  `~/Projects/geosolver-bench/`), never with `--bench` alone (26 pre-selected rows).
- Soundness is the product. Every change to goal compilation or a prover needs a
  regression test with a FALSE statement that must stay unproved.
- **0° and 180° are different** (owner's rule, 2026-10-16): displayed proofs never
  identify them. Proof steps use ordinary angles ∠ wherever a derivation exists;
  collinearity is a named theorem with its side condition (vertical angles,
  linear pair, angle construction), right angles add to 180°, and a ∠ sum is
  never "= 0°" unless the angle really is 0°. Directed angles ∡ (between lines,
  mod 180°) are the last resort and are always written as congruences
  `≡ … (mod 180°)`, never with `=`. Gates: `check.rs:chain_lints` L1/L2/L3/L5/L7 = 0
  (`tests/human_chain_rows.rs`, ag-studio `proof_audit`); docs/HUMAN_PROOFS.md §6.6.5.
- State is one SQLite file (`AGSTUDIO_DB`, WAL mode, schema version in
  `PRAGMA user_version`); deploy files must put it on persistent, writable
  storage, and backups go through `agstudio backup`, never a copy of the bare
  `.db`. A schema change bumps `db.rs:SCHEMA_VERSION` with a migration.
