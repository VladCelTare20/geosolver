# Pascal's theorem (for a circle) — the three intersections of opposite sides of a hexagon inscribed in a circle are collinear (source: Pascal; Coxeter & Greitzer, Geometry Revisited)
A B C = triangle
D = on_circum(A, B, C)
E = on_circum(A, B, C)
F = on_circum(A, B, C)
X = meet(line(A, B), line(D, E))
Y = meet(line(B, C), line(E, F))
Z = meet(line(C, D), line(F, A))
prove coll(X, Y, Z)
