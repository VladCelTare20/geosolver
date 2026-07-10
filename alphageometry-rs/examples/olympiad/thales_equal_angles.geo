# Two points on a circle subtend equal directed angles over a diameter's chord.
O = free
A = free
B = reflect(A, O)
C = point: on(C, circle(O, A))
D = point: on(D, circle(O, A))
prove eqangle(A, C, C, B, A, D, D, B)
