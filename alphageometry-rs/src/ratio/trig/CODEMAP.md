# alphageometry-rs/src/ratio/trig — product-side trigonometric stages of the ratio prover

Up: [../CODEMAP.md](../CODEMAP.md)

## Files
- `bridge.rs` — T4 power of a point by the centre, T3 log bridges, S5 `prove_products_trig`, S6 `aux_search_trig`.
- `area.rs` — T6 sine-area rows, area additivity, the multiply-and-substitute closure, S7 `prove_by_areas`.
- `cos.rs` — T7 law of cosines, signed cosine substitutions, S8 `prove_by_cosines`.
- `render.rs` — display grouping of trig rows (law-of-sines chains, ratio lemma, trig Ceva).
- `tests.rs` — unit tests, including the injected-row and guard tests of TRIG_PLAN §3.

## Notes
- `bridge.rs` — S5/S6. `gather_center_power` (T4): `PX·PY = σ(OX² − OP²)`, σ read from betweenness and only when every instance agrees; pending `cong(O,X,O,Y)`, `coll(P,X,Y)`; headline. `gather_log_bridges` (T3): for a goal monomial and a pool monomial with the same small rational ratio `k` in every instance (`candidate_rat`, irrational never), proves `ln m1 − ln m2 − ln k = 0` with `certified_prove_core` and adds `m1 − k·m2` carrying the closure facts (`PStep::facts`, merged into the lead derivation). Cheap filters before any certified solve: the goal must lie in the span of all rows and outside the span of the sine-free rows (those are the cofactor rows' job). At most 64 accepted bridges and 2000 attempts. `aux_search_trig` (S6): ≤ 200 candidates — concyclic and centred second meets, perpendicular feet — similar triangles over goal points + the aux only (all points cost 45 s on a false Euler variant).
- `render.rs:display_groups` — cosmetic only (adds no reason, hides no row): the law-of-sines rows of one triangle print as one chain; both chains on either side of a cevian `AX` (X strictly between B and C in every instance) plus the equal-sines row at X print as "Ratio lemma in △ABC with cevian AX"; three such in one triangle get the trig-Ceva remark. Used by `Figure::render` (S2) and the bridge sub-proofs; non-trig proofs are singleton groups, byte-identical. A proof with a sine row closes with "Multiplying the sine relations above".
- `area.rs` (T6, stage S7) — goals of degree ≥ 2 over ≥ 4 points with a quadrilateral convex in every instance: the goal times one or two sines (non-zero in every instance; ≤ 40 multipliers) is closed over `2[xyz] = xy·xz·sin∠x` and `[abc] + [acd] = [abd] + [bcd]` (`area_closure` multiplies a base row by the quotient that makes one of its terms a pool monomial, and substitutes equal sines / lengths as pending `eqangle` / `cong` support rows), then log bridges and `certified_products`. Proves `named/ptolemy_second_general.geo`.
- `cos.rs` (T7, stage S8) — goals with a `cos(angle)` factor, times nothing or one goal length: law of cosines at each vertex (headline, one intro per base row so all its multiples count as one citation), `sin² + cos² = 1` (support), `cos = 0` from a certified `perp`, and `cos f = τ·cos g` with `τ` the sign of `cos f·cos g`, required constant over the instances (supplementary pairs get τ = −1). `LKey::Cos`/`Area` are product factors only, never log atoms.
