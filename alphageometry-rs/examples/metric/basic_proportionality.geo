# Basic proportionality / Thales' intercept theorem, from similar triangles:
# a line DE parallel to BC cuts AB, AC proportionally.
A = free
B = free
C = free
D = on_line(A, B)
E = meet(para_line(D, line(B,C)), line(A,C))
prove dist(A,D)*dist(A,C) = dist(A,E)*dist(A,B)
