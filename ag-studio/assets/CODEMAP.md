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
- `site.js:Viewer.highlight(points, facts)` — with facts (steps, GIVEN, PROVE) only the objects the predicate names light up (`factShape`): cyclic → its circle, simtri → six sides, perp/para/cong → two segments, eqangle → the arms and the arc at the vertex, coll → its line; a drawn line is lit when it contains one of those segments. Facts it cannot read (prose, formulas) fall back to points: a line with ≥ 2 of them, a circle with ≥ 3 (or its centre + 2), marks when all of theirs are.
- `app.js:solve` — a new solve clears the previous result before anything else, so an error can never sit next to an old verdict. "Shortest proof" shows the standard result first, then swaps in the best-mode proof only if it is strictly shorter.
- `app.js:deleteHistory` — deletion is deferred 6 s for Undo; pending deletes are flushed with `keepalive` on `pagehide`.
- Colour tokens were checked for WCAG AA (text ≥ 4.5:1, borders/graphics ≥ 3:1) in both themes.
- `site.js:modal` — full-screen figure and the history drawer: everything outside is `inert`, the element becomes `role=dialog aria-modal=true`, Tab wraps inside, and closing restores exactly what was made inert.
- `site.js:renderSteps` — roving tabindex on steps; a step's cites are tab stops only while that step is active; a `focusout` that leaves the list clears the highlight. Steps of kind `given` (restated hypotheses) are folded into one "Steps 1–8, 10 restate …" row with Show/Hide; they stay in the list hidden (`data-restated`), and a cite to one unfolds them. `subs` render as a bordered list under the step. The rule, `←` and cite chips are one inline flow; the arrow is glued to the first chip.
- `site.js:math` — prose tokens split at `·` and `/`, and `sin∠ABC` / `△ABC` italicise the point run, so trig steps use the same type as the facts.
- `site.js:placeLabels` — at a label scale above the server's (`k` > 1 in a small panel or the hero) labels of close points grow into each other; each keeps the server's spot unless that now hits a placed label (with a gap), a dot or a leader, else takes the least-colliding spot around its point. Leaders follow the new spot.
- `app.js:announceStatus` — `#announce-status` (role=status) carries secondary messages (shorter-proof search, history search/filter counts, "explanation ready") so they never cut the verdict announcement in `#announce`. `#ai-text` is not a live region. Both regions are cleared on a language switch.
- `app.js:refineShorter` — focus inside `#refine` goes to `#verdict-head` when the line is hidden; when a shorter proof is swapped in, `focusMark`/`restoreFocus` put focus back on the same control or step index.
- `app.js` editor — a right-edge fade (`.code.can-scroll-x`) shows while a line runs past the editor's edge.
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
- `landing.js` — stats and the showcase blurb come from `showcase.json` (`first_secs`, `budget_secs`); the step count excludes restated hypotheses; the folded preview ends after `FIRST_DEDUCTIONS` deductions. `showcase.json`'s figure predates server-side leaders; its crowded labels had `f-lead` lines added to the stored SVG.
- `site.js:placeLabels` — goal lines (`line.f-goal`, weight 12) and construction lines (weight 8) count as collisions that send a label away from the server's spot; a leader crossing a placed label and a spot nearer another dot than its own each cost 14. `f-ext` (dashed extensions to right-angle feet) are obstacles too.
- `app.css` figure focus — dimmed labels keep opacity 1 and switch to `--fig-lbl-dim` / `--fig-aux-dim` (≥ 5.8:1 on `--fig-bg` in both themes); lines and dots still fade.
- `app.css` header — `.seg`, `.icon-btn` and `.btn-sm` in `.site-header` share `--hdr-ctl` (36 px, 44 px on coarse pointers).
- `app.css` verdict actions — a wrapping flex row (buttons grow, never truncate); meta items may wrap. Toasts sit bottom-right from 1200 px so they never cover the verdict actions; `.toast :focus-visible` uses `--bg` for its ring.
- `app.js:sessionEnded` — any 401 (solve, history list/delete/open, export, shorter-proof search) refreshes `/api/status`, swaps the header to Sign in and puts the session-ended message in the rail.
- `app.js:refineShorter` — “none found” only for a 2xx answer that is not shorter; a 503/429 says the server was busy, other failures say the search could not run.
- `app.js:exportName` — downloads are named by title and verdict (`-proof`, `-counterexample`, `-numerical`, `-not-proved`).
- `app.js:histStatus` — `proved-drawn` (stored `as_drawn`) shows “Proved (as drawn)” under the Proved filter.
- `app.js:verdictModel` — a proof with no steps says it follows at once and offers no Copy proof.
- `site.js:fitLabels` — labels are at least 15 px on screen (`labelPx`); marks with `data-a` are scaled about their anchor by the label scale (1–2.2) so right angles and ticks stay visible in small panels.
- `site.js:Viewer` — while zoomed, a pan pad (four `.pan-btn` at the viewport edges, `fig.pan.*` labels) pans by 30 % of the view without dragging (WCAG 2.5.7); it is `hidden` at fit, and focus inside it moves to the viewport when it hides. The figure SVG is the viewport's direct `svg` child (`:scope > svg`), not the icons' SVGs.
- `app.css` layout — below 1024 px the app is one column (composer, status, figure, proof) and the figure panel takes the full width; `fitViewportToFigure` sizes it (≤ 60 % of the screen on tablets, 70 % on phones).
- `app.js:exportAs` — the menu click closes with focus back on Export; the button is `aria-disabled` (never `disabled`, which would drop focus) while an export runs. A 410 re-caches the stored history copy (`GET /api/history/{id}`) and retries, otherwise exports by `{input, title}`; a 401 toasts `export.auth`.
- `app.js:renderHistory(keepFocus)` — when the focused row disappears (stale 404, delete flush, reload) focus goes to the same row, the next one, the rail's Sign in link, or the search box; `sessionEnded` empties the rail synchronously and keeps focus there; a 401 on open focuses the error heading.
- `app.js:solve` — a solve that aborts a running shorter-proof search retries up to 4 × 350 ms on `busy_self`, since the server frees the aborted request's slot only once it notices the disconnect.
- `app.js:startTimer` — past its maximum the solving card reads “Finishing up · time limit N s” with a full bar instead of counting over the limit (queue wait and the worker's grace are not part of the deadline).
- `landing.js` — `#more` carries `aria-expanded`; expanding focuses the first newly shown step, collapsing keeps the button where it was on screen.
- `app.css` forced colors — `.editor:focus-within`, `.input:focus`, `.textarea:focus` get a `Highlight` outline (their normal ring is a box-shadow, which forced colors removes).
- `i18n.js` RO — hyphenated clitics (`fotografiaz-o`, `s-a`) carry a word joiner (U+2060) after the hyphen so lines never break there.
