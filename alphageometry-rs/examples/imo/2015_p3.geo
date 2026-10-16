# IMO 2015 Problem 3
#
# ABC is an acute triangle with orthocenter H; F is the foot of the altitude
# from A and M the midpoint of BC. Q on the circumcircle has angle HQA = 90,
# and K on the circumcircle has angle HKQ = 90. Prove that the circumcircles
# of KQH and FKM are tangent: their centres and K are collinear.
# (Corpus: translated_imo_2015_p3; needs one auxiliary point.)
A B C = triangle
H = orthocenter(A, B, C)
F = foot(A, line(B, C))
M = midpoint(B, C)
O = circumcenter(A, B, C)
Q = point: on(Q, circumcircle(A, B, C)), perp(Q, A, Q, H)
K = point: on(K, circumcircle(A, B, C)), perp(K, H, K, Q)
O1 = circumcenter(K, Q, H)
O2 = circumcenter(F, K, M)
prove coll(O1, O2, K)
