# Monge's theorem — the three external centres of similitude of three circles are collinear
O1 = free
O2 = free
O3 = free
A1 = point: dist(O1, A1) = 3
B1 = point: on(B1, circle(O1, A1))
A2 = point: dist(O2, A2) = 2
B2 = point: on(B2, circle(O2, A2))
A3 = point: dist(O3, A3) = 1
B3 = point: on(B3, circle(O3, A3))
Z12 = point: coll(Z12, O1, O2), dist(Z12,O1)*dist(O2,A2) = dist(Z12,O2)*dist(O1,A1), dist(Z12,O2) + dist(O1,O2) = dist(Z12,O1)
Z13 = point: coll(Z13, O1, O3), dist(Z13,O1)*dist(O3,A3) = dist(Z13,O3)*dist(O1,A1), dist(Z13,O3) + dist(O1,O3) = dist(Z13,O1)
Z23 = point: coll(Z23, O2, O3), dist(Z23,O2)*dist(O3,A3) = dist(Z23,O3)*dist(O2,A2), dist(Z23,O3) + dist(O2,O3) = dist(Z23,O2)
prove coll(Z12, Z13, Z23)
