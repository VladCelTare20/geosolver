//! Coincidence-ranked candidate pools and randomised multi-point rollouts.

use crate::aux_score::{defs_of, Scorer};
use crate::aux_search::{
    augment, candidates_ranked, double_key, env_usize, heuristic_score, natural_name, past,
    render_tpl, taken_names, AuxProof, Construction, Kind, RankContext, SearchStats, WarmBase,
};
use crate::aux_virtual::virtual_candidates;
use crate::numerics::{distance, Vec2};
use crate::predicate::PointId;
use crate::{Ddar, Problem};
use rayon::prelude::*;
use rustc_hash::{FxHashSet, FxHasher};
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const MAX_POINTS: usize = 4;
const CHILD_CAP: usize = 192;
const PARENTS: usize = 512;

pub(crate) struct Item {
    pub c: Construction,
    pub inc: f64,
    pub heur: f64,
}

pub(crate) struct Pool {
    pub items: Vec<Item>,
    cum: Vec<f64>,
}

fn weight(it: &Item) -> f64 {
    (0.9 * it.inc.min(8.0) + 0.25 * it.heur.min(8.0)).exp()
}

impl Pool {
    fn tap(self, f: impl FnOnce(&Pool)) -> Pool {
        f(&self);
        self
    }

    fn new(mut items: Vec<Item>) -> Pool {
        items.sort_by(|a, b| {
            let ka = 2.0 * a.inc + a.heur;
            let kb = 2.0 * b.inc + b.heur;
            kb.partial_cmp(&ka).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut cum = Vec::with_capacity(items.len());
        let mut acc = 0.0;
        for it in &items {
            acc += weight(it);
            cum.push(acc);
        }
        Pool { items, cum }
    }

    fn sample(&self, rng: &mut Rng) -> Option<usize> {
        let total = *self.cum.last()?;
        let r = rng.f64() * total;
        Some(
            self.cum
                .partition_point(|&c| c <= r)
                .min(self.items.len() - 1),
        )
    }
}

/// Every candidate over `problem` (the classical library plus the new kinds
/// reaching `min_new` extra incidences), scored and ranked. With `must`, only
/// candidates built on that point.
pub(crate) fn build_pool(
    problem: &Problem,
    ddar: Option<&Ddar>,
    must: Option<PointId>,
    min_new: f64,
    parallel: bool,
) -> Pool {
    let n = problem.points.len();
    let pts: Vec<Vec2> = problem.points.iter().map(|p| p.value).collect();
    let goal: Vec<PointId> = problem
        .goal
        .as_ref()
        .map(|g| g.points.clone())
        .unwrap_or_default();
    let scorer = Scorer::new(&pts, &goal);
    let ctx = RankContext::new(problem);
    let new_id = n as PointId;

    let score_of = |h: f64, c: Construction| {
        let coord = |i: PointId| {
            if i == new_id {
                c.coord
            } else {
                pts[i as usize]
            }
        };
        let inc = scorer.score(c.coord, &defs_of(&c.preds, new_id, coord));
        Item { c, inc, heur: h }
    };
    let doubles = must.is_none() && ddar.is_some() && doubles_enabled();
    let on_point = |c: &Construction| (0..n).find(|&i| pts[i] == c.coord).map(|i| i as PointId);
    let old = candidates_ranked(problem, true, must, doubles);
    let (old_dbl, old): (Vec<_>, Vec<_>) =
        old.into_iter().partition(|(_, c)| on_point(c).is_some());
    let mut items: Vec<Item> = if parallel {
        old.into_par_iter().map(|(h, c)| score_of(h, c)).collect()
    } else {
        old.into_iter().map(|(h, c)| score_of(h, c)).collect()
    };
    let key = |c: &Construction| match on_point(c) {
        Some(p) => double_key(c.kind, p, &c.args),
        None => (
            c.kind,
            (c.coord.x * 1e6).round() as i64,
            (c.coord.y * 1e6).round() as i64,
        ),
    };
    let mut seen: FxHashSet<(Kind, i64, i64)> = items.iter().map(|it| key(&it.c)).collect();
    let mut dbl: Vec<(f64, Construction)> = old_dbl;
    for (s, c) in virtual_candidates(problem, ddar, &scorer, min_new, must, parallel, doubles) {
        if on_point(&c).is_some() {
            let heur = heuristic_score(problem, &c, &ctx);
            dbl.push((heur, c));
        } else if seen.insert(key(&c)) {
            let heur = heuristic_score(problem, &c, &ctx);
            items.push(Item { c, inc: s, heur });
        }
    }
    if let (true, Some(d)) = (doubles, ddar) {
        let mut d = d.clone();
        for (heur, c) in dbl {
            let p = on_point(&c).unwrap();
            if seen.insert(key(&c)) && double_point_ok(problem, &mut d, p, &c) {
                let inc = if goal.contains(&p) {
                    DOUBLE_INC + 2.0
                } else {
                    DOUBLE_INC
                };
                items.push(Item { c, inc, heur });
            }
        }
    }
    let mut pool = Pool::new(items);
    if must.is_some() && pool.items.len() > CHILD_CAP {
        pool.items.truncate(CHILD_CAP);
        pool.cum.truncate(CHILD_CAP);
        pool.items.shrink_to_fit();
        pool.cum.shrink_to_fit();
    }
    pool
}

/// Coincidence score given to a double point instead of [`Scorer::score`],
/// which would count every figure object through the point it doubles and
/// flood the sampler (+2 when that point is a goal point).
const DOUBLE_INC: f64 = 3.0;

/// `AUX_DOUBLES=0` turns double-point candidates off (A/B measurement).
fn doubles_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("AUX_DOUBLES").map_or(true, |v| v != "0"))
}

/// Whether `c`, snapped onto the existing point `p`, is a useful and honest
/// double point: no defining predicate takes a direction or length between
/// the new point and a point at `p` (that would be degenerate there — a
/// foot of `p` landing on `p`, say), every defining predicate holds
/// numerically, at least one holds of `p` in the base closure `d` (so a
/// proof can identify the two through a shared object, `Ddar::merge_points`),
/// and at least one does not (otherwise it is `p` itself). Only symbolic
/// merges ever identify the two points; this only filters and ranks.
fn double_point_ok(problem: &Problem, d: &mut Ddar, p: PointId, c: &Construction) -> bool {
    let new_id = problem.points.len() as PointId;
    let at_p = |i: PointId| i == new_id || problem.points[i as usize].value == c.coord;
    let degenerate = c.preds.iter().any(|q| {
        !matches!(q.name.as_str(), "coll" | "cyclic")
            && q.points.chunks(2).any(|w| w.len() == 2 && w[0] != w[1] && at_p(w[0]) && at_p(w[1]))
    });
    let aug = augment(problem, c);
    if degenerate
        || c.preds
            .iter()
            .any(|q| crate::geo::numeric_holds(&aug, q) != Some(true))
    {
        return false;
    }
    let mut symbolic = 0;
    for q in &c.preds {
        let on_p = crate::predicate::Predicate {
            name: q.name.clone(),
            points: q
                .points
                .iter()
                .map(|&i| if i == new_id { p } else { i })
                .collect(),
            constants: q.constants.clone(),
        };
        let by_definition = matches!(q.name.as_str(), "coll" | "cyclic")
            && q.points.iter().any(|&i| i != new_id && at_p(i));
        let holds = by_definition
            || std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.check_pred(&on_p)))
                .unwrap_or(false);
        symbolic += holds as usize;
    }
    symbolic >= 1 && symbolic < c.preds.len()
}

struct Rng(u64);

impl Rng {
    fn seeded(parts: impl Hash) -> Rng {
        let mut h = FxHasher::default();
        parts.hash(&mut h);
        Rng(h.finish())
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn f64(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Pick {
    Base(u32),
    Child(u32, u32),
}

struct Search<'a> {
    problem: &'a Problem,
    warm: &'a WarmBase,
    pool: Pool,
    children: Vec<OnceLock<Pool>>,
    visited: Mutex<FxHashSet<Vec<Pick>>>,
    runs: AtomicUsize,
    seed: u64,
    scale: f64,
    deadline: Option<Instant>,
    verbose: bool,
}

impl Search<'_> {
    fn n(&self) -> PointId {
        self.problem.points.len() as PointId
    }

    fn child_pool(&self, b: usize) -> &Pool {
        self.children[b].get_or_init(|| {
            if past(self.deadline) {
                return Pool::new(Vec::new());
            }
            let t = Instant::now();
            let aug = augment(self.problem, &self.pool.items[b].c);
            let n = self.n();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                build_pool(&aug, Some(self.warm.ddar()), Some(n), 1.0, false)
            }))
            .unwrap_or_else(|_| Pool::new(Vec::new()))
            .tap(|p| {
                if self.verbose {
                    eprintln!(
                        "[aux] child pool {b}: {} in {:.3}s",
                        p.items.len(),
                        t.elapsed().as_secs_f64()
                    );
                }
            })
        })
    }

    fn construction(&self, p: Pick) -> &Construction {
        match p {
            Pick::Base(b) => &self.pool.items[b as usize].c,
            Pick::Child(b, j) => &self.child_pool(b as usize).items[j as usize].c,
        }
    }

    /// The picks as constructions on consecutive new ids in the given order
    /// (every child after its parent).
    fn materialise(&self, picks: &[Pick]) -> Vec<Construction> {
        let n = self.n();
        let slot_of = |p: Pick| n + picks.iter().position(|&q| q == p).unwrap() as PointId;
        picks
            .iter()
            .map(|&p| {
                let me = slot_of(p);
                match p {
                    Pick::Base(_) => self.construction(p).remap(|i| if i == n { me } else { i }),
                    Pick::Child(b, _) => {
                        let parent = slot_of(Pick::Base(b));
                        self.construction(p).remap(|i| {
                            if i == n {
                                parent
                            } else if i == n + 1 {
                                me
                            } else {
                                i
                            }
                        })
                    }
                }
            })
            .collect()
    }

    fn check(&self, picks: &[Pick]) -> bool {
        self.runs.fetch_add(1, Ordering::Relaxed);
        self.warm.check_all(&self.materialise(picks), self.deadline)
    }

    fn sample_firsts(&self, round: u64, m: usize) -> Vec<u32> {
        let mut rng = Rng::seeded((self.seed, round, u64::MAX));
        let mut out: Vec<u32> = Vec::new();
        let mut tries = 0;
        while out.len() < m && tries < 8 * m {
            tries += 1;
            if let Some(i) = self.pool.sample(&mut rng) {
                if !out.contains(&(i as u32)) {
                    out.push(i as u32);
                }
            }
        }
        out
    }

    /// A rollout continuing `first`: one to three more points drawn by weight
    /// from the pool or from the children of points already drawn. `None` if
    /// the set was tried before.
    fn sample_rest(&self, round: u64, first: u32, idx: u64) -> Option<Vec<Pick>> {
        let mut rng = Rng::seeded((self.seed, round, first, idx));
        let r = rng.f64();
        let extra = if r < 0.6 {
            1
        } else if r < 0.9 {
            2
        } else {
            3
        };
        let mut picks: Vec<Pick> = vec![Pick::Base(first)];
        let mut coords: Vec<Vec2> = vec![self.pool.items[first as usize].c.coord];
        let mut tries = 0;
        while picks.len() < 1 + extra && tries < 6 * extra {
            tries += 1;
            let bases: Vec<u32> = picks
                .iter()
                .filter_map(|p| match p {
                    Pick::Base(b) => Some(*b),
                    _ => None,
                })
                .collect();
            let parents: Vec<u32> = bases
                .iter()
                .copied()
                .filter(|&b| (b as usize) < PARENTS)
                .collect();
            let pick = if !parents.is_empty() && rng.f64() < 0.4 {
                let b = parents[(rng.next() % parents.len() as u64) as usize];
                match self.child_pool(b as usize).sample(&mut rng) {
                    Some(j) => Pick::Child(b, j as u32),
                    None => continue,
                }
            } else {
                match self.pool.sample(&mut rng) {
                    Some(i) => Pick::Base(i as u32),
                    None => return None,
                }
            };
            if picks.contains(&pick) {
                continue;
            }
            let x = self.construction(pick).coord;
            if coords
                .iter()
                .any(|&y| distance(x, y) < 1e-6 * self.scale.max(1.0))
            {
                continue;
            }
            picks.push(pick);
            coords.push(x);
        }
        if picks.len() < 2 {
            return None;
        }
        picks[1..].sort();
        let mut key = picks.clone();
        key.sort();
        if !self.visited.lock().unwrap().insert(key) {
            return None;
        }
        Some(picks)
    }

    fn minimise(&self, mut cur: Vec<Pick>) -> Vec<Pick> {
        for p in cur.clone().into_iter().rev() {
            if past(self.deadline) || cur.len() <= 1 || !cur.contains(&p) {
                continue;
            }
            let trial: Vec<Pick> = cur
                .iter()
                .copied()
                .filter(|&q| {
                    q != p && !matches!((p, q), (Pick::Base(b), Pick::Child(c, _)) if b == c)
                })
                .collect();
            if !trial.is_empty() && self.check(&trial) {
                cur = trial;
            }
        }
        cur
    }

    fn finish(&self, picks: &[Pick]) -> Vec<Construction> {
        let mut aug = self.problem.clone();
        let mut taken = taken_names(self.problem);
        let mut out = Vec::new();
        for mut c in self.materialise(picks) {
            let id = aug.points.len() as PointId;
            c.name = natural_name(c.kind, &c.args, &aug, &taken, id);
            c.desc = render_tpl(&c.tpl, |i| aug.points[i as usize].name.clone());
            taken.insert(c.name.to_lowercase());
            aug = augment(&aug, &c);
            out.push(c);
        }
        out
    }
}

/// Every construction of [`search`]'s depth-1 pool that lets DDAR prove the
/// goal, in pool (rank) order and named as [`search`] names its result, plus
/// whether the sweep covered the whole pool before `deadline`. `None` when the
/// base figure cannot be closed.
pub(crate) fn depth1_solvers(
    problem: &Problem,
    deadline: Option<Instant>,
) -> Option<(Vec<Construction>, bool, SearchStats)> {
    let warm = guarded(|| WarmBase::with_slots(problem, MAX_POINTS)).flatten()?;
    let warm1 = guarded(|| WarmBase::new(problem)).flatten()?;
    let pool = guarded(|| build_pool(problem, Some(warm.ddar()), None, 1.0, true))?;
    let runs = AtomicUsize::new(1);
    let hits: Vec<usize> = (0..pool.items.len())
        .into_par_iter()
        .filter(|&i| {
            if past(deadline) {
                return false;
            }
            runs.fetch_add(1, Ordering::Relaxed);
            warm1.check_all(std::slice::from_ref(&pool.items[i].c), deadline)
        })
        .collect();
    let complete = !past(deadline);
    let taken = taken_names(problem);
    let n = problem.points.len() as PointId;
    let named = hits
        .into_iter()
        .map(|i| {
            let mut c = pool.items[i].c.clone();
            c.name = natural_name(c.kind, &c.args, problem, &taken, n);
            c.desc = render_tpl(&c.tpl, |j| problem.points[j as usize].name.clone());
            c
        })
        .collect();
    Some((
        named,
        complete,
        SearchStats {
            runs: runs.load(Ordering::Relaxed),
        },
    ))
}

fn problem_seed(problem: &Problem) -> u64 {
    let mut h = FxHasher::default();
    problem.to_ag_string().hash(&mut h);
    h.finish()
}

fn guarded<T>(f: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).ok()
}

/// Depth-1 sweep over the coincidence-ranked pool, then rounds of randomised
/// rollouts of 2..=4 points until `deadline` (or `AUX_ROUNDS` rounds),
/// minimising any set found. `None` means the base figure could not be closed
/// (the caller falls back to the classical search).
pub(crate) fn search(
    problem: &Problem,
    verbose: bool,
    deadline: Option<Instant>,
    sweep: bool,
) -> Option<(Option<AuxProof>, SearchStats)> {
    search_with(problem, verbose, deadline, sweep, lemmas_enabled())
}

/// `AUX_LEMMAS=0` turns the lemma phase off (A/B measurement).
fn lemmas_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("AUX_LEMMAS").map_or(true, |v| v != "0"))
}

/// Share of the budget after which the lemma phase starts, and where it ends.
const LEMMA_START: f64 = 0.4;
const LEMMA_END: f64 = 0.7;
/// Lemmas tried per phase, and the share of the phase one lemma may use.
const LEMMA_MAX: usize = 8;

/// Collinear triples and concyclic quadruples of the problem's points that
/// hold in the figure (relative tolerance 1e-9) but not in the closure `d`:
/// the facts the figure asserts and DDAR has not explained. The goal is left
/// out, and so is any set with two numerically identical points. Points of
/// the goal first, then collinearities, then the order found.
fn open_lemmas(problem: &Problem, d: &Ddar) -> Vec<crate::predicate::Predicate> {
    use crate::aux_search::pred;
    let n = problem.points.len();
    let v = |i: usize| problem.points[i].value;
    let scale = Scorer::new(&problem.points.iter().map(|p| p.value).collect::<Vec<_>>(), &[]).scale();
    let tol = 1e-9 * scale.max(1.0);
    let goal = problem.goal.as_ref();
    let goal_pts: Vec<PointId> = goal.map(|g| g.points.clone()).unwrap_or_default();
    let same_as_goal = |name: &str, pts: &[PointId]| {
        goal.is_some_and(|g| {
            g.name == name && {
                let mut a = g.points.clone();
                let mut b = pts.to_vec();
                a.sort_unstable();
                a.dedup();
                b.sort_unstable();
                a == b
            }
        })
    };
    let ident = |i: usize, j: usize| distance(v(i), v(j)) < tol;
    let mut d = d.clone();
    let mut holds = |p: &crate::predicate::Predicate| {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| d.check_pred(p))).unwrap_or(true)
    };
    let mut out: Vec<(usize, u8, crate::predicate::Predicate)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if ident(i, j) {
                continue;
            }
            for k in (j + 1)..n {
                if ident(i, k) || ident(j, k) {
                    continue;
                }
                let (a, b, c) = (v(i), v(j), v(k));
                let s = distance(a, b).max(distance(b, c)).max(distance(a, c));
                let cr = (b - a).x * (c - a).y - (b - a).y * (c - a).x;
                let pts = vec![i as PointId, j as PointId, k as PointId];
                if cr.abs() < tol * s {
                    let p = pred("coll", pts.clone());
                    if !same_as_goal("coll", &pts) && !holds(&p) {
                        let g = pts.iter().filter(|q| goal_pts.contains(q)).count();
                        out.push((g, 0, p));
                    }
                    continue;
                }
                let Some(circ) = crate::numerics::NumCircle::through(a, b, c) else {
                    continue;
                };
                if circ.r > 100.0 * scale {
                    continue;
                }
                for l in (k + 1)..n {
                    if ident(i, l) || ident(j, l) || ident(k, l) || circ.distance(v(l)) > tol {
                        continue;
                    }
                    let mut pts = pts.clone();
                    pts.push(l as PointId);
                    let p = pred("cyclic", pts.clone());
                    if !same_as_goal("cyclic", &pts) && !holds(&p) {
                        let g = pts.iter().filter(|q| goal_pts.contains(q)).count();
                        out.push((g, 1, p));
                    }
                }
            }
        }
    }
    out.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
    out.into_iter().map(|(_, _, p)| p).collect()
}

/// **Lemma phase.** Each open lemma ([`open_lemmas`]) becomes the goal of a
/// short search of its own; the constructions of every lemma proved this way
/// are added to the problem, one after another, so a later lemma and the
/// main goal start from them. Depth then adds per lemma, not per point set.
/// Only constructions are carried over — never the lemma as a premise — so
/// the final proof is still one DDAR closure over the hypotheses and the
/// auxiliary definitions. `None` if no lemma was proved.
fn lemma_phase(
    problem: &Problem,
    d: &Ddar,
    verbose: bool,
    until: Instant,
) -> Option<(Problem, Vec<Construction>)> {
    let lemmas = open_lemmas(problem, d);
    if verbose {
        eprintln!("[aux] lemma phase: {} open lemmas", lemmas.len());
    }
    let total = until.saturating_duration_since(Instant::now());
    let mut aug = problem.clone();
    let mut cons: Vec<Construction> = Vec::new();
    for lemma in lemmas.into_iter().take(LEMMA_MAX) {
        let now = Instant::now();
        if now >= until {
            break;
        }
        let mut lp = aug.clone();
        lp.goal = Some(lemma);
        let per = (total / 2).min(until - now);
        let t = Instant::now();
        let res = guarded(|| search_with(&lp, false, Some(now + per), true, false)).flatten();
        let text = lp.goal.as_ref().map(|g| {
            let names: Vec<&str> = g.points.iter().map(|&i| lp.points[i as usize].name.as_str()).collect();
            format!("{} {}", g.name, names.join(" "))
        });
        if let Some((Some(proof), _)) = res {
            if verbose {
                let used: Vec<String> =
                    proof.constructions.iter().map(|c| format!("{} = {}", c.name, c.desc)).collect();
                eprintln!(
                    "[aux] lemma {} proved in {:.2}s with {:?}",
                    text.unwrap_or_default(),
                    t.elapsed().as_secs_f64(),
                    used
                );
            }
            aug = crate::aux_search::apply_constructions(&aug, &proof.constructions);
            cons.extend(proof.constructions);
        } else if verbose {
            eprintln!("[aux] lemma {} not proved ({:.2}s)", text.unwrap_or_default(), t.elapsed().as_secs_f64());
        }
    }
    (!cons.is_empty()).then_some((aug, cons))
}

fn search_with(
    problem: &Problem,
    verbose: bool,
    deadline: Option<Instant>,
    sweep: bool,
    lemmas: bool,
) -> Option<(Option<AuxProof>, SearchStats)> {
    let t0 = Instant::now();
    let warm = guarded(|| WarmBase::with_slots(problem, MAX_POINTS)).flatten()?;
    if warm.proves_goal() {
        return Some((
            Some(AuxProof {
                constructions: vec![],
            }),
            SearchStats { runs: 1 },
        ));
    }
    let warm1 = guarded(|| WarmBase::new(problem)).flatten()?;
    let pool = guarded(|| build_pool(problem, Some(warm.ddar()), None, 1.0, true))?;
    let pts: Vec<Vec2> = problem.points.iter().map(|p| p.value).collect();
    let size = pool.items.len();
    let s = Search {
        problem,
        warm: &warm,
        children: (0..size).map(|_| OnceLock::new()).collect(),
        pool,
        visited: Mutex::new(FxHashSet::default()),
        runs: AtomicUsize::new(1),
        seed: problem_seed(problem),
        scale: Scorer::new(&pts, &[]).scale(),
        deadline,
        verbose,
    };
    let stats = |s: &Search| SearchStats {
        runs: s.runs.load(Ordering::Relaxed),
    };
    if verbose {
        eprintln!(
            "[aux] pool {} ({} with incidences, {} double points) built in {:.2}s",
            size,
            s.pool.items.iter().filter(|it| it.inc >= 1.0).count(),
            s.pool
                .items
                .iter()
                .filter(|it| pts.contains(&it.c.coord))
                .count(),
            t0.elapsed().as_secs_f64()
        );
        for it in s
            .pool
            .items
            .iter()
            .filter(|it| pts.contains(&it.c.coord))
            .take(12)
        {
            eprintln!(
                "    double inc {:.1} heur {:.2}  {} = {}",
                it.inc, it.heur, it.c.name, it.c.desc
            );
        }
        for it in s.pool.items.iter().take(12) {
            eprintln!(
                "    inc {:.1} heur {:.2}  {} = {}",
                it.inc, it.heur, it.c.name, it.c.desc
            );
        }
    }

    let found1 = (0..size).into_par_iter().find_first(|&i| {
        if !sweep || past(deadline) {
            return false;
        }
        s.runs.fetch_add(1, Ordering::Relaxed);
        warm1.check_all(std::slice::from_ref(&s.pool.items[i].c), deadline)
    });
    if verbose {
        eprintln!(
            "[aux] depth-1 sweep done at {:.2}s, {} runs",
            t0.elapsed().as_secs_f64(),
            stats(&s).runs
        );
    }
    if let Some(i) = found1 {
        let c = s.finish(&[Pick::Base(i as u32)]);
        return Some((Some(AuxProof { constructions: c }), stats(&s)));
    }
    drop(warm1);

    let max_rounds = env_usize("AUX_ROUNDS", usize::MAX) as u64;
    let threads = rayon::current_num_threads().max(1);
    let m = (4 * threads).min(size);
    let r = 48u64;
    let mut round = 0u64;
    let budget = deadline.map(|d| d.saturating_duration_since(t0));
    let mut lemma_due = lemmas && budget.is_some();
    while !past(deadline) && round < max_rounds && size >= 2 {
        if let (true, Some(b)) = (lemma_due, budget) {
            if t0.elapsed() >= b.mul_f64(LEMMA_START) {
                lemma_due = false;
                let until = t0 + b.mul_f64(LEMMA_END);
                if let Some((aug, cons)) = lemma_phase(problem, warm.ddar(), verbose, until) {
                    if verbose {
                        eprintln!(
                            "[aux] lemma phase added {} points at {:.2}s; main search on them",
                            cons.len(),
                            t0.elapsed().as_secs_f64()
                        );
                    }
                    let runs = stats(&s).runs;
                    drop(s);
                    let (found, st) = search_with(&aug, verbose, deadline, true, false)?;
                    let found = found.map(|p| {
                        let mut all = cons;
                        all.extend(p.constructions);
                        AuxProof { constructions: all }
                    });
                    return Some((found, SearchStats { runs: runs + st.runs }));
                }
            }
        }
        let firsts = s.sample_firsts(round, m);
        let prefixes: Vec<OnceLock<Option<WarmBase>>> =
            firsts.iter().map(|_| OnceLock::new()).collect();
        let total = firsts.len() as u64 * r;
        let hit = (0..total).into_par_iter().find_map_first(|t| {
            if past(deadline) {
                return None;
            }
            let fi = (t / r) as usize;
            let picks = s.sample_rest(round, firsts[fi], t % r)?;
            let prefix = prefixes[fi]
                .get_or_init(|| {
                    s.warm
                        .extend(&s.pool.items[firsts[fi] as usize].c, deadline)
                })
                .as_ref()?;
            s.runs.fetch_add(1, Ordering::Relaxed);
            let cons = s.materialise(&picks);
            prefix.check_all(&cons[1..], deadline).then_some(picks)
        });
        if let Some(picks) = hit {
            if verbose {
                eprintln!(
                    "[aux] rollout hit in round {round} at {:.2}s: {:?}",
                    t0.elapsed().as_secs_f64(),
                    picks
                );
            }
            let mut picks = picks;
            picks.sort();
            let picks = s.minimise(picks);
            let c = s.finish(&picks);
            return Some((Some(AuxProof { constructions: c }), stats(&s)));
        }
        round += 1;
    }
    if verbose {
        eprintln!(
            "[aux] no proof after {round} rounds, {} runs, {:.2}s",
            stats(&s).runs,
            t0.elapsed().as_secs_f64()
        );
    }
    Some((None, stats(&s)))
}
