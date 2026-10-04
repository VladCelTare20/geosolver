# ag-studio/assets — the web pages, compiled into the binary

Up: [../../CODEMAP.md](../../CODEMAP.md) · Server: [../src/](../src/CODEMAP.md)

## Files
- `index.html` + `app.js` — the solver app (`/app`, or `/` in guest mode): history rail, composer, verdict, statement, steps, figure.
- `landing.html` + `landing.js` — the signed-out home page; its showcase is `showcase.json`, a real `/api/solve` answer (IMO 2023 P2, shortest-proof mode).
- `auth.html` + `auth.js` — sign in / create account.
- `site.js` — shared by all pages: icons, theme menu, math typesetting (`GS.math`), typed facts (`GS.fact`), step list (`GS.renderSteps`), figure viewer (`GS.Viewer`).
- `i18n.js` — EN/RO catalogue and the `data-i18n*` markup engine; plurals via `tp(key, n)` with `.one/.few/.other`.
- `app.css` — the design system (tokens, components) and the app layout; landing/auth add small page-local `<style>` blocks.
- `fonts/` — Inter and STIX Two Text as woff2 subsets (Latin, Latin Ext-A incl. Romanian, Greek, math symbols), both SIL OFL 1.1 (see `fonts/LICENSES.md`). The `.ttf` files are renamed static instances ("GeoSolver Sans", "GeoSolver Math", with math symbols added from DejaVu) used only by the server's PDF/PNG reports; DejaVu TTFs are the last fallback.

## Notes
- Every inline `<script>` is pinned by hash in the CSP (`security.rs:content_security_policy`, computed from these files at startup); page logic lives in the `.js` files, which `'self'` allows. Only the tiny theme/language no-flash snippet in each `<head>` is inline.
- `site.js:Viewer` — zoom changes the SVG `viewBox` (crisp at any scale); labels and dots are re-sized per zoom from `data-x/y/dx/dy` so they stay a constant screen size.
- `site.js:Viewer.highlight` — a line lights up when ≥ 2 of its points are in the step, a circle with ≥ 3 (or its centre + 2), marks when all of theirs are.
- `app.js:solve` — a new solve clears the previous result before anything else, so an error can never sit next to an old verdict. "Shortest proof" shows the standard result first, then swaps in the best-mode proof only if it is strictly shorter.
- `app.js:deleteHistory` — deletion is deferred 6 s for Undo; pending deletes are flushed with `keepalive` on `pagehide`.
- Colour tokens were checked for WCAG AA (text ≥ 4.5:1, borders/graphics ≥ 3:1) in both themes.
- `site.js:modal` — full-screen figure and the history drawer: everything outside is `inert`, the element becomes `role=dialog aria-modal=true`, Tab wraps inside, and closing restores exactly what was made inert.
- `site.js:renderSteps` — roving tabindex on steps; a step's cites are tab stops only while that step is active; a `focusout` that leaves the list clears the highlight.
- `site.js:fitLabels` — the constant-screen-size label logic, shared by the Viewer and the landing hero; `Viewer.reset` pads the view box when labels are drawn larger than the figure was framed for (high zoom, small panels).
- `site.js:toast` — the timer pauses while the toast is hovered or focused; no `role=status` (the region is the live region).
- `app.js:EXAMPLES` — each example has a comment per language; the editor is re-seeded on a language switch only while it still holds an untouched example.
- `app.js:showError` — errors keep their i18n keys so a language switch repaints them; a compile error is re-requested in the new language.
- `app.js:deleteHistory` — focus moves to the next row (or the search box); Ctrl/⌘+Z undoes within the 6 s window.
- `app.js:fitViewportToFigure` — on phones the figure panel's height follows the figure's aspect ratio (clamped 0.6–1.25, ≤ 70 % of the screen).
- `app.js:validSolution` — a 2xx answer without an `input` and a known `status` is an "unreadable answer" error; no verdict is ever inferred from a malformed body.
- `app.js` Escape — menus, the drawer and full screen `preventDefault` the Escape they consume; the global handler cancels a solve only for an unconsumed Escape with no layer open.
- `app.js:placeMenu` — menus are laid out off-screen to the left first, then placed inside the viewport (8 px margin); laying them out at an overflowing position first widened the phone layout viewport.
- `app.js:openHistory` — the previous result is cleared before the fetch and the editor changes only on success; a 404 removes the row.
- `app.js` undo — Clear and history deletes set `S.lastUndo`; Ctrl/⌘+Z works from the field focus was moved to while it is still untouched.
- `site.js:fact` — relation symbols get screen-reader words (`sr.*` keys); `factText` (copy) keeps the symbols.
- `site.js:Viewer.stepPoint` — P / Shift+P in the focused figure walk the points in name order, highlight the steps using each and announce them.
- `site.js:toggleFull` — the figure panel gets `has-full` (z-index above the sticky header) while full screen.
- `auth.js` — server errors carry a `code`; field codes (`user_taken`, `username_rule`, `pw_len`) mark the field instead of a banner and are repainted on a language switch. Field hints are not live regions.
- `landing.js` — stats and the showcase blurb come from `showcase.json` (`first_secs`, `budget_secs`); the step count excludes restated hypotheses.
