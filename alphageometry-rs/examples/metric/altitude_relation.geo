# A COMPOUND goal (not itself a named theorem): in a right triangle with the
# altitude foot H on the hypotenuse, CA^2 * HB = CB^2 * AH. The proof USES the
# two geometric-mean leg relations (CA^2 = AH*AB and CB^2 = BH*AB) as cited
# lemmas — theorems are used when they are useful shortcuts.
A = free
B = free
C = point: perp(C,A,C,B)
H = foot(C, line(A,B))
prove dist(C,A)^2 * dist(H,B) = dist(C,B)^2 * dist(A,H)
