# IMO 2013 Problem 4
#
# Let ABC be an acute triangle with orthocenter H, and let W be a point
# on side BC. Let M and N be the feet of the altitudes from B and C.
# ω₁ is the circumcircle of BWN and X is the antipode of W on ω₁.
# ω₂ is the circumcircle of CWM and Y is the antipode of W on ω₂.
# Prove that X, Y, and H are collinear.
#
# The key: M = foot(B, line(A,C)), N = foot(C, line(A,B)).
# "Antipode of W on circumcircle of BWN" means X is such that WX is a
# diameter — i.e. the circumcenter O₁ of BWN is the midpoint of WX.

A B C = triangle
H = orthocenter(A, B, C)
M = foot(B, line(A, C))
N = foot(C, line(A, B))
W = point: coll(B, C, W)
O1 = circumcenter(B, W, N)
O2 = circumcenter(C, W, M)
X = reflect(W, O1)
Y = reflect(W, O2)
prove coll(X, H, Y)
