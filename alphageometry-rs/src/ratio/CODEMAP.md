# alphageometry-rs/src/ratio — child modules of `ratio.rs` (they reach `Figure`'s private items)

Up: [../CODEMAP.md](../CODEMAP.md)

## Files
- `trig.rs` — sine atoms and the certified log engine (stage S2), cofactor substitution (T2), the centred-circle second-meet aux family (T5).

## Notes
- `trig.rs:centred_second_meets` — appended after every older aux family (existing candidate order unchanged); facts `Coll(v,r,K)`, `Cong(o,a,o,K)`. Only circles with exactly three points not already in a `concyclic` set.
- `trig.rs:sin_atom` — `LKey::Sin(v,p,q)` is `ln|sin∠pvq|`, created only when the corner is proper in every instance (angle in (1e-4, π−1e-4), |sin| > 1e-6). Rows: law of sines (two per triangle, headline, no premises), equal sines (pending `eqangle` in either orientation — directed equality mod π gives equal |sin|), known sines 30/45/60/90/120/135/150 with exact half-integer prime exponents (pending `aconst d` or `aconst 180−d`, `perp` for 90), extended law of sines (pending both radii `cong`), derived congruences (pending `cong`). Helper points named `_k` are excluded from the triangles.
- `trig.rs:certified_prove` — the `LEq` counterpart of `certified_products`: rows with `alts` need one alternative derived by DDAR (≤ 256 checks per solve), else rejected and the solve retried; greedy minimisation; a proof made of one headline plus only `support` rows (congruences, equal/known sines) is rejected as a restatement; one lead step prints the derivation. `prune_private_atoms` first drops rows holding an atom nothing else mentions (exact for a fixed goal).
- `trig.rs:gather_cofactor_substitutions` (T2) — for a pool monomial `m` holding a segment equal (in every instance) to another, the support row `m − m[s1→s2]` behind a pending `cong`; fixpoint under 512 monomials / 4096 rows.

## Subfolders
- [trig/](trig/CODEMAP.md) — product-side trig stages (S5–S8), rendering, tests.
