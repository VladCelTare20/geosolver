# Reim's theorem — two circles meet at P and Q; lines through P and Q cut the circles at A, B and C, D; then AC is parallel to BD (source: Evan Chen, EGMO)
O1 O2 = segment
P = free
Q = meet(circle(O1, P), circle(O2, P))
A = on_circle(O1, P)
B = meet(line(A, P), circle(O2, P))
C = on_circle(O1, P)
D = meet(line(C, Q), circle(O2, P))
prove para(A, C, B, D)
