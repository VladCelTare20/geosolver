# Lemoine (symmedian) point — the three symmedians of a triangle are concurrent (source: Lemoine; Johnson, Advanced Euclidean Geometry)
A B C = triangle
O = circumcenter(A, B, C)
Ta = meet(perp_line(B, line(O, B)), perp_line(C, line(O, C)))
Tb = meet(perp_line(C, line(O, C)), perp_line(A, line(O, A)))
Tc = meet(perp_line(A, line(O, A)), perp_line(B, line(O, B)))
K = meet(line(A, Ta), line(B, Tb))
prove coll(C, Tc, K)
