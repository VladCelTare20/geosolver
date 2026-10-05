# Human proof writer — golden examples

Acceptance tests for the writer specified in [`../HUMAN_PROOFS.md`](../HUMAN_PROOFS.md). Each file holds:
1. the program;
2. the engine proof as the deployed CLI prints it;
3. the exact target text;
4. a mapping from every displayed line to engine steps and certificate atoms;
5. prototype evidence;
6. the acceptance rules for that example.

| golden | what it pins | raw → blocks |
|---|---|---|
| [orthocenter-reflection.md](orthocenter-reflection.md) | a simple named theorem; **EN + RO** text; similarity steps that become "perpendicular bisector" and "isosceles" | 8 → 2 |
| [euler-line.md](euler-line.md) | **key similarities kept** as claims; pruning of facts the human certificate does not need | 27 → 5 |
| [orthocenter-vertex-distance.md](orthocenter-vertex-distance.md) | **law of sines**: a 27-link product chain grouped by triangle; `sin(x + 90°)` shown as `cos x` | 42 → 1 |
| [imo-2004-p1.md](imo-2004-p1.md) | **auxiliary point** (foot F); Thales and power of a point replacing similarity spam; radical axis; a halving chain shown "as drawn"; pooled finale | 47 → 6 |
| [imo-2023-p2.md](imo-2023-p2.md) | the **shortest proof** from `agstudio best`; tangent–chord; a claim (XA = XP) recovered from congruence ratio rows; pooled chases with collapsed reasons | 51 → 5 |

[`prototype.patch`](prototype.patch) is the measurement prototype (row recording in `ElimCore`, the `hproof` binary). It is evidence for §4 of the spec, not code to merge: apply it only to a scratch copy.
