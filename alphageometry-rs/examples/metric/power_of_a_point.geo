# Power of a point, derived from similar triangles (inscribed + vertical angles):
# two chords of one circle meeting at P give PA*PB = PC*PD.
O = free
R = free
A = on_circle(O, R)
B = on_circle(O, R)
C = on_circle(O, R)
D = on_circle(O, R)
P = meet(line(A,B), line(C,D))
prove dist(P,A)*dist(P,B) = dist(P,C)*dist(P,D)
