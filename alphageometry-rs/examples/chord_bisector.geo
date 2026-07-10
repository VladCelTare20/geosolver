# The line from a circle's centre to the midpoint of a chord is perpendicular
# to that chord. B is defined by a constraint: it lies on the circle.
O = free
A = free
B = point: on(B, circle(O, A))
M = midpoint(A, B)
prove perp(O, M, A, B)
