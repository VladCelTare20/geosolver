use super::*;
use crate::certify::pred_c;
use crate::proof::FactId;

mod area;
mod cos;
mod bridge;
mod render;
pub(super) use render::Tag;
#[cfg(test)]
mod tests;

const MAX_POOL: usize = 512;
const MAX_COFACTOR_ROWS: usize = 4096;
const MAX_TRIANGLES: usize = 2000;
const MAX_SIN_CERTS: usize = 256;
const MAX_CLASS_ROWS: usize = 4000;
const MAX_ROUNDS: usize = 64;

/// `(degrees, Σ eₚ·ln p = ln sin)` for the tabulated angles. No 15°, 72°, 75°.
const KNOWN_SINES: &[(i64, &[(i64, i64, i64)])] = &[
    (30, &[(2, -1, 1)]),
    (45, &[(2, -1, 2)]),
    (60, &[(3, 1, 2), (2, -1, 1)]),
    (90, &[]),
    (120, &[(3, 1, 2), (2, -1, 1)]),
    (135, &[(2, -1, 2)]),
    (150, &[(2, -1, 1)]),
];

fn sin_key(v: PointId, p: PointId, q: PointId) -> LKey {
    if p <= q {
        LKey::Sin(v, p, q)
    } else {
        LKey::Sin(v, q, p)
    }
}

impl LKey {
    pub(super) fn points(self) -> Vec<PointId> {
        match self {
            LKey::Len(a, b) => vec![a, b],
            LKey::Sin(v, p, q) | LKey::Area(v, p, q) | LKey::Cos(v, p, q) => vec![v, p, q],
        }
    }
}

impl Figure {
    pub(super) fn sin_abs(&self, i: usize, v: PointId, p: PointId, q: PointId) -> f64 {
        let (u, w) = (
            self.insts[i][p as usize] - self.insts[i][v as usize],
            self.insts[i][q as usize] - self.insts[i][v as usize],
        );
        (u.x * w.y - u.y * w.x).abs() / (u.norm() * w.norm())
    }

    /// The unsigned angle `∠pvq` in `[0, π]`.
    pub(super) fn ang(&self, i: usize, v: PointId, p: PointId, q: PointId) -> f64 {
        let (u, w) = (
            self.insts[i][p as usize] - self.insts[i][v as usize],
            self.insts[i][q as usize] - self.insts[i][v as usize],
        );
        (u.x * w.y - u.y * w.x).abs().atan2(u.dot(w))
    }

    /// `|sin ∠pvq|` as an unknown, only for a triple that is a proper triangle
    /// corner in every instance (same gate as `similar_in_all`).
    pub(super) fn sin_atom(&self, v: PointId, p: PointId, q: PointId) -> Option<LKey> {
        if v == p || v == q || p == q {
            return None;
        }
        let ok = (0..self.insts.len()).all(|i| {
            let (dp, dq) = (self.dist(i, v, p), self.dist(i, v, q));
            let a = self.ang(i, v, p, q);
            dp > 1e-9
                && dq > 1e-9
                && self.dist(i, p, q) > 1e-9
                && a.is_finite()
                && a > 1e-4
                && a < std::f64::consts::PI - 1e-4
                && self.sin_abs(i, v, p, q) > 1e-6
        });
        ok.then(|| sin_key(v, p, q))
    }

    pub(super) fn angle_text(&self, k: LKey) -> String {
        match k {
            LKey::Sin(v, p, q) | LKey::Cos(v, p, q) => format!("∠{}{}{}", self.nm(p), self.nm(v), self.nm(q)),
            LKey::Len(a, b) => self.seg(a, b),
            LKey::Area(a, b, c) => format!("[{}{}{}]", self.nm(a), self.nm(b), self.nm(c)),
        }
    }

    fn sin_text(&self, k: LKey) -> String {
        format!("sin{}", self.angle_text(k))
    }

    fn push_row(
        &mut self,
        text: String,
        eq: LEq,
        alts: Vec<Vec<Predicate>>,
        headline: bool,
        support: bool,
    ) -> usize {
        let i = self.push(text, Some(eq), vec![], headline);
        self.steps[i].alts = alts;
        self.steps[i].support = support;
        i
    }

    /// The trigonometric rows over the triangles on `pts`: law of sines,
    /// equal and known sines, the extended law of sines, and the derived
    /// congruences that tie them together.
    pub(super) fn gather_trig(&mut self, pts: &[PointId], goal_pts: &BTreeSet<PointId>) {
        let mut tris: Vec<[PointId; 3]> = Vec::new();
        for (i, &a) in pts.iter().enumerate() {
            for (j, &b) in pts.iter().enumerate().skip(i + 1) {
                for &c in &pts[j + 1..] {
                    if self.sin_atom(a, b, c).is_some()
                        && self.sin_atom(b, c, a).is_some()
                        && self.sin_atom(c, a, b).is_some()
                    {
                        tris.push([a, b, c]);
                    }
                }
            }
        }
        if tris.len() > MAX_TRIANGLES {
            tris.sort_by_key(|t| std::cmp::Reverse(t.iter().filter(|p| goal_pts.contains(p)).count()));
            tris.truncate(MAX_TRIANGLES);
        }
        let mut atoms: BTreeSet<LKey> = BTreeSet::new();
        for &[a, b, c] in &tris {
            atoms.insert(sin_key(a, b, c));
            atoms.insert(sin_key(b, c, a));
            atoms.insert(sin_key(c, a, b));
        }
        let atoms: Vec<LKey> = atoms.into_iter().collect();
        self.gather_law_of_sines(&tris);
        self.gather_known_sines(&atoms);
        self.gather_equal_sines(&atoms);
        self.gather_extended_sines();
        self.gather_log_congruences(pts);
    }

    /// Two rows per triangle `abc`: `ln bc − ln sin a = ln ca − ln sin b` and
    /// `ln ca − ln sin b = ln ab − ln sin c`. Unconditional on a proper
    /// triangle, so no premises; headline, so it never stands alone.
    fn gather_law_of_sines(&mut self, tris: &[[PointId; 3]]) {
        for &[a, b, c] in tris {
            for (x, y, z) in [(a, b, c), (b, c, a)] {
                let (sx, sy) = (sin_key(x, y, z), sin_key(y, z, x));
                let mut e = LEq::default();
                e.add_term(latom(y, z), Rat::one());
                e.add_term(sx, -Rat::one());
                e.add_term(latom(z, x), -Rat::one());
                e.add_term(sy, Rat::one());
                let text = format!(
                    "Law of sines in △{}{}{}: {}/{} = {}/{}.",
                    self.nm(a),
                    self.nm(b),
                    self.nm(c),
                    self.seg(y, z),
                    self.sin_text(sx),
                    self.seg(z, x),
                    self.sin_text(sy)
                );
                let r = self.push_row(text, e, vec![], true, false);
                self.steps[r].tag = Some(Tag::Los([a, b, c], (x, y, z)));
            }
        }
    }

    /// `ln sin θ = ln(tabulated value)` for an angle equal to a tabulated
    /// constant in every instance, pending the `aconst` (or `perp`) fact.
    fn gather_known_sines(&mut self, atoms: &[LKey]) {
        for &k in atoms {
            let LKey::Sin(v, p, q) = k else { continue };
            for &(deg, primes) in KNOWN_SINES {
                let target = (deg as f64).to_radians();
                if !(0..self.insts.len()).all(|i| (self.ang(i, v, p, q) - target).abs() < 1e-9) {
                    continue;
                }
                let mut e = LEq::default();
                e.add_term(k, Rat::one());
                for &(prime, num, den) in primes {
                    e.add_prime(prime, -Rat::new(num, den));
                }
                let alts = if deg == 90 {
                    vec![vec![pred("perp", &[v, p, v, q])]]
                } else {
                    vec![
                        vec![pred_c("aconst", &[v, p, v, q], vec![Rat::from_int(deg)])],
                        vec![pred_c("aconst", &[v, p, v, q], vec![Rat::from_int(180 - deg)])],
                    ]
                };
                let value = match deg {
                    30 | 150 => "1/2",
                    45 | 135 => "√2/2",
                    60 | 120 => "√3/2",
                    _ => "1",
                };
                let text = format!(
                    "{} = {deg}° (derived from the hypotheses), so {} = {value}.",
                    self.angle_text(k),
                    self.sin_text(k)
                );
                self.push_row(text, e, alts, false, true);
            }
        }
    }

    /// Classes of atoms with equal `|sin|` in every instance; each row
    /// `ln sin θ = ln sin φ` waits for the directed-angle fact `θ ≡ ±φ`
    /// (either orientation). Right angles are left to `gather_known_sines`.
    fn gather_equal_sines(&mut self, atoms: &[LKey]) {
        let n = self.insts.len();
        let vals: Vec<(LKey, Vec<f64>)> = atoms
            .iter()
            .filter_map(|&k| {
                let LKey::Sin(v, p, q) = k else { return None };
                let s: Vec<f64> = (0..n).map(|i| self.sin_abs(i, v, p, q)).collect();
                (!s.iter().all(|x| (x - 1.0).abs() < 1e-9)).then_some((k, s))
            })
            .collect();
        let same = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9 * x.max(*y));
        let mut sorted: Vec<usize> = (0..vals.len()).collect();
        sorted.sort_by(|&a, &b| vals[a].1[0].total_cmp(&vals[b].1[0]));
        let mut classes: Vec<Vec<usize>> = Vec::new();
        let mut start = 0;
        while start < sorted.len() {
            let mut end = start + 1;
            while end < sorted.len()
                && (vals[sorted[end]].1[0] - vals[sorted[end - 1]].1[0]).abs()
                    < 1e-9 * vals[sorted[end]].1[0]
            {
                end += 1;
            }
            let mut group: Vec<Vec<usize>> = Vec::new();
            for &i in &sorted[start..end] {
                match group.iter_mut().find(|c| same(&vals[c[0]].1, &vals[i].1)) {
                    Some(c) => c.push(i),
                    None => group.push(vec![i]),
                }
            }
            classes.extend(group.into_iter().filter(|c| c.len() > 1));
            start = end;
        }
        let mut rows = 0;
        for class in classes {
            let mut class = class;
            class.sort_by_key(|&i| vals[i].0);
            let mut pairs: Vec<(usize, usize)> = Vec::new();
            if class.len() <= 6 {
                for i in 0..class.len() {
                    for j in (i + 1)..class.len() {
                        pairs.push((class[i], class[j]));
                    }
                }
            } else {
                for j in 1..class.len() {
                    pairs.push((class[0], class[j]));
                    if j >= 2 {
                        pairs.push((class[j - 1], class[j]));
                    }
                }
            }
            for (x, y) in pairs {
                if rows >= MAX_CLASS_ROWS {
                    return;
                }
                rows += 1;
                let (kx, ky) = (vals[x].0, vals[y].0);
                self.push_equal_sines(kx, ky);
            }
        }
    }

    fn push_equal_sines(&mut self, kx: LKey, ky: LKey) {
        let (LKey::Sin(v, p, q), LKey::Sin(w, r, s)) = (kx, ky) else { return };
        let mut e = LEq::default();
        e.add_term(kx, Rat::one());
        e.add_term(ky, -Rat::one());
        let n = self.insts.len();
        let equal = (0..n).all(|i| (self.ang(i, v, p, q) - self.ang(i, w, r, s)).abs() < 1e-9);
        let supp = (0..n).all(|i| {
            (self.ang(i, v, p, q) + self.ang(i, w, r, s) - std::f64::consts::PI).abs() < 1e-9
        });
        let (ax, ay) = (self.angle_text(kx), self.angle_text(ky));
        let rel = if equal {
            format!("{ax} = {ay}")
        } else if supp {
            format!("{ax} + {ay} = 180°")
        } else {
            format!("{ax} = ±{ay} (mod 180°)")
        };
        let text = format!(
            "{rel} (derived from the hypotheses), so {} = {}.",
            self.sin_text(kx),
            self.sin_text(ky)
        );
        let alts = vec![
            vec![pred("eqangle", &[v, p, v, q, w, r, w, s])],
            vec![pred("eqangle", &[v, p, v, q, w, s, w, r])],
        ];
        let i = self.push_row(text, e, alts, false, true);
        self.steps[i].tag = Some(Tag::EqSin(kx, ky, rel));
    }

    /// `XY = 2·OX·sin∠XZY` for a chord `XY` seen from `Z` on a circle with
    /// centre `O`, pending the two equal radii.
    fn gather_extended_sines(&mut self) {
        for (o, set) in self.circles.clone() {
            let on: Vec<PointId> = set.iter().copied().collect();
            for (i, &x) in on.iter().enumerate() {
                for &y in &on[i + 1..] {
                    for &z in &on {
                        if z == x || z == y {
                            continue;
                        }
                        let Some(k) = self.sin_atom(z, x, y) else { continue };
                        let mut e = LEq::default();
                        e.add_term(latom(x, y), Rat::one());
                        e.add_prime(2, -Rat::one());
                        e.add_term(latom(o, x), -Rat::one());
                        e.add_term(k, -Rat::one());
                        let text = format!(
                            "Extended law of sines in circle ({}) through {}, {}, {}: {} = 2·{}·{}.",
                            self.nm(o),
                            self.nm(x),
                            self.nm(y),
                            self.nm(z),
                            self.seg(x, y),
                            self.seg(o, x),
                            self.sin_text(k)
                        );
                        let alts = vec![vec![pred("cong", &[o, x, o, y]), pred("cong", &[o, x, o, z])]];
                        self.push_row(text, e, alts, true, false);
                    }
                }
            }
        }
    }

    /// `ln s₁ = ln s₂` for segments equal in every instance, pending `cong`.
    fn gather_log_congruences(&mut self, pts: &[PointId]) {
        let mut segs: Vec<LAtom> = Vec::new();
        for (i, &a) in pts.iter().enumerate() {
            for &b in &pts[i + 1..] {
                if (0..self.insts.len()).all(|k| self.dist(k, a, b) > 1e-9) {
                    segs.push(latom(a, b));
                }
            }
        }
        segs.sort();
        let mut classes: Vec<Vec<LAtom>> = Vec::new();
        for s in segs {
            match classes.iter_mut().find(|c| self.equal_len_in_all(c[0], s)) {
                Some(c) => c.push(s),
                None => classes.push(vec![s]),
            }
        }
        let mut rows = 0;
        for class in classes.into_iter().filter(|c| c.len() > 1) {
            let mut pairs = Vec::new();
            if class.len() <= 6 {
                for i in 0..class.len() {
                    for j in (i + 1)..class.len() {
                        pairs.push((class[i], class[j]));
                    }
                }
            } else {
                for j in 1..class.len() {
                    pairs.push((class[0], class[j]));
                    if j >= 2 {
                        pairs.push((class[j - 1], class[j]));
                    }
                }
            }
            for (s1, s2) in pairs {
                if rows >= MAX_CLASS_ROWS {
                    return;
                }
                rows += 1;
                let mut e = LEq::default();
                e.add_term(s1, Rat::one());
                e.add_term(s2, -Rat::one());
                let text = format!(
                    "{} = {} (derived from the hypotheses).",
                    self.seg(s1.0, s1.1),
                    self.seg(s2.0, s2.1)
                );
                let alts = vec![vec![pred("cong", &[s1.0, s1.1, s2.0, s2.1])]];
                self.push_row(text, e, alts, false, true);
            }
        }
    }

    /// Drop, repeatedly, every row holding an atom that no other remaining row
    /// and not the goal mentions: such a row has coefficient 0 in any
    /// combination proving the goal, so this is exact.
    pub(super) fn prune_private_atoms(&self, goal: &LEq, allowed: &BTreeSet<usize>) -> BTreeSet<usize> {
        let mut keep: BTreeSet<usize> = allowed
            .iter()
            .copied()
            .filter(|&i| self.steps[i].eq.is_some())
            .collect();
        let mut count: BTreeMap<LKey, usize> = BTreeMap::new();
        for &i in &keep {
            for k in self.steps[i].eq.as_ref().unwrap().terms.keys() {
                *count.entry(*k).or_insert(0) += 1;
            }
        }
        loop {
            let drop: Vec<usize> = keep
                .iter()
                .copied()
                .filter(|&i| {
                    self.steps[i]
                        .eq
                        .as_ref()
                        .unwrap()
                        .terms
                        .keys()
                        .any(|k| count[k] == 1 && !goal.terms.contains_key(k))
                })
                .collect();
            if drop.is_empty() {
                return keep;
            }
            for i in drop {
                keep.remove(&i);
                for k in self.steps[i].eq.as_ref().unwrap().terms.keys() {
                    *count.get_mut(k).unwrap() -= 1;
                }
            }
        }
    }

    /// DDAR certification of one pending row, cached per figure. `None` once
    /// the per-solve budget of checks is spent.
    fn certify_row(&mut self, i: usize) -> Option<Option<Vec<FactId>>> {
        if let Some(r) = self.trig_cache.get(&i) {
            return Some(r.clone());
        }
        let mut got = None;
        for alt in self.steps[i].alts.clone() {
            self.trig_checks += 1;
            if self.trig_checks > MAX_SIN_CERTS {
                return None;
            }
            got = self.ddar.borrow_mut().derive_deps(&self.names, &self.insts[0], &self.preds, self.hyps, &alt);
            if got.is_some() {
                break;
            }
        }
        self.trig_cache.insert(i, got.clone());
        Some(got)
    }

    /// `prove` with certification: every used row that carries premise
    /// alternatives must have one of them derived by DDAR, or it is rejected
    /// and the solve retried. Then greedy minimisation and the lone-headline
    /// guard. Returns the rows and the closure facts they rest on.
    pub(super) fn certified_prove_core(
        &mut self,
        goal: &LEq,
        base: &BTreeSet<usize>,
    ) -> Option<(BTreeSet<usize>, Vec<FactId>)> {
        let mut rejected: BTreeSet<usize> = BTreeSet::new();
        for _ in 0..MAX_ROUNDS {
            let allowed: BTreeSet<usize> = base
                .iter()
                .copied()
                .filter(|i| !rejected.contains(i) && !matches!(self.trig_cache.get(i), Some(None)))
                .collect();
            let allowed = self.prune_private_atoms(goal, &allowed);
            let mut used = self.prove_with(goal, &allowed)?;
            let mut all_ok = true;
            for &i in &used {
                if self.steps[i].alts.is_empty() {
                    continue;
                }
                if self.certify_row(i)?.is_none() {
                    rejected.insert(i);
                    all_ok = false;
                }
            }
            if !all_ok {
                continue;
            }
            loop {
                let mut removed = false;
                for &s in used.clone().iter() {
                    let mut trial = used.clone();
                    trial.remove(&s);
                    if let Some(smaller) = self.prove_with(goal, &trial) {
                        used = smaller;
                        removed = true;
                        break;
                    }
                }
                if !removed {
                    break;
                }
            }
            if used.is_empty() {
                return None;
            }
            let heads: Vec<usize> = used.iter().copied().filter(|&i| self.steps[i].headline).collect();
            if heads.len() == 1 && used.iter().all(|&i| i == heads[0] || self.steps[i].support) {
                rejected.insert(heads[0]);
                continue;
            }
            let mut deps: Vec<FactId> = used
                .iter()
                .filter_map(|i| self.trig_cache.get(i).cloned().flatten())
                .flatten()
                .collect();
            deps.sort_unstable();
            deps.dedup();
            return Some((used, deps));
        }
        None
    }

    /// [`Figure::certified_prove_core`], with the certified facts printed once
    /// in a lead step that every certified row cites.
    pub(super) fn certified_prove(&mut self, goal: &LEq, base: &BTreeSet<usize>) -> Option<BTreeSet<usize>> {
        let (used, deps) = self.certified_prove_core(goal, base)?;
        if !deps.is_empty() {
            let lines = self.ddar.borrow().lines(&deps);
            let lead = self.push(
                format!(
                    "Facts derived from the hypotheses by the deductive closure:{}",
                    derivation_block(&lines)
                ),
                None,
                vec![],
                false,
            );
            self.steps[lead].lead = true;
            for &i in &used {
                if !self.steps[i].alts.is_empty() {
                    self.steps[i].premises.push(lead);
                }
            }
        }
        Some(used)
    }

    /// Stage S2: the monomial goal over the S1 figure plus the trig rows.
    pub(super) fn prove_trig_log(
        &mut self,
        goal: &LEq,
        pts: &[PointId],
        goal_pts: &BTreeSet<PointId>,
    ) -> Option<BTreeSet<usize>> {
        let first = self.steps.len();
        self.gather_trig(pts, goal_pts);
        let base: BTreeSet<usize> = (0..self.steps.len())
            .filter(|&i| self.steps[i].eq.is_some() && (i >= first || self.steps[i].alts.is_empty()))
            .collect();
        self.certified_prove(goal, &base)
    }

    /// T2: for every
    pub(super) fn factor_val(&self, inst: usize, k: LKey) -> f64 {
        match k {
            LKey::Len(a, b) => self.dist(inst, a, b),
            LKey::Sin(v, p, q) => self.sin_abs(inst, v, p, q),
            LKey::Area(a, b, c) => {
                let (u, w) = (
                    self.insts[inst][b as usize] - self.insts[inst][a as usize],
                    self.insts[inst][c as usize] - self.insts[inst][a as usize],
                );
                0.5 * (u.x * w.y - u.y * w.x).abs()
            }
            LKey::Cos(v, p, q) => self.cos_val(inst, v, p, q),
        }
    }

    pub(super) fn cos_val(&self, inst: usize, v: PointId, p: PointId, q: PointId) -> f64 {
        let (u, w) = (
            self.insts[inst][p as usize] - self.insts[inst][v as usize],
            self.insts[inst][q as usize] - self.insts[inst][v as usize],
        );
        u.dot(w) / (u.norm() * w.norm())
    }

    pub(super) fn factor_text(&self, k: LKey) -> String {
        match k {
            LKey::Len(a, b) => self.seg(a, b),
            LKey::Sin(..) => self.sin_text(k),
            LKey::Area(a, b, c) => format!("[{}{}{}]", self.nm(a), self.nm(b), self.nm(c)),
            LKey::Cos(..) => format!("cos{}", self.angle_text(k)),
        }
    }

    pub(super) fn mono_text(&self, m: &[LKey]) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut i = 0;
        while i < m.len() {
            let mut j = i;
            while j < m.len() && m[j] == m[i] {
                j += 1;
            }
            let s = self.factor_text(m[i]);
            parts.push(match j - i {
                1 => s,
                2 => format!("{s}²"),
                3 => format!("{s}³"),
                k => format!("{s}^{k}"),
            });
            i = j;
        }
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join("·")
        }
    }

    pub(super) fn equal_len_in_all(&self, s1: LAtom, s2: LAtom) -> bool {
        (0..self.insts.len()).all(|i| {
            let (a, b) = (self.dist(i, s1.0, s1.1), self.dist(i, s2.0, s2.1));
            a > 1e-9 && b > 1e-9 && (a - b).abs() < 1e-9 * a.max(b).max(1.0)
        })
    }

    /// Segments of the figure equal to `s` in every instance (candidates only).
    pub(super) fn congruent_candidates(&self, s: LAtom) -> Vec<LAtom> {
        let n = self.names.len() as PointId;
        let mut out = Vec::new();
        for a in 0..n {
            for b in (a + 1)..n {
                if (a, b) != s && self.equal_len_in_all(s, (a, b)) {
                    out.push((a, b));
                }
            }
        }
        out
    }

    fn cong_intro(&mut self, intros: &mut BTreeMap<(LAtom, LAtom), usize>, s1: LAtom, s2: LAtom) -> usize {
        let key = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
        if let Some(&i) = intros.get(&key) {
            return i;
        }
        let intro = self.ppush(
            format!(
                "{} = {} (derived from the hypotheses).",
                self.seg(key.0 .0, key.0 .1),
                self.seg(key.1 .0, key.1 .1)
            ),
            None,
            vec![],
        );
        self.pending
            .push((intro, vec![pred("cong", &[key.0 .0, key.0 .1, key.1 .0, key.1 .1])], false));
        intros.insert(key, intro);
        intro
    }

    /// T5: second meets of a line `v r` (`v` a goal point, `r` on the circle)
    /// with a circle known by its centre and exactly three points — the
    /// circumcircle `{A,B,C}` with centre `O` never reaches `concyclic`.
    pub(super) fn centred_second_meets(&self, g: &[PointId]) -> Vec<AuxCand> {
        let kid = self.names.len() as PointId;
        let mut out = Vec::new();
        for (o, set) in &self.circles {
            if set.len() != 3 || self.concyclic.iter().any(|c| set.is_subset(c)) {
                continue;
            }
            let o = *o;
            let on: Vec<PointId> = set.iter().copied().collect();
            let a = on[0];
            for &v in g {
                for &r in &on {
                    if v == r {
                        continue;
                    }
                    let cand = self.build_cand(
                        "K".to_string(),
                        format!(
                            "Let K be the second meet of line {} with the circle ({}){}.",
                            self.seg(v, r),
                            self.nm(o),
                            if on.iter().any(|&p| self.nm(p).starts_with('_')) {
                                String::new()
                            } else {
                                format!(" through {}", on.iter().map(|&p| self.nm(p)).collect::<String>())
                            }
                        ),
                        vec![AuxFact::Coll(v, r, kid), AuxFact::Cong([o, a, o, kid])],
                        move |i| {
                            let rad = (i[a as usize] - i[o as usize]).norm();
                            second_meet(i[v as usize], i[r as usize], i[o as usize], rad, i[r as usize])
                        },
                    );
                    out.extend(cand);
                }
            }
        }
        out
    }

    /// T2: for every certified-candidate congruence `s1 = s2` and every pool
    /// monomial `m` containing `s1`, the row `m − m[s1→s2] = 0`; the new
    /// monomial joins the pool. Runs to a fixpoint under the pool/row caps.
    pub(super) fn gather_cofactor_substitutions(&mut self, pool: &mut BTreeSet<Mono>) {
        let mut queue: Vec<Mono> = pool.iter().cloned().collect();
        let mut equal: BTreeMap<LAtom, Vec<LAtom>> = BTreeMap::new();
        let mut intros: BTreeMap<(LAtom, LAtom), usize> = BTreeMap::new();
        let mut rows: BTreeSet<(Mono, Mono)> = BTreeSet::new();
        let mut qi = 0;
        while qi < queue.len() {
            let m = queue[qi].clone();
            qi += 1;
            let atoms: BTreeSet<LAtom> = m.iter().filter_map(|k| k.len()).collect();
            for s in atoms {
                if !equal.contains_key(&s) {
                    let c = self.congruent_candidates(s);
                    equal.insert(s, c);
                }
                for s2 in equal[&s].clone() {
                    let mut m2 = m.clone();
                    let pos = m2.iter().position(|x| *x == LKey::from(s)).unwrap();
                    m2[pos] = s2.into();
                    m2.sort();
                    let key = if m <= m2 { (m.clone(), m2.clone()) } else { (m2.clone(), m.clone()) };
                    if !rows.insert(key) {
                        continue;
                    }
                    if rows.len() > MAX_COFACTOR_ROWS {
                        return;
                    }
                    let intro = self.cong_intro(&mut intros, s, s2);
                    let mut e = PEq::default();
                    e.add(m.clone(), Rat::one());
                    e.add(m2.clone(), -Rat::one());
                    let text = format!(
                        "{} = {}, so {} = {}.",
                        self.seg(s.0, s.1),
                        self.seg(s2.0, s2.1),
                        self.mono_text(&m),
                        self.mono_text(&m2)
                    );
                    let row = self.ppush(text, Some(e), vec![intro]);
                    self.psteps[row].support = true;
                    if pool.len() < MAX_POOL && pool.insert(m2.clone()) {
                        queue.push(m2);
                    }
                }
            }
        }
    }
}
