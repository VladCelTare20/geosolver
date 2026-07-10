# THEOREM (tangent-chord angle): the angle between a tangent at A and the
# chord AB equals the inscribed angle of AB in the alternate segment.
O A = segment
B = point: on(B, circle(O, A))
C = point: on(C, circle(O, A))
X = point: perp(X, A, A, O)
prove eqangle(A, X, A, B, C, A, C, B)
