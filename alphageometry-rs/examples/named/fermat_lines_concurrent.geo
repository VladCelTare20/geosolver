# Fermat–Torricelli concurrency — the lines joining each vertex to the apex of the equilateral triangle on the opposite side are concurrent (source: Torricelli; Coxeter & Greitzer)
A B C = triangle
A2 = eq_triangle(B, C)
B2 = eq_triangle(C, A)
C2 = eq_triangle(A, B)
X = meet(line(A, A2), line(B, B2))
prove coll(C, X, C2)
