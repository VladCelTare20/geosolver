# GeoSolver (formerly AlphaGeometry Studio)

Local product wrapping the `alphageometry-rs` Rust DDAR engine so a photographed
or described olympiad geometry problem becomes a numbered proof + figure (PDF/PNG),
via a web app, an MCP server (Claude Desktop/Code), or a CLI.

## Layout
- `alphageometry-rs/` — the DDAR engine (library `ddar` + `ddar` binary). Do not
  break its tests; it is verified against the original AlphaGeometry2 Python.
- `ag-studio/` — this product. One binary `agstudio` with subcommands
  `render` / `translate` / `serve` / `mcp`.
  - `src/engine.rs` — in-process solve wrapper (DDAR + aux search; routes
    absolute-length goals to the classical Euclidean prover). Returns proof + SVG.
  - `src/render.rs` — SVG→PNG (resvg) / SVG→PDF (svg2pdf); the combined
    figure+proof "report" page. Bundles `assets/fonts/DejaVu*` for glyph coverage.
  - `src/translate.rs` — photo/NL→`.geo` by shelling out to the local `claude`
    CLI (subscription auth, **no API key**). `prompts/geo_grammar.md` is the prompt.
  - `src/web.rs` + `assets/index.html` — axum server + single-page UI.
  - `src/mcp.rs` — stdio MCP server (newline-delimited JSON-RPC, hand-rolled).
- `corpus/` — AlphaGeometry problem sets (imo_ag_30, jgex_ag_231) + defs/rules.

## Commands
```sh
cargo build --release -p ag-studio          # the agstudio binary
cargo test                                   # engine + ag-studio tests
./run.ps1                                     # build + launch the web app (:8787)
agstudio render <program|file> [--pdf F --png F --svg F --theme light|dark]
agstudio translate "<text>" | --image P [--solve --pdf F]
agstudio serve [--port N]                     # web app
agstudio mcp                                  # MCP server (stdio)
```

## Conventions
- No Python in this repo (it was the reference impl; the Rust is verified against it).
- Rendering is pure Rust and headless (no browser). Exports default to a light,
  print-friendly page.
- Translation must use the Claude **subscription** (the `claude` CLI), never an
  `ANTHROPIC_API_KEY`.
- MCP stdout is the protocol channel — never print to stdout in `mcp` mode.
