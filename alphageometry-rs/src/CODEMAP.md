# alphageometry-rs/src — the DDAR engine (Rust port of AlphaGeometry2's symbolic core)

Up: [../../CODEMAP.md](../../CODEMAP.md)

## Files
- `geo.rs` — the `.geo` language: parse, expand constructions, sample a figure, compile to predicates. Point cap counts expanded points.
- `predicate.rs` — the low-level AlphaGeometry predicate language and parser.
- `engine.rs` — DDAR deductive closure.
- `elim_core.rs`, `elimination.rs`, `lincomb.rs`, `rational.rs` — exact Gaussian elimination over rationals (angles, ratios, distances).
- `aux_search.rs` — LM-free auxiliary-point search over DDAR.
- `metric.rs` — metric goals: `solve` is `Ok` only for a Euclidean proof; otherwise `MetricError::{Refuted, NoProof{evidence}, Failed}`. `check_numerically` is evidence, never a proof.
- `synthetic.rs`, `ratio.rs` — classical theorem-citing proofs for length / ratio goals.
- `ratio/trig.rs` — child module of `ratio.rs` (reaches `Figure`'s private items): cofactor substitution (T2), the centred-circle second-meet aux family (T5) and the trigonometric layer.
- `certify.rs` — `Certifier`: derives a proposed fact with the DDAR closure and returns the numbered derivation.
- `proof.rs` — provenance and readable proof text.
- `numerics.rs` — f64 geometry oracle.
- `svg.rs` — figure rendering; all text is XML-escaped here.
- `quiet_panic.rs` — one process-wide hook, thread-local mute.
- `runner.rs` — helpers shared by `bin/ddar.rs` and tests.
- `corpus.rs` — reader for the original AlphaGeometry corpus language (`corpus/*.txt`, constructions from `corpus/defs.txt`) → `Problem`.
- `bench.rs` — solve-rate benchmark: `solve_one` (DDAR, then aux search, under a deadline) and `run_corpus` (one child process per problem, killed at budget + grace).

## Notes
- `geo.rs:compile` — rejects metric goals with `CompileError::MetricGoal`; before this, they compiled to `cong a b a b` and were "proved" trivially.
- Expression parsers cap exponents at 32 and binary operators at 256 per relation (stack overflow and blowup guards).
- `corpus.rs` — premises are exactly the `defs.txt` predicates of each construction; numerics follow its sketch line. Every premise is checked numerically on the sample (a mis-ported sketch is rejected, never fed to DDAR); the figure is resampled until the goal holds, like AG1's `build_problem`. Argument lists missing the new points get them prepended (AG1 `add_clause`). Shapes built from nothing are mirrored with p=1/2 (AG1 `random_rfss`) — the `contri` square/15° problem needs the other orientation. `s_angle a b x y` lowers to `aconst b x b a y`. `simtri`/`contri`/`midp` goals become conjunctions of directed-angle/`cong`/`coll` facts (sufficient for the goal).
- `bench.rs:solve_one` — a result is `proved` only with an engine proof for every goal conjunct; a numerically false goal that DDAR proves is reported `UNSOUND` (the CLI exits 2).
- `aux_search.rs:hypothesis_circle_members` — antipode-on-circle and pole-of-chord candidates may only use points on the circle *by hypothesis cong chains*. Numeric membership let IMO 2008 P6 assert its own goal (`tests/soundness.rs: aux_search_does_not_assume_numeric_circle_membership`).
- `aux_search.rs:solve_max_until` — full depth-1 sweep, then deepening, with a cooperative deadline; a single DDAR closure is not interruptible, hence the process kill in `bench.rs`.
- `synthetic.rs`, `ratio.rs` — coordinates only propose candidates and read configuration (order, orientation, inside/outside); every cited fact is symbolic or DDAR-certified (`certify.rs`): generic similar triangles (`ratio.rs:certified_products`), the intercept parallel, the bisector, Stewart's ratio (`synthetic.rs:derived_ratio`). Circles come from symbolic equal-radius classes (`synthetic.rs:equal_radius_circles`).
- `synthetic.rs:prove_euclidean` — before the theorem rules, DDAR-certified right angles, midpoints and (aux pass) equal lengths are added; the aux pass also introduces intersections of goal segments and the centres of quadrilaterals on the goal points. All certified steps share one printed derivation (`render`).
- `ratio.rs:prove_ratio` — the product engine runs on a fresh figure, so the monomial engine's perpendicular feet never leak into its derivations. Its candidates (similar triangles, collinear splits, the bisector theorem) wait in `pending` until certified; a lone named-theorem citation is rejected as circular.
- `ratio.rs:prove_products` — any homogeneous degree. Degree 2 runs the original gathers first, unchanged; only when that fails (and only with `t2`, i.e. not inside the old `aux_search`) are the T2 cofactor rows `m − m[s1→s2]` added, each congruence a pending `cong` certified by DDAR. Golden proof text: `tests/metric_golden.rs`.
- `ratio/trig.rs:centred_second_meets` — appended after every older aux family (existing candidate order unchanged); facts `Coll(v,r,K)`, `Cong(o,a,o,K)`. Only circles with exactly three points not already in a `concyclic` set.
- `engine.rs:search_radical_axis` — the inside/outside branch is read from chord-end order, not from the conclusion.
- `synthetic.rs` rectangle detection needs `AD ∥ BC` or a third right angle; `AB ∥ DC` is implied by two right angles and proves nothing.

## Subfolders
- `bin/` — `ddar.rs`, the engine CLI (`--help` lists modes).
