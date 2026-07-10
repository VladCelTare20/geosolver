# Van Schooten's theorem — for a point P on the arc BC of the circumcircle of an equilateral triangle, PA = PB + PC (source: van Schooten; cut-the-knot)
A B = segment
C = eq_triangle(A, B)
P = meet(line(A, midpoint(centroid(A, B, C), B)), circumcircle(A, B, C))
prove dist(P, A) = dist(P, B) + dist(P, C)
