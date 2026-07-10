# THEOREM (incenter-excenter lemma, "Fact 5"): the second intersection M of
# line AI with the circumcircle is equidistant from B and I (and C).
A B C = triangle
I = incenter(A, B, C)
M = meet(line(A, I), circumcircle(A, B, C))
prove cong(M, B, M, I)
