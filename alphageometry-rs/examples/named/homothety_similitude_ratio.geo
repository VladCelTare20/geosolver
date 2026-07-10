# Homothety at the external centre of similitude maps a circle point to the parallel-radius point
O1 = free
O2 = free
A1 = point: dist(O1, A1) = 3
B1 = point: on(B1, circle(O1, A1))
A2 = point: dist(O2, A2) = 2
B2 = point: on(B2, circle(O2, A2))
Z = point: coll(Z, O1, O2), dist(Z,O1)*dist(O2,A2) = dist(Z,O2)*dist(O1,A1), dist(Z,O2) + dist(O1,O2) = dist(Z,O1)
P = point: on(P, circle(O1, A1))
Q = point: on(Q, circle(O2, A2)), coll(Z, P, Q), para(O1, P, O2, Q)
prove eqratio(Z, P, Z, Q, O1, A1, O2, A2)
