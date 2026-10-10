use super::classics::Pass;
use super::Ddar;
use crate::elimination::{dist_mul_sqrt, rat_sqrt, Angle, DistMul, DistSq};
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason};
use crate::rational::Rat;
use rustc_hash::FxHashMap;

type Seg = (PointId, PointId);

impl Ddar {
    /// `|ab|²` as a raw combination; zero for `a == b`, `None` for two
    /// numerically identical but unmerged points (no variable exists).
    fn sq_term(&self, a: PointId, b: PointId) -> Option<LinComb> {
        if a == b {
            return Some(LinComb::zero());
        }
        if self.num_identical(a, b) {
            return None;
        }
        Some(self.raw_dist_sq(a, b).0)
    }

    /// `Σ cᵢ·|pᵢqᵢ|²`, or `None` if a term has no variable.
    fn sq_comb(&self, terms: &[(PointId, PointId, Rat)]) -> Option<DistSq> {
        let mut comb = LinComb::zero();
        for (p, q, c) in terms {
            comb.iadd_mul(&self.sq_term(*p, *q)?, c);
        }
        Some(DistSq(comb))
    }

    /// `|ac|² + |bd|² − |ad|² − |bc|²`, which is `2·(a−b)·(d−c)`: zero iff
    /// `ab ⟂ cd`.
    fn sq_perp_comb(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<DistSq> {
        let one = Rat::one();
        let m1 = Rat::from_int(-1);
        self.sq_comb(&[
            (a, c, one.clone()),
            (b, d, one),
            (a, d, m1.clone()),
            (b, c, m1),
        ])
    }

    fn sq_force_new(&mut self, eq: &DistSq, reason: Reason, prem: impl FnOnce(&Ddar) -> Vec<FactId>) -> bool {
        if !self.dsq.holds_numerically(eq) || self.dsq.simplify(eq).is_zero() {
            return false;
        }
        let prem = prem(self);
        let fact = self.log.add(reason, prem);
        self.dsq.force_zero(eq, Some(fact))
    }

    /// The squared-length table (Yuclid, arXiv 2510.01346 §2.3.2): one variable
    /// `s(XY) = |XY|²` per segment, and only linear relations that are
    /// theorems —
    ///
    /// * `AB ⟂ CD ⇔ s(AC) + s(BD) = s(AD) + s(BC)` (Pythagoras when the lines
    ///   meet at a figure point), both directions;
    /// * `|AB| = q·|CD|` ⇔ `s(AB) = q²·s(CD)`, both directions;
    /// * Stewart's theorem for `D = (1−t)B + tC` with `|BD|/|BC|` a rational
    ///   constant (the median / Apollonius case is `t = ½`); the sign of `t`
    ///   is the betweenness branch, read from the figure.
    ///
    /// Orthocentre and perpendicular-bisector concurrency, radical axes with
    /// figure centres, and the British-flag family are linear consequences.
    pub(super) fn search_squared_lengths(&mut self) -> bool {
        let mut changed = false;
        if self.inputs_changed(Pass::SqFromPerpendiculars) {
            changed |= self.sq_from_perpendicular_lines();
        }
        if self.inputs_changed(Pass::SqFromRatios) {
            changed |= self.sq_from_ratios();
        }
        if self.inputs_changed(Pass::SqFromStewart) {
            changed |= self.sq_from_stewart();
        }
        if self.inputs_changed(Pass::SqDeriveRatios) {
            changed |= self.sq_derive_ratios();
        }
        if self.inputs_changed(Pass::SqDerivePerps) {
            changed |= self.sq_derive_perps();
        }
        changed
    }

    #[inline(never)]
    fn sq_from_perpendicular_lines(&mut self) -> bool {
        let live = self.live_lines.clone();
        let mut by_dir: FxHashMap<&Angle, Vec<usize>> = FxHashMap::default();
        let half = self.angle.const_ratio(1, 2);
        let mut dirs: Vec<Angle> = Vec::with_capacity(live.len());
        for &l in &live {
            let (a, b) = self.lines[l].main_pair;
            dirs.push(self.cached_dir(a, b).clone());
        }
        for (k, d) in dirs.iter().enumerate() {
            by_dir.entry(d).or_default().push(k);
        }
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (k, d) in dirs.iter().enumerate() {
            if let Some(ks) = by_dir.get(&d.add(&half)) {
                pairs.extend(ks.iter().filter(|&&k2| k2 > k).map(|&k2| (live[k], live[k2])));
            }
        }
        drop(by_dir);
        let mut changed = false;
        for (l1, l2) in pairs {
            let p1 = self.lines[l1].points.clone();
            let p2 = self.lines[l2].points.clone();
            let common = p1.iter().copied().find(|p| p2.contains(p));
            let a0 = common.unwrap_or(p1[0]);
            let c0 = common.unwrap_or(p2[0]);
            for &b in &p1 {
                if b == a0 || self.num_identical(a0, b) {
                    continue;
                }
                for &d in &p2 {
                    if d == c0 || self.num_identical(c0, d) {
                        continue;
                    }
                    let key = [a0, b, c0, d];
                    if !self.classics.sq_perp.insert(key) {
                        continue;
                    }
                    let Some(eq) = self.sq_perp_comb(a0, b, c0, d) else {
                        continue;
                    };
                    changed |= self.sq_force_new(
                        &eq,
                        Reason::Theorem("perpendicular ⇒ squared lengths (Pythagoras)", vec![a0, b, c0, d]),
                        |s| s.deps_of_angle_expr(a0, b, c0, d),
                    );
                }
            }
        }
        changed
    }

    #[inline(never)]
    fn sq_from_ratios(&mut self) -> bool {
        let active = self.active.clone();
        let mut first: FxHashMap<DistMul, ((PointId, PointId), Rat)> = FxHashMap::default();
        let mut todo: Vec<(Seg, Seg, Rat)> = Vec::new();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if self.num_identical(a, b) {
                    continue;
                }
                let (norm, coef) = self.dmul.normalize(self.cached_dist_mul(a, b));
                match first.get(&norm) {
                    Some((p0, c0)) => {
                        let r = &coef / c0;
                        todo.push((*p0, (a, b), &r * &r));
                    }
                    None => {
                        first.insert(norm, ((a, b), coef));
                    }
                }
            }
        }
        let mut changed = false;
        for ((a0, b0), (a, b), q) in todo {
            if !self.classics.sq_ratio.insert([a0, b0, a, b]) {
                continue;
            }
            let Some(eq) = self.sq_comb(&[(a, b, Rat::one()), (a0, b0, -&q)]) else {
                continue;
            };
            changed |= self.sq_force_new(
                &eq,
                Reason::Theorem("squares of proportional lengths", vec![a0, b0, a, b]),
                |s| s.deps_of_ratio_expr(a0, b0, a, b),
            );
        }
        changed
    }

    #[inline(never)]
    fn sq_from_stewart(&mut self) -> bool {
        let lines: Vec<usize> = self
            .live_lines
            .iter()
            .copied()
            .filter(|&l| self.lines[l].points.len() >= 3)
            .collect();
        let active = self.active.clone();
        let mut changed = false;
        for lid in lines {
            let pts = self.lines[lid].points.clone();
            let line_fact = self.lines[lid].fact;
            for x in 0..pts.len() {
                for y in (x + 1)..pts.len() {
                    for z in (y + 1)..pts.len() {
                        let (px, py, pz) = (pts[x], pts[y], pts[z]);
                        if !self.all_distinct_pts(&[px, py, pz]) {
                            continue;
                        }
                        let Some((b, d, c, lam)) = [(px, py, pz), (py, px, pz), (px, pz, py)]
                            .into_iter()
                            .find_map(|(b, d, c)| self.rational_ratio(b, c, b, d).map(|l| (b, d, c, l)))
                        else {
                            continue;
                        };
                        let (vb, vd, vc) = (self.coord(b), self.coord(d), self.coord(c));
                        let t = if (vd - vb).dot(vc - vb) > 0.0 { lam } else { -&lam };
                        let one_minus_t = &Rat::one() - &t;
                        let tt = &t * &one_minus_t;
                        for &a in &active {
                            if pts.contains(&a) || !self.all_distinct_pts(&[a, b, c, d]) {
                                continue;
                            }
                            if !self.classics.sq_stewart.insert([a, b, d, c]) {
                                continue;
                            }
                            let Some(eq) = self.sq_comb(&[
                                (a, d, Rat::one()),
                                (a, b, -&one_minus_t),
                                (a, c, -&t),
                                (b, c, tt.clone()),
                            ]) else {
                                continue;
                            };
                            changed |= self.sq_force_new(
                                &eq,
                                Reason::Theorem("Stewart's theorem", vec![a, b, d, c]),
                                |s| {
                                    let mut p = s.deps_of_ratio_expr(b, c, b, d);
                                    p.extend(line_fact);
                                    p
                                },
                            );
                        }
                    }
                }
            }
        }
        changed
    }

    fn all_distinct_pts(&self, pts: &[PointId]) -> bool {
        (0..pts.len()).all(|i| ((i + 1)..pts.len()).all(|j| !self.num_identical(pts[i], pts[j])))
    }

    /// `|cd| / |ab|` when the ratio table fixes it to a rational constant.
    fn rational_ratio(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Option<Rat> {
        let r = self.get_dist_ratio(a, b, c, d);
        let (norm, coef) = self.dmul.normalize(&r);
        norm.is_one().then_some(coef)
    }

    #[inline(never)]
    fn sq_derive_ratios(&mut self) -> bool {
        let active = self.active.clone();
        let mut first: FxHashMap<LinComb, ((PointId, PointId), Rat)> = FxHashMap::default();
        let mut todo: Vec<(Seg, Seg, Rat)> = Vec::new();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if self.num_identical(a, b) {
                    continue;
                }
                let mut key = self.dsq.simplify(&self.raw_dist_sq(a, b)).0;
                let Some((_, lead)) = key.terms.first() else { continue };
                let lead = lead.clone();
                key.mul_assign_scalar(&lead.recip());
                match first.get(&key) {
                    Some((p0, l0)) => todo.push((*p0, (a, b), &lead / l0)),
                    None => {
                        first.insert(key, ((a, b), lead));
                    }
                }
            }
        }
        let mut changed = false;
        for ((a0, b0), (a, b), q) in todo {
            let small = |x: Option<i64>| x.is_some_and(|v| v.unsigned_abs() <= 1_000_000_000);
            if q.is_negative() || q.is_zero() || !small(q.numer_i64()) || !small(q.denom_i64()) {
                continue;
            }
            let known = {
                let r = self.get_dist_ratio(a0, b0, a, b);
                match rat_sqrt(&q) {
                    Some(root) => {
                        let (norm, coef) = self.dmul.normalize(&r);
                        norm.is_one() && coef == root
                    }
                    None => {
                        let target = dist_mul_sqrt(&mut self.dmul, &q);
                        r == target
                    }
                }
            };
            if known {
                continue;
            }
            let root = dist_mul_sqrt(&mut self.dmul, &q);
            let rel = self
                .raw_dist_mul(a, b)
                .div(&self.raw_dist_mul(a0, b0))
                .div(&root);
            if (self.dmul.value_of(&rel) - 1.0).abs() > 1e-9 || self.dmul.simplify(&rel).is_one() {
                continue;
            }
            let Some(eq) = self.sq_comb(&[(a, b, Rat::one()), (a0, b0, -&q)]) else {
                continue;
            };
            let prem = self.dsq.simplify_deps(&eq).1;
            let fact = self
                .log
                .add(Reason::Theorem("lengths from squared lengths", vec![a0, b0, a, b]), prem);
            changed |= self.dmul.force_one(&rel, Some(fact));
        }
        changed
    }

    /// Figure lines that are perpendicular in the figure but not yet in the
    /// angle table: try every point pair of each for the squared-length
    /// criterion, exactly.
    #[inline(never)]
    fn sq_derive_perps(&mut self) -> bool {
        const TOL: f64 = 1e-9;
        let half = self.angle.const_ratio(1, 2);
        let mut lines: Vec<(f64, usize)> = self
            .live_lines
            .iter()
            .map(|&l| (self.lines[l].value.direction().rem_euclid(1.0), l))
            .collect();
        lines.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for &(t, l1) in &lines {
            let want = (t + 0.5).rem_euclid(1.0);
            for target in [want - 1.0, want, want + 1.0] {
                let lo = lines.partition_point(|e| e.0 < target - TOL);
                for &(f, l2) in &lines[lo..] {
                    if f > target + TOL {
                        break;
                    }
                    if l1 < l2 {
                        pairs.push((l1, l2));
                    }
                }
            }
        }
        let mut changed = false;
        for (l1, l2) in pairs {
            let (m1, m2) = (self.lines[l1].main_pair, self.lines[l2].main_pair);
            let rel = self
                .raw_dir(m1.0, m1.1)
                .sub(&self.raw_dir(m2.0, m2.1))
                .sub(&half);
            if self.angle.simplify(&rel).is_zero() {
                continue;
            }
            let p1 = self.lines[l1].points.clone();
            let p2 = self.lines[l2].points.clone();
            let mut hit: Option<([PointId; 4], Vec<FactId>)> = None;
            'search: for i in 0..p1.len() {
                for j in (i + 1)..p1.len() {
                    for k in 0..p2.len() {
                        for l in (k + 1)..p2.len() {
                            let (a, b, c, d) = (p1[i], p1[j], p2[k], p2[l]);
                            if self.num_identical(a, b) || self.num_identical(c, d) {
                                continue;
                            }
                            let Some(eq) = self.sq_perp_comb(a, b, c, d) else { continue };
                            let (r, prem) = self.dsq.simplify_deps(&eq);
                            if r.is_zero() {
                                hit = Some(([a, b, c, d], prem));
                                break 'search;
                            }
                        }
                    }
                }
            }
            let Some(([a, b, c, d], prem)) = hit else { continue };
            let (u, w) = (self.coord(b) - self.coord(a), self.coord(d) - self.coord(c));
            if u.dot(w).abs() > 1e-9 * u.norm() * w.norm() {
                debug_assert!(false, "squared-length perpendicular fails numerically");
                continue;
            }
            let rel = self.raw_dir(a, b).sub(&self.raw_dir(c, d)).sub(&half);
            let fact = self.log.add(
                Reason::Theorem("perpendicular from squared lengths", vec![a, b, c, d]),
                prem,
            );
            changed |= self.angle.force_zero(&rel, Some(fact));
        }
        changed
    }
}
