use super::Ddar;
use crate::numerics::{direction_of, NumLine, Vec2, ATOM};
use crate::predicate::PointId;
use crate::proof::{FactId, Reason};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Rule {
    BisectorConcurrency,
    MenelausCeva,
    SquaredLengths,
    TriangleEquality,
}

impl Rule {
    const ALL: [Rule; 4] = [
        Rule::BisectorConcurrency,
        Rule::MenelausCeva,
        Rule::SquaredLengths,
        Rule::TriangleEquality,
    ];

    fn key(self) -> &'static str {
        match self {
            Rule::BisectorConcurrency => "bisconc",
            Rule::MenelausCeva => "menelaus",
            Rule::SquaredLengths => "sqlen",
            Rule::TriangleEquality => "trieq",
        }
    }

    fn bit(self) -> u8 {
        1 << (self as u8)
    }

    pub(crate) fn by_name(name: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.key() == name)
    }
}

/// The rules switched off for this process: `DDAR_DISABLE_RULES=sqlen,menelaus,bisconc,trieq`
/// (A/B measurement). All are on by default.
pub(crate) fn env_disabled_mask() -> u8 {
    static OFF: OnceLock<u8> = OnceLock::new();
    *OFF.get_or_init(|| {
        std::env::var("DDAR_DISABLE_RULES")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| Rule::by_name(s.trim()))
            .fold(0, |m, r| m | r.bit())
    })
}

#[derive(Clone, Copy)]
pub(crate) enum Pass {
    BisectorConcurrency,
    MenelausCeva,
    SqFromPerpendiculars,
    SqFromRatios,
    SqFromStewart,
    SqDeriveRatios,
    SqDerivePerps,
    TriangleEquality,
}

impl Pass {
    const COUNT: usize = 8;
}

impl Ddar {
    /// Whether the tables a pass reads changed since it last ran. Each pass is
    /// a function of these inputs (its done-sets only skip work), so an
    /// unchanged input means nothing new to find.
    pub(super) fn inputs_changed(&mut self, pass: Pass) -> bool {
        let (ang, dm, sq) = (self.angle.core.rows(), self.dmul.core.rows(), self.dsq.core.rows());
        let da = self.dadd.core.rows();
        let (lines, active) = (self.lines.len(), self.active.len());
        let key = match pass {
            Pass::BisectorConcurrency => [ang, active, 0],
            Pass::MenelausCeva => [lines, dm, active],
            Pass::SqFromPerpendiculars => [ang, lines, active],
            Pass::SqFromRatios => [dm, active, 0],
            Pass::SqFromStewart => [dm, lines, active],
            Pass::SqDeriveRatios | Pass::SqDerivePerps => [sq, active, 0],
            Pass::TriangleEquality => [da, lines, active],
        };
        let slot = &mut self.classics.seen[pass as usize];
        let changed = *slot != Some(key);
        *slot = Some(key);
        changed
    }

    pub(super) fn rule_on(&self, rule: Rule) -> bool {
        self.rules_off & rule.bit() == 0
    }

    /// Switch one closure rule (`sqlen`, `menelaus`, `bisconc`, `trieq`) on or off for
    /// this engine; `false` for an unknown name.
    pub fn set_rule(&mut self, name: &str, on: bool) -> bool {
        let Some(rule) = Rule::by_name(name) else {
            return false;
        };
        if on {
            self.rules_off &= !rule.bit();
        } else {
            self.rules_off |= rule.bit();
        }
        true
    }
}

#[derive(Clone, Default)]
pub(crate) struct Done {
    seen: [Option<[usize; 3]>; Pass::COUNT],
    pub menelaus: FxHashSet<[PointId; 6]>,
    pub sq_perp: FxHashSet<[PointId; 4]>,
    pub sq_ratio: FxHashSet<[PointId; 4]>,
    pub sq_stewart: FxHashSet<[PointId; 4]>,
}

fn strictly_between(p: Vec2, a: Vec2, b: Vec2) -> bool {
    (a - p).dot(b - p) < 0.0
}

impl Ddar {
    fn nontrivial_lines(&self) -> Vec<usize> {
        self.live_lines
            .iter()
            .copied()
            .filter(|&l| self.lines[l].points.len() >= 3)
            .collect()
    }

    fn line_meet(&self, l1: usize, l2: usize) -> Option<PointId> {
        let p2 = &self.lines[l2].points;
        self.lines[l1]
            .points
            .iter()
            .copied()
            .find(|p| p2.contains(p))
    }

    fn all_distinct(&self, pts: &[PointId]) -> bool {
        (0..pts.len()).all(|i| ((i + 1)..pts.len()).all(|j| !self.num_identical(pts[i], pts[j])))
    }

    /// `|BD|·|CE|·|AF| / (|DC|·|EA|·|FB|)` in log space.
    fn menelaus_product(&self, [a, b, c, d, e, f]: [PointId; 6]) -> crate::elimination::DistMul {
        self.raw_dist_mul(b, d)
            .mul(&self.raw_dist_mul(c, e))
            .mul(&self.raw_dist_mul(a, f))
            .div(&self.raw_dist_mul(d, c))
            .div(&self.raw_dist_mul(e, a))
            .div(&self.raw_dist_mul(f, b))
    }

    fn menelaus_product_cached(&self, [a, b, c, d, e, f]: [PointId; 6]) -> bool {
        let num = &(&self.cached_dist_mul(b, d).0 + &self.cached_dist_mul(c, e).0)
            + &self.cached_dist_mul(a, f).0;
        let den = &(&self.cached_dist_mul(d, c).0 + &self.cached_dist_mul(e, a).0)
            + &self.cached_dist_mul(f, b).0;
        num == den
    }

    fn menelaus_product_numeric(&self, [a, b, c, d, e, f]: [PointId; 6]) -> f64 {
        let dd = |p: PointId, q: PointId| crate::numerics::distance(self.coord(p), self.coord(q));
        (dd(b, d) * dd(c, e) * dd(a, f)) / (dd(d, c) * dd(e, a) * dd(f, b))
    }

    fn numerically_collinear(&self, pts: &[PointId]) -> bool {
        let a0 = pts[0];
        let b = Self::arg_max_first(pts, |p| crate::numerics::distance(self.coord(a0), self.coord(p)));
        let c = Self::arg_max_first(pts, |p| crate::numerics::distance(self.coord(p), self.coord(b)));
        if self.num_identical(b, c) {
            return false;
        }
        let l = NumLine::through(self.coord(b), self.coord(c));
        pts.iter().all(|&p| l.distance(self.coord(p)) < ATOM)
    }

    /// **Menelaus' theorem** and its converse, and the converse of **Ceva's
    /// theorem**, over triangles whose sides are lines of the figure.
    ///
    /// * Menelaus: in a complete quadrilateral of four figure lines (all six
    ///   meets are figure points), each line is a transversal of the triangle
    ///   cut out by the other three: `|BD|·|CE|·|AF| = |DC|·|EA|·|FB|`. Ceva's
    ///   theorem is a product of two Menelaus instances, so it needs no pass.
    /// * Menelaus converse: `D ∈ BC, E ∈ CA, F ∈ AB` with the product `1`
    ///   in the ratio table and an even number of the three inside their
    ///   sides (signed product `-1`) ⇒ `D, E, F` collinear.
    /// * Ceva converse: the same product with an odd number inside (signed
    ///   product `+1`) ⇒ the cevians are concurrent or parallel; when two of
    ///   them already meet at a figure point `P`, the third passes through `P`.
    ///
    /// The ratio identity is symbolic; betweenness only picks the sign branch.
    #[inline(never)]
    pub(super) fn search_menelaus_ceva(&mut self) -> bool {
        let lines = self.nontrivial_lines();
        let k = lines.len();
        if k < 3 {
            return false;
        }
        let mut meet: Vec<Option<PointId>> = vec![None; k * k];
        for i in 0..k {
            for j in (i + 1)..k {
                let m = self.line_meet(lines[i], lines[j]);
                meet[i * k + j] = m;
                meet[j * k + i] = m;
            }
        }
        let mut changed = false;
        for i in 0..k {
            for j in (i + 1)..k {
                let Some(cc) = meet[i * k + j] else { continue };
                for l in (j + 1)..k {
                    let (Some(aa), Some(bb)) = (meet[j * k + l], meet[l * k + i]) else {
                        continue;
                    };
                    if !self.all_distinct(&[aa, bb, cc]) || self.numerically_flat(aa, bb, cc) {
                        continue;
                    }
                    changed |= self.converses_on_triangle([lines[i], lines[j], lines[l]], [aa, bb, cc]);
                    for m in (l + 1)..k {
                        let (Some(d), Some(e), Some(f)) =
                            (meet[i * k + m], meet[j * k + m], meet[l * k + m])
                        else {
                            continue;
                        };
                        changed |=
                            self.menelaus_quadrilateral([lines[i], lines[j], lines[l], lines[m]], [aa, bb, cc, d, e, f]);
                    }
                }
            }
        }
        changed
    }

    /// Lines `ls = [BC, CA, AB, t]`, points `[A, B, C, D=t∩BC, E=t∩CA, F=t∩AB]`:
    /// the four Menelaus relations of the complete quadrilateral.
    fn menelaus_quadrilateral(&mut self, ls: [usize; 4], [a, b, c, d, e, f]: [PointId; 6]) -> bool {
        if !self.all_distinct(&[a, b, c, d, e, f]) {
            return false;
        }
        let instances = [
            [a, b, c, d, e, f],
            [a, f, e, d, c, b],
            [f, b, d, c, e, a],
            [e, d, c, b, a, f],
        ];
        let line_facts: Vec<FactId> = ls.iter().filter_map(|&l| self.lines[l].fact).collect();
        let mut changed = false;
        for inst in instances {
            if self.classics.menelaus.contains(&inst) {
                continue;
            }
            self.classics.menelaus.insert(inst);
            if self.menelaus_product_cached(inst) {
                continue;
            }
            let rel = self.menelaus_product(inst);
            if (self.menelaus_product_numeric(inst) - 1.0).abs() > 1e-9 {
                debug_assert!(false, "Menelaus product fails numerically");
                continue;
            }
            if self.dmul.simplify(&rel).is_one() {
                continue;
            }
            let fact = self.log.add(
                Reason::Theorem("Menelaus' theorem", inst.to_vec()),
                line_facts.clone(),
            );
            changed |= self.dmul.force_one(&rel, Some(fact));
        }
        changed
    }

    /// Triangle `[A, B, C]` with side lines `[BC, CA, AB]`.
    fn converses_on_triangle(&mut self, sides: [usize; 3], [a, b, c]: [PointId; 3]) -> bool {
        let pick = |s: &Ddar, l: usize, x: PointId, y: PointId| -> Vec<PointId> {
            s.lines[l]
                .points
                .iter()
                .copied()
                .filter(|&p| !s.num_identical(p, x) && !s.num_identical(p, y))
                .collect()
        };
        let ds = pick(self, sides[0], b, c);
        let es = pick(self, sides[1], c, a);
        let fs = pick(self, sides[2], a, b);
        let mut changed = false;
        for &d in &ds {
            for &e in &es {
                for &f in &fs {
                    if !self.all_distinct(&[a, b, c, d, e, f]) {
                        continue;
                    }
                    let inside = [
                        strictly_between(self.coord(d), self.coord(b), self.coord(c)),
                        strictly_between(self.coord(e), self.coord(c), self.coord(a)),
                        strictly_between(self.coord(f), self.coord(a), self.coord(b)),
                    ];
                    let n_inside = inside.iter().filter(|&&x| x).count();
                    let inst = [a, b, c, d, e, f];
                    if n_inside % 2 == 0 {
                        if self.numerically_collinear(&[d, e, f]) && !self.check_collinear(&[d, e, f]) {
                            changed |= self.menelaus_converse(sides, inst);
                        }
                    } else {
                        changed |= self.ceva_converse(sides, inst);
                    }
                }
            }
        }
        changed
    }

    fn product_deps_if_one(&self, inst: [PointId; 6]) -> Option<Vec<FactId>> {
        if (self.menelaus_product_numeric(inst) - 1.0).abs() > 1e-9 {
            return None;
        }
        let (r, deps) = self.dmul.simplify_deps(&self.menelaus_product(inst));
        r.is_one().then_some(deps)
    }

    fn menelaus_converse(&mut self, sides: [usize; 3], inst: [PointId; 6]) -> bool {
        let Some(mut prem) = self.product_deps_if_one(inst) else {
            return false;
        };
        prem.extend(sides.iter().filter_map(|&l| self.lines[l].fact));
        let [_, _, _, d, e, f] = inst;
        let fact = self
            .log
            .add(Reason::Theorem("Menelaus' theorem (converse)", inst.to_vec()), prem);
        self.force_collinear(&[d, e, f], vec![fact])
    }

    /// Cevians `AD, BE, CF`; when two of them meet at a figure point `P` and
    /// the third does not yet pass through it symbolically.
    fn ceva_converse(&mut self, sides: [usize; 3], inst: [PointId; 6]) -> bool {
        let [a, b, c, d, e, f] = inst;
        let cevians = [(a, d), (b, e), (c, f)];
        let mut changed = false;
        for missing in 0..3 {
            let (x, y) = cevians[(missing + 1) % 3];
            let (u, v) = cevians[(missing + 2) % 3];
            let (Some(l1), Some(l2)) = (self.pair_line[self.pk(x, y)], self.pair_line[self.pk(u, v)]) else {
                continue;
            };
            let Some(p) = self.lines[l1]
                .points
                .iter()
                .copied()
                .find(|q| self.lines[l2].points.contains(q) && ![x, y, u, v].contains(q))
            else {
                continue;
            };
            let (cv, cf) = cevians[missing];
            if !self.all_distinct(&[p, cv, cf])
                || !self.numerically_collinear(&[cv, p, cf])
                || self.check_collinear(&[cv, p, cf])
            {
                continue;
            }
            let Some(mut prem) = self.product_deps_if_one(inst) else {
                return changed;
            };
            prem.extend(sides.iter().filter_map(|&l| self.lines[l].fact));
            prem.extend(self.lines[l1].fact);
            prem.extend(self.lines[l2].fact);
            let fact = self.log.add(
                Reason::Theorem("Ceva's theorem (converse)", vec![a, b, c, d, e, f, p]),
                prem,
            );
            changed |= self.force_collinear(&[cv, p, cf], vec![fact]);
        }
        changed
    }

    /// **Bisectors concur**: if `AI` bisects angle `BAC` and `BI` bisects angle
    /// `ABC` (each internal or external, `2·dir(AI) ≡ dir(AB) + dir(AC)` mod
    /// π), then `I` is the incentre or an excentre, so `CI` bisects angle
    /// `ACB`. Sound for every internal/external combination, so no branch.
    /// Candidates come from the figure's directions; each is checked exactly.
    #[inline(never)]
    pub(super) fn search_bisector_concurrency(&mut self) -> bool {
        const TOL: f64 = 1e-9;
        let active = self.active.clone();
        let mut found: FxHashMap<(PointId, [PointId; 3]), u8> = FxHashMap::default();
        let mut phi: Vec<(f64, PointId)> = Vec::with_capacity(active.len());
        let mut theta: Vec<(PointId, f64)> = Vec::with_capacity(active.len());
        for &a in &active {
            theta.clear();
            phi.clear();
            for &x in &active {
                if x == a || self.num_identical(x, a) {
                    continue;
                }
                let t = direction_of(self.coord(x) - self.coord(a)).rem_euclid(1.0);
                theta.push((x, t));
                phi.push((t.rem_euclid(0.5), x));
            }
            phi.sort_by(|p, q| p.0.total_cmp(&q.0));
            for bi in 0..theta.len() {
                for ci in (bi + 1)..theta.len() {
                    let ((b, tb), (c, tc)) = (theta[bi], theta[ci]);
                    if self.num_identical(b, c) {
                        continue;
                    }
                    let t = ((tb + tc) / 2.0).rem_euclid(0.5);
                    for target in [t - 0.5, t, t + 0.5] {
                        let lo = phi.partition_point(|e| e.0 < target - TOL);
                        for &(f, i) in &phi[lo..] {
                            if f > target + TOL {
                                break;
                            }
                            if i == b
                                || i == c
                                || self.num_identical(i, b)
                                || self.num_identical(i, c)
                                || self.numerically_flat(a, b, c)
                                || !self.is_bisector(a, b, c, i)
                            {
                                continue;
                            }
                            let mut tri = [a, b, c];
                            tri.sort_unstable();
                            let bit = tri.iter().position(|&p| p == a).unwrap();
                            *found.entry((i, tri)).or_default() |= 1 << bit;
                        }
                    }
                }
            }
        }
        let mut fire: Vec<((PointId, [PointId; 3]), u8)> =
            found.into_iter().filter(|(_, m)| m.count_ones() == 2).collect();
        fire.sort_unstable();
        let mut changed = false;
        for ((i, tri), mask) in fire {
            let v = (0..3).find(|&k| mask & (1 << k) == 0).unwrap();
            let c = tri[v];
            let (p, q) = (tri[(v + 1) % 3], tri[(v + 2) % 3]);
            if self.num_identical(c, i) || self.is_bisector(c, p, q, i) {
                continue;
            }
            let rel = self
                .raw_dir(c, p)
                .add(&self.raw_dir(c, q))
                .sub(&self.raw_dir(c, i))
                .sub(&self.raw_dir(c, i));
            let val = self.angle.value_of(&rel);
            if ((val + 0.5).rem_euclid(1.0) - 0.5).abs() > 1e-9 {
                debug_assert!(false, "bisector concurrency fails numerically");
                continue;
            }
            if self.angle.simplify(&rel).is_zero() {
                continue;
            }
            let mut prem: Vec<FactId> = Vec::new();
            for &u in tri.iter().filter(|&&u| u != c) {
                let o: Vec<PointId> = tri.iter().copied().filter(|&w| w != u).collect();
                prem.extend(self.deps_of_angle_expr(u, o[0], u, i));
                prem.extend(self.deps_of_angle_expr(u, i, u, o[1]));
            }
            let fact = self.log.add(
                Reason::Theorem("angle bisectors concur (incentre/excentre)", vec![i, tri[0], tri[1], tri[2]]),
                prem,
            );
            changed |= self.angle.force_zero(&rel, Some(fact));
        }
        changed
    }

    /// `2·dir(AI) ≡ dir(AB) + dir(AC)` in the cached normal forms.
    fn is_bisector(&self, a: PointId, b: PointId, c: PointId, i: PointId) -> bool {
        let ai = self.cached_dir(a, i);
        self.cached_dir(a, b).add(self.cached_dir(a, c)).sub(ai).sub(ai).is_zero()
    }

    /// **Equality case of the triangle inequality**: `|XM| + |MY| = |XY|` in
    /// the additive length table forces `M` onto segment `XY`, so `X, M, Y`
    /// are collinear. The lengths are unsigned, so no configuration branch is
    /// read; the figure only proposes the triples (numerically flat, not yet
    /// collinear in the line table) and which point is the middle one.
    #[inline(never)]
    pub(super) fn search_triangle_equality(&mut self) -> bool {
        let active = self.active.clone();
        let mut cands: Vec<[PointId; 3]> = Vec::new();
        for (i, &a) in active.iter().enumerate() {
            for (j, &b) in active.iter().enumerate().skip(i + 1) {
                if self.num_identical(a, b) {
                    continue;
                }
                for &c in active.iter().skip(j + 1) {
                    if self.num_identical(a, c)
                        || self.num_identical(b, c)
                        || !self.numerically_flat(a, b, c)
                        || self.check_collinear(&[a, b, c])
                    {
                        continue;
                    }
                    let (pa, pb, pc) = (self.coord(a), self.coord(b), self.coord(c));
                    let tri = if strictly_between(pb, pa, pc) {
                        [a, b, c]
                    } else if strictly_between(pa, pb, pc) {
                        [b, a, c]
                    } else {
                        [a, c, b]
                    };
                    cands.push(tri);
                }
            }
        }
        let mut changed = false;
        for [x, m, y] in cands {
            if self.check_collinear(&[x, m, y]) {
                continue;
            }
            let rel = self
                .raw_dist_add(x, m)
                .add(&self.raw_dist_add(m, y))
                .sub(&self.raw_dist_add(x, y));
            let (r, deps) = self.dadd.simplify_deps(&rel);
            if !r.is_zero() {
                continue;
            }
            let fact = self.log.add(
                Reason::Theorem("equality case of the triangle inequality", vec![x, m, y]),
                deps,
            );
            changed |= self.force_collinear(&[x, m, y], vec![fact]);
        }
        changed
    }
}
