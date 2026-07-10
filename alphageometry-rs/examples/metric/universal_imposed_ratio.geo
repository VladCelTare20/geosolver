# UNIVERSAL ENCODING: impose an arbitrary relation on a free point (BC = 3·AB),
# then ask an arbitrary high-degree algebraic question. The proof USES the
# imposed hypothesis. Nothing about this is hard-coded — any relation, any goal.
A = free
B = free
C = point: dist(B,C) = 3*dist(A,B)
prove dist(A,B)^12 * dist(B,C)^7 = 2187 * dist(A,B)^19
