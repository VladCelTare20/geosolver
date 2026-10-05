# Golden: AH = 2·OMₐ — a trigonometric (law of sines) proof

Program: `alphageometry-rs/examples/named/orthocenter_vertex_distance.geo`

```
A B C = triangle
O = circumcenter(A, B, C)
H = orthocenter(A, B, C)
Ma = midpoint(B, C)
prove dist(A, H) = 2 * dist(O, Ma)
```

DDAR proves this length goal through the trig fallback (`--trig fallback`: law-of-sines rows enter the closure).

## Engine proof (deployed `ddar --geo … --proof`, 42 steps)

```
001. assumption: cong O A O B
002. assumption: cong O B O C
003. assumption: perp A H B C
004. assumption: perp B H A C
005. assumption: coll B C Ma
006. collinear: B Ma C [005]
007. assumption: cong Ma B Ma C
008. similar triangles: △AOB ∼ △BOA [001]
009. similar triangles: △ACO ∼ △CAO [001 & 002 & 004]
010. similar triangles: △BCO ∼ △CBO [002 & 004 & 009]
011. similar triangles: △BOMa ∼ △COMa [002 & 004 & 006 & 007 & 009 & 010]
012. segment arithmetic: |BMa| ↔ |CMa| (add/mul transfer) [007]
013. segment arithmetic: |BC| ↔ |BMa| (add/mul transfer) [006 & 007 & 012]
014. perpendicular ⇒ squared lengths (Pythagoras): A C B H [004]
015. perpendicular ⇒ squared lengths (Pythagoras): A H B C [003]
016. perpendicular from squared lengths: A B C H [014 & 015]
017. equal or supplementary angles have equal sines: |sin(∠(OB,OC))| = |sin ∠(OC,OB)| [004 & 009 & 010]
018. equal or supplementary angles have equal sines: |sin(∠(AB,AC))| = |sin ∠(AC,AB)| [004 & 008 & 009 & 010]
019. equal or supplementary angles have equal sines: |sin(∠(AB,AC) + 90°)| = |sin ∠(BA,BH)| [004 & 008 & 009 & 010]
020. double-angle formula: x = ∠(AB,AC), 2x ≡ ±∠(OC,OB): |sin x|·|sin(x + 90°)| = |sin 2x| / 2
021. equal or supplementary angles have equal sines: |sin(∠(OA,OC))| = |sin ∠(OC,OA)| [004 & 009]
022. equal or supplementary angles have equal sines: |sin(∠(AB,AH))| = |sin ∠(AH,AB)| [003 & 004 & 008 & 009 & 010]
023. equal or supplementary angles have equal sines: |sin(∠(AB,AH) + 90°)| = |sin ∠(BA,BC)| [004 & 008 & 009 & 010]
024. double-angle formula: x = ∠(AB,AH), 2x ≡ ±∠(OC,OA): |sin x|·|sin(x + 90°)| = |sin 2x| / 2
025. equal or supplementary angles have equal sines: |sin(∠(OB,OA))| = |sin ∠(OB,OA)| [004 & 009 & 010]
026. equal or supplementary angles have equal sines: |sin(∠(CB,CA))| = |sin ∠(CB,CA)| [004]
027. equal or supplementary angles have equal sines: |sin(∠(CB,CA) + 90°)| = |sin ∠(AO,AB)| [004 & 008 & 009 & 010]
028. double-angle formula: x = ∠(CB,CA), 2x ≡ ±∠(OB,OA): |sin x|·|sin(x + 90°)| = |sin 2x| / 2
029. law of sines: in △ABC, BC / |sin ∠(AC,AB)| = AC / |sin ∠(BA,BC)|
030. law of sines: in △ABC, CA / |sin ∠(BA,BC)| = BA / |sin ∠(CB,CA)|
031. law of sines: in △ABO, BO / |sin ∠(AO,AB)| = AO / |sin ∠(BA,BO)|
032. law of sines: in △ABO, OA / |sin ∠(BA,BO)| = BA / |sin ∠(OB,OA)|
033. law of sines: in △ABH, BH / |sin ∠(AH,AB)| = AH / |sin ∠(BA,BH)|
034. law of sines: in △ACO, OA / |sin ∠(CA,CO)| = CA / |sin ∠(OC,OA)|
035. law of sines: in △BCO, OB / |sin ∠(CB,CO)| = CB / |sin ∠(OC,OB)|
036. law of sines: in △BCH, HB / |sin ∠(CB,CH)| = CB / |sin ∠(HC,HB)|
037. law of sines: in △BOMa, OMa / |sin ∠(BMa,BO)| = BMa / |sin ∠(OB,OMa)|
038. law of sines: in △BOMa, MaB / |sin ∠(OB,OMa)| = OB / |sin ∠(MaO,MaB)|
039. sine of 90°: |sin ∠(MaO,MaB)| = 1 [004 & 006 & 009 & 010 & 011]
040. equal or supplementary angles have equal sines: |sin ∠(AC,AB)| = |sin ∠(HC,HB)| [004 & 008 & 009 & 010 & 016]
041. equal or supplementary angles have equal sines: |sin ∠(CA,CO)| = |sin ∠(CB,CH)| [004 & 008 & 009 & 010 & 016]
042. equal or supplementary angles have equal sines: |sin ∠(CB,CO)| = |sin ∠(BMa,BO)| [004 & 006 & 009 & 010]
∎ rconst A H Ma O 2
```

## Target human proof — EN (exact)

> Write R = OA = OB = OC. All angles below are angles of the figure's triangles, so their sines are positive.
>
> **Claim.** AH = 2·OMₐ.
>
> *Proof.*
>
> | | | reason |
> |---|---|---|
> | AH / (2·OMₐ) | | |
> | | = AH / (2R·sin∠OCB) | law of sines in △BOMₐ, where ∠OMₐB = 90°; ∠OBC = ∠OCB |
> | | = AH·BC·sin∠ACB / (R·AB·sin∠BOC) | law of sines in △AOB and △BOC; ∠AOB = 2∠ACB, so sin∠AOB = 2 sin∠ACB·cos∠ACB; cos∠ACB = sin∠OAB |
> | | = AH·BC·sin∠ABC / (R·AC·sin∠BOC) | law of sines in △ABC |
> | | = AH·AC·sin²∠BAC / (R·BC·sin∠BOC·cos∠BAH) | law of sines in △ABC; cos∠BAH = sin∠ABC (AH ⟂ BC) |
> | | = 2·AH·sin²∠BAC·sin∠BAH / (BC·sin∠BOC·sin∠ACO) | law of sines in △AOC; ∠AOC = 2∠BAH, so sin∠AOC = 2 sin∠BAH·cos∠BAH |
> | | = AH·sin∠BAC·sin∠BAH / (BC·sin∠ABH·sin∠ACO) | ∠BOC = 2∠BAC, so sin∠BOC = 2 sin∠BAC·cos∠BAC; cos∠BAC = sin∠ABH (BH ⟂ AC) |
> | | = BH·sin∠BAC / (BC·sin∠ACO) | law of sines in △ABH |
> | | = sin∠BAC·sin∠BCH / (sin∠BHC·sin∠ACO) | law of sines in △BCH |
> | | = 1 | ∠BHC = 180° − ∠BAC and ∠BCH = 90° − ∠ABC = ∠ACO, since CH ⟂ AB (the altitudes concur) |
>
> ∎

In the app each row is one display line with its reason in the margin. On narrow screens the reasons move under a "why" disclosure (§7.3).

## Mapping (displayed equality → engine rows, in chain order)

The prototype recovered this exact 27-link product chain for the goal certificate. Each displayed equality is the product of the consecutive links listed, with `p15 = log 2` and `sNN` the sine variables.

| # | displayed equality | engine rows (links) | sine names used (from the engine's own row text) |
|---|---|---|---|
| 1 | = AH/(2R·sin∠OCB) | 039, 001, 038, 037, 042 | s17 = sin∠OMₐB = 1, s60 = sin∠BOMₐ, s59 = sin∠OBMₐ, s55 = sin∠OCB |
| 2 | = AH·BC·sin∠ACB/(R·AB·sin∠BOC) | 032, 025, 028, 026, 027, 031, 035, 017 | s32/s31 = sin∠AOB, s33/s34 = sin∠ACB, s35 = cos∠ACB = s36 = sin∠OAB, s37 = sin∠ABO, s20/s19 = sin∠BOC |
| 3 | = AH·BC·sin∠ABC/(R·AC·sin∠BOC) | 030 | s30 = sin∠ABC |
| 4 | = AH·AC·sin²∠BAC/(R·BC·sin∠BOC·cos∠BAH) | 023, 029 (×2) | s29 = cos∠BAH, s22 = sin∠BAC |
| 5 | = 2·AH·sin²∠BAC·sin∠BAH/(BC·sin∠BOC·sin∠ACO) | 034, 021, 024 | s26/s25 = sin∠AOC, s42 = sin∠ACO, s27 = sin∠BAH |
| 6 | = AH·sin∠BAC·sin∠BAH/(BC·sin∠ABH·sin∠ACO) | 020, 018, 019, 022 | s21 = sin∠BAC, s23 = cos∠BAC = s24 = sin∠ABH, s28 = sin∠BAH |
| 7 | = BH·sin∠BAC/(BC·sin∠ACO) | 033 | |
| 8 | = sin∠BAC·sin∠BCH/(sin∠BHC·sin∠ACO) | 036 | s58 = sin∠BHC, s57 = sin∠BCH |
| 9 | = 1 | 040, 041 | |

- **Silent:** 008–010 (isosceles OA = OB = OC); 011 (△BOMₐ ≅ △COMₐ, only feeding "∠OMₐB = 90°").
- **Inline reason:** 014–016. The engine derives the third altitude CH ⟂ AB from AH ⟂ BC and BH ⟂ AC through squared lengths (Pythagoras twice). The writer shows it as "CH ⟂ AB (the altitudes concur)". That is an `Orthocentre` atom: its hypotheses are 003 and 004, and its row is the row of 016. 040 and 041 need it: their angle obligations, ∠BHC + ∠BAC = 180° and ∠BCH = ∠ACO, are certified from 003, 004, 008–010 and 016.
- **Pruned:** 012, 013 (segment transfers; no certificate uses them).

Rendering rules exercised here (§6.6.4):
- `|sin(x + 90°)|` → `cos x`;
- class/corner equal-sines rows (017, 018, 021, 022, 025, 026) are silent;
- consecutive links in the same triangles are grouped into one row.

## Evidence (prototype, `HP_CHAINS=1 hproof --geo …orthocenter_vertex_distance.geo`)

```
GOAL rconst A H Ma O 2 engine-deps=29 certified=true minimal=27
    AH / OMA·p15
  = AH / OMA·p15·p17    [sine of 90°: p17 / 1]
  = AH·BO / AO·OMA·p15·p17    [hyp cong O A O B: AO / BO]
  = AH·BMA / AO·OMA·p15·p60    [law of sines: BO·p60 / BMA·p17]
  = AH / AO·p15·p59    [law of sines: BMA·p59 / OMA·p60]
  = AH / AO·p15·p55    [equal or supplementary angles have equal sines: p55 / p59]
  …                                    (27 links; all coefficients ±1)
  = p57 / p42    [equal or supplementary angles have equal sines: p22 / p58]
  = 1 / 1    [equal or supplementary angles have equal sines: p57 / p42]
  (target rhs 1 / 1; chain reaches it: true)
```

The sine-variable names come from `HP_TRIGMAP=1`, which prints each trig row's text with its ratio-table row (e.g. `TRIG 039 sine of 90°: |sin ∠(MaO,MaB)| = 1  ROWS p17 / 1`).

## Acceptance

- One claim; the computation has exactly these 10 terms in this order.
- Every row's reason list names the triangles and identities shown.
- EN text matches after whitespace normalisation.
- 012 and 013 appear in no block; 016 appears only as the inline reason "CH ⟂ AB".
- Metrics: raw 42 → 1 block, 14 display lines (with the table header), about 180 word tokens.
