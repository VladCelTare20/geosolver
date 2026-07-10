# THEOREM (intercept / Thales): three parallel lines cut two transversals in
# proportional segments — provable even though the transversals' intersection
# is not part of the figure.
A B = segment
M = free
Q = free
N = meet(para_line(M, line(A, B)), line(B, Q))
D = point: on(D, line(A, M))
C = meet(para_line(D, line(A, B)), line(B, Q))
prove eqratio(A, M, M, D, B, N, N, C)
