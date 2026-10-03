//! Linear fingerprints of [`LinComb`]s modulo the Mersenne prime 2^61 - 1.
//!
//! Equal combinations always have equal fingerprints, so a fingerprint can
//! only rule an equality *out*; every equality the engine acts on is
//! re-checked exactly.

use crate::lincomb::LinComb;
use crate::rational::Rat;
use num_bigint::BigInt;
use num_traits::{ToPrimitive, Zero};
use rustc_hash::FxHashMap;
use std::cell::RefCell;

pub const P: u64 = (1u64 << 61) - 1;

#[inline]
fn reduce(x: u128) -> u64 {
    let s = ((x as u64) & P) + ((x >> 61) as u64);
    let s = (s & P) + (s >> 61);
    if s >= P {
        s - P
    } else {
        s
    }
}

#[inline]
pub fn mul(a: u64, b: u64) -> u64 {
    reduce(a as u128 * b as u128)
}

#[inline]
pub fn add(a: u64, b: u64) -> u64 {
    let s = a + b;
    if s >= P {
        s - P
    } else {
        s
    }
}

#[inline]
pub fn sub(a: u64, b: u64) -> u64 {
    if a >= b {
        a - b
    } else {
        a + P - b
    }
}

fn pow(mut b: u64, mut e: u64) -> u64 {
    let mut r = 1u64;
    while e > 0 {
        if e & 1 == 1 {
            r = mul(r, b);
        }
        b = mul(b, b);
        e >>= 1;
    }
    r
}

pub fn weight(var: u32) -> u64 {
    let mut z = (var as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z % P).max(1)
}

thread_local! {
    static INVERSES: RefCell<FxHashMap<u64, u64>> = RefCell::new(FxHashMap::default());
}

fn inverse(d: u64) -> Option<u64> {
    const SMALL: usize = 64;
    static SMALL_INVERSES: std::sync::OnceLock<[u64; SMALL]> = std::sync::OnceLock::new();
    if d == 0 {
        return None;
    }
    if (d as usize) < SMALL {
        let table = SMALL_INVERSES.get_or_init(|| {
            let mut t = [0u64; SMALL];
            for (i, x) in t.iter_mut().enumerate().skip(1) {
                *x = pow(i as u64, P - 2);
            }
            t
        });
        return Some(table[d as usize]);
    }
    Some(INVERSES.with(|m| *m.borrow_mut().entry(d).or_insert_with(|| pow(d, P - 2))))
}

fn residue_int(n: i128) -> u64 {
    n.rem_euclid(P as i128) as u64
}

fn residue_big(n: &BigInt) -> u64 {
    let r = n % BigInt::from(P);
    let r = if r < BigInt::zero() { r + BigInt::from(P) } else { r };
    r.to_u64().expect("residue fits")
}

pub fn rat(r: &Rat) -> Option<u64> {
    match r {
        Rat::Small(n, 1) => Some(residue_int(*n as i128)),
        Rat::Small(n, d) => Some(mul(residue_int(*n as i128), inverse(residue_int(*d as i128))?)),
        Rat::Big(b) => Some(mul(
            residue_big(b.numer()),
            inverse(residue_big(b.denom()))?,
        )),
    }
}

/// Fingerprint of `comb` with the term of `skip` left out.
pub fn lincomb_without(comb: &LinComb, skip: Option<u32>) -> Option<u64> {
    let mut acc = 0u64;
    for (v, c) in &comb.terms {
        if Some(*v) == skip {
            continue;
        }
        acc = add(acc, mul(rat(c)?, weight(*v)));
    }
    Some(acc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lincomb::LinComb;
    use proptest::prelude::*;

    fn comb(terms: &[(u32, i64, i64)]) -> LinComb {
        let mut c = LinComb::zero();
        for &(v, n, d) in terms {
            c.add_term(v, Rat::new(n, d));
        }
        c
    }

    proptest! {
        #[test]
        fn fingerprint_is_linear(
            xs in proptest::collection::vec((0u32..6, -50i64..50, 1i64..13), 0..6),
            ys in proptest::collection::vec((0u32..6, -50i64..50, 1i64..13), 0..6),
        ) {
            let (x, y) = (comb(&xs), comb(&ys));
            let d = &x - &y;
            let fx = lincomb_without(&x, None).unwrap();
            let fy = lincomb_without(&y, None).unwrap();
            prop_assert_eq!(lincomb_without(&d, None).unwrap(), sub(fx, fy));
            if d.is_zero() {
                prop_assert_eq!(fx, fy);
            }
        }
    }

    #[test]
    fn big_rationals_reduce() {
        let big = &Rat::new(i64::MAX, 3) * &Rat::new(i64::MAX, 5);
        assert!(matches!(big, Rat::Big(_)));
        let back = &big * &Rat::new(15, 1);
        let lhs = mul(rat(&big).unwrap(), rat(&Rat::from_int(15)).unwrap());
        assert_eq!(lhs, rat(&back).unwrap());
    }
}
