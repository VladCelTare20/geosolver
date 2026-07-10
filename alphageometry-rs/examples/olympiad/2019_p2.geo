# IMO 2019 Problem 2 — solved from the bare statement.
# The auxiliary construction the search must DISCOVER for itself: the
# circumcircle of ABC and the second intersections of cevians AA1, BB1 with it.
A B C = triangle
A1 = point: coll(B, C, A1)
B1 = point: coll(A, C, B1)
P = point: coll(A, A1, P)
Q = point: coll(B, B1, Q), para(P, Q, A, B)
P1 = point: coll(B1, P, P1), eqangle(P1, P, P1, C, A, B, A, C)
Q1 = point: coll(A1, Q, Q1), eqangle(Q1, C, Q1, Q, B, C, B, A)
prove cyclic(P, Q, P1, Q1)
