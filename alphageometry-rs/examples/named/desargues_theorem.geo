# Desargues's theorem — two triangles perspective from a point are perspective from a line: the intersections of corresponding sides are collinear (source: Desargues; Coxeter & Greitzer)
O = free
A B C = triangle
A2 = on_line(O, A)
B2 = on_line(O, B)
C2 = on_line(O, C)
X = meet(line(A, B), line(A2, B2))
Y = meet(line(B, C), line(B2, C2))
Z = meet(line(C, A), line(C2, A2))
prove coll(X, Y, Z)
