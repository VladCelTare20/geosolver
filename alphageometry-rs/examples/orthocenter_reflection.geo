# A classic olympiad lemma: the reflection of the orthocenter of a triangle
# over any side lies on the circumcircle.
A B C = triangle
H = orthocenter(A, B, C)
K = reflect(H, line(B, C))       # reflect H over the line BC
prove cyclic(A, B, C, K)
