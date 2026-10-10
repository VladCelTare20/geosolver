# IMO 2004 Problem 1
#
# ABC is an acute triangle with AB != AC. The circle with diameter BC meets
# AB and AC again at M and N; O is the midpoint of BC. The bisectors of angle
# BAC and angle MON meet at R. Prove that the circumcircles of BMR and CNR
# meet on BC. (Corpus: translated_imo_2004_p1; needs one auxiliary point.)
A B C = triangle
O = midpoint(B, C)
M = meet(line(A, B), circle(O, B))
N = meet(line(A, C), circle(O, B))
R = meet(bisector(B, A, C), bisector(M, O, N))
P = meet(circumcircle(B, M, R), circumcircle(C, N, R))
prove coll(P, B, C)
