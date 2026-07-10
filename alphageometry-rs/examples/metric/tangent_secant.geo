# Tangent-secant power, derived from similar triangles (tangent-chord angle
# equals the inscribed angle): PT^2 = PA*PS for tangent PT and secant PAS.
O = free
A = on_circle(O, free)
P = free
T1 T2 = tangent(P, circle(O, A))
S = meet(line(P, A), circle(O, A))
prove dist(P,T1)^2 = dist(P,A)*dist(P,S)
