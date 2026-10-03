# alphageometry-rs/src — the DDAR engine (Rust port of AlphaGeometry2's symbolic core)

Up: [../../CODEMAP.md](../../CODEMAP.md)

## Files
- `geo.rs` — the `.geo` language: parse, expand constructions, sample a figure, compile to predicates. Point cap counts expanded points.
- `predicate.rs` — the low-level AlphaGeometry predicate language and parser.
- `engine.rs` — DDAR deductive closure; goals include `acompute` (angle) and `rcompute` (ratio fixed to a constant).
- `elim_core.rs`, `elimination.rs`, `lincomb.rs`, `rational.rs` — exact Gaussian elimination over rationals (angles, ratios, distances, squared lengths).
- `fingerprint.rs` — linear fingerprints of `LinComb`s mod 2^61−1; can only rule an equality out.
- `aux_search.rs` — LM-free auxiliary-point search over DDAR: the classical construction library, `WarmBase`, and the entry points (`solve_max_until` → `aux_rollout::search`).
- `aux_virtual.rs` — new candidate kinds: virtual lines (parallel / perpendicular / tangent / isogonal through a point) met with figure lines and circles, intersections with circles the closure proved, harmonic conjugates.
- `aux_score.rs` — coincidence score: extra figure lines, circles and equal-distance classes through a candidate beyond its defining loci. Ranking only.
- `aux_rollout.rs` — scored pool, depth-1 sweep, randomised multi-point rollouts, minimisation.
- `metric.rs` — metric goals: `solve` is `Ok` only for a Euclidean proof; otherwise `MetricError::{Refuted, NoProof{evidence}, Failed}`. `check_numerically` is evidence, never a proof.
- `synthetic.rs`, `ratio.rs` — classical theorem-citing proofs for length / ratio goals.
- `certify.rs` — `Certifier`: derives a proposed fact with the DDAR closure and returns the numbered derivation.
- `proof.rs` — provenance and readable proof text.
- `numerics.rs` — f64 geometry oracle.
- `svg.rs` — figure rendering; all text is XML-escaped here.
- `quiet_panic.rs` — one process-wide hook, thread-local mute.
- `runner.rs` — helpers shared by `bin/ddar.rs` and tests.
- `corpus.rs` — reader for the original AlphaGeometry corpus language (`corpus/*.txt`, constructions from `corpus/defs.txt`) → `Problem`.
- `bench.rs` — solve-rate benchmark: `solve_one` (DDAR, then aux search, under a deadline) and `run_corpus` (one child process per problem, killed at budget + grace; `RunConfig.child_flag` picks the child mode).
- `fuzz.rs` — soundness fuzzer: genuinely false variants of corpus problems, each run through DDAR + aux search in its own process; any proof is a soundness bug (exit 2). Run under the bench lock: `flock ~/Projects/geosolver-bench/.bench.lock ddar --fuzz-false corpus/jgex_ag_231.txt --per 8 --budget 10 --jobs 6 --threads 2 --out f.tsv` (cases in `f.tsv.cases.txt`, proofs of any proved case in `f.tsv.proofs/`).

## Notes
- `geo.rs:compile` — rejects metric goals with `CompileError::MetricGoal`; before this, they compiled to `cong a b a b` and were "proved" trivially.
- Expression parsers cap exponents at 32 and binary operators at 256 per relation (stack overflow and blowup guards).
- `corpus.rs` — premises are exactly the `defs.txt` predicates of each construction; numerics follow its sketch line. Every premise is checked numerically on the sample (a mis-ported sketch is rejected, never fed to DDAR); the figure is resampled until the goal holds, like AG1's `build_problem`. Argument lists missing the new points get them prepended (AG1 `add_clause`). Shapes built from nothing are mirrored with p=1/2 (AG1 `random_rfss`) — the `contri` square/15° problem needs the other orientation. `s_angle a b x y` lowers to `aconst b x b a y`. `simtri`/`contri`/`midp` goals become conjunctions of directed-angle/`cong`/`coll` facts (sufficient for the goal).
- `bench.rs:solve_one` — a result is `proved` only with an engine proof for every goal conjunct; a numerically false goal that DDAR proves is reported `UNSOUND` (the CLI exits 2).
- `aux_search.rs:hypothesis_circle_members` — antipode-on-circle and pole-of-chord candidates may only use points on the circle *by hypothesis cong chains*. Numeric membership let IMO 2008 P6 assert its own goal (`tests/soundness.rs: aux_search_does_not_assume_numeric_circle_membership`).
- `aux_search.rs:solve_max_until` — runs `aux_rollout::search` (`AUX_LEGACY=1` restores the old full depth-1 sweep + rank-first deepening). `solve_max`/`solve_with_aux` (ag-studio, tests) still use the old search.
- `aux_rollout.rs:search` — pool = classical library + new kinds with ≥1 extra incidence, ranked by `2·inc + heuristic`. Exhaustive depth-1 sweep, then rounds: 4·threads first points drawn by weight `exp(0.9·inc + 0.25·heur)`, each extended 48 times by 1–3 more points (from the pool, or from the lazily built child pool of one of the top `PARENTS` = 512 points already drawn: its top `CHILD_CAP` = 192 candidates built on it — the caps bound memory, unbounded child pools reached 1.8 GB on `morley`). Each first point is closed once (`WarmBase::extend`) and its continuations checked warm. Seeds come from a hash of the problem + round + index and `find_map_first` returns the lowest index, so the found set does not depend on thread timing (only on how many rounds fit before the deadline). Tried sets are deduplicated; a found set is minimised by dropping points (and their children) while the goal still follows.
- `aux_rollout.rs` — points with (numerically) equal coordinates are never combined in one rollout: DDAR keeps no pair variables for numerically identical points, so two definitions of one point could be identified without a proof.
- `aux_rollout.rs:build_pool` — child pools are built with `parallel = false`: they are initialised inside rayon tasks through `OnceLock`, and a nested parallel iterator there could steal a task that waits on the same lock.
- `aux_virtual.rs` — every candidate asserts exactly two loci (a line or circle each); intersections that are near-tangent or near-parallel are skipped, so no relation among existing points is implied. `tests/soundness.rs: new_aux_kinds_do_not_assume_numeric_coincidences` audits each kind next to a free point that only numerically satisfies the goal.
- `aux_search.rs:circle3`, `flat` — no circle through, or triangle centre of, a numerically flat triple (a triple collinear because the goal says so gave a giant "circumcircle" and asserted `cyclic` on collinear points); candidates farther than 100× the figure span are dropped.
- `aux_search.rs:Construction::tpl` — `desc` with point ids as tokens (`tok`), re-rendered after rollout points are renumbered and renamed (`aux_rollout.rs:finish`).
- `engine.rs:deduction_closure_until` — gives up between passes past a deadline (returns `false`, so a check reads "not proved"); the rollout and sweep checks use it, the old search and the base closure do not, hence still the process kill in `bench.rs`.
- `synthetic.rs`, `ratio.rs` — coordinates only propose candidates and read configuration (order, orientation, inside/outside); every cited fact is symbolic or DDAR-certified (`certify.rs`): generic similar triangles (`ratio.rs:certified_products`), the intercept parallel, the bisector, Stewart's ratio (`synthetic.rs:derived_ratio`). Circles come from symbolic equal-radius classes (`synthetic.rs:equal_radius_circles`).
- `synthetic.rs:prove_euclidean` — before the theorem rules, DDAR-certified right angles, midpoints and (aux pass) equal lengths are added; the aux pass also introduces intersections of goal segments and the centres of quadrilaterals on the goal points. All certified steps share one printed derivation (`render`).
- `ratio.rs:prove_ratio` — the product engine runs on a fresh figure, so the monomial engine's perpendicular feet never leak into its derivations. Its candidates (similar triangles, collinear splits, the bisector theorem) wait in `pending` until certified; a lone named-theorem citation is rejected as circular.
- `engine.rs:search_similitude` — the image-vs-antihomologous pairing is read off the figure only when line ZP meets circle 1 in two numerically distinct points; at a tangency both pairings coincide, so the choice would be a coincidence, not a configuration (found by the fuzzer, four false statements: `tests/soundness.rs: similitude_pairing_is_not_read_off_a_tangent_coincidence`, `..._tangent_line`).
- `engine.rs:search_radical_axis` — the inside/outside branch is read from chord-end order, not from the conclusion.
- `engine.rs:PairIds` — per-pass keys for the O(n³) searches (similar, concyclic, bisector): a triangle's angle/ratio key is a fingerprint difference of two cached pair values, O(1). A fingerprint match is acted on only after an exact comparison of interned normal forms (`ang`/`rat`, memoised in the thread-local stamped `TripleMemo`); a coefficient with no residue switches the pass to exact ids (`fp_ok`). Shared across the three searches until the next `update_cache`.
- `engine.rs` speed work keeps the force order: bucket maps are only probed, never iterated; concyclic's group map is keyed by `&LinComb`, which hashes like the old `Angle` key. wf/speed kept proofs byte-identical to a764596; the engine/ rules (wf/rules) change proofs on purpose.
- `engine.rs:search_similar` — degenerate triangles are skipped by `force_collinear`'s tolerance (`numerically_flat`) as well as `orientation`: a long base with sub-ATOM height would otherwise force a bogus ratio.
- `engine.rs:row_already_concyclic`, `line_holds` — skip forces that `force_concyclic`/`force_collinear` would exit from as no-ops. They rely on `triple_to_circle`/`pair_line` mapping every numerically distinct triple/pair of active points to the one object holding it; `line_holds` also repeats `force_collinear`'s numeric check so a would-be panic still happens.
- `engine.rs:force_*_with` — premises are computed lazily, after the no-op exit; nothing mutates in between, so the cited facts are unchanged.
- `aux_search.rs:try_solve` — at depth 1 the `WarmBase` closure also answers "does the base already prove the goal" (`proves_goal`), so a branch costs one cold closure, not two.
- `synthetic.rs` rectangle detection needs `AD ∥ BC` or a third right angle; `AB ∥ DC` is implied by two right angles and proves nothing.

- `fuzz.rs:generate` — four case kinds. `goal`: mutated goal (swap/transpose points, re-segment, other predicate over the same points) kept only if false on every one of `--samples` figures where the original goal holds. `hyp-generic`: one construction dropped (or replaced by `free`/`triangle`/`quadrangle`) and the goal false on every figure of the weaker problem. `hyp-special`: that same weaker problem pinned to the ORIGINAL figure, where the dropped fact and the goal still hold numerically — catches a rule that reads an unstated fact off the coordinates. `degenerate`: original problem with a point pinned onto an earlier one or a collinear base triangle; unsound only if a conjunct false on that figure is proved.
- `fuzz.rs` — every case is corpus text with pinned coordinates (`corpus.rs:render_problem`, shortest round-trip floats), so the child reproduces the parent's figure exactly. Generation is deterministic per (`--seed`, problem name).
- `fuzz.rs:generate_geo` — `--fuzz-false <dir>`: metric goals of every `.geo` program under it (goal: swap/transpose points, change a constant, scale a side; hypothesis: drop an `assume` or a `point:` constraint, or replace a definition by `free`), kept only if clearly false (rel. error > 1e-4) on every sampled instance — instance 0 is the provers' own figure. Text is `geo: <stmts; ...> ? <goal>`. `solve_geo_case` calls `synthetic::prove_euclidean` and `ratio::prove_ratio` directly, bypassing `metric::solve`'s figure backstop, so a wrong derivation is seen even though the product would reject it.
- `fuzz.rs:verdict` — a non-degenerate case is false as a theorem, so any proof is `UNSOUND`, even when the goal holds on the pinned figure (`hyp-special`).

## Subfolders
- [engine/](engine/CODEMAP.md) — classical closure rules: squared lengths, Menelaus/Ceva, bisector concurrency.
- `bin/` — `ddar.rs`, the engine CLI (`--help` lists modes).
