# THEOREM (nine-point circle): the three side midpoints and the foot of an
# altitude are concyclic.
A B C = triangle
Ma = midpoint(B, C)
Mb = midpoint(A, C)
Mc = midpoint(A, B)
F = foot(A, line(B, C))
prove cyclic(Ma, Mb, Mc, F)
