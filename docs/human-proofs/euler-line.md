# Golden: Euler line — the case where humans *do* keep the similarities

Program: `alphageometry-rs/examples/euler_line.geo`

```
A B C = triangle
O = circumcenter(A, B, C)
G = centroid(A, B, C)
H = orthocenter(A, B, C)
prove coll(O, G, H)
```

`centroid` expands to the helper midpoints `_5` (of BC) and `_6` (of CA). `present.rs:Names` shows them as **M** and **N**.

## Engine proof (deployed `ddar --geo … --proof`, 27 steps)

```
001. assumption: cong O A O B
002. assumption: cong O B O C
003. assumption: coll B C _5
004. collinear: B _5 C [003]
005. assumption: cong _5 B _5 C
006. assumption: coll A C _6
007. collinear: A _6 C [006]
008. assumption: cong _6 A _6 C
009. assumption: coll A _5 G
010. collinear: A G _5 [009]
011. assumption: coll B _6 G
012. collinear: B G _6 [011]
013. assumption: perp A H B C
014. assumption: perp B H A C
015. similar triangles: △AOB ∼ △BOA [001]
016. similar triangles: △ACO ∼ △CAO [001 & 002 & 007]
017. similar triangles: △BCO ∼ △CBO [002 & 004 & 007 & 016]
018. similar triangles: △BO_5 ∼ △CO_5 [002 & 004 & 005 & 007 & 016 & 017]
019. similar triangles: △AO_6 ∼ △CO_6 [001 & 002 & 007 & 008 & 016]
020. concyclic (inscribed angles): _6 _5 O C [004 & 007 & 016 & 017 & 018 & 019]
021. segment arithmetic: |A_6| ↔ |C_6| (add/mul transfer) [008]
022. similar triangles: △CBA ∼ △C_5_6 [004 & 007 & 015 & 016 & 017 & 018 & 020]
023. similar triangles: △AHB ∼ △_5O_6 [004 & 007 & 013 & 014 & 015 & 016 & 017 & 018 & 019 & 020 & 022]
024. similar triangles: △ABG ∼ △_5_6G [004 & 007 & 010 & 012 & 015 & 016 & 017 & 018 & 020 & 022]
025. segment arithmetic: |AC| ↔ |A_6| (add/mul transfer) [007 & 008 & 021 & 022]
026. similar triangles: △AHG ∼ △_5OG [004 & 007 & 010 & 013 & 016 & 017 & 018 & 022 & 023 & 024 & 025]
027. collinear: O G H [004 & 007 & 013 & 016 & 017 & 018 & 026]
∎ coll O G H
```

## Target human proof — EN (exact)

> *∡ denotes directed angles modulo 180°.*
>
> Let M and N be the midpoints of BC and CA, so that G = AM ∩ BN.
>
> Since OB = OC and MB = MC, we have OM ⟂ BC; likewise ON ⟂ CA. By the midline theorem, MN ∥ AB.
>
> **Claim 1.** △AHB ∼ △MON.
>
> *Proof.* The triangles have parallel sides: AH ∥ MO (both ⟂ BC), HB ∥ ON (both ⟂ CA) and AB ∥ MN.
>
> **Claim 2.** △ABG ∼ △MNG.
>
> *Proof.* AB ∥ MN and G = AM ∩ BN.
>
> By Claims 1 and 2, AH : MO = AB : MN = AG : MG, and ∡HAG = ∡OMG because AH ∥ MO. Hence △AHG ∼ △MOG, so ∡AGH = ∡MGO. As A, G, M are collinear, so are H, G, O. ∎

## Mapping

| human text | block | engine steps | certificate |
|---|---|---|---|
| "Let M and N be the midpoints …, G = AM ∩ BN" | setup | 003–012 (hypotheses of `centroid`) | helper points named by `Names`; not restated |
| "OM ⟂ BC; likewise ON ⟂ CA" | step | 018, 019 (△BOM ≅ △COM, △AON ≅ △CON) | atom `PerpBisector(O, M; B, C)`: OB = OC (002), MB = MC (005). Its row is in the span of 017 + 018 (prototype 026 certificate: `isosceles OB=OC ×−½`, `△BO_5∼△CO_5 ×½`). Likewise ON ⟂ CA from 019 |
| "MN ∥ AB (midline)" | step | 022 (△CBA ∼ △CMN) | atom `Midline(M, N; △ABC)`: hypotheses M, N midpoints (003–008); row MN ∥ AB is a row of 022 |
| Claim 1 | claim | 023 | AA: {AH ∥ MO, AB ∥ MN} and {AH ∥ MO, HB ∥ ON}. `Parallel` atoms: AH ⟂ BC (013) with OM ⟂ BC; HB ⟂ CA (014) with ON ⟂ CA |
| Claim 2 | claim | 024 | AA from AB ∥ MN (022) alone; the vertical angle at G is trivial. Prototype: `uses 1 atoms: [similar] △CBA∼△C_5_6` |
| "AH : MO = AB : MN = AG : MG" | conclusion | 026 (ratio part) | prototype ratio certificate: `[similar] △AHB∼△_5O_6 \| [similar] △ABG∼△_5_6G` |
| "∡HAG = ∡OMG because AH ∥ MO" | conclusion | 026 (angle part) | `Parallel(AH, MO)` + line AG = line MG (010) |
| "△AHG ∼ △MOG, so ∡AGH = ∡MGO … collinear" | conclusion | 026, 027 = goal | prototype 027: `uses 2 atoms: [similar] △AHG∼△_5OG` |

- **Silent:** 015–017 (OA = OB = OC: self-similar isosceles).
- **Pruned** (not needed by any human certificate): 020 (C, O, M, N concyclic; the engine's route to 022/023), 021 and 025 (segment transfers; the claims need only the ratio *equality* AB : MN, not its value 2).

This golden shows §6.3 rows 7 and 9. 023 and 024 are each used twice and involve the key configuration, so they stay **displayed** similarities, as in every human Euler-line proof.

## Evidence (prototype)

```
023 similar △AHB∼△_5O_6  (engine cites 11)   via AA
   uses 3 atoms: [assumption] hyp perp A H B C | [assumption] hyp perp B H A C | cyclic CO_5_6
024 similar △ABG∼△_5_6G  (engine cites 10)   via AA
   uses 1 atoms: [similar] △CBA∼△C_5_6   (PURE chain)
026 similar △AHG∼△_5OG  (engine cites 11)   via SAS@1
   uses 3 atoms: [assumption] hyp perp A H B C | isosceles OB=OC | [similar] △BO_5∼△CO_5     (angle)
   uses 2 atoms: [similar] △AHB∼△_5O_6 | [similar] △ABG∼△_5_6G                              (ratio)
027 collinear coll OGH  (engine cites 7)       via dir
   uses 2 atoms: [similar] △AHG∼△_5OG
```

The prototype certifies 023 through the engine's own route (circle C, O, M, N). The golden requires the `Parallel` and `Midline` atoms of §6.2, which the prototype does not implement. With them, 023 is certified by the parallel-sides atoms, which have lower cost. That is the second certification pass of §6.5, and it lets the pruning drop 020.

## Acceptance

- Blocks (setup; 2 steps; Claims 1, 2; conclusion), statements and engine steps per block match exactly.
- EN text matches after whitespace normalisation.
- 020, 021 and 025 must not appear in any block.
- Metrics: raw 27 → 5 blocks, 8 display lines, about 105 words.

## Produced by the implementation (engine writer, branch wf/human-proofs-engine)

Pinned verbatim in `alphageometry-rs/tests/golden/human/euler_line.en.txt` (test `golden_examples_render_as_recorded`; `golden_examples_have_the_planned_shape` pins the structure). Every block passed the independent checker in strict mode.

```
∡ denotes directed angles modulo 180°.
Let M be the midpoint of BC.
Let N be the midpoint of AC.

Claim 1. △AHB ∼ △MON.
Proof. ∡HAB = ∡(AH, MN) = ∡OMN (midline MN ∥ BA; AH ∥ OM, both ⟂ BC). ∡ABH = ∡(MN, BH) = ∡MNO (midline MN ∥ BA; BH ∥ ON, both ⟂ AC). Hence △AHB ∼ △MON.
Claim 2. △ABG ∼ △MNG.
Proof. ∡BAG = ∡NMG (midline MN ∥ BA). Hence △ABG ∼ △MNG.
∡HAG = ∡OMG (AH ∥ OM, both ⟂ BC). From Claim 1 and Claim 2, AH : AG = MO : MG. Hence △AHG ∼ △MOG. ∡OGH = ∡(OM, AH) = ∡OMB + 90° = 0° (△AHG ∼ △MOG; AH ⟂ BC; OM is the perpendicular bisector of BC), so O, G, H are collinear. ∎
```

Deviations from the target:
- The setup introduces M and N as midpoints (the program's helpers); "so that G = AM ∩ BN" is not stated (G is a hypothesis point).
- The opening sentence (OM ⟂ BC, ON ⟂ CA, MN ∥ AB) is not a separate block: those facts are silent registrations or atoms, cited inline where used ("OM is the perpendicular bisector of BC", "midline MN ∥ BA").
- Claim 1 is proved by two angle chains (AA) rather than by "parallel sides"; each chain is checked link by link.
- Claim 2 cites only the midline (the angle at G is between the same two lines on both sides, so it costs nothing); the target also cites G = AM ∩ BN.
- The finale proves △AHG ∼ △MOG (SAS from Claims 1 and 2) and then the collinearity as one chain ending in `0°`, instead of "∡AGH = ∡MGO … so are H, G, O".
