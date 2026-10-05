# Golden: reflection of the orthocentre lies on the circumcircle (EN + RO)

Program: `alphageometry-rs/examples/orthocenter_reflection.geo`

```
A B C = triangle
H = orthocenter(A, B, C)
K = reflect(H, line(B, C))
prove cyclic(A, B, C, K)
```

## Engine proof (deployed `ddar --geo … --proof`, 8 steps)

```
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

## Target human proof — EN (exact)

> *∡ denotes directed angles modulo 180°.*
>
> Since K is the reflection of H in BC, we have BK = BH and CK = CH, so BC is the perpendicular bisector of HK and HK ⟂ BC. As AH ⟂ BC as well, K lies on line AH.
>
> Then
>
> ∡KBC = ∡BKH + 90° = ∡AHB + 90° = ∡HAC = ∡KAC
>
> (AH ⟂ BC; BK = BH; BH ⟂ AC; K on AH), so A, B, K, C are concyclic. ∎

## Target human proof — RO (exact)

> *∡ notează unghiuri orientate modulo 180°.*
>
> Cum K este simetricul lui H față de BC, avem BK = BH și CK = CH, deci BC este mediatoarea segmentului HK și HK ⟂ BC. Cum și AH ⟂ BC, punctul K se află pe dreapta AH.
>
> Atunci
>
> ∡KBC = ∡BKH + 90° = ∡AHB + 90° = ∡HAC = ∡KAC
>
> (AH ⟂ BC; BK = BH; BH ⟂ AC; K pe AH), deci punctele A, B, K, C sunt conciclice. ∎

## Mapping (every line → engine facts)

| human text | block | engine steps re-presented | certificate (atoms → source) |
|---|---|---|---|
| "BK = BH and CK = CH" | step 1 | 003, 004 (hypotheses: the definition of the reflection) | quoted as hypotheses, not restated as steps |
| "BC is the perpendicular bisector of HK and HK ⟂ BC" | step 1 | 005 (△BCH ≅ △BCK), 006 (BH = BK, isosceles) | atom `PerpBisector(B, C; H, K)`: hypotheses BH = BK, CH = CK (003, 004); row dir(BC) − dir(HK) = 90° is **in the span** of 005 and 006 (prototype certificate of 007: AH ⟂ BC + ½·[005] + ½·[006]) |
| "K lies on line AH" | step 1 | 007 | perp(AH, BC) [001] + PerpBisector row; collinear with pivot H |
| ∡KBC = ∡BKH + 90° | conclusion | 008 | hyp 001 (AH ⟂ BC) with line KH = line AH (007) |
| = ∡AHB + 90° | conclusion | 008 | atom `Isosceles(B; K, H)` from 003 |
| = ∡HAC | conclusion | 008 | hyp 002 (BH ⟂ AC) |
| = ∡KAC | conclusion | 008 | line AH = line AK (007), silent quotient |
| "so A, B, K, C are concyclic" | conclusion | 008 = goal | converse of the inscribed angle theorem, unnamed (§2 convention 4) |

Silent: 005 and 006 are never shown as similarities (self-similarity and SSS congruence; §6.3).

## Evidence (prototype, `HP_HUMAN=1 hproof --geo …orthocenter_reflection.geo`)

```
007 collinear coll HAK  (engine cites 3)
   uses 3 atoms: [assumption] hyp perp A H B C | isosceles BH=BK | [similar] △BCH∼△BCK
    ∡BAH
  = ∡ABC + 90°                         [[assumption] hyp perp A H B C]
  = −d(AB) + 1/2·d(BH) + 1/2·d(BK)     [[similar] △BCH∼△BCK ×-1/2]
  = ∡BAH                               [isosceles BH=BK ×-1/2]
008 concyclic cyclic KCBA  (engine cites 4)
   uses 3 atoms: [assumption] hyp perp A H B C | [assumption] hyp perp B H A C | isosceles BH=BK
    ∡KBC
  = ∡BKH + 90°                         [[assumption] hyp perp A H B C]
  = ∡AHB + 90°                         [isosceles BH=BK]
  = ∡HAC                               [[assumption] hyp perp B H A C]
  (PURE chain; reaches rhs ∡HAC: true)
```

The ½·[005] + ½·[006] pair in 007 is exactly the row "BC ⟂ HK". The perpendicular-bisector atom (not in the prototype) replaces it with one honest reason.

## Acceptance

- The structure (one unnumbered step, then the conclusion), the statements and the engine steps per block match exactly.
- The chain terms match exactly.
- EN and RO text match after whitespace normalisation.
- Metrics: raw 8 steps → 2 blocks, 5 display lines, about 65 words (EN; RO about 65).

## Produced by the implementation (engine writer, branch wf/human-proofs-engine)

Pinned verbatim in `alphageometry-rs/tests/golden/human/orthocenter_reflection.en.txt` (test `golden_examples_render_as_recorded`; `golden_examples_have_the_planned_shape` pins the structure). Every block passed the independent checker in strict mode.

```
∡ denotes directed angles modulo 180°.

∡AHK = 0° (AH ∥ HK, both ⟂ BC), so H, A, K are collinear.
∡ACB = ∡HBC + 90° = ∡BHK = ∡HKB = ∡AKB (BH ⟂ AC; BC is the perpendicular bisector of HK; BH = BK; K on HA), so A, B, C, K are concyclic. ∎
```

Deviations from the target (all equivalent; none changes what is proved):
- Two blocks, no claims, as planned. The first block states the collinearity as a one-link chain `∡AHK = 0° (AH ∥ HK, both ⟂ BC)`; the target spells out BK = BH, CK = CH and the perpendicular bisector first. The writer cites the Parallel atom (two lines ⟂ BC) directly.
- The conclusion chain runs from ∡ACB to ∡AKB instead of ∡KBC … ∡KAC. Both are inscribed-angle forms of the same concyclicity; the writer takes the cheapest form whose certificate is a pure single-angle chain. Same four links with the same reasons (BH ⟂ AC; BC the perpendicular bisector of HK; BH = BK; K on AH).
- The point order in "A, B, C, K are concyclic" follows the engine goal.
- The RO text is rendered by ag-studio from the structured blocks, not by the engine.
