# Thales' theorem: an angle inscribed in a semicircle is a right angle.
# O is the midpoint of AB (so AB is a diameter); C lies on the circle.
A = free
O = free
B = reflect(A, O)
C = point: on(C, circle(O, A))
prove perp(C, A, C, B)
