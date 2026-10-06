# alphageometry-rs/src/human — the deterministic human-style proof writer (docs/HUMAN_PROOFS.md)

Up: [../CODEMAP.md](../CODEMAP.md)

Presentation only: reads an `EngineTrace` (engine/export.rs) and the goal's fact closure, never changes what is proved. Entry: `mod.rs:write` (or `for_problem`), `render_en`, `view::engine_json`, `verify`, `repair`.

## Files
- `mod.rs` — pipeline `Ctx → Writer::first_pass → select → recertify_with_claims → select → Presenter::run → drop_self_reasons → setup_lines → check::check → fill_metrics`; `Opts{deadline (default 2 s), strict}`; HumanCost (`fill_metrics`); `named_circles` and the setup lines (notation, circles, aux, helpers).
- `trace.rs` — `EngineTrace`: facts, the four tables' rows per fact, figure values, pair-variable maps, primes, sine variables (canonicalised by `(v,{p,q},shift)` key).
- `cert.rs` — exact span machinery: `Basis` (fully reduced, tracks combinations), `certify_greedy` (cost-level insertion, then support minimisation).
- `ctx.rs` — fact classes (Hyp, HypReg, TheoremReg, MergeReg, Silent, SilentHyp, Derived, Outside), quotient by hypothesis lines (`Quot`), line/circle objects, `fact_sources` for silent facts.
- `atoms.rs` — admissible human atoms (Inscribed, Thales, TangentChord, PerpBisector, Parallel, Radii, Isosceles, CentralAngle, PowerOfPoint, Midline, Orthocentre) + engine rows (Glue/Line/Engine), each with cost and availability horizon.
- `classify.rs` — fact kinds, statements, obligations (`Obl` = targets with alternative forms + required objects); `centre_obligations`, `central_obligations`, `sq_stmt`, `formula_stmt` (short trig statements), `dehelper`.
- `claims.rs` — `Writer` (certificates, claim selection, `cover`, `goal_obls`) and `Presenter` (blocks, chains, lemma splits, pooled sentences, trig computations).
- `chain.rs` — shortest single-angle chain search (Dijkstra, budget 20 000), A* ordering of a certificate over composite nodes (`order_items_mode`), `decomps`/`decomps_upto`, `display_node`, ratio splits.
- `expr.rs` — `Expr` evaluation to table rows; undirected interior angles read from the figure.
- `check.rs` — the independent checker (I1 statements, I2 links, I3 templates, I5 order) and the repair loop.
- `aux.rs` — wording for aux constructions: `parse` maps a construction to `AuxWording{key, args}` (keys match ag-studio i18n: `aux.midpoint`, `aux.foot`, …), `en` renders it as "Let X be …".
- `model.rs` — the serialisable data model (§7.2 + the changes listed in docs/HUMAN_PROOFS.md "Data model changes").
- `text.rs` — EN rendering (CLI and tests only; ag-studio renders from keys); thread-local notation and circle names (`with_notation`).
- `view.rs` — JSON for ag-studio: statements as `{kind,args,points}`, reasons with closure step numbers.

## Notes
- `claims.rs:Writer::certify` — obligations offer every algebraic form of a concyclic/collinear/similar fact; any one in the admissible span suffices. Concyclicity and collinearity are not linear consequences of their premises (only one inscribed form is), so a single fixed form would fail.
- `claims.rs:Writer::cover` — a merged circle/line (≥5 / ≥4 points) is re-proved as a growing chain of 4-/3-point subsets each in the admissible span. `goal_obls` adds the goal fact's own obligations (label `merged`) when the goal is a subset of a merged fact: the last chain states the fact, then "Hence <goal>".
- `classify.rs:centre_obligations` — a concyclic fact may be re-proved as equal distances from a figure point (IMO 2008 P1's six-point circle).
- `classify.rs:sq_stmt` — a theorem with one squared-length row is stated as that equation (`Expr::Sq`); with none it stays `Stmt::Formula` and is never a claim.
- `claims.rs:select` — claim cap `MAX_CLAIMS` = 7 is strict: the extra claims with the fewest users, then the lowest cost, become steps.
- `check.rs:check` — a failing block becomes Raw (its engine steps); claims it demoted are re-cited as `Reason::Engine` and the rest renumbered (`uncite`). A failing conclusion makes the whole proof unavailable: never wrong, at worst absent. `strict` panics instead (tests).
- `check.rs:sentence_ok` — a chain's `then` for a ≥4-point circle or ≥3-point line is accepted when the chain gives any one subset form; the block statement itself is proved separately from engine rows (`stmt_ok`).
- `mod.rs:write` — runs under `catch_unwind` + `quiet_panic`; any panic yields `available: false`. `HP_DEBUG=1` disables the catch and dumps nodes/violations to stderr; `HP_BFS=1` prints chain searches.
- `claims.rs:Presenter::chain_sentences` — stages, first that succeeds wins: trig lemmas (ratio goals with sine atoms) → BFS chain (≤ 8 links) → A* ordering of the certificate (whole, or split at non-integral λ) → lemma split (`lemma_plan`: connected subsets whose sum is a 2-angle or 2-ratio equality, each shown as its own chain, then the main chain cites them as `Reason::Lemma`) → undirected ("as drawn") ordering and halves lemma split → partial plans → pooled. A support with λ denominators other than 1 and 2 is first re-certified with those atoms banned (`Writer::certify_banned`).
- `claims.rs:Presenter::pooled_steps` — a pooled remainder citing more than `MAX_POOLED` = 5 reasons is cut into exact intermediate equalities (≤ 2 terms a side, sums of two named angles allowed; ratios as products with positive exponents); below 5 only clean single-angle intermediates are introduced. After the cut the remainder is tried as a chain again.
- `claims.rs:Presenter::radii_groups` — with `collapse`, hypothesis congruences and isosceles/radii atoms that share a centre become one multi-point atom `[O, P…]`. The rows are all pairs, so each combination term is remapped to its pair row with a sign; an atom whose row does not match ± that pair row is left unmerged.
- `claims.rs:Presenter::trig_lemmas` — for a length goal whose certificate uses sine atoms, find `XY = k·R·trig(v)` for both sides with one sine variable and show each as a `Computation`; identity rows (|sin x| = |sin x|) are dropped.
- `claims.rs:drop_self_reasons` — a fact reason never lists itself (or a hypothesis with the same statement) among its own reasons; `text.rs` also flattens restated reasons in `Because` sentences.
- `claims.rs:Presenter::run` — a claim whose only user restates it through a radii/isosceles atom is replaced by the user's statement (IMO 2023 P2: `XA = XP`, not `△OAX ≅ △OPX`), guarded by a proof check of the new statement.
- `mod.rs:named_circles` — circles with ≥ 2 references get a name (Ω for the main triangle's circumcircle, ω_k for centre O_k, then ω, γ, Γ, σ, τ, κ); a circle line is emitted after the aux point that completes its third defining point, listing only points already defined.
- Debug env vars (stderr only): `HP_TIME` (stage and per-node timings), `HP_POOL` (pooled fallbacks and `pooled_steps`), `HP_ITEMS`, `HP_LEMMA` (lemma candidates).
- `mod.rs:Opts::default` — 2 s deadline; past it the writer stops certifying, emits raw blocks and sets `metrics.timed_out`.
- Determinism: only `BTreeMap`/sorted iteration and integer costs; `tests/human_proofs.rs:output_is_deterministic` compares JSON across runs and threads.
- Tests: `tests/human_proofs.rs` (goldens in `tests/golden/human/*.en.txt`, `HP_BLESS=1` to accept; mutations; fallback; deadline; JGEX corpus with zero checker violations).
