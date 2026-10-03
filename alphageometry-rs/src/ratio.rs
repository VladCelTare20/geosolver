//! Classical Euclidean proofs of **multiplicative** (ratio / product) length
//! goals — the length theorems that live outside the additive squared-length
//! system because they equate *products* of unsquared lengths: power of a point,
//! the geometric-mean relations in a right triangle, the basic proportionality
//! (Thales') theorem, the angle-bisector ratio, Menelaus, and Ceva.
//!
//! Method. Take logarithms. A monomial identity `∏|·|^{aᵢ} = ∏|·|^{bⱼ}·k`
//! becomes a **linear** equation `Σ aᵢ·Lᵢ − Σ bⱼ·Lⱼ = ln k` over the log-length
//! atoms `L_{ab} = ln|ab|`, with the constant `ln k` carried as an exact
//! rational combination of `ln(prime)`s. Every applicable classical theorem
//! contributes one such linear equation; exact rational elimination reduces the
//! goal to `0`, and the chain of theorems used is printed as the proof.
//!
//! Purity. The ratio-producing engine is **similar triangles**: two triangles
//! shown similar (from elementary angle facts — inscribed angles in a circle,
//! parallels, vertical/right/shared angles) give proportional sides. The famous
//! results (geometric mean, power of a point, Thales) are *derived* this way, so
//! a proof never rests on the named result it is asked to establish. The
//! remaining classics (Menelaus, Ceva, the angle-bisector ratio) are cited as
//! lemmas — legitimate when they are a *step*, and blocked by the
//! anti-circularity guard when they would *be* the goal.
//!
//! Every fact a step rests on is derived, never measured. The figure's
//! coordinates only *propose* candidates (which triangles look similar, which
//! lines look parallel) and read configuration (order, orientation); a
//! candidate is used only once its angle facts are derived from the hypotheses
//! by the DDAR closure ([`crate::engine`]), and that derivation is printed with
//! the step.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use crate::geo::SampledFigure;
use crate::metric::MExpr;
use crate::numerics::{intersect_ll, NumCircle, NumLine, Vec2};
use crate::certify::{derivation_block, pred, with_refs, Certifier};
use crate::predicate::{PointId, Predicate};
use crate::rational::Rat;
use crate::synthetic::{equal_radius_circles, rat_of, Outcome};

mod trig;

/// A log-length unknown `ln|ab|`, keyed by the unordered pair.
type LAtom = (PointId, PointId);

fn latom(a: PointId, b: PointId) -> LAtom {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// A log unknown: `Sin(v, p, q)` is `ln|sin ∠pvq|` (`p < q`), `Len(a, b)` is
/// `ln|ab|` (`a < b`). Sines sort first, so elimination pivots them out first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum LKey {
    Sin(PointId, PointId, PointId),
    Len(PointId, PointId),
}

impl From<LAtom> for LKey {
    fn from(a: LAtom) -> LKey {
        LKey::Len(a.0, a.1)
    }
}

// ===========================================================================
// Linear equations over log-lengths (constant carried as Σ eₚ·ln p)
// ===========================================================================

/// `Σ coeff·ln|atom| + Σ eₚ·ln(p) = 0`.
#[derive(Clone, Debug, Default)]
struct LEq {
    terms: BTreeMap<LKey, Rat>,
    primes: BTreeMap<i64, Rat>,
}

impl LEq {
    fn is_zero(&self) -> bool {
        self.terms.values().all(Rat::is_zero) && self.primes.values().all(Rat::is_zero)
    }
    fn add_term(&mut self, a: impl Into<LKey>, c: Rat) {
        let a = a.into();
        let cur = self.terms.get(&a).cloned().unwrap_or_else(Rat::zero);
        let s = &cur + &c;
        if s.is_zero() {
            self.terms.remove(&a);
        } else {
            self.terms.insert(a, s);
        }
    }
    fn add_prime(&mut self, p: i64, c: Rat) {
        let cur = self.primes.get(&p).cloned().unwrap_or_else(Rat::zero);
        let s = &cur + &c;
        if s.is_zero() {
            self.primes.remove(&p);
        } else {
            self.primes.insert(p, s);
        }
    }
    /// `self := self − factor·other`.
    fn sub_scaled(&mut self, other: &LEq, factor: &Rat) {
        for (a, c) in &other.terms {
            self.add_term(*a, -&(c * factor));
        }
        for (p, c) in &other.primes {
            self.add_prime(*p, -&(c * factor));
        }
    }
    fn first_atom(&self) -> Option<LKey> {
        self.terms.iter().find(|(_, c)| !c.is_zero()).map(|(a, _)| *a)
    }
}

/// `Σ eₚ·ln(p) = ln(v)` for a positive rational `v` (its prime factorisation).
/// `None` if `v ≤ 0`.
fn ln_primes(v: &Rat) -> Option<BTreeMap<i64, Rat>> {
    let num = v.numer_i64()?;
    let den = v.denom_i64()?;
    if num <= 0 || den <= 0 {
        return None;
    }
    let mut m = BTreeMap::new();
    for (p, e) in factor(num as u64) {
        *m.entry(p).or_insert(0i64) += e;
    }
    for (p, e) in factor(den as u64) {
        *m.entry(p).or_insert(0i64) -= e;
    }
    Some(
        m.into_iter()
            .filter(|(_, e)| *e != 0)
            .map(|(p, e)| (p, Rat::from_int(e)))
            .collect(),
    )
}

/// Prime factorisation of a positive integer as `(prime, exponent)` pairs.
fn factor(mut n: u64) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut d = 2u64;
    while d * d <= n {
        if n.is_multiple_of(d) {
            let mut e = 0;
            while n.is_multiple_of(d) {
                n /= d;
                e += 1;
            }
            out.push((d as i64, e));
        }
        d += 1;
    }
    if n > 1 {
        out.push((n as i64, 1));
    }
    out
}

// ===========================================================================
// Proof steps and the gathered figure
// ===========================================================================

struct Step {
    text: String,
    eq: Option<LEq>,
    premises: Vec<usize>,
    /// A compound named result cited directly (Menelaus, Ceva, the
    /// angle-bisector ratio). A lone citation equal to the goal is circular.
    headline: bool,
    /// Alternative premise sets; the row is usable only once DDAR derives one.
    alts: Vec<Vec<Predicate>>,
    /// Bookkeeping (a congruence, an equal or known sine): does not count as
    /// a second reason next to a lone headline.
    support: bool,
    lead: bool,
}

struct Figure {
    names: Vec<String>,
    /// All instances (index 0 is primary); extra instances gate every numeric
    /// detection so a rule fires only on a construction-fixed quantity.
    insts: Vec<Vec<Vec2>>,
    concyclic: Vec<BTreeSet<PointId>>,
    circles: Vec<(PointId, BTreeSet<PointId>)>,
    colls: Vec<Vec<PointId>>,
    perps: Vec<[PointId; 4]>,
    paras: Vec<[PointId; 4]>,
    congs: Vec<[PointId; 4]>,
    abs_len: Vec<(PointId, PointId, Rat)>,
    steps: Vec<Step>,
    /// Steps of the general sum-of-products engine (degree-2 length identities).
    psteps: Vec<PStep>,
    /// Counter of perpendicular-foot *drop groups*; each drop (one transversal or
    /// one bisector) bumps it, so its feet get a shared clean subscript
    /// (`A₁,B₁,C₁` on the first drop, `A₂,B₂,C₂` on the second). Feet stay fresh
    /// per drop (never merged) so the solver behaves identically to distinct
    /// auxiliary points.
    drop_group: usize,
    /// The construction's hypothesis predicates plus the defining facts of every
    /// auxiliary point added since — exactly what the DDAR certifier may use.
    preds: Vec<Predicate>,
    /// How many of `preds` are the problem's hypotheses.
    hyps: usize,
    /// The DDAR closure over `preds`, rebuilt when points or facts are added.
    ddar: RefCell<Certifier>,
    /// Candidate steps of the product engine awaiting DDAR certification:
    /// `(intro step, facts it needs, is it a named-theorem citation)`.
    pending: Vec<(usize, Vec<Predicate>, bool)>,
}

impl Figure {
    fn coord(&self, p: PointId) -> Vec2 {
        self.insts[0][p as usize]
    }
    fn nm(&self, p: PointId) -> String {
        self.names[p as usize].clone()
    }
    fn seg(&self, a: PointId, b: PointId) -> String {
        format!("{}{}", self.nm(a), self.nm(b))
    }
    fn dist(&self, inst: usize, a: PointId, b: PointId) -> f64 {
        (self.insts[inst][a as usize] - self.insts[inst][b as usize]).norm()
    }
    /// Unsigned angle at `b` in the triangle `a b c`, in the given instance.
    fn angle(&self, inst: usize, a: PointId, b: PointId, c: PointId) -> f64 {
        let (u, v) = (
            self.insts[inst][a as usize] - self.insts[inst][b as usize],
            self.insts[inst][c as usize] - self.insts[inst][b as usize],
        );
        let d = u.norm() * v.norm();
        if d < 1e-12 {
            return f64::NAN;
        }
        (u.dot(v) / d).clamp(-1.0, 1.0).acos()
    }

    fn push(&mut self, text: String, eq: Option<LEq>, premises: Vec<usize>, headline: bool) -> usize {
        self.steps.push(Step {
            text,
            eq,
            premises,
            headline,
            alts: Vec::new(),
            support: false,
            lead: false,
        });
        self.steps.len() - 1
    }

    // -- symbolic predicate queries ----------------------------------------

    /// Is `coll(a, b, c)` an asserted collinearity (any collinear set holding
    /// all three)?
    fn coll(&self, a: PointId, b: PointId, c: PointId) -> bool {
        self.colls
            .iter()
            .any(|s| s.contains(&a) && s.contains(&b) && s.contains(&c))
    }
    fn has_perp(&self, w: PointId, x: PointId, y: PointId, z: PointId) -> bool {
        let (s1, s2) = (latom(w, x), latom(y, z));
        self.perps.iter().any(|p| {
            let (u, v) = (latom(p[0], p[1]), latom(p[2], p[3]));
            (u, v) == (s1, s2) || (u, v) == (s2, s1)
        })
    }

    // ----------------------------------------------------------------------

    fn gather(fig: &SampledFigure, insts: Vec<Vec<Vec2>>) -> Figure {
        let mut f = Figure {
            names: fig.names.clone(),
            insts,
            concyclic: Vec::new(),
            circles: Vec::new(),
            colls: Vec::new(),
            perps: Vec::new(),
            paras: Vec::new(),
            congs: Vec::new(),
            abs_len: Vec::new(),
            steps: Vec::new(),
            psteps: Vec::new(),
            drop_group: 0,
            preds: fig.preds.clone(),
            hyps: fig.preds.len(),
            ddar: RefCell::new(Certifier::default()),
            pending: Vec::new(),
        };
        for p in &fig.preds {
            let pts = &p.points;
            match p.name.as_str() {
                "perp" if pts.len() == 4 => f.perps.push([pts[0], pts[1], pts[2], pts[3]]),
                "para" if pts.len() == 4 => f.paras.push([pts[0], pts[1], pts[2], pts[3]]),
                "cong" if pts.len() == 4 && !(pts[0] == pts[2] && pts[1] == pts[3]) => {
                    f.congs.push([pts[0], pts[1], pts[2], pts[3]])
                }
                "coll" if pts.len() >= 3 => f.colls.push(pts.clone()),
                "cyclic" if pts.len() >= 4 => {
                    f.concyclic.push(pts.iter().copied().collect());
                }
                _ => {}
            }
        }
        for &(a, b, v) in &fig.scale {
            if let Some(r) = rat_of(v) {
                f.abs_len.push((a, b, r));
            }
        }
        f.detect_circles();
        f
    }

    /// Circles whose equal radii follow from the hypotheses (see
    /// [`equal_radius_circles`]), and the concyclic point-sets they induce
    /// (added to `concyclic` for power-of-a-point). The figure is only a
    /// consistency guard.
    fn detect_circles(&mut self) {
        for (o, set) in equal_radius_circles(self.names.len(), &self.congs, &self.abs_len) {
            let pts: Vec<PointId> = set.iter().copied().collect();
            let r2 = self.dist(0, o, pts[0]).powi(2);
            if pts
                .iter()
                .all(|&p| (self.dist(0, o, p).powi(2) - r2).abs() < 1e-6 * (1.0 + r2))
            {
                self.circles.push((o, set.clone()));
                if set.len() >= 4 {
                    self.concyclic.push(set.clone());
                }
            }
        }
    }

    /// Does the figure show `p` strictly between `x` and `y` (a configuration
    /// fact, read from every instance)?
    fn between(&self, x: PointId, p: PointId, y: PointId) -> bool {
        (0..self.insts.len()).all(|i| {
            let (vx, vp, vy) = (
                self.insts[i][x as usize],
                self.insts[i][p as usize],
                self.insts[i][y as usize],
            );
            (vp - vx).dot(vy - vp) > 0.0
        })
    }

    /// How the angles `∠xPx'` and `∠yPy'` at a common vertex relate when the
    /// lines `xy` and `x'y'` cross at `P`: vertical angles if `P` lies between
    /// `x` and `y`, otherwise the same angle.
    fn angle_at(&self, x: PointId, p: PointId, y: PointId) -> &'static str {
        if self.between(x, p, y) {
            "vertical angles"
        } else {
            "the same angle"
        }
    }

    // -- DDAR certification -------------------------------------------------

    /// Derive every predicate in `goals` with the DDAR closure over the
    /// hypotheses (and auxiliary facts) of this figure. Returns the numbered
    /// derivation, or `None` if any goal is not derivable.
    fn certify(&self, goals: &[Predicate]) -> Option<Vec<String>> {
        self.ddar
            .borrow_mut()
            .derive(&self.names, &self.insts[0], &self.preds, self.hyps, goals)
    }

    /// The directed-angle facts that make `t1 = (p,q,r) ~ t2 = (u,v,w)` (by
    /// position) similar, given their orientations in the figure: the angles
    /// at `p`/`u` and at `q`/`v` are equal.
    fn similarity_angles(&self, t1: [PointId; 3], t2: [PointId; 3]) -> Option<[Predicate; 2]> {
        let orient = |t: [PointId; 3], i: usize| {
            let (a, b, c) = (
                self.insts[i][t[0] as usize],
                self.insts[i][t[1] as usize],
                self.insts[i][t[2] as usize],
            );
            let (u, v) = (b - a, c - a);
            (u.x * v.y - u.y * v.x).signum()
        };
        let same = orient(t1, 0) == orient(t2, 0);
        if (0..self.insts.len()).any(|i| (orient(t1, i) == orient(t2, i)) != same) {
            return None;
        }
        let [p, q, r] = t1;
        let [u, v, w] = t2;
        Some(if same {
            [
                pred("eqangle", &[p, r, p, q, u, w, u, v]),
                pred("eqangle", &[q, r, q, p, v, w, v, u]),
            ]
        } else {
            [
                pred("eqangle", &[p, r, p, q, u, v, u, w]),
                pred("eqangle", &[q, r, q, p, v, u, v, w]),
            ]
        })
    }

    /// Are the two triangles similar (equal angles at corresponding vertices) in
    /// *every* instance, and non-degenerate? The soundness gate for `emit_similar`.
    fn similar_in_all(&self, t1: [PointId; 3], t2: [PointId; 3]) -> bool {
        (0..self.insts.len()).all(|i| {
            let a0 = self.angle(i, t1[2], t1[0], t1[1]);
            let a1 = self.angle(i, t1[0], t1[1], t1[2]);
            let b0 = self.angle(i, t2[2], t2[0], t2[1]);
            let b1 = self.angle(i, t2[0], t2[1], t2[2]);
            a0.is_finite()
                && a1.is_finite()
                && b0.is_finite()
                && b1.is_finite()
                && (a0 - b0).abs() < 1e-6
                && (a1 - b1).abs() < 1e-6
                // exclude degenerate (straight/zero) triangles
                && a0 > 1e-4
                && a1 > 1e-4
                && (a0 + a1) < std::f64::consts::PI - 1e-4
        })
    }

    /// Emit the proportional-sides equations of two similar triangles
    /// `t1 = (p,q,r) ~ t2 = (u,v,w)` (correspondence by position), justified by
    /// the caller's elementary `reason`. Guarded: fires only if the triangles are
    /// numerically similar in all instances. Contributes `pq/uv = qr/vw` and
    /// `qr/vw = rp/wu`.
    fn emit_similar(&mut self, t1: [PointId; 3], t2: [PointId; 3], reason: &str, prem: Vec<usize>) {
        if !self.similar_in_all(t1, t2) {
            return;
        }
        let [p, q, r] = t1;
        let [u, v, w] = t2;
        let (reason, derivation) = match reason.split_once('\n') {
            Some((head, rest)) => (head, format!("\n{rest}")),
            None => (reason, String::new()),
        };
        let intro = self.push(
            format!(
                "{reason} — so triangles {} and {} are similar (AA), giving \
                 {}:{} = {}:{} = {}:{}.{derivation}",
                tri(self, t1),
                tri(self, t2),
                self.seg(p, q),
                self.seg(u, v),
                self.seg(q, r),
                self.seg(v, w),
                self.seg(r, p),
                self.seg(w, u),
            ),
            None,
            prem,
            false,
        );
        // pq/uv = qr/vw
        let mut e1 = LEq::default();
        e1.add_term(latom(p, q), Rat::one());
        e1.add_term(latom(u, v), -Rat::one());
        e1.add_term(latom(q, r), -Rat::one());
        e1.add_term(latom(v, w), Rat::one());
        if !e1.is_zero() {
            self.push(String::new(), Some(e1), vec![intro], false);
        }
        // qr/vw = rp/wu
        let mut e2 = LEq::default();
        e2.add_term(latom(q, r), Rat::one());
        e2.add_term(latom(v, w), -Rat::one());
        e2.add_term(latom(r, p), -Rat::one());
        e2.add_term(latom(w, u), Rat::one());
        if !e2.is_zero() {
            self.push(String::new(), Some(e2), vec![intro], false);
        }
    }

    /// Push a headline **citation** of a named theorem — the monomial identity
    /// `∏num = ∏den`. It gives a short one-line proof when the theorem is a
    /// *useful lemma* for a larger goal; the anti-circularity guard blocks it
    /// from standing alone as a proof of the very theorem it names (then the
    /// elementary derivation is used instead).
    fn cite(&mut self, num: &[(PointId, PointId)], den: &[(PointId, PointId)], text: String) {
        let mut eq = LEq::default();
        for &(a, b) in num {
            eq.add_term(latom(a, b), Rat::one());
        }
        for &(a, b) in den {
            eq.add_term(latom(a, b), -Rat::one());
        }
        if eq.is_zero() {
            return;
        }
        self.push(text, Some(eq), vec![], true);
    }

    // === elementary supports ==============================================

    fn emit_congruent(&mut self) {
        for c in self.congs.clone() {
            let (a, b, x, y) = (c[0], c[1], c[2], c[3]);
            if latom(a, b) == latom(x, y) {
                continue;
            }
            let mut eq = LEq::default();
            eq.add_term(latom(a, b), Rat::one());
            eq.add_term(latom(x, y), -Rat::one());
            if eq.is_zero() {
                continue;
            }
            self.push(
                format!(
                    "|{}| = |{}| (given).",
                    self.seg(a, b),
                    self.seg(x, y)
                ),
                Some(eq),
                vec![],
                false,
            );
        }
    }

    fn emit_absolute(&mut self) {
        for (a, b, v) in self.abs_len.clone() {
            let Some(primes) = ln_primes(&v) else { continue };
            let mut eq = LEq::default();
            eq.add_term(latom(a, b), Rat::one());
            for (p, e) in primes {
                eq.add_prime(p, -e);
            }
            self.push(
                format!("|{}| = {} (given).", self.seg(a, b), v),
                Some(eq),
                vec![],
                false,
            );
        }
    }

    // === derivations (pure, via similar triangles) =========================

    /// Geometric-mean relations in a right triangle. For the right angle at `C`
    /// and the foot `H` of the altitude from `C` onto the hypotenuse `AB`,
    /// `AC² = AH·AB`, `BC² = BH·AB`, and `CH² = AH·BH` — each from a pair of
    /// similar right triangles.
    fn derive_geometric_mean(&mut self) {
        let n = self.names.len() as PointId;
        for c in 0..n {
            for a in 0..n {
                for b in 0..n {
                    if a >= b || a == c || b == c {
                        continue;
                    }
                    if !self.has_perp(c, a, c, b) {
                        continue; // right angle at C
                    }
                    for h in 0..n {
                        if h == a || h == b || h == c {
                            continue;
                        }
                        if !(self.coll(a, h, b) && self.has_perp(c, h, a, b)) {
                            continue; // H = foot of the altitude from C to AB
                        }
                        // Citable geometric-mean relations first (preferred as a
                        // short lemma); the similar-triangle derivation follows.
                        self.cite(
                            &[(c, h), (c, h)],
                            &[(a, h), (h, b)],
                            format!(
                                "By the geometric-mean (altitude) relation in right triangle \
                                 {a}{c}{b}, {ch}² = {ah}·{hb}.",
                                a = self.nm(a),
                                c = self.nm(c),
                                b = self.nm(b),
                                ch = self.seg(c, h),
                                ah = self.seg(a, h),
                                hb = self.seg(h, b),
                            ),
                        );
                        self.cite(
                            &[(c, a), (c, a)],
                            &[(a, h), (a, b)],
                            format!(
                                "By the geometric-mean (leg) relation in right triangle {a}{c}{b}, \
                                 {ca}² = {ah}·{ab}.",
                                a = self.nm(a),
                                c = self.nm(c),
                                b = self.nm(b),
                                ca = self.seg(c, a),
                                ah = self.seg(a, h),
                                ab = self.seg(a, b),
                            ),
                        );
                        let r1 = format!(
                            "∠{ah}{c} = 90° = ∠{a}{c}{b} and ∠{h}{a}{c} = ∠{c}{a}{b} (shared at {a})",
                            ah = self.seg(a, h),
                            c = self.nm(c),
                            a = self.nm(a),
                            b = self.nm(b),
                            h = self.nm(h),
                        );
                        self.emit_similar([a, c, h], [a, b, c], &r1, vec![]);
                        let r2 = format!(
                            "∠{bh}{c} = 90° = ∠{b}{c}{a} and ∠{h}{b}{c} = ∠{c}{b}{a} (shared at {b})",
                            bh = self.seg(b, h),
                            c = self.nm(c),
                            a = self.nm(a),
                            b = self.nm(b),
                            h = self.nm(h),
                        );
                        self.emit_similar([b, c, h], [b, a, c], &r2, vec![]);
                        let r3 = format!(
                            "∠{c}{h}{a} = 90° = ∠{c}{h}{b} and ∠{h}{a}{c} = ∠{h}{c}{b} \
                             (each the complement of ∠{a}{b}{c})",
                            c = self.nm(c),
                            h = self.nm(h),
                            a = self.nm(a),
                            b = self.nm(b),
                        );
                        self.emit_similar([a, c, h], [c, b, h], &r3, vec![]);
                    }
                }
            }
        }
    }

    /// Power of a point, derived from similar triangles. Two chords of a circle
    /// through `P` — `{a,b}` and `{c,d}` — give `△aPc ~ △dPb` (vertical angles at
    /// `P`, inscribed angles on chord `cb`), hence `Pa·Pb = Pc·Pd`.
    fn derive_power_of_a_point(&mut self) {
        for set in self.concyclic.clone() {
            let pts: Vec<PointId> = set.iter().copied().collect();
            let n = self.names.len() as PointId;
            for p in 0..n {
                if set.contains(&p) {
                    continue; // P on the circle → degenerate power
                }
                // Chords through P: unordered on-circle pairs collinear with P.
                let mut chords: Vec<(PointId, PointId)> = Vec::new();
                for i in 0..pts.len() {
                    for j in (i + 1)..pts.len() {
                        if self.coll(p, pts[i], pts[j]) {
                            chords.push((pts[i], pts[j]));
                        }
                    }
                }
                for i in 0..chords.len() {
                    for j in (i + 1)..chords.len() {
                        let (a, b) = chords[i];
                        let (c, d) = chords[j];
                        if [a, b].contains(&c) || [a, b].contains(&d) {
                            continue;
                        }
                        self.cite(
                            &[(p, a), (p, b)],
                            &[(p, c), (p, d)],
                            format!(
                                "By the power of the point {p} with respect to the circle, \
                                 {pa}·{pb} = {pc}·{pd}.",
                                p = self.nm(p),
                                pa = self.seg(p, a),
                                pb = self.seg(p, b),
                                pc = self.seg(p, c),
                                pd = self.seg(p, d),
                            ),
                        );
                        // Pick the labelling with △aPc ~ △dPb numerically similar.
                        let mut done = false;
                        for (aa, bb) in [(a, b), (b, a)] {
                            for (cc, dd) in [(c, d), (d, c)] {
                                if done {
                                    break;
                                }
                                if self.similar_in_all([aa, p, cc], [dd, p, bb]) {
                                    let inside = self.between(a, p, b);
                                    let at_p = if inside { "vertical angles" } else { "the same angle" };
                                    let at_chord = if inside {
                                        "inscribed angles on the same arc"
                                    } else {
                                        "an exterior angle of a cyclic quadrilateral equals the interior opposite angle"
                                    };
                                    let reason = format!(
                                        "{a}, {b}, {c}, {d} are concyclic: ∠{aa}{p}{cc} = ∠{dd}{p}{bb} \
                                         ({at_p}) and ∠{p}{aa}{cc} = ∠{p}{dd}{bb} ({at_chord})",
                                        a = self.nm(a),
                                        b = self.nm(b),
                                        c = self.nm(c),
                                        d = self.nm(d),
                                        aa = self.nm(aa),
                                        bb = self.nm(bb),
                                        cc = self.nm(cc),
                                        dd = self.nm(dd),
                                        p = self.nm(p),
                                    );
                                    self.emit_similar([aa, p, cc], [dd, p, bb], &reason, vec![]);
                                    done = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Tangent–secant power, derived from similar triangles. If `PT` is tangent
    /// to a circle at `T` and `PAB` is a secant (`A`, `B` on the circle), then
    /// `△PTA ~ △PBT` (shared angle at `P`, tangent–chord angle `∠PTA` equal to
    /// the inscribed angle `∠TBA`), hence `PT² = PA·PB`.
    fn derive_tangent_secant(&mut self) {
        for (o, on) in self.circles.clone() {
            let pts: Vec<PointId> = on.iter().copied().collect();
            let n = self.names.len() as PointId;
            for &t in &pts {
                for p in 0..n {
                    if on.contains(&p) || p == o {
                        continue;
                    }
                    if !self.has_perp(o, t, t, p) {
                        continue; // PT ⟂ OT ⇒ PT tangent at T
                    }
                    for i in 0..pts.len() {
                        for j in (i + 1)..pts.len() {
                            let (a, b) = (pts[i], pts[j]);
                            if a == t || b == t || !self.coll(p, a, b) {
                                continue;
                            }
                            self.cite(
                                &[(p, t), (p, t)],
                                &[(p, a), (p, b)],
                                format!(
                                    "By the tangent–secant power of {p} (tangent {pt}, secant \
                                     {p}{a}{b}), {pt}² = {pa}·{pb}.",
                                    p = self.nm(p),
                                    pt = self.seg(p, t),
                                    a = self.nm(a),
                                    b = self.nm(b),
                                    pa = self.seg(p, a),
                                    pb = self.seg(p, b),
                                ),
                            );
                            for (aa, bb) in [(a, b), (b, a)] {
                                if self.similar_in_all([p, t, aa], [p, bb, t]) {
                                    let reason = format!(
                                        "{pt} is tangent at {t} and {p}{aa}{bb} is a secant: \
                                         ∠{t}{p}{aa} = ∠{bb}{p}{t} (shared at {p}) and \
                                         ∠{p}{t}{aa} = ∠{t}{bb}{aa} (tangent–chord = inscribed angle)",
                                        pt = self.seg(p, t),
                                        t = self.nm(t),
                                        p = self.nm(p),
                                        aa = self.nm(aa),
                                        bb = self.nm(bb),
                                    );
                                    self.emit_similar([p, t, aa], [p, bb, t], &reason, vec![]);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Basic proportionality (Thales' / intercept theorem). A line `DE ∥ BC` with
    /// `D` on `AB` and `E` on `AC` gives `△ADE ~ △ABC`, hence
    /// `AD:AB = AE:AC = DE:BC`.
    fn derive_thales(&mut self) {
        let n = self.names.len() as PointId;
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    if a == b || a == c || b == c {
                        continue;
                    }
                    for d in 0..n {
                        for e in 0..n {
                            if [a, b, c].contains(&d) || [a, b, c, d].contains(&e) {
                                continue;
                            }
                            if !(self.coll(a, d, b)
                                && self.coll(a, e, c)
                                && self.all_parallel(d, e, b, c))
                            {
                                continue;
                            }
                            let Some(why) = self.certify(&[pred("para", &[d, e, b, c])]) else {
                                continue;
                            };
                            let reason = format!(
                                "{de} ∥ {bc} (derived below), so ∠{a}{d}{e} = ∠{a}{b}{c} and \
                                 ∠{a}{e}{d} = ∠{a}{c}{b} (corresponding angles){}",
                                derivation_block(&why),
                                de = self.seg(d, e),
                                bc = self.seg(b, c),
                                a = self.nm(a),
                                b = self.nm(b),
                                c = self.nm(c),
                                d = self.nm(d),
                                e = self.nm(e),
                            );
                            self.cite(
                                &[(a, d), (a, c)],
                                &[(a, e), (a, b)],
                                format!(
                                    "By the basic proportionality (intercept) theorem, since \
                                     {de} ∥ {bc}: {ad}·{ac} = {ae}·{ab}.",
                                    de = self.seg(d, e),
                                    bc = self.seg(b, c),
                                    ad = self.seg(a, d),
                                    ac = self.seg(a, c),
                                    ae = self.seg(a, e),
                                    ab = self.seg(a, b),
                                ),
                            );
                            self.emit_similar([a, d, e], [a, b, c], &reason, vec![]);
                        }
                    }
                }
            }
        }
    }

    // === cited lemmas (headline; blocked only when they would be the goal) ==

    /// The angle-bisector theorem, **derived** (not cited). If `AD` bisects
    /// `∠BAC` with `D` on `BC`, drop perpendiculars `B′, C′` from `B, C` onto the
    /// bisector line `AD`. Then `△BDB′ ~ △CDC′` gives `BD:DC = BB′:CC′`, and
    /// `△ABB′ ~ △ACC′` (equal angles `∠BAB′ = ∠CAC′` because `AD` bisects) gives
    /// `BB′:CC′ = AB:AC`; hence `BD:DC = AB:AC`.
    fn derive_bisector(&mut self) {
        let n = self.names.len() as PointId;
        let mut found: Vec<(PointId, PointId, PointId, PointId, Vec<String>)> = Vec::new();
        for a in 0..n {
            for b in 0..n {
                for c in (b + 1)..n {
                    if a == b || a == c {
                        continue;
                    }
                    for d in 0..n {
                        if [a, b, c].contains(&d) || !self.coll(b, d, c) {
                            continue;
                        }
                        let looks_bisected = (0..self.insts.len()).all(|i| {
                            let x = self.angle(i, b, a, d);
                            let y = self.angle(i, d, a, c);
                            x.is_finite() && y.is_finite() && (x - y).abs() < 1e-6 && x > 1e-4
                        });
                        if !looks_bisected || !self.between(b, d, c) {
                            continue;
                        }
                        let Some(why) = self.certify(&[pred("eqangle", &[a, b, a, d, a, d, a, c])])
                        else {
                            continue;
                        };
                        found.push((a, b, c, d, why));
                    }
                }
            }
        }
        for (a, b, c, d, why) in found {
            self.cite(
                &[(b, d), (a, c)],
                &[(d, c), (a, b)],
                format!(
                    "By the angle-bisector theorem ({ad} bisects ∠{b}{a}{c}), \
                     {bd}·{ac} = {dc}·{ab}.",
                    ad = self.seg(a, d),
                    a = self.nm(a),
                    b = self.nm(b),
                    c = self.nm(c),
                    bd = self.seg(b, d),
                    ac = self.seg(a, c),
                    dc = self.seg(d, c),
                    ab = self.seg(a, b),
                ),
            );
            self.drop_group += 1;
            let fb = self.add_perp_foot(b, a, d);
            let fc = self.add_perp_foot(c, a, d);
            let intro = self.push(
                format!(
                    "{ad} bisects ∠{b}{a}{c} (derived below). Drop perpendiculars from {b}, {c} \
                     onto the bisector {ad}, with feet {fb}, {fc}.{}",
                    derivation_block(&why),
                    ad = self.seg(a, d),
                    a = self.nm(a),
                    b = self.nm(b),
                    c = self.nm(c),
                    fb = self.nm(fb),
                    fc = self.nm(fc),
                ),
                None,
                vec![],
                false,
            );
            let r1 = format!(
                "∠{b}{d}{fb} = ∠{c}{d}{fc} ({rel} at {d}) and the right angles at {fb}, {fc}",
                rel = self.angle_at(b, d, c),
                b = self.nm(b),
                c = self.nm(c),
                d = self.nm(d),
                fb = self.nm(fb),
                fc = self.nm(fc),
            );
            self.emit_similar([b, d, fb], [c, d, fc], &r1, vec![intro]);
            let r2 = format!(
                "∠{b}{a}{fb} = ∠{c}{a}{fc} (since {a}{d} bisects ∠{b}{a}{c}) and the \
                 right angles at {fb}, {fc}",
                a = self.nm(a),
                b = self.nm(b),
                c = self.nm(c),
                d = self.nm(d),
                fb = self.nm(fb),
                fc = self.nm(fc),
            );
            self.emit_similar([a, b, fb], [a, c, fc], &r2, vec![intro]);
        }
    }

    /// Append the foot of the perpendicular from `a` onto line `l1 l2` as a fresh
    /// auxiliary point, in *every* instance, named with the current drop group's
    /// subscript (`A₁`, `A₂`, …). Returns its id.
    fn add_perp_foot(&mut self, a: PointId, l1: PointId, l2: PointId) -> PointId {
        let name = format!("{}{}", self.nm(a), subscript(self.drop_group));
        let id = self.names.len() as PointId;
        self.names.push(name);
        let mut fresh = true;
        for inst in &mut self.insts {
            let (pa, p1, p2) = (inst[a as usize], inst[l1 as usize], inst[l2 as usize]);
            let dir = p2 - p1;
            let t = (pa - p1).dot(dir) / dir.dot(dir).max(1e-18);
            let foot = p1 + dir * t;
            fresh &= inst.iter().all(|q| (*q - foot).norm() > 1e-6);
            inst.push(foot);
        }
        if fresh {
            self.preds.push(pred("coll", &[l1, l2, id]));
            self.preds.push(pred("perp", &[a, id, l1, l2]));
        }
        id
    }

    /// Menelaus's theorem, **derived** (not cited). For a transversal of triangle
    /// `ABC` meeting the side-lines at `D∈BC, E∈CA, F∈AB`, drop perpendiculars
    /// from `A,B,C` to the transversal (feet `A′,B′,C′`). Three pairs of similar
    /// right triangles give `BD:DC = BB′:CC′`, `CE:EA = CC′:AA′`, `AF:FB =
    /// AA′:BB′`; multiplying, the perpendicular lengths cancel and
    /// `BD·CE·AF = DC·EA·FB`.
    fn derive_menelaus(&mut self) {
        for (a, b, c, d, e, f) in self.side_cevian_triples() {
            if !self.coll(d, e, f) {
                continue; // transversal: D, E, F collinear
            }
            self.cite(
                &[(b, d), (c, e), (a, f)],
                &[(d, c), (e, a), (f, b)],
                format!(
                    "By Menelaus's theorem for the transversal {d}{e}{f} of triangle {a}{b}{c}, \
                     {bd}·{ce}·{af} = {dc}·{ea}·{fb}.",
                    a = self.nm(a),
                    b = self.nm(b),
                    c = self.nm(c),
                    d = self.nm(d),
                    e = self.nm(e),
                    f = self.nm(f),
                    bd = self.seg(b, d),
                    ce = self.seg(c, e),
                    af = self.seg(a, f),
                    dc = self.seg(d, c),
                    ea = self.seg(e, a),
                    fb = self.seg(f, b),
                ),
            );
            self.drop_group += 1;
            let (fa, fb, fc) = (
                self.add_perp_foot(a, d, e),
                self.add_perp_foot(b, d, e),
                self.add_perp_foot(c, d, e),
            );
            let intro = self.push(
                format!(
                    "Drop perpendiculars from {a}, {b}, {c} onto the transversal {d}{e}{f}, \
                     with feet {fa}, {fb}, {fc}.",
                    a = self.nm(a),
                    b = self.nm(b),
                    c = self.nm(c),
                    d = self.nm(d),
                    e = self.nm(e),
                    f = self.nm(f),
                    fa = self.nm(fa),
                    fb = self.nm(fb),
                    fc = self.nm(fc),
                ),
                None,
                vec![],
                false,
            );
            let vertical = |f: &Figure, x: PointId, p: PointId, fx: PointId, y: PointId, fy: PointId| {
                format!(
                    "∠{x}{p}{fx} = ∠{y}{p}{fy} ({rel} at {p}) and the right angles at \
                     {fx}, {fy}",
                    rel = f.angle_at(x, p, y),
                    x = f.nm(x),
                    p = f.nm(p),
                    fx = f.nm(fx),
                    y = f.nm(y),
                    fy = f.nm(fy),
                )
            };
            let rd = vertical(self, b, d, fb, c, fc);
            self.emit_similar([b, d, fb], [c, d, fc], &rd, vec![intro]);
            let re = vertical(self, c, e, fc, a, fa);
            self.emit_similar([c, e, fc], [a, e, fa], &re, vec![intro]);
            let rf = vertical(self, a, f, fa, b, fb);
            self.emit_similar([a, f, fa], [b, f, fb], &rf, vec![intro]);
        }
    }

    /// Triangles `ABC` with side-points `D∈BC, E∈CA, F∈AB` (all collinearities
    /// asserted), the common shape of Menelaus and Ceva.
    fn side_cevian_triples(
        &self,
    ) -> Vec<(PointId, PointId, PointId, PointId, PointId, PointId)> {
        let n = self.names.len() as PointId;
        let mut out = Vec::new();
        for a in 0..n {
            for b in (a + 1)..n {
                for c in (b + 1)..n {
                    if self.numerically_collinear(a, b, c) {
                        continue;
                    }
                    for d in 0..n {
                        if [a, b, c].contains(&d) || !self.coll(b, d, c) {
                            continue;
                        }
                        for e in 0..n {
                            if [a, b, c, d].contains(&e) || !self.coll(c, e, a) {
                                continue;
                            }
                            for f in 0..n {
                                if [a, b, c, d, e].contains(&f) || !self.coll(a, f, b) {
                                    continue;
                                }
                                out.push((a, b, c, d, e, f));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn numerically_collinear(&self, a: PointId, b: PointId, c: PointId) -> bool {
        let (u, v) = (self.coord(b) - self.coord(a), self.coord(c) - self.coord(a));
        (u.x * v.y - u.y * v.x).abs() < 1e-9 * (1.0 + u.norm() * v.norm())
    }

    /// Are lines `ab` and `cd` parallel in *every* instance (a construction-fixed
    /// fact — robust to the parallel being asserted through a helper point)?
    fn all_parallel(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> bool {
        (0..self.insts.len()).all(|i| {
            let u = self.insts[i][a as usize] - self.insts[i][b as usize];
            let v = self.insts[i][c as usize] - self.insts[i][d as usize];
            let (un, vn) = (u.norm(), v.norm());
            un > 1e-9 && vn > 1e-9 && (u.x * v.y - u.y * v.x).abs() < 1e-9 * (1.0 + un * vn)
        })
    }

    fn apply(&mut self) {
        self.emit_congruent();
        self.emit_absolute();
        self.derive_geometric_mean();
        self.derive_power_of_a_point();
        self.derive_tangent_secant();
        self.derive_thales();
        self.derive_bisector();
        self.derive_menelaus();
    }

    // === solving ===========================================================

    fn prove_with(&self, goal: &LEq, allowed: &BTreeSet<usize>) -> Option<BTreeSet<usize>> {
        let mut rows: Vec<(LEq, LKey, BTreeSet<usize>)> = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            if !allowed.contains(&i) {
                continue;
            }
            let Some(eq0) = &step.eq else { continue };
            let mut eq = eq0.clone();
            let mut deps: BTreeSet<usize> = BTreeSet::new();
            deps.insert(i);
            reduce(&mut eq, &rows, &mut deps);
            if let Some(pivot) = eq.first_atom() {
                rows.push((eq, pivot, deps));
            }
        }
        let mut g = goal.clone();
        let mut deps: BTreeSet<usize> = BTreeSet::new();
        reduce(&mut g, &rows, &mut deps);
        (g.is_zero()).then_some(deps)
    }

    fn prove(&self, goal: &LEq) -> Option<BTreeSet<usize>> {
        let all: BTreeSet<usize> = (0..self.steps.len())
            .filter(|&i| self.steps[i].eq.is_some())
            .collect();
        let mut used = self.prove_with(goal, &all)?;
        // If a lone headline citation carries the proof, retry without headlines
        // to force an elementary derivation (anti-circularity).
        if used.len() == 1 && self.steps[*used.iter().next().unwrap()].headline {
            let elementary: BTreeSet<usize> =
                all.iter().copied().filter(|&i| !self.steps[i].headline).collect();
            used = self.prove_with(goal, &elementary)?;
        }
        // Greedy minimisation.
        loop {
            let mut removed = false;
            for &s in used.clone().iter() {
                let mut trial = used.clone();
                trial.remove(&s);
                if let Some(smaller) = self.prove_with(goal, &trial) {
                    used = smaller;
                    removed = true;
                    break;
                }
            }
            if !removed {
                break;
            }
        }
        if used.len() == 1 && self.steps[*used.iter().next().unwrap()].headline {
            return None;
        }
        Some(used)
    }

    fn render(&self, used: &BTreeSet<usize>, goal_text: &str, goal_val: f64) -> String {
        let mut keep: BTreeSet<usize> = BTreeSet::new();
        let mut stack: Vec<usize> = used.iter().copied().collect();
        while let Some(i) = stack.pop() {
            if keep.insert(i) {
                stack.extend(self.steps[i].premises.iter().copied());
            }
        }
        // Drop the pure-equation steps that carry no prose (their justification is
        // the similar-triangle intro they cite); keep intros and cited lemmas.
        let mut order: Vec<usize> = keep.iter().copied().filter(|&i| !self.steps[i].text.is_empty()).collect();
        order.sort_by_key(|&i| (!self.steps[i].lead, i));
        let trig = keep.iter().any(|&i| {
            self.steps[i]
                .eq
                .as_ref()
                .is_some_and(|e| e.terms.keys().any(|k| matches!(k, LKey::Sin(..))))
        });
        let number: BTreeMap<usize, usize> =
            order.iter().enumerate().map(|(k, &i)| (i, k + 1)).collect();

        let mut out = String::new();
        out.push_str("EUCLIDEAN PROOF (ratios)\n");
        out.push_str(&format!("  Goal:  {goal_text}\n\n"));
        for (k, &i) in order.iter().enumerate() {
            let cites: Vec<String> = self.steps[i]
                .premises
                .iter()
                .filter_map(|p| number.get(p).map(|k| k.to_string()))
                .collect();
            let refs = if cites.is_empty() {
                String::new()
            } else {
                format!("  [from {}]", cites.join(", "))
            };
            out.push_str(&format!("  {}. {}\n", k + 1, with_refs(&self.steps[i].text, &refs)));
        }
        out.push_str(&format!(
            "\n  {} gives {goal_text}  (= {}). ∎\n",
            if trig {
                "Multiplying the sine relations above"
            } else {
                "Combining the proportions above"
            },
            crate::synthetic::pretty_len(goal_val)
        ));
        out
    }
}

/// `△XYZ` name for a triangle.
fn tri(f: &Figure, t: [PointId; 3]) -> String {
    format!("△{}{}{}", f.nm(t[0]), f.nm(t[1]), f.nm(t[2]))
}

/// Unicode subscript digits of `n` (`1 → ₁`, `12 → ₁₂`), for aux-foot names.
fn subscript(n: usize) -> String {
    const D: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];
    n.to_string()
        .chars()
        .map(|c| D[c.to_digit(10).unwrap() as usize])
        .collect()
}

fn reduce(eq: &mut LEq, rows: &[(LEq, LKey, BTreeSet<usize>)], deps: &mut BTreeSet<usize>) {
    for (req, pivot, rdeps) in rows {
        if let Some(c) = eq.terms.get(pivot).cloned() {
            if c.is_zero() {
                continue;
            }
            let factor = &c / req.terms.get(pivot).unwrap();
            eq.sub_scaled(req, &factor);
            deps.extend(rdeps.iter().copied());
        }
    }
}

// ===========================================================================
// Goal lowering: monomial (product/ratio) expression → log-linear equation
// ===========================================================================

/// Lower one side of the goal to `(Σ coeff·L_atom, Σ eₚ·ln p)` = `ln(side)`, or
/// `None` if it is not a monomial in lengths (a sum, an area, an angle, …).
fn llower(e: &MExpr, fig: &Figure) -> Option<LEq> {
    match e {
        MExpr::Num(v) => {
            let r = rat_of(*v)?;
            let mut eq = LEq::default();
            for (p, exp) in ln_primes(&r)? {
                eq.add_prime(p, exp);
            }
            Some(eq)
        }
        MExpr::Dist(a, b) => {
            let (a, b) = (fig.pt(a)?, fig.pt(b)?);
            let mut eq = LEq::default();
            eq.add_term(latom(a, b), Rat::one());
            Some(eq)
        }
        MExpr::Mul(x, y) => {
            let mut a = llower(x, fig)?;
            let b = llower(y, fig)?;
            a.sub_scaled(&b, &-Rat::one());
            Some(a)
        }
        MExpr::Div(x, y) => {
            let mut a = llower(x, fig)?;
            let b = llower(y, fig)?;
            a.sub_scaled(&b, &Rat::one());
            Some(a)
        }
        MExpr::Pow(base, p) => {
            let r = rat_of(*p)?;
            let mut a = llower(base, fig)?;
            scale(&mut a, &r);
            Some(a)
        }
        MExpr::Sqrt(x) => {
            let mut a = llower(x, fig)?;
            scale(&mut a, &Rat::new(1, 2));
            Some(a)
        }
        _ => None,
    }
}

fn scale(eq: &mut LEq, k: &Rat) {
    for c in eq.terms.values_mut() {
        *c = &*c * k;
    }
    for c in eq.primes.values_mut() {
        *c = &*c * k;
    }
}

impl Figure {
    fn pt(&self, name: &str) -> Option<PointId> {
        self.names.iter().position(|n| n == name).map(|i| i as PointId)
    }
}

/// Numerically evaluate a monomial length expression (for the closing value).
fn eval_numeric(e: &MExpr, fig: &Figure) -> Option<f64> {
    Some(match e {
        MExpr::Num(v) => *v,
        MExpr::Dist(a, b) => fig.dist(0, fig.pt(a)?, fig.pt(b)?),
        MExpr::Mul(x, y) => eval_numeric(x, fig)? * eval_numeric(y, fig)?,
        MExpr::Div(x, y) => eval_numeric(x, fig)? / eval_numeric(y, fig)?,
        MExpr::Pow(b, p) => eval_numeric(b, fig)?.powf(*p),
        MExpr::Sqrt(x) => eval_numeric(x, fig)?.max(0.0).sqrt(),
        MExpr::Add(x, y) => eval_numeric(x, fig)? + eval_numeric(y, fig)?,
        MExpr::Sub(x, y) => eval_numeric(x, fig)? - eval_numeric(y, fig)?,
        MExpr::Neg(x) => -eval_numeric(x, fig)?,
        MExpr::Sin(x) => match &**x {
            MExpr::Angle(a, b, c) => fig.angle(0, fig.pt(a)?, fig.pt(b)?, fig.pt(c)?).sin(),
            _ => return None,
        },
        _ => return None,
    })
}

// ===========================================================================
// General sum-of-products engine (degree-2 length identities) — NO per-theorem
// code. Fed by *generically* detected similar triangles + collinear segment
// sums + congruences, and by the auxiliary-point search below.
// ===========================================================================

type PAtom = Mono;
fn pmul(a: LAtom, b: LAtom) -> PAtom {
    if a <= b {
        vec![a, b]
    } else {
        vec![b, a]
    }
}

#[derive(Clone, Default)]
struct PEq {
    terms: BTreeMap<Mono, Rat>,
}
impl PEq {
    fn is_zero(&self) -> bool {
        self.terms.values().all(Rat::is_zero)
    }
    fn add(&mut self, a: PAtom, c: Rat) {
        let cur = self.terms.get(&a).cloned().unwrap_or_else(Rat::zero);
        let s = &cur + &c;
        if s.is_zero() {
            self.terms.remove(&a);
        } else {
            self.terms.insert(a, s);
        }
    }
    fn sub_scaled(&mut self, o: &PEq, f: &Rat) {
        for (a, c) in &o.terms {
            self.add(a.clone(), -&(c * f));
        }
    }
    fn first(&self) -> Option<PAtom> {
        self.terms.iter().find(|(_, c)| !c.is_zero()).map(|(a, _)| a.clone())
    }
}

struct PStep {
    text: String,
    eq: Option<PEq>,
    premises: Vec<usize>,
    /// Printed before the other steps (the shared angle derivation).
    lead: bool,
}

type Mono = Vec<LAtom>;
fn mono_scale(m: &BTreeMap<Mono, Rat>, k: &Rat) -> BTreeMap<Mono, Rat> {
    m.iter().map(|(a, c)| (a.clone(), c * k)).collect()
}
fn mono_add(a: &BTreeMap<Mono, Rat>, b: &BTreeMap<Mono, Rat>, f: &Rat) -> BTreeMap<Mono, Rat> {
    let mut out = a.clone();
    for (m, c) in b {
        let cur = out.get(m).cloned().unwrap_or_else(Rat::zero);
        let s = &cur + &(c * f);
        if s.is_zero() {
            out.remove(m);
        } else {
            out.insert(m.clone(), s);
        }
    }
    out
}
/// Expansion budget for [`mono_mul`]: a product of long sums (or a high power
/// of one) multiplies term counts, so cap the work per product and give up
/// (`None` — the goal is then simply not handled here) beyond it.
const MAX_MONO_PRODUCT: usize = 1 << 14;

fn mono_mul(a: &BTreeMap<Mono, Rat>, b: &BTreeMap<Mono, Rat>) -> Option<BTreeMap<Mono, Rat>> {
    if a.len().saturating_mul(b.len()) > MAX_MONO_PRODUCT {
        return None;
    }
    let mut out: BTreeMap<Mono, Rat> = BTreeMap::new();
    for (ma, ca) in a {
        for (mb, cb) in b {
            let mut m = ma.clone();
            m.extend_from_slice(mb);
            m.sort();
            let cur = out.get(&m).cloned().unwrap_or_else(Rat::zero);
            let s = &cur + &(ca * cb);
            if s.is_zero() {
                out.remove(&m);
            } else {
                out.insert(m, s);
            }
        }
    }
    Some(out)
}
fn mono_lower(e: &MExpr, fig: &Figure) -> Option<BTreeMap<Mono, Rat>> {
    let unit = |m: Mono, c: Rat| {
        let mut b = BTreeMap::new();
        if !c.is_zero() {
            b.insert(m, c);
        }
        b
    };
    match e {
        MExpr::Num(v) => Some(unit(vec![], rat_of(*v)?)),
        MExpr::Dist(a, b) => {
            let (a, b) = (fig.pt(a)?, fig.pt(b)?);
            Some(unit(vec![latom(a, b)], Rat::one()))
        }
        MExpr::Neg(x) => Some(mono_scale(&mono_lower(x, fig)?, &-Rat::one())),
        MExpr::Add(x, y) => Some(mono_add(&mono_lower(x, fig)?, &mono_lower(y, fig)?, &Rat::one())),
        MExpr::Sub(x, y) => Some(mono_add(&mono_lower(x, fig)?, &mono_lower(y, fig)?, &-Rat::one())),
        MExpr::Mul(x, y) => mono_mul(&mono_lower(x, fig)?, &mono_lower(y, fig)?),
        MExpr::Div(x, y) => {
            let b = mono_lower(y, fig)?;
            if b.len() != 1 {
                return None;
            }
            let (m, c) = b.iter().next()?;
            if !m.is_empty() || c.is_zero() {
                return None;
            }
            Some(mono_scale(&mono_lower(x, fig)?, &c.recip()))
        }
        MExpr::Pow(base, p) => {
            let n = *p;
            if !(0.0..=crate::metric::MAX_EXPONENT).contains(&n) || (n - n.round()).abs() > 1e-9 {
                return None;
            }
            let n = n.round() as u32;
            let b = mono_lower(base, fig)?;
            let mut acc = unit(vec![], Rat::one());
            for _ in 0..n {
                acc = mono_mul(&acc, &b)?;
            }
            Some(acc)
        }
        _ => None,
    }
}

impl Figure {
    fn ppush(&mut self, text: String, eq: Option<PEq>, premises: Vec<usize>) -> usize {
        self.psteps.push(PStep {
            text,
            eq,
            premises,
            lead: false,
        });
        self.psteps.len() - 1
    }

    /// Propose the three product equalities of similar `t1 = (p,q,r) ~
    /// t2 = (u,v,w)` when the triangles look similar in every instance. The
    /// step is only a candidate: [`Figure::prove_products`] keeps it only if the
    /// DDAR closure derives the two angle equalities, and prints that derivation.
    fn psim(&mut self, t1: [PointId; 3], t2: [PointId; 3]) {
        if !self.similar_in_all(t1, t2) {
            return;
        }
        let Some(angles) = self.similarity_angles(t1, t2) else {
            return;
        };
        let [p, q, r] = t1;
        let [u, v, w] = t2;
        let intro = self.ppush(
            format!(
                "∠{}{}{} = ∠{}{}{} and ∠{}{}{} = ∠{}{}{} (derived from the hypotheses), so {} ~ {} (AA): {}·{} = {}·{}, {}·{} = {}·{}, {}·{} = {}·{}.",
                self.nm(r), self.nm(p), self.nm(q), self.nm(w), self.nm(u), self.nm(v),
                self.nm(p), self.nm(q), self.nm(r), self.nm(u), self.nm(v), self.nm(w),
                tri(self, t1), tri(self, t2),
                self.seg(p, q), self.seg(v, w), self.seg(u, v), self.seg(q, r),
                self.seg(q, r), self.seg(w, u), self.seg(v, w), self.seg(r, p),
                self.seg(p, q), self.seg(w, u), self.seg(u, v), self.seg(r, p),
            ),
            None,
            vec![],
        );
        self.pending.push((intro, angles.to_vec(), false));
        let (pq, qr, rp) = (latom(p, q), latom(q, r), latom(r, p));
        let (uv, vw, wu) = (latom(u, v), latom(v, w), latom(w, u));
        for (x, y) in [
            (pmul(pq, vw), pmul(uv, qr)),
            (pmul(qr, wu), pmul(vw, rp)),
            (pmul(pq, wu), pmul(uv, rp)),
        ] {
            let mut e = PEq::default();
            e.add(x, Rat::one());
            e.add(y, -Rat::one());
            if !e.is_zero() {
                self.ppush(String::new(), Some(e), vec![intro]);
            }
        }
    }

    /// Detect *every* similar-triangle pair among `pts` (numeric AA gate) and emit
    /// their product relations. Bounded by |pts|, so callers pass a small set.
    fn gather_generic_similar(&mut self, pts: &[PointId]) {
        let tris: Vec<[PointId; 3]> = {
            let mut v = Vec::new();
            for &a in pts {
                for &b in pts {
                    for &c in pts {
                        if a != b && b != c && a != c && !self.numerically_collinear(a, b, c) {
                            v.push([a, b, c]);
                        }
                    }
                }
            }
            v
        };
        let mut seen: BTreeSet<([PointId; 3], [PointId; 3])> = BTreeSet::new();
        for i in 0..tris.len() {
            for j in (i + 1)..tris.len() {
                let (t1, t2) = (tris[i], tris[j]);
                let mut key = [t1, t2];
                key.sort();
                if !seen.insert((key[0], key[1])) {
                    continue;
                }
                self.psim(t1, t2);
            }
        }
    }

    /// The angle-bisector theorem as a product relation, `BD·AC = DC·AB`, for
    /// every bisector the figure suggests; it is used only once DDAR derives
    /// `D` on `BC` and `∠BAD = ∠DAC`.
    fn gather_bisector_products(&mut self) {
        let n = self.names.len() as PointId;
        for a in 0..n {
            for b in 0..n {
                for c in (b + 1)..n {
                    if a == b || a == c {
                        continue;
                    }
                    for d in 0..n {
                        if [a, b, c].contains(&d)
                            || !self.numerically_collinear(b, d, c)
                            || !self.between(b, d, c)
                        {
                            continue;
                        }
                        let bisects = (0..self.insts.len()).all(|i| {
                            let x = self.angle(i, b, a, d);
                            let y = self.angle(i, d, a, c);
                            x.is_finite() && y.is_finite() && (x - y).abs() < 1e-6 && x > 1e-4
                        });
                        if !bisects {
                            continue;
                        }
                        let intro = self.ppush(
                            format!(
                                "{ad} bisects ∠{b}{a}{c} with {d} on {bc} (derived from the \
                                 hypotheses), so by the angle-bisector theorem {bd}·{ac} = {dc}·{ab}.",
                                ad = self.seg(a, d),
                                a = self.nm(a),
                                b = self.nm(b),
                                c = self.nm(c),
                                d = self.nm(d),
                                bc = self.seg(b, c),
                                bd = self.seg(b, d),
                                ac = self.seg(a, c),
                                dc = self.seg(d, c),
                                ab = self.seg(a, b),
                            ),
                            None,
                            vec![],
                        );
                        self.pending.push((
                            intro,
                            vec![
                                pred("coll", &[b, d, c]),
                                pred("eqangle", &[a, b, a, d, a, d, a, c]),
                            ],
                            true,
                        ));
                        let mut e = PEq::default();
                        e.add(pmul(latom(b, d), latom(a, c)), Rat::one());
                        e.add(pmul(latom(d, c), latom(a, b)), -Rat::one());
                        if !e.is_zero() {
                            self.ppush(String::new(), Some(e), vec![intro]);
                        }
                    }
                }
            }
        }
    }

    /// Collinear splits (`Y on XZ` ⇒ `XY·L + YZ·L = XZ·L` for each goal length L)
    /// and congruences (`|ab| = |cd|` ⇒ `ab·L = cd·L`).
    fn gather_product_relations(&mut self, goal_segs: &BTreeSet<LAtom>) {
        let mut splits: Vec<(PointId, PointId, PointId)> = Vec::new();
        let mut derived_splits: Vec<(PointId, PointId, PointId)> = Vec::new();
        let n = self.names.len() as PointId;
        for x in 0..n {
            for z in (x + 1)..n {
                for y in 0..n {
                    if y == x || y == z || self.coll(x, y, z) {
                        continue;
                    }
                    if self.numerically_collinear(x, y, z) && self.between(x, y, z) {
                        derived_splits.push((x, y, z));
                    }
                }
            }
        }
        for set in self.colls.clone() {
            for i in 0..set.len() {
                for j in 0..set.len() {
                    for k in 0..set.len() {
                        if i == j || j == k || i == k {
                            continue;
                        }
                        let (x, y, z) = (set[i], set[j], set[k]);
                        if x >= z {
                            continue;
                        }
                        let between = (0..self.insts.len()).all(|q| {
                            let (px, py, pz) = (
                                self.insts[q][x as usize],
                                self.insts[q][y as usize],
                                self.insts[q][z as usize],
                            );
                            (py - px).dot(pz - py) > 1e-9
                                && (px - pz).norm() > (px - py).norm().max((py - pz).norm())
                        });
                        if between {
                            splits.push((x, y, z));
                        }
                    }
                }
            }
        }
        for (x, y, z) in derived_splits {
            let intro = self.ppush(
                format!(
                    "{}, {}, {} are collinear (derived from the hypotheses), with {} between {} and {}.",
                    self.nm(x),
                    self.nm(y),
                    self.nm(z),
                    self.nm(y),
                    self.nm(x),
                    self.nm(z)
                ),
                None,
                vec![],
            );
            self.pending.push((intro, vec![pred("coll", &[x, y, z])], false));
            for &l in goal_segs {
                let mut e = PEq::default();
                e.add(pmul(latom(x, y), l), Rat::one());
                e.add(pmul(latom(y, z), l), Rat::one());
                e.add(pmul(latom(x, z), l), -Rat::one());
                if e.is_zero() {
                    continue;
                }
                self.ppush(
                    format!(
                        "So {xy} + {yz} = {xz}; times {l}: {xy}·{l} + {yz}·{l} = {xz}·{l}.",
                        xz = self.seg(x, z),
                        xy = self.seg(x, y),
                        yz = self.seg(y, z),
                        l = self.seg(l.0, l.1),
                    ),
                    Some(e),
                    vec![intro],
                );
            }
        }
        for (x, y, z) in splits {
            for &l in goal_segs {
                let mut e = PEq::default();
                e.add(pmul(latom(x, y), l), Rat::one());
                e.add(pmul(latom(y, z), l), Rat::one());
                e.add(pmul(latom(x, z), l), -Rat::one());
                if e.is_zero() {
                    continue;
                }
                self.ppush(
                    format!(
                        "{y} lies on {xz}, so {xy} + {yz} = {xz}; times {l}: {xy}·{l} + {yz}·{l} = {xz}·{l}.",
                        y = self.nm(y),
                        xz = self.seg(x, z),
                        xy = self.seg(x, y),
                        yz = self.seg(y, z),
                        l = self.seg(l.0, l.1),
                    ),
                    Some(e),
                    vec![],
                );
            }
        }
        for cg in self.congs.clone() {
            let (s1, s2) = (latom(cg[0], cg[1]), latom(cg[2], cg[3]));
            if s1 == s2 {
                continue;
            }
            for &l in goal_segs {
                let mut e = PEq::default();
                e.add(pmul(s1, l), Rat::one());
                e.add(pmul(s2, l), -Rat::one());
                if e.is_zero() {
                    continue;
                }
                self.ppush(
                    format!(
                        "|{}| = |{}|, so |{}|·{l} = |{}|·{l}.",
                        self.seg(s1.0, s1.1),
                        self.seg(s2.0, s2.1),
                        self.seg(s1.0, s1.1),
                        self.seg(s2.0, s2.1),
                        l = self.seg(l.0, l.1),
                    ),
                    Some(e),
                    vec![],
                );
            }
        }
    }

    fn pprove_with(&self, goal: &PEq, allowed: &BTreeSet<usize>) -> Option<BTreeSet<usize>> {
        let mut rows: Vec<(PEq, PAtom, BTreeSet<usize>)> = Vec::new();
        let reduce = |eq: &mut PEq, rows: &[(PEq, PAtom, BTreeSet<usize>)], deps: &mut BTreeSet<usize>| {
            for (req, piv, rdeps) in rows {
                if let Some(cf) = eq.terms.get(piv).cloned() {
                    if cf.is_zero() {
                        continue;
                    }
                    let factor = &cf / req.terms.get(piv).unwrap();
                    eq.sub_scaled(req, &factor);
                    deps.extend(rdeps.iter().copied());
                }
            }
        };
        for (i, step) in self.psteps.iter().enumerate() {
            if !allowed.contains(&i) {
                continue;
            }
            let Some(eq0) = &step.eq else { continue };
            let mut eq = eq0.clone();
            let mut deps: BTreeSet<usize> = BTreeSet::new();
            deps.insert(i);
            reduce(&mut eq, &rows, &mut deps);
            if let Some(piv) = eq.first() {
                rows.push((eq, piv, deps));
            }
        }
        let mut g = goal.clone();
        let mut deps: BTreeSet<usize> = BTreeSet::new();
        reduce(&mut g, &rows, &mut deps);
        g.is_zero().then_some(deps)
    }

    fn pprove(&self, goal: &PEq, all: &BTreeSet<usize>) -> Option<BTreeSet<usize>> {
        let mut used = self.pprove_with(goal, all)?;
        loop {
            let mut removed = false;
            for &s in used.clone().iter() {
                let mut trial = used.clone();
                trial.remove(&s);
                if let Some(smaller) = self.pprove_with(goal, &trial) {
                    used = smaller;
                    removed = true;
                    break;
                }
            }
            if !removed {
                break;
            }
        }
        Some(used)
    }

    fn prender(&self, used: &BTreeSet<usize>, goal_text: &str, goal_val: f64, aux: &[String]) -> String {
        let mut keep: BTreeSet<usize> = BTreeSet::new();
        let mut stack: Vec<usize> = used.iter().copied().collect();
        while let Some(i) = stack.pop() {
            if keep.insert(i) {
                stack.extend(self.psteps[i].premises.iter().copied());
            }
        }
        let mut order: Vec<usize> = keep
            .iter()
            .copied()
            .filter(|&i| !self.psteps[i].text.is_empty())
            .collect();
        order.sort_by_key(|&i| (!self.psteps[i].lead, i));
        let number: BTreeMap<usize, usize> = order
            .iter()
            .enumerate()
            .map(|(k, &i)| (i, k + 1 + aux.len()))
            .collect();
        let mut out = String::new();
        out.push_str("EUCLIDEAN PROOF (sum of products)\n");
        out.push_str(&format!("  Goal:  {goal_text}\n\n"));
        for (k, a) in aux.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", k + 1, a));
        }
        for (k, &i) in order.iter().enumerate() {
            let cites: Vec<String> = self.psteps[i]
                .premises
                .iter()
                .filter_map(|p| number.get(p).map(|k| k.to_string()))
                .collect();
            let refs = if cites.is_empty() {
                String::new()
            } else {
                format!("  [from {}]", cites.join(", "))
            };
            out.push_str(&format!(
                "  {}. {}\n",
                k + 1 + aux.len(),
                with_refs(&self.psteps[i].text, &refs)
            ));
        }
        out.push_str(&format!(
            "\n  Adding the relations above gives {goal_text}  (= {}). ∎\n",
            crate::synthetic::pretty_len(goal_val)
        ));
        out
    }

    /// Solve `goal` with the product steps, using a similar-triangle step only if
    /// DDAR derives its angle equalities. Uncertifiable candidates are dropped
    /// and the solve retried; certified steps get their derivation appended.
    fn certified_products(&mut self, goal: &PEq) -> Option<BTreeSet<usize>> {
        let intro_of: BTreeMap<usize, (Vec<Predicate>, bool)> = self
            .pending
            .iter()
            .map(|(i, goals, headline)| (*i, (goals.clone(), *headline)))
            .collect();
        let mut rejected: BTreeSet<usize> = BTreeSet::new();
        let mut certified: BTreeMap<usize, Vec<crate::proof::FactId>> = BTreeMap::new();
        for _ in 0..64 {
            let allowed: BTreeSet<usize> = (0..self.psteps.len())
                .filter(|&i| {
                    self.psteps[i].eq.is_some()
                        && !self.psteps[i].premises.iter().any(|p| rejected.contains(p))
                })
                .collect();
            let used = self.pprove(goal, &allowed)?;
            // Anti-circularity: one cited named theorem standing alone merely
            // restates the goal.
            let lone: Vec<usize> = used
                .iter()
                .flat_map(|&i| self.psteps[i].premises.iter().copied())
                .filter(|p| intro_of.get(p).is_some_and(|(_, h)| *h))
                .collect();
            if used.len() == 1 && lone.len() == 1 {
                rejected.insert(lone[0]);
                continue;
            }
            let intros: BTreeSet<usize> = used
                .iter()
                .flat_map(|&i| self.psteps[i].premises.iter().copied())
                .filter(|p| intro_of.contains_key(p))
                .collect();
            let mut all_ok = true;
            for intro in intros {
                if certified.contains_key(&intro) {
                    continue;
                }
                let goals = &intro_of[&intro].0;
                let why = self.ddar.borrow_mut().derive_deps(
                    &self.names,
                    &self.insts[0],
                    &self.preds,
                    self.hyps,
                    goals,
                );
                match why {
                    Some(deps) => {
                        certified.insert(intro, deps);
                    }
                    None => {
                        rejected.insert(intro);
                        all_ok = false;
                    }
                }
            }
            if all_ok {
                let used_intros: BTreeSet<usize> = used
                    .iter()
                    .flat_map(|&i| self.psteps[i].premises.iter().copied())
                    .filter(|p| certified.contains_key(p))
                    .collect();
                if !used_intros.is_empty() {
                    let mut deps: Vec<crate::proof::FactId> = used_intros
                        .iter()
                        .flat_map(|i| certified[i].iter().copied())
                        .collect();
                    deps.sort_unstable();
                    deps.dedup();
                    let lines = self.ddar.borrow().lines(&deps);
                    let lead = self.ppush(
                        format!(
                            "Facts derived from the hypotheses by the deductive closure:{}",
                            derivation_block(&lines)
                        ),
                        None,
                        vec![],
                    );
                    self.psteps[lead].lead = true;
                    for intro in used_intros {
                        self.psteps[intro].premises.push(lead);
                    }
                }
                return Some(used);
            }
        }
        None
    }

    /// Try to prove a homogeneous degree-2 length identity from the CURRENT
    /// figure (plus any aux already added). `relevant` bounds similar-triangle
    /// detection; `aux_prose` prefixes the proof with the aux constructions.
    fn prove_products(
        &mut self,
        lhs: &MExpr,
        rhs: &MExpr,
        goal_text: &str,
        relevant: &[PointId],
        aux_prose: &[String],
        t2: bool,
    ) -> Option<String> {
        let goal = self.homogeneous_goal(lhs, rhs)?;
        let degree = goal.terms.keys().next()?.len();
        if degree != 2 && !t2 {
            return None;
        }
        self.psteps.clear();
        self.pending.clear();
        let mut used = None;
        if degree == 2 {
            let goal_segs: BTreeSet<LAtom> = goal.terms.keys().flatten().copied().collect();
            self.gather_generic_similar(relevant);
            self.gather_bisector_products();
            self.gather_product_relations(&goal_segs);
            used = self.certified_products(&goal);
        }
        if used.is_none() && t2 {
            let mut pool: BTreeSet<Mono> = goal.terms.keys().cloned().collect();
            self.gather_cofactor_substitutions(&mut pool);
            used = self.certified_products(&goal);
        }
        let used = used?;
        if used.is_empty() {
            return None;
        }
        let val = eval_numeric(lhs, self).unwrap_or(f64::NAN);
        Some(self.prender(&used, goal_text, val, aux_prose))
    }

    /// `lhs − rhs` as a product equation when every monomial has the same
    /// positive degree.
    fn homogeneous_goal(&self, lhs: &MExpr, rhs: &MExpr) -> Option<PEq> {
        let lm = mono_lower(lhs, self)?;
        let rm = mono_lower(rhs, self)?;
        let goal_map = mono_add(&lm, &rm, &-Rat::one());
        let degree = goal_map.keys().next()?.len();
        if degree == 0 || !goal_map.keys().all(|m| m.len() == degree) {
            return None;
        }
        let mut goal = PEq::default();
        for (m, c) in goal_map {
            goal.add(m, c);
        }
        Some(goal)
    }
}

// ===========================================================================
// General auxiliary-point search (metric). No per-theorem code: a library of
// standard constructions is tried; each adds a point (coords in every instance
// + symbolic facts) and the sum-of-products engine retries. Finds aux-requiring
// proofs (e.g. Ptolemy's point on a diagonal) generically.
// ===========================================================================

#[derive(Clone)]
enum AuxFact {
    Coll(PointId, PointId, PointId),
    Perp([PointId; 4]),
    Cong([PointId; 4]),
    Cyclic(Vec<PointId>),
    /// `dir(p0p1) − dir(p2p3) = dir(p4p5) − dir(p6p7)` (directed, mod π).
    EqAngle([PointId; 8]),
}

struct AuxCand {
    name: String,
    coords: Vec<Vec2>,
    facts: Vec<AuxFact>,
    intro: String,
}

fn collect_points_m(e: &MExpr, out: &mut BTreeSet<String>) {
    match e {
        MExpr::Dist(a, b) => {
            out.insert(a.clone());
            out.insert(b.clone());
        }
        MExpr::Angle(a, b, c) | MExpr::Area(a, b, c) => {
            out.insert(a.clone());
            out.insert(b.clone());
            out.insert(c.clone());
        }
        MExpr::Neg(x) | MExpr::Sqrt(x) | MExpr::Sin(x) | MExpr::Cos(x) | MExpr::Tan(x) | MExpr::Pow(x, _) => {
            collect_points_m(x, out)
        }
        MExpr::Add(x, y) | MExpr::Sub(x, y) | MExpr::Mul(x, y) | MExpr::Div(x, y) => {
            collect_points_m(x, out);
            collect_points_m(y, out);
        }
        MExpr::Num(_) => {}
    }
}

/// The construction's user-named points among the first `n` (not the `_k`
/// helpers the compiler introduces, nor feet dropped later).
fn named_points(fig: &Figure, n: usize) -> Vec<PointId> {
    (0..n as PointId).filter(|&p| !fig.names[p as usize].starts_with('_')).collect()
}

fn goal_points(lhs: &MExpr, rhs: &MExpr, fig: &Figure) -> BTreeSet<PointId> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    collect_points_m(lhs, &mut names);
    collect_points_m(rhs, &mut names);
    names.iter().filter_map(|n| fig.pt(n)).collect()
}

impl Figure {
    /// A structural copy (facts + instances) with no accumulated proof steps —
    /// the substrate for trying an auxiliary point.
    fn lean_clone(&self) -> Figure {
        Figure {
            names: self.names.clone(),
            insts: self.insts.clone(),
            concyclic: self.concyclic.clone(),
            circles: self.circles.clone(),
            colls: self.colls.clone(),
            perps: self.perps.clone(),
            paras: self.paras.clone(),
            congs: self.congs.clone(),
            abs_len: self.abs_len.clone(),
            steps: Vec::new(),
            psteps: Vec::new(),
            drop_group: 0,
            preds: self.preds.clone(),
            hyps: self.hyps,
            ddar: RefCell::new(Certifier::default()),
            pending: Vec::new(),
        }
    }
    fn add_fact(&mut self, f: &AuxFact) {
        match f {
            AuxFact::Coll(a, b, c) => {
                self.colls.push(vec![*a, *b, *c]);
                self.preds.push(pred("coll", &[*a, *b, *c]));
            }
            AuxFact::Perp(p) => {
                self.perps.push(*p);
                self.preds.push(pred("perp", p));
            }
            AuxFact::Cong(p) => {
                self.congs.push(*p);
                self.preds.push(pred("cong", p));
            }
            AuxFact::Cyclic(v) => {
                self.concyclic.push(v.iter().copied().collect());
                self.preds.push(pred("cyclic", v));
            }
            AuxFact::EqAngle(p) => self.preds.push(pred("eqangle", p)),
        }
    }

    /// Compute an aux point's coordinates in every instance via `f`, rejecting it
    /// if it is undefined, non-finite, or coincides with an existing point.
    fn build_cand(
        &self,
        name: String,
        intro: String,
        facts: Vec<AuxFact>,
        f: impl Fn(&[Vec2]) -> Option<Vec2>,
    ) -> Option<AuxCand> {
        let mut coords = Vec::with_capacity(self.insts.len());
        for inst in &self.insts {
            let c = f(inst)?;
            if !c.x.is_finite() || !c.y.is_finite() || inst.iter().any(|q| (*q - c).norm() < 1e-6) {
                return None;
            }
            coords.push(c);
        }
        Some(AuxCand {
            name,
            coords,
            facts,
            intro,
        })
    }

    /// The library of candidate auxiliary points, built around the goal points.
    /// Generation stops at [`MAX_AUX_CANDIDATES`] usable candidates (the
    /// search never looks past them) or [`MAX_AUX_ATTEMPTS`] tries — the
    /// angle-copy family alone is O(g⁶) in the goal's point count.
    fn aux_candidates(&self, g: &[PointId]) -> Vec<AuxCand> {
        let mut out = Vec::new();
        let mut tried = 0usize;
        let nm = |p: PointId| self.names[p as usize].clone();
        let seg = |a: PointId, b: PointId| format!("{}{}", nm(a), nm(b));
        macro_rules! push {
            ($cand:expr) => {{
                tried += 1;
                if let Some(c) = $cand {
                    out.push(c);
                }
                if out.len() >= MAX_AUX_CANDIDATES || tried >= MAX_AUX_ATTEMPTS {
                    return out;
                }
            }};
        }
        // Only one auxiliary point is ever added at a time, so every candidate is
        // simply named "K".
        let kid = self.names.len() as PointId;
        // reflect p over o  →  Coll(p,o,K), Cong(o,p,o,K)
        for &p in g {
            for &o in g {
                if p == o {
                    continue;
                }
                push!(self.build_cand(
                    "K".to_string(),
                    format!("Let {} be the reflection of {} in {}.", "K", nm(p), nm(o)),
                    vec![AuxFact::Coll(p, o, kid), AuxFact::Cong([o, p, o, kid])],
                    move |i| Some(i[o as usize] * 2.0 - i[p as usize]),
                ));
            }
        }
        // midpoint of a goal pair  →  Coll(a,K,b), Cong(K,a,K,b)
        for (ai, &a) in g.iter().enumerate() {
            for &b in &g[ai + 1..] {
                push!(self.build_cand(
                    "K".to_string(),
                    format!("Let {} be the midpoint of {}.", "K", seg(a, b)),
                    vec![AuxFact::Coll(a, kid, b), AuxFact::Cong([kid, a, kid, b])],
                    move |i| Some((i[a as usize] + i[b as usize]) * 0.5),
                ));
            }
        }
        // foot of the perpendicular from p to line ab  →  Coll(a,K,b), Perp(p,K,a,b)
        for &p in g {
            for (ai, &a) in g.iter().enumerate() {
                for &b in &g[ai + 1..] {
                    if p == a || p == b {
                        continue;
                    }
                    push!(self.build_cand(
                        "K".to_string(),
                        format!("Let {} be the foot of the perpendicular from {} to {}.", "K", nm(p), seg(a, b)),
                        vec![AuxFact::Coll(a, kid, b), AuxFact::Perp([p, kid, a, b])],
                        move |i| {
                            let (pa, va, vb) = (i[p as usize], i[a as usize], i[b as usize]);
                            let d = vb - va;
                            let t = (pa - va).dot(d) / d.dot(d).max(1e-18);
                            Some(va + d * t)
                        },
                    ));
                }
            }
        }
        // circumcentre of a goal triple  →  Cong(K,a,K,b), Cong(K,b,K,c)
        for (ai, &a) in g.iter().enumerate() {
            for (bi, &b) in g.iter().enumerate().skip(ai + 1) {
                for &c in &g[bi + 1..] {
                    push!(self.build_cand(
                        "K".to_string(),
                        format!("Let {} be the circumcentre of {}{}{}.", "K", nm(a), nm(b), nm(c)),
                        vec![AuxFact::Cong([kid, a, kid, b]), AuxFact::Cong([kid, b, kid, c])],
                        move |i| NumCircle::through(i[a as usize], i[b as usize], i[c as usize]).map(|circ| circ.center),
                    ));
                }
            }
        }
        // intersection of lines ab, cd  →  Coll(a,K,b), Coll(c,K,d)
        for (ai, &a) in g.iter().enumerate() {
            for &b in &g[ai + 1..] {
                for (ci, &c) in g.iter().enumerate() {
                    for &d in &g[ci + 1..] {
                        if [a, b].contains(&c) || [a, b].contains(&d) {
                            continue;
                        }
                        push!(self.build_cand(
                            "K".to_string(),
                            format!("Let {} be the intersection of {} and {}.", "K", seg(a, b), seg(c, d)),
                            vec![AuxFact::Coll(a, kid, b), AuxFact::Coll(c, kid, d)],
                            move |i| {
                                intersect_ll(
                                    &NumLine::through(i[a as usize], i[b as usize]),
                                    &NumLine::through(i[c as usize], i[d as usize]),
                                )
                            },
                        ));
                    }
                }
            }
        }
        // second intersection of line vr with a named circle → Coll(v,r,K), Cyclic(set+K)
        for set in &self.concyclic {
            let on: Vec<PointId> = set.iter().copied().collect();
            if on.len() < 3 {
                continue;
            }
            let (o0, o1, o2) = (on[0], on[1], on[2]);
            for &v in g {
                for &r in &on {
                    if v == r {
                        continue;
                    }
                    let mut cyc: Vec<PointId> = on.clone();
                    cyc.push(kid);
                    push!(self.build_cand(
                        "K".to_string(),
                        format!("Let {} be the second meet of line {} with the circle.", "K", seg(v, r)),
                        vec![AuxFact::Coll(v, r, kid), AuxFact::Cyclic(cyc)],
                        move |i| {
                            let circ =
                                NumCircle::through(i[o0 as usize], i[o1 as usize], i[o2 as usize])?;
                            second_meet(i[v as usize], i[r as usize], circ.center, circ.r, i[r as usize])
                        },
                    ));
                }
            }
        }
        // angle-copy point on line st with ∠(ray from v) = ∠pvq measured from vr.
        for &v in g {
            for &r in g {
                if r == v {
                    continue;
                }
                for &p in g {
                    for &q in g {
                        if p == q || p == v || q == v {
                            continue;
                        }
                        for (si, &s) in g.iter().enumerate() {
                            for &t in &g[si + 1..] {
                                if s == v && t == v {
                                    continue;
                                }
                                for sgn in [1.0f64, -1.0] {
                                    let Some(copied) = self.copied_angle(v, r, p, q, kid, sgn) else {
                                        continue;
                                    };
                                    push!(self.build_cand(
                                        "K".to_string(),
                                        format!(
                                            "Let {} be the point on {} with ∠{}{}{} = ∠{}{}{}.",
                                            "K", seg(s, t), nm(r), nm(v), "K", nm(p), nm(v), nm(q)
                                        ),
                                        vec![AuxFact::Coll(s, kid, t), copied],
                                        move |i| angle_copy(i, [v, r, p, q, s, t], sgn),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        for cand in self.centred_second_meets(g) {
            push!(Some(cand));
        }
        out
    }

    /// The directed-angle fact defining an angle-copy point `K` (the ray `vK` is
    /// `vr` turned by `sgn·∠pvq`): `∠(vr, vK) = ∠(vp, vq)` or `∠(vq, vp)`,
    /// whichever orientation `p, q` have about `v` — the same in every instance,
    /// or no fact (and no candidate) at all.
    fn copied_angle(
        &self,
        v: PointId,
        r: PointId,
        p: PointId,
        q: PointId,
        k: PointId,
        sgn: f64,
    ) -> Option<AuxFact> {
        let turn = |i: &Vec<Vec2>| {
            let (dp, dq) = (i[p as usize] - i[v as usize], i[q as usize] - i[v as usize]);
            (dp.x * dq.y - dp.y * dq.x).signum()
        };
        let first = turn(&self.insts[0]);
        if first == 0.0 || self.insts.iter().any(|i| turn(i) != first) {
            return None;
        }
        Some(if first == sgn {
            AuxFact::EqAngle([v, k, v, r, v, q, v, p])
        } else {
            AuxFact::EqAngle([v, k, v, r, v, p, v, q])
        })
    }

    /// Try to prove the goal by introducing one auxiliary point from the library.
    fn aux_search(&self, lhs: &MExpr, rhs: &MExpr, goal: &str) -> Option<String> {
        let mut gnames: BTreeSet<String> = BTreeSet::new();
        collect_points_m(lhs, &mut gnames);
        collect_points_m(rhs, &mut gnames);
        let gpts: Vec<PointId> = gnames.iter().filter_map(|n| self.pt(n)).collect();
        if gpts.len() < 3 {
            return None;
        }
        // Only worth searching for a homogeneous degree-2 length identity (which
        // is what the sum-of-products engine proves). Bail fast otherwise.
        let degree2 = mono_lower(lhs, self)
            .and_then(|l| mono_lower(rhs, self).map(|r| mono_add(&l, &r, &-Rat::one())))
            .map(|g| !g.is_empty() && g.keys().all(|m| m.len() == 2))
            .unwrap_or(false);
        if !degree2 {
            return None;
        }
        let kid = self.names.len() as PointId;
        let mut candidates = self.aux_candidates(&gpts);
        candidates.truncate(MAX_AUX_CANDIDATES); // keep the search bounded
        for cand in candidates {
            let mut f = self.lean_clone();
            f.names.push(cand.name.clone());
            for (i, inst) in f.insts.iter_mut().enumerate() {
                inst.push(cand.coords[i]);
            }
            for fact in &cand.facts {
                f.add_fact(fact);
            }
            let mut rel = gpts.clone();
            rel.push(kid);
            let intro = cand.intro.replace('K', &cand.name);
            if let Some(p) = f.prove_products(lhs, rhs, goal, &rel, std::slice::from_ref(&intro), false) {
                return Some(p);
            }
        }
        None
    }
}

const MAX_AUX_CANDIDATES: usize = 4000;
const MAX_AUX_ATTEMPTS: usize = 200_000;

/// The second intersection of line `ab` with the circle `(o, r)`, avoiding the
/// point `known` (one intersection we already have).
fn second_meet(a: Vec2, b: Vec2, o: Vec2, r: f64, known: Vec2) -> Option<Vec2> {
    let d = b - a;
    let dd = d.dot(d);
    if dd < 1e-18 {
        return None;
    }
    let f = a - o;
    let (bq, c) = (2.0 * f.dot(d), f.dot(f) - r * r);
    let disc = bq * bq - 4.0 * dd * c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let (t1, t2) = ((-bq + sq) / (2.0 * dd), (-bq - sq) / (2.0 * dd));
    let (p1, p2) = (a + d * t1, a + d * t2);
    if (p1 - known).norm() < (p2 - known).norm() {
        Some(p2)
    } else {
        Some(p1)
    }
}

/// The point where the ray from `v`, obtained by rotating `v→r` by `sgn·∠pvq`,
/// meets line `st`. `pts = [v, r, p, q, s, t]`.
fn angle_copy(i: &[Vec2], pts: [PointId; 6], sgn: f64) -> Option<Vec2> {
    let [v, r, p, q, s, t] = pts;
    let vv = i[v as usize];
    let dr = i[r as usize] - vv;
    if dr.norm() < 1e-9 {
        return None;
    }
    let (dp, dq) = (i[p as usize] - vv, i[q as usize] - vv);
    let n = dp.norm() * dq.norm();
    if n < 1e-12 {
        return None;
    }
    let theta = (dp.dot(dq) / n).clamp(-1.0, 1.0).acos() * sgn;
    let (c, sn) = (theta.cos(), theta.sin());
    let dir = Vec2::new(dr.x * c - dr.y * sn, dr.x * sn + dr.y * c);
    intersect_ll(
        &NumLine::through(vv, vv + dir),
        &NumLine::through(i[s as usize], i[t as usize]),
    )
}

/// The sampled figure plus up to four more independent instances. They gate
/// the numeric detections: a similarity or parallelism must hold in every
/// one, so a rule fires only on a construction-general fact, never a
/// single-instance coincidence.
fn sampled_instances(cons_src: &str) -> Result<(SampledFigure, Vec<Vec<Vec2>>), String> {
    let sampled = crate::geo::build_sampled_figure(cons_src)?;
    let mut insts: Vec<Vec<Vec2>> = vec![sampled.coords.clone()];
    if let Ok(more) = crate::geo::build_instances(cons_src, 4) {
        let names = &sampled.names;
        for inst in more {
            let map: std::collections::HashMap<&str, Vec2> =
                inst.iter().map(|(k, v)| (k.as_str(), *v)).collect();
            if let Some(v) = names
                .iter()
                .map(|n| map.get(n.as_str()).copied())
                .collect::<Option<Vec<Vec2>>>()
            {
                insts.push(v);
            }
        }
    }
    Ok((sampled, insts))
}

/// Attempt a classical Euclidean proof of a **multiplicative** length goal.
pub fn prove_ratio(cons_src: &str, goal: &str) -> Result<Outcome, String> {
    let (lhs, rhs) = crate::metric::parse_equation(goal)?;
    let (sampled, insts) = sampled_instances(cons_src)?;

    let mut fig = Figure::gather(&sampled, insts.clone());
    fig.apply();

    // General metric hypotheses imposed by `point:` constraints become given
    // equations — so an arbitrary relation the user imposed can be *used* here.
    let mut hyp_steps: Vec<(String, LEq)> = Vec::new();
    for (l, r) in &sampled.metric_hyps {
        if let (Some(le), Some(re)) = (llower(l, &fig), llower(r, &fig)) {
            let mut eq = le;
            eq.sub_scaled(&re, &Rat::one());
            if !eq.is_zero() {
                hyp_steps.push((
                    format!("Given: {} = {} (hypothesis).", l.to_display(), r.to_display()),
                    eq,
                ));
            }
        }
    }
    for (text, eq) in hyp_steps {
        fig.push(text, Some(eq), vec![], false);
    }

    let goal_val = eval_numeric(&lhs, &fig).unwrap_or(f64::NAN);

    // 1. Monomial (product/ratio/power) ratio engine.
    let mut mono_goal = None;
    if let (Some(l), Some(r)) = (llower(&lhs, &fig), llower(&rhs, &fig)) {
        let mut goal_eq = l;
        goal_eq.sub_scaled(&r, &Rat::one());
        if let Some(used) = fig.prove(&goal_eq) {
            if !used.is_empty() {
                return Ok(Outcome::Proved(fig.render(&used, goal, goal_val)));
            }
        }
        mono_goal = Some(goal_eq);
    }
    let mut s1_fig = fig;

    // 2. General sum-of-products engine (degree-2 identities) — with the general
    //    auxiliary-point search for the aux-requiring cases (Ptolemy, …).
    //    A fresh figure: the feet the monomial engine dropped are not part of
    //    this proof, so they must not reach its facts or its derivations.
    let mut fig = Figure::gather(&sampled, insts.clone());
    let relevant: Vec<PointId> = (0..fig.names.len() as PointId).collect();
    if let Some(proof) = fig.prove_products(&lhs, &rhs, goal, &relevant, &[], true) {
        return Ok(Outcome::Proved(proof));
    }
    if let Some(proof) = fig.aux_search(&lhs, &rhs, goal) {
        return Ok(Outcome::Proved(proof));
    }

    // 3. Trigonometry, only once every trig-free stage has failed.
    //    S2: the S1 figure plus sine rows over the construction's own points.
    let pts = named_points(&s1_fig, sampled.names.len());
    let goal_pts = goal_points(&lhs, &rhs, &s1_fig);
    if let Some(goal_eq) = &mono_goal {
        if let Some(used) = s1_fig.prove_trig_log(goal_eq, &pts, &goal_pts) {
            return Ok(Outcome::Proved(s1_fig.render(&used, goal, goal_val)));
        }
    }

    Ok(Outcome::Unhandled(
        "no chain of the known ratio theorems reaches the goal".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proved(cons: &str, goal: &str) -> bool {
        match prove_ratio(cons, goal) {
            Ok(Outcome::Proved(p)) => {
                assert!(p.contains("EUCLIDEAN PROOF"));
                true
            }
            Ok(Outcome::Unhandled(e)) => {
                eprintln!("unhandled: {e}");
                false
            }
            Err(e) => {
                eprintln!("error: {e}");
                false
            }
        }
    }

    #[test]
    fn aux_candidate_generation_is_bounded() {
        // Ten goal points: the angle-copy family alone would be ~580k candidates.
        let names: Vec<String> = (0..10).map(|i| format!("P{i}")).collect();
        let cons: String = names.iter().map(|n| format!("{n} = free\n")).collect();
        let sampled = crate::geo::build_sampled_figure(&cons).expect("build");
        let fig = Figure::gather(&sampled, vec![sampled.coords.clone()]);
        let g: Vec<PointId> = (0..names.len() as PointId).collect();
        let cands = fig.aux_candidates(&g);
        assert!(cands.len() <= MAX_AUX_CANDIDATES, "{} candidates", cands.len());
    }

    #[test]
    fn geometric_mean_altitude() {
        // Right angle at C, H foot of the altitude to AB ⇒ CH² = AH·HB.
        assert!(proved(
            "A = free\nB = free\nC = point: perp(C,A,C,B)\nH = foot(C, line(A,B))",
            "dist(C,H)^2 = dist(A,H)*dist(H,B)"
        ));
    }

    #[test]
    fn power_of_a_point_two_chords() {
        // Two chords of one circle meeting at P ⇒ PA·PB = PC·PD.
        assert!(proved(
            "O = free\nR = free\nA = on_circle(O, R)\nB = on_circle(O, R)\n\
             C = on_circle(O, R)\nD = on_circle(O, R)\n\
             P = meet(line(A,B), line(C,D))",
            "dist(P,A)*dist(P,B) = dist(P,C)*dist(P,D)"
        ));
    }

    #[test]
    fn basic_proportionality_thales() {
        // DE ∥ BC with D on AB, E on AC ⇒ AD·AC = AE·AB.
        assert!(proved(
            "A = free\nB = free\nC = free\nD = on_line(A, B)\n\
             E = meet(para_line(D, line(B,C)), line(A,C))",
            "dist(A,D)*dist(A,C) = dist(A,E)*dist(A,B)"
        ));
    }

    #[test]
    fn angle_bisector_theorem() {
        // AD bisects ∠BAC, D on BC ⇒ BD·AC = DC·AB.
        assert!(proved(
            "A = free\nB = free\nC = free\nD = meet(bisector(B,A,C), line(B,C))",
            "dist(B,D)*dist(A,C) = dist(D,C)*dist(A,B)"
        ));
    }

    #[test]
    fn tangent_secant_power() {
        // PT tangent, PAS secant ⇒ PT² = PA·PS.
        assert!(proved(
            "O = free\nA = on_circle(O, free)\nP = free\n\
             T1 T2 = tangent(P, circle(O, A))\nS = meet(line(P, A), circle(O, A))",
            "dist(P,T1)^2 = dist(P,A)*dist(P,S)"
        ));
    }

    #[test]
    fn menelaus_transversal() {
        // Transversal DEF of triangle ABC ⇒ BD·CE·AF = DC·EA·FB (derived).
        assert!(proved(
            "A = free\nB = free\nC = free\nF = on_line(A, B)\nE = on_line(A, C)\n\
             D = meet(line(E, F), line(B, C))",
            "dist(B,D)*dist(C,E)*dist(A,F) = dist(D,C)*dist(E,A)*dist(F,B)"
        ));
    }

    #[test]
    fn ceva_concurrent_cevians() {
        // Concurrent cevians ⇒ BD·CE·AF = DC·EA·FB (derived via Menelaus).
        assert!(proved(
            "A = free\nB = free\nC = free\nP = on_line(A, midpoint(B,C))\n\
             D = meet(line(A, P), line(B, C))\nE = meet(line(B, P), line(C, A))\n\
             F = meet(line(C, P), line(A, B))",
            "dist(B,D)*dist(C,E)*dist(A,F) = dist(D,C)*dist(E,A)*dist(F,B)"
        ));
    }

    #[test]
    fn ptolemy_via_general_aux_search() {
        // A generic (isosceles-trapezoid) cyclic quadrilateral. The general
        // auxiliary-point search must DISCOVER the classic construction and prove
        // AC·BD = AB·CD + AD·BC — with no per-theorem code.
        assert!(proved(
            "A B C = triangle\nD = reflect(C, perp_bisector(A, B))",
            "dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D) + dist(A,D)*dist(B,C)"
        ));
    }

    #[test]
    fn rejects_false_ratio() {
        assert!(!proved(
            "A = free\nB = free\nC = free\nD = free",
            "dist(A,B)*dist(C,D) = dist(A,C)*dist(B,D)"
        ));
    }

    #[test]
    fn rejects_false_sum_of_products() {
        // A false degree-2 identity on a cyclic quad must NOT be "proved" by the
        // aux search (the similarity gates reject spurious relations).
        assert!(!proved(
            "A B C = triangle\nD = reflect(C, perp_bisector(A, B))",
            "dist(A,C)*dist(B,D) = dist(A,B)*dist(C,D)"
        ));
    }
}
