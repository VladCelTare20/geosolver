# alphageometry-rs

A high-performance Rust reimplementation of the **DDAR** symbolic reasoning core
of [AlphaGeometry2](https://www.jmlr.org/papers/v26/25-1654.html), plus a
language-model-free **auxiliary-point search** that extends what the standalone
system can prove.

It is a from-scratch port of the reference Python engine in the parent directory
(`ddar.py`, `elimination.py`, `numericals.py`, `parse.py`), verified to produce
the **same results** on every bundled problem while running **~47× faster** — and
it restores or adds the capabilities the AG2 release left out:

* a **high-level construction language** (no coordinates, nestable expressions,
  constraint-solved points),
* **numbered, citation-annotated proofs** (`--proof`) in the style of the
  original AlphaGeometry — the AG2 release only answers yes/no,
* **classical Euclidean proofs of *length* goals** (`--metric`) — specific
  lengths, sums of squares — that DDAR's three linear algebras cannot express,
  written as a numbered synthetic proof citing *named theorems* (Pythagoras, the
  perpendicular-from-the-centre lemma, Apollonius, perpendicular chords, …), the
  way a human writes one; `--theorems` lists the library,
* **figure rendering** to SVG (`--svg`), the port of original AG's matplotlib
  drawing,
* a **universal MAX solver** (`--max`) — throw maximum effort at *any* problem
  (DDAR, else an iterative-deepening auxiliary-construction search across all CPU
  cores), assuming nothing about how hard it is; plus `--batch` to run it over a
  whole directory in parallel. No trained model is used.

## What this is (and isn't)

The open-sourced part of AlphaGeometry2 is its *logical core*, **DDAR**
("Deductive Database + Algebraic Reasoning"). It proves olympiad geometry facts
by:

1. using concrete floating-point coordinates as an **oracle** for which facts
   hold (collinearity, equal distances, inscribed angles, …), and
2. **discharging** those facts exactly via Gaussian elimination over three
   algebraic systems — directed angles (mod a half-turn), multiplicative
   distances (in log space), and additive segment lengths.

The full AlphaGeometry2 system pairs DDAR with a trained transformer that
proposes *auxiliary constructions*. **That neural model is not part of this (or
the reference) repository and is not reproduced here.** What this project adds on
the symbolic side is a search over classical constructions (see
[Auxiliary-point search](#auxiliary-point-search)) that recovers some of that
capability without any learned model — made practical only because a single DDAR
run now costs milliseconds.

## Describing problems: the high-level language

The low-level DDAR format is brutal to author — you must supply an exact
floating-point coordinate for **every** point and write raw predicates:

```text
a@0.0_0.0 b@1.0_0.0 m@0.5_0.0 = coll a b m, cong m a m b ? coll a b m
```

The high-level language lets you describe *how points relate* instead;
coordinates and defining predicates are generated for you. It is fully
expression-based with first-class lines and circles, and — for anything the
built-in constructions don't cover — you can pin a point with **arbitrary
constraints** that are solved numerically.

```text
A B C = triangle
O = circumcenter(A, B, C)
F = foot(A, line(B, C))              # first-class objects; calls nest
prove perp(O, midpoint(B, C), B, C)  # midpoint computed inline in the goal
```

A real olympiad lemma — the reflection of the orthocenter over a side lies on
the circumcircle — is four lines:

```text
A B C = triangle
H = orthocenter(A, B, C)
K = reflect(H, line(B, C))
prove cyclic(A, B, C, K)
```

The **escape hatch** — define a point by *any* constraints and let the solver
find it (no fixed construction needed):

```text
A B = segment
P = point: dist(P, A) = dist(P, B), angle(A, P, B) = 90
prove cong(P, A, P, B)
```

Run a program or file, see the generated low-level form, or list the vocabulary:

```sh
cargo run --release --bin ddar -- --geo examples/orthocenter_reflection.geo
cargo run --release --bin ddar -- --geo-show examples/midsegment.geo
cargo run --release --bin ddar -- --constructions
```

What makes it flexible:

- **Expressions nest.** Any argument can itself be a construction:
  `foot(A, line(B, C))`, `midpoint(midpoint(A, B), C)`,
  `meet(line(A, B), circle(O, C))`.
- **First-class objects.** `line(...)`, `circle(O, A)`, `circle(A, B, C)` /
  `circumcircle(...)`; intersect them with `meet` (which returns one or two
  points, e.g. `P Q = meet(line(A,B), circle(O,C))`).
- **Constraint-defined points.** `P = point: <relations>` solves for `P` with
  Levenberg–Marquardt + random restarts. Relations: `on`, `coll`, `cong`,
  `perp`, `para`, `cyclic`, `eqangle`, `eqratio`, plus the sugar
  `dist(A,B)=dist(C,D)` and `angle(A,B,C)=<deg>` / `=angle(D,E,F)`.
- **A large construction library** (41 built-ins, covering AlphaGeometry's
  `defs.txt`): centres (`circumcenter`, `orthocenter`, `incenter`, `excenter`,
  `centroid`, `nine_point_center`), `midpoint`, `foot`, `reflect` (over a point
  or line), `bisector`, `perp_bisector`, `perp_line`, `para_line`, `tangent`,
  `shift`, `parallelogram`, `square`, `eq_triangle`, `iso_triangle`, `meet`, and
  the locus family `on_line` / `on_circle` / `on_circum` / `on_bline` /
  `on_pline` / `on_tline` / `on_dia`. Run `--constructions` for the full list.
- **No coordinates, reproducible.** Free points are sampled with a deterministic
  RNG; the figure is re-sampled if degenerate and **validated through DDAR** so a
  compiled problem never crashes the engine.
- **Mistake detection.** If the goal does not hold in the sampled figure, the
  compiler warns you the statement is probably false before trying to prove it.
- **Auto-fallback to aux search.** `--geo` first tries pure deduction and, if
  that is not enough, automatically searches for auxiliary points
  (see [Auxiliary-point search](#auxiliary-point-search)).
- **Round-trips** to the low-level format (`--geo-show`).

Worked examples live in [`examples/`](examples/) and are all kept proving by
[`tests/geo_examples.rs`](tests/geo_examples.rs).

## Proofs, not just verdicts

The AG2 open-source release performs "elimination of variables, *without
proof*" — it answers yes/no. The original AlphaGeometry printed numbered
proofs; this engine brings that back. Every constraint row in the Gaussian
elimination carries the set of facts it depends on, so when a goal reduces to
zero the engine knows *exactly which facts it used* and renders a backward
closure as numbered steps with citations:

```
$ ddar --geo examples/orthocenter_reflection.geo --proof

Proof of cyclic A B C K (8 steps, 19 facts recorded in total):
001. assumption: perp A H B C
002. assumption: perp B H A C
003. assumption: cong B H B K
004. assumption: cong C H C K
005. similar triangles: △BCH ∼ △BCK [003 & 004]
006. similar triangles: △BHK ∼ △BKH [003 & 005]
007. collinear: H A K [001 & 005 & 006]
008. concyclic (inscribed angles): K C B A [002 & 005 & 006 & 007]
∎ cyclic A B C K
```

Proofs are *filtered*, not dumps: IMO 2000 P1's proof is 48 steps out of 224
facts derived during the closure, and a deliberately-irrelevant assumption is
provably excluded ([`tests/proofs.rs`](tests/proofs.rs)). The citation sets are
a sound over-approximation (a step may occasionally cite a premise a
hand-minimized proof could drop, but every cited premise genuinely entered the
algebra). Provenance tracking costs ~4% and is only enabled when a proof is
requested.

## Theorem knowledge

How much classical "theory" does the engine actually command? We audited all
43 deduction rules of the original AlphaGeometry (`rules.txt`) plus the named
theorems olympiad solvers lean on, and verified each against this engine
(worked examples in [`examples/`](examples/), all kept proving by the test
suite):

| Theorem | Status |
| --- | --- |
| Euler line (O, G, H collinear) | ✓ derived by the closure, 6 ms |
| Nine-point circle | ✓ derived |
| Incenter–excenter lemma ("Fact 5") | ✓ derived |
| Power of a point (secant–secant) | ✓ derived |
| Tangent–chord angle (r15/r16) | ✓ derived |
| Thales, both directions (r19/r20) | ✓ derived |
| Perpendicular-bisector / equidistance family (r22–r24) | ✓ derived |
| Midline & midpoint rules (r06, r25–r29) | ✓ derived |
| **Angle bisector theorem, both directions (r11/r12)** | **built-in rule** (was: unprovable without 2 auxiliary points and ~1000 search runs) |
| **Intercept / Thales theorem, parallel transversals (r41/r42)** | **built-in rule** (unprovable before: the transversals' intersection is not in the figure) |
| Ptolemy, Stewart | not representable (sums of products are outside the three linear algebras) — documented limitation |

The two built-in rules are implemented as closure passes with full proof
provenance — proofs cite them by name:

```
006. angle bisector theorem: A X B C [001 & 003 & 005]
```

Both are proved sound for *directed angles mod π* (the internal/external
bisector cases collapse into one valid rule), and every instance they force is
double-checked numerically by the debug-assert suite across the entire IMO
corpus.

New in the language: product-of-lengths goals,
`dist(P,A) * dist(P,B) = dist(P,C) * dist(P,D)` (power of a point is
[`examples/power_of_a_point.geo`](examples/power_of_a_point.geo)), and
second-intersection constructions — `meet(line(A, I), circumcircle(A, B, C))`
skips existing points, so arc midpoints are one line.

## Metric goals: classical Euclidean proofs

DDAR reasons over three *linear* algebras (directed angles, log-distances,
additive lengths). Whole classes of olympiad statements live outside them —
**length problems**: a length equal to `3√3`, a sum of squares `AC² + BD² = 144`,
a median-length (Apollonius) identity.

`--metric` proves these with a **classical Euclidean proof** — a numbered,
synthetic deduction that cites *named theorems*, exactly the way a human writes
one ([`src/synthetic.rs`](src/synthetic.rs)) — **not** a coordinate computation.

A Romanian national-exam problem: a circle of radius 6, perpendicular chords
`AB ⟂ CD`, `M` the midpoint of `AB`, `OM = 3`.

```
$ ddar --metric part_a.geo          # show that AM = 3√3

EUCLIDEAN PROOF
  Goal:  dist(A,M)^2 = 27
  1. |OM| = 3 (given), so OM² = 9.
  2. |OA| = 6 (given), so OA² = 36.
  3. In right triangle AMO (right angle at M), by the Pythagorean theorem
     AO² = MA² + MO².
  Combining the equations above gives dist(A,M)^2 = 27  (= 3√3²). ∎

$ ddar --metric part_b.geo          # prove that AC² + BD² = 144

EUCLIDEAN PROOF
  Goal:  dist(A,C)^2 + dist(B,D)^2 = 144
  1. Let B' be the point diametrically opposite B (so BB' is a diameter through O).
  2. |OB| = 6 (given), so OB² = 36.
  3. O is the midpoint of BB', so BO² = ¼·BB'².
  4. BB' is a diameter, so ∠BAB' = 90°  (Thales' theorem).
  5. BB' is a diameter, so ∠BDB' = 90°  (Thales' theorem).
  6. CD ⟂ AB and AB' ⟂ AB, so CD ∥ AB'  (two lines ⟂ the same line are parallel).
  7. In right triangle BDB' (right angle at D), BB'² = DB² + DB'²  (Pythagoras).
  8. CD ∥ AB' are parallel chords, so AC = DB'  (equal arcs ⇒ equal chords).
  ⇒  AC² + BD² = DB'² + BD² = BB'² = 4·OB² = 144. ∎
```

The second proof is the important one. `AC² + BD² = 4R²` **is** a named theorem
(the perpendicular-chords theorem) — so citing it would be proving the problem
with itself. The engine refuses to do that: it **derives** the result from
scratch, introducing the classical auxiliary point it needs (the antipode `B'`)
and finishing with Thales, "two perpendiculars to a line are parallel", "parallel
chords cut equal arcs", and Pythagoras.

How it works: each pair of points has a *squared length* `AB²`. Every elementary
theorem applicable to the figure contributes one exact linear equation among
these squared lengths; which theorems apply is decided by the figure's own
hypotheses (symbolically — never a coordinate coincidence). When the base figure
is not enough, the engine adds classical auxiliary points (antipodes) and retries
from elementary theorems. It eliminates the unknown squared lengths by exact
rational combination until the goal is reached, and a **minimisation pass** drops
every citation the proof does not need, so it reads like a hand-written proof.
`ddar --theorems` lists the library.

The prover is **sound and strictly Euclidean**: it only cites true theorems
whose hypotheses follow from the construction, and combines them exactly.
Products and ratios of unsquared lengths (Ptolemy, Menelaus, Ceva, power of a
point, the angle-bisector ratio) go to the multiplicative prover (`ratio.rs`),
which builds them from similar triangles. The figure's coordinates may only
*propose* a fact (two triangles that look similar, a 2:1 division) and read
configuration (which side, which order); a proposed fact is used only after the
DDAR closure derives it from the hypotheses, and that derivation is printed in
the proof.

A goal neither prover reaches is **not proved**. It is checked in 48
independently sampled figures and reported as `MetricError::NoProof` with that
numeric evidence ("holds numerically — not a proof"), or as `Refuted` with a
counterexample. A numeric check is never presented as a proof. `tests/` cover
the two Romanian parts, Pythagoras, the rejection of false claims, and the
seven named theorems that currently hold only numerically
(`tests/named_theorems.rs`).

## Figures

`--svg <file>` renders the problem to a standalone SVG (the spirit of original
AG's matplotlib drawing): labelled points, hypothesis segments, lines through
collinear sets, circles from `cyclic`/center patterns, and the goal highlighted
in dashed red. Works with every solving mode:

```sh
ddar --geo examples/thales.geo --svg thales.svg --proof
```

## Results

All 26 bundled IMO problems (15 solvable by DDAR alone, 11 with the paper's
manually-provided auxiliary points) are proved by both implementations.

| Workload | Python reference | Rust (`alphageometry-rs`) | Speedup |
| --- | ---: | ---: | ---: |
| All 26 problems | 46.3 s | **0.99 s** | **~47×** |
| Hardest single problem (IMO 2008 P6, 31 points) | 18.1 s | **0.37 s** | **~50×** |

*(Measured on the same 16-core Windows machine; Python 3.10 + NumPy, Rust 1.94
release build. Reproduce with the commands below.)*

Where the speed comes from:

- **Points are dense integer ids** and every per-pair table is a flat `n×n`
  array, eliminating the reference's pervasive hashing of point *objects* and
  ordered pair/triple tuples.
- **Exact rationals with a machine-word fast path.** [`Rat`](src/rational.rs)
  runs on `i64/i64` and only promotes to arbitrary precision on overflow, versus
  Python's always-bignum `fractions.Fraction`.
- **Sparse linear combinations** are canonical sorted small-vectors, so
  simplification and constraint insertion are cache-friendly linear merges.

## Quick start

```sh
# Prove the bundled IMO set with per-problem timings (like `python -m test`):
cargo run --release --bin ddar -- --bench

# Prove a single problem given in the AlphaGeometry format:
cargo run --release --bin ddar -- "a@0.0_0.0 = ; b@1.0_0.0 = ; c@0.5_0.9 = \
  cong a b a c ? cong a b a c"

# Search for auxiliary points if pure deduction is not enough:
cargo run --release --bin ddar -- --aux "<problem string>"

# Prove a length goal with a classical Euclidean proof (named theorems):
cargo run --release --bin ddar -- --metric "O = free; A = point: dist(O,A)=6; \
  B = point: dist(O,B)=6; C = point: dist(O,C)=6; D = point: dist(O,D)=6, perp(A,B,C,D); \
  prove dist(A,C)^2 + dist(B,D)^2 = 144"
cargo run --release --bin ddar -- --theorems     # the classical-theorem library

# Universal MAX solver — maximum effort at ONE problem (assumes nothing about it):
cargo run --release --bin ddar -- --max examples/olympiad/2019_p2.geo

# Run the universal MAX solver on a whole directory, in parallel across all cores:
cargo run --release --bin ddar -- --batch examples/olympiad

# Report which bundled auxiliary points are load-bearing:
cargo run --release --bin ddar -- --explore
```

The release build targets the host CPU (`.cargo/config.toml` sets
`target-cpu=native`) so the f64 geometry oracle and 128-bit rational path use the
machine's full instruction set. Delete that file for a portable binary.

Run the test suite (the debug build activates numeric-consistency assertions
that check *every* forced fact is numerically true):

```sh
cargo test                       # unit + integration (26/26) + aux rediscovery
cargo test --lib rational        # 4000-case property fuzz of Rat vs BigRational
cargo clippy --all-targets       # lints (clean)
```

## Architecture

Bottom-up, each layer is small and independently tested:

| Module | Role |
| --- | --- |
| [`rational`](src/rational.rs) | Exact `Rat`: `i64` fast path + bignum fallback, canonical form |
| [`numerics`](src/numerics.rs) | Floating-point Euclidean geometry (the oracle) |
| [`lincomb`](src/lincomb.rs) | Sparse linear combinations over `Rat` |
| [`elim_core`](src/elim_core.rs) | Incremental Gaussian elimination (RREF) |
| [`elimination`](src/elimination.rs) | Angle / multiplicative-distance / additive-distance systems |
| [`predicate`](src/predicate.rs) | The low-level problem language and parser |
| [`geo`](src/geo.rs) | The high-level construction language → low-level compiler |
| [`engine`](src/engine.rs) | The `Ddar` deductive-closure loop |
| [`proof`](src/proof.rs) | Fact log, reasons, backward closure, proof rendering |
| [`synthetic`](src/synthetic.rs) | Classical **Euclidean** proofs of length goals (named-theorem library) |
| [`ratio`](src/ratio.rs) | Classical proofs of ratio / product length goals (similar triangles) |
| [`certify`](src/certify.rs) | DDAR certification of facts the metric provers propose |
| [`metric`](src/metric.rs) | Metric-goal grammar; Euclidean proof, else `NoProof` with numeric evidence |
| [`aux_search`](src/aux_search.rs) | Ranked auxiliary-point construction search |
| [`svg`](src/svg.rs) | Figure rendering to standalone SVG |

## Correctness methodology

This is a theorem prover, so faithfulness and soundness are the priority:

- **Same answers as the reference** on all 26 problems
  ([`tests/imo.rs`](tests/imo.rs)).
- **Numeric-consistency assertions.** Every `force_*` operation `debug_assert!`s
  that the fact being asserted is numerically true (angle ≈ integer multiple of
  π, ratio ≈ 1, segment sum ≈ 0). The full suite runs in debug with these on and
  none fire — strong evidence the port is faithful, not accidentally passing.
- **Property-based fuzzing.** `Rat` is checked against `num_rational::BigRational`
  over thousands of full-range `i64` inputs for `+ - * / neg`, ordering,
  equality, and floored `mod 1`, exercising the overflow-promotion path.
- **A subtle invariant is documented and relied upon:** the Gaussian-elimination
  normal form is a canonical coset representative (because `simplify` is linear
  and reduces modulo the row space), so *pivot-selection order does not affect
  any result*. This lets the port use a different, deterministic tie-break than
  Python without changing behavior. See the module docs in
  [`elim_core.rs`](src/elim_core.rs).
- **Adversarially reviewed.** Multi-agent review rounds (each reviewer verifies
  its own findings by constructing concrete inputs and running them) have found
  and fixed real defects that tests missed — e.g. an SVG NaN/escaping bug, a
  nondeterministic parallel-search budget, an aux-search completeness gap, and a
  faithfully-ported upstream tolerance gap where a long, sub-`ATOM`-thin triple
  was admitted as a triangle and forced a false ratio (the debug asserts turned
  that silent upstream unsoundness into a caught panic). Regressions live in
  [`tests/robustness.rs`](tests/robustness.rs).

## Auxiliary-point search

Many olympiad problems cannot be solved by pure deduction on the given figure —
the proof needs an extra point. `aux_search` searches a library of classical
constructions, each built with exact coordinates *and* the predicates that pin it
down symbolically (13 construction kinds):

- midpoints, circumcenters, orthocentres, perpendicular feet;
- reflections over a point or a line, angle-bisector centres (incentre and the
  three excentres) and bisector feet, tangent points from a point to a circle;
- line/line and line/circle intersections — including the second intersection of
  a *cevian* with the circumcircle of a figure triangle (the search invents the
  circumcircle; no hypothesis need name it), and circle/circle intersections.

Each candidate is appended to the problem and DDAR is re-run; degenerate
candidates that would panic are caught, so the search is robust. Because a DDAR
run is now ~10 ms, trying hundreds of candidates takes under a second.

Three things make it find constructions *fast* without sacrificing completeness:

1. **Ranking, not pruning.** *Every* construction is generated (so nothing
   provable is missed), but each is scored — a construction-kind prior, a
   goal-relevance bonus (goal points full weight, their 1-hop neighbours half),
   and a bonus for candidates built on *salient* lines and circles (those the
   figure actually names). The handful of relevant constructions are therefore
   tried first. Line ∩ circle intersections are included, so second-intersection
   points (a cevian meeting the circumcircle again) come out directly.
2. **Parallelism by default.** The bottom search level fans candidate DDAR runs
   across all cores with rayon's `find_first` over a fixed ranked prefix, which
   returns the *sequentially-first* success — a deterministic result identical to
   a serial search, just faster.

**Validation** ([`tests/aux_rediscovery.rs`](tests/aux_rediscovery.rs)):

* IMO 2001 P5 with its auxiliary point `o` deleted: pure DDAR fails; the search
  finds a completing construction in **17 DDAR runs**.
* IMO 2019 P2 with `a2`/`b2` deleted — *second-intersection* points the earlier
  generator could not express at all: the search rediscovers both
  `intersect(a·a1, circle(o,a))` constructions and re-proves the goals in
  **~3 s** (down from 28 s before these optimizations).
* IMO 2005 P1 with `p` deleted: rediscovers `reflect(a1 over a2c2)` — a
  construction on a line whose two endpoints appear together in *no* predicate,
  which a purely salient generator would miss ([`tests/robustness.rs`](tests/robustness.rs)).

**Universal MAX solver.** There is no "hard set" and no per-problem tuning: the
same maximum-effort path is applied to *every* problem. `solve_max` tries DDAR
directly, then **iteratively deepens** the auxiliary search (depth 2, then 3, …)
with an escalating budget, saturating all cores at each level. Easy problems
finish in the first pass; only genuinely deep ones ever reach the expensive
levels — nothing is assumed easy or hard. Raise the ceiling without recompiling
via `AUX_MAX_RUNS` and `AUX_MAX_DEPTH`. The bare-statement problems in
[`examples/olympiad/`](examples/olympiad/) (kept solving by
[`tests/universal.rs`](tests/universal.rs), runnable together with `--batch`) are
stated with *only their given points*: IMO 2019 P2 invents the circumcircle of
`ABC` and the second intersections of both cevians with it; "the reflection of
the orthocenter over a side-midpoint is concyclic" is proved by discovering two
circumcircles the statement never names.

This is a genuine, learning-free extension of standalone solving power. It is
not a replacement for the trained model on the hardest problems: it only
asserts a construction's *minimal* defining predicates, so it cannot recover
auxiliary points that also carry extra hypotheses.

## On machine learning (an honest note)

The full AlphaGeometry2 pairs DDAR with a trained transformer that proposes
auxiliary constructions. **Neither AG repository releases the training
pipeline or the synthetic-data generator** — the original AG repo ships only
inference code with downloadable checkpoints, and the AG2 repo ships only DDAR.
Retraining or fine-tuning the "recognition" model from public material is
therefore not possible, here or anywhere.

A learned ranker *was* built and GPU-trained here (a small MLP over
hand-engineered features, reaching a respectable offline average-precision), but
rigorous A/B testing showed it **did not beat the hand-tuned heuristic** in the
actual search — the goal-relevance features the net learned kept burying the one
construction it needed to promote. The decisive signal it was meant to teach
(that a line through a high-degree triangle vertex is a *cevian*, so its second
intersection with the circumcircle is the useful auxiliary — the crux of IMO
2019 P2) works far better hard-coded in the heuristic, where there is no learned
bias to fight. So **the ML path was removed entirely**: the auxiliary-point
search ranks with the heuristic alone. The honest substitute for the missing
neural model is a ranking function grounded in a *statistical prior* over
construction kinds (which kinds succeed on this repo's own strip-and-rediscover
corpus, plus the distribution reported in the AlphaGeometry paper) and
goal-relevance — documented as exactly that, no neural network involved.

## Parallelism (and why not async)

The prover uses **data parallelism** (rayon), on by default, wherever it is both
a real win and semantics-preserving:

* the **auxiliary-point search** fans candidate DDAR runs across all cores with
  rayon's `find_first` over a fixed ranked prefix, which returns the
  *sequentially first* success in ranked order — so the chosen construction is
  identical to a serial search, only faster;
* **`--batch`** runs the universal MAX solver over a whole directory in parallel,
  and **`--bench`** re-times the bundled set as a parallel batch; both report the
  wall-clock vs. summed-CPU speedup;
* the optional `--features parallel` additionally parallelizes the O(n³)
  similar-triangle search's per-triangle arithmetic (modest at IMO sizes,
  headroom for larger figures).

An honest note on **async**: async/await is a tool for overlapping *IO waits*
(sockets, disks, timers). This prover is purely CPU-bound — there is no IO to
overlap, and wrapping the solver in an async runtime would add scheduling
overhead and `Send`/lifetime friction for zero throughput gain. What the
workload actually calls for is *data parallelism* (independent candidate runs
on separate cores), which is what the rayon integration above provides —
deterministically. The default build remains fully serial.

## Relationship to the reference

This crate is a clean-room-style port of the Python in the parent directory and
mirrors its semantics closely, including a couple of deliberate quirks (e.g. the
key/insert asymmetry in the arc↔chord transfer rule). The Python remains the
specification; the bundled `problems.tsv` is generated from `test.py`.

## License

Apache-2.0, matching the upstream AlphaGeometry2 code. This is not an official
Google or DeepMind product.
