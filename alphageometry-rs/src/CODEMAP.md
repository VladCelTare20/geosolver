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
- `certify.rs` — `Certifier`: derives a proposed fact with the DDAR closure and returns the numbered derivation.
- `proof.rs` — provenance and readable proof text.
- `numerics.rs` — f64 geometry oracle.
- `svg.rs` — figure rendering; all text is XML-escaped here.
- `quiet_panic.rs` — one process-wide hook, thread-local mute.
- `runner.rs` — helpers shared by `bin/ddar.rs` and tests.

## Notes
- `geo.rs:compile` — rejects metric goals with `CompileError::MetricGoal`; before this, they compiled to `cong a b a b` and were "proved" trivially.
- Expression parsers cap exponents at 32 and binary operators at 256 per relation (stack overflow and blowup guards).
- `synthetic.rs`, `ratio.rs` — coordinates only propose candidates and read configuration (order, orientation, inside/outside); every cited fact is symbolic or DDAR-certified (`certify.rs`): generic similar triangles (`ratio.rs:certified_products`), the intercept parallel, the bisector, Stewart's ratio (`synthetic.rs:derived_ratio`). Circles come from symbolic equal-radius classes (`synthetic.rs:equal_radius_circles`).
- `synthetic.rs:prove_euclidean` — before the theorem rules, DDAR-certified right angles, midpoints and (aux pass) equal lengths are added; the aux pass also introduces intersections of goal segments and the centres of quadrilaterals on the goal points. All certified steps share one printed derivation (`render`).
- `ratio.rs:prove_ratio` — the product engine runs on a fresh figure, so the monomial engine's perpendicular feet never leak into its derivations. Its candidates (similar triangles, collinear splits, the bisector theorem) wait in `pending` until certified; a lone named-theorem citation is rejected as circular.
- `engine.rs:search_radical_axis` — the inside/outside branch is read from chord-end order, not from the conclusion.
- `synthetic.rs` rectangle detection needs `AD ∥ BC` or a third right angle; `AB ∥ DC` is implied by two right angles and proves nothing.

## Subfolders
- `bin/` — `ddar.rs`, the engine CLI (`--help` lists modes).
