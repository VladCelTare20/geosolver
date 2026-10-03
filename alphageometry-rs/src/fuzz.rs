//! Soundness fuzzer: genuinely false variants of corpus problems, which the
//! full pipeline (DDAR, then the aux search) must never prove.

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{Duration, Instant};

use crate::bench::{proof_steps, prove_goal, Attempt, Outcome};
use crate::corpus::{
    self, goal_conjuncts, is_number_arg, parse_problem, render_problem, sample_figure, term_holds,
    AgProblem, FigureSample, Term,
};
use crate::numerics::Vec2;
use crate::quiet_panic::quiet;
use crate::runner::solve_problem_with_proof;

/// How a fuzz case was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The goal was mutated; the figure is a generic one where it is false.
    Goal,
    /// A hypothesis was dropped; generic figure of the weaker problem.
    HypGeneric,
    /// The same weaker problem, pinned to the original figure, where the
    /// dropped hypothesis and the goal still hold numerically.
    HypSpecial,
    /// The original problem on a degenerate figure (coincident points,
    /// collinear base triangle).
    Degenerate,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Goal => "goal",
            Kind::HypGeneric => "hyp-generic",
            Kind::HypSpecial => "hyp-special",
            Kind::Degenerate => "degenerate",
        }
    }

    pub const ALL: [Kind; 4] = [Kind::Goal, Kind::HypGeneric, Kind::HypSpecial, Kind::Degenerate];
}

/// One fuzz input: a corpus statement whose goal must not be proved (for
/// [`Kind::Degenerate`], must not be proved where it is numerically false).
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    pub origin: String,
    pub kind: Kind,
    pub mutation: String,
    pub text: String,
}

/// Generation parameters.
#[derive(Clone, Debug)]
pub struct Config {
    /// Goal mutations per problem; hypothesis drops get `per / 2`, degenerate
    /// figures `per / 4` (each at least one).
    pub per: usize,
    pub seed: u64,
    /// Independent figures a variant must be false on.
    pub samples: usize,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            per: 8,
            seed: 1,
            samples: 5,
        }
    }
}

/// The generated cases, plus what could not be fuzzed and why.
#[derive(Clone, Debug, Default)]
pub struct Generated {
    pub cases: Vec<Case>,
    pub skipped: Vec<(String, String)>,
    /// Variants discarded because they held on at least one sampled figure.
    pub not_false: usize,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed ^ 0x6A09_E667_F3BC_C908)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[self.below(v.len())]
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }
}

fn fnv(s: &str) -> u64 {
    s.bytes()
        .fold(0xCBF2_9CE4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01B3))
}

const PREDICATES: &[(&str, usize)] = &[
    ("coll", 3),
    ("cyclic", 4),
    ("cong", 4),
    ("para", 4),
    ("perp", 4),
    ("eqangle", 8),
    ("eqratio", 8),
    ("s_angle", 3),
    ("rconst", 4),
];
const ANGLES: &[&str] = &["15", "30", "45", "60", "75", "90", "105", "120", "135", "150"];
const RATIOS: &[&str] = &["1/2", "2", "1/3", "3/2", "2/3", "3", "3/4"];

fn point_args(t: &Term) -> Vec<&str> {
    t.args
        .iter()
        .map(String::as_str)
        .filter(|a| !is_number_arg(a))
        .collect()
}

fn distinct(xs: &[&str]) -> bool {
    xs.iter().collect::<HashSet<_>>().len() == xs.len()
}

/// A goal shape the engine accepts with no zero-length segment or repeated
/// point, so "numerically false" means false, not degenerate.
pub fn well_formed(t: &Term) -> bool {
    let p = point_args(t);
    let nums = t.args.len() - p.len();
    let segments = |n: usize| p.len() == n && p.chunks(2).all(|c| c[0] != c[1]);
    match t.name.as_str() {
        "coll" => p.len() >= 3 && nums == 0 && distinct(&p),
        "cyclic" => p.len() >= 4 && nums == 0 && distinct(&p),
        "cong" | "para" | "perp" => nums == 0 && segments(4),
        "eqangle" | "eqratio" => nums == 0 && segments(8),
        "s_angle" => p.len() == 3 && nums == 1 && p[0] != p[1] && p[1] != p[2],
        "rconst" => nums == 1 && segments(4),
        "midp" => p.len() == 3 && nums == 0 && distinct(&p),
        "simtri" | "contri" => p.len() == 6 && nums == 0 && distinct(&p[..3]) && distinct(&p[3..]),
        _ => false,
    }
}

fn swap_point(goal: &Term, names: &[String], rng: &mut Rng) -> Option<(Term, String)> {
    let idx: Vec<usize> = (0..goal.args.len()).filter(|&i| !is_number_arg(&goal.args[i])).collect();
    if idx.is_empty() {
        return None;
    }
    let i = *rng.pick(&idx);
    let new = rng.pick(names).clone();
    if new == goal.args[i] {
        return None;
    }
    let mut t = goal.clone();
    let old = std::mem::replace(&mut t.args[i], new.clone());
    Some((t, format!("swap point {old} -> {new}")))
}

fn transpose(goal: &Term, rng: &mut Rng) -> Option<(Term, String)> {
    let idx: Vec<usize> = (0..goal.args.len()).filter(|&i| !is_number_arg(&goal.args[i])).collect();
    if idx.len() < 2 {
        return None;
    }
    let (i, j) = (*rng.pick(&idx), *rng.pick(&idx));
    if goal.args[i] == goal.args[j] {
        return None;
    }
    let mut t = goal.clone();
    t.args.swap(i, j);
    Some((t, format!("transpose {} <-> {}", goal.args[i], goal.args[j])))
}

fn repredicate(goal: &Term, names: &[String], rng: &mut Rng) -> Option<(Term, String)> {
    let mut pool: Vec<String> = Vec::new();
    for a in point_args(goal) {
        if !pool.iter().any(|p| p == a) {
            pool.push(a.to_string());
        }
    }
    if rng.below(2) == 0 {
        let extra = rng.pick(names).clone();
        if !pool.contains(&extra) {
            pool.push(extra);
        }
    }
    let &(name, arity) = rng.pick(PREDICATES);
    if name == goal.name {
        return None;
    }
    let mut args: Vec<String> = (0..arity).map(|_| rng.pick(&pool).clone()).collect();
    match name {
        "s_angle" => args.push(rng.pick(ANGLES).to_string()),
        "rconst" => args.push(rng.pick(RATIOS).to_string()),
        _ => {}
    }
    Some((
        Term {
            name: name.to_string(),
            args,
        },
        format!("predicate {} -> {name}", goal.name),
    ))
}

fn resegment(goal: &Term, names: &[String], rng: &mut Rng) -> Option<(Term, String)> {
    if !matches!(goal.name.as_str(), "cong" | "para" | "perp" | "eqangle" | "eqratio") {
        return swap_point(goal, names, rng);
    }
    let k = rng.below(goal.args.len() / 2);
    let (a, b) = (rng.pick(names).clone(), rng.pick(names).clone());
    let mut t = goal.clone();
    let old = format!("{}{}", t.args[2 * k], t.args[2 * k + 1]);
    t.args[2 * k] = a.clone();
    t.args[2 * k + 1] = b.clone();
    Some((t, format!("segment {old} -> {a}{b}")))
}

fn mutate_goal(goal: &Term, names: &[String], rng: &mut Rng) -> Option<(Term, String)> {
    let (t, how) = match rng.below(4) {
        0 => swap_point(goal, names, rng),
        1 => transpose(goal, rng),
        2 => repredicate(goal, names, rng),
        _ => resegment(goal, names, rng),
    }?;
    well_formed(&t).then_some((t, how))
}

fn figures(prob: &AgProblem, seed: u64, want: usize, tries: u64, prefer_goal: bool) -> Result<Vec<FigureSample>, String> {
    let mut good = Vec::new();
    let mut other = Vec::new();
    for k in 0..tries {
        if let Some(f) = sample_figure(prob, seed.wrapping_add(k))? {
            if !prefer_goal || f.goal_holds {
                good.push(f);
            } else if other.len() < want {
                other.push(f);
            }
        }
        if good.len() >= want {
            break;
        }
    }
    let missing = want.saturating_sub(good.len());
    good.extend(other.into_iter().take(missing));
    Ok(good)
}

fn false_conjunct(goal: &Term, figs: &[FigureSample]) -> Option<Term> {
    goal_conjuncts(goal, &figs[0]).ok()?.into_iter().find(|c| {
        well_formed(c) && figs.iter().all(|f| matches!(term_holds(c, f), Ok(false)))
    })
}

fn with_goal(prob: &AgProblem, goal: Term) -> AgProblem {
    AgProblem {
        goal,
        ..prob.clone()
    }
}

fn translates(name: &str, text: &str) -> bool {
    parse_problem(name, text).is_ok_and(|p| corpus::translate(&p, 1, 30).is_ok())
}

fn weakenings(prob: &AgProblem) -> Vec<(AgProblem, String)> {
    let defs = corpus::definitions();
    let mut out = Vec::new();
    for (i, c) in prob.clauses.iter().enumerate() {
        let pts: Vec<String> = c.points.iter().map(|p| p.name.clone()).collect();
        let has_premises = |t: &Term| defs.get(&t.name).is_some_and(|d| !d.premises.is_empty());
        if c.constructions.len() >= 2 {
            for j in 0..c.constructions.len() {
                if !has_premises(&c.constructions[j]) {
                    continue;
                }
                let mut w = prob.clone();
                let dropped = w.clauses[i].constructions.remove(j);
                out.push((w, format!("drop `{dropped}` from `{}`", pts.join(" "))));
            }
            continue;
        }
        let only = &c.constructions[0];
        if !has_premises(only) {
            continue;
        }
        let no_inputs = defs.get(&only.name).is_some_and(|d| d.inputs.is_empty());
        let replacement = match (pts.len(), no_inputs) {
            (1, _) if i > 0 => "free",
            (3, true) => "triangle",
            (4, true) => "quadrangle",
            _ => continue,
        };
        let mut w = prob.clone();
        w.clauses[i].constructions = vec![Term {
            name: replacement.to_string(),
            args: pts.clone(),
        }];
        out.push((w, format!("replace `{only}` by `{replacement}`")));
    }
    out
}

fn degenerate_pins(
    prob: &AgProblem,
    base: &FigureSample,
    rng: &mut Rng,
) -> Option<(HashMap<String, Vec2>, String)> {
    let defs = corpus::definitions();
    let k = rng.below(prob.clauses.len());
    let mut pins: HashMap<String, Vec2> = HashMap::new();
    let mut earlier: Vec<String> = Vec::new();
    for c in &prob.clauses[..k] {
        for p in &c.points {
            pins.insert(p.name.clone(), base.coords[&p.name]);
            earlier.push(p.name.clone());
        }
    }
    let clause = &prob.clauses[k];
    let names: Vec<String> = clause.points.iter().map(|p| p.name.clone()).collect();
    for n in &names {
        pins.insert(n.clone(), base.coords[n]);
    }
    let base_shape = clause
        .constructions
        .iter()
        .all(|t| defs.get(&t.name).is_some_and(|d| d.inputs.is_empty()));
    if base_shape && names.len() >= 3 && rng.below(2) == 0 {
        let (a, b) = (base.coords[&names[0]], base.coords[&names[1]]);
        let t = *rng.pick(&[0.5, -0.7, 1.6]);
        pins.insert(names[2].clone(), a + (b - a) * t);
        return Some((
            pins,
            format!("collinear base: {} on line {}{} (t = {t})", names[2], names[0], names[1]),
        ));
    }
    let i = rng.below(names.len());
    let mut candidates = earlier;
    candidates.extend(names[..i].iter().cloned());
    if candidates.is_empty() {
        return None;
    }
    let q = rng.pick(&candidates).clone();
    pins.insert(names[i].clone(), base.coords[&q]);
    Some((pins, format!("coincident: {} placed on {q}", names[i])))
}

/// Generate the fuzz cases for every `(name, statement)` of a corpus.
pub fn generate(problems: &[(String, String)], cfg: &Config) -> Generated {
    let mut out = Generated::default();
    let want = cfg.samples.max(2);
    for (name, text) in problems {
        let mut rng = Rng::new(cfg.seed ^ fnv(name));
        let prob = match parse_problem(name, text) {
            Ok(p) => p,
            Err(e) => {
                out.skipped.push((name.clone(), format!("parse: {e}")));
                continue;
            }
        };
        let seed0 = 1 + rng.next() % 1_000_000;
        let base = match figures(&prob, seed0, want, 150, true) {
            Ok(f) if f.len() >= want && f[0].goal_holds => f,
            Ok(_) => {
                out.skipped.push((name.clone(), "too few figures where the goal holds".into()));
                continue;
            }
            Err(e) => {
                out.skipped.push((name.clone(), format!("figure: {e}")));
                continue;
            }
        };
        let names = base[0].names.clone();
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(prob.goal.to_string());
        let push = |out: &mut Generated, kind: Kind, idx: usize, mutation: String, text: String| {
            let tag = match kind {
                Kind::Goal => "g",
                Kind::HypGeneric => "hg",
                Kind::HypSpecial => "hs",
                Kind::Degenerate => "d",
            };
            let case_name = format!("{name}~{tag}{idx}");
            if translates(&case_name, &text) {
                out.cases.push(Case {
                    name: case_name,
                    origin: name.clone(),
                    kind,
                    mutation,
                    text,
                });
                true
            } else {
                false
            }
        };

        let mut made = 0;
        for _ in 0..cfg.per * 40 {
            if made >= cfg.per {
                break;
            }
            let Some((goal, how)) = mutate_goal(&prob.goal, &names, &mut rng) else {
                continue;
            };
            if !seen.insert(goal.to_string()) {
                continue;
            }
            let Some(conj) = false_conjunct(&goal, &base) else {
                out.not_false += 1;
                continue;
            };
            let variant = with_goal(&prob, conj.clone());
            let text = render_problem(&variant, &base[0].coords);
            let how = format!("goal `{}` -> `{conj}` ({how})", prob.goal);
            if push(&mut out, Kind::Goal, made, how, text) {
                made += 1;
            }
        }

        let mut weak = weakenings(&prob);
        rng.shuffle(&mut weak);
        let mut made = 0;
        for (variant, how) in weak {
            if made >= (cfg.per / 2).max(1) {
                break;
            }
            let seed = 1 + rng.next() % 1_000_000;
            let Ok(figs) = figures(&variant, seed, want, 80, false) else {
                continue;
            };
            if figs.len() < want {
                continue;
            }
            let Some(conj) = false_conjunct(&prob.goal, &figs) else {
                out.not_false += 1;
                continue;
            };
            if !matches!(term_holds(&conj, &base[0]), Ok(true)) {
                continue;
            }
            let variant = with_goal(&variant, conj.clone());
            let generic = render_problem(&variant, &figs[0].coords);
            let special = render_problem(&variant, &base[0].coords);
            let how = format!("{how}; goal `{conj}`");
            if push(&mut out, Kind::HypGeneric, made, how.clone(), generic) {
                push(&mut out, Kind::HypSpecial, made, how, special);
                made += 1;
            }
        }

        let mut made = 0;
        let mut descs: HashSet<String> = HashSet::new();
        for _ in 0..60 {
            if made >= (cfg.per / 4).max(1) {
                break;
            }
            let Some((pins, how)) = degenerate_pins(&prob, &base[0], &mut rng) else {
                continue;
            };
            if !descs.insert(how.clone()) {
                continue;
            }
            let text = render_problem(&prob, &pins);
            if push(&mut out, Kind::Degenerate, made, how, text) {
                made += 1;
            }
        }
    }
    out
}

/// Prefix of a case text that is a `.geo` metric program rather than corpus text.
pub const GEO_PREFIX: &str = "geo: ";

const OBJECT_CONSTRUCTORS: &[&str] = &[
    "line(",
    "circle(",
    "circumcircle(",
    "bisector(",
    "perp_bisector(",
    "perp_line(",
    "para_line(",
];

/// Split a `.geo` program into its statements and its `prove` goal.
pub fn split_geo(src: &str) -> Option<(Vec<String>, String)> {
    let mut stmts = Vec::new();
    let mut goal = None;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            match s
                .strip_prefix("prove ")
                .or_else(|| s.strip_prefix("goal:"))
                .or_else(|| s.strip_prefix('?'))
            {
                Some(g) => goal = Some(g.trim().to_string()),
                None => stmts.push(s.to_string()),
            }
        }
    }
    Some((stmts, goal?))
}

fn geo_points(stmts: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for s in stmts {
        let Some((lhs, rhs)) = s.split_once('=') else { continue };
        if lhs.trim_start().starts_with("assume") {
            continue;
        }
        let rhs = rhs.trim_start();
        if OBJECT_CONSTRUCTORS.iter().any(|c| rhs.starts_with(c)) {
            continue;
        }
        out.extend(
            lhs.split(|c: char| c.is_whitespace() || c == ',')
                .filter(|n| !n.is_empty())
                .map(str::to_string),
        );
    }
    out
}

fn geo_tokens(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_ascii_alphabetic() || c == '_' {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        } else if c.is_ascii_digit() {
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push(chars[start..i].iter().collect());
    }
    out
}

fn mutate_equation(goal: &str, points: &[String], rng: &mut Rng) -> Option<(String, String)> {
    let mut toks = geo_tokens(goal);
    let pts: Vec<usize> = (0..toks.len()).filter(|&i| points.contains(&toks[i])).collect();
    let nums: Vec<usize> = (0..toks.len())
        .filter(|&i| toks[i].parse::<f64>().is_ok())
        .collect();
    let how = match rng.below(4) {
        0 if !pts.is_empty() => {
            let i = *rng.pick(&pts);
            let new = rng.pick(points).clone();
            if new == toks[i] {
                return None;
            }
            let how = format!("swap point {} -> {new}", toks[i]);
            toks[i] = new;
            how
        }
        1 if pts.len() >= 2 => {
            let (i, j) = (*rng.pick(&pts), *rng.pick(&pts));
            if toks[i] == toks[j] {
                return None;
            }
            let how = format!("transpose {} <-> {}", toks[i], toks[j]);
            toks.swap(i, j);
            how
        }
        2 if !nums.is_empty() => {
            let i = *rng.pick(&nums);
            let v: f64 = toks[i].parse().ok()?;
            let new = if rng.below(2) == 0 { v + 1.0 } else { v * 2.0 };
            let how = format!("constant {} -> {new}", toks[i]);
            toks[i] = format!("{new}");
            how
        }
        _ => {
            let (l, r) = goal.split_once('=')?;
            let k = *rng.pick(&["2", "3", "1/2"]);
            return Some((format!("{} = {k} * ({})", l.trim(), r.trim()), format!("scale right side by {k}")));
        }
    };
    Some((toks.concat(), how))
}

fn geo_weakenings(stmts: &[String]) -> Vec<(Vec<String>, String)> {
    let mut out = Vec::new();
    for (i, s) in stmts.iter().enumerate() {
        if s.trim_start().starts_with("assume") {
            let mut w = stmts.to_vec();
            w.remove(i);
            out.push((w, format!("drop `{s}`")));
            continue;
        }
        let Some((lhs, rhs)) = s.split_once('=') else { continue };
        let (lhs, rhs) = (lhs.trim(), rhs.trim());
        if i == 0 || lhs.contains(char::is_whitespace) || rhs == "free" {
            continue;
        }
        if OBJECT_CONSTRUCTORS.iter().any(|c| rhs.starts_with(c)) {
            continue;
        }
        if let Some(cons) = rhs.strip_prefix("point:") {
            let parts = split_top_level(cons);
            if parts.len() >= 2 {
                for j in 0..parts.len() {
                    let mut kept = parts.clone();
                    let dropped = kept.remove(j);
                    let mut w = stmts.to_vec();
                    w[i] = format!("{lhs} = point: {}", kept.join(", "));
                    out.push((w, format!("drop `{}` from `{lhs}`", dropped.trim())));
                }
                continue;
            }
        }
        let mut w = stmts.to_vec();
        w[i] = format!("{lhs} = free");
        out.push((w, format!("replace `{s}` by `{lhs} = free`")));
    }
    out
}

fn split_top_level(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
                continue;
            }
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// The goal's value on each of `n` re-sampled figures: `Some(true)` holds,
/// `Some(false)` clearly fails, `None` undecided (non-finite or borderline).
fn equation_truths(cons: &str, goal: &str, n: usize) -> Option<Vec<Option<bool>>> {
    let (lhs, rhs) = crate::metric::parse_equation(goal).ok()?;
    let insts = crate::geo::build_instances(cons, n).ok()?;
    Some(
        insts
            .iter()
            .map(|inst| {
                let m: HashMap<&str, Vec2> = inst.iter().map(|(k, v)| (k.as_str(), *v)).collect();
                let (l, r) = (lhs.eval(&m).ok()?, rhs.eval(&m).ok()?);
                if !(l.is_finite() && r.is_finite()) {
                    return None;
                }
                let rel = (l - r).abs() / (1.0 + r.abs());
                if rel < 1e-7 {
                    Some(true)
                } else if rel > 1e-4 {
                    Some(false)
                } else {
                    None
                }
            })
            .collect(),
    )
}

fn false_everywhere(cons: &str, goal: &str, n: usize) -> bool {
    equation_truths(cons, goal, n)
        .is_some_and(|t| t.len() >= n.min(2) && t.iter().all(|x| *x == Some(false)))
}

/// Metric-goal cases from `.geo` programs (`(name, source)`): mutated goals and
/// dropped hypotheses, each false on every sampled figure (the classical
/// provers' own figure included). Programs whose goal is not an equation are
/// skipped.
pub fn generate_geo(programs: &[(String, String)], cfg: &Config) -> Generated {
    let mut out = Generated::default();
    let n = cfg.samples.max(2);
    for (name, src) in programs {
        let mut rng = Rng::new(cfg.seed ^ fnv(name));
        let Some((stmts, goal)) = split_geo(src) else {
            out.skipped.push((name.clone(), "no goal".into()));
            continue;
        };
        if crate::metric::parse_equation(&goal).is_err() {
            continue;
        }
        let cons = stmts.join("\n");
        if !equation_truths(&cons, &goal, n).is_some_and(|t| t.iter().all(|x| *x == Some(true))) {
            out.skipped.push((name.clone(), "goal does not hold on every sample".into()));
            continue;
        }
        let points = geo_points(&stmts);
        let encode = |stmts: &[String], goal: &str| format!("{GEO_PREFIX}{} ? {goal}", stmts.join("; "));
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(goal.replace(' ', ""));
        let mut made = 0;
        for _ in 0..cfg.per * 30 {
            if made >= cfg.per {
                break;
            }
            let Some((g, how)) = mutate_equation(&goal, &points, &mut rng) else {
                continue;
            };
            if !seen.insert(g.replace(' ', "")) {
                continue;
            }
            if !false_everywhere(&cons, &g, n) {
                out.not_false += 1;
                continue;
            }
            out.cases.push(Case {
                name: format!("{name}~g{made}"),
                origin: name.clone(),
                kind: Kind::Goal,
                mutation: format!("goal `{goal}` -> `{g}` ({how})"),
                text: encode(&stmts, &g),
            });
            made += 1;
        }
        let mut weak = geo_weakenings(&stmts);
        rng.shuffle(&mut weak);
        let mut made = 0;
        for (w, how) in weak {
            if made >= (cfg.per / 2).max(1) {
                break;
            }
            if !false_everywhere(&w.join("\n"), &goal, n) {
                out.not_false += 1;
                continue;
            }
            out.cases.push(Case {
                name: format!("{name}~hg{made}"),
                origin: name.clone(),
                kind: Kind::HypGeneric,
                mutation: how,
                text: encode(&w, &goal),
            });
            made += 1;
        }
    }
    out
}

type Prover = fn(&str, &str) -> Result<crate::synthetic::Outcome, String>;

/// Run one metric case through the classical provers directly — the additive
/// (`synthetic`) and the multiplicative (`ratio`) one — without
/// `metric::solve`'s figure backstop, so a wrong derivation is seen.
pub fn solve_geo_case(name: &str, text: &str) -> Outcome {
    use crate::synthetic::Outcome as Proof;
    let start = Instant::now();
    let mut out = Outcome {
        name: name.to_string(),
        method: "-".into(),
        ..Outcome::default()
    };
    let body = text.strip_prefix(GEO_PREFIX).unwrap_or(text);
    let Some((cons, goal)) = body.rsplit_once(" ? ") else {
        out.status = "parse-error".into();
        out.detail = "no ` ? ` goal separator".into();
        return out;
    };
    let cons: String = cons.split(';').map(str::trim).collect::<Vec<_>>().join("\n");
    out.parsed = true;
    out.goal_numeric = equation_truths(&cons, goal, 1).and_then(|t| t.first().copied().flatten());
    let provers: [(&str, Prover); 2] = [
        ("synthetic", crate::synthetic::prove_euclidean),
        ("ratio", crate::ratio::prove_ratio),
    ];
    let mut reasons = Vec::new();
    for (label, prove) in provers {
        match catch_unwind(AssertUnwindSafe(|| quiet(|| prove(&cons, goal)))) {
            Ok(Ok(Proof::Proved(proof))) => {
                out.proved = true;
                out.method = label.into();
                out.steps = Some(proof_steps(&proof));
                out.proof = Some(proof);
                out.status = "proved".into();
                break;
            }
            Ok(Ok(Proof::Unhandled(r))) => reasons.push(format!("{label}: {r}")),
            Ok(Err(e)) => reasons.push(format!("{label} error: {e}")),
            Err(_) => reasons.push(format!("{label} panicked")),
        }
    }
    if !out.proved {
        out.status = "unproved".into();
        out.detail = reasons.join("; ");
    }
    out.secs = start.elapsed().as_secs_f64();
    out
}

/// The cases as a corpus file (name line, statement line).
pub fn corpus_text(cases: &[Case]) -> String {
    let mut s = String::new();
    for c in cases {
        s.push_str(&c.name);
        s.push('\n');
        s.push_str(&c.text);
        s.push('\n');
    }
    s
}

fn ddar_only(problem: &crate::Problem) -> Attempt {
    match catch_unwind(AssertUnwindSafe(|| quiet(|| solve_problem_with_proof(problem)))) {
        Err(_) => Attempt::Panicked,
        Ok(Err(e)) => Attempt::NotProved {
            status: "engine-panic",
            detail: e,
        },
        Ok(Ok(Some(proof))) => Attempt::Proved { proof, aux: vec![] },
        Ok(Ok(None)) => Attempt::NotProved {
            status: "unproved",
            detail: "DDAR closure does not reach the goal".into(),
        },
    }
}

/// Run one case: every goal conjunct goes through DDAR and (with `aux`) the
/// aux search, whether or not it holds on the figure. `UNSOUND` when a
/// conjunct that is numerically false on the figure is proved.
pub fn solve_case(name: &str, text: &str, budget: Duration, aux: bool) -> Outcome {
    let start = Instant::now();
    let deadline = start + budget;
    let mut out = Outcome {
        name: name.to_string(),
        method: "-".into(),
        ..Outcome::default()
    };
    let finish = |mut o: Outcome| {
        o.secs = start.elapsed().as_secs_f64();
        o
    };
    let prob = match parse_problem(name, text) {
        Ok(p) => p,
        Err(e) => {
            out.status = "parse-error".into();
            out.detail = e;
            return finish(out);
        }
    };
    let tr = match corpus::translate(&prob, 1, 40) {
        Ok(t) => t,
        Err(e) => {
            out.status = "translate-error".into();
            out.detail = e;
            return finish(out);
        }
    };
    out.parsed = true;
    let truth: Vec<Option<bool>> = tr
        .goals
        .iter()
        .map(|g| corpus::pred_holds(&tr.problem, g))
        .collect();
    out.goal_numeric = Some(truth.iter().all(|t| *t == Some(true)));
    let mut proofs = Vec::new();
    let mut aux_used = Vec::new();
    let mut proved_false = Vec::new();
    let mut failure: Option<(&'static str, String)> = None;
    for (i, g) in tr.goals.iter().enumerate() {
        if Instant::now() >= deadline {
            failure.get_or_insert(("timeout", "budget spent before every conjunct ran".into()));
            break;
        }
        let mut p = tr.problem.clone();
        p.goal = Some(g.clone());
        let attempt = if aux { prove_goal(&p, deadline) } else { ddar_only(&p) };
        match attempt {
            Attempt::Proved { proof, aux: a } => {
                proofs.push(proof);
                aux_used.extend(a);
                if truth[i] != Some(true) {
                    proved_false.push(i);
                }
            }
            Attempt::NotProved { status, detail } => {
                failure.get_or_insert((status, detail));
            }
            Attempt::Panicked => {
                failure.get_or_insert(("engine-panic", format!("DDAR panicked on conjunct {i}")));
            }
        }
    }
    out.proved = proofs.len() == tr.goals.len();
    if !proofs.is_empty() {
        out.steps = Some(proofs.iter().map(|p| proof_steps(p)).sum());
        out.method = if aux_used.is_empty() { "ddar" } else { "aux" }.into();
        out.proof = Some(proofs.join("\n"));
    }
    out.aux = aux_used;
    if !proved_false.is_empty() {
        out.status = "UNSOUND".into();
        out.detail = format!("proved numerically false conjunct(s) {proved_false:?}");
    } else if out.proved {
        out.status = "proved".into();
    } else {
        let (s, d) = failure.unwrap_or(("unproved", String::new()));
        out.status = s.into();
        out.detail = d;
    }
    finish(out)
}

/// Child side of a process-isolated fuzz run: solve one named case of a cases
/// file with the full pipeline.
pub fn child_main(cases_file: &std::path::Path, name: &str, budget: Duration, proofs_dir: Option<&std::path::Path>) -> Outcome {
    let text = std::fs::read_to_string(cases_file).unwrap_or_default();
    let cases = corpus::read_corpus(&text).unwrap_or_default();
    let o = match cases.iter().find(|(n, _)| n == name) {
        Some((n, t)) if t.starts_with(GEO_PREFIX) => solve_geo_case(n, t),
        Some((n, t)) => solve_case(n, t, budget, true),
        None => Outcome {
            name: name.to_string(),
            method: "-".into(),
            status: "parse-error".into(),
            detail: format!("no case named `{name}` in {}", cases_file.display()),
            ..Outcome::default()
        },
    };
    if let Some(dir) = proofs_dir {
        crate::bench::write_proof(dir, &o);
    }
    o
}

/// `UNSOUND`, `CRASH` or `ok`. Every non-degenerate case is false as a
/// theorem, so any proof of it is unsound; a degenerate case is unsound only
/// where the proved goal is false on its own figure.
pub fn verdict(case: &Case, o: &Outcome) -> &'static str {
    if o.status == "UNSOUND" || (case.kind != Kind::Degenerate && o.proved) {
        "UNSOUND"
    } else if o.status == "crashed" {
        "CRASH"
    } else {
        "ok"
    }
}

/// Column header of the fuzz results TSV.
pub const TSV_HEADER: &str = "case\tkind\torigin\tverdict\tmutation\tstatus\tsecs\tdetail";

/// One results row.
pub fn tsv_row(case: &Case, o: &Outcome) -> String {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}",
        clean(&case.name),
        case.kind.tag(),
        clean(&case.origin),
        verdict(case, o),
        clean(&case.mutation),
        o.status,
        o.secs,
        if o.detail.is_empty() { "-".into() } else { clean(&o.detail) },
    )
}
