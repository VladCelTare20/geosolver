# Equal chords equidistant — equal chords of a circle are equally distant from the center (source: Euclid, Elements III.14)
O A = segment
B = on_circle(O, A)
C = on_circle(O, A)
D = point: on(D, circle(O, A)), cong(C, D, A, B)
F1 = foot(O, line(A, B))
F2 = foot(O, line(C, D))
prove cong(O, F1, O, F2)
