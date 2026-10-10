use super::*;

const MAX_AREA_POOL: usize = 512;
const MAX_AREA_ROWS: usize = 4096;
const MAX_MULTIPLIERS: usize = 40;

fn area_key(a: PointId, b: PointId, c: PointId) -> LKey {
    let mut t = [a, b, c];
    t.sort();
    LKey::Area(t[0], t[1], t[2])
}

/// `m / t` as a multiset quotient, when `t` divides `m`.
fn divide(m: &[LKey], t: &[LKey]) -> Option<Mono> {
    let mut rest = m.to_vec();
    for k in t {
        let pos = rest.iter().position(|x| x == k)?;
        rest.remove(pos);
    }
    Some(rest)
}

fn times(m: &[LKey], q: &[LKey]) -> Mono {
    let mut out = m.to_vec();
    out.extend_from_slice(q);
    out.sort();
    out
}

#[derive(Clone)]
pub(in crate::ratio) struct BaseRow {
    pub(in crate::ratio) eq: PEq,
    pub(in crate::ratio) text: String,
    pub(in crate::ratio) pending: Option<Vec<Predicate>>,
    /// A named theorem: a proof made of this row alone is a restatement.
    pub(in crate::ratio) headline: bool,
    pub(in crate::ratio) support: bool,
}

impl Figure {
    /// Does `eqangle(p)` hold numerically in every instance (directed, mod π)?
    fn eqangle_in_all(&self, p: [PointId; 8]) -> bool {
        let dir = |k: usize, a: PointId, b: PointId| {
            let d = self.insts[k][b as usize] - self.insts[k][a as usize];
            d.y.atan2(d.x)
        };
        (0..self.insts.len()).all(|k| {
            let x = (dir(k, p[0], p[1]) - dir(k, p[2], p[3])) - (dir(k, p[4], p[5]) - dir(k, p[6], p[7]));
            let r = x.rem_euclid(std::f64::consts::PI);
            r.min(std::f64::consts::PI - r) < 1e-9
        })
    }

    /// The `eqangle` making `sin kx = sin ky`, in the orientation the figure
    /// shows in every instance (a configuration branch), if any.
    pub(in crate::ratio) fn equal_sine_fact(&self, kx: LKey, ky: LKey) -> Option<Predicate> {
        let (LKey::Sin(v, p, q), LKey::Sin(w, r, s)) = (kx, ky) else { return None };
        [[v, p, v, q, w, r, w, s], [v, p, v, q, w, s, w, r]]
            .into_iter()
            .find(|&e| self.eqangle_in_all(e))
            .map(|e| pred("eqangle", &e))
    }

    fn convex_in_all(&self, q: [PointId; 4]) -> bool {
        (0..self.insts.len()).all(|k| {
            let p = |i: usize| self.insts[k][q[i] as usize];
            let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).x * (b - o).y - (a - o).y * (b - o).x;
            let (d1, d2) = (cross(p(0), p(2), p(1)), cross(p(0), p(2), p(3)));
            let (d3, d4) = (cross(p(1), p(3), p(0)), cross(p(1), p(3), p(2)));
            let tol = 1e-9 * (1.0 + (p(2) - p(0)).norm() * (p(3) - p(1)).norm());
            d1 * d2 < -tol * tol && d3 * d4 < -tol * tol && d1.abs() > tol && d2.abs() > tol && d3.abs() > tol && d4.abs() > tol
        })
    }

    /// T6 base rows: `2·[xyz] = xy·xz·sin∠yxz` for every proper triangle and
    /// vertex, and `[abc] + [acd] = [abd] + [bcd]` for every quadrilateral
    /// convex (in that order) in every instance.
    pub(in crate::ratio) fn area_base_rows(&self, pts: &[PointId]) -> Vec<BaseRow> {
        let mut out = Vec::new();
        for (i, &a) in pts.iter().enumerate() {
            for (j, &b) in pts.iter().enumerate().skip(i + 1) {
                for &c in &pts[j + 1..] {
                    if self.sin_atom(a, b, c).is_none() || self.sin_atom(b, c, a).is_none() || self.sin_atom(c, a, b).is_none() {
                        continue;
                    }
                    for (x, y, z) in [(a, b, c), (b, c, a), (c, a, b)] {
                        let mut e = PEq::default();
                        e.add(vec![area_key(a, b, c)], Rat::from_int(2));
                        let mut m: Mono = vec![latom(x, y).into(), latom(x, z).into(), sin_key(x, y, z)];
                        m.sort();
                        e.add(m, -Rat::one());
                        out.push(BaseRow {
                            eq: e,
                            text: format!(
                                "Sine area formula: [{}{}{}] = ½·{}·{}·{}",
                                self.nm(a),
                                self.nm(b),
                                self.nm(c),
                                self.seg(x, y),
                                self.seg(x, z),
                                self.sin_text(sin_key(x, y, z))
                            ),
                            pending: None,
                            headline: false,
                            support: false,
                        });
                    }
                }
            }
        }
        for (i, &a) in pts.iter().enumerate() {
            for (j, &b) in pts.iter().enumerate().skip(i + 1) {
                for (k, &c) in pts.iter().enumerate().skip(j + 1) {
                    for &d in &pts[k + 1..] {
                        for q in [[a, b, c, d], [a, b, d, c], [a, c, b, d]] {
                            if !self.convex_in_all(q) {
                                continue;
                            }
                            let [p0, p1, p2, p3] = q;
                            let mut e = PEq::default();
                            e.add(vec![area_key(p0, p1, p2)], Rat::one());
                            e.add(vec![area_key(p0, p2, p3)], Rat::one());
                            e.add(vec![area_key(p0, p1, p3)], -Rat::one());
                            e.add(vec![area_key(p1, p2, p3)], -Rat::one());
                            let t = |x: PointId, y: PointId, z: PointId| format!("[{}{}{}]", self.nm(x), self.nm(y), self.nm(z));
                            out.push(BaseRow {
                                eq: e,
                                text: format!(
                                    "{}{}{}{} is convex in the figure, so its area splits along either diagonal: \
                                     {} + {} = {} + {}",
                                    self.nm(p0),
                                    self.nm(p1),
                                    self.nm(p2),
                                    self.nm(p3),
                                    t(p0, p1, p2),
                                    t(p0, p2, p3),
                                    t(p0, p1, p3),
                                    t(p1, p2, p3)
                                ),
                                pending: None,
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

    /// Multiply the base rows by the monomials that make one of their terms a
    /// pool monomial, substitute equal sines (pending `eqangle`) and equal
    /// lengths (pending `cong`), to a fixpoint under the caps.
    pub(in crate::ratio) fn area_closure(&mut self, base: &[BaseRow], pool: &mut BTreeSet<Mono>) {
        let mut sine_class: BTreeMap<LKey, Vec<LKey>> = BTreeMap::new();
        let mut cong: BTreeMap<LAtom, Vec<LAtom>> = BTreeMap::new();
        let mut done: BTreeSet<(usize, Mono)> = BTreeSet::new();
        let mut subst: BTreeSet<(Mono, Mono)> = BTreeSet::new();
        let mut intros: BTreeMap<String, usize> = BTreeMap::new();
        let mut queue: Vec<Mono> = pool.iter().cloned().collect();
        let mut rows = 0usize;
        let mut qi = 0;
        while qi < queue.len() {
            let m = queue[qi].clone();
            qi += 1;
            let mut new_rows: Vec<(PEq, String, Option<Vec<Predicate>>, Option<usize>, bool)> = Vec::new();
            for (bi, r) in base.iter().enumerate() {
                for t in r.eq.terms.keys() {
                    let Some(q) = divide(&m, t) else { continue };
                    if !done.insert((bi, q.clone())) {
                        continue;
                    }
                    let mut e = PEq::default();
                    for (mono, c) in &r.eq.terms {
                        e.add(times(mono, &q), c.clone());
                    }
                    let text = if q.is_empty() {
                        format!("{}.", r.text)
                    } else {
                        format!("{}; times {}.", r.text, self.mono_text(&q))
                    };
                    new_rows.push((e, text, r.pending.clone(), r.headline.then_some(bi), r.support));
                }
            }
            let factors: BTreeSet<LKey> = m.iter().copied().collect();
            for f in factors {
                let alts: Vec<(LKey, Predicate, Rat)> = match f {
                    LKey::Sin(..) => {
                        if !sine_class.contains_key(&f) {
                            let c = self.equal_sine_atoms(f);
                            sine_class.insert(f, c);
                        }
                        sine_class[&f]
                            .iter()
                            .filter_map(|&g| self.equal_sine_fact(f, g).map(|p| (g, p, Rat::one())))
                            .collect()
                    }
                    LKey::Len(a, b) => {
                        let s = (a, b);
                        if !cong.contains_key(&s) {
                            let c = self.congruent_candidates(s);
                            cong.insert(s, c);
                        }
                        cong[&s]
                            .iter()
                            .map(|&(c, d)| (LKey::Len(c, d), pred("cong", &[a, b, c, d]), Rat::one()))
                            .collect()
                    }
                    LKey::Cos(..) => self.cos_substitutions(f),
                    LKey::Area(..) => Vec::new(),
                };
                for (g, fact, tau) in alts {
                    let mut m2 = m.clone();
                    let pos = m2.iter().position(|x| *x == f).unwrap();
                    m2[pos] = g;
                    m2.sort();
                    let key = if m <= m2 { (m.clone(), m2.clone()) } else { (m2.clone(), m.clone()) };
                    if !subst.insert(key) {
                        continue;
                    }
                    let mut e = PEq::default();
                    e.add(m.clone(), Rat::one());
                    e.add(m2.clone(), -tau.clone());
                    let minus = if tau.is_negative() { "−" } else { "" };
                    let why = format!("{} = {minus}{}", self.factor_text(f), self.factor_text(g));
                    new_rows.push((
                        e,
                        format!(
                            "{why} (derived from the hypotheses), so {} = {minus}{}.",
                            self.mono_text(&m),
                            self.mono_text(&m2)
                        ),
                        Some(vec![fact]),
                        None,
                        true,
                    ));
                }
            }
            for (e, text, pending, head, support) in new_rows {
                rows += 1;
                if rows > MAX_AREA_ROWS {
                    return;
                }
                for mono in e.terms.keys() {
                    if pool.len() < MAX_AREA_POOL && pool.insert(mono.clone()) {
                        queue.push(mono.clone());
                    }
                }
                let row = match (pending, head) {
                    (None, None) => self.ppush(text, Some(e), vec![]),
                    (facts, head) => {
                        let facts = facts.unwrap_or_default();
                        let key = format!("{facts:?}{head:?}");
                        let intro = match intros.get(&key) {
                            Some(&i) => i,
                            None => {
                                let i = self.ppush(String::new(), None, vec![]);
                                self.pending.push((i, facts, head.is_some()));
                                intros.insert(key, i);
                                i
                            }
                        };
                        self.ppush(text, Some(e), vec![intro])
                    }
                };
                self.psteps[row].support = support;
            }
        }
    }

    /// Sine atoms with the same `|sin|` as `k` in every instance.
    fn equal_sine_atoms(&self, k: LKey) -> Vec<LKey> {
        let LKey::Sin(v, p, q) = k else { return Vec::new() };
        let n = self.names.len() as PointId;
        let mut out = Vec::new();
        for w in 0..n {
            for r in 0..n {
                for s in (r + 1)..n {
                    let Some(g) = self.sin_atom(w, r, s) else { continue };
                    if g == k {
                        continue;
                    }
                    if (0..self.insts.len()).all(|i| {
                        let (x, y) = (self.sin_abs(i, v, p, q), self.sin_abs(i, w, r, s));
                        (x - y).abs() < 1e-9 * x.max(y)
                    }) {
                        out.push(g);
                    }
                }
            }
        }
        out
    }

    /// Stage S7 (T6): multiply the goal by one or two sines that are non-zero
    /// in every instance, close the sine-area and area-additivity rows over
    /// it, bridge with the certified log engine, and solve.
    pub(in crate::ratio) fn prove_by_areas(&self, lhs: &MExpr, rhs: &MExpr, goal_text: &str, pts: &[PointId]) -> Option<String> {
        let goal = self.homogeneous_goal(lhs, rhs)?;
        let gpts: Vec<PointId> = {
            let s: BTreeSet<PointId> = goal.terms.keys().flatten().flat_map(|&k| k.points()).collect();
            s.into_iter().collect()
        };
        let degree = goal.terms.keys().next()?.len();
        if gpts.len() < 4 || degree < 2 {
            return None;
        }
        let mut sines: Vec<LKey> = Vec::new();
        for &v in &gpts {
            for (i, &p) in gpts.iter().enumerate() {
                for &q in &gpts[i + 1..] {
                    if v != p && v != q {
                        if let Some(k) = self.sin_atom(v, p, q) {
                            sines.push(k);
                        }
                    }
                }
            }
        }
        sines.sort();
        sines.dedup();
        let mut multipliers: Vec<Mono> = sines.iter().map(|&k| vec![k]).collect();
        for (i, &a) in sines.iter().enumerate() {
            for &b in &sines[i..] {
                multipliers.push(vec![a, b]);
            }
        }
        multipliers.truncate(MAX_MULTIPLIERS);
        let base_probe = self.area_base_rows(&gpts);
        if !base_probe.iter().any(|r| r.text.contains("convex")) {
            return None;
        }
        for mult in multipliers {
            let mut f = self.lean_clone();
            let mut g = PEq::default();
            for (m, c) in &goal.terms {
                g.add(times(m, &mult), c.clone());
            }
            let mut pool: BTreeSet<Mono> = g.terms.keys().cloned().collect();
            let base = f.area_base_rows(&gpts);
            f.area_closure(&base, &mut pool);
            let goal_pts: BTreeSet<PointId> = gpts.iter().copied().collect();
            f.gather_trig(pts, &goal_pts);
            let goal_monos: BTreeSet<Mono> = g.terms.keys().cloned().collect();
            f.gather_log_bridges(&pool, &goal_monos);
            let Some(used) = f.certified_products(&g) else { continue };
            if used.is_empty() {
                continue;
            }
            let mt = f.mono_text(&mult);
            let pre = format!(
                "Multiply both sides by {mt}, which is not 0: every angle in it is strictly between 0° and 180° \
                 in the figure (non-degenerate triangles)."
            );
            let val = eval_numeric(lhs, &f).unwrap_or(f64::NAN);
            let proof = f.prender(&used, goal_text, val, std::slice::from_ref(&pre));
            return Some(proof.replace(
                "\n  Adding the relations above gives",
                &format!("\n  Adding the relations above and dividing by {mt} gives"),
            ));
        }
        None
    }
}
