# The reflection of the orthocenter across a side lies on the circumcircle.
A B C = triangle
H = orthocenter(A, B, C)
R = reflect(H, line(B, C))
prove cyclic(A, B, C, R)
