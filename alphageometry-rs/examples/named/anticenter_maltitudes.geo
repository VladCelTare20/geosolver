# Anticenter (maltitude) theorem — the maltitudes of a cyclic quadrilateral (lines through a side midpoint perpendicular to the opposite side) are concurrent (source: Johnson, Advanced Euclidean Geometry)
A B C = triangle
O = circumcenter(A, B, C)
D = reflect(B, O)
M1 = midpoint(A, B)
M2 = midpoint(B, C)
M3 = midpoint(C, D)
X = meet(perp_line(M1, line(C, D)), perp_line(M2, line(D, A)))
prove perp(M3, X, A, B)
