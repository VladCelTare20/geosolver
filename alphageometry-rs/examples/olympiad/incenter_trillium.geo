# Trillium (incenter–excenter) lemma: the second intersection M of AI with the
# circumcircle satisfies MB = MI (M is the arc midpoint, equidistant from B, C, I).
A B C = triangle
I = incenter(A, B, C)
M = meet(line(A, I), circumcircle(A, B, C))
prove cong(M, B, M, I)
