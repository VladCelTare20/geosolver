# Symmedian ratio — the symmedian from A divides BC in the ratio AB² : AC² (source: Johnson, Advanced Euclidean Geometry)
A B C = triangle
O = circumcenter(A, B, C)
T = meet(perp_line(B, line(O, B)), perp_line(C, line(O, C)))
X = meet(line(A, T), line(B, C))
prove dist(B, X) * dist(A, C) ^ 2 = dist(X, C) * dist(A, B) ^ 2
