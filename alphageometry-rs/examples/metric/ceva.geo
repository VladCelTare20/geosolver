# Ceva's theorem, derived from Menelaus applied to the two sub-triangles ABD and
# ACD: three concurrent cevians give BD*CE*AF = DC*EA*FB.
A = free
B = free
C = free
P = on_line(A, midpoint(B,C))
D = meet(line(A, P), line(B, C))
E = meet(line(B, P), line(C, A))
F = meet(line(C, P), line(A, B))
prove dist(B,D)*dist(C,E)*dist(A,F) = dist(D,C)*dist(E,A)*dist(F,B)
