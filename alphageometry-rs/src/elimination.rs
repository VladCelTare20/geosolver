//! The three geometric algebraic systems built on top of [`ElimCore`]:
//!
//! * [`ElimAngle`] — directed angles measured in half-turns (`pi`), taken modulo
//!   1 (a line is undirected). The constant `pi` is a fixed RHS variable.
//! * [`ElimDistMul`] — multiplicative distances handled in log space, so that
//!   products of lengths become linear. Rational constants are decomposed into
//!   prime logarithms (RHS variables).
//! * [`ElimDistAdd`] — additive segment lengths.
//!
//! Each carries a thin value newtype ([`Angle`], [`DistMul`], [`DistAdd`]) that
//! wraps a [`LinComb`]. `Angle` reduces its `pi` coefficient modulo 1 on every
//! construction so that equal angles always share an identical representation
//! (essential for the hash-bucketed closure search).

use crate::elim_core::ElimCore;
use crate::lincomb::{LinComb, VarId};
use crate::rational::Rat;
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use rustc_hash::FxHashMap;

/// The fixed variable id of the half-turn unit `pi` in the angle system.
pub const ANGLE_UNIT: VarId = 0;

// ---------------------------------------------------------------------------
// Prime decomposition (for multiplicative constants)
// ---------------------------------------------------------------------------

/// Factor `n > 0` into `(prime, exponent)` pairs.
pub fn prime_decomposition(mut n: u64) -> Vec<(u64, u32)> {
    assert!(n > 0);
    let mut result = Vec::new();
    let mut p2 = 0u32;
    while n.is_multiple_of(2) {
        p2 += 1;
        n /= 2;
    }
    if p2 > 0 {
        result.push((2, p2));
    }
    let mut d = 3u64;
    while d * d <= n {
        if n.is_multiple_of(d) {
            let mut e = 0u32;
            while n.is_multiple_of(d) {
                e += 1;
                n /= d;
            }
            result.push((d, e));
        }
        d += 2;
    }
    if n > 1 {
        result.push((n, 1));
    }
    result
}

// ---------------------------------------------------------------------------
// Value newtypes
// ---------------------------------------------------------------------------

/// A directed angle, with its `pi` coefficient reduced modulo 1.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Angle(pub LinComb);

impl Angle {
    /// Wrap a combination, normalizing the `pi` coefficient into `[0, 1)`.
    pub fn new(mut comb: LinComb) -> Angle {
        let reduced = match comb.terms.first() {
            Some((v, c)) if *v == ANGLE_UNIT => {
                let r = c.mod_one();
                (r != *c).then_some(r)
            }
            _ => None,
        };
        if let Some(r) = reduced {
            if r.is_zero() {
                comb.terms.remove(0);
            } else {
                comb.terms[0].1 = r;
            }
        }
        Angle(comb)
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn neg(&self) -> Angle {
        Angle::new(self.0.negated())
    }

    pub fn add(&self, other: &Angle) -> Angle {
        Angle::new(&self.0 + &other.0)
    }

    pub fn sub(&self, other: &Angle) -> Angle {
        Angle::new(&self.0 - &other.0)
    }
}

/// A multiplicative distance in log space; empty means "equals 1".
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistMul(pub LinComb);

impl DistMul {
    #[inline]
    pub fn is_one(&self) -> bool {
        self.0.is_zero()
    }

    /// Multiply (add in log space).
    pub fn mul(&self, other: &DistMul) -> DistMul {
        DistMul(&self.0 + &other.0)
    }

    /// Divide (subtract in log space).
    pub fn div(&self, other: &DistMul) -> DistMul {
        DistMul(&self.0 - &other.0)
    }
}

/// An additive combination of segment lengths; empty means "equals 0".
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistAdd(pub LinComb);

impl DistAdd {
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    pub fn add(&self, other: &DistAdd) -> DistAdd {
        DistAdd(&self.0 + &other.0)
    }
    pub fn sub(&self, other: &DistAdd) -> DistAdd {
        DistAdd(&self.0 - &other.0)
    }
    pub fn neg(&self) -> DistAdd {
        DistAdd(self.0.negated())
    }
    pub fn mul_scalar(&self, s: &Rat) -> DistAdd {
        let mut c = self.0.clone();
        c.mul_assign_scalar(s);
        DistAdd(c)
    }
    pub fn div_scalar(&self, s: &Rat) -> DistAdd {
        self.mul_scalar(&s.recip())
    }
}

// ---------------------------------------------------------------------------
// Angle system
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ElimAngle {
    pub core: ElimCore,
}

impl ElimAngle {
    pub fn new() -> ElimAngle {
        let mut core = ElimCore::new();
        // Reserve ANGLE_UNIT (id 0) as the constant `pi`, value 1.
        let unit = core.new_var(1.0, false);
        debug_assert_eq!(unit, ANGLE_UNIT);
        ElimAngle { core }
    }

    /// A fresh direction variable with numeric value `value` in `[0,1)`.
    pub fn new_var(&mut self, value: f64) -> Angle {
        let v = self.core.new_var(value, true);
        Angle(LinComb::singleton(v, Rat::one()))
    }

    /// The constant angle `frac * pi` (measured in half-turns).
    pub fn const_frac(&self, frac: Rat) -> Angle {
        Angle::new(LinComb::singleton(ANGLE_UNIT, frac))
    }

    /// The constant angle `num/den` (in half-turns).
    pub fn const_ratio(&self, num: i64, den: i64) -> Angle {
        self.const_frac(Rat::new(num, den))
    }

    pub fn value_of(&self, angle: &Angle) -> f64 {
        self.core.value_of(&angle.0)
    }

    pub fn force_zero(&mut self, angle: &Angle, fact: Option<crate::proof::FactId>) -> bool {
        let val = self.value_of(angle);
        debug_assert!(
            ((val + 0.5).rem_euclid(1.0) - 0.5).powi(2) < crate::numerics::ATOM,
            "force_zero on non-integer angle: {val}"
        );
        let mut comb = angle.0.clone();
        let k = (val + 0.5).floor() as i64;
        comb.add_term(ANGLE_UNIT, Rat::from_int(-k));
        self.core.add_constraint(comb, fact)
    }

    pub fn simplify(&self, angle: &Angle) -> Angle {
        let mut comb = angle.0.clone();
        self.core.simplify(&mut comb);
        Angle::new(comb)
    }

    /// Simplify, returning both the normal form and the facts used (provenance).
    pub fn simplify_deps(&self, angle: &Angle) -> (Angle, Vec<crate::proof::FactId>) {
        let mut comb = angle.0.clone();
        let mut deps = Vec::new();
        self.core.simplify_collect(&mut comb, &mut deps);
        (Angle::new(comb), deps)
    }

    /// Whether the (singleton) direction variable already participates.
    pub fn was_encountered(&self, angle: &Angle) -> bool {
        debug_assert_eq!(angle.0.len(), 1);
        self.core.var_encountered(angle.0.terms[0].0)
    }
}

impl Default for ElimAngle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Multiplicative-distance system
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ElimDistMul {
    pub core: ElimCore,
    prime_to_var: FxHashMap<u64, VarId>,
}

impl ElimDistMul {
    pub fn new() -> ElimDistMul {
        ElimDistMul {
            core: ElimCore::new(),
            prime_to_var: FxHashMap::default(),
        }
    }

    /// A fresh log-distance variable with numeric value `value` (the distance).
    pub fn new_var(&mut self, value: f64) -> DistMul {
        let v = self.core.new_var(value, true);
        DistMul(LinComb::singleton(v, Rat::one()))
    }

    fn prime_var(&mut self, p: u64) -> VarId {
        if let Some(&v) = self.prime_to_var.get(&p) {
            return v;
        }
        // A prime-log constant is an RHS variable whose numeric value is the
        // prime itself (DistMul values are products of value^exponent).
        let v = self.core.new_var(p as f64, false);
        self.prime_to_var.insert(p, v);
        v
    }

    /// The multiplicative constant `frac` expressed in prime logarithms.
    pub fn frac_value(&mut self, frac: &Rat) -> DistMul {
        if frac.is_one() {
            return DistMul(LinComb::zero());
        }
        assert!(!frac.is_negative() && !frac.is_zero(), "non-positive ratio");
        let num = frac.numer_i64().expect("ratio numerator too large") as u64;
        let den = frac.denom_i64().expect("ratio denominator too large") as u64;
        let mut comb = LinComb::zero();
        for (p, e) in prime_decomposition(num) {
            let v = self.prime_var(p);
            comb.add_term(v, Rat::from_int(e as i64));
        }
        for (p, e) in prime_decomposition(den) {
            let v = self.prime_var(p);
            comb.add_term(v, Rat::from_int(-(e as i64)));
        }
        DistMul(comb)
    }

    pub fn mul_const(&mut self, dm: &DistMul, frac: &Rat) -> DistMul {
        let fv = self.frac_value(frac);
        dm.mul(&fv)
    }

    pub fn div_const(&mut self, dm: &DistMul, frac: &Rat) -> DistMul {
        let fv = self.frac_value(frac);
        dm.div(&fv)
    }

    /// Multiplicative value: product of `value^exponent`.
    pub fn value_of(&self, dm: &DistMul) -> f64 {
        let mut v = 1.0;
        for (var, exp) in &dm.0.terms {
            let base = self.core.values[*var as usize];
            let e = exp.to_f64();
            v *= base.powf(e);
        }
        v
    }

    /// Separate integer prime powers into a numeric coefficient, keeping the
    /// (LHS + fractional-prime) part symbolic. Mirrors `DistMul.normalize`.
    pub fn normalize(&self, dm: &DistMul) -> (DistMul, Rat) {
        let mut normalized = LinComb::zero();
        let mut coef = Rat::one();
        for (var, exp) in &dm.0.terms {
            if self.core.is_lhs[*var as usize] {
                normalized.add_term(*var, exp.clone());
            } else {
                // prime constant
                let (int_part, frac_part) = exp.split_floor();
                if !frac_part.is_zero() {
                    normalized.add_term(*var, frac_part);
                }
                let base = self.prime_pow(*var, &int_part);
                coef = &coef * &base;
            }
        }
        (DistMul(normalized), coef)
    }

    fn prime_pow(&self, var: VarId, exp: &BigInt) -> Rat {
        // value of the prime variable, as an exact integer
        let p = self.core.values[var as usize].round() as i64;
        let e = exp.to_i64().expect("prime exponent too large");
        if e >= 0 {
            let mut r = Rat::one();
            for _ in 0..e {
                r = &r * &Rat::from_int(p);
            }
            r
        } else {
            let mut r = Rat::one();
            for _ in 0..(-e) {
                r = &r * &Rat::from_int(p);
            }
            r.recip()
        }
    }

    pub fn force_one(&mut self, dm: &DistMul, fact: Option<crate::proof::FactId>) -> bool {
        debug_assert!(
            (self.value_of(dm) - 1.0).powi(2) < crate::numerics::ATOM,
            "force_one on non-unit ratio: {}",
            self.value_of(dm)
        );
        self.core.add_constraint(dm.0.clone(), fact)
    }

    pub fn simplify(&self, dm: &DistMul) -> DistMul {
        let mut comb = dm.0.clone();
        self.core.simplify(&mut comb);
        DistMul(comb)
    }

    /// Simplify, returning both the normal form and the facts used (provenance).
    pub fn simplify_deps(&self, dm: &DistMul) -> (DistMul, Vec<crate::proof::FactId>) {
        let mut comb = dm.0.clone();
        let mut deps = Vec::new();
        self.core.simplify_collect(&mut comb, &mut deps);
        (DistMul(comb), deps)
    }

    pub fn was_encountered(&self, dm: &DistMul) -> bool {
        debug_assert_eq!(dm.0.len(), 1);
        self.core.var_encountered(dm.0.terms[0].0)
    }
}

impl Default for ElimDistMul {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Additive-distance system
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ElimDistAdd {
    pub core: ElimCore,
}

impl ElimDistAdd {
    pub fn new() -> ElimDistAdd {
        ElimDistAdd {
            core: ElimCore::new(),
        }
    }

    pub fn new_var(&mut self, value: f64) -> DistAdd {
        let v = self.core.new_var(value, true);
        DistAdd(LinComb::singleton(v, Rat::one()))
    }

    pub fn value_of(&self, da: &DistAdd) -> f64 {
        self.core.value_of(&da.0)
    }

    /// Divide by the smallest absolute LHS coefficient. Mirrors
    /// `DistAdd.normalize`.
    pub fn normalize(&self, da: &DistAdd) -> (DistAdd, Rat) {
        let c =
            da.0.terms
                .iter()
                .filter(|(v, _)| self.core.is_lhs[*v as usize])
                .map(|(_, coef)| coef.abs())
                .min()
                .expect("normalize of empty additive distance");
        (da.div_scalar(&c), c)
    }

    pub fn force_zero(&mut self, da: &DistAdd, fact: Option<crate::proof::FactId>) -> bool {
        debug_assert!(
            self.value_of(da).powi(2) < crate::numerics::ATOM,
            "force_zero on nonzero additive distance: {}",
            self.value_of(da)
        );
        self.core.add_constraint(da.0.clone(), fact)
    }

    pub fn simplify(&self, da: &DistAdd) -> DistAdd {
        let mut comb = da.0.clone();
        self.core.simplify(&mut comb);
        DistAdd(comb)
    }

    /// Simplify, returning both the normal form and the facts used (provenance).
    pub fn simplify_deps(&self, da: &DistAdd) -> (DistAdd, Vec<crate::proof::FactId>) {
        let mut comb = da.0.clone();
        let mut deps = Vec::new();
        self.core.simplify_collect(&mut comb, &mut deps);
        (DistAdd(comb), deps)
    }
}

impl Default for ElimDistAdd {
    fn default() -> Self {
        Self::new()
    }
}

/// A homogeneous linear combination of squared segment lengths `|XY|²`;
/// empty means "equals 0".
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DistSq(pub LinComb);

impl DistSq {
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

/// The squared-length system (Yuclid's fourth table): one LHS variable per
/// segment, valued `|XY|²`, and only homogeneous equations between them.
#[derive(Clone, Default)]
pub struct ElimDistSq {
    pub core: ElimCore,
}

impl ElimDistSq {
    pub fn new() -> ElimDistSq {
        ElimDistSq::default()
    }

    pub fn new_var(&mut self, squared_length: f64) -> DistSq {
        let v = self.core.new_var(squared_length, true);
        DistSq(LinComb::singleton(v, Rat::one()))
    }

    pub fn value_of(&self, ds: &DistSq) -> f64 {
        self.core.value_of(&ds.0)
    }

    /// Whether `ds == 0` holds in the figure, relative to the size of its terms.
    pub fn holds_numerically(&self, ds: &DistSq) -> bool {
        let scale: f64 = ds
            .0
            .terms
            .iter()
            .map(|(v, c)| (c.to_f64() * self.core.values[*v as usize]).abs())
            .sum();
        self.value_of(ds).abs() <= 1e-7 * scale.max(1e-9)
    }

    /// Add `ds == 0`. An equation the figure contradicts is never added: every
    /// caller states a theorem, so a mismatch can only be numeric noise or a
    /// degenerate figure, and dropping it costs completeness, not soundness.
    pub fn force_zero(&mut self, ds: &DistSq, fact: Option<crate::proof::FactId>) -> bool {
        if !self.holds_numerically(ds) {
            debug_assert!(false, "squared-length equation fails numerically: {}", self.value_of(ds));
            return false;
        }
        self.core.add_constraint(ds.0.clone(), fact)
    }

    pub fn simplify(&self, ds: &DistSq) -> DistSq {
        let mut comb = ds.0.clone();
        self.core.simplify(&mut comb);
        DistSq(comb)
    }

    pub fn simplify_deps(&self, ds: &DistSq) -> (DistSq, Vec<crate::proof::FactId>) {
        let mut comb = ds.0.clone();
        let mut deps = Vec::new();
        self.core.simplify_collect(&mut comb, &mut deps);
        (DistSq(comb), deps)
    }
}

/// `sqrt(q)` as a multiplicative constant: half the prime-log exponents of `q`.
pub fn dist_mul_sqrt(dmul: &mut ElimDistMul, q: &Rat) -> DistMul {
    let mut comb = dmul.frac_value(q).0;
    comb.mul_assign_scalar(&Rat::new(1, 2));
    DistMul(comb)
}

/// The exact rational square root of `q`, when `q` is a perfect square.
pub fn rat_sqrt(q: &Rat) -> Option<Rat> {
    if q.is_negative() {
        return None;
    }
    let isqrt = |n: i64| -> Option<i64> {
        let r = (n as f64).sqrt().round() as i64;
        (r.checked_mul(r)? == n).then_some(r)
    };
    let (n, d) = (q.numer_i64()?, q.denom_i64()?);
    Some(Rat::new(isqrt(n)?, isqrt(d)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squared_length_system_detects_consequences() {
        let mut s = ElimDistSq::new();
        let (a, b, c) = (s.new_var(9.0), s.new_var(16.0), s.new_var(25.0));
        let pyth = DistSq(&(&a.0 + &b.0) - &c.0);
        assert!(s.force_zero(&pyth, None));
        assert!(s.simplify(&pyth).is_zero());
        assert!(!s.simplify(&DistSq(&a.0 - &b.0)).is_zero());
    }

    #[test]
    fn rational_square_roots() {
        assert_eq!(rat_sqrt(&Rat::new(9, 4)), Some(Rat::new(3, 2)));
        assert_eq!(rat_sqrt(&Rat::new(2, 1)), None);
        assert_eq!(rat_sqrt(&Rat::new(-4, 1)), None);
        let mut d = ElimDistMul::new();
        let half = dist_mul_sqrt(&mut d, &Rat::new(2, 1));
        assert!((d.value_of(&half) - 2f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn prime_decomp() {
        assert_eq!(prime_decomposition(12), vec![(2, 2), (3, 1)]);
        assert_eq!(prime_decomposition(1), vec![]);
        assert_eq!(prime_decomposition(97), vec![(97, 1)]);
        assert_eq!(prime_decomposition(360), vec![(2, 3), (3, 2), (5, 1)]);
    }

    #[test]
    fn angle_reduces_mod_one() {
        let a = ElimAngle::new();
        let x = a.const_frac(Rat::new(3, 2)); // 3/2 -> 1/2
        assert_eq!(x.0.get(ANGLE_UNIT), Rat::new(1, 2));
        let y = a.const_frac(Rat::from_int(2)); // -> 0
        assert!(y.is_zero());
        let z = a.const_frac(Rat::new(-1, 3)); // -> 2/3
        assert_eq!(z.0.get(ANGLE_UNIT), Rat::new(2, 3));
    }

    #[test]
    fn distmul_frac_value_roundtrip() {
        let mut d = ElimDistMul::new();
        let dm = d.frac_value(&Rat::new(6, 1)); // 2 * 3
        assert!((d.value_of(&dm) - 6.0).abs() < 1e-9);
        let dm2 = d.frac_value(&Rat::new(1, 12));
        assert!((d.value_of(&dm2) - 1.0 / 12.0).abs() < 1e-9);
    }

    #[test]
    fn angle_equation() {
        // Two directions equal -> their difference forced zero is detected.
        let mut a = ElimAngle::new();
        let d1 = a.new_var(0.25);
        let d2 = a.new_var(0.25);
        let diff = d1.sub(&d2);
        assert!(a.force_zero(&diff, None));
        let probe = a.simplify(&d1.sub(&d2));
        assert!(probe.is_zero());
    }
}
