# Incircle diameter lemma — the point diametrically opposite the contact point on the incircle is collinear with A and the extouch point (source: Evan Chen, EGMO Lemma 4.9)
A B C = triangle
I = incenter(A, B, C)
Ia = excenter(A, B, C)
T = foot(I, line(B, C))
T2 = reflect(T, I)
X = foot(Ia, line(B, C))
prove coll(A, T2, X)
