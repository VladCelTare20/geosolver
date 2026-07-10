# Simson line bisects PH — the Simson line of P passes through the midpoint of P and the orthocenter H (source: Coxeter & Greitzer, Geometry Revisited)
A B C = triangle
H = orthocenter(A, B, C)
P = on_circum(A, B, C)
X = foot(P, line(B, C))
Y = foot(P, line(C, A))
M = midpoint(P, H)
prove coll(X, Y, M)
