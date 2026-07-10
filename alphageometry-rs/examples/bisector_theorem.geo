# THEOREM (angle bisector): the bisector from A divides BC in the ratio of the
# adjacent sides: |XB| / |XC| = |AB| / |AC|.
A B C = triangle
X = meet(bisector(B, A, C), line(B, C))
prove eqratio(X, B, X, C, A, B, A, C)
