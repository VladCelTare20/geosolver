//! Certification of candidate facts by the DDAR closure.
//!
//! The classical metric provers ([`crate::synthetic`], [`crate::ratio`]) may
//! notice a fact in the sampled figure — two triangles that look similar, a
//! point that looks like it divides a segment 2:1 — but may only *use* it once
//! it is derived from the hypotheses. [`Certifier::derive`] runs the deductive
//! closure over the construction's predicates and returns the numbered
//! derivation, or `None` when the fact does not follow.

use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::numerics::Vec2;
use crate::predicate::{Point, PointId, Predicate};
use crate::proof::FactId;
use crate::rational::Rat;
use crate::Ddar;

/// A predicate with no constants.
pub(crate) fn pred(name: &str, points: &[PointId]) -> Predicate {
    pred_c(name, points, Vec::new())
}

/// A predicate with constants (e.g. `rconst a b c d k`: `|ab| / |cd| = k`).
pub(crate) fn pred_c(name: &str, points: &[PointId], constants: Vec<Rat>) -> Predicate {
    Predicate {
        name: name.to_string(),
        points: points.to_vec(),
        constants,
    }
}

/// Derivation lines, indented under the proof step that relies on them.
pub(crate) fn derivation_block(lines: &[String]) -> String {
    lines.iter().map(|l| format!("\n       ↳ {l}")).collect()
}

/// `text` with `refs` attached to its first line (derivation lines follow).
pub(crate) fn with_refs(text: &str, refs: &str) -> String {
    match text.split_once('\n') {
        Some((head, rest)) => format!("{head}{refs}\n{rest}"),
        None => format!("{text}{refs}"),
    }
}

/// A small-denominator rational near `v` — a *candidate* read off the figure,
/// to be certified before it is used.
pub(crate) fn candidate_rat(v: f64) -> Option<Rat> {
    if !v.is_finite() || v.abs() > 1e6 {
        return None;
    }
    for den in 1..=12i64 {
        let n = v * den as f64;
        if (n - n.round()).abs() < 1e-6 * n.abs().max(1.0) {
            return Some(Rat::new(n.round() as i64, den));
        }
    }
    None
}

/// A lazily built DDAR closure over a figure whose points and facts only grow.
/// It is rebuilt whenever the point or fact count has changed since the last
/// build, so callers never have to invalidate it by hand.
#[derive(Default)]
pub(crate) struct Certifier {
    built_for: (usize, usize),
    closure: Option<Option<Ddar>>,
}

impl Certifier {
    /// Derive every predicate in `goals` from `preds` over the figure
    /// (`names`, `coords`); the first `hyps` predicates are the problem's
    /// hypotheses, the rest auxiliary constructions. Returns the numbered
    /// derivation, or `None` if any goal does not follow (or the closure could
    /// not be built).
    pub(crate) fn derive(
        &mut self,
        names: &[String],
        coords: &[Vec2],
        preds: &[Predicate],
        hyps: usize,
        goals: &[Predicate],
    ) -> Option<Vec<String>> {
        let deps = self.derive_deps(names, coords, preds, hyps, goals)?;
        Some(self.lines(&deps))
    }

    /// The numbered derivation of the facts `deps` in the current closure.
    pub(crate) fn lines(&self, deps: &[FactId]) -> Vec<String> {
        match &self.closure {
            Some(Some(ddar)) => ddar.proof_lines(deps),
            _ => Vec::new(),
        }
    }

    /// Like [`Certifier::derive`], returning the closure facts the goals rest
    /// on (render them with [`Certifier::lines`]).
    pub(crate) fn derive_deps(
        &mut self,
        names: &[String],
        coords: &[Vec2],
        preds: &[Predicate],
        hyps: usize,
        goals: &[Predicate],
    ) -> Option<Vec<FactId>> {
        let key = (names.len(), preds.len());
        if self.closure.is_none() || self.built_for != key {
            let points: Vec<Point> = names
                .iter()
                .zip(coords)
                .map(|(n, v)| Point {
                    name: n.clone(),
                    value: *v,
                })
                .collect();
            let built = crate::quiet_panic::quiet(|| {
                catch_unwind(AssertUnwindSafe(|| {
                    let mut d = Ddar::new_tracked(&points);
                    for (i, p) in preds.iter().enumerate() {
                        if i < hyps {
                            d.force_pred(p);
                        } else {
                            d.force_construction(p);
                        }
                    }
                    d.deduction_closure();
                    d
                }))
            })
            .ok();
            self.closure = Some(built);
            self.built_for = key;
        }
        let ddar = self.closure.as_mut()?.as_mut()?;
        let mut deps = Vec::new();
        for g in goals {
            let found =
                crate::quiet_panic::quiet(|| catch_unwind(AssertUnwindSafe(|| ddar.check_pred_deps(g))));
            match found {
                Ok(Some(d)) => deps.extend(d),
                Ok(None) => return None,
                Err(_) => {
                    self.closure = Some(None);
                    return None;
                }
            }
        }
        deps.sort_unstable();
        deps.dedup();
        Some(deps)
    }
}
