# alphageometry-rs/src/engine — child modules of `engine.rs`: classical closure rules and law-of-sines rows

Up: [../CODEMAP.md](../CODEMAP.md)

Child modules of `engine.rs` (they read `Ddar`'s private tables). Each rule is
one closure pass that logs a `Reason::Theorem` with symbolic premises; the
figure only picks configuration branches and rejects degenerate cases.

## Files
- `classics.rs` — rule switches (`Rule`, `Ddar::set_rule`, env `DDAR_DISABLE_RULES=sqlen,menelaus,bisconc`), the per-engine done-sets (`Done`), Menelaus + its converse, Ceva's converse, and bisector concurrency.
- `sqlen.rs` — the squared-length table (`dsq`, Yuclid arXiv 2510.01346 §2.3.2): feeds and reads `s(XY)=|XY|²`.
- `trig.rs` — law-of-sines rows in the DDAR closure; `TrigMode` from `GEO_TRIG` / `ddar --trig` (off, fallback, lazy, always; default off).

## Notes
- `classics.rs:search_menelaus_ceva` — only lines with ≥3 points can be sides. Forward Menelaus runs on every complete quadrilateral of figure lines (4 relations each); forward Ceva is the product of two of those, so it has no pass. Converses: unsigned product `1` in `dmul`, then the sign branch from betweenness — even number of D,E,F inside their sides ⇒ collinear (Menelaus), odd ⇒ the third cevian passes through the figure point where the other two meet (Ceva). The midpoint-triangle false test pins the branch.
- `classics.rs:search_bisector_concurrency` — two bisectors (internal or external, mod π) of a triangle meet at the incentre or an excentre, both on a bisector of the third angle, so the rule needs no branch.
- `sqlen.rs` — inputs: perpendicular figure lines (one basis equation per point pair, based at the meet point when there is one, which is Pythagoras), proportional `dmul` pairs (squares), Stewart for collinear triples whose ratio is a rational constant (signed `t` from betweenness), and point merges (`engine.rs:force_equal_points`). Outputs: proportional squared lengths ⇒ `dmul` ratio (`elimination.rs:dist_mul_sqrt` for irrational roots), and `s(AC)+s(BD)=s(AD)+s(BC)` ⇒ `AB ⟂ CD`, tried on every point pair of figure lines that are perpendicular in the figure but not yet in the angle table.
- `ElimDistSq::force_zero` drops an equation the figure contradicts (relative 1e-7) instead of adding it: callers only state theorems, so a mismatch is noise or degeneracy and dropping it costs completeness only.
- `engine.rs:deduction_closure` — the original passes run to their own fixpoint (`base_closure`) before these rules run once (`classical_rules`); repeat until neither changes. `deduction_closure_until` (wf/search) checks its deadline inside `base_closure` between passes and again before each `classical_rules` round; a classical pass itself is not interruptible. `classics.rs:inputs_changed` skips a pass whose input tables (row counts of the elimination cores, line arena, active points) are unchanged since it last ran.
- Candidates for bisector concurrency and squared-length perpendiculars come from the figure's directions (sorted, 1e-9 tolerance) and are then checked exactly in the normal forms; a missed candidate costs completeness only.
- Cost (hyperfine, `../geosolver-bench/rules/hf-*.md`): `--bench` 1.23-1.29x; aux-heavy corpus problems 1.4-1.9x time-to-proof on a loaded box.
- Done-sets are keyed by point ids and cloned with the engine, so warm-start (`aux_search.rs:WarmBase`) keeps them; they only skip re-adding equations already added.
- `trig.rs:search_trig` — one round, called from the closure loop: candidate triangles pass the `search_similar` filter; each corner's angle `dir(vp) − dir(vq)` is reduced and classed up to sign (equal or supplementary ⇒ equal `|sin|`); a k-core keeps triangles with ≥ 2 corners whose class is shared by another triangle or constant; law of sines only between two such corners; equal sines chained along each class citing the angle reductions; tabulated constants 30/45/60/90 (and supplements) as exact prime combinations (half exponents via scaled `frac_value`). A class reducing to angle 0 disables trig for that `Ddar`.
- Sine variables are LHS with pivot rank 0 (`elim_core.rs:new_var_ranked`): they are eliminated first so length normal forms stay in lengths. An RHS variable would be read as a prime by `normalize`/`prime_pow`.
- `trig.rs:trig_force` — release-mode residual guard: a row whose numeric value is not 1 within 1e-9·terms is skipped and counted (`trig_stats`); debug builds assert.
- Off is bit-identical: no rows, no variables. Fallback is wired in `runner.rs:trig_fallback` (eligible goals `cong`, `eqratio`, `rconst`, `distmeq`, after the trig-free fixpoint); the aux search never sees it. Lazy switches trig on at every closure's trig-free fixpoint (`deduction_closure_until` loops once more after `base_closure` and `classical_rules` are both quiet) — base and aux search at every depth, since `aux_search.rs` is untouched. Always enables it from the first round. Reasons are `Reason::Theorem("law of sines" | "equal or supplementary angles have equal sines" | "sine of 30°: 1/2" …)`.
