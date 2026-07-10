//! Exact rational arithmetic, tuned for a theorem prover.
//!
//! The AlphaGeometry2 elimination engine reasons over exact rationals. The
//! reference implementation uses Python's `fractions.Fraction`, which is
//! arbitrary precision but slow. In practice the coefficients that arise in
//! olympiad geometry proofs are *tiny* (small integers, halves, and fractions
//! whose denominators divide numbers like 180 or a handful of primes), so a
//! machine-word fast path handles essentially every operation.
//!
//! [`Rat`] therefore keeps an `i64/i64` fast path and *automatically* falls
//! back to arbitrary precision on overflow. This preserves the soundness of an
//! exact prover (we never silently wrap) while running at native speed on the
//! common case.
//!
//! # Canonical form
//!
//! Every `Rat` is stored in lowest terms with a positive denominator, and a
//! value that fits in `i64/i64` is *always* stored as [`Rat::Small`] (never
//! `Big`). This makes [`PartialEq`], [`Eq`], and [`Hash`] purely structural,
//! which is essential: the closure search buckets linear combinations by exact
//! equality of their coefficients.

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{Signed, ToPrimitive};

/// An exact rational number.
#[derive(Clone)]
pub enum Rat {
    /// Fast path: `num/den` in lowest terms, `den > 0`. Used whenever the value
    /// fits in two `i64`s.
    Small(i64, i64),
    /// Fallback: only used when the reduced value does not fit in `i64/i64`.
    /// Always in lowest terms with positive denominator.
    Big(Box<BigRational>),
}

impl Rat {
    /// The rational `0`.
    #[inline]
    pub const fn zero() -> Rat {
        Rat::Small(0, 1)
    }

    /// The rational `1`.
    #[inline]
    pub const fn one() -> Rat {
        Rat::Small(1, 1)
    }

    /// An integer value.
    #[inline]
    pub const fn from_int(n: i64) -> Rat {
        Rat::Small(n, 1)
    }

    /// Build from a numerator/denominator pair (any signs), reducing to
    /// canonical form. Panics if `den == 0`.
    pub fn new(num: i64, den: i64) -> Rat {
        assert!(den != 0, "zero denominator");
        Rat::from_i128(num as i128, den as i128)
    }

    /// Build from an `i128` numerator/denominator, reducing and picking the
    /// tightest representation. Panics if `den == 0`.
    fn from_i128(mut num: i128, mut den: i128) -> Rat {
        debug_assert!(den != 0);
        if num == 0 {
            return Rat::Small(0, 1);
        }
        if den < 0 {
            num = -num;
            den = -den;
        }
        let g = num.unsigned_abs().gcd(&den.unsigned_abs()) as i128;
        num /= g;
        den /= g;
        match (i64::try_from(num), i64::try_from(den)) {
            (Ok(n), Ok(d)) => Rat::Small(n, d),
            _ => Rat::Big(Box::new(BigRational::new(
                BigInt::from(num),
                BigInt::from(den),
            ))),
        }
    }

    /// Normalize a `BigRational` into a `Rat`, demoting to `Small` when it fits.
    fn from_big(r: BigRational) -> Rat {
        if let (Some(n), Some(d)) = (r.numer().to_i64(), r.denom().to_i64()) {
            // BigRational is already reduced with positive denominator.
            Rat::Small(n, d)
        } else {
            Rat::Big(Box::new(r))
        }
    }

    fn to_big(&self) -> BigRational {
        match self {
            Rat::Small(n, d) => BigRational::new(BigInt::from(*n), BigInt::from(*d)),
            Rat::Big(r) => (**r).clone(),
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        matches!(self, Rat::Small(0, _))
    }

    #[inline]
    pub fn is_one(&self) -> bool {
        matches!(self, Rat::Small(1, 1))
    }

    /// Whether the value is an integer.
    pub fn is_integer(&self) -> bool {
        match self {
            Rat::Small(_, d) => *d == 1,
            Rat::Big(r) => r.is_integer(),
        }
    }

    /// Numerator, if it fits in `i64`.
    pub fn numer_i64(&self) -> Option<i64> {
        match self {
            Rat::Small(n, _) => Some(*n),
            Rat::Big(r) => r.numer().to_i64(),
        }
    }

    /// Denominator, if it fits in `i64`.
    pub fn denom_i64(&self) -> Option<i64> {
        match self {
            Rat::Small(_, d) => Some(*d),
            Rat::Big(r) => r.denom().to_i64(),
        }
    }

    /// Floating-point approximation.
    pub fn to_f64(&self) -> f64 {
        match self {
            Rat::Small(n, d) => *n as f64 / *d as f64,
            Rat::Big(r) => r.to_f64().unwrap_or(f64::NAN),
        }
    }

    /// The greatest integer `<= self`.
    pub fn floor_int(&self) -> BigInt {
        match self {
            Rat::Small(n, d) => BigInt::from(n.div_euclid(*d)),
            Rat::Big(r) => r.floor().to_integer(),
        }
    }

    /// The fractional part `self - floor(self)`, always in `[0, 1)`.
    ///
    /// Mirrors Python's `x % 1` for `Fraction` (which floors toward negative
    /// infinity), used to reduce angle constants modulo a half-turn.
    pub fn mod_one(&self) -> Rat {
        match self {
            Rat::Small(n, d) => {
                let r = n.rem_euclid(*d);
                Rat::Small(r, *d) // 0 <= r < d, already reduced? not necessarily
                    .reduced_small()
            }
            Rat::Big(_) => {
                let f = self.floor_int();
                let frac = self.to_big() - BigRational::from(f);
                Rat::from_big(frac)
            }
        }
    }

    /// The integer part truncated toward negative infinity plus fractional
    /// remainder in `[0,1)`. Returns `(floor, self - floor)`.
    pub fn split_floor(&self) -> (BigInt, Rat) {
        (self.floor_int(), self.mod_one())
    }

    fn reduced_small(self) -> Rat {
        match self {
            Rat::Small(n, d) => Rat::from_i128(n as i128, d as i128),
            other => other,
        }
    }

    /// Multiplicative inverse. Panics if `self == 0`.
    pub fn recip(&self) -> Rat {
        match self {
            Rat::Small(n, d) => {
                assert!(*n != 0, "reciprocal of zero");
                Rat::from_i128(*d as i128, *n as i128)
            }
            Rat::Big(r) => Rat::from_big(r.recip()),
        }
    }

    #[inline]
    pub fn is_negative(&self) -> bool {
        match self {
            Rat::Small(n, _) => *n < 0,
            Rat::Big(r) => r.is_negative(),
        }
    }

    pub fn abs(&self) -> Rat {
        if self.is_negative() {
            -self.clone()
        } else {
            self.clone()
        }
    }
}

// ---- arithmetic ----

impl std::ops::Add for &Rat {
    type Output = Rat;
    fn add(self, rhs: &Rat) -> Rat {
        match (self, rhs) {
            (Rat::Small(a, b), Rat::Small(c, d)) => {
                // a/b + c/d = (a*d + c*b) / (b*d)
                let ad = (*a as i128).checked_mul(*d as i128);
                let cb = (*c as i128).checked_mul(*b as i128);
                let bd = (*b as i128).checked_mul(*d as i128);
                match (ad, cb, bd) {
                    (Some(ad), Some(cb), Some(bd)) => match ad.checked_add(cb) {
                        Some(num) => Rat::from_i128(num, bd),
                        None => Rat::from_big(self.to_big() + rhs.to_big()),
                    },
                    _ => Rat::from_big(self.to_big() + rhs.to_big()),
                }
            }
            _ => Rat::from_big(self.to_big() + rhs.to_big()),
        }
    }
}

impl std::ops::Sub for &Rat {
    type Output = Rat;
    fn sub(self, rhs: &Rat) -> Rat {
        self + &(-rhs.clone())
    }
}

impl std::ops::Mul for &Rat {
    type Output = Rat;
    fn mul(self, rhs: &Rat) -> Rat {
        match (self, rhs) {
            (Rat::Small(a, b), Rat::Small(c, d)) => {
                let num = (*a as i128).checked_mul(*c as i128);
                let den = (*b as i128).checked_mul(*d as i128);
                match (num, den) {
                    (Some(num), Some(den)) => Rat::from_i128(num, den),
                    _ => Rat::from_big(self.to_big() * rhs.to_big()),
                }
            }
            _ => Rat::from_big(self.to_big() * rhs.to_big()),
        }
    }
}

impl std::ops::Div for &Rat {
    type Output = Rat;
    // Division is multiplication by the reciprocal; the `*` is expected here.
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn div(self, rhs: &Rat) -> Rat {
        self * &rhs.recip()
    }
}

impl std::ops::Neg for Rat {
    type Output = Rat;
    fn neg(self) -> Rat {
        match self {
            Rat::Small(n, d) => Rat::Small(-n, d),
            Rat::Big(r) => Rat::from_big(-(*r)),
        }
    }
}

impl std::ops::Neg for &Rat {
    type Output = Rat;
    fn neg(self) -> Rat {
        -self.clone()
    }
}

// Convenience owned-operand operators.
macro_rules! owned_binop {
    ($tr:ident, $meth:ident) => {
        impl std::ops::$tr for Rat {
            type Output = Rat;
            fn $meth(self, rhs: Rat) -> Rat {
                std::ops::$tr::$meth(&self, &rhs)
            }
        }
    };
}
owned_binop!(Add, add);
owned_binop!(Sub, sub);
owned_binop!(Mul, mul);
owned_binop!(Div, div);

// ---- equality, ordering, hashing (all structural thanks to canonical form) ----

impl PartialEq for Rat {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Rat::Small(a, b), Rat::Small(c, d)) => a == c && b == d,
            (Rat::Big(a), Rat::Big(b)) => a == b,
            // A value that fits in i64 is always Small, so a Small can never
            // equal a Big.
            _ => false,
        }
    }
}
impl Eq for Rat {}

impl Hash for Rat {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Rat::Small(n, d) => {
                0u8.hash(state);
                n.hash(state);
                d.hash(state);
            }
            Rat::Big(r) => {
                1u8.hash(state);
                r.hash(state);
            }
        }
    }
}

impl PartialOrd for Rat {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Rat {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Rat::Small(a, b), Rat::Small(c, d)) => {
                // a/b vs c/d  (b,d > 0)  ->  a*d vs c*b
                ((*a as i128) * (*d as i128)).cmp(&((*c as i128) * (*b as i128)))
            }
            _ => self.to_big().cmp(&other.to_big()),
        }
    }
}

impl fmt::Debug for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rat::Small(n, 1) => write!(f, "{n}"),
            Rat::Small(n, d) => write!(f, "{n}/{d}"),
            Rat::Big(r) => write!(f, "{}/{}", r.numer(), r.denom()),
        }
    }
}
impl fmt::Display for Rat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl From<i64> for Rat {
    fn from(n: i64) -> Rat {
        Rat::from_int(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_reduction() {
        assert_eq!(Rat::new(2, 4), Rat::new(1, 2));
        assert_eq!(Rat::new(-2, -4), Rat::new(1, 2));
        assert_eq!(Rat::new(2, -4), Rat::new(-1, 2));
        assert_eq!(Rat::new(0, 5), Rat::zero());
    }

    #[test]
    fn arithmetic() {
        let a = Rat::new(1, 2);
        let b = Rat::new(1, 3);
        assert_eq!(&a + &b, Rat::new(5, 6));
        assert_eq!(&a - &b, Rat::new(1, 6));
        assert_eq!(&a * &b, Rat::new(1, 6));
        assert_eq!(&a / &b, Rat::new(3, 2));
        assert_eq!(-a.clone(), Rat::new(-1, 2));
    }

    #[test]
    fn mod_one_matches_python() {
        // Python: Fraction(-1,3) % 1 == Fraction(2,3)
        assert_eq!(Rat::new(-1, 3).mod_one(), Rat::new(2, 3));
        assert_eq!(Rat::new(1, 3).mod_one(), Rat::new(1, 3));
        assert_eq!(Rat::new(7, 3).mod_one(), Rat::new(1, 3));
        assert_eq!(Rat::new(-7, 3).mod_one(), Rat::new(2, 3));
        assert_eq!(Rat::new(4, 2).mod_one(), Rat::zero());
        assert_eq!(Rat::new(3, 1).mod_one(), Rat::zero());
    }

    #[test]
    fn overflow_promotes_to_big() {
        let big = Rat::new(i64::MAX, 1);
        let two = Rat::from_int(2);
        let r = &big * &two; // 2*i64::MAX overflows i64
        assert!(matches!(r, Rat::Big(_)));
        // And it demotes again when it comes back into range.
        let back = &r / &two;
        assert_eq!(back, big);
        assert!(matches!(back, Rat::Small(_, _)));
    }

    #[test]
    fn big_small_never_equal_but_compare() {
        let big = Rat::new(i64::MAX, 1) + Rat::from_int(1); // Big
        assert!(matches!(big, Rat::Big(_)));
        assert!(big > Rat::new(i64::MAX, 1));
    }

    #[test]
    fn ordering() {
        assert!(Rat::new(1, 3) < Rat::new(1, 2));
        assert!(Rat::new(-1, 2) < Rat::new(1, 100));
        assert!(Rat::new(5, 10) == Rat::new(1, 2));
    }

    #[test]
    fn floor_and_recip() {
        assert_eq!(Rat::new(7, 2).floor_int(), BigInt::from(3));
        assert_eq!(Rat::new(-7, 2).floor_int(), BigInt::from(-4));
        assert_eq!(Rat::new(3, 4).recip(), Rat::new(4, 3));
    }

    // The `Rat` fast path plus overflow promotion must agree exactly with
    // arbitrary-precision arithmetic on every input. We fuzz against
    // `BigRational` as the reference oracle. Full-range `i64` operands force the
    // i128-overflow-to-bignum fallback to be exercised.
    use proptest::prelude::*;

    fn big(n: i64, d: i64) -> BigRational {
        BigRational::new(BigInt::from(n), BigInt::from(d))
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(4000))]

        #[test]
        fn rat_matches_bigrational(a in any::<i64>(), b in any::<i64>(), c in any::<i64>(), d in any::<i64>()) {
            prop_assume!(b != 0 && d != 0);
            let (ra, rb) = (Rat::new(a, b), Rat::new(c, d));
            let (ba, bb) = (big(a, b), big(c, d));

            prop_assert_eq!((&ra + &rb).to_big(), &ba + &bb);
            prop_assert_eq!((&ra - &rb).to_big(), &ba - &bb);
            prop_assert_eq!((&ra * &rb).to_big(), &ba * &bb);
            prop_assert_eq!((-ra.clone()).to_big(), -ba.clone());

            // Ordering and equality must match too.
            prop_assert_eq!(ra.cmp(&rb), ba.cmp(&bb));
            prop_assert_eq!(ra == rb, ba == bb);

            if !rb.is_zero() {
                prop_assert_eq!((&ra / &rb).to_big(), &ba / &bb);
            }
            // mod_one matches Python-style floored remainder.
            let m = ra.mod_one();
            prop_assert_eq!(m.to_big(), &ba - ba.floor());
        }
    }
}
