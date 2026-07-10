# The reflection of the orthocenter H over the midpoint of a side lies on the
# circumcircle (it is the antipode of the opposite vertex). No hint is given
# about the circumcircle — the search must invent it.
A B C = triangle
H = orthocenter(A, B, C)
M = midpoint(B, C)
D = reflect(H, M)
prove cyclic(A, B, C, D)
