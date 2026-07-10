# The nine-point circle: the three side-midpoints and a foot of an altitude are
# concyclic (four of the nine points).
A B C = triangle
Ma = midpoint(B, C)
Mb = midpoint(C, A)
Mc = midpoint(A, B)
Fa = foot(A, line(B, C))
prove cyclic(Ma, Mb, Mc, Fa)
