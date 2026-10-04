//! Coincidence-ranked candidate pools and randomised multi-point rollouts.

use crate::aux_score::{defs_of, Scorer};
use crate::aux_search::{
    augment, candidates_ranked, env_usize, heuristic_score, natural_name, past, render_tpl,
    taken_names, AuxProof, Construction, Kind, RankContext, SearchStats, WarmBase,
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
    let old = candidates_ranked(problem, true, must);
    let mut items: Vec<Item> = if parallel {
        old.into_par_iter().map(|(h, c)| score_of(h, c)).collect()
    } else {
        old.into_iter().map(|(h, c)| score_of(h, c)).collect()
    };
    let key = |c: &Construction| {
        (
            c.kind,
            (c.coord.x * 1e6).round() as i64,
            (c.coord.y * 1e6).round() as i64,
        )
    };
    let mut seen: FxHashSet<(Kind, i64, i64)> = items.iter().map(|it| key(&it.c)).collect();
    for (s, c) in virtual_candidates(problem, ddar, &scorer, min_new, must, parallel) {
        if seen.insert(key(&c)) {
            let heur = heuristic_score(problem, &c, &ctx);
            items.push(Item { c, inc: s, heur });
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
            "[aux] pool {} ({} with incidences) built in {:.2}s",
            size,
            s.pool.items.iter().filter(|it| it.inc >= 1.0).count(),
            t0.elapsed().as_secs_f64()
        );
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
    while !past(deadline) && round < max_rounds && size >= 2 {
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
