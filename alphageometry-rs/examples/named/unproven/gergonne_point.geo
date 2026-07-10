# Gergonne point — the cevians to the incircle contact points are concurrent (source: Gergonne; Coxeter & Greitzer, Geometry Revisited)
A B C = triangle
I = incenter(A, B, C)
Ta = foot(I, line(B, C))
Tb = foot(I, line(C, A))
Tc = foot(I, line(A, B))
X = meet(line(A, Ta), line(B, Tb))
prove coll(C, Tc, X)
