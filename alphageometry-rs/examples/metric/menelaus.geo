# Menelaus's theorem, derived (drop perpendiculars to the transversal, three
# pairs of similar right triangles): a transversal DEF of triangle ABC gives
# BD*CE*AF = DC*EA*FB.
A = free
B = free
C = free
F = on_line(A, B)
E = on_line(A, C)
D = meet(line(E, F), line(B, C))
prove dist(B,D)*dist(C,E)*dist(A,F) = dist(D,C)*dist(E,A)*dist(F,B)
