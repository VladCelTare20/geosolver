# Torricelli circles — the circumcircles of the three outer equilateral triangles pass through a common point (source: Torricelli; cut-the-knot)
A B C = triangle
A2 = eq_triangle(B, C)
B2 = eq_triangle(C, A)
C2 = eq_triangle(A, B)
X = meet(circumcircle(B, C, A2), circumcircle(C, A, B2))
prove cyclic(A, B, C2, X)
