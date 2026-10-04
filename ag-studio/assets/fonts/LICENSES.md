# Bundled fonts

| File | Font | Licence |
|---|---|---|
| `inter.woff2` | Inter 4.1 (variable, subset) — © The Inter Project Authors | SIL Open Font License 1.1 |
| `stix-two-text.woff2`, `stix-two-text-italic.woff2` | STIX Two Text 2.13 (variable, subset) — © The STIX Fonts Project Authors | SIL Open Font License 1.1 |
| `DejaVuSans.ttf`, `DejaVuSansMono.ttf` | DejaVu — © Bitstream, DejaVu changes public domain | Bitstream Vera / DejaVu licence |

The woff2 files are subsets (Latin, Latin Extended-A, Greek, punctuation,
arrows, mathematical operators, geometric shapes) made with `pyftsubset`.
The OFL text: https://openfontlicense.org

## Export fonts (PDF/PNG reports)

`inter-regular.ttf`, `inter-semibold.ttf` (family "GeoSolver Sans") and
`stix-two-text-{regular,semibold,italic}.ttf` (family "GeoSolver Math") are
static instances of the woff2 files above made with fontTools
(`varLib.instancer`), with the math symbols they lack (∠ ⊥ △ ∼ ∎ ∥ ≅ ⇒ ⇔ ↔ ←
≠ √) copied in from DejaVu Sans. They are renamed because they are modified
versions (OFL §3, Bitstream Vera terms) and are used only by the server-side
renderer.
