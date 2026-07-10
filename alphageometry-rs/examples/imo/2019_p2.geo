# IMO 2019 Problem 2 (needs auxiliary points — solve with `--geo`, which falls
# back to the aux search; a direct DDAR run does not suffice).
#
# In triangle ABC, A1 lies on BC and B1 on AC. P on AA1 and Q on BB1 with
# PQ ∥ AB. P1 on line B1P with ∠PP1C = ∠BAC. Q1 on line A1Q with ∠CQ1Q = ∠CBA.
# Prove P, Q, P1, Q1 are concyclic.
#
# The auxiliary construction the search must find: the circumcircle of ABC and
# the second intersections of the cevians AA1, BB1 with it.
A B C = triangle
A1 = point: coll(B, C, A1)
B1 = point: coll(A, C, B1)
P = point: coll(A, A1, P)
Q = point: coll(B, B1, Q), para(P, Q, A, B)
P1 = point: coll(B1, P, P1), eqangle(P1, P, P1, C, A, B, A, C)
Q1 = point: coll(A1, Q, Q1), eqangle(Q1, C, Q1, Q, B, C, B, A)
prove cyclic(P, Q, P1, Q1)
