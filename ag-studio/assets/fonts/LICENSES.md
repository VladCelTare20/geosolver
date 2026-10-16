# Bundled fonts

| File | Font | Licence |
|---|---|---|
| `inter.woff2` | Inter 4.1 (variable, subset) — © The Inter Project Authors | SIL Open Font License 1.1 |
| `stix-two-text.woff2`, `stix-two-text-italic.woff2` | STIX Two Text 2.13 (variable, subset) — © The STIX Fonts Project Authors | SIL Open Font License 1.1 |
| `DejaVuSans.ttf`, `DejaVuSansMono.ttf` | DejaVu — © Bitstream, DejaVu changes public domain | Bitstream Vera / DejaVu licence |

The woff2 files are subsets (Latin, Latin Extended-A, Greek, punctuation,
arrows, mathematical operators, geometric shapes) made with `pyftsubset`.
The licence texts (OFL 1.1 for Inter and STIX, Bitstream Vera and Arev for
DejaVu) are in `FONT-LICENSES.txt`, built into the binary and served at
`/licenses`.

## Export fonts (PDF/PNG reports)

`inter-regular.ttf`, `inter-semibold.ttf` (family "GeoSolver Sans") and
`stix-two-text-{regular,semibold,italic}.ttf` (family "GeoSolver Math") are
static instances of the woff2 files above made with fontTools
(`varLib.instancer`), with the math symbols they lack (∠ ∡ ⊥ △ ∼ ∎ ∥ ≅ ∩ ⇒ ⇔
↔ ← ≠ √) copied in from DejaVu Sans (∡ and ∩, for the human-style proofs, scaled
like ∠ with fontTools pens). They are renamed because they are modified
versions (OFL §3, Bitstream Vera terms) and are used only by the server-side
renderer.

So that the reports print exactly the characters the web app shows, with no
look-alike substitution, both families also carry ∢ (U+2222, the Romanian
undirected-angle sign), ≡ (U+2261, the Romanian congruence sign) and ⟂
(U+27C2), and "GeoSolver Sans" carries ∡ and ∩ too. All of them come from
DejaVu Sans (Bitstream Vera / DejaVu licence; DejaVu has no U+27C2, so ⟂ is
its ⊥ outline encoded at U+27C2 as its own glyph, so PDF text extraction
returns ⟂). They were drawn with fontTools pens (`DecomposingRecordingPen` →
`TransformPen` → `RoundingPen` → `TTGlyphPen`), scaled by the unitsPerEm ratio
(1 for Inter, 1000/2048 for STIX Two), exactly as ∠ was, with DejaVu's advance
widths and side bearings scaled the same way. ∡ and ∩ in "GeoSolver Math" were
redrawn the same way at the same time; the earlier copies had their outlines
shifted 85 units left of ∠'s. ∥ (U+2225) was already in all five fonts.
Family and style names are unchanged.
