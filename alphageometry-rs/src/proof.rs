//! Proof provenance: recording *why* each fact was derived so a human-readable
//! proof can be reconstructed.
//!
//! The AlphaGeometry2 open-source release performs "elimination of variables,
//! **without proof**" — it answers yes/no. The original AlphaGeometry, by
//! contrast, printed numbered proofs (`001. premise & premise ⇒ conclusion`).
//! This module ports that capability to the Rust engine.
//!
//! Design: every constraint row inserted into the Gaussian elimination cores
//! carries a set of [`FactId`]s (see `ElimCore::row_deps`). When a goal's
//! linear combination reduces to zero, the union of the used rows' fact sets is
//! exactly the facts the reduction relied on. Each fact records a structured
//! [`Reason`] plus its premise facts, so a backward closure from the goal yields
//! a compact proof DAG which [`ProofLog::report`] renders as numbered steps.
//!
//! The fact sets are a sound *over-approximation*: row updates union
//! dependencies at row granularity, so a step may occasionally cite a premise
//! that a hand-minimized proof could omit — but every cited premise genuinely
//! entered the algebra that established the goal.

use crate::predicate::PointId;

/// Identifier of a recorded fact.
pub type FactId = u32;

type Pair = (PointId, PointId);
type Triple = (PointId, PointId, PointId);

/// Structured justification of a fact (rendered lazily with point names).
#[derive(Clone, Debug)]
pub enum Reason {
    /// A hypothesis of the problem, pre-rendered (they are few).
    Assumption(String),
    /// The defining fact of an auxiliary point a prover introduced.
    Construction(String),
    /// Two triangles matched as similar by the closure search.
    SimilarTriangles(Triple, Triple),
    /// Points found concyclic (inscribed-angle criterion / equal radii).
    Concyclic(Vec<PointId>),
    /// Points found collinear.
    Collinear(Vec<PointId>),
    /// A circle recognized from equal distances to a center.
    EqualRadius(PointId, Vec<PointId>),
    /// Two numerically-equal points proved equal and merged.
    PointMerge(PointId, PointId),
    /// Additive/multiplicative distance transfer on matching segments.
    TransferAddMul(Pair, Pair),
    /// Equal arcs ⇔ equal chords on a circle.
    TransferArcChord(Pair, Pair),
    /// A named classical theorem applied to the listed points.
    Theorem(&'static str, Vec<PointId>),
}

/// One recorded fact: its justification and the facts it relied upon.
#[derive(Clone, Debug)]
pub struct Fact {
    pub reason: Reason,
    pub premises: Vec<FactId>,
}

/// Append-only log of facts derived during the closure.
#[derive(Clone, Default)]
pub struct ProofLog {
    pub facts: Vec<Fact>,
}

impl ProofLog {
    pub fn new() -> ProofLog {
        ProofLog::default()
    }

    pub fn add(&mut self, reason: Reason, mut premises: Vec<FactId>) -> FactId {
        premises.sort_unstable();
        premises.dedup();
        let id = self.facts.len() as FactId;
        self.facts.push(Fact { reason, premises });
        id
    }

    /// Backward closure from the facts used by the goal.
    pub fn closure(&self, used: &[FactId]) -> Vec<FactId> {
        let mut seen = vec![false; self.facts.len()];
        let mut stack: Vec<FactId> = used.to_vec();
        while let Some(f) = stack.pop() {
            let i = f as usize;
            if i >= self.facts.len() || seen[i] {
                continue;
            }
            seen[i] = true;
            stack.extend(self.facts[i].premises.iter().copied());
        }
        let mut out: Vec<FactId> = (0..self.facts.len() as FactId)
            .filter(|&f| seen[f as usize])
            .collect();
        out.sort_unstable();
        out
    }

    /// Render a numbered proof in the spirit of the original AlphaGeometry
    /// (`001. premise & premise ⇒ conclusion`).
    pub fn report(&self, used: &[FactId], goal_text: &str, names: &[String]) -> String {
        let lines = self.step_lines(used, names);
        let mut out = String::new();
        out.push_str(&format!(
            "Proof of {goal_text} ({} steps, {} facts recorded in total):\n",
            lines.len(),
            self.facts.len()
        ));
        for line in &lines {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("∎ {goal_text}\n"));
        out
    }

    /// The numbered derivation lines (`001. reason [premises]`) of the backward
    /// closure from `used`, without a header or conclusion line.
    pub fn step_lines(&self, used: &[FactId], names: &[String]) -> Vec<String> {
        let steps = self.closure(used);
        let number: std::collections::HashMap<FactId, usize> =
            steps.iter().enumerate().map(|(i, &f)| (f, i + 1)).collect();
        steps
            .iter()
            .map(|&f| {
                let fact = &self.facts[f as usize];
                let cites: Vec<String> = fact
                    .premises
                    .iter()
                    .filter_map(|p| number.get(p).map(|n| format!("{n:03}")))
                    .collect();
                let arrow = if cites.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", cites.join(" & "))
                };
                format!(
                    "{:03}. {}{}",
                    number[&f],
                    render_reason(&fact.reason, names),
                    arrow
                )
            })
            .collect()
    }
}

fn nm(names: &[String], p: PointId) -> String {
    names
        .get(p as usize)
        .cloned()
        .unwrap_or_else(|| format!("p{p}"))
}

fn nms(names: &[String], ps: &[PointId]) -> String {
    ps.iter()
        .map(|&p| nm(names, p))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_reason(r: &Reason, names: &[String]) -> String {
    match r {
        Reason::Assumption(s) => format!("assumption: {s}"),
        Reason::Construction(s) => format!("construction: {s}"),
        Reason::SimilarTriangles((a, b, c), (x, y, z)) => format!(
            "similar triangles: △{}{}{} ∼ △{}{}{}",
            nm(names, *a),
            nm(names, *b),
            nm(names, *c),
            nm(names, *x),
            nm(names, *y),
            nm(names, *z)
        ),
        Reason::Concyclic(ps) => format!("concyclic (inscribed angles): {}", nms(names, ps)),
        Reason::Collinear(ps) => format!("collinear: {}", nms(names, ps)),
        Reason::EqualRadius(o, ps) => format!(
            "equal distances from {} ⇒ circle through {}",
            nm(names, *o),
            nms(names, ps)
        ),
        Reason::PointMerge(a, b) => format!(
            "points {} and {} coincide (two non-tangent objects share both)",
            nm(names, *a),
            nm(names, *b)
        ),
        Reason::TransferAddMul((a, b), (c, d)) => format!(
            "segment arithmetic: |{}{}| ↔ |{}{}| (add/mul transfer)",
            nm(names, *a),
            nm(names, *b),
            nm(names, *c),
            nm(names, *d)
        ),
        Reason::TransferArcChord((a, b), (c, d)) => format!(
            "equal arcs ⇔ equal chords: {}{} and {}{}",
            nm(names, *a),
            nm(names, *b),
            nm(names, *c),
            nm(names, *d)
        ),
        Reason::Theorem(name, ps) => format!("{name}: {}", nms(names, ps)),
    }
}

/// Merge two sorted, deduplicated dep vectors (used by the elimination cores).
pub fn merge_deps(dst: &mut Vec<FactId>, src: &[FactId]) {
    if src.is_empty() {
        return;
    }
    if dst.is_empty() {
        dst.extend_from_slice(src);
        return;
    }
    let mut out = Vec::with_capacity(dst.len() + src.len());
    let (mut i, mut j) = (0, 0);
    while i < dst.len() && j < src.len() {
        match dst[i].cmp(&src[j]) {
            std::cmp::Ordering::Less => {
                out.push(dst[i]);
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                out.push(src[j]);
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                out.push(dst[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out.extend_from_slice(&dst[i..]);
    out.extend_from_slice(&src[j..]);
    *dst = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_deps_dedups() {
        let mut a = vec![1, 3, 5];
        merge_deps(&mut a, &[2, 3, 6]);
        assert_eq!(a, vec![1, 2, 3, 5, 6]);
    }

    #[test]
    fn closure_follows_premises() {
        let mut log = ProofLog::new();
        let a = log.add(Reason::Assumption("p".into()), vec![]);
        let b = log.add(Reason::Assumption("q".into()), vec![]);
        let _unused = log.add(Reason::Assumption("r".into()), vec![]);
        let d = log.add(Reason::Collinear(vec![0, 1, 2]), vec![a, b]);
        let steps = log.closure(&[d]);
        assert_eq!(steps, vec![a, b, d]); // r not included
    }
}
