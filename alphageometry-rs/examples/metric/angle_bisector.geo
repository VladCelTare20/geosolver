# The angle-bisector theorem, derived (perpendiculars to the bisector + similar
# triangles): AD bisects angle A, so BD:DC = AB:AC.
A = free
B = free
C = free
D = meet(bisector(B,A,C), line(B,C))
prove dist(B,D)*dist(A,C) = dist(D,C)*dist(A,B)
