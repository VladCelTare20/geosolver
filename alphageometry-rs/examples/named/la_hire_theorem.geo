# La Hire's theorem — if Q lies on the polar of P, then P lies on the polar of Q (source: Coxeter & Greitzer, Geometry Revisited)
O A = segment
P = shift(A, O, A)
T1 T2 = tangent(P, circle(O, A))
Q = shift(T1, T2, T1)
U1 U2 = tangent(Q, circle(O, A))
prove coll(P, U1, U2)
