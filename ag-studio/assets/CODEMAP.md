# ag-studio/assets — the web pages, compiled into the binary

Up: [../../CODEMAP.md](../../CODEMAP.md) · Server: [../src/](../src/CODEMAP.md)

## Files
- `index.html` + `app.js` — the solver app (`/app`, or `/` in guest mode): history rail, composer, verdict, statement, steps, figure.
- `landing.html` + `landing.js` — the signed-out home page; its showcase is `showcase.json`, a real `/api/solve` answer (IMO 2023 P2, shortest-proof mode).
- `auth.html` + `auth.js` — sign in / create account.
- `site.js` — shared by all pages: icons, theme menu, math typesetting (`GS.math`), typed facts (`GS.fact`), step list (`GS.renderSteps`), figure viewer (`GS.Viewer`).
- `i18n.js` — EN/RO catalogue and the `data-i18n*` markup engine; plurals via `tp(key, n)` with `.one/.few/.other`.
- `app.css` — the design system (tokens, components) and the app layout; landing/auth add small page-local `<style>` blocks.
- `fonts/` — Inter and STIX Two Text as woff2 subsets (Latin, Latin Ext-A incl. Romanian, Greek, math symbols), both SIL OFL 1.1 (see `fonts/LICENSES.md`); DejaVu TTFs are for resvg exports.

## Notes
- Every inline `<script>` is pinned by hash in the CSP (`security.rs:content_security_policy`, computed from these files at startup); page logic lives in the `.js` files, which `'self'` allows. Only the tiny theme/language no-flash snippet in each `<head>` is inline.
- `site.js:Viewer` — zoom changes the SVG `viewBox` (crisp at any scale); labels and dots are re-sized per zoom from `data-x/y/dx/dy` so they stay a constant screen size.
- `site.js:Viewer.highlight` — a line lights up when ≥ 2 of its points are in the step, a circle with ≥ 3 (or its centre + 2), marks when all of theirs are.
- `app.js:solve` — a new solve clears the previous result before anything else, so an error can never sit next to an old verdict. "Shortest proof" shows the standard result first, then swaps in the best-mode proof only if it is strictly shorter.
- `app.js:deleteHistory` — deletion is deferred 6 s for Undo; pending deletes are flushed with `keepalive` on `pagehide`.
- Colour tokens were checked for WCAG AA (text ≥ 4.5:1, borders/graphics ≥ 3:1) in both themes.
