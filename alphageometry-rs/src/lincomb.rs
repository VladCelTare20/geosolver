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
    pub fn iadd_mul(&mut self, other: &LinComb, coef: &Rat) {
        if coef.is_zero() || other.is_zero() {
            return;
        }
        if self.is_zero() {
            self.terms = scaled_terms(&other.terms, coef);
            return;
        }
        self.terms = merge_terms(&self.terms, &other.terms, coef);
    }

    /// `a + b * coef` as a fresh combination.
    pub fn combine(a: &LinComb, b: &LinComb, coef: &Rat) -> LinComb {
        if coef.is_zero() || b.is_zero() {
            return a.clone();
        }
        if a.is_zero() {
            return LinComb {
                terms: scaled_terms(&b.terms, coef),
            };
        }
        LinComb {
            terms: merge_terms(&a.terms, &b.terms, coef),
        }
    }

    pub fn negated(&self) -> LinComb {
        LinComb {
            terms: self.terms.iter().map(|(v, c)| (*v, -c)).collect(),
        }
    }

    /// In-place scalar multiply.
    pub fn mul_assign_scalar(&mut self, coef: &Rat) {
        if coef.is_zero() {
            self.terms.clear();
            return;
        }
        if coef.is_one() {
            return;
        }
        for (_, c) in self.terms.iter_mut() {
            *c = scale(c, coef);
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

type Terms = SmallVec<[(VarId, Rat); 4]>;

#[inline]
fn scale(c: &Rat, coef: &Rat) -> Rat {
    if coef.is_one() {
        c.clone()
    } else if coef.is_minus_one() {
        -c
    } else {
        c * coef
    }
}

fn scaled_terms(b: &[(VarId, Rat)], coef: &Rat) -> Terms {
    let mut out = Terms::with_capacity(b.len());
    for (v, c) in b {
        let x = scale(c, coef);
        if !x.is_zero() {
            out.push((*v, x));
        }
    }
    out
}

fn merge_terms(a: &[(VarId, Rat)], b: &[(VarId, Rat)], coef: &Rat) -> Terms {
    let mut result = Terms::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].0.cmp(&b[j].0) {
            std::cmp::Ordering::Less => {
                result.push(a[i].clone());
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                let c = scale(&b[j].1, coef);
                if !c.is_zero() {
                    result.push((b[j].0, c));
                }
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                let c = if coef.is_one() {
                    &a[i].1 + &b[j].1
                } else if coef.is_minus_one() {
                    &a[i].1 - &b[j].1
                } else {
                    &a[i].1 + &(&b[j].1 * coef)
                };
                if !c.is_zero() {
                    result.push((a[i].0, c));
                }
                i += 1;
                j += 1;
            }
        }
    }
    result.extend(a[i..].iter().cloned());
    for (v, c) in &b[j..] {
        let x = scale(c, coef);
        if !x.is_zero() {
            result.push((*v, x));
        }
    }
    result
}

impl std::ops::Add for &LinComb {
    type Output = LinComb;
    fn add(self, rhs: &LinComb) -> LinComb {
        LinComb::combine(self, rhs, &Rat::one())
    }
}
impl std::ops::Sub for &LinComb {
    type Output = LinComb;
    fn sub(self, rhs: &LinComb) -> LinComb {
        LinComb::combine(self, rhs, &Rat::from_int(-1))
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
