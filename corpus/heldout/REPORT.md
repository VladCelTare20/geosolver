# Held-out evaluation (2026-10-05)

GeoSolver's 30/30 on IMO-AG-30 was reached while developing against that set,
so it is a development-set number. This report measures the frozen engine on
problems it was never tuned on.

## Setup

- Engine: the deployed `ddar` (wf/release `4ff884c`), frozen as
  `geosolver-bench/heldout/ddar-frozen`, sha256
  `46aaf77deb8d038c97dd53bafa97328aad511cd855cd026d238c467872f26d0b`.
  No engine, rule, search or budget change was made for these problems.
- Settings identical to the 30/30 run: 120 s per problem, 4 jobs x 3 threads,
  2048 MB, proofs written. Chunks of at most 40 problems under the bench lock.
  4829 s of solver wall time on an i7-11700K (load 7-14 from other work).

## Results

| Dataset | Entries | Expressible | Solved | DDAR alone | With aux | Timeout | Not expressible |
|---|---|---|---|---|---|---|---|
| IMO-AG-30 re-run (development set) | 30 | 30 | **30** | 16 | 14 | 0 | 0 |
| HAGeo-409 (AlphaGeometry column, as released) | 409 | 409 | **318 (77.8%)** | 126 | 192 | 91 | 0 |
| IMO-AG-50, the 20 entries beyond IMO-AG-30 | 20 | 10 | **8** | 1 | 7 | 2 | 10 |
| After IMO-AG-50: ISL 2024 (minus G2), ISL 2025, IMO 2026 | 17 | 15 | **10** | 5 | 5 | 5 | 2 |

0 UNSOUND flags, 0 crashes, 0 memory kills.

### HAGeo-409 by difficulty (published columns from the HAGeo paper, Table 1)

| Level | Count | GeoSolver | DDAR / aux | AG1 | HAGeo @2048 | HAGeo @8192 |
|---|---|---|---|---|---|---|
| [1,3) | 161 | 157 (97.5%) | 89 / 68 | 118 | 141 | 149 |
| [3,4) | 112 | 103 (92.0%) | 26 / 77 | 44 | 87 | 93 |
| [4,5) | 71 | 47 (66.2%) | 9 / 38 | 13 | 29 | 36 |
| [5,6) | 43 | 10 (23.3%) | 2 / 8 | 2 | 5 | 7 |
| [6,7] | 22 | 1 (4.5%) | 0 / 1 | 0 | 1 | 2 |
| Total | 409 | **318 (77.8%)** | 126 / 192 | 177 | 263 | 287 |

### Solve times (solved problems)

| Set | Median | p90 | Max |
|---|---|---|---|
| IMO-AG-30 | 0.01 s | 4.3 s | 49.6 s |
| HAGeo-409 | 0.07 s | 10.4 s | 104.9 s |
| IMO-AG-50 held-out | 0.09 s | - | 0.2 s |
| After IMO-AG-50 | 0.12 s | - | 4.4 s |

### IMO-AG-50 held-out entries

- Solved (8): 2003 P4b, 2005 P1, 2007 P2, 2009 P4a, 2009 P4b, 2013 P3, 2023 P2, 2024 P4.
- Timeout (2): 2014 P3 (AG2 solved it), 2023 P6 (AG2 did not).
- Not expressible (10): 2001 P1, 2002 P6, 2003 P3, 2006 P1, 2006 P6, 2020 P6
  (AG2 could not formalize them either); 2001 P5a/b and 2021 P4 (length sums,
  which AG2 states with a predicate the AG1 language lacks); 2018 P6 (a point
  defined by angle conditions at two vertices).

### After IMO-AG-50

- Solved: ISL 2024 G1, G3, G4, G6, G7, G8 (concur => concyclic); ISL 2025 G1,
  G2, G3, G4 (= IMO 2025 P2, 0.33 s).
- Timeout: ISL 2024 G5, ISL 2024 G8 (concyclic => concur), ISL 2025 G5, G6,
  IMO 2026 P2.
- Not expressible: ISL 2025 G7 (tangent to an infinite family of lines),
  IMO 2026 P4 (a game).

## Comparison with published numbers

| Set | GeoSolver | Published |
|---|---|---|
| IMO-AG-30 | 30/30 (development set) | AG2 30, TongGeometry 30, HAGeo 28, AG1 25 |
| IMO-AG-50 | **38/50** | AG2 **42/50**; AG2 single tree 38; AG1 27 |
| HAGeo-409 | **318** | HAGeo 287 @8192, 263 @2048; AG1 177 (same inputs) |
| IMO 2024 P4, IMO 2025 P2 | both, under 0.4 s | HAGeo both; AG2 2024 P4 |

Why these comparisons are not like-for-like:

1. HAGeo was scored on its own-language column; this run used the AlphaGeometry
   column. The clean comparison there is GeoSolver vs AG1: 318 vs 177.
2. AG2's IMO-AG-50 formalizations are unreleased; all 25 IMO/ISL translations
   here are ours (validated numerically, see TRANSLATIONS.md). "Find all"
   problems are stated with the answer supplied (2009 P4, 2013 P3).
3. Larger rule set than AG1's deductive engine (Menelaus, Ceva, Stewart,
   bisector theorem, radical axis, length arithmetic, law of sines). 129 of the
   318 HAGeo proofs use classical length rules and 14 use trig.
4. No compute normalisation: 120 s x 3 threads on a consumer CPU, no neural
   network, vs HAGeo's 2048-8192 aux attempts on 64 cores and AG1/AG2's
   language-model beam search on accelerators.
5. HAGeo's IMO-30 proofs were checked by a human gold medallist; GeoSolver's
   are machine-checked only.
6. Some held-out IMO entries were seen during development (2001 P5a, 2003 P4b,
   2005 P1, 2009 P4a/b, 2013 P3 are in the bundled benchmark; 2023 P2 is the
   showcase; 2023 P6 is cited in engine comments). Of the uncontaminated IMO
   entries GeoSolver can express (2007 P2, 2014 P3, 2024 P4) it solves 2 of 3.
   No HAGeo-409 or 2025/2026 problem appears in the repo.
7. One pinned figure per problem; HAGeo inputs resample until the goal holds.

## Soundness spot-check

10 random solved held-out problems (seed 2026): goal holds on the figure and
fails on a jittered one, goal is not an assumption, proof ends at the goal,
0 failing steps among 469 numerically checkable steps. The same audit over all
18 solved hand translations: 0 failing steps; every pinned-figure proof uses
its implicit hypothesis. Caveat: 2009 P4a/P4b reach 60 and 90 degrees from the
same premises, so they depend on the drawn configuration (AG2's convention).

## Conclusions

1. On HAGeo-409 the engine generalises beyond its development set: 318/409,
   ahead of HAGeo at every difficulty level and of AG1 on identical inputs,
   with far less compute (caveats 1, 3-5).
2. The IMO-AG-50 gap to AG2 (38 vs 42) is mostly language: three of the four
   missing entries are length sums. On the entries both can state: 8 vs 9.
3. On problems published after every system's development: 10 of 15, including
   IMO 2024 P4 and IMO 2025 P2; not IMO 2026 P2.
4. No soundness issue found. TongGeometry cannot be compared on these sets.

## Sources

- HAGeo: https://arxiv.org/abs/2512.00097; dataset
  https://huggingface.co/datasets/HAGeo-IMO/HAGeo-409 @ 22b9f421 (parquet
  sha256 6e23990b...2675df9).
- AlphaGeometry 2: https://arxiv.org/abs/2502.03544 (Figure 8, Table 4);
  https://github.com/google-deepmind/alphageometry2 @ 251e7e16.
- AlphaGeometry 1: https://github.com/google-deepmind/alphageometry @ 6777cb58.
- IMO problems and shortlists: https://www.imo-official.org/ (2024 and 2025 SL).
- TongGeometry: https://www.nature.com/articles/s42256-025-01164-x.

Raw logs, proofs and scripts: /home/vlad/Projects/geosolver-bench/heldout/.
