# alphageometry-rs/src/engine — classical closure rules added to the DDAR fixpoint

Up: [../CODEMAP.md](../CODEMAP.md)

Child modules of `engine.rs` (they read `Ddar`'s private tables). Each rule is
one closure pass that logs a `Reason::Theorem` with symbolic premises; the
figure only picks configuration branches and rejects degenerate cases.

## Files
- `classics.rs` — rule switches (`Rule`, `Ddar::set_rule`, env `DDAR_DISABLE_RULES=sqlen,menelaus,bisconc`), the per-engine done-sets (`Done`), Menelaus + its converse, Ceva's converse, and bisector concurrency.
- `sqlen.rs` — the squared-length table (`dsq`, Yuclid arXiv 2510.01346 §2.3.2): feeds and reads `s(XY)=|XY|²`.

## Notes
- `classics.rs:search_menelaus_ceva` — only lines with ≥3 points can be sides. Forward Menelaus runs on every complete quadrilateral of figure lines (4 relations each); forward Ceva is the product of two of those, so it has no pass. Converses: unsigned product `1` in `dmul`, then the sign branch from betweenness — even number of D,E,F inside their sides ⇒ collinear (Menelaus), odd ⇒ the third cevian passes through the figure point where the other two meet (Ceva). The midpoint-triangle false test pins the branch.
- `classics.rs:search_bisector_concurrency` — two bisectors (internal or external, mod π) of a triangle meet at the incentre or an excentre, both on a bisector of the third angle, so the rule needs no branch.
- `sqlen.rs` — inputs: perpendicular figure lines (one basis equation per point pair, based at the meet point when there is one, which is Pythagoras), proportional `dmul` pairs (squares), Stewart for collinear triples whose ratio is a rational constant (signed `t` from betweenness), and point merges (`engine.rs:force_equal_points`). Outputs: proportional squared lengths ⇒ `dmul` ratio (`elimination.rs:dist_mul_sqrt` for irrational roots), and `s(AC)+s(BD)=s(AD)+s(BC)` ⇒ `AB ⟂ CD`, tried on every point pair of figure lines that are perpendicular in the figure but not yet in the angle table.
- `ElimDistSq::force_zero` drops an equation the figure contradicts (relative 1e-7) instead of adding it: callers only state theorems, so a mismatch is noise or degeneracy and dropping it costs completeness only.
- `engine.rs:deduction_closure` — the original passes run to their own fixpoint (`base_closure`) before these rules run once (`classical_rules`); repeat until neither changes. `classics.rs:inputs_changed` skips a pass whose input tables (row counts of the elimination cores, line arena, active points) are unchanged since it last ran.
- Candidates for bisector concurrency and squared-length perpendiculars come from the figure's directions (sorted, 1e-9 tolerance) and are then checked exactly in the normal forms; a missed candidate costs completeness only.
- Cost (hyperfine, `../geosolver-bench/rules/hf-*.md`): `--bench` 1.23-1.29x; aux-heavy corpus problems 1.4-1.9x time-to-proof on a loaded box.
- Done-sets are keyed by point ids and cloned with the engine, so warm-start (`aux_search.rs:WarmBase`) keeps them; they only skip re-adding equations already added.
