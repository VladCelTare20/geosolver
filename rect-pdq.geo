# Rectangle ABCD, E on CD, F on AD arbitrary.
# Perp from E to line FB meets BC at P; perp from F to line EB meets AB at Q.
# Show P, D, Q collinear.
A B = segment
D = on_tline(A, A, B)
C = shift(D, A, B)
E = on_line(C, D)
F = on_line(A, D)
P = meet(perp_line(E, line(F, B)), line(B, C))
Q = meet(perp_line(F, line(E, B)), line(A, B))
prove coll(P, D, Q)
