//! Sparse linear combinations of variables over [`Rat`].
//!
//! A [`LinComb`] is kept in a canonical form — terms sorted by variable id with
//! no zero coefficients — so that [`PartialEq`]/[`Eq`]/[`Hash`] are structural.
//! The closure search relies on this: two geometric quantities are provably
//! equal iff their simplified linear combinations are *identical*, and we bucket
//! them in hash maps keyed by the combination itself.

use crate::rational::Rat;
use smallvec::SmallVec;
use std::hash::Hash;

/// Identifier of an elimination variable within a single system.
pub type VarId = u32;

/// A sparse linear combination `sum_i coef_i * var_i`, in canonical form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct LinComb {
    /// `(var, coef)` pairs, sorted ascending by `var`, all coefficients nonzero.
    pub terms: SmallVec<[(VarId, Rat); 4]>,
}

impl LinComb {
    #[inline]
    pub fn zero() -> LinComb {
        LinComb {
            terms: SmallVec::new(),
        }
    }

    /// A single term `coef * var`.
    pub fn singleton(var: VarId, coef: Rat) -> LinComb {
        if coef.is_zero() {
            LinComb::zero()
        } else {
            let mut terms = SmallVec::new();
            terms.push((var, coef));
            LinComb { terms }
        }
    }

    #[inline]
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// Alias of [`Self::is_zero`]; an empty combination has no terms.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Coefficient of `var` (zero if absent).
    pub fn get(&self, var: VarId) -> Rat {
        match self.terms.binary_search_by_key(&var, |(v, _)| *v) {
            Ok(i) => self.terms[i].1.clone(),
            Err(_) => Rat::zero(),
        }
    }

    /// `self += other * coef`, preserving canonical form.
    ///
    /// This is the hot inner loop of both simplification and constraint
    /// insertion; it is a linear merge of two sorted term lists.
    pub fn iadd_mul(&mut self, other: &LinComb, coef: &Rat) {
        if coef.is_zero() || other.is_zero() {
            return;
        }
        let mut result: SmallVec<[(VarId, Rat); 4]> =
            SmallVec::with_capacity(self.terms.len() + other.terms.len());
        let mut i = 0;
        let mut j = 0;
        let a = &self.terms;
        let b = &other.terms;
        while i < a.len() && j < b.len() {
            match a[i].0.cmp(&b[j].0) {
                std::cmp::Ordering::Less => {
                    result.push(a[i].clone());
                    i += 1;
                }
                std::cmp::Ordering::Greater => {
                    let c = &b[j].1 * coef;
                    if !c.is_zero() {
                        result.push((b[j].0, c));
                    }
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let c = &a[i].1 + &(&b[j].1 * coef);
                    if !c.is_zero() {
                        result.push((a[i].0, c));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        while i < a.len() {
            result.push(a[i].clone());
            i += 1;
        }
        while j < b.len() {
            let c = &b[j].1 * coef;
            if !c.is_zero() {
                result.push((b[j].0, c));
            }
            j += 1;
        }
        self.terms = result;
    }

    /// In-place scalar multiply.
    pub fn mul_assign_scalar(&mut self, coef: &Rat) {
        if coef.is_zero() {
            self.terms.clear();
            return;
        }
        for (_, c) in self.terms.iter_mut() {
            *c = &*c * coef;
        }
    }

    /// Add a single term `coef * var`.
    pub fn add_term(&mut self, var: VarId, coef: Rat) {
        if coef.is_zero() {
            return;
        }
        match self.terms.binary_search_by_key(&var, |(v, _)| *v) {
            Ok(i) => {
                let c = &self.terms[i].1 + &coef;
                if c.is_zero() {
                    self.terms.remove(i);
                } else {
                    self.terms[i].1 = c;
                }
            }
            Err(i) => self.terms.insert(i, (var, coef)),
        }
    }
}

impl std::ops::Add for &LinComb {
    type Output = LinComb;
    fn add(self, rhs: &LinComb) -> LinComb {
        let mut r = self.clone();
        r.iadd_mul(rhs, &Rat::one());
        r
    }
}
impl std::ops::Sub for &LinComb {
    type Output = LinComb;
    fn sub(self, rhs: &LinComb) -> LinComb {
        let mut r = self.clone();
        r.iadd_mul(rhs, &Rat::from_int(-1));
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_and_merge() {
        let mut a = LinComb::singleton(2, Rat::from_int(1));
        a.add_term(1, Rat::from_int(3));
        // sorted by var id
        assert_eq!(a.terms[0].0, 1);
        assert_eq!(a.terms[1].0, 2);

        let b = LinComb::singleton(2, Rat::from_int(-1));
        a.iadd_mul(&b, &Rat::one());
        // var 2 cancels
        assert_eq!(a.get(2), Rat::zero());
        assert_eq!(a.get(1), Rat::from_int(3));
    }

    #[test]
    fn equality_is_structural() {
        let mut a = LinComb::zero();
        a.add_term(5, Rat::new(1, 2));
        a.add_term(3, Rat::from_int(2));
        let mut b = LinComb::zero();
        b.add_term(3, Rat::from_int(2));
        b.add_term(5, Rat::new(1, 2));
        assert_eq!(a, b);
    }

    #[test]
    fn scalar_mul() {
        let mut a = LinComb::singleton(1, Rat::new(1, 2));
        a.add_term(2, Rat::from_int(3));
        a.mul_assign_scalar(&Rat::from_int(2));
        assert_eq!(a.get(1), Rat::from_int(1));
        assert_eq!(a.get(2), Rat::from_int(6));
    }
}
