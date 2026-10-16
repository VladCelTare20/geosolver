# IMO 2008 Problem 1 (the B/C half, as corpus translated_imo_2008_p1a)
#
# H is the orthocenter of the acute triangle ABC. The circle through H
# centred at the midpoint of CA meets line CA at B1, B2; the circle through H
# centred at the midpoint of AB meets line AB at C1, C2. Prove that B1, B2,
# C1, C2 are concyclic. (Needs one auxiliary point.)
A B C = triangle
H = orthocenter(A, B, C)
D = midpoint(B, C)
E = midpoint(C, A)
F = midpoint(A, B)
B1 B2 = meet(line(C, A), circle(E, H))
C1 C2 = meet(line(A, B), circle(F, H))
prove cyclic(C1, C2, B1, B2)
