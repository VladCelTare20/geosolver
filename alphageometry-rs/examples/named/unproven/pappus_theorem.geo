# Pappus's hexagon theorem — with A, B, C on one line and D, E, F on another, the cross intersections AE·BD, AF·CD, BF·CE are collinear (source: Pappus of Alexandria)
A B = segment
C = on_line(A, B)
D E = segment
F = on_line(D, E)
X = meet(line(A, E), line(B, D))
Y = meet(line(A, F), line(C, D))
Z = meet(line(B, F), line(C, E))
prove coll(X, Y, Z)
