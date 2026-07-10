# Impose two lengths and a squared-length relation on a point, then prove a
# consequence — the constraint language expresses anything a solver can pin.
A = free
B = point: dist(A,B) = 5
P = point: dist(P,A)^2 = 9, dist(P,B)^2 = 16
prove dist(P,A)^2 + dist(P,B)^2 = 25
