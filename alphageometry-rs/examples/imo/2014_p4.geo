# IMO 2014 Problem 4
#
# In triangle ABC, points P and Q lie on side BC such that
#   angle(PAB) = angle(BCA) and angle(QAC) = angle(CBA).
# M and N are on rays AP and AQ with AP = PM and AQ = QN.
# Prove that BM and CN meet on the circumcircle of ABC.
#
# The key angle conditions define P and Q as "isogonal" feet:
#   eqangle(A,B, A,P, C,A, C,B)   ←  ∠PAB = ∠BCA
#   eqangle(B,A, B,C, A,C, A,Q)   ←  ∠QAC = ∠CBA
# M = reflect(A, P) and N = reflect(A, Q) give AP = PM, AQ = QN.

A B C = triangle
P = point: coll(B, C, P), eqangle(A, B, A, P, C, A, C, B)
Q = point: coll(B, C, Q), eqangle(B, A, B, C, A, C, A, Q)
M = reflect(A, P)
N = reflect(A, Q)
X = meet(line(B, M), line(C, N))
O = circumcenter(A, B, C)
prove cong(O, X, O, A)
