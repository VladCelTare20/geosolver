# The diagonals of a parallelogram bisect each other.
# D completes parallelogram A B C D; X is where diagonals AC and BD meet.
A B C = triangle
D = parallelogram(A, B, C)
X = meet(line(A, C), line(B, D))
prove cong(A, X, X, C)
