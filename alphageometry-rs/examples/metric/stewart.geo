# Stewart's theorem (additive engine): for a cevian AD with D dividing BC in the
# ratio BD:DC = 1:2, the section formula gives AD^2 exactly. Here AD^2 = 14.
B = free
C = point: dist(B,C)=6
D = point: coll(B,D,C), dist(B,D)=2
A = point: dist(A,B)=5, dist(A,C)=4
prove dist(A,D)^2 = 14
