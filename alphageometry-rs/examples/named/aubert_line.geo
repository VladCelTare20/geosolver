# Aubert line (Steiner line of a complete quadrilateral) — the orthocenters of the four triangles of a complete quadrilateral are collinear (source: Aubert/Steiner; Johnson)
A B C = triangle
D = free
E = meet(line(A, B), line(C, D))
F = meet(line(A, D), line(B, C))
H1 = orthocenter(E, A, D)
H2 = orthocenter(E, B, C)
H3 = orthocenter(A, B, F)
prove coll(H1, H2, H3)
