# THEOREM (Euler line): the circumcenter O, centroid G, and orthocenter H of
# any triangle are collinear.
A B C = triangle
O = circumcenter(A, B, C)
G = centroid(A, B, C)
H = orthocenter(A, B, C)
prove coll(O, G, H)
