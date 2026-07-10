# Isogonic (Fermat) point angles — at the first isogonic point the three vertices are seen under equal (directed) angles (source: Torricelli; Johnson)
A B C = triangle
A2 = eq_triangle(B, C)
B2 = eq_triangle(C, A)
X = meet(line(A, A2), line(B, B2))
prove eqangle(X, A, X, B, X, B, X, C)
