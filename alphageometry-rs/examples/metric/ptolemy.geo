# PTOLEMY'S THEOREM, found by the GENERAL auxiliary-point search (no per-theorem
# code): for a cyclic quadrilateral, AC·BD = AB·CD + AD·BC. The figure is a
# generic (isosceles-trapezoid) cyclic quad; the engine introduces the classic
# auxiliary point on a side (∠BAK = ∠CAD) and finishes with similar triangles.
A B C = triangle
D = reflect(C, perp_bisector(A, B))
prove dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D) + dist(A,D)*dist(B,C)
