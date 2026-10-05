use super::*;
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::VarId;
use std::sync::OnceLock;

const MAX_LOS_ROWS: usize = 2000;
const MAX_EQ_ROWS: usize = 4000;
const MAX_TRIANGLES: usize = 4000;

/// How the DDAR closure uses the law of sines (`GEO_TRIG`, or `ddar --trig`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrigMode {
    /// Never: the closure is exactly the trig-free one.
    Off,
    /// Only when a closed figure leaves an eligible goal unproved
    /// ([`crate::runner`]); the aux search stays trig-free. The default.
    Fallback,
    /// In every closure, once its trig-free fixpoint is reached (base and aux).
    Lazy,
    /// In every closure from the first round.
    Always,
}

pub fn mode() -> TrigMode {
    static MODE: OnceLock<TrigMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("GEO_TRIG").as_deref() {
        Ok("off") => TrigMode::Off,
        Ok("lazy") => TrigMode::Lazy,
        Ok("always") => TrigMode::Always,
        _ => TrigMode::Fallback,
    })
}

/// Goal predicates the fallback may help with (length relations).
pub fn eligible_goal(name: &str) -> bool {
    matches!(name, "cong" | "eqratio" | "rconst" | "distmeq")
}

#[derive(Clone, Default)]
pub(crate) struct TrigState {
    enabled: bool,
    disabled: bool,
    svar: FxHashMap<(VarId, VarId), DistMul>,
    los_done: FxHashSet<((VarId, VarId), (VarId, VarId))>,
    eq_done: FxHashSet<((VarId, VarId), (VarId, VarId))>,
    known_done: FxHashSet<(VarId, VarId)>,
    /// `|sin|` of an angle class (a reduced angle up to sign), whether or not
    /// a corner of the figure has it.
    vsvar: FxHashMap<Class, DistMul>,
    vref: FxHashMap<Class, ClassRef>,
    vlink_done: FxHashSet<(Class, (VarId, VarId))>,
    mult_done: FxHashSet<(Class, u8)>,
    conv_done: FxHashSet<((VarId, VarId), (VarId, VarId), Class)>,
    rows: usize,
    eq_rows: usize,
    pub(crate) rejected: usize,
    pub(crate) admitted: usize,
    pub(crate) candidates: usize,
}

/// An angle class: the reduced directed angle up to sign, as its terms.
type Class = Vec<(VarId, Rat)>;

const MAX_MULT_ROWS: usize = 400;
const MAX_CONVERSE: usize = 200;

/// One corner of a candidate triangle: vertex and the two other points.
#[derive(Clone, Copy)]
struct Corner {
    v: PointId,
    p: PointId,
    q: PointId,
    key: (VarId, VarId),
}

fn dir_var(a: &Angle) -> Option<VarId> {
    (a.0.terms.len() == 1).then(|| a.0.terms[0].0)
}

/// `(−1)^neg · ((−1)^flip · ∠corner + shift·π)`, the angle a class stands for.
#[derive(Clone)]
struct ClassRef {
    c: Corner,
    flip: bool,
    shift: Rat,
    neg: bool,
}

impl ClassRef {
    fn negated(mut self, by: bool) -> ClassRef {
        if by {
            self.flip = !self.flip;
            self.shift = (-&self.shift).mod_one();
        }
        self
    }
}

#[derive(Default)]
struct Spell {
    pts: Vec<PointId>,
}

impl Spell {
    fn p(&mut self, x: PointId) -> String {
        self.pts.push(x);
        format!("{{{}}}", self.pts.len() - 1)
    }

    fn corner(&mut self, c: &Corner, flip: bool) -> String {
        let (a, b) = if flip { (c.p, c.q) } else { (c.q, c.p) };
        format!("∠({}{},{}{})", self.p(c.v), self.p(a), self.p(c.v), self.p(b))
    }

    fn class(&mut self, r: &ClassRef) -> String {
        let a = self.corner(&r.c, r.flip);
        let deg = &r.shift.mod_one() * &Rat::from_int(180);
        if deg.is_zero() {
            a
        } else {
            format!("{a} + {deg}°")
        }
    }

    fn side(&mut self, a: PointId, b: PointId) -> String {
        format!("{}{}", self.p(a), self.p(b))
    }

    fn reason(self, name: &'static str, text: String) -> Reason {
        Reason::Formula(name, text, self.pts)
    }
}

impl Ddar {
    /// Rows every table refused because the figure contradicts them.
    pub fn rejected_rows(&self) -> usize {
        self.angle.core.rejected + self.dmul.core.rejected + self.dadd.core.rejected + self.dsq.core.rejected + self.trig.rejected
    }

    pub fn trig_stats(&self) -> (usize, usize, usize, usize) {
        let t = &self.trig;
        (t.candidates, t.admitted, t.rows, t.rejected)
    }

    /// Turn the trig rows on for this figure (fallback activation).
    pub fn enable_trig(&mut self) {
        self.trig.enabled = true;
    }

    /// Called at a trig-free fixpoint of the closure: in lazy mode switch the
    /// trig rows on once and report that the closure must continue.
    pub(super) fn trig_activate_at_fixpoint(&mut self) -> bool {
        if mode() == TrigMode::Lazy && !self.trig.enabled && !self.trig.disabled {
            self.trig.enabled = true;
            return true;
        }
        false
    }

    fn corner(&self, v: PointId, p: PointId, q: PointId) -> Option<Corner> {
        let (dp, dq) = (dir_var(&self.raw_dir(v, p))?, dir_var(&self.raw_dir(v, q))?);
        let key = if dp <= dq { (dp, dq) } else { (dq, dp) };
        Some(Corner { v, p, q, key })
    }

    /// `dir(vp) − dir(vq)` reduced, and the facts the reduction used.
    fn corner_angle(&self, c: &Corner) -> (Angle, Vec<FactId>) {
        let a = self.raw_dir(c.v, c.p).sub(&self.raw_dir(c.v, c.q));
        self.angle.simplify_deps(&a)
    }

    /// The class of a corner up to sign (equal or supplementary angles have
    /// equal `|sin|`).
    fn corner_class(angle: &Angle) -> Vec<(VarId, Rat)> {
        let a = angle.0.terms.to_vec();
        let b = angle.neg().0.terms.to_vec();
        a.min(b)
    }

    fn sin_value(&self, c: &Corner) -> f64 {
        let (o, p, q) = (self.coord(c.v), self.coord(c.p), self.coord(c.q));
        let (u, w) = (p - o, q - o);
        (u.x * w.y - u.y * w.x).abs() / (u.norm() * w.norm())
    }

    fn sin_var(&mut self, c: &Corner) -> DistMul {
        if let Some(s) = self.trig.svar.get(&c.key) {
            return s.clone();
        }
        let value = self.sin_value(c);
        let v = self.dmul.core.new_var_ranked(value, true, 0);
        let s = DistMul(LinComb::singleton(v, Rat::one()));
        self.trig.svar.insert(c.key, s.clone());
        s
    }

    /// Add `row = 1` unless it fails numerically (release-mode guard).
    fn trig_force(&mut self, row: &DistMul, reason: Reason, prem: Vec<FactId>) -> bool {
        let terms = row.0.len().max(1) as f64;
        if (self.dmul.value_of(row) - 1.0).abs() > 1e-9 * terms {
            self.trig.rejected += 1;
            debug_assert!(false, "trig row fails numerically: {reason:?}");
            return false;
        }
        let fact = self.log.add(reason, prem);
        self.dmul.force_one(row, Some(fact))
    }

    /// `|sin|` of a known angle `k·π` as an exact prime combination.
    fn known_sine(&mut self, k: &Rat) -> Option<(DistMul, &'static str, &'static str)> {
        let two = self.dmul.frac_value(&Rat::from_int(2));
        let three = self.dmul.frac_value(&Rat::from_int(3));
        let scaled = |d: &DistMul, f: Rat| {
            let mut c = d.0.clone();
            c.mul_assign_scalar(&f);
            DistMul(c)
        };
        let k6 = k * &Rat::from_int(12);
        Some(match k6.numer_i64()? {
            _ if !k6.is_integer() => return None,
            2 | 10 => (scaled(&two, Rat::from_int(-1)), "sine of 30°", "1/2"),
            3 | 9 => (scaled(&two, Rat::new(-1, 2)), "sine of 45°", "√2/2"),
            4 | 8 => (scaled(&three, Rat::new(1, 2)).mul(&scaled(&two, Rat::from_int(-1))), "sine of 60°", "√3/2"),
            6 => (DistMul(LinComb::zero()), "sine of 90°", "1"),
            _ => return None,
        })
    }

    /// One round of the trigonometric rows: law of sines between pairs of
    /// shared angle classes of the candidate triangles (a k-core prune),
    /// equal sines along each class, and the known sines.
    pub(super) fn search_trig(&mut self) -> bool {
        if self.trig.disabled {
            return false;
        }
        if mode() == TrigMode::Always {
            self.trig.enabled = true;
        }
        if !self.trig.enabled {
            return false;
        }
        let active = self.active.clone();
        let mut tris: Vec<[Corner; 3]> = Vec::new();
        'outer: for (i, &a) in active.iter().enumerate() {
            for (j, &b) in active.iter().enumerate().skip(i + 1) {
                if self.num_identical(a, b) {
                    continue;
                }
                for &c in &active[j + 1..] {
                    if self.num_identical(a, c) || self.num_identical(b, c) {
                        continue;
                    }
                    if orientation(self.coord(a), self.coord(b), self.coord(c)) == 0 || self.numerically_flat(a, b, c) {
                        continue;
                    }
                    let (Some(x), Some(y), Some(z)) = (self.corner(a, b, c), self.corner(b, c, a), self.corner(c, a, b)) else {
                        continue;
                    };
                    tris.push([x, y, z]);
                    if tris.len() >= MAX_TRIANGLES {
                        break 'outer;
                    }
                }
            }
        }
        self.trig.candidates = tris.len();
        let mut classes: Vec<[Vec<(VarId, Rat)>; 3]> = Vec::with_capacity(tris.len());
        let mut angles: FxHashMap<(VarId, VarId), (Angle, Vec<FactId>)> = FxHashMap::default();
        for t in &tris {
            let mut cl: [Vec<(VarId, Rat)>; 3] = Default::default();
            for (k, c) in t.iter().enumerate() {
                let (ang, deps) = self.corner_angle(c);
                if ang.is_zero() {
                    self.trig.disabled = true;
                    return false;
                }
                cl[k] = Self::corner_class(&ang);
                angles.insert(c.key, (ang, deps));
            }
            classes.push(cl);
        }
        let mut changed = self.trig_multiple_angles(&tris, &classes);
        let virt: FxHashSet<Class> = self.trig.vsvar.keys().cloned().collect();
        let is_const = |c: &[(VarId, Rat)]| c.iter().all(|(v, _)| *v == ANGLE_UNIT) || virt.contains(c);
        let mut count: FxHashMap<Vec<(VarId, Rat)>, usize> = FxHashMap::default();
        for cl in &classes {
            for c in cl {
                *count.entry(c.clone()).or_insert(0) += 1;
            }
        }
        let mut alive = vec![true; tris.len()];
        loop {
            let mut dropped = false;
            for i in 0..tris.len() {
                if !alive[i] {
                    continue;
                }
                let shared = classes[i].iter().filter(|c| count[*c] >= 2 || is_const(c)).count();
                if shared < 2 {
                    alive[i] = false;
                    dropped = true;
                    for c in &classes[i] {
                        *count.get_mut(c).unwrap() -= 1;
                    }
                }
            }
            if !dropped {
                break;
            }
        }
        let mut admitted = 0;
        let mut buckets: FxHashMap<Vec<(VarId, Rat)>, Vec<Corner>> = FxHashMap::default();
        for i in 0..tris.len() {
            if !alive[i] {
                continue;
            }
            admitted += 1;
            let t = tris[i];
            let shared: Vec<bool> = classes[i].iter().map(|c| count[c] >= 2 || is_const(c)).collect();
            for k in 0..3 {
                if shared[k] {
                    let e = buckets.entry(classes[i][k].clone()).or_default();
                    if !e.iter().any(|c| c.key == t[k].key) {
                        e.push(t[k]);
                    }
                }
            }
            for (x, y) in [(0, 1), (1, 2), (0, 2)] {
                if !(shared[x] && shared[y]) || self.trig.rows >= MAX_LOS_ROWS {
                    continue;
                }
                let (cx, cy) = (t[x], t[y]);
                let key = if cx.key <= cy.key { (cx.key, cy.key) } else { (cy.key, cx.key) };
                if !self.trig.los_done.insert(key) {
                    continue;
                }
                let (sx, sy) = (self.sin_var(&cx), self.sin_var(&cy));
                let (ox, oy) = (if cy.p == cx.v { cy.q } else { cy.p }, if cx.p == cy.v { cx.q } else { cx.p });
                let opp_x = self.raw_dist_mul(cy.v, ox);
                let opp_y = self.raw_dist_mul(cx.v, oy);
                let row = opp_x.mul(&sy).div(&opp_y.mul(&sx));
                self.trig.rows += 1;
                let mut sp = Spell::default();
                let text = format!(
                    "in △{}{}{}, {} / |sin {}| = {} / |sin {}|",
                    sp.p(t[0].v),
                    sp.p(t[1].v),
                    sp.p(t[2].v),
                    sp.side(cy.v, ox),
                    sp.corner(&cx, false),
                    sp.side(cx.v, oy),
                    sp.corner(&cy, false)
                );
                changed |= self.trig_force(&row, sp.reason("law of sines", text), vec![]);
            }
        }
        self.trig.admitted = admitted;
        let mut keys: Vec<Vec<(VarId, Rat)>> = buckets.keys().cloned().collect();
        keys.sort();
        for k in keys {
            let corners = buckets[&k].clone();
            if let (Some(vs), Some(c)) = (self.trig.vsvar.get(&k).cloned(), corners.first().copied()) {
                if self.trig.vlink_done.insert((k.clone(), c.key)) {
                    let s = self.sin_var(&c);
                    let deps = angles[&c.key].1.clone();
                    let reason = self.link_reason(&k, &c);
                    changed |= self.trig_force(&vs.div(&s), reason, deps);
                }
            }
            if k.iter().all(|(v, _)| *v == ANGLE_UNIT) {
                let frac = k.first().map(|(_, r)| r.mod_one()).unwrap_or_else(Rat::zero);
                if let Some((value, name, text)) = self.known_sine(&frac) {
                    for c in &corners {
                        if !self.trig.known_done.insert(c.key) {
                            continue;
                        }
                        let s = self.sin_var(c);
                        let deps = angles[&c.key].1.clone();
                        let mut sp = Spell::default();
                        let stmt = format!("|sin {}| = {text}", sp.corner(c, false));
                        changed |= self.trig_force(&s.div(&value), sp.reason(name, stmt), deps);
                    }
                }
            }
            for w in corners.windows(2) {
                if self.trig.eq_rows >= MAX_EQ_ROWS {
                    break;
                }
                let (c1, c2) = (w[0], w[1]);
                let key = if c1.key <= c2.key { (c1.key, c2.key) } else { (c2.key, c1.key) };
                if !self.trig.eq_done.insert(key) {
                    continue;
                }
                self.trig.eq_rows += 1;
                let (s1, s2) = (self.sin_var(&c1), self.sin_var(&c2));
                let mut deps = angles[&c1.key].1.clone();
                deps.extend(angles[&c2.key].1.iter().copied());
                let mut sp = Spell::default();
                let stmt = format!("|sin {}| = |sin {}|", sp.corner(&c1, false), sp.corner(&c2, false));
                changed |= self.trig_force(&s1.div(&s2), sp.reason(EQUAL_SINES, stmt), deps);
            }
        }
        changed |= self.trig_converse(&tris, &classes, &angles);
        changed
    }
}

const EQUAL_SINES: &str = "equal or supplementary angles have equal sines";

/// Smallest angle (in half-turns) the converse accepts at `R` and at `Q`, so
/// the two roots of its sine equation are far apart compared with rounding.
const CONVERSE_MARGIN: f64 = 1e-4;

impl Ddar {
    fn class_angle(class: &[(VarId, Rat)]) -> Angle {
        Angle::new(LinComb {
            terms: class.iter().cloned().collect(),
        })
    }

    /// The class of `class + k·π`, or of `n·class`.
    fn class_shift(&self, class: &[(VarId, Rat)], k: Rat) -> Class {
        let a = Self::class_angle(class).add(&self.angle.const_frac(k));
        Self::corner_class(&self.angle.simplify(&a))
    }

    fn class_times(&self, class: &[(VarId, Rat)], n: i64) -> Class {
        let mut c = Self::class_angle(class).0;
        c.mul_assign_scalar(&Rat::from_int(n));
        Self::corner_class(&self.angle.simplify(&Angle::new(c)))
    }

    fn corner_ref(&self, class: &[(VarId, Rat)], c: &Corner) -> ClassRef {
        let (a, _) = self.corner_angle(c);
        ClassRef {
            c: *c,
            flip: a.0.terms[..] != *class,
            shift: Rat::zero(),
            neg: false,
        }
    }

    fn class_ref(&self, class: &Class, corners: &FxHashMap<Class, Corner>) -> Option<ClassRef> {
        match corners.get(class) {
            Some(c) => Some(self.corner_ref(class, c)),
            None => self.trig.vref.get(class).cloned(),
        }
    }

    fn link_reason(&self, class: &Class, c: &Corner) -> Reason {
        let mut sp = Spell::default();
        let left = match self.trig.vref.get(class) {
            Some(r) => sp.class(r),
            None => sp.corner(c, false),
        };
        let stmt = format!("|sin({left})| = |sin {}|", sp.corner(c, false));
        sp.reason(EQUAL_SINES, stmt)
    }

    /// `|sin|` of a class as a sine variable (LHS, rank 0, like the corner
    /// ones), linked to the first corner of `corners` in that class by an
    /// equal-sines row. `None` for a class whose sine is (numerically) zero.
    fn class_sine(&mut self, class: &Class, corners: &FxHashMap<Class, Corner>, origin: Option<ClassRef>) -> Option<DistMul> {
        let value = (std::f64::consts::PI * self.angle.value_of(&Self::class_angle(class))).sin().abs();
        if value < 1e-9 {
            return None;
        }
        let s = match self.trig.vsvar.get(class) {
            Some(s) => s.clone(),
            None => {
                let r = origin.or_else(|| self.class_ref(class, corners))?;
                let v = self.dmul.core.new_var_ranked(value, true, 0);
                let s = DistMul(LinComb::singleton(v, Rat::one()));
                self.trig.vsvar.insert(class.clone(), s.clone());
                self.trig.vref.insert(class.clone(), r);
                s
            }
        };
        if let Some(c) = corners.get(class).copied() {
            if self.trig.vlink_done.insert((class.clone(), c.key)) {
                let corner = self.sin_var(&c);
                let (_, deps) = self.corner_angle(&c);
                let reason = self.link_reason(class, &c);
                self.trig_force(&s.div(&corner), reason, deps);
            }
        }
        Some(s)
    }

    /// The first corner (in triangle order) of every class.
    fn corners_by_class(tris: &[[Corner; 3]], classes: &[[Class; 3]]) -> FxHashMap<Class, Corner> {
        let mut out: FxHashMap<Class, Corner> = FxHashMap::default();
        for (t, cl) in tris.iter().zip(classes) {
            for k in 0..3 {
                out.entry(cl[k].clone()).or_insert(t[k]);
            }
        }
        out
    }

    /// **Multiple-angle product rows**: for a class `x` whose `n·x` (n = 2, 3)
    /// is the class of a corner, `∏_{j<n} |sin(x + jπ/n)| = |sin(n·x)| / 2^(n−1)`.
    fn trig_multiple_angles(&mut self, tris: &[[Corner; 3]], classes: &[[Class; 3]]) -> bool {
        let corners = Self::corners_by_class(tris, classes);
        let mut keys: Vec<Class> = corners.keys().cloned().collect();
        keys.sort();
        let mut changed = false;
        for x in &keys {
            for n in [2i64, 3] {
                if self.trig.mult_done.len() >= MAX_MULT_ROWS {
                    return changed;
                }
                let nx = self.class_times(x, n);
                if !corners.contains_key(&nx) || nx.is_empty() {
                    continue;
                }
                if !self.trig.mult_done.insert((x.clone(), n as u8)) {
                    continue;
                }
                let Some(sn) = self.class_sine(&nx, &corners, None) else {
                    continue;
                };
                let xr = self.corner_ref(x, &corners[x]);
                let mut row = self.dmul.frac_value(&Rat::from_int(1 << (n - 1))).div(&sn);
                let mut ok = true;
                for j in 0..n {
                    let k = Rat::new(j, n);
                    let c = self.class_shift(x, k.clone());
                    let shifted = self.angle.simplify(&Self::class_angle(x).add(&self.angle.const_frac(k.clone())));
                    let origin = ClassRef {
                        shift: k,
                        neg: shifted.0.terms[..] != c[..],
                        ..xr.clone()
                    };
                    match self.class_sine(&c, &corners, Some(origin)) {
                        Some(sj) => row = row.mul(&sj),
                        None => ok = false,
                    }
                }
                if !ok {
                    continue;
                }
                let mut sp = Spell::default();
                let xt = sp.class(&xr);
                let nt = sp.corner(&corners[&nx], false);
                let (name, stmt) = if n == 2 {
                    ("double-angle formula", format!("x = {xt}, 2x ≡ ±{nt}: |sin x|·|sin(x + 90°)| = |sin 2x| / 2"))
                } else {
                    (
                        "triple-angle formula",
                        format!("x = {xt}, 3x ≡ ±{nt}: |sin x|·|sin(x + 60°)|·|sin(x + 120°)| = |sin 3x| / 4"),
                    )
                };
                changed |= self.trig_force(&row, sp.reason(name, stmt), Vec::new());
            }
        }
        changed
    }

    /// **Converse of the law of sines.** In a triangle `PQR`, a class `w` with
    /// `|PQ|·|sin(∠P + w)| = |PR|·|sin w|` in the ratio table fixes the angle
    /// at `R`: with `p, q, r` the interior angles, `h(t) = |sin t| / |sin(p + t)|`
    /// takes each positive value exactly twice mod π, once in `(0, π − p)` (at
    /// `t = r`, by the law of sines) and once in `(π − p, π)`. The figure picks
    /// the root (`σ·w ≡ r`, `σ` the orientation); both `r` and `q` are at least
    /// [`CONVERSE_MARGIN`], so the roots are apart by more than the figure's
    /// error and the choice holds on a neighbourhood of the figure. The printed
    /// step states that configuration.
    fn trig_converse(
        &mut self,
        tris: &[[Corner; 3]],
        classes: &[[Class; 3]],
        angles: &FxHashMap<(VarId, VarId), (Angle, Vec<FactId>)>,
    ) -> bool {
        let corners = Self::corners_by_class(tris, classes);
        let mut pool: Vec<(f64, Class)> = corners
            .keys()
            .chain(self.trig.vsvar.keys())
            .map(|c| {
                let v = (std::f64::consts::PI * self.angle.value_of(&Self::class_angle(c))).sin().abs();
                (v, c.clone())
            })
            .collect();
        pool.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        pool.dedup_by(|a, b| a.1 == b.1);
        let wrap = |x: f64| x.rem_euclid(1.0);
        let near = |x: f64, y: f64| {
            let d = wrap(x - y);
            d.min(1.0 - d) < 1e-7
        };
        let interior = |s: &Ddar, c: &Corner| -> f64 {
            let (o, p, q) = (s.coord(c.v), s.coord(c.p), s.coord(c.q));
            let (u, w) = (p - o, q - o);
            (u.dot(w) / (u.norm() * w.norm())).clamp(-1.0, 1.0).acos() / std::f64::consts::PI
        };
        let mut changed = false;
        let mut fired = 0;
        for (t, cl) in tris.iter().zip(classes) {
            for k in 0..3 {
                let r = t[k];
                let r_int = interior(self, &r);
                let a_r = wrap(self.angle.value_of(&angles[&r.key].0));
                let s_r = (std::f64::consts::PI * a_r).sin().abs();
                let lo = pool.partition_point(|e| e.0 < s_r - 1e-9);
                let cands: Vec<Class> = pool[lo..]
                    .iter()
                    .take_while(|e| e.0 <= s_r + 1e-9)
                    .map(|e| e.1.clone())
                    .filter(|c| *c != cl[k])
                    .collect();
                if cands.is_empty() {
                    continue;
                }
                for (pi, qi) in [((k + 1) % 3, (k + 2) % 3), ((k + 2) % 3, (k + 1) % 3)] {
                    let (p, q) = (t[pi], t[qi]);
                    let p_int = interior(self, &p);
                    let a_p = wrap(self.angle.value_of(&angles[&p.key].0));
                    let sigma = if near(a_p, p_int) {
                        1.0
                    } else if near(a_p, -p_int) {
                        -1.0
                    } else {
                        continue;
                    };
                    for w in &cands {
                        if fired >= MAX_CONVERSE {
                            return changed;
                        }
                        let w_pos = Self::class_angle(w);
                        let (w_s, w_neg) = if near(self.angle.value_of(&w_pos), a_r) {
                            (w_pos, false)
                        } else if near(self.angle.value_of(&w_pos.neg()), a_r) {
                            (w_pos.neg(), true)
                        } else {
                            continue;
                        };
                        let r_rep = wrap(sigma * self.angle.value_of(&w_s));
                        if !near(r_rep, r_int) || r_rep < CONVERSE_MARGIN || 1.0 - p_int - r_rep < CONVERSE_MARGIN {
                            continue;
                        }
                        if self.trig.conv_done.contains(&(r.key, p.key, w.clone())) {
                            continue;
                        }
                        let a_p_form = &angles[&p.key].0;
                        let v = Self::corner_class(&self.angle.simplify(&a_p_form.neg().sub(&w_s)));
                        let known = |s: &Ddar, c: &Class| corners.contains_key(c) || s.trig.vsvar.contains_key(c);
                        if !known(self, &v) || !known(self, w) {
                            continue;
                        }
                        let (Some(sv), Some(sw)) = (self.class_sine(&v, &corners, None), self.class_sine(w, &corners, None)) else {
                            continue;
                        };
                        let row = self
                            .raw_dist_mul(p.v, q.v)
                            .mul(&sv)
                            .div(&self.raw_dist_mul(p.v, r.v).mul(&sw));
                        let (red, mut prem) = self.dmul.simplify_deps(&row);
                        if !red.is_one() {
                            continue;
                        }
                        let rel = self.raw_dir(r.v, r.p).sub(&self.raw_dir(r.v, r.q)).sub(&w_s);
                        if self.angle.simplify(&rel).is_zero() {
                            continue;
                        }
                        if !near(self.angle.value_of(&rel), 0.0) {
                            self.trig.rejected += 1;
                            debug_assert!(false, "converse law of sines disagrees with the figure");
                            continue;
                        }
                        let Some(wr) = self.class_ref(w, &corners) else {
                            continue;
                        };
                        self.trig.conv_done.insert((r.key, p.key, w.clone()));
                        prem.extend(angles[&p.key].1.iter().copied());
                        let mut sp = Spell::default();
                        let wt = sp.class(&wr.clone().negated(w_neg != wr.neg));
                        let tri = format!("{}{}{}", sp.p(p.v), sp.p(q.v), sp.p(r.v));
                        let pq = sp.side(p.v, q.v);
                        let pr = sp.side(p.v, r.v);
                        let at_p = sp.corner(&p, false);
                        let at_r = sp.corner(&r, false);
                        let stmt = format!(
                            "in △{tri} with w = {wt}, {pq}·|sin({at_p} + w)| = {pr}·|sin w|; \
                             by the law of sines the interior angle t = R solves |sin t| / |sin(P + t)| = {pq} / {pr}, \
                             which has one root with 0 < t < 180° − P, and in this configuration ±w is that root \
                             ⇒ {at_r} = w"
                        );
                        let fact = self.log.add(sp.reason("law of sines, converse", stmt), prem);
                        fired += 1;
                        changed |= self.angle.force_zero(&rel, Some(fact));
                    }
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::Point;

    #[test]
    fn residual_guard_skips_a_numerically_false_row() {
        let pts: Vec<Point> = [(0.0, 0.0), (3.0, 0.0), (0.5, 2.0)]
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| Point {
                name: format!("P{i}"),
                value: Vec2::new(x, y),
            })
            .collect();
        let mut d = Ddar::new(&pts);
        let (ab, ac) = (d.raw_dist_mul(0, 1), d.raw_dist_mul(0, 2));
        let row = ab.div(&ac);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            d.trig_force(&row, Reason::Theorem("law of sines", vec![0, 1, 2]), vec![])
        }));
        if cfg!(debug_assertions) {
            assert!(res.is_err(), "debug builds assert on a failing trig row");
        } else {
            assert_eq!(res.ok(), Some(false));
            assert_eq!(d.trig.rejected, 1);
            assert!(!d.dmul.simplify(&row).is_one(), "the false row must not enter the system");
        }
    }
}
