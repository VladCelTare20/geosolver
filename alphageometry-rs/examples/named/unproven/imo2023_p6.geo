A B = segment
C = eq_triangle(A, B)
A1 = on_line(A, midpoint(B, C))
B1 = on_line(B, midpoint(C, A))
C1 = on_line(C, midpoint(A, B))
assume angle(B, A1, C) + angle(C, B1, A) + angle(A, C1, B) = 480
A2 = meet(line(B, C1), line(C, B1))
B2 = meet(line(C, A1), line(A, C1))
C2 = meet(line(A, B1), line(B, A1))
P = meet(circumcircle(A, A1, A2), circumcircle(B, B1, B2))
prove cyclic(C, C1, C2, P)
