//! Incremental Gaussian elimination over exact rationals.
//!
//! This is a direct, carefully-audited port of `ElimCore` in the reference
//! `elimination.py`. It maintains a system of linear equations in *reduced*
//! form and answers "is this linear combination a consequence of the system?"
//!
//! # Representation & invariants
//!
//! Variables are split into two classes:
//! * **LHS** variables — the geometric unknowns (a pair's direction, its
//!   log-distance, its segment length). These may be *pivoted* (eliminated).
//! * **RHS** variables — constants (the half-turn unit `pi`, prime logarithms).
//!   These are never pivoted and act as free parameters.
//!
//! For each pivoted variable `p` we store an equation `eq` (a [`LinComb`]) whose
//! value is identically zero and in which `p` has coefficient `-1`. Reading it
//! as `p = eq + p`, i.e. `p` equals the remaining terms, lets [`Self::simplify`]
//! substitute `p` away via `comb += coef * eq`.
//!
//! Two invariants are maintained:
//! 1. A pivoted variable never appears on the right-hand side of any stored
//!    equation. Hence a single substitution pass fully reduces a combination.
//! 2. `free_to_usage[v]` is exactly the set of pivots whose equation mentions
//!    the free variable `v`, enabling incremental fill updates.
//!
//! A key consequence (proved in the module tests conceptually): because
//! `simplify` reduces modulo the row space and is linear, the normal form is a
//! canonical coset representative. Therefore whether a combination reduces to
//! zero — and whether two combinations share a normal form — is *independent* of
//! pivot-selection order. We are free to use a different (deterministic) tie
//! break than the Python reference without changing any result.

use crate::lincomb::{LinComb, VarId};
use crate::proof::{merge_deps, FactId};
use crate::rational::Rat;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Default)]
pub struct ElimCore {
    /// Numeric value of each variable (for consistency checks / oracle).
    pub values: Vec<f64>,
    /// Whether each variable is an eliminable (LHS) unknown.
    pub is_lhs: Vec<bool>,
    /// Pivot preference: lower ranks are eliminated first. Every variable
    /// created by [`Self::new_var`] has rank 1, so with no ranked variables the
    /// pivot choice is exactly the fewest-usages rule.
    rank: Vec<u8>,
    /// pivot var -> its zeroed equation (with pivot coefficient `-1`).
    instantiated: FxHashMap<VarId, LinComb>,
    /// free var -> set of pivots whose equation mentions it.
    free_to_usage: FxHashMap<VarId, FxHashSet<VarId>>,
    /// pivot var -> facts its equation depends on (proof provenance).
    /// Only populated when `track` is set; sorted and deduplicated.
    row_deps: FxHashMap<VarId, Vec<FactId>>,
    /// Whether to maintain `row_deps` (small cost; off for bulk solving).
    pub track: bool,
    /// Rows refused because the figure contradicts them (a rule applied
    /// outside its hypotheses); never added.
    pub rejected: usize,
}

impl ElimCore {
    pub fn new() -> ElimCore {
        ElimCore::default()
    }

    /// Allocate a fresh variable, returning its id.
    pub fn new_var(&mut self, value: f64, is_lhs: bool) -> VarId {
        self.new_var_ranked(value, is_lhs, 1)
    }

    /// [`Self::new_var`] with an explicit pivot rank (0 = eliminate first).
    pub fn new_var_ranked(&mut self, value: f64, is_lhs: bool, rank: u8) -> VarId {
        let id = self.values.len() as VarId;
        self.values.push(value);
        self.is_lhs.push(is_lhs);
        self.rank.push(rank);
        id
    }

    /// Numeric value of a combination.
    pub fn value_of(&self, comb: &LinComb) -> f64 {
        let mut v = 0.0;
        for (var, coef) in &comb.terms {
            v += coef.to_f64() * self.values[*var as usize];
        }
        v
    }

    /// Reduce `comb` to its normal form modulo the current system, in place.
    pub fn simplify(&self, comb: &mut LinComb) {
        // Snapshot the pivots present. Substituting a pivot only introduces
        // free (non-pivot) variables, so remaining pivots keep their snapshot
        // coefficients and one pass suffices (invariant 1).
        let updates: Vec<(VarId, Rat)> = comb
            .terms
            .iter()
            .filter(|(v, _)| self.instantiated.contains_key(v))
            .map(|(v, c)| (*v, c.clone()))
            .collect();
        for (v, coef) in updates {
            if let Some(eq) = self.instantiated.get(&v) {
                comb.iadd_mul(eq, &coef);
            }
        }
    }

    /// Like [`Self::simplify`], additionally unioning the fact dependencies of
    /// every row used into `deps` (proof provenance).
    pub fn simplify_collect(&self, comb: &mut LinComb, deps: &mut Vec<FactId>) {
        let updates: Vec<(VarId, Rat)> = comb
            .terms
            .iter()
            .filter(|(v, _)| self.instantiated.contains_key(v))
            .map(|(v, c)| (*v, c.clone()))
            .collect();
        for (v, coef) in updates {
            if let Some(eq) = self.instantiated.get(&v) {
                comb.iadd_mul(eq, &coef);
                if let Some(d) = self.row_deps.get(&v) {
                    merge_deps(deps, d);
                }
            }
        }
    }

    /// Add the constraint `added_eq == 0` to the system.
    ///
    /// `fact`, when provided (and `track` is on), is the proof-log fact that
    /// justifies this constraint; the stored row also inherits the deps of
    /// every row used while reducing it.
    ///
    /// Returns `true` if this added new information (a fresh pivot), `false` if
    /// the constraint was already implied or trivial.
    pub fn add_constraint(&mut self, mut added_eq: LinComb, fact: Option<FactId>) -> bool {
        let mut new_deps: Vec<FactId> = match fact {
            Some(f) if self.track => vec![f],
            _ => Vec::new(),
        };
        if self.track {
            let mut collected = Vec::new();
            self.simplify_collect(&mut added_eq, &mut collected);
            merge_deps(&mut new_deps, &collected);
        } else {
            self.simplify(&mut added_eq);
        }

        // The eliminable variables remaining after reduction.
        let lhs_all: Vec<VarId> = added_eq
            .terms
            .iter()
            .filter(|(v, _)| self.is_lhs[*v as usize])
            .map(|(v, _)| *v)
            .collect();
        if lhs_all.is_empty() {
            return false;
        }

        // Fill-reducing pivot choice: fewest existing usages. Deterministic tie
        // break by variable id (see module docs: this does not affect results).
        let pivot = *lhs_all
            .iter()
            .min_by_key(|v| (self.rank[**v as usize], self.free_to_usage.get(v).map_or(0, |s| s.len())))
            .unwrap();
        let lhs: Vec<VarId> = lhs_all.into_iter().filter(|v| *v != pivot).collect();

        // Normalize so the pivot's coefficient becomes -1.
        let pivot_coef = added_eq.get(pivot);
        let scale = &Rat::from_int(-1) / &pivot_coef;
        added_eq.mul_assign_scalar(&scale);

        // Substitute the new pivot definition into every equation that used it.
        let users: Vec<VarId> = self
            .free_to_usage
            .get(&pivot)
            .map(|s| s.iter().copied().collect())
            .unwrap_or_default();
        for x in users {
            let coef2 = self.instantiated[&x].get(pivot);
            {
                let eq = self.instantiated.get_mut(&x).unwrap();
                eq.iadd_mul(&added_eq, &coef2);
            }
            // The updated row now also depends on the new row's facts.
            if self.track {
                merge_deps(self.row_deps.entry(x).or_default(), &new_deps);
            }
            // Update fill bookkeeping for the other free LHS vars of added_eq.
            for &y in &lhs {
                let present = !self.instantiated[&x].get(y).is_zero();
                if present {
                    self.free_to_usage.entry(y).or_default().insert(x);
                } else if let Some(s) = self.free_to_usage.get_mut(&y) {
                    s.remove(&x);
                }
            }
        }

        self.instantiated.insert(pivot, added_eq);
        if self.track {
            self.row_deps.insert(pivot, new_deps);
        }
        for &y in &lhs {
            self.free_to_usage.entry(y).or_default().insert(pivot);
        }
        true
    }

    /// Number of independent equations stored; grows with every new fact.
    pub fn rows(&self) -> usize {
        self.instantiated.len()
    }

    /// Whether a single variable already participates in the system (used to
    /// prune the similar-triangle search).
    pub fn var_encountered(&self, var: VarId) -> bool {
        self.instantiated.contains_key(&var)
            || self.free_to_usage.get(&var).is_some_and(|s| !s.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lincomb::LinComb;

    fn var(core: &mut ElimCore, val: f64) -> VarId {
        core.new_var(val, true)
    }

    #[test]
    fn detects_consequences() {
        let mut c = ElimCore::new();
        // x, y, z with x = 1, y = 2, z = 3 numerically.
        let x = var(&mut c, 1.0);
        let y = var(&mut c, 2.0);
        let z = var(&mut c, 3.0);

        // Constraint: x + y - z = 0  (1 + 2 - 3).
        let mut eq = LinComb::singleton(x, Rat::one());
        eq.add_term(y, Rat::one());
        eq.add_term(z, Rat::from_int(-1));
        assert!(c.add_constraint(eq, None));

        // Now x + y - z should reduce to zero.
        let mut probe = LinComb::singleton(x, Rat::one());
        probe.add_term(y, Rat::one());
        probe.add_term(z, Rat::from_int(-1));
        c.simplify(&mut probe);
        assert!(probe.is_zero());

        // 2x + 2y - 2z is also a consequence.
        let mut probe2 = LinComb::singleton(x, Rat::from_int(2));
        probe2.add_term(y, Rat::from_int(2));
        probe2.add_term(z, Rat::from_int(-2));
        c.simplify(&mut probe2);
        assert!(probe2.is_zero());

        // x - y is NOT a consequence.
        let mut probe3 = LinComb::singleton(x, Rat::one());
        probe3.add_term(y, Rat::from_int(-1));
        c.simplify(&mut probe3);
        assert!(!probe3.is_zero());
    }

    #[test]
    fn chained_constraints() {
        let mut c = ElimCore::new();
        let a = var(&mut c, 1.0);
        let b = var(&mut c, 1.0);
        let d = var(&mut c, 1.0);
        // a = b, b = d  => a = d.
        let mut e1 = LinComb::singleton(a, Rat::one());
        e1.add_term(b, Rat::from_int(-1));
        c.add_constraint(e1, None);
        let mut e2 = LinComb::singleton(b, Rat::one());
        e2.add_term(d, Rat::from_int(-1));
        c.add_constraint(e2, None);

        let mut probe = LinComb::singleton(a, Rat::one());
        probe.add_term(d, Rat::from_int(-1));
        c.simplify(&mut probe);
        assert!(probe.is_zero());
    }

    #[test]
    fn redundant_constraint_returns_false() {
        let mut c = ElimCore::new();
        let a = var(&mut c, 1.0);
        let b = var(&mut c, 1.0);
        let mut e1 = LinComb::singleton(a, Rat::one());
        e1.add_term(b, Rat::from_int(-1));
        assert!(c.add_constraint(e1.clone(), None));
        // Same constraint again -> no new info.
        assert!(!c.add_constraint(e1, None));
    }
}
