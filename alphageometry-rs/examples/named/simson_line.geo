# Simson line — the feet of the perpendiculars from a point on the circumcircle to the three sides are collinear (source: Coxeter & Greitzer, Geometry Revisited)
A B C = triangle
P = on_circum(A, B, C)
X = foot(P, line(B, C))
Y = foot(P, line(C, A))
Z = foot(P, line(A, B))
prove coll(X, Y, Z)
