# Geometric-mean (right-triangle altitude) relation, derived from similar
# triangles: with the right angle at C and H the foot of the altitude to AB,
# CH is the geometric mean of the two hypotenuse segments.
A = free
B = free
C = point: perp(C,A,C,B)
H = foot(C, line(A,B))
prove dist(C,H)^2 = dist(A,H)*dist(H,B)
