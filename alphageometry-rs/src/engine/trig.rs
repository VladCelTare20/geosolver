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
    /// Never (the default): the closure is exactly the trig-free one.
    Off,
    /// Only when a closed figure leaves an eligible goal unproved
    /// ([`crate::runner`]); the aux search stays trig-free.
    Fallback,
    /// In every closure, once its trig-free fixpoint is reached (base and aux).
    Lazy,
    /// In every closure from the first round.
    Always,
}

pub fn mode() -> TrigMode {
    static MODE: OnceLock<TrigMode> = OnceLock::new();
    *MODE.get_or_init(|| match std::env::var("GEO_TRIG").as_deref() {
        Ok("fallback") => TrigMode::Fallback,
        Ok("lazy") => TrigMode::Lazy,
        Ok("always") => TrigMode::Always,
        _ => TrigMode::Off,
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
    rows: usize,
    eq_rows: usize,
    pub(crate) rejected: usize,
    pub(crate) admitted: usize,
    pub(crate) candidates: usize,
}

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

impl Ddar {
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
    fn known_sine(&mut self, k: &Rat) -> Option<(DistMul, &'static str)> {
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
            2 | 10 => (scaled(&two, Rat::from_int(-1)), "sine of 30°: 1/2"),
            3 | 9 => (scaled(&two, Rat::new(-1, 2)), "sine of 45°: √2/2"),
            4 | 8 => (scaled(&three, Rat::new(1, 2)).mul(&scaled(&two, Rat::from_int(-1))), "sine of 60°: √3/2"),
            6 => (DistMul(LinComb::zero()), "sine of 90°: 1"),
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
        let is_const = |c: &[(VarId, Rat)]| c.iter().all(|(v, _)| *v == ANGLE_UNIT);
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
        let mut changed = false;
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
                let opp_x = self.raw_dist_mul(cy.v, if cy.p == cx.v { cy.q } else { cy.p });
                let opp_y = self.raw_dist_mul(cx.v, if cx.p == cy.v { cx.q } else { cx.p });
                let row = opp_x.mul(&sy).div(&opp_y.mul(&sx));
                self.trig.rows += 1;
                changed |= self.trig_force(&row, Reason::Theorem("law of sines", vec![t[0].v, t[1].v, t[2].v]), vec![]);
            }
        }
        self.trig.admitted = admitted;
        let mut keys: Vec<Vec<(VarId, Rat)>> = buckets.keys().cloned().collect();
        keys.sort();
        for k in keys {
            let corners = buckets[&k].clone();
            if is_const(&k) {
                let frac = k.first().map(|(_, r)| r.mod_one()).unwrap_or_else(Rat::zero);
                if let Some((value, name)) = self.known_sine(&frac) {
                    for c in &corners {
                        if !self.trig.known_done.insert(c.key) {
                            continue;
                        }
                        let s = self.sin_var(c);
                        let deps = angles[&c.key].1.clone();
                        changed |= self.trig_force(&s.div(&value), Reason::Theorem(name, vec![c.p, c.v, c.q]), deps);
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
                changed |= self.trig_force(
                    &s1.div(&s2),
                    Reason::Theorem("equal or supplementary angles have equal sines", vec![c1.p, c1.v, c1.q, c2.p, c2.v, c2.q]),
                    deps,
                );
            }
        }
        changed
    }
}
