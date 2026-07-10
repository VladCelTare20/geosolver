# The base angles of an isosceles triangle are equal.
# A is defined purely by a constraint (equal distances to B and C) — the
# solver finds a suitable position. This is the flexible `point:` form.
B C = segment
A = point: dist(A, B) = dist(A, C)
prove eqangle(B, C, B, A, C, A, C, B)
