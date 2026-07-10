# FREE-STANDING HYPOTHESIS: declare points, then `assume` any relation among
# them — solved jointly by a global solver, and usable in the proof. No point is
# redefined; the figure is stated by its properties. Here: three free points
# assumed to make an isosceles triangle, then a length consequence is proven.
A = free
B = free
C = free
assume dist(A,B) = dist(A,C)
prove dist(A,B)^2 = dist(A,C)^2
