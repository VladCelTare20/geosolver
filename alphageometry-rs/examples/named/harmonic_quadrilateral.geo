# Harmonic quadrilateral — the second intersection of the A-symmedian with the circumcircle forms a quadrilateral with AB·CD = AC·BD (source: Evan Chen, EGMO)
A B C = triangle
O = circumcenter(A, B, C)
T = meet(perp_line(B, line(O, B)), perp_line(C, line(O, C)))
D = meet(line(A, T), circumcircle(A, B, C))
prove dist(A, B) * dist(C, D) = dist(A, C) * dist(B, D)
