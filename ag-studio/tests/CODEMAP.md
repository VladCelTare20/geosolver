# ag-studio/tests — integration tests of the shipped binary and assets

Up: [../../CODEMAP.md](../../CODEMAP.md)

## Files
- `worker_binary.rs` — the real `agstudio` binary: the `__solve-worker` subcommand and a `serve` whose solves go through it.
- `mobile_css.rs` — iPhone invariants read from `assets/` source: hover rules behind `@media (hover: hover)`, 16 px touch fields, safe-area gutters, `--vh-dyn`, 44 px touch targets, iOS field attributes. Part of `cargo test`.
- `webkit/iphone-layout.mjs` — the same behaviour in a browser: Playwright WebKit with the iPhone SE / 15 / 15 Pro Max presets, portrait and landscape, against a running server. Not run by `cargo test`.

## Notes
- `webkit/iphone-layout.mjs` — run against a guest-mode server: `PLAYWRIGHT_MODULE=<playwright>/index.mjs PLAYWRIGHT_BROWSERS_PATH=<browsers> BASE=http://127.0.0.1:<port> BASIC_PASSWORD=<pw> node ag-studio/tests/webkit/iphone-layout.mjs` (`SHOTS=<dir>` saves screenshots). It checks the page never scrolls sideways (also with Syntax open at 320 px), the editor is 16 px, nothing sits under simulated 59 px landscape notch strips (it overrides `--sai-l/r`), a step tap shows the figure peek, and the double-tap rules. Emulation has no real safe areas, keyboard or focus zoom; those need a device.
