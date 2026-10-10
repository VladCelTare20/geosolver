use super::area::BaseRow;
use super::*;

const MAX_COS_MULTIPLIERS: usize = 8;

impl Figure {
    /// `cos f = τ·cos g` candidates: `|cos|` equal (and > 1e-6) in every
    /// instance with one sign `τ` throughout, pending the `eqangle` that makes
    /// the two angles equal or supplementary (orientation read from the figure).
    pub(in crate::ratio) fn cos_substitutions(&self, f: LKey) -> Vec<(LKey, Predicate, Rat)> {
        let LKey::Cos(v, p, q) = f else { return Vec::new() };
        let n = self.names.len() as PointId;
        let mut out = Vec::new();
        for w in 0..n {
            for r in 0..n {
                for s in (r + 1)..n {
                    let Some(LKey::Sin(w2, r2, s2)) = self.sin_atom(w, r, s) else { continue };
                    let g = LKey::Cos(w2, r2, s2);
                    if g == f {
                        continue;
                    }
                    let vals: Vec<(f64, f64)> = (0..self.insts.len())
                        .map(|i| (self.cos_val(i, v, p, q), self.cos_val(i, w2, r2, s2)))
                        .collect();
                    if !vals.iter().all(|(x, y)| x.abs() > 1e-6 && (x.abs() - y.abs()).abs() < 1e-9 * x.abs().max(y.abs())) {
                        continue;
                    }
                    let tau = (vals[0].0 * vals[0].1).signum();
                    if !vals.iter().all(|(x, y)| (x * y).signum() == tau) {
                        continue;
                    }
                    let Some(fact) = self.equal_sine_fact(LKey::Sin(v, p, q), LKey::Sin(w2, r2, s2)) else { continue };
                    out.push((g, fact, Rat::from_int(tau as i64)));
                }
            }
        }
        out
    }

    /// T7 rows over the triangles on `gpts`: the law of cosines at every
    /// vertex (headline), `sin² + cos² = 1`, and `cos = 0` for a right angle
    /// (pending `perp`).
    fn cosine_base_rows(&self, gpts: &[PointId]) -> Vec<BaseRow> {
        let mut out = Vec::new();
        for (i, &a) in gpts.iter().enumerate() {
            for (j, &b) in gpts.iter().enumerate().skip(i + 1) {
                for &c in &gpts[j + 1..] {
                    if self.sin_atom(a, b, c).is_none() || self.sin_atom(b, c, a).is_none() || self.sin_atom(c, a, b).is_none() {
                        continue;
                    }
                    for (x, y, z) in [(a, b, c), (b, c, a), (c, a, b)] {
                        let Some(LKey::Sin(v, p, q)) = self.sin_atom(x, y, z) else { continue };
                        let (sk, ck) = (LKey::Sin(v, p, q), LKey::Cos(v, p, q));
                        let (yz, xy, xz): (LKey, LKey, LKey) = (latom(y, z).into(), latom(x, y).into(), latom(x, z).into());
                        let sorted = |mut m: Mono| {
                            m.sort();
                            m
                        };
                        let mut e = PEq::default();
                        e.add(sorted(vec![yz, yz]), Rat::one());
                        e.add(sorted(vec![xy, xy]), -Rat::one());
                        e.add(sorted(vec![xz, xz]), -Rat::one());
                        e.add(sorted(vec![xy, xz, ck]), Rat::from_int(2));
                        out.push(BaseRow {
                            eq: e,
                            text: format!(
                                "Law of cosines in △{}{}{}: {}² = {}² + {}² − 2·{}·{}·{}",
                                self.nm(a),
                                self.nm(b),
                                self.nm(c),
                                self.seg(y, z),
                                self.seg(x, y),
                                self.seg(x, z),
                                self.seg(x, y),
                                self.seg(x, z),
                                self.factor_text(ck)
                            ),
                            pending: None,
                            headline: true,
                            support: false,
                        });
                        let mut e = PEq::default();
                        e.add(vec![sk, sk], Rat::one());
                        e.add(vec![ck, ck], Rat::one());
                        e.add(vec![], -Rat::one());
                        out.push(BaseRow {
                            eq: e,
                            text: format!("{}² + {}² = 1", self.factor_text(sk), self.factor_text(ck)),
                            pending: None,
                            headline: false,
                            support: true,
                        });
                        if (0..self.insts.len()).all(|i| self.cos_val(i, v, p, q).abs() < 1e-9) {
                            let mut e = PEq::default();
                            e.add(vec![ck], Rat::one());
                            out.push(BaseRow {
                                eq: e,
                                text: format!("{} = 90° (derived from the hypotheses), so {} = 0", self.angle_text(ck), self.factor_text(ck)),
                                pending: Some(vec![pred("perp", &[v, p, v, q])]),
                                headline: false,
                                support: false,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// Stage S8 (T7): a goal with a cosine factor, multiplied by nothing or
    /// by one goal length (non-zero in every instance), closed over the
    /// law-of-cosines rows and the signed cosine substitutions, then solved.
    pub(in crate::ratio) fn prove_by_cosines(&self, lhs: &MExpr, rhs: &MExpr, goal_text: &str, pts: &[PointId]) -> Option<String> {
        let lm = mono_lower(lhs, self)?;
        let rm = mono_lower(rhs, self)?;
        let goal_map = mono_add(&lm, &rm, &-Rat::one());
        if goal_map.is_empty() || !goal_map.keys().flatten().any(|k| matches!(k, LKey::Cos(..))) {
            return None;
        }
        let mut goal = PEq::default();
        for (m, c) in &goal_map {
            goal.add(m.clone(), c.clone());
        }
        let gpts: Vec<PointId> = {
            let s: BTreeSet<PointId> = goal.terms.keys().flatten().flat_map(|&k| k.points()).collect();
            s.into_iter().collect()
        };
        let mut multipliers: Vec<Mono> = vec![vec![]];
        let lens: BTreeSet<LKey> = goal.terms.keys().flatten().copied().filter(|k| matches!(k, LKey::Len(..))).collect();
        multipliers.extend(lens.into_iter().map(|k| vec![k]));
        multipliers.truncate(MAX_COS_MULTIPLIERS);
        for mult in multipliers {
            let mut f = self.lean_clone();
            let mut g = PEq::default();
            for (m, c) in &goal.terms {
                let mut t = m.clone();
                t.extend_from_slice(&mult);
                t.sort();
                g.add(t, c.clone());
            }
            let mut pool: BTreeSet<Mono> = g.terms.keys().cloned().collect();
            let base = f.cosine_base_rows(&gpts);
            f.area_closure(&base, &mut pool);
            let goal_pts: BTreeSet<PointId> = gpts.iter().copied().collect();
            f.gather_trig(pts, &goal_pts);
            let goal_monos: BTreeSet<Mono> = g.terms.keys().cloned().collect();
            f.gather_log_bridges(&pool, &goal_monos);
            let Some(used) = f.certified_products(&g) else { continue };
            if used.is_empty() {
                continue;
            }
            let val = eval_numeric(lhs, &f).unwrap_or(f64::NAN);
            if mult.is_empty() {
                return Some(f.prender(&used, goal_text, val, &[]));
            }
            let mt = f.mono_text(&mult);
            let pre = format!("Multiply both sides by {mt}, which is not 0 (distinct points in the figure).");
            let proof = f.prender(&used, goal_text, val, std::slice::from_ref(&pre));
            return Some(proof.replace(
                "\n  Adding the relations above gives",
                &format!("\n  Adding the relations above and dividing by {mt} gives"),
            ));
        }
        None
    }
}
