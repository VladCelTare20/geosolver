# Steiner line — the reflections of a circumcircle point over two sides are collinear with the orthocenter (source: Johnson, Advanced Euclidean Geometry)
A B C = triangle
H = orthocenter(A, B, C)
P = on_circum(A, B, C)
R1 = reflect(P, line(B, C))
R2 = reflect(P, line(C, A))
prove coll(R1, R2, H)
