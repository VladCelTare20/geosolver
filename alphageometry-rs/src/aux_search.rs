//! Auxiliary-point search — a language-model-free way to extend DDAR's reach.
//!
//! Many olympiad problems are *not* solvable by pure deduction on the given
//! figure: the proof needs an extra point (a midpoint, an intersection, a
//! reflection, a circumcenter, …). In the full AlphaGeometry2 system a trained
//! transformer proposes these auxiliary constructions. Here we instead *search*
//! over a library of classical constructions, which is only practical because a
//! single DDAR run now takes on the order of ten milliseconds.
//!
//! Each candidate is built numerically (its coordinates are exact) together with
//! the predicates that define it symbolically. We append it to the problem and
//! re-run the closure; if the goal now follows, we have a proof-with-aux.
//!
//! Candidate DDAR runs are wrapped in [`std::panic::catch_unwind`] so a
//! degenerate construction can never crash the search.

use crate::numerics::{distance, intersect_ll, midpoint, NumCircle, NumLine, Vec2};
use crate::predicate::{Point, PointId, Predicate};
use crate::rational::Rat;
use crate::runner::solve_problem;
use crate::Problem;
use rustc_hash::FxHashSet;
use std::time::Instant;

/// The construction template a candidate came from (used for ranking).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Midpoint,
    Circumcenter,
    Foot,
    ReflectPoint,
    ReflectLine,
    IntersectLL,
    IntersectLC,
    /// Second intersection of a line with the circumcircle of a point triple —
    /// the classic "cevian meets the circumcircle again" auxiliary. Unlike
    /// [`Kind::IntersectLC`] the circle need not be named by a hypothesis; it is
    /// the circumcircle of a triangle already present in the figure.
    IntersectLCircum,
    /// Orthocentre of a triangle (two altitudes meet).
    Orthocenter,
    /// A tritangent centre (incentre or one of the three excentres): the four
    /// intersections of the internal/external angle-bisector line-pairs.
    BisectorCenter,
    /// Where the internal bisector of an angle meets the opposite side.
    BisectorFoot,
    /// A tangency point of a tangent drawn from a point to a named circle.
    TangentPoint,
    /// An intersection of two circles (known circles and figure circumcircles).
    CircleCircle,
    /// The midpoint of an arc cut by two circle points: on the circle and
    /// equidistant from both (the incentre–excentre lemma's workhorse).
    ArcMidpoint,
    /// A centre of similitude (internal or external homothety centre) of two
    /// circles whose centres are existing points — on the line of centres,
    /// dividing it in the ratio of the radii (Monge, tangent-line problems).
    HomothetyCenter,
    /// The point diametrically opposite a point on a circumcircle or named
    /// circle (right angles in the semicircle at every other circle point).
    Antipode,
    /// The fourth vertex completing a parallelogram on three existing points.
    Parallelogram,
    /// A touch point of the incircle (or an excircle) on a triangle side,
    /// pinned by the tangent-length identity as an additive `distseq` fact.
    IncircleTouch,
    /// The centre of the spiral similarity taking one segment to another —
    /// the Miquel-point workhorse (△XAB ~ △XCD directly).
    SpiralCenter,
    /// The isogonal conjugate of an existing point w.r.t. a triangle of the
    /// figure (cevians reflected in the angle bisectors concur there).
    IsogonalConjugate,
    /// The inverse of a point in a named circle (|OX|·|OP| = r²).
    InversePoint,
    /// The pole of a chord of a named circle: where the tangents at its
    /// endpoints meet.
    PoleOfChord,
}

/// Lowercased names already used by the figure's points. Natural aux names
/// must avoid these case-insensitively: the renderer's `disp` uppercases every
/// name, so `m` and an existing `M` would be indistinguishable on the drawing.
fn taken_names(problem: &Problem) -> FxHashSet<String> {
    problem
        .points
        .iter()
        .map(|p| p.name.to_lowercase())
        .collect()
}

/// The name a textbook would give this construction — `m` (M) for a midpoint,
/// `o` (O) for a circumcenter, `p'` (P′) for a point derived from P by
/// reflection / antipode / inversion / isogonal conjugation, `b'` for the
/// second meeting of a line through B with a circle — instead of the opaque
/// `auxN`. Falls back to numbered variants (`m1` → M₁) when the letter is
/// taken, and to `auxN` as the last resort. Every returned name is unique
/// against the figure (case-insensitively), so applied constructions can never
/// collide: deeper search levels regenerate candidates from the augmented
/// problem, whose point list then contains the earlier aux names.
fn natural_name(
    kind: Kind,
    args: &[PointId],
    problem: &Problem,
    taken: &FxHashSet<String>,
    new_id: PointId,
) -> String {
    let nm = |i: PointId| problem.points[i as usize].name.as_str();
    let named = |i: PointId| !nm(i).starts_with('_');
    let prime = |i: PointId| format!("{}'", nm(i));
    let c = |i: PointId| problem.points[i as usize].value;

    // Preferred stem: a primed source point for derived points.
    let stem: Option<String> = match kind {
        Kind::ReflectPoint
        | Kind::ReflectLine
        | Kind::Antipode
        | Kind::InversePoint
        | Kind::IsogonalConjugate
            if !args.is_empty() && named(args[0]) =>
        {
            Some(prime(args[0]))
        }
        // parallelogram(i,j,k): the new vertex is opposite j — j′.
        Kind::Parallelogram if args.len() >= 3 && named(args[1]) => Some(prime(args[1])),
        // Line pq meets circumcircle(x,y,z) again: "BQ meets Ω again at B′".
        Kind::IntersectLCircum if args.len() >= 5 => [args[0], args[1]]
            .into_iter()
            .find(|p| args[2..5].contains(p) && named(*p))
            .map(prime),
        // Line ij meets circle(o,a) again — prime whichever of i,j lies on it.
        Kind::IntersectLC if args.len() >= 4 => {
            let r = distance(c(args[2]), c(args[3]));
            [args[0], args[1]]
                .into_iter()
                .find(|&p| named(p) && (distance(c(args[2]), c(p)) - r).abs() < 1e-6 * r.max(1.0))
                .map(prime)
        }
        _ => None,
    };
    let letter = match kind {
        Kind::Midpoint => "m",
        Kind::Circumcenter => "o",
        Kind::Orthocenter => "h",
        Kind::BisectorCenter => "i",
        Kind::BisectorFoot => "d",
        Kind::Foot => "f",
        Kind::ArcMidpoint => "n",
        Kind::TangentPoint | Kind::IncircleTouch => "t",
        Kind::SpiralCenter => "z",
        Kind::HomothetyCenter | Kind::PoleOfChord => "k",
        // Intersections, and derived points whose source is anonymous.
        _ => "x",
    };

    let free = |n: &str| !taken.contains(&n.to_lowercase());
    if let Some(s) = stem {
        if free(&s) {
            return s;
        }
        let double = format!("{s}'");
        if free(&double) {
            return double;
        }
    }
    if free(letter) {
        return letter.to_string();
    }
    for k in 1..=9 {
        let n = format!("{letter}{k}");
        if free(&n) {
            return n;
        }
    }
    format!("aux{new_id}")
}

/// Prior weight per construction kind.
///
/// Grounded in a small self-measurement over this repo's own corpus (deleting
/// re-discoverable auxiliary points from the bundled IMO problems and recording
/// which kinds succeed) plus the distribution reported in the AlphaGeometry
/// paper, where midpoints/feet/reflections dominate LM proposals. This is a
/// statistical prior, not a neural model — see the README section on ML.
fn kind_prior(kind: Kind) -> f64 {
    match kind {
        Kind::Midpoint => 1.0,
        Kind::Circumcenter => 0.9,
        Kind::Foot => 0.8,
        Kind::Orthocenter => 0.75,
        Kind::ReflectPoint => 0.7,
        Kind::BisectorCenter => 0.68,
        Kind::IntersectLCircum => 0.65,
        Kind::BisectorFoot => 0.62,
        Kind::ReflectLine => 0.6,
        Kind::ArcMidpoint => 0.58,
        Kind::TangentPoint => 0.55,
        Kind::CircleCircle => 0.52,
        Kind::IntersectLC => 0.5,
        Kind::HomothetyCenter => 0.45,
        Kind::Antipode => 0.6,
        Kind::Parallelogram => 0.48,
        Kind::IncircleTouch => 0.44,
        Kind::SpiralCenter => 0.42,
        Kind::PoleOfChord => 0.38,
        Kind::IsogonalConjugate => 0.35,
        Kind::InversePoint => 0.3,
        Kind::IntersectLL => 0.3,
    }
}

/// A single auxiliary construction.
#[derive(Clone, Debug)]
pub struct Construction {
    pub name: String,
    pub coord: Vec2,
    pub preds: Vec<Predicate>,
    /// Human-readable description, e.g. `midpoint(b,c)`.
    pub desc: String,
    pub kind: Kind,
    /// The existing points this construction is built from.
    pub args: Vec<PointId>,
}

/// The outcome of a successful search.
#[derive(Clone, Debug)]
pub struct AuxProof {
    /// Constructions to add, in order.
    pub constructions: Vec<Construction>,
}

/// Statistics about a search.
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchStats {
    pub runs: usize,
}

fn pred(name: &str, points: Vec<PointId>) -> Predicate {
    Predicate {
        name: name.to_string(),
        points,
        constants: Vec::new(),
    }
}

/// Whether warm-start candidate evaluation is enabled (default yes; set
/// `AUX_NO_WARM=1` to force full from-scratch solves, e.g. for A/B timing).
fn warm_enabled() -> bool {
    use std::sync::OnceLock;
    static W: OnceLock<bool> = OnceLock::new();
    *W.get_or_init(|| !std::env::var("AUX_NO_WARM").is_ok_and(|v| v != "0"))
}

/// Solve a problem, treating any panic (from a degenerate construction) or
/// parse/goal error as "not solved".
fn safe_solve(problem: &Problem) -> bool {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| solve_problem(problem)))
        .map(|r| r.unwrap_or(false))
        .unwrap_or(false)
}

fn augment(problem: &Problem, cand: &Construction) -> Problem {
    let mut p = problem.clone();
    p.points.push(Point {
        name: cand.name.clone(),
        value: cand.coord,
    });
    p.preds.extend(cand.preds.iter().cloned());
    p
}

/// Apply a sequence of found constructions to a problem, returning the
/// augmented problem (used e.g. to print a proof after a successful search).
pub fn apply_constructions(problem: &Problem, constructions: &[Construction]) -> Problem {
    let mut p = problem.clone();
    for c in constructions {
        p = augment(&p, c);
    }
    p
}

/// A base figure whose deductive closure is computed **once**, so many
/// candidate constructions can be evaluated by *warm start*: clone the closed
/// base, [`Ddar::activate`] one fresh point at the candidate's coordinates,
/// force its predicates, and re-close. Because the closure is confluent, this
/// yields exactly the verdict a from-scratch [`solve_problem`] on the augmented
/// figure would — but without re-forcing the base hypotheses or re-deriving the
/// base closure for every candidate. `--verify-warm` checks the equivalence
/// exhaustively over the bundled corpus.
pub struct WarmBase {
    base: crate::Ddar,
    aux_id: PointId,
    goal: Predicate,
}

impl WarmBase {
    /// Close `problem` once, reserving a slot for the candidate point. Returns
    /// `None` for a goal-less problem (nothing to prove).
    pub fn new(problem: &Problem) -> Option<WarmBase> {
        let goal = problem.goal.clone()?;
        let mut base = crate::Ddar::new_with_slack(&problem.points, 1);
        for p in &problem.preds {
            base.force_pred(p);
        }
        base.deduction_closure();
        Some(WarmBase {
            base,
            aux_id: problem.points.len() as PointId,
            goal,
        })
    }

    /// Whether the closed base figure already proves the goal — the verdict
    /// [`safe_solve`] gives on the same problem, without a second closure.
    pub fn proves_goal(&self) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.base.clone().check_pred(&self.goal)
        }))
        .unwrap_or(false)
    }

    /// Whether adding `cand` (one new point) makes the goal provable. Panics
    /// from degenerate constructions are caught and treated as "not solved",
    /// matching [`safe_solve`].
    pub fn check(&self, cand: &Construction) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut d = self.base.clone();
            d.activate(self.aux_id, cand.coord);
            for p in &cand.preds {
                d.force_pred(p);
            }
            d.deduction_closure();
            d.check_pred(&self.goal)
        }))
        .unwrap_or(false)
    }
}

/// Circles named by the hypotheses: `cong` predicates with a repeated point
/// give a circle centred on it (both argument orders are accepted — problems
/// write `cong o a o b` and `cong a o b o` interchangeably). Returns
/// deduplicated `(center, through)` pairs.
fn known_circles(problem: &Problem) -> Vec<(PointId, PointId)> {
    let mut out: Vec<(PointId, PointId)> = Vec::new();
    let mut seen: FxHashSet<(PointId, i64)> = FxHashSet::default();
    let coord = |i: PointId| problem.points[i as usize].value;
    for p in &problem.preds {
        if p.name == "cong" && p.points.len() == 4 {
            if let Some((o, a)) = crate::svg::cong_center(&p.points) {
                let r_key = (distance(coord(o), coord(a)) * 1e6).round() as i64;
                if seen.insert((o, r_key)) {
                    out.push((o, a));
                }
            }
        }
    }
    out
}

/// The points other than `o` that lie on circle(o, a) *by the hypotheses*:
/// linked to `a` by a chain of `cong` facts centred on `o`. Numeric membership
/// is not enough — a point can sit on the circle only because the goal holds,
/// and asserting it would let the search prove the goal from itself.
fn hypothesis_circle_members(problem: &Problem, o: PointId, a: PointId) -> Vec<PointId> {
    let n = problem.points.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let spoke = |s: &[PointId]| match *s {
        [p, q] if p == o && q != o => Some(q as usize),
        [p, q] if q == o && p != o => Some(p as usize),
        _ => None,
    };
    for pr in &problem.preds {
        if pr.name == "cong" && pr.points.len() == 4 {
            if let (Some(x), Some(y)) = (spoke(&pr.points[..2]), spoke(&pr.points[2..])) {
                let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
                parent[rx] = ry;
            }
        }
    }
    let root = find(&mut parent, a as usize);
    (0..n)
        .filter(|&p| p != o as usize && find(&mut parent, p) == root)
        .map(|p| p as PointId)
        .collect()
}

/// The point pairs that form the *lines and chords of the figure*: every pair
/// named adjacently by a predicate, every pair inside a `coll` set, and every
/// pair of goal points. Auxiliary constructions overwhelmingly build on these,
/// so restricting intersections/feet/reflections to them turns an O(n^4) blow-up
/// into a handful of relevant candidates — the key to finding aux points fast.
fn salient_lines(problem: &Problem) -> Vec<(PointId, PointId)> {
    let mut set: FxHashSet<(PointId, PointId)> = FxHashSet::default();
    let mut add = |a: PointId, b: PointId| {
        if a != b {
            set.insert(if a < b { (a, b) } else { (b, a) });
        }
    };
    let mut handle = |pred: &Predicate| {
        if pred.name == "coll" {
            for i in 0..pred.points.len() {
                for j in (i + 1)..pred.points.len() {
                    add(pred.points[i], pred.points[j]);
                }
            }
        } else {
            for ch in pred.points.chunks_exact(2) {
                add(ch[0], ch[1]);
            }
        }
    };
    for p in &problem.preds {
        handle(p);
    }
    if let Some(g) = &problem.goal {
        handle(g);
        for i in 0..g.points.len() {
            for j in (i + 1)..g.points.len() {
                add(g.points[i], g.points[j]);
            }
        }
    }
    set.into_iter().collect()
}

/// Points that co-occur with a goal point in some predicate (1-hop
/// neighbourhood of the goal), used to boost the ranking of nearby candidates.
fn goal_neighborhood(problem: &Problem) -> FxHashSet<PointId> {
    let goal_pts: FxHashSet<PointId> = problem
        .goal
        .as_ref()
        .map(|g| g.points.iter().copied().collect())
        .unwrap_or_default();
    let mut nbr = goal_pts.clone();
    for pred in &problem.preds {
        if pred.points.iter().any(|p| goal_pts.contains(p)) {
            nbr.extend(pred.points.iter().copied());
        }
    }
    nbr
}

/// All construction candidates over the current point set, **ranked**: most
/// promising first (kind prior + goal-relevance). Intersections, feet and
/// line-reflections are restricted to [`salient_lines`].
///
/// `full` enables the line/line and line/circle intersections; for very large
/// figures these are skipped to keep the branching factor manageable.
pub fn candidates(problem: &Problem, full: bool) -> Vec<Construction> {
    let n = problem.points.len();
    let new_id = n as PointId;
    let c = |i: PointId| problem.points[i as usize].value;
    let nm = |i: PointId| problem.points[i as usize].name.clone();

    let mut out: Vec<Construction> = Vec::new();
    // Dedup key includes the construction kind: two kinds can produce the SAME
    // coordinates with DIFFERENT defining predicates (e.g. the midpoint of a
    // right triangle's hypotenuse is also its circumcenter), and discarding
    // one would lose the predicates a proof might need.
    let mut seen: FxHashSet<(Kind, i64, i64)> = FxHashSet::default();
    let taken = taken_names(problem);

    let try_push = |out: &mut Vec<Construction>,
                    seen: &mut FxHashSet<(Kind, i64, i64)>,
                    coord: Vec2,
                    preds: Vec<Predicate>,
                    desc: String,
                    kind: Kind,
                    args: Vec<PointId>| {
        if !coord.x.is_finite() || !coord.y.is_finite() {
            return;
        }
        for i in 0..n {
            if distance(coord, problem.points[i].value) < 1e-6 {
                return; // coincides with an existing point
            }
        }
        let key = (
            kind,
            (coord.x * 1e6).round() as i64,
            (coord.y * 1e6).round() as i64,
        );
        if !seen.insert(key) {
            return;
        }
        let name = natural_name(kind, &args, problem, &taken, new_id);
        out.push(Construction {
            name,
            coord,
            preds,
            desc,
            kind,
            args,
        });
    };

    // Midpoints.
    for i in 0..n as PointId {
        for j in (i + 1)..n as PointId {
            let coord = midpoint(c(i), c(j));
            let preds = vec![
                pred("coll", vec![i, j, new_id]),
                pred("cong", vec![i, new_id, j, new_id]),
            ];
            try_push(
                &mut out,
                &mut seen,
                coord,
                preds,
                format!("midpoint({},{})", nm(i), nm(j)),
                Kind::Midpoint,
                vec![i, j],
            );
        }
    }

    // Circumcenters.
    for i in 0..n as PointId {
        for j in (i + 1)..n as PointId {
            for k in (j + 1)..n as PointId {
                if let Some(circ) = NumCircle::through(c(i), c(j), c(k)) {
                    let preds = vec![
                        pred("cong", vec![new_id, i, new_id, j]),
                        pred("cong", vec![new_id, j, new_id, k]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        circ.center,
                        preds,
                        format!("circumcenter({},{},{})", nm(i), nm(j), nm(k)),
                        Kind::Circumcenter,
                        vec![i, j, k],
                    );
                }
            }
        }
    }

    // Orthocentres: two altitudes meet.
    for i in 0..n as PointId {
        for j in (i + 1)..n as PointId {
            for k in (j + 1)..n as PointId {
                let (va, vb, vc) = (c(i), c(j), c(k));
                let alt_i = NumLine::through1((vc - vb).normalize(), va);
                let alt_j = NumLine::through1((vc - va).normalize(), vb);
                if let Some(h) = intersect_ll(&alt_i, &alt_j) {
                    let preds = vec![
                        pred("perp", vec![i, new_id, j, k]),
                        pred("perp", vec![j, new_id, i, k]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        h,
                        preds,
                        format!("orthocenter({},{},{})", nm(i), nm(j), nm(k)),
                        Kind::Orthocenter,
                        vec![i, j, k],
                    );
                }
            }
        }
    }

    // Tritangent centres (incentre + 3 excentres) and angle-bisector feet.
    // The two eqangle predicates express the internal/external bisector *pair*
    // at vertices i and j (equal mod pi), whose four intersections are exactly
    // the incentre and three excentres; the numeric coordinate selects each.
    for i in 0..n as PointId {
        for j in (i + 1)..n as PointId {
            for k in (j + 1)..n as PointId {
                let (va, vb, vc) = (c(i), c(j), c(k));
                let (sa, sb, sc) = (distance(vb, vc), distance(vc, va), distance(va, vb));
                let bis = || {
                    vec![
                        pred("eqangle", vec![i, j, i, new_id, i, new_id, i, k]),
                        pred("eqangle", vec![j, i, j, new_id, j, new_id, j, k]),
                    ]
                };
                for (name, wa, wb, wc, denom) in [
                    ("incenter", sa, sb, sc, sa + sb + sc),
                    ("excenter", -sa, sb, sc, -sa + sb + sc),
                    ("excenter", sa, -sb, sc, sa - sb + sc),
                    ("excenter", sa, sb, -sc, sa + sb - sc),
                ] {
                    if denom.abs() < 1e-9 {
                        continue;
                    }
                    let coord = (va * wa + vb * wb + vc * wc) * (1.0 / denom);
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        bis(),
                        format!("{name}({},{},{})", nm(i), nm(j), nm(k)),
                        Kind::BisectorCenter,
                        vec![i, j, k],
                    );
                }
                // Internal bisector at each vertex meets the opposite side
                // (angle-bisector theorem: it divides the side by the adjacent
                // side lengths).
                for (v, p, q) in [(i, j, k), (j, i, k), (k, i, j)] {
                    let (dvp, dvq) = (distance(c(v), c(p)), distance(c(v), c(q)));
                    if dvp + dvq < 1e-9 {
                        continue;
                    }
                    let coord = c(p) + (c(q) - c(p)) * (dvp / (dvp + dvq));
                    let preds = vec![
                        pred("coll", vec![p, q, new_id]),
                        pred("eqangle", vec![v, p, v, new_id, v, new_id, v, q]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("bisector_foot({} in {}{}{})", nm(v), nm(i), nm(j), nm(k)),
                        Kind::BisectorFoot,
                        vec![v, p, q],
                    );
                }
            }
        }
    }

    // *All* point-pairs are candidate lines (completeness); the salient set is
    // used only to *rank* — restricting generation to salient lines would drop
    // constructions on lines whose endpoints never appear adjacently in a
    // predicate (e.g. `reflect a1 over line a2c2` when a2, c2 co-occur nowhere).
    let salient: FxHashSet<(PointId, PointId)> = salient_lines(problem).into_iter().collect();
    let all_lines: Vec<(PointId, PointId)> = (0..n as PointId)
        .flat_map(|i| ((i + 1)..n as PointId).map(move |j| (i, j)))
        .collect();

    // Feet of perpendiculars from every point onto every line.
    for p in 0..n as PointId {
        for &(i, j) in &all_lines {
            if p == i || p == j {
                continue;
            }
            let (a, b, pv) = (c(i), c(j), c(p));
            let ab = b - a;
            let foot = a + ab * ((pv - a).dot(ab) / ab.dot(ab));
            let preds = vec![
                pred("coll", vec![i, j, new_id]),
                pred("perp", vec![p, new_id, i, j]),
            ];
            try_push(
                &mut out,
                &mut seen,
                foot,
                preds,
                format!("foot({} -> {}{})", nm(p), nm(i), nm(j)),
                Kind::Foot,
                vec![p, i, j],
            );
        }
    }

    // Reflections of p over a point q.
    for p in 0..n as PointId {
        for q in 0..n as PointId {
            if p == q {
                continue;
            }
            let coord = c(q) * 2.0 - c(p);
            let preds = vec![
                pred("coll", vec![p, q, new_id]),
                pred("cong", vec![p, q, q, new_id]),
            ];
            try_push(
                &mut out,
                &mut seen,
                coord,
                preds,
                format!("reflect({} over {})", nm(p), nm(q)),
                Kind::ReflectPoint,
                vec![p, q],
            );
        }
    }

    // Reflections of every point over every line.
    for p in 0..n as PointId {
        for &(i, j) in &all_lines {
            if p == i || p == j {
                continue;
            }
            let (a, b, pv) = (c(i), c(j), c(p));
            let ab = b - a;
            let foot = a + ab * ((pv - a).dot(ab) / ab.dot(ab));
            let coord = foot * 2.0 - pv;
            let preds = vec![
                pred("cong", vec![i, p, i, new_id]),
                pred("cong", vec![j, p, j, new_id]),
            ];
            try_push(
                &mut out,
                &mut seen,
                coord,
                preds,
                format!("reflect({} over {}{})", nm(p), nm(i), nm(j)),
                Kind::ReflectLine,
                vec![p, i, j],
            );
        }
    }

    // Triangles of the figure (all three sides salient) and the salient-line
    // list — shared by several construction families below.
    let sp = |a: PointId, b: PointId| salient.contains(&if a < b { (a, b) } else { (b, a) });
    let salient_vec: Vec<(PointId, PointId)> = salient.iter().copied().collect();
    let mut salient_tris: Vec<(PointId, PointId, PointId)> = Vec::new();
    for x in 0..n as PointId {
        for y in (x + 1)..n as PointId {
            for z in (y + 1)..n as PointId {
                if sp(x, y) && sp(x, z) && sp(y, z) {
                    salient_tris.push((x, y, z));
                }
            }
        }
    }

    // Antipodes on circumcircles of figure triangles: concyclic with the
    // triangle plus right angles at the other two vertices (angle in a
    // semicircle) — hands the angle chase a diameter.
    for &(x, y, z) in &salient_tris {
        if let Some(circ) = NumCircle::through(c(x), c(y), c(z)) {
            for (v, p, q) in [(x, y, z), (y, x, z), (z, x, y)] {
                let coord = circ.center * 2.0 - c(v);
                let preds = vec![
                    pred("cyclic", vec![x, y, z, new_id]),
                    pred("perp", vec![p, v, p, new_id]),
                    pred("perp", vec![q, v, q, new_id]),
                ];
                try_push(
                    &mut out,
                    &mut seen,
                    coord,
                    preds,
                    format!("antipode({} in ({},{},{}))", nm(v), nm(x), nm(y), nm(z)),
                    Kind::Antipode,
                    vec![v, x, y, z],
                );
            }
        }
    }
    // ... and on circles named by the hypotheses, for every point on them.
    for &(o, a) in &known_circles(problem) {
        for p in hypothesis_circle_members(problem, o, a) {
            let coord = c(o) * 2.0 - c(p);
            let preds = vec![
                pred("cong", vec![o, new_id, o, a]),
                pred("coll", vec![p, o, new_id]),
                pred("cong", vec![p, o, o, new_id]),
            ];
            try_push(
                &mut out,
                &mut seen,
                coord,
                preds,
                format!("antipode({} on circle({},{}))", nm(p), nm(o), nm(a)),
                Kind::Antipode,
                vec![p, o, a],
            );
        }
    }

    if full {
        // Fourth vertices completing a parallelogram on each point triple
        // (midpoint transfers, equal+parallel segment transport).
        for i in 0..n as PointId {
            for k in (i + 1)..n as PointId {
                for j in 0..n as PointId {
                    if j == i || j == k {
                        continue;
                    }
                    let (vi, vj, vk) = (c(i), c(j), c(k));
                    let cr = (vj - vi).x * (vk - vi).y - (vj - vi).y * (vk - vi).x;
                    if cr.abs() < 1e-9 {
                        continue; // collinear triple: degenerate parallelogram
                    }
                    let coord = vi - vj + vk;
                    let preds = vec![
                        pred("para", vec![i, j, k, new_id]),
                        pred("para", vec![j, k, i, new_id]),
                        pred("cong", vec![i, j, k, new_id]),
                        pred("cong", vec![j, k, i, new_id]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("parallelogram({},{},{})", nm(i), nm(j), nm(k)),
                        Kind::Parallelogram,
                        vec![i, j, k],
                    );
                }
            }
        }

        // Incircle/excircle touch points on the sides of figure triangles,
        // pinned by the tangent-length identity as an additive distseq fact.
        for &(x, y, z) in &salient_tris {
            for (v, p, q) in [(x, y, z), (y, x, z), (z, x, y)] {
                let (vp, vq, vv) = (c(p), c(q), c(v));
                let side = distance(vp, vq);
                if side < 1e-9 {
                    continue;
                }
                for (label, t, konsts) in [
                    (
                        "incircle_touch",
                        (distance(vp, vv) + side - distance(vq, vv)) / 2.0,
                        [2i64, -1, -1, 1],
                    ),
                    (
                        "excircle_touch",
                        (side + distance(vq, vv) - distance(vp, vv)) / 2.0,
                        [2, 1, -1, -1],
                    ),
                ] {
                    let coord = vp + (vq - vp) * (t / side);
                    let preds = vec![
                        pred("coll", vec![p, q, new_id]),
                        Predicate {
                            name: "distseq".to_string(),
                            points: vec![p, new_id, p, v, p, q, q, v],
                            constants: konsts.iter().map(|&k| Rat::from_int(k)).collect(),
                        },
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("{label}(opp {} in {}{}{})", nm(v), nm(x), nm(y), nm(z)),
                        Kind::IncircleTouch,
                        vec![v, p, q],
                    );
                }
            }
        }

        // Centres of the spiral similarity taking one salient segment to
        // another — the Miquel-point workhorse: △XAB ~ △XCD directly, which
        // the eqangle/eqratio pairs express.
        let cmul = |u: Vec2, v: Vec2| Vec2::new(u.x * v.x - u.y * v.y, u.x * v.y + u.y * v.x);
        let cdiv = |u: Vec2, v: Vec2| {
            let d = v.x * v.x + v.y * v.y;
            Vec2::new((u.x * v.x + u.y * v.y) / d, (u.y * v.x - u.x * v.y) / d)
        };
        for si in 0..salient_vec.len() {
            for sj in (si + 1)..salient_vec.len() {
                let (a, b) = salient_vec[si];
                for (cc, dd) in [salient_vec[sj], (salient_vec[sj].1, salient_vec[sj].0)] {
                    let (va, vb, vc, vd) = (c(a), c(b), c(cc), c(dd));
                    let denom = vb - va;
                    if denom.norm() < 1e-9 {
                        continue;
                    }
                    let w = cdiv(vd - vc, denom);
                    let one_minus_w = Vec2::new(1.0 - w.x, -w.y);
                    if one_minus_w.norm() < 1e-6 {
                        continue; // a translation: no finite centre
                    }
                    let coord = cdiv(vc - cmul(w, va), one_minus_w);
                    let preds = vec![
                        pred("eqangle", vec![new_id, a, new_id, cc, new_id, b, new_id, dd]),
                        pred("eqangle", vec![new_id, a, new_id, b, new_id, cc, new_id, dd]),
                        pred("eqratio", vec![new_id, a, new_id, cc, new_id, b, new_id, dd]),
                        pred("eqratio", vec![new_id, a, new_id, b, new_id, cc, new_id, dd]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("spiral_center({}{} -> {}{})", nm(a), nm(b), nm(cc), nm(dd)),
                        Kind::SpiralCenter,
                        vec![a, b, cc, dd],
                    );
                }
            }
        }

        // Isogonal conjugates of existing points w.r.t. figure triangles.
        for &(x, y, z) in &salient_tris {
            let (vx, vy, vz) = (c(x), c(y), c(z));
            let cr2 = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
            let (a2, b2, c2) = (
                distance(vy, vz).powi(2),
                distance(vx, vz).powi(2),
                distance(vx, vy).powi(2),
            );
            for p in 0..n as PointId {
                if p == x || p == y || p == z {
                    continue;
                }
                let vp = c(p);
                let (bu, bv, bw) = (
                    cr2(vy - vp, vz - vp),
                    cr2(vz - vp, vx - vp),
                    cr2(vx - vp, vy - vp),
                );
                if bu.abs() < 1e-6 || bv.abs() < 1e-6 || bw.abs() < 1e-6 {
                    continue; // on a side line: conjugate degenerates
                }
                let (iu, iv, iw) = (a2 / bu, b2 / bv, c2 / bw);
                let ssum = iu + iv + iw;
                if ssum.abs() < 1e-9 {
                    continue; // on the circumcircle: conjugate at infinity
                }
                let coord = (vx * iu + vy * iv + vz * iw) * (1.0 / ssum);
                let preds = vec![
                    pred("eqangle", vec![x, y, x, p, x, new_id, x, z]),
                    pred("eqangle", vec![y, x, y, p, y, new_id, y, z]),
                    pred("eqangle", vec![z, x, z, p, z, new_id, z, y]),
                ];
                try_push(
                    &mut out,
                    &mut seen,
                    coord,
                    preds,
                    format!("isogonal({} in {}{}{})", nm(p), nm(x), nm(y), nm(z)),
                    Kind::IsogonalConjugate,
                    vec![p, x, y, z],
                );
            }
        }

        // Inverses of points in named circles (|OX|·|OP| = r², as a distmeq
        // product fact plus collinearity with the centre).
        for &(o, a) in &known_circles(problem) {
            let (vo, r) = (c(o), distance(c(o), c(a)));
            for p in 0..n as PointId {
                if p == o {
                    continue;
                }
                let d = distance(c(p), vo);
                if d < 1e-9 || (d - r).abs() < 1e-6 {
                    continue; // centre or on-circle: inverse is itself/undefined
                }
                let coord = vo + (c(p) - vo) * (r * r / (d * d));
                let preds = vec![
                    pred("coll", vec![o, p, new_id]),
                    Predicate {
                        name: "distmeq".to_string(),
                        points: vec![o, new_id, o, p, o, a, o, a],
                        constants: vec![
                            Rat::from_int(1),
                            Rat::from_int(1),
                            Rat::from_int(-1),
                            Rat::from_int(-1),
                            Rat::from_int(1),
                        ],
                    },
                ];
                try_push(
                    &mut out,
                    &mut seen,
                    coord,
                    preds,
                    format!("inverse({} in circle({},{}))", nm(p), nm(o), nm(a)),
                    Kind::InversePoint,
                    vec![p, o, a],
                );
            }
        }

        // Poles of chords of named circles: the tangents at the two endpoints
        // meet there (perpendicular to the radii, equidistant from both).
        for &(o, a) in &known_circles(problem) {
            let (vo, r) = (c(o), distance(c(o), c(a)));
            let on_circle = hypothesis_circle_members(problem, o, a);
            for pi in 0..on_circle.len() {
                for qi in (pi + 1)..on_circle.len() {
                    let (p, q) = (on_circle[pi], on_circle[qi]);
                    let m = midpoint(c(p), c(q));
                    let d2 = (m - vo).x * (m - vo).x + (m - vo).y * (m - vo).y;
                    if d2 < 1e-12 {
                        continue; // diameter: pole at infinity
                    }
                    let coord = vo + (m - vo) * (r * r / d2);
                    let preds = vec![
                        pred("perp", vec![new_id, p, o, p]),
                        pred("perp", vec![new_id, q, o, q]),
                        pred("cong", vec![new_id, p, new_id, q]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("pole({}{} of circle({},{}))", nm(p), nm(q), nm(o), nm(a)),
                        Kind::PoleOfChord,
                        vec![p, q, o, a],
                    );
                }
            }
        }

        // Line/circle intersections: every line against each circle named by
        // the hypotheses. Existing points are excluded by `try_push`, so this
        // yields *second intersections* (a cevian meeting the circumcircle
        // again) directly.
        for &(o, a) in &known_circles(problem) {
            let r = distance(c(o), c(a));
            for &(i, j) in &all_lines {
                let d = c(j) - c(i);
                let f = c(i) - c(o);
                let aa = d.dot(d);
                let bb = 2.0 * f.dot(d);
                let cc = f.dot(f) - r * r;
                let disc = bb * bb - 4.0 * aa * cc;
                if disc <= 1e-12 {
                    continue;
                }
                let sq = disc.sqrt();
                for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
                    let coord = c(i) + d * t;
                    let preds = vec![
                        pred("coll", vec![i, j, new_id]),
                        pred("cong", vec![o, new_id, o, a]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("intersect({}{}, circle({},{}))", nm(i), nm(j), nm(o), nm(a)),
                        Kind::IntersectLC,
                        vec![i, j, o, a],
                    );
                }
            }
        }

        // Line / circumcircle-of-a-triangle intersections. For every triple of
        // points whose three sides are all salient (a "triangle of the figure")
        // and every salient line, add the second intersection(s) of the line
        // with that triangle's circumcircle. `try_push` drops any intersection
        // coinciding with an existing vertex, so a cevian through a vertex
        // yields its *other* meeting with the circumcircle directly. This lets
        // the search discover the circumcircle itself — it need not be named by
        // a hypothesis — which is the crux of problems like IMO 2019 P2.
        for x in 0..n as PointId {
            for y in (x + 1)..n as PointId {
                for z in (y + 1)..n as PointId {
                    if !(sp(x, y) && sp(x, z) && sp(y, z)) {
                        continue;
                    }
                    let Some(circ) = NumCircle::through(c(x), c(y), c(z)) else {
                        continue;
                    };
                    let (o, r) = (circ.center, circ.r);
                    for &(i, j) in &salient_vec {
                        let d = c(j) - c(i);
                        let f = c(i) - o;
                        let aa = d.dot(d);
                        if aa < 1e-18 {
                            continue;
                        }
                        let bb = 2.0 * f.dot(d);
                        let cc = f.dot(f) - r * r;
                        let disc = bb * bb - 4.0 * aa * cc;
                        if disc <= 1e-12 {
                            continue;
                        }
                        let sq = disc.sqrt();
                        for t in [(-bb - sq) / (2.0 * aa), (-bb + sq) / (2.0 * aa)] {
                            let coord = c(i) + d * t;
                            let preds = vec![
                                pred("coll", vec![i, j, new_id]),
                                pred("cyclic", vec![x, y, z, new_id]),
                            ];
                            try_push(
                                &mut out,
                                &mut seen,
                                coord,
                                preds,
                                format!(
                                    "intersect({}{}, circumcircle({},{},{}))",
                                    nm(i),
                                    nm(j),
                                    nm(x),
                                    nm(y),
                                    nm(z)
                                ),
                                Kind::IntersectLCircum,
                                vec![i, j, x, y, z],
                            );
                        }
                    }
                }
            }
        }

        // Tangency points of the tangents drawn from a point to a named circle.
        for &(o, a) in &known_circles(problem) {
            let (vo, r) = (c(o), distance(c(o), c(a)));
            for pp in 0..n as PointId {
                if pp == o {
                    continue;
                }
                let vp = c(pp);
                let d = distance(vp, vo);
                if d <= r + 1e-9 {
                    continue; // point on/inside the circle: no tangent
                }
                let alpha = (r / d).acos();
                let base = (vp - vo).normalize();
                for sgn in [1.0f64, -1.0] {
                    let (sn, cs) = (alpha * sgn).sin_cos();
                    let dir = Vec2::new(base.x * cs - base.y * sn, base.x * sn + base.y * cs);
                    let t = vo + dir * r;
                    let preds = vec![
                        pred("cong", vec![o, new_id, o, a]),
                        pred("perp", vec![o, new_id, new_id, pp]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        t,
                        preds,
                        format!("tangent({} to circle({},{}))", nm(pp), nm(o), nm(a)),
                        Kind::TangentPoint,
                        vec![pp, o, a],
                    );
                }
            }
        }

        // Circle/circle intersections: every pair among the named circles and
        // the circumcircles of the figure's triangles (radical points — Miquel
        // points, second intersections of two circles, …).
        enum Cir {
            Known(PointId, PointId),
            Circum(PointId, PointId, PointId),
        }
        let sp2 = |a: PointId, b: PointId| salient.contains(&if a < b { (a, b) } else { (b, a) });
        let mut circles: Vec<(Vec2, f64, Cir)> = Vec::new();
        for &(o, a) in &known_circles(problem) {
            circles.push((c(o), distance(c(o), c(a)), Cir::Known(o, a)));
        }
        for x in 0..n as PointId {
            for y in (x + 1)..n as PointId {
                for z in (y + 1)..n as PointId {
                    if sp2(x, y) && sp2(x, z) && sp2(y, z) {
                        if let Some(cc) = NumCircle::through(c(x), c(y), c(z)) {
                            circles.push((cc.center, cc.r, Cir::Circum(x, y, z)));
                        }
                    }
                }
            }
        }
        let on_pred = |cir: &Cir, id: PointId| match *cir {
            Cir::Known(o, a) => pred("cong", vec![o, id, o, a]),
            Cir::Circum(x, y, z) => pred("cyclic", vec![x, y, z, id]),
        };
        let cir_args = |cir: &Cir| match *cir {
            Cir::Known(o, a) => vec![o, a],
            Cir::Circum(x, y, z) => vec![x, y, z],
        };
        let cir_name = |cir: &Cir| match *cir {
            Cir::Known(o, a) => format!("circle({},{})", nm(o), nm(a)),
            Cir::Circum(x, y, z) => format!("circumcircle({},{},{})", nm(x), nm(y), nm(z)),
        };
        for a in 0..circles.len() {
            for b in (a + 1)..circles.len() {
                let (c1, r1) = (circles[a].0, circles[a].1);
                let (c2, r2) = (circles[b].0, circles[b].1);
                let dvec = c2 - c1;
                let d = dvec.norm();
                if d < 1e-9 || d > r1 + r2 || d < (r1 - r2).abs() {
                    continue; // concentric or non-intersecting
                }
                let t = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
                let h2 = r1 * r1 - t * t;
                if h2 < 0.0 {
                    continue;
                }
                let h = h2.sqrt();
                let mid = c1 + dvec * (t / d);
                let perp = Vec2::new(-dvec.y, dvec.x) * (1.0 / d);
                let (k1, k2) = (&circles[a].2, &circles[b].2);
                let mut args = cir_args(k1);
                args.extend(cir_args(k2));
                for sgn in [1.0f64, -1.0] {
                    let coord = mid + perp * (h * sgn);
                    let preds = vec![on_pred(k1, new_id), on_pred(k2, new_id)];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("intersect({}, {})", cir_name(k1), cir_name(k2)),
                        Kind::CircleCircle,
                        args.clone(),
                    );
                }
            }
        }

        // Arc midpoints: for each circle and each pair of figure points on it,
        // the two points of the circle equidistant from the pair (the midpoints
        // of the two arcs) — the incentre–excentre lemma's workhorse.
        for (center, r, cir) in &circles {
            let on_c: Vec<PointId> = (0..n as PointId)
                .filter(|&p| (distance(c(p), *center) - r).abs() < 1e-7)
                .collect();
            for xi in 0..on_c.len() {
                for yi in (xi + 1)..on_c.len() {
                    let (x, y) = (on_c[xi], on_c[yi]);
                    let mid_xy = midpoint(c(x), c(y));
                    let dir = mid_xy - *center;
                    let d = dir.norm();
                    // The chord's perpendicular bisector through the centre.
                    let u = if d > 1e-9 {
                        dir * (1.0 / d)
                    } else {
                        // x, y antipodal: any perpendicular to xy.
                        let ch = (c(y) - c(x)).normalize();
                        Vec2::new(-ch.y, ch.x)
                    };
                    for sgn in [1.0f64, -1.0] {
                        let coord = *center + u * (*r * sgn);
                        let preds = vec![
                            on_pred(cir, new_id),
                            pred("cong", vec![new_id, x, new_id, y]),
                        ];
                        let mut args = cir_args(cir);
                        args.extend([x, y]);
                        try_push(
                            &mut out,
                            &mut seen,
                            coord,
                            preds,
                            format!("arc_midpoint({},{} on {})", nm(x), nm(y), cir_name(cir)),
                            Kind::ArcMidpoint,
                            args,
                        );
                    }
                }
            }
        }

        // Centres of similitude (homothety centres) of two circles whose
        // centres are existing points: on the line of centres, dividing it in
        // the ratio of the radii — where common tangents meet (Monge).
        let centered: Vec<(PointId, PointId)> = known_circles(problem);
        for a in 0..centered.len() {
            for b in (a + 1)..centered.len() {
                let (o1, a1) = centered[a];
                let (o2, a2) = centered[b];
                if o1 == o2 {
                    continue; // concentric: no similitude centre
                }
                let (v1, v2) = (c(o1), c(o2));
                let (r1, r2) = (distance(v1, c(a1)), distance(v2, c(a2)));
                // External centre divides O1O2 externally in r1:r2 (undefined
                // for equal radii); internal divides it internally.
                let mut cands: Vec<Vec2> = vec![(v1 * r2 + v2 * r1) * (1.0 / (r1 + r2))];
                if (r1 - r2).abs() > 1e-9 {
                    cands.push((v2 * r1 - v1 * r2) * (1.0 / (r1 - r2)));
                }
                for coord in cands {
                    let preds = vec![
                        pred("coll", vec![o1, o2, new_id]),
                        // |Z o1| : |Z o2| = r1 : r2
                        pred("eqratio", vec![new_id, o1, new_id, o2, o1, a1, o2, a2]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!(
                            "similitude_center(circle({},{}), circle({},{}))",
                            nm(o1),
                            nm(a1),
                            nm(o2),
                            nm(a2)
                        ),
                        Kind::HomothetyCenter,
                        vec![o1, a1, o2, a2],
                    );
                }
            }
        }

        // Line/line intersections of all line pairs.
        for a in 0..all_lines.len() {
            for b in (a + 1)..all_lines.len() {
                let (i, j) = all_lines[a];
                let (k, l) = all_lines[b];
                if i == k || i == l || j == k || j == l {
                    continue;
                }
                if let Some(coord) =
                    intersect_ll(&NumLine::through(c(i), c(j)), &NumLine::through(c(k), c(l)))
                {
                    let preds = vec![
                        pred("coll", vec![i, j, new_id]),
                        pred("coll", vec![k, l, new_id]),
                    ];
                    try_push(
                        &mut out,
                        &mut seen,
                        coord,
                        preds,
                        format!("intersect({}{}, {}{})", nm(i), nm(j), nm(k), nm(l)),
                        Kind::IntersectLL,
                        vec![i, j, k, l],
                    );
                }
            }
        }
    }

    // Rank by a hand-tuned heuristic (kind prior + goal-relevance + salience +
    // the cevian∩circumcircle signal). A learned neural ranker was built and
    // GPU-trained but never beat this heuristic in rigorous A/B testing — the
    // key patterns (notably the cevian case behind IMO 2019 P2) are encoded
    // here directly, where there is no learned goal-relevance bias to fight — so
    // the ML path was removed and the heuristic is the single source of ranking.
    let ctx = RankContext::new(problem);
    let mut scored: Vec<(f64, Construction)> = out
        .into_iter()
        .map(|cand| (heuristic_score(problem, &cand, &ctx), cand))
        .collect();
    scored.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    scored.into_iter().map(|(_, cand)| cand).collect()
}

/// Precomputed, per-problem context shared by the ranking heuristic (built once
/// per `candidates` call, not per candidate).
pub struct RankContext {
    goal_pts: FxHashSet<PointId>,
    nbr: FxHashSet<PointId>,
    salient: FxHashSet<(PointId, PointId)>,
    deg: Vec<f64>,
    max_deg: f64,
    goal_coords: Vec<Vec2>,
}

impl RankContext {
    pub fn new(problem: &Problem) -> RankContext {
        let goal_pts: FxHashSet<PointId> = problem
            .goal
            .as_ref()
            .map(|g| g.points.iter().copied().collect())
            .unwrap_or_default();
        let nbr = goal_neighborhood(problem);
        let salient: FxHashSet<(PointId, PointId)> = salient_lines(problem).into_iter().collect();
        let deg: Vec<f64> = (0..problem.points.len())
            .map(|i| {
                problem
                    .preds
                    .iter()
                    .filter(|p| p.points.contains(&(i as PointId)))
                    .count() as f64
            })
            .collect();
        let max_deg = deg.iter().cloned().fold(1.0, f64::max);
        let goal_coords: Vec<Vec2> = problem
            .goal
            .as_ref()
            .map(|g| {
                g.points
                    .iter()
                    .map(|&p| problem.points[p as usize].value)
                    .collect()
            })
            .unwrap_or_default();
        RankContext {
            goal_pts,
            nbr,
            salient,
            deg,
            max_deg,
            goal_coords,
        }
    }
    fn rel_of(&self, p: PointId) -> f64 {
        if self.goal_pts.contains(&p) {
            1.0
        } else if self.nbr.contains(&p) {
            0.5
        } else {
            0.0
        }
    }
    fn is_salient(&self, a: PointId, b: PointId) -> bool {
        self.salient.contains(&if a < b { (a, b) } else { (b, a) })
    }
    fn chord_through_goal(&self, problem: &Problem, i: PointId, j: PointId) -> bool {
        let l = NumLine::through(
            problem.points[i as usize].value,
            problem.points[j as usize].value,
        );
        self.goal_coords.iter().any(|&g| l.distance(g) < 1e-6)
    }
}

fn salient_bonus(cand: &Construction, ctx: &RankContext) -> f64 {
    let a = &cand.args;
    let s = |x: PointId, y: PointId| if ctx.is_salient(x, y) { 1.5 } else { 0.0 };
    let tri = |x: PointId, y: PointId, z: PointId| {
        0.75 * [(x, y), (x, z), (y, z)]
            .iter()
            .filter(|&&(p, q)| ctx.is_salient(p, q))
            .count() as f64
    };
    match cand.kind {
        Kind::Foot | Kind::ReflectLine | Kind::BisectorFoot if a.len() >= 3 => s(a[1], a[2]),
        Kind::IntersectLC | Kind::IntersectLCircum | Kind::Midpoint | Kind::ReflectPoint
            if a.len() >= 2 =>
        {
            s(a[0], a[1])
        }
        Kind::IntersectLL if a.len() >= 4 => s(a[0], a[1]) + s(a[2], a[3]),
        Kind::Orthocenter | Kind::BisectorCenter if a.len() >= 3 => tri(a[0], a[1], a[2]),
        _ => 0.0,
    }
}

/// The hand-tuned ranking score: kind prior + goal-relevance + salience, plus
/// the cevian∩circumcircle signal that makes the hard second-intersection
/// auxiliaries (IMO 2019 P2) rank at the top.
fn heuristic_score(problem: &Problem, cand: &Construction, ctx: &RankContext) -> f64 {
    if cand.kind == Kind::IntersectLCircum && cand.args.len() >= 5 {
        let a = &cand.args; // [i, j, x, y, z]
        let line_rel = (ctx.rel_of(a[0]) + ctx.rel_of(a[1])) / 2.0;
        let hub = ctx.deg[a[2] as usize] + ctx.deg[a[3] as usize] + ctx.deg[a[4] as usize];
        let chord = if ctx.chord_through_goal(problem, a[0], a[1]) {
            2.0
        } else {
            0.0
        };
        // A *cevian* — a line through a high-degree triangle vertex that also
        // meets the goal — is the classic "second intersection with the
        // circumcircle" auxiliary (IMO 2019 P2). Reward the line touching a hub
        // vertex, so cevians outrank interior chords (e.g. a chord joining two
        // goal points), which goal-relevance alone would over-rank.
        let line_hub = ctx.deg[a[0] as usize].max(ctx.deg[a[1] as usize]) / ctx.max_deg;
        return kind_prior(cand.kind)
            + line_rel
            + 0.3 * hub
            + chord
            + salient_bonus(cand, ctx)
            + 3.0 * line_hub;
    }
    let rel = cand.args.iter().map(|&p| ctx.rel_of(p)).sum::<f64>() / cand.args.len().max(1) as f64;
    kind_prior(cand.kind) + 2.0 * rel + salient_bonus(cand, ctx)
}

fn past(deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| Instant::now() >= d)
}

fn try_solve(
    problem: &Problem,
    depth: usize,
    runs: &mut usize,
    max_runs: usize,
    full: bool,
    verbose: bool,
    deadline: Option<Instant>,
) -> Option<Vec<Construction>> {
    if past(deadline) {
        return None;
    }
    *runs += 1;
    let warm = if depth == 1 && warm_enabled() {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| WarmBase::new(problem)))
            .ok()
            .flatten()
    } else {
        None
    };
    let solved = match &warm {
        Some(w) => w.proves_goal(),
        None => safe_solve(problem),
    };
    if solved {
        return Some(Vec::new());
    }
    if depth == 0 || *runs >= max_runs || past(deadline) {
        return None;
    }
    let cands = candidates(problem, full);
    if verbose {
        eprintln!(
            "  depth {depth}: trying {} candidates ({} runs so far)",
            cands.len(),
            runs
        );
    }

    // The bottom level fans candidate DDAR runs across cores with rayon's
    // `find_first`, which returns the *sequentially first* success in the ranked
    // order — so the chosen construction is identical to a serial search, only
    // faster. (Data parallelism, not async/await: a prover is CPU-bound; there
    // is no IO to overlap.)
    //
    // The budget is applied by taking a fixed ranked *prefix* before the sweep,
    // NOT by an in-closure counter: a shared atomic guard would let thread
    // scheduling (rather than rank) decide which candidates are skipped, making
    // `find_first` nondeterministic and able to miss a solvable candidate. Over
    // a fixed prefix, `find_first` is deterministic and matches a serial search
    // bounded by the same budget.
    if depth == 1 && cands.len() > 8 {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let take = (max_runs - *runs).min(cands.len());
        let attempted = AtomicUsize::new(0);
        let found = cands[..take].par_iter().find_first(|cand| {
            if past(deadline) {
                return false;
            }
            attempted.fetch_add(1, Ordering::Relaxed);
            match &warm {
                Some(w) => w.check(cand),
                None => safe_solve(&augment(problem, cand)),
            }
        });
        // `attempted` counts closures actually invoked (rayon may evaluate a few
        // past the winner in flight); it never exceeds the prefix length.
        *runs += attempted.load(Ordering::Relaxed).min(take);
        return found.map(|c| vec![c.clone()]);
    }

    // depth >= 2: try the top-ranked first constructions in rank order, each
    // with a bounded sub-budget. Every branch's *own* bottom-level sweep is
    // parallel (the depth-1 case above fans across all cores with rayon), so the
    // cores stay saturated; iterating branches in rank order keeps the run count
    // deterministic and stops as soon as the first-by-rank solution is found —
    // without the speculative over-evaluation a single flat parallel sweep
    // incurs. A per-branch cap prevents one mis-ranked lead from starving the
    // real answer.
    let branches = cands.len().min(64);
    let per_branch = ((max_runs - *runs) / branches.max(1)).max(1_500);
    for cand in cands.into_iter().take(branches) {
        if *runs >= max_runs || past(deadline) {
            break;
        }
        let aug = augment(problem, &cand);
        let sub_max = (*runs + per_branch).min(max_runs);
        if let Some(mut rest) = try_solve(&aug, depth - 1, runs, sub_max, full, verbose, deadline) {
            let mut v = vec![cand];
            v.append(&mut rest);
            return Some(v);
        }
    }
    None
}

/// Search for an auxiliary construction (up to `max_depth` points) that lets
/// DDAR prove the goal. Returns the proof plus how many DDAR runs it took.
///
/// Uses the *reduced* construction library on figures with more than 14 points
/// (a latency guard for interactive solves); [`solve_max`] always searches the
/// full library regardless of figure size.
pub fn solve_with_aux(
    problem: &Problem,
    max_depth: usize,
    max_runs: usize,
    verbose: bool,
) -> (Option<AuxProof>, SearchStats) {
    let full = problem.points.len() <= 14;
    solve_with_aux_opts(problem, max_depth, max_runs, verbose, full)
}

/// [`solve_with_aux`] with the construction-library breadth made explicit.
pub fn solve_with_aux_opts(
    problem: &Problem,
    max_depth: usize,
    max_runs: usize,
    verbose: bool,
    full: bool,
) -> (Option<AuxProof>, SearchStats) {
    solve_with_aux_until(problem, max_depth, max_runs, verbose, full, None)
}

/// [`solve_with_aux_opts`] with a wall-clock `deadline`: once it passes, no new
/// candidate DDAR run starts and the search returns `None` unless a proof was
/// already found. A run already in progress finishes, so a caller needing a
/// hard stop must also bound the process externally.
pub fn solve_with_aux_until(
    problem: &Problem,
    max_depth: usize,
    max_runs: usize,
    verbose: bool,
    full: bool,
    deadline: Option<Instant>,
) -> (Option<AuxProof>, SearchStats) {
    // A goal-less problem can never be proved; without this guard the search
    // would burn its entire run budget on unprovable attempts (e.g. after
    // `strip_point` removed a point the goal mentioned).
    if problem.goal.is_none() {
        return (None, SearchStats { runs: 0 });
    }

    let mut runs = 0usize;
    // Candidate runs are individually panic-caught inside `try_solve`; this
    // outer catch guards the search *machinery* itself (candidate generation,
    // warm-start bookkeeping, parallel harness). A machinery panic must not
    // kill a long-running search process — degrade to "no proof found".
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::quiet_panic::quiet(|| try_solve(problem, max_depth, &mut runs, max_runs, full, verbose, deadline))
    }))
    .unwrap_or_else(|_| {
        eprintln!(
            "warning: the auxiliary search aborted on an internal panic — \
             treating as `no proof found` (set DDAR_DEBUG_PANICS=1 for detail)"
        );
        None
    });

    (
        result.map(|constructions| AuxProof { constructions }),
        SearchStats { runs },
    )
}

/// **Universal MAX-power search.** Throws maximum effort at *any* problem,
/// making no assumption about how hard it is: it deepens the auxiliary-
/// construction search level by level (iterative deepening) with an escalating
/// per-level budget, saturating all CPU cores at every level, and simply
/// escalates until the goal is proved or the deepest budget is exhausted. Easy
/// problems finish in the first (shallow) pass; only genuinely deep ones ever
/// reach the expensive levels — so nothing is pre-judged.
///
/// The deepest level's budget can be raised without recompiling via
/// `AUX_MAX_RUNS` (default 2,000,000 DDAR runs), and the maximum depth via
/// `AUX_MAX_DEPTH` (default 3), for "as much power as possible".
pub fn solve_max(problem: &Problem, verbose: bool) -> (Option<AuxProof>, SearchStats) {
    if problem.goal.is_none() {
        return (None, SearchStats { runs: 0 });
    }
    let max_depth = env_usize("AUX_MAX_DEPTH", 3).max(1);
    let deep_budget = env_usize("AUX_MAX_RUNS", 2_000_000).max(10_000);
    let mut runs = 0usize;
    // Depth d searches every 0..=d-aux solution; running d = 2, 3, … in order is
    // classic iterative deepening (the shallow re-exploration is cheap relative
    // to the deepest pass). The budget grows with depth so deep searches get the
    // room they need without letting a shallow pass run away.
    for depth in 2..=max_depth {
        let budget = if depth == max_depth {
            deep_budget
        } else {
            // min() before max() so a small operator-set AUX_MAX_RUNS cannot
            // produce clamp(min > max), which panics.
            (deep_budget / 8).max(200_000).min(deep_budget)
        };
        if verbose {
            eprintln!(
                "[MAX] depth {depth}, budget {budget} DDAR runs, {} cores",
                rayon::current_num_threads()
            );
        }
        // MAX means maximum: never the reduced library, whatever the figure
        // size — a 17-point IMO configuration needs the circle constructions
        // most of all.
        let (res, stats) = solve_with_aux_opts(problem, depth, budget, verbose, true);
        runs += stats.runs;
        if res.is_some() {
            return (res, SearchStats { runs });
        }
    }
    (None, SearchStats { runs })
}

/// [`solve_max`] bounded by a wall-clock `deadline` and complete at depth 1: it
/// first sweeps every single-construction candidate, then deepens to
/// 2..=`AUX_MAX_DEPTH` with [`solve_max`]'s budgets.
pub fn solve_max_until(
    problem: &Problem,
    verbose: bool,
    deadline: Option<Instant>,
) -> (Option<AuxProof>, SearchStats) {
    if problem.goal.is_none() {
        return (None, SearchStats { runs: 0 });
    }
    let max_depth = env_usize("AUX_MAX_DEPTH", 3).max(1);
    let deep_budget = env_usize("AUX_MAX_RUNS", 2_000_000).max(10_000);
    let mut runs = 0usize;
    for depth in 1..=max_depth {
        if past(deadline) {
            break;
        }
        let budget = if depth == 1 || depth == max_depth {
            deep_budget
        } else {
            (deep_budget / 8).max(200_000).min(deep_budget)
        };
        let (res, stats) = solve_with_aux_until(problem, depth, budget, verbose, true, deadline);
        runs += stats.runs;
        if res.is_some() {
            return (res, SearchStats { runs });
        }
    }
    (None, SearchStats { runs })
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Remove a point (and every predicate mentioning it) from a problem, remapping
/// the remaining point ids. Used to construct aux-search test cases from
/// problems whose auxiliary points are already provided.
pub fn strip_point(problem: &Problem, name: &str) -> Problem {
    let remove_id = problem
        .points
        .iter()
        .position(|p| p.name == name)
        .expect("no such point") as PointId;

    let mut map: Vec<Option<PointId>> = vec![None; problem.points.len()];
    let mut new_points: Vec<Point> = Vec::new();
    for (i, p) in problem.points.iter().enumerate() {
        if i as PointId == remove_id {
            continue;
        }
        map[i] = Some(new_points.len() as PointId);
        new_points.push(p.clone());
    }

    let remap = |p: &Predicate| -> Option<Predicate> {
        if p.points.contains(&remove_id) {
            return None;
        }
        Some(Predicate {
            name: p.name.clone(),
            points: p.points.iter().map(|&x| map[x as usize].unwrap()).collect(),
            constants: p.constants.clone(),
        })
    };

    Problem {
        points: new_points,
        preds: problem.preds.iter().filter_map(remap).collect(),
        goal: problem.goal.as_ref().and_then(remap),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Every predicate a candidate construction emits must hold numerically in
    /// the augmented figure — this is the soundness contract of the library
    /// (a false defining predicate would let DDAR "prove" false statements).
    /// Exercises all kinds, including the circle-based ones, on a rich figure.
    #[test]
    fn all_candidate_predicates_hold_numerically() {
        let compiled = crate::geo::compile(
            "A B C = triangle\nO = circumcenter(A, B, C)\nH = orthocenter(A, B, C)\nM = midpoint(B, C)\nD = on_circle(O, A)\nprove coll(A, A, B)",
        )
        .expect("compile");
        let problem = compiled.problem;
        let cands = candidates(&problem, true);
        // The new families must actually generate on this figure.
        for kind in [
            Kind::Antipode,
            Kind::Parallelogram,
            Kind::IncircleTouch,
            Kind::SpiralCenter,
            Kind::IsogonalConjugate,
            Kind::InversePoint,
            Kind::PoleOfChord,
        ] {
            assert!(
                cands.iter().any(|cd| cd.kind == kind),
                "no candidates generated for {kind:?}"
            );
        }
        for cand in &cands {
            let aug = augment(&problem, cand);
            for p in &cand.preds {
                assert_ne!(
                    crate::geo::numeric_holds(&aug, p),
                    Some(false),
                    "candidate `{}` emits a numerically false predicate {} {:?}",
                    cand.desc,
                    p.name,
                    p.points
                );
            }
        }
    }
}
