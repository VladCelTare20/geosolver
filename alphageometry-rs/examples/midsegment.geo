# Midsegment theorem: the segment joining the midpoints of two sides of a
# triangle is parallel to the third side.
#
# Note the midpoints are computed inline in the goal — expressions nest freely.
A B C = triangle
prove para(midpoint(A, B), midpoint(A, C), B, C)
