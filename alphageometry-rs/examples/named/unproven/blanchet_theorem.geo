# Blanchet's theorem — the altitude foot from A sees the cevian intersections of any point on the altitude under equal angles with the altitude (source: Blanchet; cut-the-knot)
A B C = triangle
Fa = foot(A, line(B, C))
P = midpoint(A, Fa)
E = meet(line(B, P), line(C, A))
F = meet(line(C, P), line(A, B))
prove eqangle(Fa, F, Fa, A, Fa, A, Fa, E)
