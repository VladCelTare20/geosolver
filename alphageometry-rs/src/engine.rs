//! The DDAR deductive engine.
//!
//! This is a faithful, index-based port of `ddar.py`. The reference keys every
//! table by hashable Python point *objects* and ordered point pairs/triples;
//! here points are dense `u32` ids and the per-pair tables are flat `n*n`
//! arrays, which removes essentially all hashing from the hot path.
//!
//! The engine maintains a *deductive closure*: starting from the assumed
//! predicates it repeatedly applies a fixed set of geometric inference rules
//! (similar triangles, concyclicity, equal-radius circles, point merging, and
//! two "transfer" rules linking the additive/multiplicative/angular systems)
//! until no new fact can be derived. A goal predicate is proved iff it holds in
//! the closure.

use crate::elimination::{
    Angle, DistAdd, DistMul, DistSq, ElimAngle, ElimDistAdd, ElimDistMul, ElimDistSq,
};
use crate::fingerprint;
use crate::lincomb::LinComb;
use crate::numerics::{
    collinear, direction_of, distance, orientation, NumCircle, NumLine, Vec2, ATOM,
};
use crate::predicate::{Point, PointId, Predicate};
use crate::proof::{FactId, ProofLog, Reason};
use crate::rational::Rat;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::hash_map::Entry;

mod classics;
mod sqlen;

/// A maximal set of collinear points (immutable snapshot).
#[derive(Clone, Debug)]
struct FormalLine {
    points: Vec<PointId>,
    main_pair: (PointId, PointId),
    direction: Angle,
    value: NumLine,
    /// The proof fact that established this line (None for the trivial
    /// two-point lines of the initial figure).
    fact: Option<FactId>,
}

/// A set of concyclic points, possibly with known centers.
#[derive(Clone, Debug)]
struct FormalCircle {
    /// Three points that numerically define the circle (`None` for the transient
    /// "small" circles produced while searching, which never enter the DB).
    defining_points: Option<[PointId; 3]>,
    points: Vec<PointId>,
    centers: Vec<PointId>,
    value: NumCircle,
    /// The proof fact that established this circle.
    fact: Option<FactId>,
}

pub mod trig;

type Triple = (PointId, PointId, PointId);

/// The main logical engine.
#[derive(Clone)]
pub struct Ddar {
    /// Number of original points; all `n*n` tables are indexed by original id.
    n: usize,
    coords: Vec<Vec2>,
    names: Vec<String>,

    /// Currently active (unmerged) points.
    active: Vec<PointId>,
    /// `subst[i]` is the current representative of original point `i`.
    subst: Vec<PointId>,

    angle: ElimAngle,
    dmul: ElimDistMul,
    dadd: ElimDistAdd,
    dsq: ElimDistSq,

    // Per-pair tables (flat n*n; `None` for numerically identical pairs).
    pair_dir: Vec<Option<Angle>>,
    pair_dist_mul: Vec<Option<DistMul>>,
    pair_dist_add: Vec<Option<DistAdd>>,
    pair_dist_sq: Vec<Option<DistSq>>,
    pair_line: Vec<Option<usize>>,

    // Object arenas + live-id lists (mirrors the reference's `set`s).
    lines: Vec<FormalLine>,
    live_lines: Vec<usize>,
    circles: Vec<FormalCircle>,
    live_circles: Vec<usize>,
    triple_to_circle: FxHashMap<Triple, usize>,

    known_similar: FxHashSet<(Triple, Triple)>,
    last_small_circles: Vec<FormalCircle>,

    // Caches of simplified values, refreshed by `update_cache`.
    dist_mul_cache: Vec<Option<DistMul>>,
    dir_cache: Vec<Option<Angle>>,

    /// Proof provenance log (facts + reasons). Row-level dependency tracking is
    /// only active when constructed via [`Ddar::new_tracked`].
    log: ProofLog,

    classics: classics::Done,
    rules_off: u8,
    trig: trig::TrigState,
}

impl Ddar {
    #[inline]
    fn pk(&self, a: PointId, b: PointId) -> usize {
        a as usize * self.n + b as usize
    }

    /// Whether two points are numerically identical (no variable was created).
    #[inline]
    fn num_identical(&self, a: PointId, b: PointId) -> bool {
        self.pair_dist_mul[self.pk(a, b)].is_none()
    }

    // ---- owned-clone accessors (avoid borrow conflicts; values are small) ----

    fn raw_dir(&self, a: PointId, b: PointId) -> Angle {
        self.pair_dir[self.pk(a, b)]
            .clone()
            .expect("no direction for pair")
    }
    fn raw_dist_mul(&self, a: PointId, b: PointId) -> DistMul {
        self.pair_dist_mul[self.pk(a, b)]
            .clone()
            .expect("no dist_mul for pair")
    }
    fn raw_dist_add(&self, a: PointId, b: PointId) -> DistAdd {
        self.pair_dist_add[self.pk(a, b)]
            .clone()
            .expect("no dist_add for pair")
    }
    fn raw_dist_sq(&self, a: PointId, b: PointId) -> DistSq {
        self.pair_dist_sq[self.pk(a, b)]
            .clone()
            .expect("no dist_sq for pair")
    }
    fn cached_dir(&self, a: PointId, b: PointId) -> &Angle {
        self.dir_cache[self.pk(a, b)]
            .as_ref()
            .expect("no cached dir")
    }
    fn cached_dist_mul(&self, a: PointId, b: PointId) -> &DistMul {
        self.dist_mul_cache[self.pk(a, b)]
            .as_ref()
            .expect("no cached dist_mul")
    }

    fn get_dist_mul(&self, a: PointId, b: PointId) -> DistMul {
        self.dmul.simplify(&self.raw_dist_mul(a, b))
    }
    fn get_dist_add(&self, a: PointId, b: PointId) -> DistAdd {
        self.dadd.simplify(&self.raw_dist_add(a, b))
    }
    fn get_point_dir(&self, a: PointId, b: PointId) -> Angle {
        self.angle.simplify(&self.raw_dir(a, b))
    }
    /// `|cd| / |ab|` from the cache.
    fn get_dist_ratio(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> DistMul {
        self.cached_dist_mul(c, d).div(self.cached_dist_mul(a, b))
    }
    /// `dir(cd) - dir(ab)` from the cache.
    fn get_point_angle(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Angle {
        self.cached_dir(c, d).sub(self.cached_dir(a, b))
    }

    fn pair_ids(&self) -> PairIds {
        let n = self.n;
        let pairs = self.active.len() * self.active.len();
        let mut ids = PairIds {
            n,
            dir: Interner::with_capacity(true, pairs),
            dm: Interner::with_capacity(false, pairs),
            dir_id: vec![u32::MAX; n * n],
            dm_id: vec![u32::MAX; n * n],
            memo: TripleMemo::take(n * n * n),
            fp_ok: true,
            dir_fp: vec![0; n * n],
            dir_unit: vec![Rat::zero(); n * n],
            dir_unit_rank: vec![0; n * n],
            w0: fingerprint::weight(crate::elimination::ANGLE_UNIT),
            dm_fp: vec![0; n * n],
        };
        let unit = crate::elimination::ANGLE_UNIT;
        for &a in &self.active {
            for &b in &self.active {
                if self.num_identical(a, b) {
                    continue;
                }
                let k = self.pk(a, b);
                let dir = &self.dir_cache[k].as_ref().unwrap().0;
                let dm = &self.dist_mul_cache[k].as_ref().unwrap().0;
                ids.dir_id[k] = ids.dir.intern(dir.clone());
                ids.dm_id[k] = ids.dm.intern(dm.clone());
                if !ids.fp_ok {
                    continue;
                }
                let u = dir.get(unit);
                match (
                    fingerprint::lincomb_without(dir, Some(unit)),
                    fingerprint::rat(&u),
                    fingerprint::lincomb_without(dm, None),
                ) {
                    (Some(rest), Some(ufp), Some(dmfp)) => {
                        ids.dir_fp[k] = fingerprint::add(rest, fingerprint::mul(ids.w0, ufp));
                        ids.dir_unit[k] = u;
                        ids.dm_fp[k] = dmfp;
                    }
                    _ => ids.fp_ok = false,
                }
            }
        }
        if ids.fp_ok {
            let mut units: Vec<Rat> = Vec::new();
            for &a in &self.active {
                for &b in &self.active {
                    if !self.num_identical(a, b) {
                        let u = &ids.dir_unit[self.pk(a, b)];
                        if !units.contains(u) {
                            units.push(u.clone());
                        }
                    }
                }
            }
            units.sort();
            for &a in &self.active {
                for &b in &self.active {
                    if !self.num_identical(a, b) {
                        let k = self.pk(a, b);
                        ids.dir_unit_rank[k] = units.binary_search(&ids.dir_unit[k]).unwrap() as u32;
                    }
                }
            }
        }
        ids
    }

    fn get_arc(&self, circle: &FormalCircle, a: PointId, b: PointId) -> (Angle, PointId) {
        let dp = circle
            .defining_points
            .expect("arc requested on circle without defining points");
        let c = dp
            .into_iter()
            .find(|&c| !(self.num_identical(a, c) || self.num_identical(b, c)))
            .expect("no defining point distinct from a and b");
        let ang = self.raw_dir(b, c).sub(&self.raw_dir(a, c));
        (ang, c)
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl Ddar {
    /// Build an engine over the given points, creating a variable for the
    /// direction and (log-/additive) distance of every numerically-distinct
    /// pair.
    pub fn new(points: &[Point]) -> Ddar {
        Ddar::new_with_slack(points, 0)
    }

    /// Like [`Ddar::new`] but reserves `extra` inactive point slots (all tables
    /// sized for them) that [`Ddar::activate`] can later fill. This is the basis
    /// of warm-start: close a base figure once, then per candidate clone and
    /// activate a fresh point instead of rebuilding. Reserved slots are absent
    /// from `active`/`live_lines`, so the closure ignores them entirely until
    /// activated.
    pub fn new_with_slack(points: &[Point], extra: usize) -> Ddar {
        let base_n = points.len();
        let n = base_n + extra;
        let mut coords: Vec<Vec2> = points.iter().map(|p| p.value).collect();
        coords.resize(n, Vec2::new(0.0, 0.0));
        let mut names: Vec<String> = points.iter().map(|p| p.name.clone()).collect();
        names.resize(n, String::new());

        let mut angle = ElimAngle::new();
        let mut dmul = ElimDistMul::new();
        let mut dadd = ElimDistAdd::new();
        let mut dsq = ElimDistSq::new();

        let mut pair_dir: Vec<Option<Angle>> = vec![None; n * n];
        let mut pair_dist_mul: Vec<Option<DistMul>> = vec![None; n * n];
        let mut pair_dist_add: Vec<Option<DistAdd>> = vec![None; n * n];
        let mut pair_dist_sq: Vec<Option<DistSq>> = vec![None; n * n];
        let mut pair_line: Vec<Option<usize>> = vec![None; n * n];

        let mut lines: Vec<FormalLine> = Vec::new();
        let mut live_lines: Vec<usize> = Vec::new();

        for a in 0..base_n {
            for b in (a + 1)..base_n {
                let (va, vb) = (coords[a], coords[b]);
                if distance(va, vb) < ATOM {
                    continue;
                }
                let num_line = NumLine::through(va, vb);
                let dir = angle.new_var(num_line.direction());

                let line_id = lines.len();
                lines.push(FormalLine {
                    points: vec![a as PointId, b as PointId],
                    main_pair: (a as PointId, b as PointId),
                    direction: dir.clone(),
                    value: num_line,
                    fact: None,
                });
                live_lines.push(line_id);

                let idx_ab = a * n + b;
                let idx_ba = b * n + a;
                pair_dir[idx_ab] = Some(dir.clone());
                pair_dir[idx_ba] = Some(dir);
                pair_line[idx_ab] = Some(line_id);
                pair_line[idx_ba] = Some(line_id);

                let d = distance(va, vb);
                let dm = dmul.new_var(d);
                pair_dist_mul[idx_ab] = Some(dm.clone());
                pair_dist_mul[idx_ba] = Some(dm);
                let da = dadd.new_var(d);
                pair_dist_add[idx_ab] = Some(da.clone());
                pair_dist_add[idx_ba] = Some(da);
                let ds = dsq.new_var(d * d);
                pair_dist_sq[idx_ab] = Some(ds.clone());
                pair_dist_sq[idx_ba] = Some(ds);
            }
        }

        let dist_mul_cache = pair_dist_mul.clone();
        let dir_cache = pair_dir.clone();
        let subst: Vec<PointId> = (0..n as PointId).collect();
        let active: Vec<PointId> = (0..base_n as PointId).collect();

        Ddar {
            n,
            coords,
            names,
            active,
            subst,
            angle,
            dmul,
            dadd,
            dsq,
            pair_dir,
            pair_dist_mul,
            pair_dist_add,
            pair_dist_sq,
            pair_line,
            lines,
            live_lines,
            circles: Vec::new(),
            live_circles: Vec::new(),
            triple_to_circle: FxHashMap::default(),
            known_similar: FxHashSet::default(),
            last_small_circles: Vec::new(),
            dist_mul_cache,
            dir_cache,
            log: ProofLog::new(),
            classics: classics::Done::default(),
            rules_off: classics::env_disabled_mask(),
            trig: trig::TrigState::default(),
        }
    }

    /// Fill a reserved slot `id` at `coord`, creating its direction/distance
    /// variables and pair-tables against every currently-active point and
    /// marking it active. Mirrors the per-point work `new` does up front, so a
    /// subsequently-forced predicate + closure produce exactly the state a
    /// from-scratch build would — the closure is confluent, so the final
    /// proven/not verdict is identical (see `runner::WarmBase`, verified by
    /// `--verify-warm`). `id` must be a slot reserved by `new_with_slack` and
    /// not yet active.
    pub fn activate(&mut self, id: PointId, coord: Vec2) {
        self.coords[id as usize] = coord;
        for a in self.active.clone() {
            let va = self.coords[a as usize];
            if distance(va, coord) < ATOM {
                continue; // numerically identical: leave pairs None (num_identical)
            }
            let num_line = NumLine::through(va, coord);
            let dir = self.angle.new_var(num_line.direction());
            let line_id = self.lines.len();
            self.lines.push(FormalLine {
                points: vec![a, id],
                main_pair: (a, id),
                direction: dir.clone(),
                value: num_line,
                fact: None,
            });
            self.live_lines.push(line_id);

            let (ai, ia) = (self.pk(a, id), self.pk(id, a));
            self.pair_dir[ai] = Some(dir.clone());
            self.pair_dir[ia] = Some(dir);
            self.pair_line[ai] = Some(line_id);
            self.pair_line[ia] = Some(line_id);
            let d = distance(va, coord);
            let dm = self.dmul.new_var(d);
            self.pair_dist_mul[ai] = Some(dm.clone());
            self.pair_dist_mul[ia] = Some(dm);
            let da = self.dadd.new_var(d);
            self.pair_dist_add[ai] = Some(da.clone());
            self.pair_dist_add[ia] = Some(da);
            let ds = self.dsq.new_var(d * d);
            self.pair_dist_sq[ai] = Some(ds.clone());
            self.pair_dist_sq[ia] = Some(ds);
            // Seed the caches like `new` does (force_pred reads them before the
            // first `update_cache`).
            self.dist_mul_cache[ai] = self.pair_dist_mul[ai].clone();
            self.dist_mul_cache[ia] = self.pair_dist_mul[ia].clone();
            self.dir_cache[ai] = self.pair_dir[ai].clone();
            self.dir_cache[ia] = self.pair_dir[ia].clone();
        }
        self.active.push(id);
    }

    /// Like [`Ddar::new`], additionally maintaining row-level provenance so a
    /// proof can be reconstructed after solving (small overhead).
    pub fn new_tracked(points: &[Point]) -> Ddar {
        let mut d = Ddar::new(points);
        d.angle.core.track = true;
        d.dmul.core.track = true;
        d.dadd.core.track = true;
        d.dsq.core.track = true;
        d
    }

    fn coord(&self, p: PointId) -> Vec2 {
        self.coords[p as usize]
    }
    fn name(&self, p: PointId) -> &str {
        &self.names[p as usize]
    }

    /// Whether the centre of `circle` lies left of chord `a → b`, by a margin
    /// relative to the chord and radius. A chord through the centre (a
    /// diameter, up to rounding) has no reliable side.
    fn centre_strictly_left(&self, a: PointId, b: PointId, circle: &NumCircle) -> bool {
        let (pa, pb, o) = (self.coord(a), self.coord(b), circle.center);
        let det = (pb.x - pa.x) * (o.y - pa.y) - (pb.y - pa.y) * (o.x - pa.x);
        let scale = distance(pa, pb) * circle.r;
        scale > 0.0 && det > 1e-9 * scale
    }

    /// Whether `a, b, c` are numerically flat by `force_collinear`'s criterion:
    /// the smallest vertex-to-opposite-side distance (`|det| / longest side`)
    /// is below [`ATOM`]. This is *stricter about degeneracy* than
    /// [`orientation`], which only compares the raw determinant to `ATOM`.
    fn numerically_flat(&self, a: PointId, b: PointId, c: PointId) -> bool {
        let (pa, pb, pc) = (self.coord(a), self.coord(b), self.coord(c));
        let det = (pb.x - pa.x) * (pc.y - pa.y) - (pb.y - pa.y) * (pc.x - pa.x);
        let max_side = distance(pa, pb).max(distance(pb, pc)).max(distance(pa, pc));
        max_side > 0.0 && det.abs() / max_side < ATOM
    }

    /// Render a predicate with point names (for assumption reasons and goals).
    pub fn render_pred(&self, pred: &Predicate) -> String {
        let mut t = vec![pred.name.clone()];
        t.extend(pred.points.iter().map(|&p| self.names[p as usize].clone()));
        t.extend(pred.constants.iter().map(|c| c.to_string()));
        t.join(" ")
    }

    // Provenance helpers: the facts a (raw) quantity's normal form relies on.
    fn deps_of_angle_expr(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Vec<FactId> {
        let raw = self.raw_dir(c, d).sub(&self.raw_dir(a, b));
        self.angle.simplify_deps(&raw).1
    }
    fn deps_of_ratio_expr(&self, a: PointId, b: PointId, c: PointId, d: PointId) -> Vec<FactId> {
        let raw = self.raw_dist_mul(c, d).div(&self.raw_dist_mul(a, b));
        self.dmul.simplify_deps(&raw).1
    }

    fn add_line(&mut self, line: FormalLine) -> usize {
        let id = self.lines.len();
        self.lines.push(line);
        self.live_lines.push(id);
        id
    }
    fn add_circle(&mut self, circle: FormalCircle) -> usize {
        let id = self.circles.len();
        self.circles.push(circle);
        self.live_circles.push(id);
        id
    }
}

// ---------------------------------------------------------------------------
// Predicate translation
// ---------------------------------------------------------------------------

impl Ddar {
    fn subst_points(&self, pred: &Predicate) -> Vec<PointId> {
        pred.points
            .iter()
            .map(|&p| self.subst[p as usize])
            .collect()
    }

    /// Translate an angle predicate into an [`Angle`] equation (== 0 to hold).
    fn pred_to_angle(&self, name: &str, pts: &[PointId], consts: &[Rat]) -> Angle {
        match name {
            "angeq" => {
                debug_assert_eq!(pts.len(), 2 * (consts.len() - 1));
                let coefs = &consts[..consts.len() - 1];
                let konst = &consts[consts.len() - 1];
                let mut comb = LinComb::zero();
                for (i, coef) in coefs.iter().enumerate() {
                    let (a, b) = (pts[2 * i], pts[2 * i + 1]);
                    comb.iadd_mul(&self.raw_dir(a, b).0, coef);
                }
                let konst_ang = self.angle.const_frac(konst / &Rat::from_int(180));
                comb.iadd_mul(&konst_ang.0, &Rat::one());
                Angle::new(comb)
            }
            "eqangle" => {
                let p = pts;
                let ang1 = self.raw_dir(p[0], p[1]).sub(&self.raw_dir(p[2], p[3]));
                let ang2 = self.raw_dir(p[4], p[5]).sub(&self.raw_dir(p[6], p[7]));
                ang1.sub(&ang2)
            }
            "para" => self
                .raw_dir(pts[0], pts[1])
                .sub(&self.raw_dir(pts[2], pts[3])),
            "perp" => self
                .raw_dir(pts[0], pts[1])
                .sub(&self.raw_dir(pts[2], pts[3]))
                .sub(&self.angle.const_ratio(1, 2)),
            "s_angle" | "aconst" => {
                let ang = &consts[0] / &Rat::from_int(180);
                self.raw_dir(pts[0], pts[1])
                    .sub(&self.raw_dir(pts[2], pts[3]))
                    .sub(&self.angle.const_frac(ang))
            }
            _ => panic!("not an angle predicate: {name}"),
        }
    }

    /// Translate a multiplicative-distance predicate into a log-equation
    /// (== 1 to hold).
    fn pred_to_dist_mul(&mut self, name: &str, pts: &[PointId], consts: &[Rat]) -> DistMul {
        match name {
            "distmeq" => {
                debug_assert_eq!(pts.len(), 2 * (consts.len() - 1));
                let coefs = &consts[..consts.len() - 1];
                let konst = &consts[consts.len() - 1];
                assert!(!konst.is_negative() && !konst.is_zero());
                let mut comb = LinComb::zero();
                for (i, coef) in coefs.iter().enumerate() {
                    let (a, b) = (pts[2 * i], pts[2 * i + 1]);
                    comb.iadd_mul(&self.raw_dist_mul(a, b).0, coef);
                }
                self.dmul.mul_const(&DistMul(comb), konst)
            }
            "cong" => {
                let d1 = self.get_dist_mul(pts[0], pts[1]);
                let d2 = self.get_dist_mul(pts[2], pts[3]);
                d1.div(&d2)
            }
            "rconst" => {
                let d1 = self.get_dist_mul(pts[0], pts[1]);
                let d2 = self.get_dist_mul(pts[2], pts[3]);
                let r = d1.div(&d2);
                self.dmul.div_const(&r, &consts[0])
            }
            "eqratio" => {
                let rat1 = self.get_dist_ratio(pts[0], pts[1], pts[2], pts[3]);
                let rat2 = self.get_dist_ratio(pts[4], pts[5], pts[6], pts[7]);
                rat1.div(&rat2)
            }
            _ => panic!("not a ratio predicate: {name}"),
        }
    }

    /// Like [`Self::pred_to_dist_mul`] but built purely from raw quantities
    /// (no cache, no pre-simplification) — used by the proof path so the full
    /// reduction is visible to provenance collection.
    fn pred_to_dist_mul_raw(&mut self, name: &str, pts: &[PointId], consts: &[Rat]) -> DistMul {
        match name {
            "cong" => self
                .raw_dist_mul(pts[0], pts[1])
                .div(&self.raw_dist_mul(pts[2], pts[3])),
            "rconst" => {
                let r = self
                    .raw_dist_mul(pts[0], pts[1])
                    .div(&self.raw_dist_mul(pts[2], pts[3]));
                self.dmul.div_const(&r, &consts[0])
            }
            "eqratio" => {
                let rat1 = self
                    .raw_dist_mul(pts[2], pts[3])
                    .div(&self.raw_dist_mul(pts[0], pts[1]));
                let rat2 = self
                    .raw_dist_mul(pts[6], pts[7])
                    .div(&self.raw_dist_mul(pts[4], pts[5]));
                rat1.div(&rat2)
            }
            // `distmeq` is already built from raw quantities.
            _ => self.pred_to_dist_mul(name, pts, consts),
        }
    }

    /// Translate an additive-distance predicate into an equation (== 0 to hold).
    fn pred_to_dist_add(&self, name: &str, pts: &[PointId], consts: &[Rat]) -> DistAdd {
        match name {
            "distseq" => {
                debug_assert_eq!(pts.len(), 2 * consts.len());
                let mut comb = LinComb::zero();
                for (i, coef) in consts.iter().enumerate() {
                    let (a, b) = (pts[2 * i], pts[2 * i + 1]);
                    comb.iadd_mul(&self.raw_dist_add(a, b).0, coef);
                }
                DistAdd(comb)
            }
            _ => panic!("not a sum predicate: {name}"),
        }
    }
}

const ANGLE_PREDS: &[&str] = &["angeq", "para", "perp", "s_angle", "aconst", "eqangle"];
const DIST_MUL_PREDS: &[&str] = &["distmeq", "cong", "eqratio", "rconst"];

// ---------------------------------------------------------------------------
// Assuming and checking predicates
// ---------------------------------------------------------------------------

impl Ddar {
    /// Add a predicate as an assumption.
    pub fn force_pred(&mut self, pred: &Predicate) {
        let reason = Reason::Assumption(self.render_pred(pred));
        self.force_pred_because(pred, reason);
    }

    /// Add the defining fact of an auxiliary point (logged as a construction,
    /// not a hypothesis).
    pub fn force_construction(&mut self, pred: &Predicate) {
        let reason = Reason::Construction(self.render_pred(pred));
        self.force_pred_because(pred, reason);
    }

    fn force_pred_because(&mut self, pred: &Predicate, reason: Reason) {
        let fact = self.log.add(reason, vec![]);
        let pts = self.subst_points(pred);
        let name = pred.name.as_str();
        let consts = &pred.constants;
        if name == "coll" {
            self.force_collinear(&pts, vec![fact]);
        } else if ANGLE_PREDS.contains(&name) {
            let a = self.pred_to_angle(name, &pts, consts);
            self.angle.force_zero(&a, Some(fact));
        } else if DIST_MUL_PREDS.contains(&name) {
            let d = self.pred_to_dist_mul(name, &pts, consts);
            self.dmul.force_one(&d, Some(fact));
        } else if name == "distseq" {
            let d = self.pred_to_dist_add(name, &pts, consts);
            self.dadd.force_zero(&d, Some(fact));
        } else if name == "cyclic" {
            self.force_concyclic(&pts, &[], vec![fact]);
        } else if name == "cyclic_with_centers" {
            let num_centers = consts[0].numer_i64().unwrap() as usize;
            let centers = pts[..num_centers].to_vec();
            let points = pts[num_centers..].to_vec();
            let mut distinct: Vec<PointId> = Vec::new();
            for &x in &points {
                if !distinct.iter().any(|&y| self.num_identical(x, y)) {
                    distinct.push(x);
                    if distinct.len() == 3 {
                        break;
                    }
                }
            }
            if distinct.len() >= 3 {
                self.force_concyclic(&points, &centers, vec![fact]);
            } else {
                let (a0, c0) = (points[0], centers[0]);
                let d0 = self.get_dist_mul(a0, c0);
                for &a in &points {
                    for &c in &centers {
                        let d = self.get_dist_mul(a, c);
                        let ratio = d0.div(&d);
                        self.dmul.force_one(&ratio, Some(fact));
                    }
                }
            }
        } else if name == "overlap" {
            self.force_equal_points(pts[0], pts[1], vec![fact]);
        } else if name == "acompute" || name == "rcompute" {
            // Nothing to force.
        } else {
            panic!("unexpected predicate: {name}");
        }
    }

    /// Whether a predicate holds in the current closure.
    ///
    /// For `acompute` this matches the reference's truthiness: the goal counts
    /// as proved iff the angle is determined and nonzero.
    pub fn check_pred(&mut self, pred: &Predicate) -> bool {
        if pred.name == "acompute" {
            return self.acompute(pred).is_some_and(|v| !v.is_zero());
        }
        if pred.name == "rcompute" {
            return self.rcompute_deps(pred).is_some();
        }
        let pts = self.subst_points(pred);
        let name = pred.name.as_str();
        let consts = &pred.constants;
        if name == "coll" {
            self.check_collinear(&pts)
        } else if ANGLE_PREDS.contains(&name) {
            let a = self.pred_to_angle(name, &pts, consts);
            self.angle.simplify(&a).is_zero()
        } else if DIST_MUL_PREDS.contains(&name) {
            let d = self.pred_to_dist_mul(name, &pts, consts);
            self.dmul.simplify(&d).is_one()
        } else if name == "distseq" {
            let d = self.pred_to_dist_add(name, &pts, consts);
            self.dadd.simplify(&d).is_zero()
        } else if name == "cyclic" {
            self.check_concyclic(&pts, &[])
        } else if name == "cyclic_with_centers" {
            let num_centers = consts[0].numer_i64().unwrap() as usize;
            let centers = &pts[..num_centers];
            let points = &pts[num_centers..];
            self.check_concyclic(points, centers)
        } else if name == "overlap" {
            // `overlap` is intentionally *not* subst-replaced in the reference.
            self.check_equal_points(pred.points[0], pred.points[1])
        } else {
            panic!("unexpected predicate: {name}");
        }
    }

    /// Like [`Self::check_pred`], additionally returning the proof facts the
    /// goal's reduction relied on. Only meaningful on an engine built with
    /// [`Ddar::new_tracked`]. Returns `None` when the goal does not hold.
    pub fn check_pred_deps(&mut self, pred: &Predicate) -> Option<Vec<FactId>> {
        if pred.name == "acompute" {
            let pts = self.subst_points(pred);
            let ang = self
                .raw_dir(pts[0], pts[1])
                .sub(&self.raw_dir(pts[2], pts[3]));
            let (ang, deps) = self.angle.simplify_deps(&ang);
            let determined = ang
                .0
                .terms
                .iter()
                .all(|(v, _)| *v == crate::elimination::ANGLE_UNIT);
            let nonzero = !ang.0.get(crate::elimination::ANGLE_UNIT).is_zero();
            return (determined && nonzero).then_some(deps);
        }
        if pred.name == "rcompute" {
            return self.rcompute_deps(pred).map(|(_, deps)| deps);
        }
        let pts = self.subst_points(pred);
        let name = pred.name.as_str();
        let consts = &pred.constants;
        if name == "coll" {
            for i in 0..pts.len() {
                for j in (i + 1)..pts.len() {
                    let (a, b) = (pts[i], pts[j]);
                    if let Some(lid) = self.pair_line[self.pk(a, b)] {
                        let lp: FxHashSet<PointId> =
                            self.lines[lid].points.iter().copied().collect();
                        if pts.iter().all(|p| lp.contains(p)) {
                            return Some(self.lines[lid].fact.into_iter().collect());
                        }
                        return None;
                    }
                }
            }
            None
        } else if ANGLE_PREDS.contains(&name) {
            let a = self.pred_to_angle(name, &pts, consts);
            let (r, deps) = self.angle.simplify_deps(&a);
            r.is_zero().then_some(deps)
        } else if DIST_MUL_PREDS.contains(&name) {
            // Build from *raw* quantities so the reduction traverses (and
            // therefore cites) every row used; `pred_to_dist_mul` pre-simplifies
            // via the cache, which would hide the provenance. The normal form is
            // identical either way (simplification is linear).
            let d = self.pred_to_dist_mul_raw(name, &pts, consts);
            let (r, deps) = self.dmul.simplify_deps(&d);
            r.is_one().then_some(deps)
        } else if name == "distseq" {
            let d = self.pred_to_dist_add(name, &pts, consts);
            let (r, deps) = self.dadd.simplify_deps(&d);
            r.is_zero().then_some(deps)
        } else if name == "cyclic" || name == "cyclic_with_centers" {
            let (centers, points): (&[PointId], &[PointId]) = if name == "cyclic" {
                (&[], &pts[..])
            } else {
                let nc = consts[0].numer_i64().unwrap() as usize;
                (&pts[..nc], &pts[nc..])
            };
            if !self.check_concyclic(points, centers) {
                return None;
            }
            let mut distinct: Vec<PointId> = Vec::new();
            for &p in points {
                if !distinct.iter().any(|&x| self.num_identical(p, x)) {
                    distinct.push(p);
                }
            }
            let cid = self.triple_to_circle[&(distinct[0], distinct[1], distinct[2])];
            Some(self.circles[cid].fact.into_iter().collect())
        } else if name == "overlap" {
            if !self.check_equal_points(pred.points[0], pred.points[1]) {
                return None;
            }
            // Over-approximation: cite every point merge (they are rare).
            Some(
                (0..self.log.facts.len() as FactId)
                    .filter(|&f| {
                        matches!(self.log.facts[f as usize].reason, Reason::PointMerge(_, _))
                    })
                    .collect(),
            )
        } else {
            panic!("unexpected predicate: {name}");
        }
    }

    /// Render the numbered proof for goal dependencies from
    /// [`Self::check_pred_deps`], in the style of the original AlphaGeometry.
    pub fn proof_report(&self, deps: &[FactId], goal_text: &str) -> String {
        self.log.report(deps, goal_text, &self.names)
    }

    /// The numbered derivation lines behind `deps` (see [`ProofLog::step_lines`]).
    pub fn proof_lines(&self, deps: &[FactId]) -> Vec<String> {
        self.log.step_lines(deps, &self.names)
    }

    /// `|ab| / |cd|` for an `rcompute a b c d` goal when the ratio table fixes it
    /// to a constant (prime-log terms only, so square roots of rationals too),
    /// with the facts the reduction used.
    pub fn rcompute_deps(&self, pred: &Predicate) -> Option<(DistMul, Vec<FactId>)> {
        let pts = self.subst_points(pred);
        if pts.len() != 4 || self.num_identical(pts[0], pts[1]) || self.num_identical(pts[2], pts[3]) {
            return None;
        }
        let r = self
            .raw_dist_mul(pts[0], pts[1])
            .div(&self.raw_dist_mul(pts[2], pts[3]));
        let (r, deps) = self.dmul.simplify_deps(&r);
        r.0.terms
            .iter()
            .all(|(v, _)| !self.dmul.core.is_lhs[*v as usize])
            .then_some((r, deps))
    }

    /// The rational value of a determined `rcompute` ratio, if it is rational.
    pub fn rcompute(&self, pred: &Predicate) -> Option<Rat> {
        let (r, _) = self.rcompute_deps(pred)?;
        let (rest, coef) = self.dmul.normalize(&r);
        rest.is_one().then_some(coef)
    }

    /// Compute a determined angle for an `acompute` goal, if any (in half-turns).
    pub fn acompute(&mut self, pred: &Predicate) -> Option<Rat> {
        let pts = self.subst_points(pred);
        let ang = self
            .raw_dir(pts[0], pts[1])
            .sub(&self.raw_dir(pts[2], pts[3]));
        let ang = self.angle.simplify(&ang);
        // Determined iff the only variable present is the angle unit.
        if ang
            .0
            .terms
            .iter()
            .all(|(v, _)| *v == crate::elimination::ANGLE_UNIT)
        {
            Some(ang.0.get(crate::elimination::ANGLE_UNIT))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Deductive closure
// ---------------------------------------------------------------------------

impl Ddar {
    fn arg_max_first<F: Fn(PointId) -> f64>(pts: &[PointId], f: F) -> PointId {
        let mut best = pts[0];
        let mut best_v = f(pts[0]);
        for &p in &pts[1..] {
            let v = f(p);
            if v > best_v {
                best_v = v;
                best = p;
            }
        }
        best
    }

    fn update_cache(&mut self) {
        let active = self.active.clone();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if self.num_identical(a, b) {
                    continue;
                }
                let dist = self.get_dist_mul(a, b);
                let dir = self.get_point_dir(a, b);
                let (ab, ba) = (self.pk(a, b), self.pk(b, a));
                self.dist_mul_cache[ab] = Some(dist.clone());
                self.dist_mul_cache[ba] = Some(dist);
                self.dir_cache[ab] = Some(dir.clone());
                self.dir_cache[ba] = Some(dir);
            }
        }
    }

    /// Run the fixpoint loop until no new fact is derived.
    pub fn deduction_closure(&mut self) {
        self.deduction_closure_until(None);
    }

    /// [`Ddar::deduction_closure`] that gives up between passes once
    /// `deadline` has passed; returns whether the fixpoint was reached.
    pub fn deduction_closure_until(&mut self, deadline: Option<std::time::Instant>) -> bool {
        loop {
            if !self.base_closure(deadline) {
                return false;
            }
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                return false;
            }
            if self.classical_rules() {
                continue;
            }
            if !self.trig_activate_at_fixpoint() {
                return true;
            }
        }
    }

    fn classical_rules(&mut self) -> bool {
        let mut changed = false;
        if self.rule_on(classics::Rule::BisectorConcurrency)
            && self.inputs_changed(classics::Pass::BisectorConcurrency)
        {
            changed |= self.search_bisector_concurrency();
        }
        if self.rule_on(classics::Rule::MenelausCeva)
            && self.inputs_changed(classics::Pass::MenelausCeva)
        {
            if changed {
                self.update_cache();
            }
            changed |= self.search_menelaus_ceva();
        }
        if self.rule_on(classics::Rule::SquaredLengths) {
            if changed {
                self.update_cache();
            }
            changed |= self.search_squared_lengths();
        }
        changed
    }

    fn base_closure(&mut self, deadline: Option<std::time::Instant>) -> bool {
        let mut changed = true;
        while changed {
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                return false;
            }
            self.update_cache();
            changed = false;
            let mut ids: Option<PairIds> = None;

            let c = self.search_similar(&mut ids);
            if c {
                self.update_cache();
                ids = None;
            }
            changed |= c;

            let c = self.search_concyclic(&mut ids);
            if c {
                self.update_cache();
                ids = None;
            }
            changed |= c;

            changed |= self.search_circles();
            if self.merge_points() {
                changed = true;
                ids = None;
            }
            changed |= self.transfer_dist_add_mul();
            changed |= self.transfer_dist_arc_mul();
            changed |= self.search_bisector_theorem(&mut ids);
            drop(ids);
            changed |= self.search_intercept_theorem();
            changed |= self.search_similitude();
            changed |= self.search_radical_axis();
            changed |= self.search_trig();
        }
        true
    }

    /// Centres of similitude (homothety centres) of circle pairs, and the two
    /// classical rules they power — the machinery of tangent-circle problems:
    ///
    /// * **Homothety mapping**: if `Z` is a centre of similitude of circles
    ///   `(O₁)` and `(O₂)`, `P` is on `(O₁)`, `Q` is on `(O₂)`, and `Z, P, Q`
    ///   are collinear with `Q` the homothety *image* of `P` (not the
    ///   antihomologous point), then `O₁P ∥ O₂Q` and `|ZP|:|ZQ| = |ZO₁|:|ZO₂|`.
    /// * **Monge–d'Alembert**: for three circles, the three pairwise centres
    ///   of similitude are collinear whenever the number of *internal* centres
    ///   among them is even (external–external–external, or one external with
    ///   two internal).
    ///
    /// A point `Z` is recognised as a centre of similitude *symbolically* —
    /// `Z` on the centre line and `|ZO₁|:|ZO₂| = r₁:r₂` both derivable — while
    /// the internal/external type and the image-vs-antihomologous pairing,
    /// which the unsigned mod-π algebra cannot distinguish, are read off the
    /// numeric figure (the engine's standard hybrid style, as with circle
    /// detection).
    fn search_similitude(&mut self) -> bool {
        let mut changed = false;
        // Circles with a symbolic centre and a radius representative: the live
        // DB circles, plus the transient "small" (two-point) centred circles
        // from this pass — a circle named by a single `cong` is enough to have
        // a centre of similitude. (centre, radius point, on-points, base fact)
        let mut circs: Vec<(PointId, PointId, Vec<PointId>, Option<FactId>)> = Vec::new();
        for &ci in &self.live_circles {
            let c = &self.circles[ci];
            if let (Some(&o), Some(&a)) = (c.centers.first(), c.points.first()) {
                circs.push((o, a, c.points.clone(), c.fact));
            }
        }
        for c in &self.last_small_circles {
            if let (Some(&o), Some(&a)) = (c.centers.first(), c.points.first()) {
                circs.push((o, a, c.points.clone(), c.fact));
            }
        }
        // Pairwise centres of similitude, keyed by the circle pair.
        // (z, internal?, premises)
        let mut centres: FxHashMap<(usize, usize), Vec<(PointId, bool, Vec<FactId>)>> =
            FxHashMap::default();
        let mut dirs: FxHashMap<(PointId, PointId), Angle> = FxHashMap::default();
        for i in 0..circs.len() {
            for j in (i + 1)..circs.len() {
                let (o1, a1, _, f1) = (circs[i].0, circs[i].1, (), circs[i].3);
                let (o2, a2, _, f2) = (circs[j].0, circs[j].1, (), circs[j].3);
                if self.num_identical(o1, o2) {
                    continue; // concentric: no centre of similitude
                }
                for &z in &self.active.clone() {
                    if self.num_identical(z, o1) || self.num_identical(z, o2) {
                        continue;
                    }
                    // Symbolic: Z on the centre line, |ZO₁|/|ZO₂| = r₁/r₂.
                    let d1 = dirs
                        .entry((z, o1))
                        .or_insert_with(|| self.get_point_dir(z, o1))
                        .clone();
                    let d2 = dirs.entry((z, o2)).or_insert_with(|| self.get_point_dir(z, o2));
                    if d1 != *d2 {
                        continue;
                    }
                    if self.get_dist_ratio(z, o2, z, o1) != self.get_dist_ratio(o2, a2, o1, a1) {
                        continue;
                    }
                    // Numeric type: internal iff Z lies between the centres.
                    let (vz, v1, v2) = (self.coord(z), self.coord(o1), self.coord(o2));
                    let internal = (v1 - vz).dot(v2 - vz) < 0.0;
                    let mut prem: Vec<FactId> = Vec::new();
                    prem.extend(self.deps_of_angle_expr(z, o1, z, o2));
                    prem.extend(self.deps_of_ratio_expr(z, o2, z, o1));
                    prem.extend(self.deps_of_ratio_expr(o2, a2, o1, a1));
                    prem.extend(f1);
                    prem.extend(f2);
                    centres.entry((i, j)).or_default().push((z, internal, prem));
                }
            }
        }

        // (a) Homothety mapping: image pairs get parallel radii + the ratio.
        for (&(i, j), zs) in &centres.clone() {
            let (o1, o2) = (circs[i].0, circs[j].0);
            let pts1 = circs[i].2.clone();
            let pts2 = circs[j].2.clone();
            for (z, internal, prem) in zs {
                let z = *z;
                for &p in &pts1 {
                    for &q in &pts2 {
                        if self.num_identical(p, q)
                            || self.num_identical(z, p)
                            || self.num_identical(z, q)
                            || self.num_identical(o1, p)
                            || self.num_identical(o2, q)
                        {
                            continue;
                        }
                        // Symbolic: Z, P, Q collinear.
                        if self.get_point_dir(z, p) != self.get_point_dir(z, q) {
                            continue;
                        }
                        // Numeric: Q is the image of P — radii parallel, with the
                        // orientation the centre type dictates (same direction for
                        // an external centre, opposite for an internal one).
                        let (vz, vp, vo1) = (self.coord(z), self.coord(p), self.coord(o1));
                        let u = vp - vz;
                        let foot = vz + u * ((vo1 - vz).dot(u) / u.dot(u));
                        let second = foot * 2.0 - vp;
                        if (second - vp).norm() < 1e-6 * (vp - vo1).norm() {
                            continue;
                        }
                        let rp = vp - vo1;
                        let rq = self.coord(q) - self.coord(o2);
                        let cross = rp.x * rq.y - rp.y * rq.x;
                        if cross.abs() > 1e-7 * rp.norm() * rq.norm() {
                            continue; // antihomologous pairing
                        }
                        if (rp.dot(rq) > 0.0) == *internal {
                            continue; // wrong orientation for this centre type
                        }
                        // Skip when both conclusions are already known.
                        let para_known = self.get_point_dir(o1, p) == self.get_point_dir(o2, q);
                        let ratio_known = self.get_dist_ratio(z, q, z, p)
                            == self.get_dist_ratio(z, o2, z, o1);
                        if para_known && ratio_known {
                            continue;
                        }
                        let mut prem2 = prem.clone();
                        prem2.extend(self.deps_of_angle_expr(z, p, z, q));
                        let fact = self.log.add(
                            Reason::Theorem(
                                "homothety at a centre of similitude",
                                vec![z, p, q, o1, o2],
                            ),
                            prem2,
                        );
                        let rel = self.raw_dir(o1, p).sub(&self.raw_dir(o2, q));
                        changed |= self.angle.force_zero(&rel, Some(fact));
                        let rel = self
                            .raw_dist_mul(z, p)
                            .div(&self.raw_dist_mul(z, q))
                            .div(&self.raw_dist_mul(z, o1))
                            .mul(&self.raw_dist_mul(z, o2));
                        changed |= self.dmul.force_one(&rel, Some(fact));
                    }
                }
            }
        }

        // (b) Monge–d'Alembert on circle triples.
        for i in 0..circs.len() {
            for j in (i + 1)..circs.len() {
                for k in (j + 1)..circs.len() {
                    let (Some(zij), Some(zik), Some(zjk)) = (
                        centres.get(&(i, j)),
                        centres.get(&(i, k)),
                        centres.get(&(j, k)),
                    ) else {
                        continue;
                    };
                    for (z1, t1, p1) in zij {
                        for (z2, t2, p2) in zik {
                            for (z3, t3, p3) in zjk {
                                if !(*t1 as u8 + *t2 as u8 + *t3 as u8).is_multiple_of(2) {
                                    continue; // odd internal count: not collinear
                                }
                                let (z1, z2, z3) = (*z1, *z2, *z3);
                                if self.num_identical(z1, z2)
                                    || self.num_identical(z1, z3)
                                    || self.num_identical(z2, z3)
                                {
                                    continue;
                                }
                                if self.get_point_dir(z1, z2) == self.get_point_dir(z1, z3) {
                                    continue; // collinearity already known
                                }
                                let mut prem = p1.clone();
                                prem.extend_from_slice(p2);
                                prem.extend_from_slice(p3);
                                let fact = self.log.add(
                                    Reason::Theorem("Monge–d'Alembert", vec![z1, z2, z3]),
                                    prem,
                                );
                                let rel =
                                    self.raw_dir(z1, z2).sub(&self.raw_dir(z1, z3));
                                changed |= self.angle.force_zero(&rel, Some(fact));
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    /// The angle bisector theorem, both directions (rules r11/r12 of the
    /// original AlphaGeometry, not derivable by the closure without auxiliary
    /// constructions):
    ///
    /// For `x` on line `bc` and apex `a`:
    /// * if `ax` bisects the angle at `a` (`2·dir(ax) ≡ dir(ab) + dir(ac)`),
    ///   then `|xb| / |xc| = |ab| / |ac|`;
    /// * conversely, if the ratio holds, `ax` bisects the angle.
    ///
    /// In directed angles mod π the bisector relation is satisfied by both the
    /// internal and the external bisector, and the *unsigned* ratio conclusion
    /// holds in both cases (internal/external division), so both directions
    /// are sound without a case split.

    /// **Radical axis** — the classical DDAR blind spot. If two known circles
    /// share two points U and V, any point X whose powers to the two circles
    /// are *symbolically* equal lies on line UV. The power of X is witnessed
    /// by a chord of each circle through X — D₁, D₂ on the first circle with
    /// X, D₁, D₂ collinear (likewise E₁, E₂ on the second) — together with the
    /// product identity |XD₁|·|XD₂| = |XE₁|·|XE₂|, which the multiplicative
    /// distance table already derives via similar-triangle chains. Radical-axis
    /// membership is what turns pairwise power equalities into concurrency and
    /// coaxality (e.g. the radical centre step of IMO 2023 P6).
    ///
    /// Soundness: unsigned products equal signed powers only up to the
    /// inside/outside branch. The branch is read from the figure's
    /// configuration — X between both chord ends (inside both circles) or
    /// neither — and X, U, V being numerically collinear is a further guard.
    fn search_radical_axis(&mut self) -> bool {
        let mut changed = false;
        let dbg = std::env::var_os("RADAX_DEBUG").is_some_and(|v| !v.is_empty());
        let circs: Vec<(Vec<PointId>, Option<FactId>)> = self
            .live_circles
            .iter()
            .map(|&ci| (self.circles[ci].points.clone(), self.circles[ci].fact))
            .collect();
        if dbg {
            eprintln!(
                "[radax] pass: {} circles: {:?}",
                circs.len(),
                circs
                    .iter()
                    .map(|c| c.0.iter().map(|&p| self.name(p).to_string()).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            );
        }
        for i in 0..circs.len() {
            for j in (i + 1)..circs.len() {
                // Two numerically distinct points shared by both circles.
                let mut uv: Vec<PointId> = Vec::new();
                for &p in &circs[i].0 {
                    if circs[j].0.contains(&p) && !uv.iter().any(|&q| self.num_identical(p, q)) {
                        uv.push(p);
                        if uv.len() == 2 {
                            break;
                        }
                    }
                }
                if uv.len() < 2 {
                    continue;
                }
                let (u, v) = (uv[0], uv[1]);
                for &x in &self.active.clone() {
                    if x == u || x == v || self.num_identical(x, u) || self.num_identical(x, v) {
                        continue;
                    }
                    // Branch guard: X must actually lie on UV in the figure.
                    // The figure may come from a numeric global solve (assume
                    // hypotheses), so use the candidate-grade tolerance rather
                    // than the exact-arithmetic ATOM.
                    if NumLine::through(self.coord(u), self.coord(v))
                        .distance(self.coord(x))
                        > 1e-6
                    {
                        continue;
                    }
                    // Already known collinear: nothing to add.
                    if self.get_point_dir(x, u) == self.get_point_dir(x, v) {
                        continue;
                    }
                    // A symbolic chord of the first circle through X.
                    let mut chord1: Option<(PointId, PointId)> = None;
                    'c1: for (ai, &d1) in circs[i].0.iter().enumerate() {
                        for &d2 in &circs[i].0[ai + 1..] {
                            if self.num_identical(d1, d2)
                                || self.num_identical(x, d1)
                                || self.num_identical(x, d2)
                            {
                                continue;
                            }
                            if self.get_point_dir(x, d1) == self.get_point_dir(x, d2) {
                                chord1 = Some((d1, d2));
                                break 'c1;
                            }
                        }
                    }
                    let Some((d1, d2)) = chord1 else { continue };
                    // A chord of the second circle through X whose product
                    // matches: |XD₁|·|XD₂| = |XE₁|·|XE₂| in the dmul table.
                    let mut chord2: Option<(PointId, PointId)> = None;
                    'c2: for (bi, &e1) in circs[j].0.iter().enumerate() {
                        for &e2 in &circs[j].0[bi + 1..] {
                            if self.num_identical(e1, e2)
                                || self.num_identical(x, e1)
                                || self.num_identical(x, e2)
                            {
                                continue;
                            }
                            if self.get_point_dir(x, e1) != self.get_point_dir(x, e2) {
                                continue;
                            }
                            if self.get_dist_ratio(x, d1, x, e1)
                                == self.get_dist_ratio(x, e2, x, d2)
                            {
                                chord2 = Some((e1, e2));
                                break 'c2;
                            }
                        }
                    }
                    let Some((e1, e2)) = chord2 else { continue };
                    // Branch: |XD₁|·|XD₂| = |XE₁|·|XE₂| equates *unsigned*
                    // powers. They are equal as signed powers — X on the
                    // radical axis — only if X is inside both circles or
                    // outside both, i.e. between both chord ends or neither.
                    let inside = |p: PointId, q: PointId| {
                        (self.coord(p) - self.coord(x)).dot(self.coord(q) - self.coord(x)) < 0.0
                    };
                    if inside(d1, d2) != inside(e1, e2) {
                        continue;
                    }
                    let mut prem: Vec<FactId> = Vec::new();
                    prem.extend(self.deps_of_angle_expr(x, d1, x, d2));
                    prem.extend(self.deps_of_angle_expr(x, e1, x, e2));
                    prem.extend(self.deps_of_ratio_expr(x, d1, x, e1));
                    prem.extend(self.deps_of_ratio_expr(x, e2, x, d2));
                    prem.extend(circs[i].1);
                    prem.extend(circs[j].1);
                    if dbg {
                        eprintln!(
                            "[radax] FIRE: coll({}, {}, {})",
                            self.name(x),
                            self.name(u),
                            self.name(v)
                        );
                    }
                    let fact = self
                        .log
                        .add(Reason::Theorem("radical axis", vec![x, u, v]), prem);
                    self.force_collinear(&[x, u, v], vec![fact]);
                    changed = true;
                }
            }
        }
        changed
    }

    fn search_bisector_theorem(&mut self, ids: &mut Option<PairIds>) -> bool {
        let mut changed = false;
        let ids = ids.get_or_insert_with(|| self.pair_ids());
        for lid in self.live_lines.clone() {
            let pts = self.lines[lid].points.clone();
            if pts.len() < 3 {
                continue;
            }
            let line_fact = self.lines[lid].fact;
            let on_line: FxHashSet<PointId> = pts.iter().copied().collect();
            for bi in 0..pts.len() {
                for ci in (bi + 1)..pts.len() {
                    let (b, c) = (pts[bi], pts[ci]);
                    if self.num_identical(b, c) {
                        continue;
                    }
                    for &x in &pts {
                        if x == b || x == c || self.num_identical(x, b) || self.num_identical(x, c)
                        {
                            continue;
                        }
                        for &a in &self.active.clone() {
                            if on_line.contains(&a)
                                || self.num_identical(a, b)
                                || self.num_identical(a, c)
                                || self.num_identical(a, x)
                            {
                                continue;
                            }
                            // Bisector relation: dir(ax)-dir(ab) == dir(ac)-dir(ax).
                            let angle_holds = ids.ang_eq((a, b, x), (a, x, c));
                            let ratio_holds = ids.rat_eq((x, c, b), (a, c, b));
                            if angle_holds == ratio_holds {
                                continue; // nothing new in either direction
                            }
                            let mut prem: Vec<FactId> = line_fact.into_iter().collect();
                            if angle_holds {
                                // r12: bisector ⇒ ratio.
                                prem.extend(self.deps_of_angle_expr(a, b, a, x));
                                prem.extend(self.deps_of_angle_expr(a, x, a, c));
                                let fact = self.log.add(
                                    Reason::Theorem("angle bisector theorem", vec![a, x, b, c]),
                                    prem,
                                );
                                let rel = self
                                    .raw_dist_mul(x, b)
                                    .div(&self.raw_dist_mul(x, c))
                                    .div(&self.raw_dist_mul(a, b))
                                    .mul(&self.raw_dist_mul(a, c));
                                changed |= self.dmul.force_one(&rel, Some(fact));
                            } else {
                                // r11: ratio ⇒ bisector.
                                prem.extend(self.deps_of_ratio_expr(x, c, x, b));
                                prem.extend(self.deps_of_ratio_expr(a, c, a, b));
                                let fact = self.log.add(
                                    Reason::Theorem(
                                        "angle bisector theorem (converse)",
                                        vec![a, x, b, c],
                                    ),
                                    prem,
                                );
                                let rel = self
                                    .raw_dir(a, b)
                                    .add(&self.raw_dir(a, c))
                                    .sub(&self.raw_dir(a, x))
                                    .sub(&self.raw_dir(a, x));
                                changed |= self.angle.force_zero(&rel, Some(fact));
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    /// The intercept (Thales) theorem: three parallel "rungs" across two
    /// transversal lines cut off proportional segments (rule r42 of the
    /// original AlphaGeometry). Needed when the transversals' intersection is
    /// not a point of the figure — otherwise similar triangles cover it.
    fn search_intercept_theorem(&mut self) -> bool {
        let mut changed = false;
        let live = self.live_lines.clone();
        for i in 0..live.len() {
            for j in (i + 1)..live.len() {
                let (l1, l2) = (live[i], live[j]);
                let pts1 = self.lines[l1].points.clone();
                let pts2 = self.lines[l2].points.clone();
                if pts1.len() < 3 || pts2.len() < 3 {
                    continue;
                }
                #[cfg(feature = "trace-rules")]
                eprintln!(
                    "[intercept] lines {:?} x {:?}",
                    pts1.iter().map(|&p| self.name(p)).collect::<Vec<_>>(),
                    pts2.iter().map(|&p| self.name(p)).collect::<Vec<_>>()
                );
                // Bucket rungs (p ∈ L1, q ∈ L2) by direction normal form.
                let mut buckets: FxHashMap<&Angle, Vec<(PointId, PointId)>> = FxHashMap::default();
                for &p in &pts1 {
                    for &q in &pts2 {
                        if p == q || self.num_identical(p, q) {
                            continue;
                        }
                        // Skip rungs parallel to either transversal (the affine
                        // projection along the rung direction is undefined).
                        let v = self.coord(q) - self.coord(p);
                        let d1 = self.lines[l1].value.n;
                        let d2 = self.lines[l2].value.n;
                        if (v.x * d1.x + v.y * d1.y).abs() < 1e-9
                            || (v.x * d2.x + v.y * d2.y).abs() < 1e-9
                        {
                            continue;
                        }
                        buckets
                            .entry(self.cached_dir(p, q))
                            .or_default()
                            .push((p, q));
                    }
                }
                let buckets: Vec<Vec<(PointId, PointId)>> = buckets.into_values().collect();
                let line_facts: Vec<FactId> = self.lines[l1]
                    .fact
                    .into_iter()
                    .chain(self.lines[l2].fact)
                    .collect();
                for rungs in buckets {
                    #[cfg(feature = "trace-rules")]
                    eprintln!(
                        "[intercept]   bucket: {:?}",
                        rungs
                            .iter()
                            .map(|&(p, q)| format!("{}-{}", self.name(p), self.name(q)))
                            .collect::<Vec<_>>()
                    );
                    if rungs.len() < 3 {
                        continue;
                    }
                    for t in 0..rungs.len() {
                        for u in (t + 1)..rungs.len() {
                            for w in (u + 1)..rungs.len() {
                                let ((a, b), (m, n), (d, cc)) = (rungs[t], rungs[u], rungs[w]);
                                // Both transversal sides need pairwise-distinct
                                // points: identical pairs have no distance
                                // variable at all.
                                if self.num_identical(a, m)
                                    || self.num_identical(m, d)
                                    || self.num_identical(a, d)
                                    || self.num_identical(b, n)
                                    || self.num_identical(n, cc)
                                    || self.num_identical(b, cc)
                                {
                                    continue;
                                }
                                // The affine projection along the rung direction
                                // maps a→b, m→n, d→c and preserves ratios, giving
                                // two independent product relations (the third,
                                // |md|/|nc| = |ad|/|bc|, follows linearly):
                                //   |am|/|bn| = |md|/|nc|   and   |am|/|bn| = |ad|/|bc|
                                let base = self.raw_dist_mul(a, m).div(&self.raw_dist_mul(b, n));
                                let rel1 = base
                                    .clone()
                                    .div(&self.raw_dist_mul(m, d))
                                    .mul(&self.raw_dist_mul(n, cc));
                                let rel2 = base
                                    .div(&self.raw_dist_mul(a, d))
                                    .mul(&self.raw_dist_mul(b, cc));
                                // Only force when new (cheap normal-form check).
                                let new1 = !self.dmul.simplify(&rel1).is_one();
                                let new2 = !self.dmul.simplify(&rel2).is_one();
                                if !new1 && !new2 {
                                    continue;
                                }
                                let mut prem = line_facts.clone();
                                prem.extend(self.deps_of_angle_expr(a, b, m, n));
                                prem.extend(self.deps_of_angle_expr(m, n, d, cc));
                                let fact = self.log.add(
                                    Reason::Theorem(
                                        "intercept theorem (parallel rungs)",
                                        vec![a, b, m, n, d, cc],
                                    ),
                                    prem,
                                );
                                if new1 {
                                    changed |= self.dmul.force_one(&rel1, Some(fact));
                                }
                                if new2 {
                                    changed |= self.dmul.force_one(&rel2, Some(fact));
                                }
                            }
                        }
                    }
                }
            }
        }
        changed
    }

    fn force_similar(&mut self, t1: Triple, t2: Triple) -> bool {
        if self.known_similar.contains(&(t1, t2)) {
            return false;
        }
        let (a, b, c) = t1;
        let (x, y, z) = t2;
        let syms = [
            ((a, b, c), (x, y, z)),
            ((a, c, b), (x, z, y)),
            ((b, a, c), (y, x, z)),
            ((c, a, b), (z, x, y)),
            ((b, c, a), (y, z, x)),
            ((c, b, a), (z, y, x)),
            ((x, y, z), (a, b, c)),
            ((x, z, y), (a, c, b)),
            ((y, x, z), (b, a, c)),
            ((z, x, y), (c, a, b)),
            ((y, z, x), (b, c, a)),
            ((z, y, x), (c, b, a)),
        ];
        for k in syms {
            self.known_similar.insert(k);
        }

        // Premises: the facts that made the two triangles' angle/ratio normal
        // forms coincide in the search.
        let mut prem: Vec<FactId> = Vec::new();
        for (p, q, r, s) in [(a, b, a, c), (c, b, c, a), (x, y, x, z), (z, y, z, x)] {
            prem.extend(self.deps_of_angle_expr(p, q, r, s));
            prem.extend(self.deps_of_ratio_expr(p, q, r, s));
        }
        let fact = self.log.add(Reason::SimilarTriangles(t1, t2), prem);

        let t1_rat1 = self.get_dist_ratio(a, b, a, c);
        let t1_ang1 = self.get_point_angle(a, b, a, c);
        let t1_rat2 = self.get_dist_ratio(a, b, b, c);
        let t1_ang2 = self.get_point_angle(a, b, b, c);
        let t2_rat1 = self.get_dist_ratio(x, y, x, z);
        let mut t2_ang1 = self.get_point_angle(x, y, x, z);
        let t2_rat2 = self.get_dist_ratio(x, y, y, z);
        let mut t2_ang2 = self.get_point_angle(x, y, y, z);

        if orientation(self.coord(a), self.coord(b), self.coord(c))
            != orientation(self.coord(x), self.coord(y), self.coord(z))
        {
            t2_ang1 = t2_ang1.neg();
            t2_ang2 = t2_ang2.neg();
        }

        let mut changed = false;
        changed |= self.angle.force_zero(&t1_ang1.sub(&t2_ang1), Some(fact));
        changed |= self.angle.force_zero(&t1_ang2.sub(&t2_ang2), Some(fact));
        changed |= self.dmul.force_one(&t1_rat1.div(&t2_rat1), Some(fact));
        changed |= self.dmul.force_one(&t1_rat2.div(&t2_rat2), Some(fact));
        changed
    }

    fn search_similar(&mut self, ids: &mut Option<PairIds>) -> bool {
        let active = self.active.clone();

        let mut triangles: Vec<(Triple, i32)> = Vec::new();
        for &a in &active {
            for &b in &active {
                if self.num_identical(a, b) {
                    continue;
                }
                let k = self.pk(a, b);
                let encountered = self.angle.was_encountered(self.pair_dir[k].as_ref().unwrap())
                    || self
                        .dmul
                        .was_encountered(self.pair_dist_mul[k].as_ref().unwrap());
                if !encountered {
                    continue;
                }
                for &c in &active {
                    if self.num_identical(a, c) || self.num_identical(b, c) {
                        continue;
                    }
                    let orient = orientation(self.coord(a), self.coord(b), self.coord(c));
                    if orient == 0 || self.numerically_flat(a, b, c) {
                        continue;
                    }
                    triangles.push(((a, b, c), orient));
                }
            }
        }
        if triangles.is_empty() {
            return false;
        }

        let ids = ids.get_or_insert_with(|| self.pair_ids());
        let cap = triangles.len();
        let mut sss: FpBuckets<(u64, u64)> = FpBuckets::with_capacity(cap);
        let mut aa: FpBuckets<(u64, u64)> = FpBuckets::with_capacity(2 * cap);
        let mut sas: FpBuckets<(u64, u64, i32)> = FpBuckets::with_capacity(2 * cap);
        let mut ssa: FpBuckets<(u64, u64, i32)> = FpBuckets::with_capacity(2 * cap);
        let n = self.n;
        let mut dist = vec![0.0f64; n * n];
        for &a in &active {
            for &b in &active {
                dist[a as usize * n + b as usize] = distance(self.coord(a), self.coord(b));
            }
        }
        let mut similar_pairs: Vec<(Triple, Triple)> = Vec::new();

        for &((a, b, c), orient) in &triangles {
            let t = (a, b, c);
            let (r1, r2) = (ids.rat_key(a, b, c), ids.rat_key(c, b, a));
            let (g1, g2) = (ids.ang_key(a, b, c), ids.ang_key(c, b, a));

            if let Some(t0) = find_or_put(&mut sss, ids, (r1, r2), (t, 0), sss_exact) {
                similar_pairs.push((t0, t));
            }

            if let Some(t0) = find_or_put(&mut aa, ids, (g1, g2), (t, 0), aa_exact) {
                similar_pairs.push((t0, t));
            } else {
                let neg_key = (ids.ang_key(a, c, b), ids.ang_key(c, a, b));
                put(&mut aa, ids, neg_key, (t, 1), aa_exact);
            }

            if let Some(t0) = find_or_put(&mut sas, ids, (g1, r1, orient), (t, 0), sas_exact) {
                similar_pairs.push((t0, t));
            } else {
                let neg_key = (ids.ang_key(a, c, b), r1, -orient);
                put(&mut sas, ids, neg_key, (t, 1), sas_exact);
            }

            let candidates = [(a, b, c, g1, r2, orient), (c, b, a, g2, r1, -orient)];
            for (a1, b1, c1, ang, rat, cur_orient) in candidates {
                let (a1u, b1u, c1u) = (a1 as usize, b1 as usize, c1 as usize);
                if dist[c1u * n + b1u] - dist[c1u * n + a1u] > ATOM {
                    let u = (a1, b1, c1);
                    if !ids.memo.mark_ssa((a1u * n + b1u) * n + c1u) {
                        continue;
                    }
                    let key = (ang, rat, cur_orient);
                    if let Some(t0) = find_or_put(&mut ssa, ids, key, (u, 0), ssa_exact) {
                        similar_pairs.push((t0, u));
                    } else {
                        let neg_key = (ids.ang_key(a1, c1, b1), rat, -cur_orient);
                        put(&mut ssa, ids, neg_key, (u, 1), ssa_exact);
                    }
                }
            }
        }

        let mut changed = false;
        for (t1, t2) in similar_pairs {
            changed = self.force_similar(t1, t2) || changed;
        }
        changed
    }

    fn search_concyclic(&mut self, ids: &mut Option<PairIds>) -> bool {
        let active = self.active.clone();
        let ids = ids.get_or_insert_with(|| self.pair_ids());
        let mut changed = false;
        let mut row_keys: Vec<(PointId, Option<u64>, Option<u64>)> = Vec::new();
        let mut counts: FxHashMap<u64, (u32, u32)> = FxHashMap::default();
        let mut row: Vec<(PointId, Option<u32>, Option<u32>)> = Vec::new();
        let mut on_line: Vec<PointId> = Vec::new();
        for &a in &active {
            for &b in &active {
                row_keys.clear();
                on_line.clear();
                let ab_identical = self.num_identical(a, b);
                for &c in &active {
                    if self.num_identical(a, c) || self.num_identical(b, c) {
                        continue;
                    }
                    let key = ids.ang_key(c, a, b);
                    if ids.ang_key_is_zero(key, c, a, b) {
                        on_line.push(c);
                    }
                    if ab_identical {
                        continue;
                    }
                    let inscribed =
                        (!collinear(self.coord(a), self.coord(b), self.coord(c))).then_some(key);
                    let central = (ids.dm_id[self.pk(c, a)] == ids.dm_id[self.pk(c, b)])
                        .then(|| ids.half_turn_key(a, c, b));
                    row_keys.push((c, inscribed, central));
                }
                for &c in &on_line {
                    if self.line_holds(a, b, c) {
                        continue;
                    }
                    changed = self
                        .force_collinear_with(&[a, b, c], |s| s.deps_of_angle_expr(c, a, c, b))
                        || changed;
                }
                counts.clear();
                let mut may_fire = false;
                for &(_, inscribed, central) in &row_keys {
                    if let Some(k) = inscribed {
                        let e = counts.entry(k).or_default();
                        e.0 += 1;
                        may_fire |= e.0 >= 2 || e.1 >= 1;
                    }
                    if let Some(k) = central {
                        let e = counts.entry(k).or_default();
                        e.1 += 1;
                        may_fire |= e.0 >= 1;
                    }
                }
                if !may_fire || self.row_already_concyclic(a, b, &row_keys, &counts) {
                    continue;
                }
                row.clear();
                for &(c, inscribed, central) in &row_keys {
                    let inscribed = inscribed.map(|_| ids.ang(c, a, b));
                    let central = central.map(|_| {
                        let half = ids.ang(a, c, b);
                        ids.dir.plus_half_turn(half)
                    });
                    row.push((c, inscribed, central));
                }
                let groups: Vec<(Vec<PointId>, Vec<PointId>)> = {
                    let mut ang_map: FxHashMap<&LinComb, (Vec<PointId>, Vec<PointId>)> =
                        FxHashMap::default();
                    for &(c, inscribed, central) in &row {
                        if let Some(k) = inscribed {
                            ang_map.entry(ids.dir.item(k)).or_default().0.push(c);
                        }
                        if let Some(k) = central {
                            ang_map.entry(ids.dir.item(k)).or_default().1.push(c);
                        }
                    }
                    ang_map.into_values().collect()
                };
                for (points_list, centers) in groups {
                    if points_list.len() >= 2 || (!centers.is_empty() && !points_list.is_empty()) {
                        let mut pts = vec![a, b];
                        pts.extend_from_slice(&points_list);
                        let premises = |s: &Ddar| {
                            let mut prem: Vec<FactId> = Vec::new();
                            for &c in &points_list {
                                prem.extend(s.deps_of_angle_expr(c, a, c, b));
                            }
                            for &c in &centers {
                                prem.extend(s.deps_of_ratio_expr(c, a, c, b));
                                prem.extend(s.deps_of_angle_expr(a, c, a, b));
                            }
                            prem
                        };
                        changed = self.force_concyclic_with(&pts, &centers, premises) || changed;
                    }
                }
            }
        }
        changed
    }

    fn line_holds(&self, a: PointId, b: PointId, c: PointId) -> bool {
        let Some(lid) = self.pair_line[self.pk(a, b)] else {
            return false;
        };
        let line = &self.lines[lid];
        line.points.contains(&c)
            && [a, b, c]
                .iter()
                .all(|&p| line.value.distance(self.coord(p)) < ATOM)
    }

    fn row_already_concyclic(
        &self,
        a: PointId,
        b: PointId,
        row: &[(PointId, Option<u64>, Option<u64>)],
        counts: &FxHashMap<u64, (u32, u32)>,
    ) -> bool {
        counts.iter().all(|(&k, &(inscribed, central))| {
            if !(inscribed >= 2 || (inscribed >= 1 && central >= 1)) {
                return true;
            }
            let mut on = row.iter().filter(|r| r.1 == Some(k)).map(|r| r.0);
            let Some(first) = on.next() else {
                return false;
            };
            let Some(&cid) = self.triple_to_circle.get(&(a, b, first)) else {
                return false;
            };
            let circle = &self.circles[cid];
            on.all(|c| circle.points.contains(&c))
                && row
                    .iter()
                    .filter(|r| r.2 == Some(k))
                    .all(|r| circle.centers.contains(&r.0))
        })
    }

    fn search_circles(&mut self) -> bool {
        let active = self.active.clone();
        let mut changed = false;
        self.last_small_circles.clear();
        for &a in &active {
            let mut dist_to_points: FxHashMap<DistMul, Vec<PointId>> = FxHashMap::default();
            for &b in &active {
                if self.num_identical(a, b) {
                    continue;
                }
                let dist = self.get_dist_mul(a, b);
                dist_to_points.entry(dist).or_default().push(b);
            }
            let groups: Vec<Vec<PointId>> = dist_to_points
                .into_values()
                .filter(|v| v.len() > 1)
                .collect();
            for pts in groups {
                let mut distinct: Vec<PointId> = Vec::new();
                for &p in &pts {
                    if distinct.iter().any(|&x| self.num_identical(p, x)) {
                        continue;
                    }
                    distinct.push(p);
                }
                if distinct.len() >= 3 {
                    // Premises: the equal distances from `a` to every point.
                    let mut prem: Vec<FactId> = Vec::new();
                    for &p in &pts[1..] {
                        prem.extend(self.deps_of_ratio_expr(a, pts[0], a, p));
                    }
                    let fq = self.log.add(Reason::EqualRadius(a, pts.clone()), prem);
                    changed = self.force_concyclic(&pts, &[a], vec![fq]) || changed;
                } else {
                    let center = self.coord(a);
                    let r = distance(center, self.coord(pts[0]));
                    self.last_small_circles.push(FormalCircle {
                        defining_points: None,
                        points: pts.clone(),
                        centers: vec![a],
                        value: NumCircle { center, r },
                        fact: None,
                    });
                }
            }
        }
        changed
    }

    fn merge_points(&mut self) -> bool {
        #[derive(Clone, Copy)]
        enum ObjRef {
            Line(NumLine, Option<FactId>),
            Circle(Vec2, Option<FactId>),
            /// Transient equal-distance circle; premises are recomputed lazily
            /// from the center's equal distances if a merge actually fires.
            SmallCircle(Vec2, PointId),
        }
        let order = |a: PointId, b: PointId| if a <= b { (a, b) } else { (b, a) };

        let active = self.active.clone();
        let mut same_pairs: FxHashMap<(PointId, PointId), Vec<ObjRef>> = FxHashMap::default();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if self.num_identical(a, b) {
                    same_pairs.insert(order(a, b), Vec::new());
                }
            }
        }
        if same_pairs.is_empty() {
            return false;
        }

        // Collect the objects passing through each equal pair.
        let record = |same_pairs: &mut FxHashMap<(PointId, PointId), Vec<ObjRef>>,
                      pts: &[PointId],
                      obj: ObjRef| {
            for ii in 0..pts.len() {
                for jj in (ii + 1)..pts.len() {
                    let key = order(pts[ii], pts[jj]);
                    if let Some(v) = same_pairs.get_mut(&key) {
                        v.push(obj);
                    }
                }
            }
        };
        for &lid in &self.live_lines.clone() {
            let pts = self.lines[lid].points.clone();
            let val = self.lines[lid].value;
            let fact = self.lines[lid].fact;
            record(&mut same_pairs, &pts, ObjRef::Line(val, fact));
        }
        for k in 0..self.last_small_circles.len() {
            let pts = self.last_small_circles[k].points.clone();
            let center = self.last_small_circles[k].value.center;
            let center_id = self.last_small_circles[k].centers[0];
            record(
                &mut same_pairs,
                &pts,
                ObjRef::SmallCircle(center, center_id),
            );
        }
        for &cid in &self.live_circles.clone() {
            let pts = self.circles[cid].points.clone();
            let center = self.circles[cid].value.center;
            let fact = self.circles[cid].fact;
            record(&mut same_pairs, &pts, ObjRef::Circle(center, fact));
        }

        // Merge multiple centers of the same circle (does not set `changed`).
        for cid in self.live_circles.clone() {
            let centers = self.circles[cid].centers.clone();
            if centers.len() > 1 {
                let prem: Vec<FactId> = self.circles[cid].fact.into_iter().collect();
                for &center in &centers[1..] {
                    self.force_equal_points(centers[0], center, prem.clone());
                }
            }
        }

        // If two non-tangent objects pass through an equal pair, merge it.
        let mut changed = false;
        for (key, objs) in &same_pairs {
            if objs.len() <= 1 {
                continue;
            }
            let (a, b) = *key;
            let a_val = self.coord(a);
            let dirs: Vec<f64> = objs
                .iter()
                .map(|obj| match obj {
                    ObjRef::Circle(center, _) | ObjRef::SmallCircle(center, _) => {
                        direction_of(a_val - *center) + 0.5
                    }
                    ObjRef::Line(l, _) => l.direction(),
                })
                .collect();
            let d0 = dirs[0];
            for &d1 in &dirs[1..] {
                if ((d0 - d1 + 0.5).rem_euclid(1.0) - 0.5).powi(2) >= ATOM {
                    // Premises: the facts that put both points on each object.
                    let mut prem: Vec<FactId> = Vec::new();
                    for obj in objs {
                        match obj {
                            ObjRef::Line(_, f) | ObjRef::Circle(_, f) => prem.extend(*f),
                            ObjRef::SmallCircle(_, center_id) => {
                                prem.extend(self.deps_of_ratio_expr(*center_id, a, *center_id, b));
                            }
                        }
                    }
                    self.force_equal_points(a, b, prem);
                    changed = true;
                    break;
                }
            }
        }

        if changed {
            self.update_cache();
        }
        changed
    }

    /// Facts underlying both representations of the pair's distance.
    fn deps_of_pair_dists(&self, a: PointId, b: PointId) -> Vec<FactId> {
        let mut d = self.dmul.simplify_deps(&self.raw_dist_mul(a, b)).1;
        d.extend(self.dadd.simplify_deps(&self.raw_dist_add(a, b)).1);
        d
    }

    fn transfer_dist_add_mul(&mut self) -> bool {
        type Pair = (PointId, PointId);
        let active = self.active.clone();
        let mut changed = false;
        let mut mul_to_add: FxHashMap<DistMul, (DistAdd, Pair)> = FxHashMap::default();
        let mut add_to_mul: FxHashMap<DistAdd, (DistMul, Pair)> = FxHashMap::default();
        for i in 0..active.len() {
            for j in (i + 1)..active.len() {
                let (a, b) = (active[i], active[j]);
                if self.num_identical(a, b) {
                    continue;
                }
                let mul = self.get_dist_mul(a, b);
                let add = self.get_dist_add(a, b);
                let (mul_n, mul_coef) = self.dmul.normalize(&mul);
                let (add_n, add_coef) = self.dadd.normalize(&add);
                let mul1 = self.dmul.div_const(&mul, &add_coef);
                let add1 = add.div_scalar(&mul_coef);

                debug_assert!(
                    (self.dadd.value_of(&add_n) - self.dmul.value_of(&mul1)).powi(2) < ATOM
                );
                debug_assert!(
                    (self.dmul.value_of(&mul_n) - self.dadd.value_of(&add1)).powi(2) < ATOM
                );

                if let Some((add2, pair2)) = mul_to_add.get(&mul_n).cloned() {
                    let mut prem = self.deps_of_pair_dists(a, b);
                    prem.extend(self.deps_of_pair_dists(pair2.0, pair2.1));
                    let fact = self.log.add(Reason::TransferAddMul(pair2, (a, b)), prem);
                    changed = self.dadd.force_zero(&add2.sub(&add1), Some(fact)) || changed;
                } else {
                    mul_to_add.insert(mul_n, (add1.clone(), (a, b)));
                }
                if let Some((mul2, pair2)) = add_to_mul.get(&add_n).cloned() {
                    let mut prem = self.deps_of_pair_dists(a, b);
                    prem.extend(self.deps_of_pair_dists(pair2.0, pair2.1));
                    let fact = self.log.add(Reason::TransferAddMul(pair2, (a, b)), prem);
                    changed = self.dmul.force_one(&mul2.div(&mul1), Some(fact)) || changed;
                } else {
                    add_to_mul.insert(add_n, (mul1.clone(), (a, b)));
                }
            }
        }
        changed
    }

    fn transfer_dist_arc_mul(&mut self) -> bool {
        type Pair = (PointId, PointId);
        let mut changed = false;
        for cid in self.live_circles.clone() {
            let circle = self.circles[cid].clone();
            if circle.points.len() <= 3 {
                continue;
            }
            let mut dist_to_src: FxHashMap<DistMul, (Angle, DistMul, Pair)> = FxHashMap::default();
            let mut arc_to_src: FxHashMap<Angle, (DistMul, Angle, Pair)> = FxHashMap::default();
            for &a in &circle.points {
                for &b in &circle.points {
                    if !self.centre_strictly_left(a, b, &circle.value) {
                        continue;
                    }
                    let (arc, _) = self.get_arc(&circle, a, b);
                    let (arc_val, arc_deps) = self.angle.simplify_deps(&arc);
                    let dist = self.raw_dist_mul(a, b);
                    let (dist_val, dist_deps) = self.dmul.simplify_deps(&dist);

                    if let Some((dist2, arc_src, pair2)) = arc_to_src.get(&arc_val).cloned() {
                        let mut prem: Vec<FactId> = circle.fact.into_iter().collect();
                        prem.extend(arc_deps.iter().copied());
                        prem.extend(self.angle.simplify_deps(&arc_src).1);
                        prem.extend(self.dmul.simplify_deps(&dist2).1);
                        prem.extend(dist_deps.iter().copied());
                        let fact = self.log.add(Reason::TransferArcChord(pair2, (a, b)), prem);
                        changed = self.dmul.force_one(&dist.div(&dist2), Some(fact)) || changed;
                    } else {
                        arc_to_src.insert(arc_val.clone(), (dist.clone(), arc.clone(), (a, b)));
                    }
                    if let Some((arc2, dist_src, pair2)) = dist_to_src.get(&dist_val).cloned() {
                        let mut prem: Vec<FactId> = circle.fact.into_iter().collect();
                        prem.extend(dist_deps.iter().copied());
                        prem.extend(self.dmul.simplify_deps(&dist_src).1);
                        prem.extend(self.angle.simplify_deps(&arc2).1);
                        prem.extend(arc_deps.iter().copied());
                        let fact = self.log.add(Reason::TransferArcChord(pair2, (a, b)), prem);
                        changed = self.angle.force_zero(&arc.sub(&arc2), Some(fact)) || changed;
                    } else {
                        dist_to_src.insert(dist_val.clone(), (arc.clone(), dist.clone(), (a, b)));
                    }
                }
            }
        }
        changed
    }
}

// ---------------------------------------------------------------------------
// Collinearity, concyclicity and point merging
// ---------------------------------------------------------------------------

impl Ddar {
    fn force_collinear(&mut self, points_in: &[PointId], premises: Vec<FactId>) -> bool {
        self.force_collinear_with(points_in, |_| premises)
    }

    fn force_collinear_with(
        &mut self,
        points_in: &[PointId],
        premises: impl FnOnce(&Ddar) -> Vec<FactId>,
    ) -> bool {
        assert!(points_in.len() > 1);
        let a0 = points_in[0];
        let b = Self::arg_max_first(points_in, |p| distance(self.coord(a0), self.coord(p)));
        let c = Self::arg_max_first(points_in, |p| distance(self.coord(p), self.coord(b)));
        if self.num_identical(b, c) {
            panic!("collinearity predicate requires at least two distinct points");
        }
        let line1_id = self.pair_line[self.pk(b, c)].expect("no line for far pair");
        let line1_val = self.lines[line1_id].value;
        for &p in points_in {
            if line1_val.distance(self.coord(p)) >= ATOM {
                panic!("points not numerically collinear");
            }
        }
        let line1_points = self.lines[line1_id].points.clone();
        {
            let set1: FxHashSet<PointId> = line1_points.iter().copied().collect();
            if points_in.iter().all(|p| set1.contains(p)) {
                return false;
            }
        }

        // BFS: gather all lines sharing points with the growing set.
        let mut stack: Vec<PointId> = points_in.to_vec();
        let mut points_set: FxHashSet<PointId> = line1_points.iter().copied().collect();
        let mut points: Vec<PointId> = line1_points;
        let mut line_ids: Vec<usize> = vec![line1_id];
        let mut line_set: FxHashSet<usize> = [line1_id].into_iter().collect();
        while let Some(x) = stack.pop() {
            if points_set.contains(&x) {
                continue;
            }
            for y in points.clone() {
                if self.num_identical(x, y) {
                    continue;
                }
                let lid = match self.pair_line[self.pk(x, y)] {
                    Some(l) => l,
                    None => continue,
                };
                if line_set.contains(&lid) {
                    continue;
                }
                line_ids.push(lid);
                line_set.insert(lid);
                stack.extend(self.lines[lid].points.clone());
            }
            points.push(x);
            points_set.insert(x);
        }

        points.sort_by(|&p, &q| {
            line1_val
                .position(self.coord(p))
                .partial_cmp(&line1_val.position(self.coord(q)))
                .unwrap()
        });

        // Register the collinearity fact: premises plus the facts that
        // established the lines being merged.
        let mut prem = premises(self);
        for &lid in &line_ids {
            prem.extend(self.lines[lid].fact);
        }
        let fact = self.log.add(Reason::Collinear(points.clone()), prem);

        let main_pair = self.lines[line1_id].main_pair;
        let direction = self.lines[line1_id].direction.clone();

        // Additive segment relations along the line.
        let a = points[0];
        let a1 = *points.last().unwrap();
        for bi in 1..points.len() {
            for ci in (bi + 1)..points.len() {
                let (b, c) = (points[bi], points[ci]);
                if self.num_identical(b, c) {
                    continue;
                }
                let pos_b = if self.num_identical(a, b) {
                    self.raw_dist_add(a, a1).sub(&self.raw_dist_add(a1, b))
                } else {
                    self.raw_dist_add(a, b)
                };
                let pos_c = if self.num_identical(a, c) {
                    self.raw_dist_add(a, a1).sub(&self.raw_dist_add(a1, c))
                } else {
                    self.raw_dist_add(a, c)
                };
                let expr = pos_b.add(&self.raw_dist_add(b, c)).sub(&pos_c);
                self.dadd.force_zero(&expr, Some(fact));
            }
        }

        // Glue the directions of all merged lines.
        for &lid in &line_ids {
            let ld = self.lines[lid].direction.clone();
            self.angle.force_zero(&direction.sub(&ld), Some(fact));
        }

        // Replace the merged lines with the new one.
        let merged: FxHashSet<usize> = line_ids.iter().copied().collect();
        self.live_lines.retain(|id| !merged.contains(id));
        let main_line = FormalLine {
            points: points.clone(),
            main_pair,
            direction,
            value: line1_val,
            fact: Some(fact),
        };
        let main_id = self.add_line(main_line);
        for ii in 0..points.len() {
            for jj in (ii + 1)..points.len() {
                let (x, y) = (points[ii], points[jj]);
                if !self.num_identical(x, y) {
                    let (ab, ba) = (self.pk(x, y), self.pk(y, x));
                    self.pair_line[ab] = Some(main_id);
                    self.pair_line[ba] = Some(main_id);
                }
            }
        }
        true
    }

    fn force_concyclic(
        &mut self,
        points_in: &[PointId],
        centers_in: &[PointId],
        premises: Vec<FactId>,
    ) -> bool {
        self.force_concyclic_with(points_in, centers_in, |_| premises)
    }

    fn force_concyclic_with(
        &mut self,
        points_in: &[PointId],
        centers_in: &[PointId],
        premises: impl FnOnce(&Ddar) -> Vec<FactId>,
    ) -> bool {
        let mut stack: Vec<PointId> = points_in.to_vec();
        let mut points: Vec<PointId> = Vec::new();
        let mut points_set: FxHashSet<PointId> = FxHashSet::default();
        let mut circ_ids: Vec<usize> = Vec::new();
        let mut circ_set: FxHashSet<usize> = FxHashSet::default();

        while let Some(a) = stack.pop() {
            if points_set.contains(&a) {
                continue;
            }
            let cur = points.clone();
            for bi in 0..cur.len() {
                for ci in (bi + 1)..cur.len() {
                    let (b, c) = (cur[bi], cur[ci]);
                    if self.num_identical(a, b)
                        || self.num_identical(a, c)
                        || self.num_identical(b, c)
                    {
                        continue;
                    }
                    let circle_id = match self.triple_to_circle.get(&(a, b, c)) {
                        Some(&x) => x,
                        None => continue,
                    };
                    if circ_set.contains(&circle_id) {
                        continue;
                    }
                    if circ_ids.is_empty() {
                        let cpts: FxHashSet<PointId> =
                            self.circles[circle_id].points.iter().copied().collect();
                        let ccenters: FxHashSet<PointId> =
                            self.circles[circle_id].centers.iter().copied().collect();
                        let mut need: FxHashSet<PointId> = points.iter().copied().collect();
                        need.extend(stack.iter().copied());
                        if need.iter().all(|p| cpts.contains(p))
                            && centers_in.iter().all(|c| ccenters.contains(c))
                        {
                            return false;
                        }
                    }
                    circ_ids.push(circle_id);
                    circ_set.insert(circle_id);
                    stack.extend(self.circles[circle_id].points.clone());
                }
            }
            points_set.insert(a);
            points.push(a);
        }

        let (defining_points, circle_value): ([PointId; 3], NumCircle) = if !circ_ids.is_empty() {
            let c0 = &self.circles[circ_ids[0]];
            (
                c0.defining_points.expect("merged circle without defining"),
                c0.value,
            )
        } else {
            let mut defining: Vec<PointId> = Vec::new();
            for &x in &points {
                if !defining.iter().any(|&y| self.num_identical(x, y)) {
                    defining.push(x);
                    if defining.len() == 3 {
                        break;
                    }
                }
            }
            if defining.len() <= 2 {
                panic!("need at least three different points on a circle");
            }
            let cv = if !centers_in.is_empty() {
                NumCircle {
                    center: self.coord(centers_in[0]),
                    r: distance(self.coord(centers_in[0]), self.coord(points[0])),
                }
            } else {
                NumCircle::through(
                    self.coord(defining[0]),
                    self.coord(defining[1]),
                    self.coord(defining[2]),
                )
                .expect("collinear defining points")
            };
            ([defining[0], defining[1], defining[2]], cv)
        };

        for &x in &points {
            if circle_value.distance(self.coord(x)).powi(2) >= ATOM {
                panic!("points not numerically concyclic");
            }
        }

        let mut centers_set: FxHashSet<PointId> = centers_in.iter().copied().collect();
        for &cid in &circ_ids {
            for &c in &self.circles[cid].centers {
                centers_set.insert(c);
            }
        }
        let mut centers: Vec<PointId> = centers_set.into_iter().collect();
        centers.sort_by(|&p, &q| self.name(p).cmp(self.name(q)));

        // Register the concyclicity fact: premises plus the facts behind the
        // circles being merged.
        let mut prem = premises(self);
        for &cid in &circ_ids {
            prem.extend(self.circles[cid].fact);
        }
        let fact = self.log.add(Reason::Concyclic(points.clone()), prem);

        let main_circle = FormalCircle {
            defining_points: Some(defining_points),
            points: points.clone(),
            centers: centers.clone(),
            value: circle_value,
            fact: Some(fact),
        };

        // Implied inscribed-angle equalities.
        let (da, db, dc) = (defining_points[0], defining_points[1], defining_points[2]);
        let in_def = |p: PointId| p == da || p == db || p == dc;
        for xi in 0..points.len() {
            for yi in (xi + 1)..points.len() {
                let (mut x, mut y) = (points[xi], points[yi]);
                if self.num_identical(x, y) {
                    continue;
                }
                if in_def(x) {
                    std::mem::swap(&mut x, &mut y);
                    if in_def(x) {
                        continue;
                    }
                }
                let y2 = if self.num_identical(x, dc) { db } else { dc };
                let ang = self.raw_dir(x, y2).sub(&self.raw_dir(x, y));
                let (arc, _) = self.get_arc(&main_circle, y, y2);
                self.angle.force_zero(&ang.sub(&arc), Some(fact));
            }
        }

        // Equal distances from the (first) center.
        if !centers.is_empty() {
            let center = centers[0];
            let radius = self.get_dist_mul(points[0], center);
            for &x in &points[1..] {
                let dist = self.get_dist_mul(x, center);
                self.dmul.force_one(&radius.div(&dist), Some(fact));
            }
        }

        // Exchange in the database.
        let merged: FxHashSet<usize> = circ_ids.iter().copied().collect();
        self.live_circles.retain(|id| !merged.contains(id));
        let main_id = self.add_circle(main_circle);
        for &a in &points {
            for &b in &points {
                if self.num_identical(a, b) {
                    continue;
                }
                for &c in &points {
                    if self.num_identical(a, c) || self.num_identical(b, c) {
                        continue;
                    }
                    self.triple_to_circle.insert((a, b, c), main_id);
                }
            }
        }
        true
    }

    fn check_collinear(&self, points: &[PointId]) -> bool {
        for i in 0..points.len() {
            for j in (i + 1)..points.len() {
                let (a, b) = (points[i], points[j]);
                if let Some(lid) = self.pair_line[self.pk(a, b)] {
                    let lp: FxHashSet<PointId> = self.lines[lid].points.iter().copied().collect();
                    return points.iter().all(|p| lp.contains(p));
                }
            }
        }
        false
    }

    /// Lines of the closure holding at least three points.
    pub fn proved_lines(&self) -> Vec<Vec<PointId>> {
        self.live_lines
            .iter()
            .map(|&l| &self.lines[l])
            .filter(|l| l.points.len() >= 3)
            .map(|l| l.points.clone())
            .collect()
    }

    /// Circles of the closure as `(points, centers)`.
    pub fn proved_circles(&self) -> Vec<(Vec<PointId>, Vec<PointId>)> {
        self.live_circles
            .iter()
            .map(|&c| &self.circles[c])
            .filter(|c| c.points.len() >= 3 || (!c.centers.is_empty() && !c.points.is_empty()))
            .map(|c| (c.points.clone(), c.centers.clone()))
            .collect()
    }

    fn check_concyclic(&self, points: &[PointId], centers: &[PointId]) -> bool {
        let mut distinct: Vec<PointId> = Vec::new();
        for &p in points {
            if distinct.iter().any(|&x| self.num_identical(p, x)) {
                continue;
            }
            distinct.push(p);
        }
        assert!(
            distinct.len() >= 3,
            "need at least three numerically distinct points"
        );
        let triple = (distinct[0], distinct[1], distinct[2]);
        let cid = match self.triple_to_circle.get(&triple) {
            Some(&c) => c,
            None => return false,
        };
        let circle = &self.circles[cid];
        let cpts: FxHashSet<PointId> = circle.points.iter().copied().collect();
        let ccenters: FxHashSet<PointId> = circle.centers.iter().copied().collect();
        centers.iter().all(|c| ccenters.contains(c)) && points.iter().all(|p| cpts.contains(p))
    }

    fn force_equal_points(&mut self, a_in: PointId, b_in: PointId, premises: Vec<FactId>) -> bool {
        let a = self.subst[a_in as usize];
        let b = self.subst[b_in as usize];
        if a == b {
            return false;
        }
        let fact = self.log.add(Reason::PointMerge(a, b), premises);

        // Extend lines that contain exactly one of a, b.
        for lid in self.live_lines.clone() {
            let has_a = self.lines[lid].points.contains(&a);
            let has_b = self.lines[lid].points.contains(&b);
            if has_a == has_b {
                continue;
            }
            let mut pts = self.lines[lid].points.clone();
            pts.push(if has_a { b } else { a });
            self.force_collinear(&pts, vec![fact]);
        }
        // Rebuild lines with b removed.
        for lid in self.live_lines.clone() {
            if !self.lines[lid].points.contains(&b) {
                continue;
            }
            let mut main_pair = self.lines[lid].main_pair;
            let direction = self.lines[lid].direction.clone();
            if main_pair.0 == b {
                main_pair.0 = a;
            } else if main_pair.1 == b {
                main_pair.1 = a;
            }
            let new_points: Vec<PointId> = self.lines[lid]
                .points
                .iter()
                .copied()
                .filter(|&x| x != b)
                .collect();
            let value = self.lines[lid].value;
            let old_fact = self.lines[lid].fact;
            let line2 = FormalLine {
                points: new_points.clone(),
                main_pair,
                direction,
                value,
                fact: old_fact,
            };
            self.live_lines.retain(|&id| id != lid);
            let new_id = self.add_line(line2);
            for ii in 0..new_points.len() {
                for jj in 0..new_points.len() {
                    if ii == jj {
                        continue;
                    }
                    let (x, y) = (new_points[ii], new_points[jj]);
                    if !self.num_identical(x, y) {
                        let idx = self.pk(x, y);
                        self.pair_line[idx] = Some(new_id);
                    }
                }
            }
        }

        // Extend circles that contain exactly one of a, b.
        for cid in self.live_circles.clone() {
            let has_a = self.circles[cid].points.contains(&a);
            let has_b = self.circles[cid].points.contains(&b);
            if has_a == has_b {
                continue;
            }
            let mut pts = self.circles[cid].points.clone();
            pts.push(if has_a { b } else { a });
            let centers = self.circles[cid].centers.clone();
            self.force_concyclic(&pts, &centers, vec![fact]);
        }
        // Rebuild circles with b removed.
        for cid in self.live_circles.clone() {
            let has_b =
                self.circles[cid].points.contains(&b) || self.circles[cid].centers.contains(&b);
            if !has_b {
                continue;
            }
            let mut defining = self.circles[cid].defining_points;
            if let Some(d) = &mut defining {
                for x in d.iter_mut() {
                    if *x == b {
                        *x = a;
                    }
                }
            }
            let new_points: Vec<PointId> = self.circles[cid]
                .points
                .iter()
                .copied()
                .filter(|&x| x != b)
                .collect();
            let new_centers: Vec<PointId> = self.circles[cid]
                .centers
                .iter()
                .copied()
                .filter(|&x| x != b)
                .collect();
            let value = self.circles[cid].value;
            let old_fact = self.circles[cid].fact;
            let circle2 = FormalCircle {
                defining_points: defining,
                points: new_points.clone(),
                centers: new_centers,
                value,
                fact: old_fact,
            };
            self.live_circles.retain(|&id| id != cid);
            let new_id = self.add_circle(circle2);
            for xi in 0..new_points.len() {
                for yi in 0..new_points.len() {
                    for zi in 0..new_points.len() {
                        let (x, y, z) = (new_points[xi], new_points[yi], new_points[zi]);
                        if self.num_identical(x, y)
                            || self.num_identical(y, z)
                            || self.num_identical(z, x)
                        {
                            continue;
                        }
                        self.triple_to_circle.insert((x, y, z), new_id);
                    }
                }
            }
        }

        // Equate distances to all other points.
        for x in self.active.clone() {
            if x == a || x == b {
                continue;
            }
            if !self.num_identical(x, a) && !self.num_identical(x, b) {
                let d1 = self.raw_dist_mul(x, a);
                let d2 = self.raw_dist_mul(x, b);
                self.dmul.force_one(&d1.div(&d2), Some(fact));
                let s = DistSq(&self.raw_dist_sq(x, a).0 - &self.raw_dist_sq(x, b).0);
                self.dsq.force_zero(&s, Some(fact));
            }
        }

        // Retire b.
        for s in self.subst.iter_mut() {
            if *s == b {
                *s = a;
            }
        }
        self.active.retain(|&x| x != b);
        true
    }

    fn check_equal_points(&self, a: PointId, b: PointId) -> bool {
        self.subst[a as usize] == self.subst[b as usize]
    }
}

struct Interner {
    angular: bool,
    table: hashbrown::HashTable<(u64, u32)>,
    items: Vec<LinComb>,
    halves: FxHashMap<u32, u32>,
}

impl Interner {
    fn with_capacity(angular: bool, cap: usize) -> Interner {
        Interner {
            angular,
            table: hashbrown::HashTable::with_capacity(cap),
            items: Vec::with_capacity(cap),
            halves: FxHashMap::default(),
        }
    }

    fn intern(&mut self, comb: LinComb) -> u32 {
        use std::hash::BuildHasher;
        let h = rustc_hash::FxBuildHasher.hash_one(&comb);
        let items = &self.items;
        match self.table.entry(
            h,
            |&(hh, i)| hh == h && items[i as usize] == comb,
            |&(hh, _)| hh,
        ) {
            hashbrown::hash_table::Entry::Occupied(e) => e.get().1,
            hashbrown::hash_table::Entry::Vacant(e) => {
                let id = self.items.len() as u32;
                e.insert((h, id));
                self.items.push(comb);
                id
            }
        }
    }

    fn diff(&mut self, x: u32, y: u32) -> u32 {
        let d = &self.items[x as usize] - &self.items[y as usize];
        let d = if self.angular { Angle::new(d).0 } else { d };
        self.intern(d)
    }

    fn item(&self, id: u32) -> &LinComb {
        &self.items[id as usize]
    }

    fn is_zero(&self, id: u32) -> bool {
        self.items[id as usize].is_zero()
    }

    fn plus_half_turn(&mut self, id: u32) -> u32 {
        debug_assert!(self.angular);
        if let Some(&h) = self.halves.get(&id) {
            return h;
        }
        let half = LinComb::singleton(crate::elimination::ANGLE_UNIT, Rat::new(1, 2));
        let comb = Angle::new(&self.items[id as usize] + &half).0;
        let h = self.intern(comb);
        self.halves.insert(id, h);
        h
    }
}

struct PairIds {
    n: usize,
    dir: Interner,
    dm: Interner,
    dir_id: Vec<u32>,
    dm_id: Vec<u32>,
    memo: TripleMemo,
    fp_ok: bool,
    dir_fp: Vec<u64>,
    dir_unit: Vec<Rat>,
    dir_unit_rank: Vec<u32>,
    w0: u64,
    dm_fp: Vec<u64>,
}

#[derive(Default)]
struct TripleMemo {
    stamp: u32,
    ang: Vec<(u32, u32)>,
    rat: Vec<(u32, u32)>,
    ssa: Vec<u32>,
}

thread_local! {
    static TRIPLE_MEMO: std::cell::RefCell<TripleMemo> = std::cell::RefCell::new(TripleMemo::default());
}

impl TripleMemo {
    fn take(len: usize) -> TripleMemo {
        let mut m = TRIPLE_MEMO.with(|t| std::mem::take(&mut *t.borrow_mut()));
        m.stamp = m.stamp.wrapping_add(1);
        if m.stamp == 0 {
            m.ang.clear();
            m.rat.clear();
            m.ssa.clear();
            m.stamp = 1;
        }
        if m.ang.len() < len {
            m.ang.resize(len, (0, 0));
            m.rat.resize(len, (0, 0));
            m.ssa.resize(len, 0);
        }
        m
    }

    fn mark_ssa(&mut self, k: usize) -> bool {
        let fresh = self.ssa[k] != self.stamp;
        self.ssa[k] = self.stamp;
        fresh
    }
}

impl Drop for PairIds {
    fn drop(&mut self) {
        let m = std::mem::take(&mut self.memo);
        TRIPLE_MEMO.with(|t| {
            let mut slot = t.borrow_mut();
            if m.ang.len() >= slot.ang.len() {
                *slot = m;
            }
        });
    }
}

impl PairIds {
    fn ang(&mut self, a: PointId, b: PointId, c: PointId) -> u32 {
        let n = self.n;
        let (a, b, c) = (a as usize, b as usize, c as usize);
        let k = (a * n + b) * n + c;
        let (stamp, id) = self.memo.ang[k];
        if stamp == self.memo.stamp {
            return id;
        }
        let id = self.dir.diff(self.dir_id[a * n + c], self.dir_id[a * n + b]);
        self.memo.ang[k] = (self.memo.stamp, id);
        id
    }

    fn rat(&mut self, a: PointId, b: PointId, c: PointId) -> u32 {
        let n = self.n;
        let (a, b, c) = (a as usize, b as usize, c as usize);
        let k = (a * n + b) * n + c;
        let (stamp, id) = self.memo.rat[k];
        if stamp == self.memo.stamp {
            return id;
        }
        let id = self.dm.diff(self.dm_id[a * n + c], self.dm_id[a * n + b]);
        self.memo.rat[k] = (self.memo.stamp, id);
        id
    }

    fn ang_key(&mut self, a: PointId, b: PointId, c: PointId) -> u64 {
        if !self.fp_ok {
            return self.ang(a, b, c) as u64;
        }
        let n = self.n;
        let (x, y) = (a as usize * n + c as usize, a as usize * n + b as usize);
        let d = fingerprint::sub(self.dir_fp[x], self.dir_fp[y]);
        if self.dir_unit_rank[x] < self.dir_unit_rank[y] {
            fingerprint::add(d, self.w0)
        } else {
            d
        }
    }

    fn half_turn_key(&mut self, a: PointId, b: PointId, c: PointId) -> u64 {
        if !self.fp_ok {
            let id = self.ang(a, b, c);
            return self.dir.plus_half_turn(id) as u64;
        }
        let n = self.n;
        let (x, y) = (a as usize * n + c as usize, a as usize * n + b as usize);
        let mut u = &self.dir_unit[x] - &self.dir_unit[y];
        if u.is_negative() {
            u = &u + &Rat::one();
        }
        let half = fingerprint::mul(self.w0, fingerprint::rat(&Rat::new(1, 2)).unwrap());
        let shift = if u >= Rat::new(1, 2) {
            fingerprint::sub(half, self.w0)
        } else {
            half
        };
        fingerprint::add(self.ang_key(a, b, c), shift)
    }

    fn rat_key(&mut self, a: PointId, b: PointId, c: PointId) -> u64 {
        if !self.fp_ok {
            return self.rat(a, b, c) as u64;
        }
        let n = self.n;
        let (x, y) = (a as usize * n + c as usize, a as usize * n + b as usize);
        fingerprint::sub(self.dm_fp[x], self.dm_fp[y])
    }

    fn ang_key_is_zero(&mut self, key: u64, a: PointId, b: PointId, c: PointId) -> bool {
        if self.fp_ok && key != 0 {
            return false;
        }
        let id = self.ang(a, b, c);
        self.dir.is_zero(id)
    }

    fn ang_eq(&mut self, p: Triple, q: Triple) -> bool {
        if self.fp_ok && self.ang_key(p.0, p.1, p.2) != self.ang_key(q.0, q.1, q.2) {
            return false;
        }
        self.ang(p.0, p.1, p.2) == self.ang(q.0, q.1, q.2)
    }

    fn rat_eq(&mut self, p: Triple, q: Triple) -> bool {
        if self.fp_ok && self.rat_key(p.0, p.1, p.2) != self.rat_key(q.0, q.1, q.2) {
            return false;
        }
        self.rat(p.0, p.1, p.2) == self.rat(q.0, q.1, q.2)
    }
}

fn sss_exact(ids: &mut PairIds, (a, b, c): Triple, _form: u8) -> (u32, u32) {
    (ids.rat(a, b, c), ids.rat(c, b, a))
}

fn aa_exact(ids: &mut PairIds, (a, b, c): Triple, form: u8) -> (u32, u32) {
    if form == 0 {
        (ids.ang(a, b, c), ids.ang(c, b, a))
    } else {
        (ids.ang(a, c, b), ids.ang(c, a, b))
    }
}

fn sas_exact(ids: &mut PairIds, (a, b, c): Triple, form: u8) -> (u32, u32) {
    if form == 0 {
        (ids.ang(a, b, c), ids.rat(a, b, c))
    } else {
        (ids.ang(a, c, b), ids.rat(a, b, c))
    }
}

fn ssa_exact(ids: &mut PairIds, (a, b, c): Triple, form: u8) -> (u32, u32) {
    if form == 0 {
        (ids.ang(a, b, c), ids.rat(c, b, a))
    } else {
        (ids.ang(a, c, b), ids.rat(c, b, a))
    }
}

type FpEntry = (Triple, u8);
type ExactKey = fn(&mut PairIds, Triple, u8) -> (u32, u32);

fn find_or_put<K: std::hash::Hash + Eq + Copy>(
    map: &mut FpBuckets<K>,
    ids: &mut PairIds,
    key: K,
    entry: FpEntry,
    exact: ExactKey,
) -> Option<Triple> {
    let mut want: Option<(u32, u32)> = None;
    let mut same = |e: FpEntry| {
        let w = match want {
            Some(w) => w,
            None => *want.insert(exact(ids, entry.0, entry.1)),
        };
        exact(ids, e.0, e.1) == w
    };
    if let Some(t0) = map.find(key, &mut same) {
        return Some(t0);
    }
    map.put(key, entry, same);
    None
}

fn put<K: std::hash::Hash + Eq + Copy>(
    map: &mut FpBuckets<K>,
    ids: &mut PairIds,
    key: K,
    entry: FpEntry,
    exact: ExactKey,
) {
    let mut want: Option<(u32, u32)> = None;
    map.put(key, entry, |e: FpEntry| {
        let w = match want {
            Some(w) => w,
            None => *want.insert(exact(ids, entry.0, entry.1)),
        };
        exact(ids, e.0, e.1) == w
    });
}

struct FpBuckets<K> {
    first: FxHashMap<K, FpEntry>,
    more: FxHashMap<K, Vec<FpEntry>>,
}

impl<K: std::hash::Hash + Eq + Copy> FpBuckets<K> {
    fn with_capacity(cap: usize) -> Self {
        let mut first = FxHashMap::default();
        first.reserve(cap);
        FpBuckets {
            first,
            more: FxHashMap::default(),
        }
    }

    fn find(&self, k: K, mut same: impl FnMut(FpEntry) -> bool) -> Option<Triple> {
        let e = *self.first.get(&k)?;
        if same(e) {
            return Some(e.0);
        }
        self.more.get(&k)?.iter().find(|&&x| same(x)).map(|x| x.0)
    }

    fn put(&mut self, k: K, v: FpEntry, mut same: impl FnMut(FpEntry) -> bool) {
        match self.first.entry(k) {
            Entry::Vacant(e) => {
                e.insert(v);
            }
            Entry::Occupied(mut e) => {
                if same(*e.get()) {
                    *e.get_mut() = v;
                    return;
                }
                let more = self.more.entry(k).or_default();
                match more.iter_mut().find(|x| same(**x)) {
                    Some(x) => *x = v,
                    None => more.push(v),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fp_buckets_only_match_exactly_equal_entries() {
        let exact = |t: Triple| t.0 % 3;
        let (t1, t2, t3, t4) = ((1, 0, 0), (5, 0, 0), (9, 0, 0), (10, 0, 0));
        let mut b: FpBuckets<u64> = FpBuckets::with_capacity(4);
        b.put(7, (t1, 0), |e| exact(e.0) == exact(t1));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t2)), None);
        b.put(7, (t2, 0), |e| exact(e.0) == exact(t2));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t1)), Some(t1));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t2)), Some(t2));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t3)), None);
        assert_eq!(b.find(8, |_| true), None);
        b.put(7, (t4, 1), |e| exact(e.0) == exact(t4));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t1)), Some(t4));
        assert_eq!(b.find(7, |e| exact(e.0) == exact(t2)), Some(t2));
    }
}
