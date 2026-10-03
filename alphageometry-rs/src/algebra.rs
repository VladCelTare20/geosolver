//! Full **synthetic-by-coordinates** proofs of *metric* goals — statements about
//! concrete quantities (specific lengths, angles, areas, sums of squares,
//! products of lengths) that lie outside DDAR's angle/ratio/linear-length
//! algebra and so previously received only a *numerical* certificate.
//!
//! This module proves such goals **exactly**, and prints a real proof, by the
//! classical mechanical-geometry method (Wu Wen-Tsün's characteristic-set /
//! pseudo-division algorithm):
//!
//! 1. Place the figure in the coordinate plane. Each point gets two coordinate
//!    variables; the construction fixes some as polynomial functions of the
//!    earlier ones (dependent), the rest are free parameters (the shape's
//!    degrees of freedom).
//! 2. Every hypothesis (collinear, equal length, perpendicular, concyclic, an
//!    absolute length, …) and the goal become **polynomial equations** over the
//!    exact rationals [`Rat`].
//! 3. Because the figure is *constructed*, the hypotheses already form an
//!    ascending (triangular) chain: each dependent coordinate is the leading
//!    variable of one hypothesis. We eliminate the dependent variables from the
//!    goal one by one with pseudo-division. If the goal reduces to the zero
//!    polynomial, it is an identity on the construction's variety — a proof —
//!    valid wherever the recorded *nondegeneracy conditions* (the leading
//!    coefficients we divided by) are nonzero.
//!
//! Unlike the numerical certificate this is exact and it *explains* the result.
//! When a goal cannot be put in polynomial form (a transcendental angle sum, an
//! exotic function, a length that survives only to an odd power) the caller
//! falls back to the numerical certificate.

use std::collections::BTreeMap;

use crate::geo::{AlgFigure, PointDef};
use crate::metric::MExpr;
use crate::predicate::PointId;
use crate::rational::Rat;

// ===========================================================================
// Monomials
// ===========================================================================

/// A monomial: the variables (with positive exponents) it is a product of,
/// kept sorted ascending by variable index. The empty vector is the constant 1.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
struct Mono(Vec<(u32, u32)>);

impl Mono {
    fn one() -> Mono {
        Mono(Vec::new())
    }

    fn var(v: u32) -> Mono {
        Mono(vec![(v, 1)])
    }

    /// Exponent of variable `v` in this monomial (0 if absent).
    fn deg(&self, v: u32) -> u32 {
        self.0
            .iter()
            .find(|&&(x, _)| x == v)
            .map(|&(_, e)| e)
            .unwrap_or(0)
    }

    /// Product of two monomials (add exponents of shared variables).
    fn mul(&self, other: &Mono) -> Mono {
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(self.0.len() + other.0.len());
        let (mut i, mut j) = (0, 0);
        while i < self.0.len() && j < other.0.len() {
            let (av, ae) = self.0[i];
            let (bv, be) = other.0[j];
            match av.cmp(&bv) {
                std::cmp::Ordering::Less => {
                    out.push((av, ae));
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    out.push((bv, be));
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    out.push((av, ae + be));
                    i += 1;
                    j += 1;
                }
            }
        }
        out.extend_from_slice(&self.0[i..]);
        out.extend_from_slice(&other.0[j..]);
        Mono(out)
    }

    /// This monomial with the exponent of `v` reduced by `k` (drops `v` if it
    /// reaches 0). Requires `deg(v) >= k`.
    fn drop_pow(&self, v: u32, k: u32) -> Mono {
        let mut out = Vec::with_capacity(self.0.len());
        for &(x, e) in &self.0 {
            if x == v {
                if e > k {
                    out.push((x, e - k));
                }
            } else {
                out.push((x, e));
            }
        }
        Mono(out)
    }

    /// Total degree.
    fn total_deg(&self) -> u32 {
        self.0.iter().map(|&(_, e)| e).sum()
    }

    fn eval(&self, vals: &[f64]) -> f64 {
        let mut p = 1.0;
        for &(v, e) in &self.0 {
            p *= vals[v as usize].powi(e as i32);
        }
        p
    }
}

// ===========================================================================
// Polynomials
// ===========================================================================

/// A multivariate polynomial over the exact rationals, as a map from monomial
/// to (nonzero) coefficient.
#[derive(Clone, Debug, Default)]
pub struct Poly {
    terms: BTreeMap<Mono, Rat>,
}

impl Poly {
    fn zero() -> Poly {
        Poly {
            terms: BTreeMap::new(),
        }
    }

    fn constant(r: Rat) -> Poly {
        let mut t = BTreeMap::new();
        if !r.is_zero() {
            t.insert(Mono::one(), r);
        }
        Poly { terms: t }
    }

    fn from_i64(n: i64) -> Poly {
        Poly::constant(Rat::from_int(n))
    }

    fn var(v: u32) -> Poly {
        let mut t = BTreeMap::new();
        t.insert(Mono::var(v), Rat::one());
        Poly { terms: t }
    }

    fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    fn n_terms(&self) -> usize {
        self.terms.len()
    }

    /// If this polynomial is a constant, return it (0 for the zero polynomial).
    fn as_const(&self) -> Option<Rat> {
        match self.terms.len() {
            0 => Some(Rat::zero()),
            1 => {
                let (m, c) = self.terms.iter().next().unwrap();
                if m.0.is_empty() {
                    Some(c.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn add_term(&mut self, m: Mono, c: Rat) {
        if c.is_zero() {
            return;
        }
        use std::collections::btree_map::Entry;
        match self.terms.entry(m) {
            Entry::Occupied(mut e) => {
                let s = e.get() + &c;
                if s.is_zero() {
                    e.remove();
                } else {
                    *e.get_mut() = s;
                }
            }
            Entry::Vacant(e) => {
                e.insert(c);
            }
        }
    }

    fn add(&self, other: &Poly) -> Poly {
        let mut out = self.clone();
        for (m, c) in &other.terms {
            out.add_term(m.clone(), c.clone());
        }
        out
    }

    fn sub(&self, other: &Poly) -> Poly {
        let mut out = self.clone();
        for (m, c) in &other.terms {
            out.add_term(m.clone(), -c.clone());
        }
        out
    }

    fn neg(&self) -> Poly {
        Poly {
            terms: self
                .terms
                .iter()
                .map(|(m, c)| (m.clone(), -c.clone()))
                .collect(),
        }
    }

    fn mul(&self, other: &Poly) -> Poly {
        let mut out = Poly::zero();
        for (ma, ca) in &self.terms {
            for (mb, cb) in &other.terms {
                out.add_term(ma.mul(mb), ca * cb);
            }
        }
        out
    }

    fn scale(&self, r: &Rat) -> Poly {
        if r.is_zero() {
            return Poly::zero();
        }
        Poly {
            terms: self.terms.iter().map(|(m, c)| (m.clone(), c * r)).collect(),
        }
    }

    fn mul_mono(&self, m: &Mono, c: &Rat) -> Poly {
        Poly {
            terms: self
                .terms
                .iter()
                .map(|(mm, cc)| (mm.mul(m), cc * c))
                .collect(),
        }
    }

    fn pow(&self, k: u32) -> Poly {
        let mut out = Poly::from_i64(1);
        for _ in 0..k {
            out = out.mul(self);
        }
        out
    }

    /// The highest variable index that appears, or `None` for a constant.
    fn max_var(&self) -> Option<u32> {
        self.terms
            .keys()
            .filter_map(|m| m.0.last().map(|&(v, _)| v))
            .max()
    }

    /// Degree in variable `v`.
    fn deg_in(&self, v: u32) -> u32 {
        self.terms.keys().map(|m| m.deg(v)).max().unwrap_or(0)
    }

    /// Coefficient of `v^k` — a polynomial not involving `v`.
    fn coeff_in(&self, v: u32, k: u32) -> Poly {
        let mut out = Poly::zero();
        for (m, c) in &self.terms {
            if m.deg(v) == k {
                out.add_term(m.drop_pow(v, k), c.clone());
            }
        }
        out
    }

    /// Leading coefficient in `v` (the "initial"): coefficient of `v^deg_v`.
    fn lead_in(&self, v: u32) -> Poly {
        self.coeff_in(v, self.deg_in(v))
    }

    fn eval(&self, vals: &[f64]) -> f64 {
        self.terms
            .iter()
            .map(|(m, c)| c.to_f64() * m.eval(vals))
            .sum()
    }

    /// A crude magnitude scale for relative comparisons.
    fn coeff_scale(&self) -> f64 {
        1.0 + self.terms.values().map(|c| c.to_f64().abs()).sum::<f64>()
    }

    /// Divide out the rational content so coefficients become coprime integers,
    /// curbing coefficient growth through the pseudo-division chain. Best-effort:
    /// leaves the polynomial unchanged if any coefficient does not fit in `i64`.
    fn primitive(&self) -> Poly {
        if self.terms.is_empty() {
            return self.clone();
        }
        let mut num_gcd: i64 = 0;
        let mut den_lcm: i64 = 1;
        for c in self.terms.values() {
            match (c.numer_i64(), c.denom_i64()) {
                (Some(n), Some(d)) => {
                    num_gcd = gcd_i64(num_gcd, n);
                    match lcm_i64(den_lcm, d) {
                        Some(l) => den_lcm = l,
                        None => return self.clone(),
                    }
                }
                _ => return self.clone(),
            }
        }
        if num_gcd == 0 {
            return self.clone();
        }
        // content = gcd(numerators) / lcm(denominators); divide it out.
        let content = Rat::new(num_gcd, den_lcm);
        self.scale(&content.recip())
    }
}

fn gcd_i64(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

fn lcm_i64(a: i64, b: i64) -> Option<i64> {
    if a == 0 || b == 0 {
        return Some(0);
    }
    let g = gcd_i64(a, b);
    (a / g).checked_mul(b.abs())
}

/// Pseudo-remainder of `f` modulo `g` treating `v` as the main variable:
/// `init(g)^k · f = q·g + prem`, with `deg_v(prem) < deg_v(g)`. Used to
/// eliminate `v` from `f` using the relation `g = 0`. Returns `f` unchanged when
/// `deg_v(g) = 0` (nothing to eliminate).
fn prem(f: &Poly, g: &Poly, v: u32) -> Poly {
    let dg = g.deg_in(v);
    if dg == 0 {
        return f.clone();
    }
    let init = g.lead_in(v);
    let mut r = f.clone();
    while !r.is_zero() && r.deg_in(v) >= dg {
        let dr = r.deg_in(v);
        let lead_r = r.lead_in(v); // coefficient of v^dr in r
        let mut shift = Mono::one();
        for _ in 0..(dr - dg) {
            shift = shift.mul(&Mono::var(v));
        }
        // r <- init·r − lead_r · v^(dr-dg) · g
        let r_scaled = r.mul(&init);
        let subtract = g.mul(&lead_r).mul_mono(&shift, &Rat::one());
        r = r_scaled.sub(&subtract);
    }
    r
}

// ===========================================================================
// Coordinate helpers (points → polynomial coordinates)
// ===========================================================================

fn vx(i: PointId) -> u32 {
    2 * i
}
fn vy(i: PointId) -> u32 {
    2 * i + 1
}

/// A 2-D vector of polynomials.
struct PVec(Poly, Poly);

fn coord(i: PointId, axis: u32) -> Poly {
    Poly::var(if axis == 0 { vx(i) } else { vy(i) })
}

/// Vector from point `a` to point `b`.
fn pvec(a: PointId, b: PointId) -> PVec {
    PVec(coord(b, 0).sub(&coord(a, 0)), coord(b, 1).sub(&coord(a, 1)))
}

fn pdot(u: &PVec, v: &PVec) -> Poly {
    u.0.mul(&v.0).add(&u.1.mul(&v.1))
}

fn pcross(u: &PVec, v: &PVec) -> Poly {
    u.0.mul(&v.1).sub(&u.1.mul(&v.0))
}

fn sq_dist(a: PointId, b: PointId) -> Poly {
    let d = pvec(a, b);
    pdot(&d, &d)
}

/// `x² + y²` of a point (for the concyclic determinant).
fn norm2(p: PointId) -> Poly {
    coord(p, 0)
        .mul(&coord(p, 0))
        .add(&coord(p, 1).mul(&coord(p, 1)))
}

/// Determinant of a small polynomial matrix (Laplace expansion), used at 4×4.
fn det(mat: &[Vec<Poly>]) -> Poly {
    let n = mat.len();
    if n == 1 {
        return mat[0][0].clone();
    }
    let mut acc = Poly::zero();
    for j in 0..n {
        let minor: Vec<Vec<Poly>> = mat[1..]
            .iter()
            .map(|row| {
                row.iter()
                    .enumerate()
                    .filter(|&(c, _)| c != j)
                    .map(|(_, p)| p.clone())
                    .collect()
            })
            .collect();
        let mut term = mat[0][j].mul(&det(&minor));
        if j % 2 == 1 {
            term = term.neg();
        }
        acc = acc.add(&term);
    }
    acc
}

/// Four-point concyclic condition `det[[|p|²,px,py,1]] = 0`.
fn concyclic4(a: PointId, b: PointId, c: PointId, d: PointId) -> Poly {
    let row = |p: PointId| vec![norm2(p), coord(p, 0), coord(p, 1), Poly::from_i64(1)];
    det(&[row(a), row(b), row(c), row(d)])
}

/// Convert a float to an exact rational if it is "nice" (integer or a fraction
/// with a small denominator); `None` otherwise.
fn rat_of(v: f64) -> Option<Rat> {
    // Beyond 2^53 an f64 is no longer an exact integer, and `as i64` would
    // saturate distinct values onto i64::MAX.
    if !v.is_finite() || v.abs() >= 9.0e15 {
        return None;
    }
    if (v - v.round()).abs() < 1e-9 {
        return Some(Rat::from_int(v.round() as i64));
    }
    for den in 2..=5040i64 {
        let n = v * den as f64;
        if (n - n.round()).abs() < 1e-9 {
            return Some(Rat::new(n.round() as i64, den));
        }
    }
    None
}

// ===========================================================================
// Lowering hypotheses
// ===========================================================================

/// A hypothesis polynomial with a human-readable label.
struct Hyp {
    poly: Poly,
    label: String,
}

/// Lower every predicate/scale hypothesis to labelled polynomials. A predicate
/// may yield several polynomials; unsupported ones are skipped (a weaker
/// hypothesis set is still sound — it can only make the goal harder to prove).
fn lower_hypotheses(fig: &AlgFigure) -> Vec<Hyp> {
    let mut out: Vec<Hyp> = Vec::new();
    let nm = |p: PointId| fig.names[p as usize].clone();
    for pred in &fig.preds {
        let p = &pred.points;
        match pred.name.as_str() {
            "coll" => {
                for k in 2..p.len() {
                    out.push(Hyp {
                        poly: pcross(&pvec(p[0], p[1]), &pvec(p[0], p[k])),
                        label: format!("{}, {}, {} collinear", nm(p[0]), nm(p[1]), nm(p[k])),
                    });
                }
            }
            "cong" if p.len() == 4 => {
                let poly = sq_dist(p[0], p[1]).sub(&sq_dist(p[2], p[3]));
                if !poly.is_zero() {
                    out.push(Hyp {
                        poly,
                        label: format!("|{}{}| = |{}{}|", nm(p[0]), nm(p[1]), nm(p[2]), nm(p[3])),
                    });
                }
            }
            "perp" if p.len() == 4 => out.push(Hyp {
                poly: pdot(&pvec(p[0], p[1]), &pvec(p[2], p[3])),
                label: format!("{}{} ⟂ {}{}", nm(p[0]), nm(p[1]), nm(p[2]), nm(p[3])),
            }),
            "para" if p.len() == 4 => out.push(Hyp {
                poly: pcross(&pvec(p[0], p[1]), &pvec(p[2], p[3])),
                label: format!("{}{} ∥ {}{}", nm(p[0]), nm(p[1]), nm(p[2]), nm(p[3])),
            }),
            "cyclic" => {
                for k in 3..p.len() {
                    out.push(Hyp {
                        poly: concyclic4(p[0], p[1], p[2], p[k]),
                        label: format!(
                            "{}, {}, {}, {} concyclic",
                            nm(p[0]),
                            nm(p[1]),
                            nm(p[2]),
                            nm(p[k])
                        ),
                    });
                }
            }
            "eqangle" if p.len() == 8 => {
                let (ab, cd) = (pvec(p[0], p[1]), pvec(p[2], p[3]));
                let (ef, gh) = (pvec(p[4], p[5]), pvec(p[6], p[7]));
                out.push(Hyp {
                    poly: pcross(&ab, &cd)
                        .mul(&pdot(&ef, &gh))
                        .sub(&pcross(&ef, &gh).mul(&pdot(&ab, &cd))),
                    label: "equal directed angles".to_string(),
                });
            }
            "eqratio" if p.len() == 8 => out.push(Hyp {
                poly: sq_dist(p[0], p[1])
                    .mul(&sq_dist(p[6], p[7]))
                    .sub(&sq_dist(p[4], p[5]).mul(&sq_dist(p[2], p[3]))),
                label: "equal ratios of lengths".to_string(),
            }),
            "aconst" if p.len() == 4 && !pred.constants.is_empty() => {
                let deg = pred.constants[0].to_f64();
                if let Some(poly) = aconst_poly(p[0], p[1], p[2], p[3], deg) {
                    out.push(Hyp {
                        poly,
                        label: format!("∠ = {deg}°"),
                    });
                }
            }
            "distmeq" => {
                if let Some(poly) = distmeq_poly(p, &pred.constants) {
                    out.push(Hyp {
                        poly,
                        label: "product-of-lengths relation".to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    for &(a, b, v) in &fig.scale {
        if let Some(rv) = rat_of(v) {
            out.push(Hyp {
                poly: sq_dist(a, b).sub(&Poly::constant(&rv * &rv)),
                label: format!("|{}{}| = {v}", nm(a), nm(b)),
            });
        }
    }

    // Replace the (multi-branch) predicate encoding of `reflect`/`shift` points
    // with their single-branch algebraic definitions: drop every predicate
    // polynomial whose leading point *is* such a point, then add the linear
    // defining equations. (A predicate's leading point is exactly the point it
    // defines, so this removes only the def-point's own constraints.)
    if !fig.defs.is_empty() {
        out.retain(|h| {
            h.poly
                .max_var()
                .map_or(true, |mv| !fig.defs.contains_key(&(mv / 2)))
        });
        let mut defs: Vec<(&PointId, &PointDef)> = fig.defs.iter().collect();
        defs.sort_by_key(|(id, _)| **id);
        for (&id, def) in defs {
            match def {
                PointDef::Affine(weights) => {
                    let terms: Vec<String> = weights
                        .iter()
                        .map(|(p, w)| format!("({w})·{}", nm(*p)))
                        .collect();
                    let label = format!("{} = {}", nm(id), terms.join(" + "));
                    for axis in [0u32, 1] {
                        let mut poly = coord(id, axis);
                        for (p, w) in weights {
                            poly = poly.sub(&coord(*p, axis).scale(w));
                        }
                        out.push(Hyp {
                            poly,
                            label: label.clone(),
                        });
                    }
                }
                PointDef::ReflectLine { p, a, b } => {
                    let label = format!(
                        "{} = reflection of {} over {}{}",
                        nm(id),
                        nm(*p),
                        nm(*a),
                        nm(*b)
                    );
                    // (NEW − p) ⟂ (b − a)
                    out.push(Hyp {
                        poly: pdot(&pvec(*p, id), &pvec(*a, *b)),
                        label: label.clone(),
                    });
                    // midpoint(p, NEW) lies on line ab
                    let midx = coord(*p, 0).add(&coord(id, 0)).scale(&Rat::new(1, 2));
                    let midy = coord(*p, 1).add(&coord(id, 1)).scale(&Rat::new(1, 2));
                    let am = PVec(midx.sub(&coord(*a, 0)), midy.sub(&coord(*a, 1)));
                    out.push(Hyp {
                        poly: pcross(&pvec(*a, *b), &am),
                        label,
                    });
                }
            }
        }
    }
    out
}

/// Lower an `aconst` (fixed angle). 0/90/180° are exact and sign-preserving;
/// other angles use the squared-tangent form (sound as a hypothesis — it merely
/// also admits the reflected angle). `None` if the squared tangent is irrational.
fn aconst_poly(a: PointId, b: PointId, c: PointId, d: PointId, deg: f64) -> Option<Poly> {
    let u = pvec(a, b);
    let v = pvec(c, d);
    let m = deg.rem_euclid(180.0);
    if (m - 90.0).abs() < 1e-9 {
        return Some(pdot(&u, &v));
    }
    if m < 1e-9 || (m - 180.0).abs() < 1e-9 {
        return Some(pcross(&u, &v));
    }
    let t = deg.to_radians().tan();
    let t2 = rat_of(t * t)?;
    let cross = pcross(&u, &v);
    let dot = pdot(&u, &v);
    Some(cross.mul(&cross).sub(&dot.mul(&dot).scale(&t2)))
}

/// Lower a `distmeq` product-of-lengths hypothesis with ±1 exponents.
fn distmeq_poly(points: &[PointId], constants: &[Rat]) -> Option<Poly> {
    if constants.len() < 2 {
        return None;
    }
    let coefs = &constants[..constants.len() - 1];
    let konst = rat_of(constants.last()?.to_f64())?;
    if points.len() != coefs.len() * 2 {
        return None;
    }
    let mut left = Poly::from_i64(1);
    let mut right = Poly::constant(&konst * &konst);
    for (k, coef) in coefs.iter().enumerate() {
        let d2 = sq_dist(points[2 * k], points[2 * k + 1]);
        if *coef == Rat::one() {
            left = left.mul(&d2);
        } else if *coef == Rat::from_int(-1) {
            right = right.mul(&d2);
        } else {
            return None;
        }
    }
    Some(left.sub(&right))
}

// ===========================================================================
// Triangular chain
// ===========================================================================

/// One rung of the ascending chain: eliminating `var` uses `poly` (whose
/// leading variable is `var`).
struct Rung {
    var: u32,
    poly: Poly,
    label: String,
}

/// Numeric rank (0/1/2) of 2-D gradient rows.
fn rank2(rows: &[(f64, f64)]) -> usize {
    let scale = rows.iter().map(|&(a, b)| a.hypot(b)).fold(0.0f64, f64::max);
    if scale < 1e-12 {
        return 0;
    }
    let mut cross_max = 0.0f64;
    for i in 0..rows.len() {
        for j in (i + 1)..rows.len() {
            cross_max = cross_max.max((rows[i].0 * rows[j].1 - rows[i].1 * rows[j].0).abs());
        }
    }
    if cross_max > 1e-9 * scale * scale {
        2
    } else {
        1
    }
}

/// Symbolic partial derivative ∂p/∂v.
fn deriv(p: &Poly, v: u32) -> Poly {
    let mut out = Poly::zero();
    for (m, c) in &p.terms {
        let e = m.deg(v);
        if e > 0 {
            out.add_term(m.drop_pow(v, 1), c * &Rat::from_int(e as i64));
        }
    }
    out
}

/// Build the ascending chain of the constructed figure. For each point, decide
/// via the numeric Jacobian how many of its coordinates the hypotheses fix, and
/// triangulate the defining polynomials so each dependent coordinate is the
/// leading variable of exactly one rung.
fn build_chain(fig: &AlgFigure, hyps: &[Hyp], vals: &[f64]) -> Result<Vec<Rung>, String> {
    let n = fig.coords.len();
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (idx, h) in hyps.iter().enumerate() {
        if let Some(mv) = h.poly.max_var() {
            groups[(mv / 2) as usize].push(idx);
        }
    }

    let mut chain: Vec<Rung> = Vec::new();
    for i in 0..n as PointId {
        let g = &groups[i as usize];
        if g.is_empty() {
            continue;
        }
        let rows: Vec<(f64, f64)> = g
            .iter()
            .map(|&idx| {
                (
                    deriv(&hyps[idx].poly, vx(i)).eval(vals),
                    deriv(&hyps[idx].poly, vy(i)).eval(vals),
                )
            })
            .collect();
        match rank2(&rows) {
            0 => continue,
            1 => {
                let (bi, &(gx, gy)) = rows
                    .iter()
                    .enumerate()
                    .max_by(|a, b| {
                        (a.1 .0.hypot(a.1 .1))
                            .partial_cmp(&(b.1 .0.hypot(b.1 .1)))
                            .unwrap()
                    })
                    .unwrap();
                let hyp = &hyps[g[bi]];
                let var = if gx.abs() >= gy.abs() { vx(i) } else { vy(i) };
                if hyp.poly.deg_in(var) == 0 {
                    return Err(format!("cannot orient point {}", fig.names[i as usize]));
                }
                chain.push(Rung {
                    var,
                    poly: hyp.poly.clone(),
                    label: hyp.label.clone(),
                });
            }
            _ => {
                // Two dependent coordinates: pick the two most-independent
                // constraints and triangulate them into an (x-rung free of y,
                // y-rung) pair by a pseudo-Euclidean elimination of y_i. One
                // pseudo-division is NOT enough when both polynomials are
                // quadratic in y_i (e.g. two circles): prem then leaves y_i to
                // degree 1, so we iterate until y_i is gone.
                let (mut best, mut best_cross) = ((0usize, 1usize), 0.0);
                for a in 0..rows.len() {
                    for b in (a + 1)..rows.len() {
                        let cr = (rows[a].0 * rows[b].1 - rows[a].1 * rows[b].0).abs();
                        if cr > best_cross {
                            best_cross = cr;
                            best = (a, b);
                        }
                    }
                }
                let label = format!(
                    "{} determined by [{}] ∧ [{}]",
                    fig.names[i as usize], hyps[g[best.0]].label, hyps[g[best.1]].label
                );
                let mut a = hyps[g[best.0]].poly.clone();
                let mut b = hyps[g[best.1]].poly.clone();
                if a.deg_in(vy(i)) < b.deg_in(vy(i)) {
                    std::mem::swap(&mut a, &mut b);
                }
                // Invariant: deg_y(a) >= deg_y(b). The last `a` that still has
                // y_i is the y-rung; the final `b` (free of y_i) is the x-rung.
                let mut fy = a.clone();
                while b.deg_in(vy(i)) > 0 {
                    let r = prem(&a, &b, vy(i)).primitive();
                    fy = b.clone();
                    a = b;
                    b = r;
                }
                let fx = b;
                if fx.is_zero() || fx.deg_in(vx(i)) == 0 || fy.deg_in(vy(i)) == 0 {
                    return Err(format!(
                        "cannot triangulate point {}",
                        fig.names[i as usize]
                    ));
                }
                chain.push(Rung {
                    var: vx(i),
                    poly: fx,
                    label: label.clone(),
                });
                chain.push(Rung {
                    var: vy(i),
                    poly: fy,
                    label,
                });
            }
        }
    }
    Ok(chain)
}

// ===========================================================================
// Lowering the goal (may introduce auxiliary variables)
// ===========================================================================

/// An auxiliary variable introduced while lowering the goal (a length, a square
/// root, a trig value), with its defining polynomial and a branch note.
struct Aux {
    var: u32,
    poly: Poly,
    label: String,
    note: String,
}

#[derive(Clone, Copy, Debug)]
enum Trig {
    Cos,
    Sin,
    Tan,
}

struct GoalLowerer<'a> {
    fig: &'a AlgFigure,
    base: u32,
    vals: Vec<f64>,
    aux: Vec<Aux>,
    dist_cache: BTreeMap<(PointId, PointId), u32>,
    area_cache: BTreeMap<[PointId; 3], u32>,
}

impl<'a> GoalLowerer<'a> {
    fn new(fig: &'a AlgFigure) -> GoalLowerer<'a> {
        let mut vals = Vec::with_capacity(fig.coords.len() * 2);
        for v in &fig.coords {
            vals.push(v.x);
            vals.push(v.y);
        }
        GoalLowerer {
            fig,
            base: 2 * fig.coords.len() as u32,
            vals,
            aux: Vec::new(),
            dist_cache: BTreeMap::new(),
            area_cache: BTreeMap::new(),
        }
    }

    fn pt(&self, name: &str) -> Result<PointId, String> {
        self.fig
            .names
            .iter()
            .position(|n| n == name)
            .map(|i| i as PointId)
            .ok_or_else(|| format!("unknown point '{name}' in goal"))
    }

    fn nm(&self, p: PointId) -> String {
        self.fig.names[p as usize].clone()
    }

    /// A length variable `d(a,b) ≥ 0` with `d² = |ab|²`.
    fn dist_var(&mut self, a: PointId, b: PointId) -> Poly {
        let key = if a < b { (a, b) } else { (b, a) };
        if let Some(&v) = self.dist_cache.get(&key) {
            return Poly::var(v);
        }
        let value = (self.fig.coords[a as usize] - self.fig.coords[b as usize]).norm();
        let var = self.base + self.aux.len() as u32;
        self.dist_cache.insert(key, var);
        self.vals.push(value);
        let label = format!("|{}{}|", self.nm(a), self.nm(b));
        let poly = Poly::var(var).mul(&Poly::var(var)).sub(&sq_dist(a, b));
        self.aux.push(Aux {
            var,
            poly,
            label: label.clone(),
            note: format!("{label} ≥ 0"),
        });
        Poly::var(var)
    }

    /// The *unsigned* area `[abc] ≥ 0` as an auxiliary variable `s` with
    /// `s² = (½·cross(ab,ac))²`. It is NOT sign-definite across the variety, so
    /// it must never be lowered to a signed shoelace polynomial: an orientation-
    /// dependent sum of areas would then collapse to the identically-true signed
    /// identity and falsely "prove" a statement that only holds for one
    /// orientation. As a length-like variable, such sums leave the area
    /// variables at odd degree, so the reduction cannot spuriously reach zero.
    fn area_var(&mut self, a: PointId, b: PointId, c: PointId) -> Poly {
        let mut key = [a, b, c];
        key.sort_unstable();
        if let Some(&v) = self.area_cache.get(&key) {
            return Poly::var(v);
        }
        let cross = pcross(&pvec(a, b), &pvec(a, c));
        // (½·cross)² = cross²/4
        let sq = cross.mul(&cross).scale(&Rat::new(1, 4));
        let value = 0.5 * cross.eval(&self.vals).abs();
        let var = self.base + self.aux.len() as u32;
        self.area_cache.insert(key, var);
        self.vals.push(value);
        let label = format!("[{}{}{}]", self.nm(a), self.nm(b), self.nm(c));
        let poly = Poly::var(var).mul(&Poly::var(var)).sub(&sq);
        self.aux.push(Aux {
            var,
            poly,
            label: label.clone(),
            note: format!("area {label} ≥ 0"),
        });
        Poly::var(var)
    }

    fn lower(&mut self, e: &MExpr) -> Result<Poly, String> {
        match e {
            MExpr::Num(v) => rat_of(*v)
                .map(Poly::constant)
                .ok_or_else(|| format!("goal constant {v} is not rational")),
            MExpr::Dist(a, b) => {
                let (a, b) = (self.pt(a)?, self.pt(b)?);
                Ok(self.dist_var(a, b))
            }
            MExpr::Area(a, b, c) => {
                let (a, b, c) = (self.pt(a)?, self.pt(b)?, self.pt(c)?);
                Ok(self.area_var(a, b, c))
            }
            MExpr::Neg(x) => Ok(self.lower(x)?.neg()),
            MExpr::Add(x, y) => Ok(self.lower(x)?.add(&self.lower(y)?)),
            MExpr::Sub(x, y) => Ok(self.lower(x)?.sub(&self.lower(y)?)),
            MExpr::Mul(x, y) => Ok(self.lower(x)?.mul(&self.lower(y)?)),
            MExpr::Div(x, y) => {
                let py = self.lower(y)?;
                match py.as_const() {
                    Some(c) if !c.is_zero() => Ok(self.lower(x)?.scale(&c.recip())),
                    _ => Err("division by a non-constant is not supported".to_string()),
                }
            }
            MExpr::Pow(base, p) => self.lower_pow(base, *p),
            MExpr::Sqrt(x) => {
                let px = self.lower(x)?;
                let value = px.eval(&self.vals).max(0.0).sqrt();
                let var = self.base + self.aux.len() as u32;
                self.vals.push(value);
                let vp = Poly::var(var);
                let poly = vp.mul(&vp).sub(&px);
                self.aux.push(Aux {
                    var,
                    poly,
                    label: "√(…)".to_string(),
                    note: "√(…) ≥ 0".to_string(),
                });
                Ok(vp)
            }
            MExpr::Cos(x) => self.lower_trig(x, Trig::Cos),
            MExpr::Sin(x) => self.lower_trig(x, Trig::Sin),
            MExpr::Tan(x) => self.lower_trig(x, Trig::Tan),
            MExpr::Angle(_, _, _) => {
                Err("a bare angle measure is transcendental; wrap it in cos/sin/tan".to_string())
            }
        }
    }

    fn lower_pow(&mut self, base: &MExpr, p: f64) -> Result<Poly, String> {
        if (p - 0.5).abs() < 1e-12 {
            return self.lower(&MExpr::Sqrt(Box::new(base.clone())));
        }
        if !(0.0..=crate::metric::MAX_EXPONENT).contains(&p) || (p - p.round()).abs() > 1e-9 {
            return Err(format!("unsupported exponent {p}"));
        }
        let k = p.round() as u32;
        // Even powers of a length or area are sign-independent, so they lower to
        // a plain polynomial with no auxiliary (branch-carrying) variable.
        if k % 2 == 0 {
            if let MExpr::Dist(a, b) = base {
                let (a, b) = (self.pt(a)?, self.pt(b)?);
                return Ok(sq_dist(a, b).pow(k / 2));
            }
            if let MExpr::Area(a, b, c) = base {
                let (a, b, c) = (self.pt(a)?, self.pt(b)?, self.pt(c)?);
                // (½·cross)^k, even k, so the sign of cross drops out.
                let half = pcross(&pvec(a, b), &pvec(a, c)).scale(&Rat::new(1, 2));
                return Ok(half.pow(k));
            }
        }
        Ok(self.lower(base)?.pow(k))
    }

    fn lower_trig(&mut self, inner: &MExpr, f: Trig) -> Result<Poly, String> {
        if let MExpr::Angle(a, b, c) = inner {
            let (a, b, c) = (self.pt(a)?, self.pt(b)?, self.pt(c)?);
            let (na, nb, nc) = (self.nm(a), self.nm(b), self.nm(c));
            let ba = pvec(b, a);
            let bc = pvec(b, c);
            let dot = pdot(&ba, &bc);
            let cross = pcross(&ba, &bc);
            // Create the length variables *first* so the trig variable, reserved
            // below, has a strictly higher index than the lengths its defining
            // polynomial references (required for the ascending chain).
            let denom = match f {
                Trig::Cos | Trig::Sin => Some(self.dist_var(b, a).mul(&self.dist_var(b, c))),
                Trig::Tan => None,
            };
            let value = self.angle_trig(a, b, c, f);
            let var = self.base + self.aux.len() as u32;
            let vp = Poly::var(var);
            let (poly, label, note) = match f {
                Trig::Cos => (
                    denom.unwrap().mul(&vp).sub(&dot),
                    format!("cos∠{na}{nb}{nc}"),
                    format!("|{nb}{na}|·|{nb}{nc}| ≠ 0"),
                ),
                Trig::Sin => (
                    denom.unwrap().mul(&vp).sub(&cross),
                    format!("sin∠{na}{nb}{nc}"),
                    format!("|{nb}{na}|·|{nb}{nc}| ≠ 0"),
                ),
                Trig::Tan => (
                    dot.mul(&vp).sub(&cross),
                    format!("tan∠{na}{nb}{nc}"),
                    format!("{nb}{na} not ⟂ {nb}{nc}"),
                ),
            };
            self.vals.push(value);
            self.aux.push(Aux {
                var,
                poly,
                label,
                note,
            });
            Ok(vp)
        } else if let MExpr::Num(deg) = inner {
            let value = match f {
                Trig::Cos => deg.to_radians().cos(),
                Trig::Sin => deg.to_radians().sin(),
                Trig::Tan => deg.to_radians().tan(),
            };
            rat_of(value)
                .map(Poly::constant)
                .ok_or_else(|| format!("{f:?}({deg}°) is not rational"))
        } else {
            Err("cos/sin/tan is only supported on an angle or a constant".to_string())
        }
    }

    fn angle_trig(&self, a: PointId, b: PointId, c: PointId, f: Trig) -> f64 {
        let ba = self.fig.coords[a as usize] - self.fig.coords[b as usize];
        let bc = self.fig.coords[c as usize] - self.fig.coords[b as usize];
        let dot = ba.dot(bc);
        let cross = ba.x * bc.y - ba.y * bc.x;
        match f {
            Trig::Cos => dot / (ba.norm() * bc.norm()),
            Trig::Sin => cross / (ba.norm() * bc.norm()),
            Trig::Tan => cross / dot,
        }
    }
}

// ===========================================================================
// Driver
// ===========================================================================

/// Outcome of an attempted algebraic proof.
pub enum Outcome {
    /// A full algebraic proof, ready to print.
    Proved(String),
    /// The goal could not be handled algebraically (reason); fall back to the
    /// numerical certificate.
    Unhandled(String),
}

/// Attempt to prove the metric equation `goal` about the coordinate-free
/// construction `cons_src` by Wu's method.
pub fn prove_metric(cons_src: &str, goal: &str) -> Result<Outcome, String> {
    let (lhs, rhs) = crate::metric::parse_equation(goal)?;
    let fig = crate::geo::build_algebraic(cons_src)?;
    let hyps = lower_hypotheses(&fig);

    let mut gl = GoalLowerer::new(&fig);
    let goal_poly = match (gl.lower(&lhs), gl.lower(&rhs)) {
        (Ok(l), Ok(r)) => l.sub(&r),
        (Err(e), _) | (_, Err(e)) => return Ok(Outcome::Unhandled(e)),
    };

    // Numeric sanity: the statement must hold at the sampled instance.
    let residual0 = goal_poly.eval(&gl.vals);
    if residual0.abs() > 1e-6 * goal_poly.coeff_scale() {
        return Ok(Outcome::Unhandled(format!(
            "the goal does not hold numerically (residual {residual0:.2e}); it appears false"
        )));
    }

    let mut chain = match build_chain(&fig, &hyps, &gl.vals) {
        Ok(c) => c,
        Err(e) => return Ok(Outcome::Unhandled(e)),
    };
    for a in &gl.aux {
        chain.push(Rung {
            var: a.var,
            poly: a.poly.clone(),
            label: format!("{} (auxiliary)", a.label),
        });
    }
    // Reduce highest variable first; the chain is in increasing-variable order.
    chain.sort_by_key(|r| r.var);

    let mut g = goal_poly.clone();
    let mut steps: Vec<(String, u32, usize)> = Vec::new();
    let mut nondeg: Vec<Poly> = Vec::new();
    for rung in chain.iter().rev() {
        if g.is_zero() {
            break;
        }
        if g.deg_in(rung.var) == 0 {
            continue;
        }
        let init = rung.poly.lead_in(rung.var);
        if init.as_const().is_none() {
            nondeg.push(init.clone());
        }
        g = prem(&g, &rung.poly, rung.var).primitive();
        steps.push((rung.label.clone(), rung.var, g.n_terms()));
    }

    if !g.is_zero() {
        return Ok(Outcome::Unhandled(format!(
            "the goal did not reduce to zero ({} term(s) remain); it may depend on a \
             sign branch outside the polynomial method",
            g.n_terms()
        )));
    }

    // The proof is non-vacuous only if every nondegeneracy condition is
    // satisfied at the sampled (generic) instance.
    for p in &nondeg {
        if p.eval(&gl.vals).abs() < 1e-9 * p.coeff_scale() {
            return Ok(Outcome::Unhandled(
                "a nondegeneracy condition is (numerically) violated at the sampled figure"
                    .to_string(),
            ));
        }
    }

    Ok(Outcome::Proved(render_proof(
        &fig, goal, &hyps, &chain, &gl, &goal_poly, &steps, &nondeg,
    )))
}

/// Render the human-readable proof.
#[allow(clippy::too_many_arguments)]
fn render_proof(
    fig: &AlgFigure,
    goal: &str,
    hyps: &[Hyp],
    chain: &[Rung],
    gl: &GoalLowerer,
    goal_poly: &Poly,
    steps: &[(String, u32, usize)],
    nondeg: &[Poly],
) -> String {
    let names = var_names(fig, gl);
    let n = fig.coords.len();
    let mut out = String::new();
    out.push_str("PROVED (exactly, by coordinates — Wu's method)\n");
    out.push_str(&format!("  {goal}\n\n"));

    out.push_str("Setup: place the figure in the coordinate plane.\n");
    let dep: std::collections::HashSet<u32> = chain
        .iter()
        .filter(|r| (r.var as usize) < 2 * n)
        .map(|r| r.var)
        .collect();
    let mut free = Vec::new();
    for i in 0..n as PointId {
        for axis in [vx(i), vy(i)] {
            if !dep.contains(&axis) {
                free.push(names[axis as usize].clone());
            }
        }
    }
    out.push_str(&format!(
        "  Free parameters (degrees of freedom): {}\n",
        free.join(", ")
    ));
    out.push_str("  Coordinates fixed by the construction:\n");
    for r in chain {
        if (r.var as usize) < 2 * n {
            out.push_str(&format!(
                "    {} — from: {}\n",
                names[r.var as usize], r.label
            ));
        }
    }

    out.push_str("\nHypotheses as polynomial equations (= 0):\n");
    let mut shown = 0;
    for h in hyps {
        if h.poly.is_zero() {
            continue;
        }
        out.push_str(&format!(
            "  [{}]:  {} = 0\n",
            h.label,
            poly_str(&h.poly, &names)
        ));
        shown += 1;
        if shown >= 20 {
            out.push_str("  …\n");
            break;
        }
    }
    if !gl.aux.is_empty() {
        out.push_str("  Auxiliary quantities in the goal:\n");
        for a in &gl.aux {
            out.push_str(&format!(
                "    {}:  {} = 0\n",
                a.label,
                poly_str(&a.poly, &names)
            ));
        }
    }

    out.push_str("\nGoal as a polynomial equation (= 0):\n");
    out.push_str(&format!("  {}\n", poly_str(goal_poly, &names)));

    out.push_str("\nElimination (pseudo-division, highest variable first):\n");
    for (label, var, remaining) in steps {
        out.push_str(&format!(
            "  eliminate {:<9} using [{}]  →  {} term(s) remain\n",
            names[*var as usize], label, remaining
        ));
    }
    out.push_str("  the goal polynomial reduces to 0.\n");

    out.push_str("\nHence the goal is an identity on the construction, provided:\n");
    let mut conds: Vec<String> = nondeg
        .iter()
        .map(|p| format!("{} ≠ 0", poly_str(p, &names)))
        .collect();
    for a in &gl.aux {
        conds.push(a.note.clone());
    }
    conds.sort();
    conds.dedup();
    if conds.is_empty() {
        out.push_str("  (no nondegeneracy conditions).\n");
    } else {
        for c in conds {
            out.push_str(&format!("  · {c}\n"));
        }
    }
    out.push_str("  (each holds in the sampled figure, so the proof is non-vacuous). ∎\n");
    out
}

fn var_names(fig: &AlgFigure, gl: &GoalLowerer) -> Vec<String> {
    let n = fig.coords.len();
    let mut names = vec![String::new(); 2 * n + gl.aux.len() + 8];
    for (i, name) in fig.names.iter().enumerate() {
        names[2 * i] = format!("{name}.x");
        names[2 * i + 1] = format!("{name}.y");
    }
    for a in &gl.aux {
        if (a.var as usize) < names.len() {
            names[a.var as usize] = a.label.clone();
        }
    }
    for (i, s) in names.iter_mut().enumerate() {
        if s.is_empty() {
            *s = format!("v{i}");
        }
    }
    names
}

/// Pretty-print a polynomial with named variables (highest-degree terms first,
/// truncated for readability).
fn poly_str(p: &Poly, names: &[String]) -> String {
    if p.terms.is_empty() {
        return "0".to_string();
    }
    let mut terms: Vec<(&Mono, &Rat)> = p.terms.iter().collect();
    terms.sort_by(|a, b| b.0.total_deg().cmp(&a.0.total_deg()).then(a.0.cmp(b.0)));
    let mut out = String::new();
    for (i, (m, c)) in terms.iter().enumerate() {
        let mono = mono_str(m, names);
        if i == 0 {
            if mono.is_empty() {
                out.push_str(&format!("{c}"));
            } else if **c == Rat::one() {
                out.push_str(&mono);
            } else if **c == Rat::from_int(-1) {
                out.push('-');
                out.push_str(&mono);
            } else {
                out.push_str(&format!("{c}·{mono}"));
            }
        } else {
            let neg = c.is_negative();
            let cc = if neg { c.abs() } else { (**c).clone() };
            out.push_str(if neg { " - " } else { " + " });
            if mono.is_empty() {
                out.push_str(&format!("{cc}"));
            } else if cc == Rat::one() {
                out.push_str(&mono);
            } else {
                out.push_str(&format!("{cc}·{mono}"));
            }
        }
        if out.len() > 150 {
            out.push_str(" + …");
            break;
        }
    }
    out
}

fn mono_str(m: &Mono, names: &[String]) -> String {
    let mut parts = Vec::new();
    for &(v, e) in &m.0 {
        let name = names
            .get(v as usize)
            .cloned()
            .unwrap_or_else(|| format!("v{v}"));
        if e == 1 {
            parts.push(name);
        } else {
            parts.push(format!("{name}^{e}"));
        }
    }
    parts.join("·")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proved(cons: &str, goal: &str) -> bool {
        match prove_metric(cons, goal) {
            Ok(Outcome::Proved(_)) => true,
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
    fn poly_arithmetic() {
        let x = Poly::var(0);
        let y = Poly::var(1);
        let p = x.add(&y).mul(&x.sub(&y)); // x² − y²
        let expect = x.mul(&x).sub(&y.mul(&y));
        assert!(p.sub(&expect).is_zero());
    }

    #[test]
    fn prem_eliminates() {
        // f = x²,  g = x − 2  ⇒ remainder is the constant 4.
        let f = Poly::var(0).mul(&Poly::var(0));
        let g = Poly::var(0).sub(&Poly::from_i64(2));
        let r = prem(&f, &g, 0);
        assert_eq!(r.deg_in(0), 0);
        assert_eq!(r.as_const(), Some(Rat::from_int(4)));
    }

    #[test]
    fn pythagoras() {
        assert!(proved(
            "A = free\nB = free\nC = point: perp(C,A,C,B)",
            "dist(A,B)^2 = dist(C,A)^2 + dist(C,B)^2"
        ));
    }

    #[test]
    fn median_length_apollonius() {
        assert!(proved(
            "A = free\nB = free\nC = free\nM = midpoint(A,B)",
            "2*dist(C,A)^2 + 2*dist(C,B)^2 = dist(A,B)^2 + 4*dist(C,M)^2"
        ));
    }

    #[test]
    fn law_of_cosines() {
        assert!(proved(
            "A = free\nB = free\nC = free",
            "dist(A,B)^2 = dist(C,A)^2 + dist(C,B)^2 - 2*dist(C,A)*dist(C,B)*cos(angle(A,C,B))"
        ));
    }

    #[test]
    fn british_flag() {
        assert!(proved(
            "A = free\nB = free\nC = point: perp(C,B,B,A)\n\
             D = point: para(D,A,B,C), para(D,C,A,B)\nP = free",
            "dist(P,A)^2 + dist(P,C)^2 = dist(P,B)^2 + dist(P,D)^2"
        ));
    }

    #[test]
    fn perpendicular_chord_sum_general() {
        // C on a circle of radius R about O, B the antipode of A ⇒ CA²+CB² = 4R²
        // (Thales). Exercises the single-branch `reflect` definition.
        assert!(proved(
            "O = free\nA = free\nB = reflect(A,O)\nC = point: on(C, circle(O,A))",
            "dist(C,A)^2 + dist(C,B)^2 = 4*dist(O,A)^2"
        ));
    }

    #[test]
    fn scale_specific_diameter() {
        // Absolute scale: radius 6 ⇒ the sum is exactly 144.
        assert!(proved(
            "O = free\nA = point: dist(O,A)=6\nB = reflect(A,O)\nC = point: on(C, circle(O,A))",
            "dist(C,A)^2 + dist(C,B)^2 = 144"
        ));
    }

    #[test]
    fn shift_is_a_translation() {
        // X = shift(P,A,B) is the translate of P by A→B, so PX = AB (exercises
        // the single-branch affine `shift` definition).
        assert!(proved(
            "A = free\nB = free\nP = free\nX = shift(P,A,B)",
            "dist(P,X)^2 = dist(A,B)^2"
        ));
    }

    #[test]
    fn point_by_two_distance_constraints() {
        // A point pinned by two distance constraints is the intersection of two
        // circles — both defining polynomials are quadratic in its y-coordinate,
        // so triangulation needs the full pseudo-Euclidean elimination (a single
        // pseudo-division would leave y at degree 1). Equilateral side 2 ⇒
        // area² = 3.
        assert!(proved(
            "A = free\nB = point: dist(A,B)=2\nC = point: dist(A,C)=2, dist(B,C)=2",
            "area(A,B,C)^2 = 3"
        ));
    }

    #[test]
    fn orientation_dependent_area_is_not_falsely_proved() {
        // The unsigned-area partition area(PAB)+area(PBC)+area(PCA)=area(ABC)
        // holds ONLY when P is inside triangle ABC; on_line puts P anywhere on
        // the median line. Lowering area to a sign-frozen signed shoelace would
        // collapse this to the identically-true signed identity and falsely
        // "prove" it. It must NOT be proved (each area is a nonnegative variable,
        // so the sum leaves them at odd degree and cannot reduce to zero).
        assert!(!proved(
            "A = free\nB = free\nC = free\nM = midpoint(B,C)\nP = on_line(A,M)",
            "area(P,A,B) + area(P,B,C) + area(P,C,A) = area(A,B,C)"
        ));
    }

    #[test]
    fn rejects_false_statement() {
        // A false metric claim must not be "proved".
        assert!(!proved(
            "A = free\nB = free\nC = free",
            "dist(A,B)^2 = dist(A,C)^2 + dist(B,C)^2"
        ));
    }
}
