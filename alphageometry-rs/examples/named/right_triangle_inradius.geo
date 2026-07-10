# Right-triangle inradius — the inradius of a right triangle is (leg + leg − hypotenuse)/2 (source: classical; olympiad folklore)
B C = segment
A = on_dia(B, C)
I = incenter(A, B, C)
F = foot(I, line(B, C))
prove 2 * dist(I, F) = dist(A, B) + dist(A, C) - dist(B, C)
