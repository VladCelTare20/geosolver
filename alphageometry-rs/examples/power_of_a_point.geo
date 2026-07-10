# THEOREM (power of a point): for two secants of a circle meeting at P,
# |PA|*|PB| = |PC|*|PD|.
A B C = triangle
O = circumcenter(A, B, C)
D = point: on(D, circle(O, A))
P = meet(line(A, B), line(C, D))
prove dist(P, A) * dist(P, B) = dist(P, C) * dist(P, D)
