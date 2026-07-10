# Napoleon's theorem — the centers of equilateral triangles erected on the sides of any triangle form an equilateral triangle (source: attributed to Napoleon; Coxeter & Greitzer)
A B C = triangle
A2 = eq_triangle(B, C)
B2 = eq_triangle(C, A)
C2 = eq_triangle(A, B)
N1 = centroid(B, C, A2)
N2 = centroid(C, A, B2)
N3 = centroid(A, B, C2)
prove cong(N1, N2, N2, N3)
