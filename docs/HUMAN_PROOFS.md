# Human proof writer — specification

Status: design, ready for implementation. Branch `wf/human-proofs-design` (from `wf/release` 4ff884c).
Golden examples: [`docs/human-proofs/`](human-proofs/). Reference prototype: [`docs/human-proofs/prototype.patch`](human-proofs/prototype.patch) (evidence only — not product code).

The owner asked for this verbatim:

> make the proofs more human! Just look at how proofs look like, people don't find a million similar triangles and solve it, they use theorems, nicely structured and compact! Look at IMO proofs, you'll see what I mean!

This document specifies a **deterministic writer** that re-presents a verified DDAR proof as a short olympiad-style proof made of claims, angle and ratio chains and named theorems. It is not an LLM, and it never adds mathematics the engine did not verify.

---

## 1. Requirements

| # | Requirement | Where it is met |
|---|---|---|
| R1 | Structured: setup, then numbered claims, then a conclusion | §6.5, §6.7 |
| R2 | Compact: no restated hypotheses, no isosceles or similar-triangle spam | §6.3 table, §8 metrics |
| R3 | Theorem-based: inscribed angles, Thales, tangent–chord, power of a point, radical axis, named engine theorems | §6.2 atom vocabulary |
| R4 | Natural prose in EN and RO; citation chips, figure highlighting, PDF/PNG | §7 |
| R5 | **Faithful**: every displayed claim is an engine fact, a hypothesis or a construction. Every "because" is backed by verified facts. A theorem name appears only where that theorem justifies the step | §5, §6.8 checker, §9 |
| R6 | Deterministic, no LLM | §6 (all stages exact or ordered), invariant I6 |
| R7 | Never affects soundness or solve speed; bounded time | §7.4 |
| R8 | The raw derivation stays available behind a "Full derivation" toggle | §7.2 |
| R9 | Falls back to the raw proof for anything it cannot classify, and never fails a solve | §6.9 |

Non-goals for v1:
- Proofs from the Euclidean metric prover (`synthetic.rs`, `ratio.rs`). They are already prose (`ProofView.style = "euclidean"`), and restructuring them is v2.
- Inventing a different proof route than the engine's. Route choice happens only by picking among engine proofs (§6.10).

---

## 2. How humans write olympiad geometry (research summary)

Sources read: Evan Chen's solution notes for IMO 2000, 2002, 2003, 2004, 2005, 2007, 2008, 2009, 2010, 2012, 2013, 2014, 2015, 2016, 2018, 2019, 2020, 2022 and 2023 (`web.evanchen.cc/exams/IMO-YYYY-notes.pdf`, 19 PDFs). Official shortlist booklets for 2007–2023 (`imo-official.org/problems/IMOyyyySL.pdf`). The IMO Compendium shortlist for 2004 (`imomath.com/imocomp/sl04.pdf`) and for 2003 and 2005 via a compilation PDF. AoPS threads for 2004/1 and 2023/2. Evan Chen's handouts "Remarks on English", "Directed Angles" and "GeoSlang". Yufei Zhao's "Lemmas in Euclidean Geometry" (geolemmas.pdf). Twenty IMO problems were analysed, 19 of them in `corpus/imo_ag_30.txt` (all but 2023 P2).

Conventions the writer implements. Quotes are verbatim from those sources.

1. **One-sentence reduction, then claims.** "By Miquel's theorem it's enough to show AMRN is cyclic." (EC 2004). Then "Claim — We have LPS collinear. *Proof.* Because …" (EC 2023). P1/P4-level solutions use 0–2 claims; P2/P5-level use 3–4.
2. **A claim's proof is one chain.** "Proof. Because ∡LPB = ∡LDB = ∡CBD = ∡CBS = ∡SCB = ∡SPB." (EC 2023). Chains have 3–7 terms.
3. **Reasons are either omitted, pooled in front, or given as one parenthetical.** "from cyclic quadrilaterals APBE and DPLB we can see that ∠PAD = …" (ISL 2023 G4). "(the latter half is Reim's theorem)" (EC 2010). The Directed-Angles handout says one may "omit the part where I wrote out the explicit reasons for each step".
4. **The converse theorem is not named; the chain ends in the conclusion.** "By ∡QPA2 = ∡BAA2 = ∡BB2A2 = ∡QB2A2, points P, Q, A2, B2 are concyclic" (ISL 2019 G3).
5. **Directed angles ∡ modulo 180°**, announced once. "Throughout the solution we use oriented angles." (ISL 2019). "Never take half of a directed angle" (Directed-Angles handout). When a half-angle is needed, authors switch to ∠ (EC 2002, 2016).
6. **Hypotheses are never restated.** Equal radii and isosceles base angles are silent or take four words: "Since OC = OR as well" (EC 2007); "Thus OP = OQ".
7. **Thales and right angles** are silent or "since … is a diameter": "Since WX and WY are diameters in ω1 and ω2, respectively, we have ∠WZX = ∠WZY = 90°" (ISL 2013).
8. **Named results are cited with "by" and the bare name**: "By power of a point", "By radical axis on BNMC, ω1, ω2", "we're done by Reim theorem", "by the incenter-excenter lemma".
9. **Auxiliary points** are introduced with "Let X be …" ("Let X be the second intersection of ΓB and ΓC", "Let F be the antipode of A", "Define K = CT ∩ AE"). They may be defined conveniently and identified later ("Let O denote the circumcenter of △PAB. We claim it is the desired concurrency point").
10. **Similar triangles: one key similarity, displayed, then cashed out.** "consequently we have the (opposite orientation) similarity △APQ ∼ △MKL. Therefore AQ/AP = ML/MK = …" (EC 2009). Across the 35 solutions read, similar triangles do real work in about 14, almost always once per solution. Chains of 2–3 similarities appear only in official solutions of length-flavoured problems (ISL 2003 G1, 2014 G1).
11. **Symmetric halves** take one word: "and similarly", "Analogously".
12. **Endings**: "as desired.", "as needed.", "which is what we wanted to prove.", ∎.
13. **Length.** EC's primary P1/P4-level solutions run 47–170 words: 2004/1 47, 2000/1 72, 2020/1 85, 2002/2 89, 2009/2 111, 2007/4 147, 2022/4 167. P2/P5-level run about 150–400 words. ISL solutions are about 1.5–2× longer.

Citation frequency over the 35 solutions (manual tally):

| Fact | Used in | How cited |
|---|---|---|
| inscribed angle / cyclic quadrilateral | ~27 | almost never named: "so ABCD is cyclic" |
| parallel lines ⇒ equal angles | ~15 | "As ML ∥ AC …" |
| similar triangles | ~14 | named every time |
| isosceles / equal radii / equal tangents | ~14 | mostly silent |
| Thales (right angle on a diameter) | ~9 | silent or "since … is a diameter" |
| tangent–chord | ~8 | silent or "PQ is tangent … so ∠DPQ = ∠DBP" |
| power of a point | ~7 | "by power of a point" |
| radical axis / centre | ~6 | "by radical axis" |
| congruent triangles | ~6 | "△EAB ≅ △MAB" |
| Miquel, Reim, homothety/spiral, midline | 3–5 each | named |
| law of sines, Menelaus, angle bisector theorem | 1–2 each | named, or the bisector theorem unnamed ("divide AC in the ratios …") |

The style the writer targets is EC's: claims, directed-angle chains, silent bookkeeping and named theorems.

---

## 3. What the engine records today

### 3.1 Fact log (`alphageometry-rs/src/proof.rs`)

`ProofLog.facts: Vec<Fact { reason: Reason, premises: Vec<FactId> }>`. The `Reason` variants are:
- `Assumption`, `Construction` (pre-rendered strings)
- `SimilarTriangles(t1, t2)`
- `Concyclic(pts)`, `Collinear(pts)`, `EqualRadius(o, pts)`
- `PointMerge`, `TangentMerge`
- `TransferAddMul(pair, pair)`, `TransferArcChord(pair, pair)`
- `Theorem(&str, pts)`, `Formula(&str, text, pts)`

The proof is `ProofLog::closure(used)`: every fact reachable from the goal's dependency set, numbered by `FactId` order.

### 3.2 Where premises come from

`premises` are not "the facts this rule used". They are the union of the **row dependency sets** of every elimination row touched while reducing the rule's input expressions (`ElimAngle/ElimDistMul::simplify_deps`). In `ElimCore`, every stored pivot row carries `row_deps` = its own fact plus the deps of every row used to reduce it (`elim_core.rs:add_constraint`).

Consequence (proved in §4.1): the target equation is in the span of the original rows of the cited facts. But the citation set is a large over-approximation, and the coefficients are lost.

### 3.3 How each rule forces rows

| Rule (engine.rs) | Fact | Rows it forces (table) |
|---|---|---|
| `force_pred` | `Assumption` | the predicate's row (angle/ratio/add), or a collinear/concyclic registration fact (next line) |
| `force_collinear_with` | `Collinear` | direction glue `dir(main) = dir(merged line)` (angle), and `|ab| + |bc| = |ac|` along the line (add) |
| `force_concyclic_with` | `Concyclic` | inscribed-angle rows against the circle's defining points (angle), and equal radii if centred (ratio) |
| `force_similar` | `SimilarTriangles` | 2 angle rows + 2 ratio rows, orientation from the figure |
| `search_circles` | `EqualRadius` → `Concyclic` | registration |
| transfers | `TransferAddMul`, `TransferArcChord` | one row in the other table |
| classics/sqlen/trig | `Theorem`, `Formula` | the theorem's rows (squared lengths, Menelaus products, law-of-sines rows with sine variables) |

`force_zero` turns an angle equation into an **exact real equation**. It adds `−k·π`, with `k` read from the figure (`elimination.rs:force_zero`). So every angle row holds over ℝ with the half-turn as unit, not merely mod π. This matters for §4.

### 3.4 How ag-studio presents it

- **`present.rs`** parses the proof **text**:
  - `parse_ddar_proof` and `ddar_line` turn each `NNN. reason [cites]` line into a `Step {n, kind, rule, fact, deps}`.
  - `drop_restatements` folds collinear/concyclic lines that restate their single premise.
  - `theorem_fact`, `transfer_fact`, `three_on_a_circle` and `mirror_similarity` (Euclidean path only) improve single lines.
  - `Names` renames engine points (`_5` → `M`).
- **`site.js:renderSteps`** draws the numbered steps:
  - Hypothesis restatements fold into a "Steps 1–8 restate …" group.
  - Citation chips (`.cite`, `data-step`) scroll to and flash the cited step.
  - Hovering or focusing a step calls `viewer.highlight(points, facts)`.
- **`render.rs:report_pages_fit`** paginates the same steps into the PDF/PNG report, using `i18n.rs` `report.*` keys in EN/RO.
- **"AI explanation"** (`translate.rs:humanize_proof`, `/api/humanize`) sends the step list to `claude -p` (Opus) and shows its markdown. It is not checked against anything.

### 3.5 What is missing for human proofs

1. **The linear combination.** The engine records which facts a reduction touched, not how they combine. Without coefficients you cannot write "∡A = ∡B = ∡C" with a reason per link.
2. **The matched criterion.** `SimilarTriangles` does not say whether AA, SAS, SSS or SSA matched. `Concyclic` does not say which inscribed-angle group or centre fired. `Theorem("radical axis", [x,u,v])` does not record the two chords used.
3. **Minimal citations.** For example, step 047 of IMO 2004 P1 cites 22 facts, and 15 suffice (§4.2).
4. **Human vocabulary.** The engine has no Thales, tangent–chord, central angle or power-of-a-point rules. It reaches them through isosceles self-similarities (`△OMB ∼ △OBM`) and halving. Of the 47 steps of IMO 2004 P1, 19 are similarity steps and 13 of those are isosceles self-similarities.
5. **Structure.** A flat list has no claims, no notion of which facts matter, and no reuse counts.
6. **Goal chase.** The final step from the last fact to the goal is not printed at all. `∎` follows the last fact, and the reduction that proves the goal is invisible.

---

## 4. The key technique: exact certificates

### 4.1 Recovery is a small exact linear solve

For a derived fact `f` with target equation `t`, for example ∡FBN − ∡FAN = 0 for "F, N, B, A concyclic":
- Let `R(f)` be the original rows forced by the facts in `premises(f)`. Each `(fact, row)` is recorded when the row is added.
- Then `t ∈ span_ℚ(R(f))`.

Proof sketch. Every stored pivot row is a ℚ-combination of original forced rows of facts in its `row_deps`; this holds inductively over `add_constraint`, including back-substitution into earlier rows, which merges deps. The reduction of `t` uses stored rows whose deps are exactly `premises(f)`.

So the writer solves `t = Σ λᵢ rᵢ` exactly over ℚ:
- **Angle table.** Rows are exact real equations (§3.3), so the residual coefficient of the π unit must be an integer; the prototype checks it is.
- **Ratio table.** Rows are in log space over pair variables and prime constants.
- **Additive and squared tables.** The same, linear.

The system is small (tens of rows, tens to hundreds of variables) and is solved with the same sparse rational elimination as `ElimCore`, tracking combination vectors.

**Minimal premises.** Insert rows cheapest-first until `t` is in the span, then drop rows (expensive first) while it stays in the span. This yields an irreducible support.

### 4.2 Feasibility, measured

Prototype: `docs/human-proofs/prototype.patch`. It records `(fact, row)` in `ElimCore` when tracking is on, plus the binary `hproof`. Corpus sweep: 4 jobs × 3 threads, aux search 45 s (IMO) / 20 s (JGEX).

| | IMO (`imo_ag_30`) | JGEX (`jgex_ag_231`) |
|---|---|---|
| proofs produced | 29 | 230 |
| derived (non-hypothesis) facts in proofs | 886 | 2528 |
| re-certified with a human certificate (§6.2) | **678 (76.5 %)** | **1970 (77.9 %)** |
| not modelled by the prototype (named theorems, trig rows, segment transfers, merges) | 184 | 473 |
| modelled but not certified | 18 (2.0 %) | 53 (2.1 %) |
| prototype panics (merged/double points) | 6 | 32 |
| goal re-certified | 28/29 | 227/230 |
| chains rendered / pure single-angle chains / "clean" chains | 976 / 299 / 787 | 2505 / 953 / 2131 |
| max links in one chain | 17 | 18 |
| writer time per proof: median / p95 / max | 78 ms / 768 ms / 1.9 s | 1 ms / 17 ms / 3.4 s |

Notes on the table:
- "Not modelled" kinds are not failures. They are shown under their theorem name, citing the engine's premises (§6.3); the prototype simply has no obligation model for them.
- The "modelled but not certified" cases come from the prototype *guessing* the matched criterion. With obligations recorded by the engine (§5.2) there is nothing to guess.
- The prototype rebuilds atoms from scratch for every step. Times above include that waste (§7.4).

Engine-premise certification on IMO 2004 P1 (all derived facts the prototype models). Minimal versus engine citations:

| step | fact | criterion found | engine cites | minimal |
|---|---|---|---|---|
| 021 | △OMB ∼ △OBM | SAS (isosceles) | 3 | 1 |
| 038 | F, N, B, A concyclic | inscribed | 10 | 6 |
| 039 | P, R, M, B concyclic | inscribed | 11 | 4 |
| 042 | △ABN ∼ △ACM | AA | 8 | 6 |
| 047 | B, P, O, C collinear | direction | 22 | 15 |
| goal | coll P B C | — | 1 | 1 |

Exact angle chain recovered for step 038, in the engine's own vocabulary:

```
∡FBN + ∡CAR
 = ∡CNB + 90°      [hyp perp b f a r]
 = …               [△ONC∼△OCN ×½]
 = 0               [△OBN∼△ONB ×½]
```

The ½ coefficients are how the engine gets Thales: two isosceles triangles on a diameter. This is why §6.2 re-certifies over a **human vocabulary**. After re-certification the same step reads ∡BFA = 90° = ∡BNA, with citations "BF ⟂ AR" and "BC is a diameter" (2 atoms instead of 10 facts; golden [IMO 2004 P1](human-proofs/imo-2004-p1.md), Claim 1).

### 4.3 Pitfall found by the prototype: atoms must be exact

Atoms the writer introduces (e.g. Thales: `dir(ZX) − dir(ZY) = ½`) are only true mod π. Engine rows are exact over ℝ. Mixing them under rational coefficients can leave a half-integer π residual and reject a valid target. The prototype hit this on the Euler line (step 023 uncertified until fixed).

Rule: **every atom and every target gets its integer π offset fixed from the figure** (`k = round(value)`), exactly as `force_zero` does. Then all rows are exact, and the residual must be exactly 0.

A second pitfall: "O, G, H collinear" written with pivot O and with pivot G are different linear forms. Collinearity obligations must name the pivot the rule used (recorded, §5.2) or try all three.

---

## 5. Engine changes (tracked mode only)

All changes are behind `ElimCore.track` / `Ddar::new_tracked`. That is the proof-extraction re-solve, never the search. With tracking off, behaviour is bit-identical.

### 5.1 Row recording

`ElimCore` gains `pub fact_rows: Vec<(FactId, LinComb)>`. `add_constraint` pushes `(fact, added_eq)` before reducing, when `track && fact.is_some()`. Redundant rows are kept: they are true consequences of their fact. Rows refused by `force_zero`/`force_one` because the figure contradicts them are never recorded. The prototype patch shows the exact 6-line change.

### 5.2 Obligation recording

`proof::Fact` gains `obligation: Option<Obligation>`. It is set at `log.add` sites when tracking:

```rust
pub enum Table { Angle, Ratio, Add, Sq }
pub struct Eq { pub table: Table, pub lhs: LinComb, pub rhs: LinComb }   // raw, exact (π offset fixed)
pub enum Obligation {
    Similar { criterion: SimCrit /* AA | SAS(vertex) | SSS | SSA(vertex) */, eqs: Vec<Eq>, same_orientation: bool },
    Inscribed { a: PointId, b: PointId, group: Vec<PointId>, centres: Vec<PointId>, eqs: Vec<Eq> },
    CircleByCentre { centre: PointId, eqs: Vec<Eq> },
    CollinearByAngle { pivot: PointId, a: PointId, b: PointId, eq: Eq },
    Registration { source: FactId },           // collinear/concyclic created right after an Assumption/Theorem
    Merge { a: PointId, b: PointId, eqs: Vec<Eq> },
    ArcChord { arcs_to_chord: bool, eq: Eq },
    RadicalAxis { x: PointId, chord1: (PointId, PointId), chord2: (PointId, PointId), circles: (FactId, FactId), eq: Eq },
    Theorem { inputs: Vec<Eq> },               // classics.rs, sqlen.rs, similitude, bisector, intercept
    Trig { row: Eq },                           // the Formula row itself
}
```

The search sites already compute these expressions (`deps_of_angle_expr(c,a,c,b)` etc.), so recording costs one clone per fact in tracked mode. Sites to change:
- `engine.rs`: `force_similar` (pass the matched bucket: sss/aa/sas/ssa from `search_similar`), `search_concyclic` (both the collinear and the cyclic branch), `search_circles`, `merge_points`, `transfer_*`, `search_radical_axis`, `search_bisector_theorem`, `search_intercept_theorem`, `search_similitude`, `force_pred_because`.
- `classics.rs`, `sqlen.rs`, `trig.rs` (`trig_force`).

### 5.3 Export

`Ddar::trace(&self) -> EngineTrace` (new, read-only) returns:
- the facts with obligations;
- the four `fact_rows` tables;
- pair→variable maps;
- the proven lines and circles **with the fact that created each**, so line classes can be built *as of a given fact* (the prototype wrongly used the final lines; see §6.6);
- point names, coordinates, `subst`.

`runner::solve_problem_with_proof` grows a sibling, `solve_problem_with_trace(problem) -> Result<Option<(String, EngineTrace, Vec<FactId>)>>`.

Tests that pin "no behaviour change":
- `--verify-warm` passes.
- A new test runs the whole corpus with tracking on and off and asserts identical fact logs (reasons, premises, order).
- `ddar --bench` stays 26/26.
- Tracked-solve overhead is measured with `hyperfine` on the corpus `--corpus-one` loop and reported in the PR.

---

## 6. The writer

New module `alphageometry-rs/src/human/` (engine data, no serde):

| file | role |
|---|---|
| `mod.rs` | `pub fn write(trace: &EngineTrace, goal: &Predicate, aux: &[AuxInfo], opts: &Opts) -> HumanProof` — never panics; returns fallback blocks instead |
| `cert.rs` | exact basis with combination tracking; `certify(table, target, atoms, cost)`; exact π offsets (§4.3) |
| `atoms.rs` | the human vocabulary (§6.2): generation from objects known before a fact, template-hypothesis checks, span checks |
| `classify.rs` | per-fact presentation (§6.3) |
| `claims.rs` | human DAG, pruning, claim selection, ordering (§6.4–6.5) |
| `chain.rs` | split choice, BFS/DFS/beam linearisation, pooled fallback, ratio and trig computations (§6.6) |
| `model.rs` | `HumanProof` and friends (§7.1) |
| `check.rs` | independent verifier (§6.8) |
| `CODEMAP.md` | folder map (house rule) |

Pipeline per proof:

```
EngineTrace ─▶ objects & line classes as of each fact
           ─▶ for each derived fact f in the proof closure:
                 obligation(f) ─▶ atoms available before f ─▶ cheapest certificate
           ─▶ classify ─▶ human DAG (edges = certificate supports) ─▶ prune from goal
           ─▶ claim selection ─▶ second certification pass (claims are cheap atoms)
           ─▶ chain linearisation ─▶ blocks ─▶ check.rs ─▶ HumanProof (or fallback)
```

### 6.1 Objects

Before writing, the writer builds:
- **Circles**: hypothesis circles (`cong`/`cyclic`/`circle(O, A)` predicates), engine `Concyclic` facts and centre groups (points certified equidistant from a point in the ratio table).
- **Lines**: union–find of `Collinear` facts.
- For every object, the earliest fact that establishes it.

Named circles in the setup follow the source program: circumcircle `Ω`/`(ABC)`, a circle with diameter `ω`, others `(BMR)`. Greek letters go to circles referenced at least twice; the rest use point lists, matching convention 8 of §2.

### 6.2 Atom vocabulary

An **atom** is one human-statable equation with a template label. An atom `a` is *admissible before fact f* iff both hold:
- **(a) Template hypotheses** are verified facts available before `f`: hypotheses, constructions or engine facts with id < f. Example for Thales: O equidistant from X, Y, Z (certified in the ratio table) and X, O, Y collinear (a proven line).
- **(b) Span:** the atom's exact row lies in the span of the original rows of facts available before `f`. Checked exactly.

Condition (a) makes the **label honest**. Condition (b) guarantees the atom adds **no mathematics beyond the engine's closure**. Both are required. An atom failing (b) is dropped even if the theorem is true. Never relax this.

| atom | template hypotheses | row | cost | display |
|---|---|---|---|---|
| hypothesis row | — | the predicate's row | 1 | "(AD ⟂ BC)" |
| collinearity | proven line | glue rows | 0 | silent (line classes) |
| inscribed angle | 4 points on one proven circle | ∡XAY = ∡XBY | 1 | "(ABXY cyclic)" or "(inscribed angles in Ω)" |
| Thales | O centre of XYZ, X, O, Y collinear | ∡XZY = 90° | 1 | "(XY is a diameter)" |
| tangent–chord | P, Y, Z on circle centre O; PX ⟂ OP proven | ∡(PX,PY) = ∡PZY | 1 | "(PX is tangent to (…) at P)" |
| perpendicular bisector / kite | XA = XB and YA = YB certified | XY ⟂ AB | 1 | "(XY is the perpendicular bisector of AB)" |
| parallel transport | ℓ ⟂ m, ℓ′ ⟂ m (or ∥ chains) | ℓ ∥ ℓ′ | 1 | "(both ⟂ BC)" |
| equal radii | centre group | OA = OB | 1 | silent or "(radii)" |
| isosceles base angles | OA = OB | ∡OAB = ∡ABO | 2 | "(OA = OB)" |
| central angle | centre group, Z on circle | ∡AOB = 2∡AZB | 2 | "(central angle)" — integer coefficient, valid mod π |
| power of a point | 4 points on a proven circle (incl. centre groups), two chords through X proven collinear | XA·XB = XC·XD | 1 | "(power of X w.r.t. ω)" |
| midline | M, N midpoints (hypotheses) | MN ∥ AB, AB = 2·MN | 1 | "(midline)" |
| orthocentre | AH ⟂ BC, BH ⟂ CA | CH ⟂ AB | 1 | "(the altitudes concur)" |
| displayed claim | an earlier block | its rows | 1 | "(Claim k)" |
| engine fact rows (fallback) | — | the fact's rows | 3 concyclic, 5 similar, 6 self-similar | the fact's statement |

Costs used in the prototype; tune with the metrics of §8. A locality term adds +1 per point of the atom outside the target's points (prototype `eff()`), which picks Thales at B,N,C over a detour through F.

Admissibility evidence from the prototype:
- Thales, isosceles, central angle and power-of-a-point atoms are generated only after a ratio-table certificate of the equal radii.
- Inscribed atoms are generated only for 4-subsets of a proven circle, and every atom is span-checked. With these atoms, 038 of IMO 2004 P1 needs 2 atoms (perp hyp, Thales) instead of 10 cited facts, and 042 needs 2 Thales atoms.

### 6.3 Classification of engine facts

Every derived fact in the proof closure gets exactly one presentation:

| engine fact (and certificate shape) | presentation |
|---|---|
| `Assumption`, `Construction` | never restated. Cited as a reason when used. Aux constructions appear once in the setup (§6.7) |
| `Collinear`/`Concyclic` with `Registration` obligation (created right after an Assumption or Theorem) | **silent** — merged into its source |
| `Concyclic` whose points all lie on a hypothesis circle, or are certified equidistant from a hypothesis centre (`CircleByCentre` with hypothesis-only atoms) | **silent** — "P lies on (BMR)" is the definition of P |
| `EqualRadius` | silent; names the circle in the setup |
| `SimilarTriangles` with t2 a permutation of t1 (self-similar) | **silent**. It becomes an isosceles atom ("OA = OB ⇒ base angles") or an equal-radius atom. Never displayed as a similarity |
| `SimilarTriangles` whose only downstream use is a product `XA·XB = XC·XD` with A, B, C, D concyclic and X on AB, CD | **renamed** to power of a point (golden IMO 2004 P1, Claim 3) |
| `SimilarTriangles` with ratio 1 from radii (SSS/SAS congruence) | "△… ≅ △… (SSS/SAS)", inline. If its only use is an angle equality, show that equality with the congruence as reason, e.g. "FO bisects ∠BFN (△OBF ≅ △ONF)" |
| `SimilarTriangles` used ≥ 2 times, used by the goal certificate, or involving an aux point | **claim**: "△AHB ∼ △MON. Proof. Their sides are parallel: …" (golden Euler line) |
| `SimilarTriangles` otherwise | inline reason in the one chain that uses it |
| `Concyclic` by inscribed angles, `Collinear` by angles | claim or inline (§6.5) with a chain whose endpoints give the conclusion ("so A, B, F, N are concyclic") |
| `PointMerge` / `TangentMerge` | sentence "so X coincides with Y" / "so the circles are tangent at T" |
| `TransferAddMul` | silent length bookkeeping. If its rows are needed, inline "AB = 2·AM" |
| `TransferArcChord` | "FB = FN, since ∠BAF = ∠FAN in (ABFN)" (equal arcs ⇔ equal chords) |
| `Theorem("radical axis")` | "A has equal powers w.r.t. (BMR) and (CNR) (AM·AB = AN·AC), so it lies on their radical axis RP" |
| `Theorem(angle bisector theorem [converse] / intercept theorem / homothety / Monge–d'Alembert / Menelaus [converse] / Ceva converse / bisector concurrency / triangle equality)` | "by ⟨name⟩, ⟨fact⟩", reusing `present.rs:theorem_fact` for the statement and `i18n.rs:theorem_ro` for RO |
| `Theorem` from `sqlen.rs` (Pythagoras, Stewart, squares) | lines of a length computation |
| `Formula` (law of sines, double/triple angle, equal sines, sine of k°) | lines of a trig computation (§6.6.4) |
| anything whose obligation cannot be certified | **fallback**: raw step text with its minimal engine premises (§6.9) |

### 6.4 Human DAG and pruning

Nodes are derived facts that are not silent. An edge g → f means an atom used by f's chosen certificate originates from g (directly, or via the template hypotheses of a derived atom). Build the backward closure from the goal and drop everything unreachable.

This removes facts the engine derived but the human certificate does not need. On IMO 2004 P1, 024, 027, 035 and 039/040 drop or go silent. On IMO 2023 P2, 049 (△OBO1 ∼ △OPO1) is not used by any human certificate.

### 6.5 Claim selection

Score each surviving node:
- `uses` (out-degree in the human DAG);
- `weight`: 3 for concyclic, collinear, tangency, congruence or a displayed similarity; 2 for equal lengths or angles; 1 otherwise;
- `aux`: 1 if the statement involves an auxiliary point;
- `size`: links in its chain.

A node becomes a **Claim** if it is:
- a concyclicity, collinearity, tangency or displayed similarity and (`uses ≥ 2` or it is in the goal's certificate or `aux = 1`); or
- any fact whose own proof has ≥ 3 links.

Otherwise it is **inlined**:
- a short proof (≤ 2 links) is appended as a parenthetical where it is used ("(A, B, F, N are concyclic as ∡BFA = 90° = ∡BNA)");
- else an unnumbered sentence before its user.

After selection, run certification again with displayed claims as cost-1 atoms ("(Claim k)"). Humans cite their own claims. This second pass usually shortens later chains.

Order: topological, ties by engine fact id, so the narrative follows the derivation. Cap at 7 claims; beyond that, consecutive single-use claims merge into unnumbered steps.

### 6.6 Chain linearisation

The statement of a target determines its **split** L = R:

| target | split options |
|---|---|
| ∡-equality | the goal as written |
| cyclic ABCD | the 6 chord/viewer choices (∡ACB = ∡ADB …) — all 4-point forms are the same linear form up to sign |
| collinear A, P, B | ∡XPA = ∡XPB for a point X off the line (EGMO Thm 3.5) |
| perpendicular / parallel | ∡(ℓ, m) = 90° / 0 |
| ratio / length | A/B = C/D, or AB = CD |

**Line classes are taken as of the fact being proved**: union–find over collinear facts with id < f. The prototype used final lines and printed circular chains for the last step (2004 P1 step 047). Don't.

Stages, first success wins:
1. **BFS on binary atoms.** Nodes are single angles ∡(ℓ₁, ℓ₂) up to the line classes, plus constants 0° and 90°. Edges are admissible atoms whose quotient row relates two nodes (inscribed, hypothesis ∡/∥/⟂, isosceles, Thales, tangent–chord, kite, parallel transport, claims). The shortest path L → R is the chain. It is a certificate by construction (sum of edges), and it is how humans chase ("∡SPD = ∡LPD = ∡LBD = …"). Bound: 20 000 node expansions. Tie-break: fewer links, then more constant intermediates ("= 90° ="), then lexicographic.
2. **Certificate + DFS ordering.** Take the cheapest certificate (§4.1 with atom costs). Search for an order of its terms in which every intermediate is a single angle (prototype `chain_atoms`, 200 000 node budget). In the prototype this succeeds on 299/976 chains on IMO and 953/2505 on JGEX. Stage 1 is expected to raise this, because it may pick a different certificate.
3. **Beam (width 64)** over orders, minimising non-single intermediates. Show the chain if ≤ 8 links and ≤ 2 non-single intermediates; elide those with "…" and attach the sub-certificate's reasons to the jump.
4. **Pooled.** "Angle chasing with ⟨reasons⟩ gives ⟨statement⟩." The reasons are the certificate's support after collapsing: atoms on one circle become "inscribed angles in Ω"; central-angle pairs on one circle become the inscribed atom they sum to; claims become "(Claim k)". The UI offers "show the computation", which displays the exact combination Σ λᵢ·(atomᵢ) as an aligned sum (§7.2). This is convention 3 of §2, and it is never unverified.

**Coefficients.**
- If every λ is an integer, the chain is written in directed angles ∡ (mod 180°). Integer multiples are valid mod π.
- If a ½ is needed (central angle halved, bisector of a directed angle), the halving link is written in undirected ∠, with values as drawn: "∠MCN = ½∠MON = ∠RON". This matches what the engine verified, because its integer offsets were fixed from the same figure (§3.3). The block is then marked `as_drawn: true`, and the UI and report already have wording for that (`present.rs:as_drawn`).

**6.6.4 Ratio and trig computations.**
- Ratio targets are rendered as product chains (`AH/(2·OM) = … = 1`) or as equalities of lengths.
- Trig rows (`Formula`) carry sine variables. They are named from the engine's own row text: `|sin ∠(AC,AB)|` → `sin∠BAC`, and `|sin(x + 90°)|` → `cos x`. Absolute values are dropped when every angle is an interior angle of the figure.
- Consecutive links touching the same triangle are grouped into one displayed equality, labelled by the triangle and rule ("law of sines in △BOMₐ"). Golden: [orthocenter–vertex distance](human-proofs/orthocenter-vertex-distance.md), 42 raw steps → one 9-line computation.

### 6.7 Setup and auxiliary points

The setup holds:
- the directed-angle convention line (only if a ∡ chain is present);
- named circles (§6.1);
- one "Let …" line per auxiliary point actually used by a displayed block, with the definition worded by the existing templates (`present.rs:aux_view` / `resolve_branch`, i18n `aux.*`). Examples: "Let F be the foot of the perpendicular from B to AR." "Let T be the point of BS with OT ⟂ BE." (EC style: "Let X be …").

Engine helper points (`_5`) get their `Names` (`M`, `N`, `H′`) and, when they carry a meaning ("midpoint of BC"), a "Let" line as well.

Hypotheses are never restated. A hypothesis appears only as a reason, in its shortest wording ("(AD ⟂ BC)", "(AR bisects ∠BAC)").

### 6.8 Independent checker (`check.rs`)

Runs on every `HumanProof` before it leaves the engine. It recomputes from `EngineTrace` and never trusts the writer's intermediate data:

- **I1.** Every block statement is an engine fact in the proof closure, a hypothesis/construction, or the goal. A statement re-expressed by the writer ("XA = XP" for △OAX ≅ △OPX) must have its rows in the span of the source fact's rows.
- **I2.** Every chain link `eₖ = eₖ₊₁` satisfies `eₖ − eₖ₊₁ = Σ λᵢ atomᵢ` exactly, with exact π offsets and a zero residual. Pooled sentences check their full combination.
- **I3.** Every atom used is admissible (§6.2 a+b) with respect to the block's position.
- **I4.** Every theorem name in the output comes from a template in `atoms.rs`/`classify.rs` whose hypotheses were checked under I3. There is no free-text label path.
- **I5.** Block supports reference only earlier blocks, silent facts, hypotheses and constructions. The DAG is acyclic and the last block proves the goal.
- **I6.** Determinism: output depends only on the trace. Sort every collection that is iterated; no hash-order output. The same input gives byte-identical JSON.
- **I7.** The writer cannot change `status`, `proved` or the raw proof.

A violation turns the offending block into a fallback block (§6.9) and is counted in metrics. Under `debug_assertions` it panics in tests.

### 6.9 Fallback

- Any fact without a model, certificate or chain becomes a **raw block**: the engine's statement and rule name (current `ddar_line` wording), citing its **minimal engine premises** (§4.1, computed from the engine's own rows). That alone cuts IMO 2004 P1 step 047 from 22 to 15 citations.
- If the goal itself cannot be certified (4 of 259 corpus proofs in the prototype, all of them prototype panics on merged points), the human proof is not offered at all. The UI shows the full derivation as today.
- The writer never fails a solve: it runs under `catch_unwind`, with a deadline, after the verdict.

### 6.10 Choosing the most human proof

`agstudio best` compares thousands of proofs and keeps the fewest steps (`engine.rs:consider`). Changes:
- `consider` keeps a top-K = 16 set by `(steps, facts)` instead of one proof.
- After the search, the writer runs on each candidate within a slice of `min(25 % of budget, 5 s)`. The candidate with the lowest **HumanCost** wins; ties go to fewer raw steps.

```
HumanCost = 10·claims + 3·displayed sentences + Σ chain links
          + 2·(pooled reasons) + 6·(non-pure chains) + 25·(fallback blocks)
          + 4·(aux points shown) + 1·⌈words/20⌉
```

`render` (first proof) uses the writer on its single proof. The note says "most readable of N proofs examined".

---

## 7. Data model, rendering, integration

### 7.1 Engine structs (`human/model.rs`, plain Rust, no serde)

```rust
pub struct HumanProof {
    pub version: u16,                  // 1
    pub setup: Vec<SetupLine>,
    pub blocks: Vec<Block>,            // claims and steps in order; last = conclusion
    pub as_drawn: bool,                // some chain halves an angle (§6.6)
    pub metrics: Metrics,              // raw steps, blocks, claims, links, sentences, fallbacks
}
pub enum SetupLine {
    DirectedAngles,
    Circle { name: String, through: Vec<PointId>, centre: Option<PointId>, diameter: Option<(PointId, PointId)> },
    Aux { point: PointId, aux_index: usize },   // wording from the aux construction
    Helper { point: PointId, meaning: HelperMeaning },
}
pub struct Block {
    pub id: u16,
    pub kind: BlockKind,              // Claim(n) | Step | Conclusion | Raw
    pub stmt: Stmt,
    pub body: Vec<Sentence>,
    pub engine_facts: Vec<FactId>,    // the engine facts this block re-presents (for chips/toggle)
    pub points: Vec<PointId>,         // highlight set
    pub objects: Vec<ObjRef>,         // circles/lines to light in the figure
}
pub enum Sentence {
    Chain { terms: Vec<Expr>, links: Vec<Link>, then: Option<Stmt>, directed: bool },
    Because { stmt: Stmt, reasons: Vec<Reason> },
    Pooled { stmt: Stmt, reasons: Vec<Reason>, combination: Vec<(AtomRef, Rat)> },
    Theorem { key: TheoremKey, stmt: Stmt, reasons: Vec<Reason> },
    Computation { kind: CompKind /* Ratio | Length | Trig */, terms: Vec<Expr>, links: Vec<Link> },
    Raw { engine_line: String, cites: Vec<FactId> },
}
pub struct Link { pub reasons: Vec<Reason>, pub combination: Vec<(AtomRef, Rat)> }
pub enum Reason { Hyp(Stmt), Claim(u16), Atom { key: AtomKey, stmt: Stmt, args: Vec<PointId> }, Engine(FactId) }
pub enum Stmt { Coll(..), Cyclic(..), Perp(..), Para(..), EqAngle(..), AngleConst(..), Cong(..), EqRatio(..),
                Sim(.., opposite: bool), Congruent(..), OnCircle(..), Tangent(..), RadicalAxis(..), Coincide(..), Formula(..) }
pub enum Expr { Angle { a, b, c, directed: bool }, LineAngle(Line, Line), Const(Rat), Lin(Vec<(Rat, Expr)>),
                Seg(PointId, PointId), Prod(Vec<(Expr, i32)>), Sin(Box<Expr>), Cos(Box<Expr>), Num(Rat) }
```

`AtomKey` and `TheoremKey` are closed enums: `Inscribed | Thales | TangentChord | PerpBisector | Parallel | Radii | Isosceles | CentralAngle | PowerOfPoint | Midline | RadicalAxis | AngleBisectorThm | Intercept | Homothety | Monge | Menelaus | Ceva | LawOfSines | DoubleAngle | …`. A closed enum is what makes I4 enforceable.

### 7.2 JSON (`ag-studio`, serde mirror `human_view.rs`)

`present.rs::build` adds `View.human: Option<HumanView>`, renamed through `Names`. Example (orthocenter reflection, abridged):

```json
{"human": {
  "version": 1, "as_drawn": false,
  "setup": [{"kind": "directed_angles"}],
  "blocks": [
    {"id": 1, "kind": "step", "stmt": {"kind": "coll", "args": ["A", "H", "K"], "points": ["A","H","K"]},
     "body": [{"kind": "because", "stmt": {"kind": "perp", "args": ["HK", "BC"]},
               "reasons": [{"kind": "atom", "key": "perp_bisector", "args": ["B","C","H","K"]}]},
              {"kind": "because", "stmt": {"kind": "coll", "args": ["A","H","K"]},
               "reasons": [{"kind": "hyp", "stmt": {"kind": "perp", "args": ["AH","BC"]}}]}],
     "engine_steps": [5, 6, 7], "points": ["A","H","K","B","C"], "objects": []},
    {"id": 2, "kind": "conclusion", "stmt": {"kind": "cyclic", "args": ["A","B","K","C"]},
     "body": [{"kind": "chain", "directed": true,
               "terms": ["∡KBC", "∡BKH + 90°", "∡AHB + 90°", "∡HAC", "∡KAC"],
               "links": [{"reasons": [{"kind": "hyp", "stmt": {"kind": "perp", "args": ["AH","BC"]}}]},
                         {"reasons": [{"kind": "atom", "key": "isosceles", "args": ["B","K","H"]}]},
                         {"reasons": [{"kind": "hyp", "stmt": {"kind": "perp", "args": ["BH","AC"]}}]},
                         {"reasons": [{"kind": "coll", "args": ["A","H","K"]}]}],
               "then": {"kind": "cyclic", "args": ["A","B","K","C"]}}],
     "engine_steps": [8], "points": ["A","B","C","K","H"], "objects": [{"circle": ["A","B","C"]}]}],
  "metrics": {"raw_steps": 8, "blocks": 2, "claims": 0, "links": 4, "words_en": 58}}}
```

Statements reuse the existing `Fact {kind, args, points, ro}` shape, so `site.js:fact()`, `render.rs:fact_text` and the RO/EN fact templates work unchanged. Expressions are typeset strings (`∡KBC`, `½∠MON`, `sin∠OCB`), which `render.rs:Typeset` already handles. Two id spaces:
- `engine_steps` are **displayed raw step numbers**, after `drop_restatements` renumbering, so the chips point into the "Full derivation" list.
- Engine `FactId`s stay engine-side.

### 7.3 Client and report

**`assets/app.js`.** The proof card gets three tabs: **Proof** (human; default when `view.human` exists) | **Full derivation** (today's steps) | **AI explanation** (existing). Arrow-key behaviour follows the existing `ptab-*` pattern.
- `proofText()` copies the human text, with a trailer "Full derivation: N steps".
- `englishProof()` for the AI tab sends the human proof plus the raw steps.

**`assets/site.js`.** `GS.renderHuman(container, human, hooks)`:
- **Claim headers.** "Claim 1." / "Afirmația 1." in bold, then the statement; "Proof." / "Demonstrație." in italic.
- **Chains.** A chain is one display line with a small reason after each "=", hidden on narrow screens behind a "why" disclosure (convention 3: reasons optional).
- **Pooled sentences.** They carry a "show the computation" button.
- **Citation chips.** Each block carries chips `→ steps 38, 41` (`engine_steps`, using the existing `.cite` look). Clicking a chip switches to Full derivation and flashes the step (existing `_gsClick` logic).
- **Highlighting.** Hover or focus on a block calls `viewer.highlight(points, facts)` with the block's points and objects. Hovering a single link highlights only the atom's points, the extra granularity humans need.
- **Accessibility.** Keyboard navigation reuses the `renderSteps` roving-tabindex pattern. The 48 px touch targets and iPhone CSS invariants (`ag-studio/tests`) are extended to the new elements.

**`src/render.rs`.** The report renders the human proof by default. The export menu gains "Include full derivation", which appends the raw steps as a second section, as today.
- Pagination rules (`report_pages_fit`, `breaks_for`) treat a block as unbreakable when it has ≤ 4 lines.
- A chain wraps at "=" signs (`math_atoms` already treats "=" runs).

**i18n.** New keys `hp.*` in `i18n.rs` (server, report) and `assets/i18n.js` (client), EN and RO:
- `hp.claim`, `hp.proof`, `hp.conclusion`, `hp.so_cyclic`, `hp.so_coll`, `hp.because`, `hp.pooled`, `hp.as_drawn`;
- `hp.reason.<atom>` for every `AtomKey` and `TheoremKey`;
- `hp.setup.directed`, `hp.setup.let`, `hp.end` ("as desired." / "ceea ce trebuia demonstrat.").

The test `every_aux_kind_the_engine_emits_reads_as_words_in_both_languages` gets a sibling: every `AtomKey`/`TheoremKey` variant has a template in all four catalogues.

**MCP (`mcp.rs`).** `solve_geometry` returns the human proof (EN or the requested language) first, then "Full derivation", then the raw steps.

### 7.4 Placement, soundness, time

- The tracked re-solve already runs once per displayed proof (`solve_problem_with_proof`). The writer runs right after it, **inside the worker process** (`worker.rs:execute`, where the view is built). It is under the solve's remaining deadline, capped at 2 s (`Opts.deadline`).
  - Atom generation is incremental by fact id: objects only grow, so per-fact atom sets are prefixes of one list. The prototype regenerates them for every fact.
  - The BFS, DFS and beam stages have node budgets.
  - On deadline, the remaining blocks become fallback blocks.
- Targets: p95 ≤ 300 ms on `imo_ag_30` and ≤ 50 ms on `jgex_ag_231`, max ≤ 2 s. The unoptimised prototype measures 768 ms / 17 ms p95, so the incremental design is required.
- The writer never runs in the aux search or the corpus benchmark, except under `--human`. `ddar --corpus` timings must not move: check with `hyperfine` before/after on 5 IMO rows.
- Soundness surface is zero by construction. The writer reads the trace and outputs text; verdicts come from `engine.rs:reconcile` exactly as now. `tests/soundness.rs` and `--fuzz-false` are unaffected and must stay green.

---

## 8. Metrics

Per proof:
- `raw_steps` (engine), `raw_shown` (after `drop_restatements`);
- `blocks`, `claims`, `sentences`, `chain_links`, `pooled`, `fallback_blocks`;
- `words_en`, `words_ro`;
- `citations`: total reasons shown;
- `human_cost`.

`ddar --corpus … --human-stats out.tsv` writes them; `docs/human-proofs/` goldens record expected values.

Prototype-era numbers to beat, from the goldens:

| golden | raw steps | human blocks | display lines (EN) | words (EN) |
|---|---|---|---|---|
| [orthocenter reflection](human-proofs/orthocenter-reflection.md) | 8 | 2 | 5 | ~65 |
| [Euler line](human-proofs/euler-line.md) | 27 | 5 | 8 | ~105 |
| [orthocenter–vertex distance (trig)](human-proofs/orthocenter-vertex-distance.md) | 42 | 1 | 14 | ~180 |
| [IMO 2004 P1](human-proofs/imo-2004-p1.md) | 47 | 6 | 15 | ~250 |
| [IMO 2023 P2 (shortest proof)](human-proofs/imo-2023-p2.md) | 51 | 5 | 13 | ~235 |

Measured on the golden texts: non-empty display lines, word tokens. For comparison, EC's solutions run 47–170 words for P1/P4 and 150–400 for P2/P5 (§2).

---

## 9. Verification plan

1. **Engine recording is inert.**
   - Corpus run with tracking on and off: identical fact logs. `--verify-warm` and `--bench` 26/26 pass.
   - `hyperfine` on `ddar --corpus-one` for 5 IMO problems, before and after. Tracked mode only adds clones; the PR states the measured overhead.
2. **Certificates.** For every derived fact with an obligation in both corpora, `certify(obligation, engine premises)` must succeed. Target 100 %: with recorded obligations nothing is guessed.
   - Unit tests: exact π offsets (a Thales atom with +½ vs −½ must both certify), pivot forms of collinearity, ratio rows with prime constants, sine variables.
3. **Atom admissibility (negative tests).** For each `AtomKey`, a FALSE instance must not be admitted:
   - Thales with O not the centre;
   - tangent–chord without the ⟂;
   - power of a point with a non-concyclic quadruple;
   - midline with a non-midpoint.
   Each lives in `tests/soundness.rs`, per the house rule that every prover change carries a false-statement test. The writer is not a prover, but its labels are claims.
4. **Faithfulness on the corpora.**
   - Write a human proof for every proof in `imo_ag_30`, `jgex_ag_231` and `examples/**/*.geo`, and run `check.rs`: **zero I1–I7 violations**.
   - Mutation tests prove the checker is not vacuous. Flip one λ, swap one reason, drop one template hypothesis, change one chain term, reorder two blocks: each must be detected.
5. **Coverage.**
   - No panic anywhere.
   - Goal certified on ≥ 99 % of proofs; the remainder fall back to the full derivation with `human: null`.
   - Fallback blocks ≤ 5 % of displayed blocks.
   - Distribution of fallback causes reported per `Reason` kind (the prototype's per-kind counts in §4.2 are the baseline).
6. **Compactness gates** (IMO corpus):
   - median `blocks` ≤ 6, median displayed sentences ≤ 30 % of `raw_shown`;
   - no displayed chain > 8 links (longer becomes pooled);
   - zero displayed self-similarities and zero restated hypotheses (asserted).
7. **Goldens.** `docs/human-proofs/*.md` become `ag-studio/tests/human_goldens.rs`:
   - structure (claim statements and order, engine facts per block) must match exactly;
   - chain terms must match exactly where the golden marks them "exact";
   - pooled reason lists must be subsets of the golden's allowed set and certify;
   - EN text must match after whitespace normalisation; RO likewise for the RO golden.
8. **Determinism.** Run each golden 3× and in parallel: byte-identical JSON.
9. **Time.** §7.4 targets, measured on the corpus with `hyperfine`. The worker's deadline and kill paths are covered by the existing `worker.rs` tests plus one that runs the writer past its deadline.
10. **UI.** Existing `ag-studio/tests` (CSS invariants, WebKit iPhone check) plus:
    - tab switching;
    - chip → full derivation;
    - highlight on hover and focus;
    - RO rendering;
    - PDF/PNG reports with and without the appendix (one-page fit for the orthocenter golden).
11. **Readability judgement protocol.**
    - Blind A/B/C on 12 problems: 6 IMO, 4 JGEX, 2 named theorems. Raters see (A) raw steps, (B) human proof, (C) AI explanation in random order, with problem and figure.
    - Raters: Vlad plus two olympiad-experienced readers. Each scores 1–5 on (i) "I can follow every step", (ii) "reads like an olympiad solution", (iii) compactness; a free-text comment is optional.
    - Pass: B beats A on all three by ≥ 1 point median, and B ≥ C on (i).
    - The checklist "no restated hypothesis / no isosceles similarity / every claim is used later / theorem names correct" is asserted mechanically, not rated.
    - Jev may pre-screen style regressions on a rubric (cheap, batched), but never decides correctness (house rule).

---

## 10. Implementation sequence (tasks and gates)

1. Engine recording (§5.1–5.3) and the inertness tests. **Gate:** §9.1.
2. `cert.rs` with exact offsets; certify every recorded obligation on both corpora. **Gate:** §9.2.
3. `atoms.rs` with all of §6.2 and the negative tests. **Gate:** §9.3.
4. `classify.rs`, `claims.rs`, `chain.rs` (BFS, DFS, beam, pooled, ratio, trig). `check.rs` and mutation tests. **Gate:** §9.4–9.6.
5. `ddar --human` (plain EN text) and `--human-stats`; golden tests. **Gate:** §9.7–9.8.
6. ag-studio DTO, `present.rs`, i18n, `site.js`/`app.js`, `render.rs`, MCP. **Gate:** §9.10.
7. Best-mode HumanCost selection. **Gate:** §9.9 and a `best` run on IMO 2023 P2 within budget.
8. Readability study. **Gate:** §9.11.

Steps 1–5 are engine-only and testable from the CLI. Step 6 needs no engine change. Vlad is needed for the RO wording review (step 6) and as a rater (step 8).

---

## 11. Open questions and recommended answers

1. **Directed or undirected angles by default?** Directed ∡ mod 180° with one convention line. That is configuration-free and is exactly the engine's algebra. Switch to undirected ∠ only for halving links, flagged "as drawn" (§6.6).
2. **May certificates use engine facts outside the proof closure** (verified but not cited by the engine)? Not in v1. It is sound, since every closure fact is verified, and it would shorten chains. But the human proof would then cite facts missing from the "Full derivation" tab. v2: allow it and add those facts to the derivation view.
3. **Should "Full derivation" keep the hypothesis restatements group?** Yes, unchanged. It is the audit trail, and chips point into it.
4. **Keep the AI explanation tab?** Yes, as a third tab. Feed it the human proof (better grounding, fewer hallucinated steps). The deterministic proof is the default.
5. **Name congruences ≅ or similarities ∼ for ratio-1 cases?** ≅ when the certificate's ratio part comes from equal lengths (radii, hypotheses); ∼ otherwise. Both are engine `SimilarTriangles`.
6. **What if the best proof for humans is not the engine's route?** IMO 2004 P1's Compendium solution uses K = AR ∩ BC. None of the engine proofs seen here take that route; the corpus run uses F, the foot from B. Only HumanCost selection among engine proofs (§6.10) is in scope. Adding "intersection of two figure lines" candidates that complete two cyclic quadrilaterals is an aux-search change: separate work with a soundness review. My attempt to state K in `.geo` failed to compile ("no attempt"), so this was **not** measured.
7. **How to write a pooled chase with 15+ atoms** (IMO 2023 P2 claims 2–4)? Collapse by circle (done) and offer "show the computation". If still > 8 distinct reasons, split by searching for an intermediate single-angle equality certified by a prefix of the support. Spec it in `chain.rs` as stage 3b; measure on IMO 2023 P2 and 2019 P6.
8. **Romanian wording** ("Afirmația" vs "Lema", "patrulater inscriptibil" vs "puncte conciclice")? Use "Afirmația k." and "punctele … sunt conciclice", with "patrulaterul … este inscriptibil" for exactly four points. Vlad reviews the RO catalogue once before ship.
9. **Euclidean-prover proofs** (lengths): out of scope for v1. They already have prose and theorem labels (`present.rs:euclid_rule`). v2 applies §6.4–6.5 to their closure sub-steps.
10. **Where should the human proof live in the stored history** (SQLite)? In the stored solution JSON (it is part of `view`). No schema change: the history replay already re-renders from stored JSON.

---

## 12. Prototype: what it does and does not show

**Shown:**
- certificate recovery is exact and fast;
- the human vocabulary replaces isosceles/similarity spam on real proofs;
- exact π offsets are necessary;
- the coverage numbers in §4.2;
- the chains quoted in the goldens.

**Not shown:**
- the BFS stage (§6.6 stage 1);
- tangent–chord through claims (second pass);
- kite, parallel-transport and midline atoms (golden lines using them are marked);
- trig naming beyond the manual mapping in the trig golden;
- any UI.

Reproduce:

```sh
git apply docs/human-proofs/prototype.patch      # on a scratch copy, never on a product branch
export CARGO_TARGET_DIR=/tmp/claude-1000/hp-design-target CARGO_BUILD_JOBS=6
cargo build --release -p alphageometry-rs --bin hproof
HP_HUMAN=1 target/release/hproof --corpus corpus/imo_ag_30.txt translated_imo_2004_p1
HP_HUMAN=1 HP_SUMMARY=1 …                          # one SUMMARY line per problem (§4.2)
HP_VERBOSE=1 HP_CHAINS=1 …                        # engine-premise certificates and chains
```
