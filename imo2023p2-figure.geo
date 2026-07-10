# IMO 2023 Problem 2
A B C = triangle
assume angle(A, B, C) = 75
assume angle(B, C, A) = 46
O = circumcenter(A, B, C)
I = incenter(A, B, C)
N = meet(line(A, I), circumcircle(A, B, C))
S = meet(line(N, O), circumcircle(A, B, C))
D = point: coll(B, S, D), perp(A, D, B, C)
E = meet(line(A, D), circumcircle(A, B, C))
L = point: coll(B, E, L), para(D, L, B, C)
P = meet(circle(B, D, L), circumcircle(A, B, C))
O1 = circumcenter(B, D, L)
X = point: coll(B, S, X), perp(X, P, P, O1)
prove eqangle(A, B, A, X, A, X, A, C)
