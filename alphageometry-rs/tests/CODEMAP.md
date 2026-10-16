# alphageometry-rs/tests — integration tests of the engine

Up: [../../CODEMAP.md](../../CODEMAP.md) · Engine: [../src/](../src/CODEMAP.md)

## Files
- `soundness.rs` — FALSE-statement regressions: inputs that once gave a false "proved", crashed or ran unbounded must stay unproved, error out, or finish. Every prover or goal-compilation change adds one here.
- `robustness.rs` — regressions for crashes and hangs found by adversarial review.
- `fuzz.rs` — the soundness fuzzer (`ddar::fuzz`) on small corpus slices: deterministic generation, pinned figures, no proof of a false variant (also `.geo` metric mode), and the 12 degenerate figures that once panicked DDAR (`golden/fuzz/`) stay unproved without a panic.
- `imo.rs` — the 26 bundled IMO rows (`--bench`) are proved, matching the Python reference.
- `proofs.rs` — proof traceback: minimal proofs without irrelevant assumptions, every bundled IMO row proves with tracking, a proof is a small fraction of the closure, trig steps spelled out.
- `corpus.rs` — the AG1 corpora: every problem translates with a numerically true goal, quick ones prove, false goals never prove, unknown constructions are reported, an overrunning child is killed at its deadline.
- `geo_examples.rs` — `examples/*.geo` compile and prove; `examples/imo/*.geo` compile with a numerically true goal.
- `named_theorems.rs` — `examples/named/`: every classical result compiles, holds numerically and gets an honest verdict.
- `metric_theorems.rs` — `examples/metric/`: classical length theorems proved without coordinates.
- `metric_golden.rs` — byte-identical proof text for metric examples (`golden/metric/`), so new prover stages cannot change old proofs.
- `closure_rules.rs` — the classical closure rules each fire where DDAR alone cannot, and never on a false near miss.
- `aux_kinds.rs` — the aux candidate generator emits arc midpoints and centres of similitude where they apply.
- `aux_rediscovery.rs` — delete a problem's given auxiliary points; the search must find replacements.
- `universal.rs` — the MAX solver proves every `examples/olympiad` problem from its bare statement.
- `human_proofs.rs` — the human-proof writer on the golden problems (`golden/human/`): recorded text and shape, every claim re-verified, the checker rejects corrupted proofs and false atoms, determinism, an expired deadline never lies, no failed check on the JGEX corpus, every angle bisector theorem named internal or external as the figure reads it, no midline of a collinear triangle, no raw predicate name, and at most two blocks (`vecten_point`, E059-47) that restate the block they cite.
- `human_chain_rows.rs` — the chain-row lints: one synthetic bad row per rule L1–L8 appended to the Simson proof, a directed congruence to 0° is not L7, a constant the chain ends with is not carried (L3); and over `golden/human/corpus_solved.tsv` (all 261 IMO/JGEX problems, 270 conjuncts, solved with their aux points) no checker violation, no L1/L2/L3/L5/L7 row, no chain whose second row repeats its start angle, and no directed row written with "=".
- `human_theorems.rs` — every theorem of the human-proof library fires on its figure and not without its hypotheses.

## Notes
- `golden/` is data: `human/` (writer goldens, the IMO 2023 P2 replica, `simson.geo`, and `corpus_solved.tsv` written by `ddar --corpus … --human` with `HP_SAVE`), `metric/` (pinned proof texts), `fuzz/` (fuzz cases in corpus format).
- `soundness.rs:bounded` — runs a check on a 2 MiB-stack thread (a server worker's size) with a timeout; debug builds get 32 MiB and 5× the time, since unoptimised frames are larger (the nested-construction cap test overflowed 2 MiB in debug).
- Run the suite in release (`cargo test --release`); the time budgets assume it.
