# alphageometry-rs/src/engine — child modules of `engine.rs`

Up: [../CODEMAP.md](../CODEMAP.md)

## Files
- `trig.rs` — law-of-sines rows in the DDAR closure; `TrigMode` from `GEO_TRIG` / `ddar --trig` (off, fallback, lazy, always; default off).

## Notes
- `trig.rs:search_trig` — one round, called from the closure loop: candidate triangles pass the `search_similar` filter; each corner's angle `dir(vp) − dir(vq)` is reduced and classed up to sign (equal or supplementary ⇒ equal `|sin|`); a k-core keeps triangles with ≥ 2 corners whose class is shared by another triangle or constant; law of sines only between two such corners; equal sines chained along each class citing the angle reductions; tabulated constants 30/45/60/90 (and supplements) as exact prime combinations (half exponents via scaled `frac_value`). A class reducing to angle 0 disables trig for that `Ddar`.
- Sine variables are LHS with pivot rank 0 (`elim_core.rs:new_var_ranked`): they are eliminated first so length normal forms stay in lengths. An RHS variable would be read as a prime by `normalize`/`prime_pow`.
- `trig.rs:trig_force` — release-mode residual guard: a row whose numeric value is not 1 within 1e-9·terms is skipped and counted (`trig_stats`); debug builds assert.
- Off is bit-identical: no rows, no variables. Fallback is wired in `runner.rs:trig_fallback` (eligible goals `cong`, `eqratio`, `rconst`, `distmeq`, after the trig-free fixpoint); the aux search never sees it. Lazy switches trig on at every closure's trig-free fixpoint (`deduction_closure` loops once more) — base and aux search at every depth, since `aux_search.rs` is untouched. Always enables it from the first round. Reasons are `Reason::Theorem("law of sines" | "equal or supplementary angles have equal sines" | "sine of 30°: 1/2" …)`.
