//! The web app's presentation layer: turns an engine [`Solution`] into what a
//! reader sees — readable point names, typed facts, a structured proof, a
//! classified note, a numeric counterexample, and an interactive figure.
//!
//! Nothing here decides anything about truth: `status`, `proved`, `proof` and
//! the numeric fields stay exactly as the engine reported them. This module only
//! re-labels (anonymous `_5` becomes `H′` or `P₁`, consistently everywhere),
//! re-shapes (proof text into numbered steps with citations), and draws.

use std::collections::{HashMap, HashSet};

use ddar::{Predicate, Problem};
use serde::Serialize;

use crate::engine::{Method, Solution, Status};
use crate::figure;

/// A screen- or world-space point.
pub type Pt = (f64, f64);

// ------------------------------------------------------------------ names --

/// The engine's display convention: first letter upper-case, digits as
/// subscripts, `'` as a prime (`a1` → `A₁`, `x'` → `X′`).
pub fn disp(name: &str) -> String {
    let keep_case = name.starts_with(|c: char| c.is_uppercase());
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else if let Some(d) = ch.to_digit(10) {
            out.push(char::from_u32(0x2080 + d).unwrap_or(ch));
        } else if ch == '\'' {
            out.push('\u{2032}');
        } else if keep_case {
            out.push(ch);
        } else {
            out.extend(ch.to_uppercase());
        }
    }
    out
}

fn subscript(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| char::from_u32(0x2080 + c.to_digit(10).unwrap_or(0)).unwrap_or(c))
        .collect()
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '\''
}

/// Raw engine point names mapped to the names a reader sees. Anonymous points
/// (`_5`, created for inline expressions such as `reflect(H, line(B, C))`) get
/// a derived name — `H′` for a reflection, `M`/`N` for a midpoint — or else
/// `P₁`, `P₂`, …, never colliding with a name already in the figure.
pub struct Names {
    map: HashMap<String, String>,
    /// Raw names, longest first, for segmenting runs like `BC_5`.
    raws: Vec<String>,
}

impl Names {
    pub fn build(problem: &Problem) -> Names {
        let mut map: HashMap<String, String> = HashMap::new();
        let mut used: HashSet<String> = HashSet::new();
        for p in &problem.points {
            if !p.name.starts_with('_') {
                let d = disp(&p.name);
                used.insert(d.clone());
                map.insert(p.name.clone(), d);
            }
        }
        let idx: HashMap<&str, usize> = problem
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.as_str(), i))
            .collect();
        let name_of = |i: u32| problem.points[i as usize].name.as_str();
        let mut generic = 0usize;
        for (i, p) in problem.points.iter().enumerate() {
            if !p.name.starts_with('_') {
                continue;
            }
            let me = i as u32;
            let mut chosen: Option<String> = None;
            // Reflection: two congruences `c Y = c X` from different centres.
            let mut pivots: HashMap<u32, Vec<u32>> = HashMap::new();
            for pred in &problem.preds {
                if pred.name == "cong" && pred.points.len() == 4 {
                    if let Some((c, a, b)) = cong_center3(&pred.points) {
                        if b == me && a != me {
                            pivots.entry(a).or_default().push(c);
                        } else if a == me && b != me {
                            pivots.entry(b).or_default().push(c);
                        }
                    }
                }
            }
            for (src, centres) in &pivots {
                let distinct: HashSet<_> = centres.iter().collect();
                if distinct.len() >= 2 || (distinct.len() == 1 && is_midpoint_of(problem, **distinct.iter().next().unwrap(), *src, me)) {
                    let base = map
                        .get(name_of(*src))
                        .cloned()
                        .unwrap_or_else(|| disp(name_of(*src)));
                    let cand = format!("{base}\u{2032}");
                    if !used.contains(&cand) {
                        chosen = Some(cand);
                        break;
                    }
                }
            }
            // Midpoint: `coll X A B` and `cong X A X B`.
            if chosen.is_none() && problem.preds.iter().any(|pred| {
                pred.name == "cong"
                    && pred.points.len() == 4
                    && cong_center3(&pred.points).is_some_and(|(c, a, b)| {
                        c == me && is_midpoint_of(problem, me, a, b)
                    })
            }) {
                for cand in ["M", "N", "K"]
                    .iter()
                    .map(|s| s.to_string())
                    .chain((1..50).map(|k| format!("M{}", subscript(k))))
                {
                    if !used.contains(&cand) {
                        chosen = Some(cand);
                        break;
                    }
                }
            }
            let name = chosen.unwrap_or_else(|| loop {
                generic += 1;
                let cand = format!("P{}", subscript(generic));
                if !used.contains(&cand) {
                    break cand;
                }
            });
            used.insert(name.clone());
            map.insert(p.name.clone(), name);
        }
        let _ = idx;
        let mut raws: Vec<String> = map.keys().cloned().collect();
        raws.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        Names { map, raws }
    }

    /// The display name for a raw name (falls back to the plain convention).
    pub fn get(&self, raw: &str) -> String {
        self.map.get(raw).cloned().unwrap_or_else(|| disp(raw))
    }

    /// Split a run like `BC_5` into raw point names, if it is made only of
    /// known names (longest names first, with backtracking).
    pub fn segment(&self, run: &str) -> Option<Vec<String>> {
        fn go(names: &Names, s: &str, out: &mut Vec<String>) -> bool {
            if s.is_empty() {
                return true;
            }
            for r in &names.raws {
                if let Some(rest) = s.strip_prefix(r.as_str()) {
                    // A raw name must not be cut in the middle of a digit run
                    // or before a prime that belongs to it.
                    if rest.starts_with(|c: char| c.is_ascii_digit() || c == '\'')
                        && !names.raws.iter().any(|x| rest.starts_with(x.as_str()))
                    {
                        continue;
                    }
                    out.push(r.clone());
                    if go(names, rest, out) {
                        return true;
                    }
                    out.pop();
                }
            }
            false
        }
        let mut out = Vec::new();
        go(self, run, &mut out).then_some(out)
    }

    /// The run with every raw name replaced by its display name.
    pub fn rename_run(&self, run: &str) -> Option<String> {
        self.segment(run)
            .map(|v| v.iter().map(|r| self.get(r)).collect::<String>())
    }

    /// Rename every point-name run in free text. With `prose`, only runs that
    /// look like names (upper-case, digits, `_`, `'`) are touched, so ordinary
    /// lower-case words are never mangled.
    pub fn rename_text(&self, text: &str, prose: bool) -> String {
        let mut out = String::with_capacity(text.len());
        let mut run = String::new();
        let flush = |run: &mut String, out: &mut String| {
            if run.is_empty() {
                return;
            }
            let looks = !prose
                || run
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '\'');
            let starts_ok = run.chars().next().is_some_and(|c| !c.is_ascii_digit());
            match (looks && starts_ok).then(|| self.rename_run(run)).flatten() {
                Some(r) => out.push_str(&r),
                None => out.push_str(run),
            }
            run.clear();
        };
        for c in text.chars() {
            if is_name_char(c) {
                run.push(c);
            } else {
                flush(&mut run, &mut out);
                out.push(c);
            }
        }
        flush(&mut run, &mut out);
        out
    }

    /// Raw point names mentioned in `text` (every segmentable run).
    pub fn mentioned(&self, text: &str, prose: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut run = String::new();
        let take = |run: &mut String, out: &mut Vec<String>| {
            if !run.is_empty() {
                let looks = !prose
                    || run.chars().all(|c| {
                        c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' || c == '\''
                    });
                if looks && !run.starts_with(|c: char| c.is_ascii_digit()) {
                    if let Some(v) = self.segment(run) {
                        for r in v {
                            if !out.contains(&r) {
                                out.push(r);
                            }
                        }
                    }
                }
                run.clear();
            }
        };
        for c in text.chars() {
            if is_name_char(c) {
                run.push(c);
            } else {
                take(&mut run, &mut out);
            }
        }
        take(&mut run, &mut out);
        out
    }

    /// These names plus points a proof introduces itself (`Let N be …`),
    /// each shown under its own name.
    fn with_points(&self, extra: &[String]) -> Names {
        let mut map = self.map.clone();
        for e in extra {
            map.entry(e.clone()).or_insert_with(|| disp(e));
        }
        let mut raws: Vec<String> = map.keys().cloned().collect();
        raws.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        Names { map, raws }
    }

    fn disp_all(&self, raws: &[String]) -> Vec<String> {
        raws.iter().map(|r| self.get(r)).collect()
    }
}

/// A `cong` read as "centre + two points at equal distance": `(c, a, b)` with
/// `ca = cb`.
fn cong_center3(p: &[u32]) -> Option<(u32, u32, u32)> {
    for (c1, t1, c2, t2) in [(0, 1, 2, 3), (0, 1, 3, 2), (1, 0, 2, 3), (1, 0, 3, 2)] {
        if p[c1] == p[c2] && p[t1] != p[t2] {
            return Some((p[c1], p[t1], p[t2]));
        }
    }
    None
}

/// Is `m` collinear with `a` and `b` (by a hypothesis) — i.e. a midpoint when
/// also equidistant?
fn is_midpoint_of(problem: &Problem, m: u32, a: u32, b: u32) -> bool {
    problem.preds.iter().any(|p| {
        p.name == "coll" && p.points.contains(&m) && p.points.contains(&a) && p.points.contains(&b)
    })
}

// ------------------------------------------------------------------ facts --

/// A typed fact. `kind` picks the sentence template on the client (so it can
/// be localized); `args` are already-typeset, language-neutral pieces.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Fact {
    pub kind: &'static str,
    pub args: Vec<String>,
    /// Display names of the points it involves (for figure highlighting).
    pub points: Vec<String>,
    /// Romanian `args` for a `prose` fact (the engine writes English).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ro: Option<Vec<String>>,
}

impl Fact {
    fn new(kind: &'static str, args: Vec<String>, points: Vec<String>) -> Fact {
        Fact { kind, args, points, ro: None }
    }
    #[cfg(test)]
    pub fn plain(&self) -> String {
        let a = |i: usize| self.args.get(i).cloned().unwrap_or_default();
        match self.kind {
            "coll" => format!("{} are collinear", self.args.join(", ")),
            "cyclic" => format!("{} are concyclic", self.args.join(", ")),
            "midp" => format!("{} is the midpoint of {}", a(0), a(1)),
            "cong" => format!("{} = {}", a(0), a(1)),
            "perp" => format!("{} \u{27c2} {}", a(0), a(1)),
            "para" => format!("{} \u{2225} {}", a(0), a(1)),
            "eqangle" => format!("{} = {}", a(0), a(1)),
            "eqratio" => self.args.chunks(2).map(|c| c.join(" : ")).collect::<Vec<_>>().join(" = "),
            "para_ratio" => format!("{} \u{2225} {}, {} : {} = {} : {}", a(0), a(1), a(2), a(3), a(4), a(5)),
            "aconst" => format!("{} = {}\u{b0}", a(0), a(1)),
            "rconst" => format!("{} : {} = {}", a(0), a(1), a(2)),
            "simtri" => format!("\u{25b3}{} \u{223c} \u{25b3}{}", a(0), a(1)),
            "contri" => format!("\u{25b3}{} \u{2245} \u{25b3}{}", a(0), a(1)),
            "circle" => format!("{} is the circumcenter of \u{25b3}{}", a(0), a(1)),
            "length" => format!("{} = {}", a(0), a(1)),
            "eqdist" => self.args.join(" = "),
            "coincide" => format!("{} = {}", a(0), a(1)),
            "points" => self.args.join(", "),
            _ => self.args.join(" "),
        }
    }
}

fn pretty_angle(n: &dyn Fn(u32) -> String, a: u32, b: u32, c: u32, d: u32) -> String {
    let (mut a, mut b, mut c, mut d) = (a, b, c, d);
    if b == c || b == d {
        std::mem::swap(&mut a, &mut b);
    }
    if a == d {
        std::mem::swap(&mut c, &mut d);
    }
    if a == c {
        format!("\u{2220}{}{}{}", n(b), n(a), n(d))
    } else {
        format!("\u{2220}({}{}, {}{})", n(a), n(b), n(c), n(d))
    }
}

fn angle_sum_text(n: &dyn Fn(u32) -> String, pred: &Predicate) -> Option<String> {
    let p = &pred.points;
    if p.len() < 4 || !p.len().is_multiple_of(2) {
        return None;
    }
    let k = p.len() / 2;
    if pred.constants.len() != k + 1 {
        return None;
    }
    let coef: Vec<i64> = pred.constants[..k]
        .iter()
        .map(|c| (c.denom_i64() == Some(1)).then(|| c.numer_i64()).flatten())
        .collect::<Option<_>>()?;
    let line = |i: usize| (p[2 * i], p[2 * i + 1]);
    let shares = |a: (u32, u32), b: (u32, u32)| a.0 == b.0 || a.0 == b.1 || a.1 == b.0 || a.1 == b.1;
    let mut used = vec![false; k];
    let mut terms: Vec<String> = Vec::new();
    for i in 0..k {
        if used[i] || coef[i] == 0 {
            continue;
        }
        let j = (i + 1..k).find(|&j| !used[j] && coef[j] == -coef[i] && shares(line(i), line(j)))?;
        used[i] = true;
        used[j] = true;
        let (pos, neg) = if coef[i] > 0 { (line(i), line(j)) } else { (line(j), line(i)) };
        let angle = pretty_angle(n, neg.0, neg.1, pos.0, pos.1);
        let c = coef[i].abs();
        terms.push(if c == 1 { angle } else { format!("{c}{angle}") });
    }
    if terms.is_empty() || used.iter().zip(&coef).any(|(u, c)| !u && *c != 0) {
        return None;
    }
    let rhs = -pred.constants[k].to_f64();
    let rhs = if rhs.fract() == 0.0 { format!("{}", rhs as i64) } else { format!("{rhs}") };
    Some(format!("{} = {rhs}\u{b0}", terms.join(" + ")))
}

fn rational_text(c: &ddar::rational::Rat) -> Option<String> {
    match (c.numer_i64()?, c.denom_i64()?) {
        (n, 1) => Some(n.to_string()),
        (n, d) => Some(format!("{n}/{d}")),
    }
}

fn dist_sum_text(n: &dyn Fn(u32) -> String, pred: &Predicate) -> Option<String> {
    let p = &pred.points;
    let k = pred.constants.len();
    if k == 0 || p.len() != 2 * k {
        return None;
    }
    let (mut lhs, mut rhs) = (Vec::new(), Vec::new());
    for (i, c) in pred.constants.iter().enumerate() {
        let seg = format!("{}{}", n(p[2 * i]), n(p[2 * i + 1]));
        let v = c.to_f64();
        if v == 0.0 {
            continue;
        }
        let mag = rational_text(&c.abs())?;
        let term = if mag == "1" { seg } else { format!("{mag}\u{b7}{seg}") };
        if v > 0.0 { lhs.push(term) } else { rhs.push(term) }
    }
    if lhs.is_empty() && rhs.is_empty() {
        return None;
    }
    let side = |v: &[String]| if v.is_empty() { "0".to_string() } else { v.join(" + ") };
    Some(format!("{} = {}", side(&lhs), side(&rhs)))
}

fn dist_product_text(n: &dyn Fn(u32) -> String, pred: &Predicate) -> Option<String> {
    let p = &pred.points;
    let k = pred.constants.len().checked_sub(1)?;
    if k == 0 || p.len() != 2 * k {
        return None;
    }
    let (mut num, mut den) = (Vec::new(), Vec::new());
    for (i, c) in pred.constants[..k].iter().enumerate() {
        let seg = format!("{}{}", n(p[2 * i]), n(p[2 * i + 1]));
        let (e, d) = (c.numer_i64()?, c.denom_i64()?);
        if e == 0 {
            continue;
        }
        let pow = match (e.abs(), d) {
            (1, 1) => seg,
            (m, 1) => format!("{seg}{}", superscript(&m.to_string())),
            (m, d) => format!("{seg}^({m}/{d})"),
        };
        if e > 0 { num.push(pow) } else { den.push(pow) }
    }
    let konst = &pred.constants[k];
    let kt = rational_text(konst)?;
    let side = |v: &[String]| if v.is_empty() { "1".to_string() } else { v.join(" \u{b7} ") };
    let rhs = match (kt.as_str(), den.is_empty()) {
        ("1", _) => side(&den),
        (_, true) => kt,
        _ => format!("{kt} \u{b7} {}", side(&den)),
    };
    Some(format!("{} = {rhs}", side(&num)))
}

fn const_str(pred: &Predicate) -> String {
    pred.constants
        .first()
        .map(|c| match (c.numer_i64(), c.denom_i64()) {
            (Some(n), Some(1)) => n.to_string(),
            (Some(n), Some(d)) => format!("{n}/{d}"),
            _ => format!("{}", c.to_f64()),
        })
        .unwrap_or_default()
}

/// A predicate over point ids as a typed fact.
fn fact_of_pred(problem: &Problem, names: &Names, pred: &Predicate) -> Fact {
    let raw = |i: u32| problem.point_name(i).to_string();
    let n = |i: u32| names.get(&raw(i));
    let p = &pred.points;
    let pts: Vec<String> = {
        let mut v: Vec<String> = Vec::new();
        for &i in p {
            let d = n(i);
            if !v.contains(&d) {
                v.push(d);
            }
        }
        v
    };
    let seg = |i: usize| format!("{}{}", n(p[i]), n(p[i + 1]));
    let nm = p.len();
    match pred.name.as_str() {
        "coll" if nm >= 2 => Fact::new("coll", pts.clone(), pts),
        "cyclic" if nm >= 3 => Fact::new("cyclic", pts.clone(), pts),
        "midp" if nm == 3 => Fact::new("midp", vec![n(p[0]), format!("{}{}", n(p[1]), n(p[2]))], pts),
        "cong" if nm == 4 => Fact::new("cong", vec![seg(0), seg(2)], pts),
        "perp" if nm == 4 => Fact::new("perp", vec![seg(0), seg(2)], pts),
        "para" if nm == 4 => Fact::new("para", vec![seg(0), seg(2)], pts),
        "eqangle" if nm == 8 => Fact::new(
            "eqangle",
            vec![
                pretty_angle(&n, p[0], p[1], p[2], p[3]),
                pretty_angle(&n, p[4], p[5], p[6], p[7]),
            ],
            pts,
        ),
        "eqratio" if nm == 8 => Fact::new("eqratio", vec![seg(0), seg(2), seg(4), seg(6)], pts),
        "aconst" | "s_angle" if nm == 4 => Fact::new(
            "aconst",
            vec![pretty_angle(&n, p[0], p[1], p[2], p[3]), const_str(pred)],
            pts,
        ),
        "rconst" if nm == 4 => Fact::new("rconst", vec![seg(0), seg(2), const_str(pred)], pts),
        "simtri" | "simtri2" | "simtri*" if nm == 6 => Fact::new(
            "simtri",
            vec![
                format!("{}{}{}", n(p[0]), n(p[1]), n(p[2])),
                format!("{}{}{}", n(p[3]), n(p[4]), n(p[5])),
            ],
            pts,
        ),
        "contri" | "contri2" | "contri*" if nm == 6 => Fact::new(
            "contri",
            vec![
                format!("{}{}{}", n(p[0]), n(p[1]), n(p[2])),
                format!("{}{}{}", n(p[3]), n(p[4]), n(p[5])),
            ],
            pts,
        ),
        "circle" if nm == 4 => Fact::new(
            "circle",
            vec![n(p[0]), format!("{}{}{}", n(p[1]), n(p[2]), n(p[3]))],
            pts,
        ),
        "angeq" if angle_sum_text(&n, pred).is_some() => {
            Fact::new("formula", vec![angle_sum_text(&n, pred).unwrap_or_default()], pts)
        }
        "distseq" if dist_sum_text(&n, pred).is_some() => {
            Fact::new("formula", vec![dist_sum_text(&n, pred).unwrap_or_default()], pts)
        }
        "distmeq" if dist_product_text(&n, pred).is_some() => {
            Fact::new("formula", vec![dist_product_text(&n, pred).unwrap_or_default()], pts)
        }
        _ => {
            let mut parts: Vec<String> = vec![pred.name.clone()];
            parts.extend(p.iter().map(|&i| n(i)));
            let c = const_str(pred);
            if !c.is_empty() {
                parts.push(c);
            }
            Fact::new("raw", vec![parts.join(" ")], pts)
        }
    }
}

/// A low-level predicate written as text (`perp A H B C`) as a typed fact.
fn fact_of_pred_text(problem: &Problem, names: &Names, text: &str) -> Option<Fact> {
    let resolve = |s: &str| problem.points.iter().position(|p| p.name == s).map(|i| i as u32);
    Predicate::parse(text.trim(), &resolve)
        .ok()
        .map(|p| fact_of_pred(problem, names, &p))
}

fn is_tautology(pred: &Predicate) -> bool {
    let p = &pred.points;
    pred.name == "cong"
        && p.len() == 4
        && ((p[0] == p[2] && p[1] == p[3]) || (p[0] == p[3] && p[1] == p[2]))
}

// ----------------------------------------------------------- metric text --

fn superscript(digits: &str) -> String {
    digits
        .chars()
        .map(|c| match c {
            '0' => '\u{2070}',
            '1' => '\u{b9}',
            '2' => '\u{b2}',
            '3' => '\u{b3}',
            '4'..='9' => char::from_u32(0x2074 + (c as u32 - '4' as u32)).unwrap_or(c),
            other => other,
        })
        .collect()
}

/// Typeset metric source text: `dist(A,D)^2 = 14` → `AD² = 14`, `angle(A,B,C)`
/// → `∠ABC`, `area(A,B,C)` → `[ABC]`, `sqrt(x)` → `√x`, `*` → `·`.
pub fn pretty_metric(src: &str, names: &Names) -> String {
    let chars: Vec<char> = src.chars().collect();
    let (s, _) = metric_seq(&chars, 0, names, false);
    // Normalise binary-operator spacing.
    let mut out = String::new();
    let mut prev_space = true;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    angle_degrees(&trig_powers(out.trim()))
}

pub fn trig_powers(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        let f = ["sin", "cos", "tan"].iter().find(|f| {
            let w: Vec<char> = f.chars().collect();
            c[i..].starts_with(&w) && c.get(i + 3) == Some(&'\u{2220}') && (i == 0 || !c[i - 1].is_alphanumeric())
        });
        if let Some(f) = f {
            let mut j = i + 4;
            while j < c.len() && (c[j].is_uppercase() || c[j].is_ascii_lowercase() || ('\u{2080}'..='\u{2089}').contains(&c[j]) || matches!(c[j], '\u{2032}' | '\u{2033}')) {
                j += 1;
            }
            if j > i + 4 && j < c.len() && matches!(c[j], '\u{b2}' | '\u{b3}') {
                out.push_str(f);
                out.push(c[j]);
                out.extend(&c[i + 3..j]);
                i = j + 1;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn angle_degrees(s: &str) -> String {
    let only_angles = s.contains('\u{2220}')
        && !["sin", "cos", "tan", "[", "\u{221a}", "/"].iter().any(|k| s.contains(k))
        && s.split(' ').all(|tok| !tok.starts_with(|c: char| c.is_uppercase()));
    if !only_angles {
        return s.to_string();
    }
    let toks: Vec<&str> = s.split(' ').collect();
    let joins = |t: Option<&&str>| t.is_none_or(|t| matches!(*t, "=" | "+" | "\u{2212}"));
    toks.iter()
        .enumerate()
        .map(|(i, t)| {
            let num = t.trim_start_matches('\u{2212}');
            let is_num = !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.');
            if is_num && joins(i.checked_sub(1).and_then(|k| toks.get(k))) && joins(toks.get(i + 1)) {
                format!("{t}\u{b0}")
            } else {
                t.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn metric_seq(c: &[char], mut i: usize, names: &Names, stop_at_close: bool) -> (String, usize) {
    let mut out = String::new();
    while i < c.len() {
        let ch = c[i];
        if ch == ')' && stop_at_close {
            return (out, i);
        }
        if ch.is_ascii_alphabetic() {
            let start = i;
            while i < c.len() && is_name_char(c[i]) {
                i += 1;
            }
            let word: String = c[start..i].iter().collect();
            let mut j = i;
            while j < c.len() && c[j] == ' ' {
                j += 1;
            }
            if j < c.len() && c[j] == '(' && ["dist", "angle", "area", "sqrt", "sin", "cos"].contains(&word.as_str()) {
                let (args, end) = metric_args(c, j + 1, names);
                i = end + 1;
                let pts = |a: &[String]| a.iter().map(|x| names.rename_run(x.trim()).unwrap_or_else(|| x.trim().to_string())).collect::<String>();
                match word.as_str() {
                    "dist" => out.push_str(&pts(&args)),
                    "angle" => {
                        out.push('\u{2220}');
                        out.push_str(&pts(&args));
                    }
                    "area" => {
                        out.push('[');
                        out.push_str(&pts(&args));
                        out.push(']');
                    }
                    "sqrt" => {
                        let inner = args.join(", ");
                        let simple = inner.chars().all(|c| c.is_alphanumeric() || "\u{2080}\u{2081}\u{2082}\u{2083}\u{2084}\u{2085}\u{2086}\u{2087}\u{2088}\u{2089}\u{2032}\u{b2}\u{b3}".contains(c));
                        out.push('\u{221a}');
                        if simple {
                            out.push_str(&inner);
                        } else {
                            out.push('(');
                            out.push_str(&inner);
                            out.push(')');
                        }
                    }
                    f if args.len() == 1 && args[0].starts_with('\u{2220}') && !args[0].contains(' ') => {
                        out.push_str(f);
                        out.push_str(&args[0]);
                    }
                    f => {
                        out.push_str(f);
                        out.push('(');
                        out.push_str(&args.join(", "));
                        out.push(')');
                    }
                }
            } else {
                out.push_str(&names.rename_run(&word).unwrap_or(word));
            }
            continue;
        }
        match ch {
            '^' => {
                let mut j = i + 1;
                while j < c.len() && c[j] == ' ' {
                    j += 1;
                }
                let st = j;
                while j < c.len() && c[j].is_ascii_digit() {
                    j += 1;
                }
                if j > st {
                    let d: String = c[st..j].iter().collect();
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push_str(&superscript(&d));
                    i = j;
                } else {
                    out.push('^');
                    i += 1;
                }
                continue;
            }
            '*' => out.push_str(" \u{b7} "),
            '-' if out.trim_end().is_empty() || out.trim_end().ends_with(['=', '(', '+', '\u{2212}', '\u{b7}', '/']) => {
                out.push_str(" \u{2212}")
            }
            '-' => out.push_str(" \u{2212} "),
            '+' => out.push_str(" + "),
            '=' => out.push_str(" = "),
            '/' => out.push_str(" / "),
            '(' => {
                let (inner, end) = metric_seq(c, i + 1, names, true);
                out.push('(');
                out.push_str(inner.trim());
                out.push(')');
                i = end + 1;
                continue;
            }
            _ => out.push(ch),
        }
        i += 1;
    }
    (out, i)
}

fn metric_args(c: &[char], mut i: usize, names: &Names) -> (Vec<String>, usize) {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    let start = i;
    while i < c.len() {
        match c[i] {
            '(' => {
                depth += 1;
                cur.push('(');
            }
            ')' if depth == 0 => break,
            ')' => {
                depth -= 1;
                cur.push(')');
            }
            ',' if depth == 0 => {
                args.push(std::mem::take(&mut cur));
            }
            ch => cur.push(ch),
        }
        i += 1;
    }
    args.push(cur);
    let _ = start;
    let args = args
        .into_iter()
        .map(|a| {
            let t = a.trim();
            if t.contains('(') || t.contains(['+', '*', '^', '-', '/']) {
                pretty_metric(t, names)
            } else {
                t.to_string()
            }
        })
        .collect();
    (args, i)
}

/// Typeset the metric calls inside prose (`… gives dist(A,D)^2 = 14 …`).
fn pretty_metric_inline(text: &str, names: &Names) -> String {
    let mut out = String::new();
    let mut rest = text;
    loop {
        let next = ["dist(", "dist (", "angle(", "area(", "sqrt("]
            .iter()
            .filter_map(|k| rest.find(k).map(|i| (i, *k)))
            .min_by_key(|(i, _)| *i);
        let Some((at, _)) = next else {
            out.push_str(rest);
            break;
        };
        // Preceded by a name char → a longer identifier, not a call.
        if at > 0 && rest[..at].chars().last().is_some_and(is_name_char) {
            out.push_str(&rest[..at + 1]);
            rest = &rest[at + 1..];
            continue;
        }
        out.push_str(&rest[..at]);
        // Find the matching close paren, then an optional `^ k`.
        let bytes: Vec<char> = rest[at..].chars().collect();
        let mut depth = 0i32;
        let mut end = None;
        for (k, ch) in bytes.iter().enumerate() {
            if *ch == '(' {
                depth += 1;
            } else if *ch == ')' {
                depth -= 1;
                if depth == 0 {
                    end = Some(k);
                    break;
                }
            }
        }
        let Some(end) = end else {
            out.push_str(&rest[at..]);
            break;
        };
        let mut stop = end + 1;
        let mut k = stop;
        while k < bytes.len() && bytes[k] == ' ' {
            k += 1;
        }
        if k < bytes.len() && bytes[k] == '^' {
            let mut j = k + 1;
            while j < bytes.len() && bytes[j] == ' ' {
                j += 1;
            }
            let st = j;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > st {
                stop = j;
            }
        }
        let call: String = bytes[..stop].iter().collect();
        out.push_str(&pretty_metric(&call, names));
        let consumed: usize = bytes[..stop].iter().map(|c| c.len_utf8()).sum();
        rest = &rest[at + consumed..];
    }
    out
}

// ------------------------------------------------------------------ proof --

#[derive(Serialize, Clone, Debug)]
pub struct Step {
    /// The engine's step number (`001` → 1).
    pub n: usize,
    /// `given` (a hypothesis restated) or `step` (a deduction).
    pub kind: &'static str,
    /// Rule key the client localizes (`similar`, `concyclic`, `theorem`, …).
    pub rule: &'static str,
    /// The engine's own rule/theorem name, when the key alone is not enough.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name_ro: Option<String>,
    pub fact: Fact,
    /// Step numbers this step cites.
    pub deps: Vec<usize>,
    /// The facts a Euclidean step lists under itself (its deductive-closure
    /// facts, or the sine relations it combines).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subs: Vec<SubStep>,
}

#[derive(Serialize, Clone, Debug)]
pub struct SubStep {
    pub rule: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_name_ro: Option<String>,
    pub fact: Fact,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ProofView {
    pub steps: Vec<Step>,
    /// The concluding statement (the goal), when the proof states it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conclusion: Option<Fact>,
    /// `ddar` (numbered deductions with citations) or `euclidean` (prose
    /// steps citing named theorems).
    pub style: &'static str,
}

fn parse_ddar_proof(text: &str, problem: &Problem, names: &Names, aux: &HashSet<String>) -> ProofView {
    let mut steps = Vec::new();
    let mut conclusion = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('\u{220e}') {
            conclusion = fact_of_pred_text(problem, names, rest.trim());
            continue;
        }
        if let Some(step) = ddar_line(t, problem, names, aux) {
            steps.push(step);
        }
    }
    ProofView { steps: drop_restatements(steps), conclusion, style: "ddar" }
}

/// One numbered DDAR proof line (`012. similar triangles: △ABC ∼ △DEF [003 & 007]`).
fn ddar_line(t: &str, problem: &Problem, names: &Names, aux: &HashSet<String>) -> Option<Step> {
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || !t[digits.len()..].starts_with(". ") {
        return None;
    }
    let n: usize = digits.parse().unwrap_or(0);
    let mut body = t[digits.len() + 2..].to_string();
    let mut deps = Vec::new();
    if body.ends_with(']') {
        if let Some(open) = body.rfind(" [") {
            deps = body[open + 2..body.len() - 1]
                .split('&')
                .filter_map(|d| d.trim().parse().ok())
                .collect();
            body.truncate(open);
        }
    }
    let pts_of = |s: &str| names.disp_all(&names.mentioned(s, false));
    let formula = |s: &str| Fact::new("formula", vec![names.rename_text(s, false)], pts_of(s));
    let (kind, rule, rule_name, fact) = if let Some(r) = body.strip_prefix("equal distances from ") {
        let (o, rest) = r.split_once(" \u{21d2} circle through ").unwrap_or((r, ""));
        let on: Vec<String> = rest.split_whitespace().map(|x| names.get(x)).collect();
        let c = names.get(o.trim());
        let mut args: Vec<String> = on.iter().map(|x| format!("{c}{x}")).collect();
        if args.is_empty() {
            args.push(c.clone());
        }
        let mut pts = vec![c];
        pts.extend(on);
        ("step", "eqradius", None, Fact::new("eqdist", args, pts))
    } else if let Some(r) = body.strip_prefix("points ") {
        let ab: Vec<&str> = r.split(" coincide").next().unwrap_or("").split(" and ").collect();
        let a: Vec<String> = ab.iter().map(|x| names.get(x.trim())).collect();
        ("step", "coincide", None, Fact::new("coincide", a.clone(), a))
    } else if let Some((rule, stmt)) = if body.starts_with("sine of ") { body.rsplit_once(": ") } else { body.split_once(": ") } {
        match rule {
            "assumption" | "construction" => {
                let f = fact_of_pred_text(problem, names, stmt).unwrap_or_else(|| formula(stmt));
                let rule = if f.points.iter().any(|p| aux.contains(p)) {
                    "aux"
                } else if rule == "assumption" {
                    "given"
                } else {
                    "construction"
                };
                ("given", rule, None, f)
            }
            "similar triangles" => {
                let tri: Vec<String> = stmt
                    .split('\u{223c}')
                    .map(|s| names.rename_text(s.trim().trim_start_matches('\u{25b3}'), false))
                    .collect();
                ("step", "similar", None, Fact::new("simtri", tri, pts_of(stmt)))
            }
            "collinear" => {
                let v: Vec<String> = stmt.split_whitespace().map(|x| names.get(x)).collect();
                ("step", "collinear", None, Fact::new("coll", v.clone(), v))
            }
            "concyclic (inscribed angles)" => {
                let raws: Vec<&str> = stmt.split_whitespace().collect();
                let v: Vec<String> = raws.iter().map(|x| names.get(x)).collect();
                let fact = if raws.len() == 3 {
                    three_on_a_circle(problem, names, &raws).unwrap_or_else(|| Fact::new("cyclic", v.clone(), v))
                } else {
                    Fact::new("cyclic", v.clone(), v)
                };
                ("step", "concyclic", None, fact)
            }
            "segment arithmetic" => {
                let s = stmt.trim_end_matches(" (add/mul transfer)");
                ("step", "transfer", None, transfer_fact(problem, names, s).unwrap_or_else(|| formula(s)))
            }
            "equal arcs \u{21d4} equal chords" => {
                let chords: Vec<Vec<String>> =
                    stmt.split(" and ").filter_map(|s| names.segment(s.trim())).filter(|v| v.len() == 2).collect();
                let fact = match chords.as_slice() {
                    [a, b] if stmt.split(" and ").count() == 2 => {
                        let mut pts: Vec<String> = Vec::new();
                        for r in a.iter().chain(b.iter()) {
                            let d = names.get(r);
                            if !pts.contains(&d) {
                                pts.push(d);
                            }
                        }
                        let seg = |v: &[String]| v.iter().map(|r| names.get(r)).collect::<String>();
                        Fact::new("cong", vec![seg(a), seg(b)], pts)
                    }
                    _ => formula(&stmt.replace(" and ", ", ")),
                };
                ("step", "arcchord", None, fact)
            }
            other if stmt.contains('=') => ("step", "theorem", Some(crate::i18n::prose_en(other)), formula(stmt)),
            other => {
                let raws: Vec<&str> = stmt.split_whitespace().collect();
                let v: Vec<String> = raws.iter().map(|x| names.get(x)).collect();
                let at: Vec<Option<Pt>> = raws.iter().map(|r| coord(problem, r)).collect();
                let fact = theorem_fact(other, &v, &at).unwrap_or_else(|| Fact::new("points", v.clone(), v));
                ("step", "theorem", Some(crate::i18n::prose_en(other)), fact)
            }
        }
    } else {
        ("step", "other", None, formula(&body))
    };
    let rule_name_ro = rule_name.as_deref().and_then(crate::i18n::theorem_ro).map(str::to_string);
    Some(Step { n, kind, rule, rule_name, rule_name_ro, fact, deps, subs: Vec::new() })
}

fn ratio_text(r: f64) -> Option<String> {
    let (p, q) = simple_ratio(r)?;
    Some(if q == 1 { p.to_string() } else { format!("{p}/{q}") })
}

fn square_ratio_text(r2: f64) -> Option<String> {
    if let Some(t) = ratio_text(r2) {
        return Some(t);
    }
    if !(r2.is_finite() && r2 > 0.0) {
        return None;
    }
    let q = (1..=144u64).find(|&q| {
        let p = (r2 * q as f64).round();
        p >= 1.0 && near(p / q as f64, r2)
    })?;
    let p = (r2 * q as f64).round() as u64;
    let g = (1..=p.min(q)).rev().find(|g| p % g == 0 && q % g == 0).unwrap_or(1);
    Some(if q / g == 1 { (p / g).to_string() } else { format!("{}/{}", p / g, q / g) })
}

fn len_at(at: &[Option<Pt>], a: usize, b: usize) -> Option<f64> {
    let (p, q) = (at.get(a).copied()??, at.get(b).copied()??);
    Some(((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt())
}

fn strictly_inside(p: Pt, a: Pt, b: Pt, c: Pt) -> bool {
    let side = |u: Pt, v: Pt| (v.0 - u.0) * (p.1 - u.1) - (v.1 - u.1) * (p.0 - u.0);
    let (s1, s2, s3) = (side(a, b), side(b, c), side(c, a));
    (s1 > 0.0 && s2 > 0.0 && s3 > 0.0) || (s1 < 0.0 && s2 < 0.0 && s3 < 0.0)
}

fn unique(p: &[String]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for x in p {
        if !v.contains(x) {
            v.push(x.clone());
        }
    }
    v
}

fn theorem_fact(name: &str, p: &[String], at: &[Option<Pt>]) -> Option<Fact> {
    let seg = |a: usize, b: usize| format!("{}{}", p[a], p[b]);
    let pts = |idx: &[usize]| idx.iter().map(|&i| p[i].clone()).collect::<Vec<_>>();
    let sq = |a: usize, b: usize| format!("{}\u{b2}", seg(a, b));
    let formula = |text: String| Fact::new("formula", vec![text], unique(p));
    if let Some(value) = name.strip_prefix("sine of ").and_then(|r| r.split_once(": ")).map(|(_, v)| v) {
        if p.len() == 3 {
            return Some(formula(format!("sin\u{2220}{}{}{} = {value}", p[0], p[1], p[2])));
        }
    }
    Some(match (name, p.len()) {
        ("Menelaus' theorem", 6) => {
            formula(format!("{} \u{b7} {} \u{b7} {} = {} \u{b7} {} \u{b7} {}", seg(1, 3), seg(2, 4), seg(0, 5), seg(3, 2), seg(4, 0), seg(5, 1)))
        }
        ("Menelaus' theorem (converse)", 6) => Fact::new("coll", pts(&[3, 4, 5]), pts(&[3, 4, 5])),
        ("Ceva's theorem (converse)", 7) => Fact::new("concur", vec![seg(0, 3), seg(1, 4), seg(2, 5), p[6].clone()], unique(p)),
        ("angle bisectors concur (incentre/excentre)", 4) => {
            let inside = match (at[0], at[1], at[2], at[3]) {
                (Some(i), Some(a), Some(b), Some(c)) => Some(strictly_inside(i, a, b, c)),
                _ => None,
            };
            let kind = match inside {
                Some(true) => "incenter",
                Some(false) => "excenter",
                None => "in_or_excenter",
            };
            Fact::new(kind, vec![p[0].clone(), format!("{}{}{}", p[1], p[2], p[3])], unique(p))
        }
        ("perpendicular \u{21d2} squared lengths (Pythagoras)", 4) => {
            let terms = |x: &[(usize, usize)]| -> String {
                x.iter().filter(|(a, b)| p[*a] != p[*b]).map(|(a, b)| sq(*a, *b)).collect::<Vec<_>>().join(" + ")
            };
            formula(format!("{} = {}", terms(&[(0, 2), (1, 3)]), terms(&[(0, 3), (1, 2)])))
        }
        ("perpendicular from squared lengths", 4) => Fact::new("perp", vec![seg(0, 1), seg(2, 3)], unique(p)),
        ("squares of proportional lengths", 4) => {
            let q = len_at(at, 2, 3).zip(len_at(at, 0, 1)).and_then(|(x, y)| square_ratio_text((x / y).powi(2)))?;
            let rhs = if q == "1" { sq(0, 1) } else { format!("{q}\u{b7}{}", sq(0, 1)) };
            formula(format!("{} = {rhs}", sq(2, 3)))
        }
        ("lengths from squared lengths", 4) => {
            let r = len_at(at, 2, 3)? / len_at(at, 0, 1)?;
            match ratio_text(r) {
                Some(k) if k == "1" => Fact::new("cong", vec![seg(2, 3), seg(0, 1)], unique(p)),
                Some(k) => Fact::new("rconst", vec![seg(2, 3), seg(0, 1), k], unique(p)),
                None => {
                    let k2 = square_ratio_text(r * r)?;
                    Fact::new("rconst", vec![seg(2, 3), seg(0, 1), format!("\u{221a}({k2})")], unique(p))
                }
            }
        }
        ("Stewart's theorem", 4) => {
            let (b, d, c) = (at[1]?, at[2]?, at[3]?);
            let bc2 = (c.0 - b.0).powi(2) + (c.1 - b.1).powi(2);
            let t = ((d.0 - b.0) * (c.0 - b.0) + (d.1 - b.1) * (c.1 - b.1)) / bc2;
            let signed = |x: f64| -> Option<String> {
                if x.abs() < 1e-9 {
                    return Some("0".into());
                }
                let r = ratio_text(x.abs())?;
                Some(if x < 0.0 { format!("\u{2212}{r}") } else { r })
            };
            let coef = |x: f64, s: String| -> Option<String> {
                Some(match signed(x)?.as_str() {
                    "0" => String::new(),
                    "1" => s,
                    k => format!("{k}\u{b7}{s}"),
                })
            };
            let terms: Vec<String> = [coef(1.0 - t, sq(0, 1))?, coef(t, sq(0, 3))?, coef(-t * (1.0 - t), sq(1, 3))?]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect();
            let rhs = terms.join(" + ").replace("+ \u{2212}", "\u{2212} ");
            formula(format!("{} = {rhs}", sq(0, 2)))
        }
        ("law of sines", 3) => formula(format!(
            "{} : sin\u{2220}{}{}{} = {} : sin\u{2220}{}{}{} = {} : sin\u{2220}{}{}{}",
            seg(1, 2), p[1], p[0], p[2], seg(2, 0), p[0], p[1], p[2], seg(0, 1), p[0], p[2], p[1]
        )),
        ("equal or supplementary angles have equal sines", 6) => {
            formula(format!("sin\u{2220}{}{}{} = sin\u{2220}{}{}{}", p[0], p[1], p[2], p[3], p[4], p[5]))
        }
        ("angle bisector theorem", 4) => {
            Fact::new("eqratio", vec![seg(1, 2), seg(1, 3), seg(0, 2), seg(0, 3)], pts(&[0, 1, 2, 3]))
        }
        ("angle bisector theorem (converse)", 4) => Fact::new(
            "eqangle",
            vec![format!("\u{2220}{}{}{}", p[2], p[0], p[1]), format!("\u{2220}{}{}{}", p[1], p[0], p[3])],
            pts(&[0, 1, 2, 3]),
        ),
        ("radical axis" | "Monge–d'Alembert", 3) => Fact::new("coll", p.to_vec(), p.to_vec()),
        ("homothety at a centre of similitude", 5) => Fact::new(
            "para_ratio",
            vec![seg(3, 1), seg(4, 2), seg(0, 1), seg(0, 2), seg(0, 3), seg(0, 4)],
            p.to_vec(),
        ),
        ("intercept theorem (parallel rungs)", 6) => Fact::new(
            "eqratio",
            vec![seg(0, 2), seg(1, 3), seg(2, 4), seg(3, 5), seg(0, 4), seg(1, 5)],
            p.to_vec(),
        ),
        _ => return None,
    })
}

fn coord(problem: &Problem, raw: &str) -> Option<Pt> {
    let p = problem.points.iter().find(|p| p.name == raw)?;
    let q = (p.value.x, p.value.y);
    finite_pt(q).then_some(q)
}

fn finite_pt(p: Pt) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * a.abs().max(b.abs()).max(1e-9)
}

fn simple_ratio(r: f64) -> Option<(u32, u32)> {
    if !(r.is_finite() && r > 0.0) {
        return None;
    }
    for q in 1..=12u32 {
        let p = (r * q as f64).round();
        if (1.0..=96.0).contains(&p) && near(p / q as f64, r) {
            let (p, q) = (p as u32, q);
            let g = (1..=p.min(q)).rev().find(|g| p % g == 0 && q % g == 0).unwrap_or(1);
            return Some((p / g, q / g));
        }
    }
    None
}

fn transfer_fact(problem: &Problem, names: &Names, s: &str) -> Option<Fact> {
    let (l, r) = s.split_once('\u{2194}')?;
    let seg = |t: &str| -> Option<(String, String)> {
        let v = names.segment(t.trim().trim_matches('|'))?;
        (v.len() == 2).then(|| (v[0].clone(), v[1].clone()))
    };
    let ((a, b), (c, d)) = (seg(l)?, seg(r)?);
    let len = |x: &str, y: &str| -> Option<f64> {
        let (p, q) = (coord(problem, x)?, coord(problem, y)?);
        Some(((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt())
    };
    let (p, q) = simple_ratio(len(&a, &b)? / len(&c, &d)?)?;
    let (s1, s2) = (format!("{}{}", names.get(&a), names.get(&b)), format!("{}{}", names.get(&c), names.get(&d)));
    let mut pts: Vec<String> = Vec::new();
    for x in [&a, &b, &c, &d] {
        let n = names.get(x);
        if !pts.contains(&n) {
            pts.push(n);
        }
    }
    Some(if p == q {
        Fact::new("cong", vec![s1, s2], pts)
    } else {
        let k = if q == 1 { p.to_string() } else { format!("{p}/{q}") };
        Fact::new("rconst", vec![s1, s2, k], pts)
    })
}

fn three_on_a_circle(problem: &Problem, names: &Names, raws: &[&str]) -> Option<Fact> {
    let pts: Vec<Pt> = raws.iter().map(|r| coord(problem, r)).collect::<Option<_>>()?;
    let d = |a: Pt, b: Pt| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
    let shown: Vec<String> = raws.iter().map(|r| names.get(r)).collect();
    for o in &problem.points {
        if raws.contains(&o.name.as_str()) {
            continue;
        }
        let c = (o.value.x, o.value.y);
        if !finite_pt(c) {
            continue;
        }
        let r = d(c, pts[0]);
        if r > 1e-9 && near(d(c, pts[1]), r) && near(d(c, pts[2]), r) {
            let on = names.get(&o.name);
            let mut all = vec![on.clone()];
            all.extend(shown.iter().cloned());
            return Some(Fact::new("oncircle", std::iter::once(on).chain(shown.iter().cloned()).collect(), all));
        }
    }
    None
}

/// Drop steps that only restate one cited step with the same points (the
/// engine's `collinear: A I M [003]` after `assumption: coll A I M`), then
/// renumber, pointing citations at the surviving step.
fn drop_restatements(steps: Vec<Step>) -> Vec<Step> {
    fn key(f: &Fact) -> (&'static str, Vec<String>) {
        let mut p = f.points.clone();
        p.sort();
        p.dedup();
        (f.kind, p)
    }
    let mut alias: HashMap<usize, usize> = HashMap::new();
    let mut facts: HashMap<usize, (&'static str, Vec<String>)> = HashMap::new();
    for s in &steps {
        facts.insert(s.n, key(&s.fact));
        if s.kind == "step" && matches!(s.rule, "collinear" | "concyclic") && s.deps.len() == 1 {
            let mut d = s.deps[0];
            while let Some(&a) = alias.get(&d) {
                d = a;
            }
            if facts.get(&d) == Some(&key(&s.fact)) {
                alias.insert(s.n, d);
            }
        }
    }
    let kept: Vec<Step> = steps.into_iter().filter(|s| !alias.contains_key(&s.n)).collect();
    let renum: HashMap<usize, usize> = kept.iter().enumerate().map(|(i, s)| (s.n, i + 1)).collect();
    kept.into_iter()
        .map(|mut s| {
            s.n = renum[&s.n];
            let mut deps: Vec<usize> = s
                .deps
                .iter()
                .filter_map(|&d| {
                    let mut d = d;
                    while let Some(&a) = alias.get(&d) {
                        d = a;
                    }
                    renum.get(&d).copied()
                })
                .collect();
            deps.sort_unstable();
            deps.dedup();
            s.deps = deps;
            s
        })
        .collect()
}

/// The theorems the Euclidean provers name in their sentences, as the step's
/// rule label (lower-case needle, label).
const KNOWN_THEOREMS: &[(&str, &str)] = &[
    ("extended law of sines", "extended law of sines"),
    ("law of sines", "law of sines"),
    ("law of cosines", "law of cosines"),
    ("sine area formula", "sine area formula"),
    ("ratio lemma", "ratio lemma"),
    ("trigonometric ceva", "trigonometric Ceva"),
    ("power of the point", "power of a point"),
    ("tangent\u{2013}secant power", "power of a point"),
    ("thales' theorem", "Thales' theorem"),
    ("pythagorean theorem", "Pythagorean theorem"),
    ("stewart's theorem", "Stewart's theorem"),
    ("apollonius's median theorem", "Apollonius's median theorem"),
    ("menelaus's theorem", "Menelaus's theorem"),
    ("ceva's theorem", "Ceva's theorem"),
    ("angle-bisector theorem", "angle-bisector theorem"),
    ("basic proportionality (intercept) theorem", "basic proportionality (intercept) theorem"),
    ("geometric-mean (altitude) relation", "geometric-mean (altitude) relation"),
    ("geometric-mean (leg) relation", "geometric-mean (leg) relation"),
    ("ptolemy's theorem", "Ptolemy's theorem"),
];

/// The named theorem a Euclidean sentence uses: the earliest one it mentions
/// (`Ratio lemma … By the law of sines …` is the ratio lemma).
fn known_theorem(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    KNOWN_THEOREMS
        .iter()
        .filter_map(|(needle, label)| lower.find(needle).map(|at| (at, std::cmp::Reverse(needle.len()), *label)))
        .min()
        .map(|(_, _, label)| label.to_string())
}

/// The theorem a Euclidean prose step cites (`… by Stewart's theorem …`).
fn cited_theorem(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let at = lower.find("by ")?;
    let mut rest = &text[at + 3..];
    if let Some(r) = rest.strip_prefix("the ") {
        rest = r;
    }
    let mut words = Vec::new();
    for w in rest.split_whitespace() {
        let clean = w.trim_end_matches([',', ':', '.', ';']);
        words.push(clean);
        let l = clean.to_lowercase();
        if ["theorem", "formula", "law", "lemma", "identity", "rule", "relation", "inequality"]
            .iter()
            .any(|k| l.starts_with(k))
        {
            return Some(words.join(" "));
        }
        if words.len() > 6 || w.ends_with([',', ':', '.']) {
            break;
        }
    }
    None
}

fn drop_value_echo(s: &str) -> String {
    match s.find("  (= ") {
        Some(at) => {
            let close = s[at..].find(')').map_or(s.len(), |c| at + c + 1);
            format!("{}{}", &s[..at], &s[close..])
        }
        None => s.to_string(),
    }
}

fn fractions(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        let digit_before = i > 0 && c[i - 1].is_ascii_digit();
        let digit_after = i + 1 < c.len() && c[i + 1].is_ascii_digit();
        let run_start = (0..i).rev().take_while(|&k| c[k].is_ascii_digit()).last().unwrap_or(i);
        let lone = run_start == 0 || !matches!(c[run_start - 1], '/' | '.' | ',');
        if c[i] == '/' && digit_before && digit_after && lone {
            out.push('\u{2044}');
            i += 1;
            while i < c.len() && c[i].is_ascii_digit() {
                out.push(c[i]);
                i += 1;
            }
            if i < c.len() && c[i] == '\u{b7}' {
                out.push_str("\u{2009}\u{b7}\u{2009}");
                i += 1;
            }
            continue;
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn as_drawn(s: &str) -> String {
    match (s.find(" lies on "), s.find(", and ")) {
        (Some(a), Some(b)) if a < b && s[a..b].contains(" between ") => {
            format!("{} (as drawn){}", &s[..b], &s[b..])
        }
        _ => s.to_string(),
    }
}

fn unbar(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < c.len() {
        if c[i] == '|' {
            let j = (i + 1..c.len()).take_while(|&k| is_name_char(c[k])).last().map_or(i, |k| k + 1);
            if j > i + 1 && c.get(j) == Some(&'|') && c[i + 1].is_ascii_uppercase() {
                out.extend(&c[i + 1..j]);
                i = j + 1;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

fn circle_through(s: &str, names: &Names) -> String {
    let key = " through ";
    let Some(at) = s.find(key) else { return s.to_string() };
    if !s[..at].ends_with(')') && !s[..at].ends_with("circle") {
        return s.to_string();
    }
    let rest = &s[at + key.len()..];
    let run: String = rest.chars().take_while(|c| is_name_char(*c)).collect();
    match names.segment(&run) {
        Some(v) if v.len() >= 3 => format!("{}{key}{}{}", &s[..at], v.join(", "), &rest[run.len()..]),
        _ => s.to_string(),
    }
}

const CLOSURE_HEADING: &str ="Facts derived from the hypotheses by the deductive closure";

/// A Euclidean sentence as a `prose` fact, typeset, renamed and translated.
fn euclid_prose(s: &str, names: &Names) -> Fact {
    let typeset = fractions(&pretty_metric_inline(&as_drawn(&circle_through(&unbar(s), names)), names));
    let shown = caret_powers(&trig_powers(&names.rename_text(&typeset, true)));
    let mut f = Fact::new("prose", vec![crate::i18n::prose_en(&shown)], names.disp_all(&names.mentioned(s, true)));
    f.ro = Some(vec![crate::i18n::prose_ro(&shown)]);
    f
}

/// The prover's `  [from 2, 4]` back-reference, split off the sentence.
fn split_from(body: &str) -> (String, Vec<usize>) {
    let t = body.trim_end();
    if t.ends_with(']') {
        if let Some(open) = t.rfind("[from ") {
            let deps: Vec<usize> = t[open + 6..t.len() - 1]
                .split([',', '&'])
                .filter_map(|d| d.trim().parse().ok())
                .collect();
            if !deps.is_empty() {
                return (t[..open].trim_end().to_string(), deps);
            }
        }
    }
    (t.to_string(), Vec::new())
}

/// Reword the prover's mechanical phrasings: `X (derived from the
/// hypotheses), so X.` loses its echo, and a trailing `; times k` says what is
/// multiplied.
fn smooth_euclid(body: &str) -> String {
    let mut s = body.to_string();
    let derived = " (derived from the hypotheses), so ";
    if let Some((a, b)) = s.split_once(derived) {
        if b.trim_end_matches('.').trim() == a.trim() {
            s = format!("{} (derived from the hypotheses).", a.trim());
        }
    }
    if let Some(at) = s.rfind("; times ") {
        let rest = s[at + 8..].to_string();
        s = match rest.split_once(": ") {
            Some((k, after)) => format!("{}; multiplying both sides by {k}: {after}", &s[..at]),
            None => {
                let k = rest.trim_end_matches('.').trim();
                let head = &s[..at];
                match head.rfind(", so ").map(|so| (so, scaled_relation(&head[so + 5..], k))) {
                    Some((_, Some(rel))) => format!("{head}, hence {rel}."),
                    _ => format!("{head} (both sides multiplied by {k})."),
                }
            }
        };
    }
    s
}

fn product(k: &str, side: &str) -> String {
    let side = side.trim();
    if side == "0" {
        return "0".to_string();
    }
    let sum = side.contains(" + ") || side.contains(" \u{2212} ") || side.contains(" - ");
    if sum { format!("{k}\u{b7}({side})") } else { format!("{k}\u{b7}{side}") }
}

fn scaled_relation(rel: &str, k: &str) -> Option<String> {
    let rel = rel.trim().trim_end_matches('.');
    let rel = rel.rsplit_once(": ").map_or(rel, |(_, r)| r).trim();
    let wordy = rel
        .split(|c: char| !c.is_alphabetic())
        .any(|w| w.chars().count() >= 3 && w.chars().all(char::is_lowercase) && !matches!(w, "sin" | "cos" | "tan" | "cot"));
    let (l, r) = rel.split_once(" = ")?;
    if wordy || r.contains(" = ") || k.is_empty() {
        return None;
    }
    Some(format!("{} = {}", product(k, l), product(k, r)))
}

fn multiplied_claim(goal: &str, k: &str) -> Option<String> {
    let (l, r) = goal.split_once(" = ")?;
    if r.contains(" = ") {
        return None;
    }
    let side = |s: &str| {
        let s = s.trim();
        if s == k && k.chars().all(|c| is_name_char(c) || ('\u{2080}'..='\u{2089}').contains(&c) || c == '\u{2032}') {
            format!("{k}\u{b2}")
        } else if s.contains(" + ") || s.contains(" \u{2212} ") {
            format!("({s}) \u{b7} {k}")
        } else {
            format!("{s} \u{b7} {k}")
        }
    };
    Some(format!("{} = {}", side(l), side(r)))
}

fn claim_multiplier(body: &str, goal: Option<&str>) -> String {
    let Some(rest) = body.strip_prefix("Multiply both sides by ") else { return body.to_string() };
    let (k, why) = match rest.split_once(", which is not 0") {
        Some((k, why)) => (k.trim(), why),
        None => return body.to_string(),
    };
    let reason = if why.starts_with(": every angle") {
        ", as every angle in it is strictly between 0\u{b0} and 180\u{b0}"
    } else {
        ""
    };
    match goal.and_then(|g| multiplied_claim(g, k)) {
        Some(c) => format!("Multiply both sides of the claim by {k} ({k} \u{2260} 0{reason}): it becomes {c}."),
        None => format!("Multiply both sides of the claim by {k} ({k} \u{2260} 0{reason})."),
    }
}

/// What kind of step a Euclidean sentence is: `(kind, rule, theorem)`.
fn euclid_rule(body: &str) -> (&'static str, &'static str, Option<String>) {
    if body.starts_with("Let ") {
        return ("construction", "construction", None);
    }
    if body.starts_with(CLOSURE_HEADING) {
        return ("step", "closure", None);
    }
    if let Some(name) = known_theorem(body).or_else(|| cited_theorem(body)) {
        return ("step", "theorem", Some(name));
    }
    if body.contains("(given)") || body.contains("(hypothesis)") || body.starts_with("Given: ") {
        let derives = body.contains(", so ");
        return (if derives { "step" } else { "given" }, "given", None);
    }
    if body.starts_with("From the sine relations") {
        return ("step", "trig", None);
    }
    if body.starts_with("Multiply both sides") {
        return ("step", "rearrange", None);
    }
    if is_pythagorean_identity(body) {
        return ("step", "theorem", Some("Pythagorean identity".to_string()));
    }
    if body.contains("area splits along") {
        return ("step", "area", None);
    }
    if body.contains(" lies on ") && body.contains(" + ") {
        return ("step", "segments", None);
    }
    if body.contains("(derived from the hypotheses)") {
        return ("step", "other", None);
    }
    ("step", "combine", None)
}

fn is_pythagorean_identity(body: &str) -> bool {
    let t: String = trig_powers(body).chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.trim_end_matches('.');
    let Some(lhs) = t.strip_suffix("=1") else { return false };
    let Some((a, b)) = lhs.split_once('+') else { return false };
    let arg = |x: &str, f: &str| x.strip_prefix(f).and_then(|r| r.strip_prefix('\u{b2}')).map(str::to_string);
    match (arg(a, "sin"), arg(b, "cos"), arg(a, "cos"), arg(b, "sin")) {
        (Some(x), Some(y), _, _) | (_, _, Some(x), Some(y)) => x == y && x.starts_with('\u{2220}'),
        _ => false,
    }
}

fn euclid_sub(line: &str, names: &Names) -> SubStep {
    let (_, rule, rule_name) = euclid_rule(line);
    let rule = if rule == "closure" || rule == "construction" || rule == "given" { "other" } else { rule };
    let rule_name_ro = rule_name.as_deref().and_then(crate::i18n::theorem_ro).map(str::to_string);
    SubStep { rule, rule_name, rule_name_ro, fact: euclid_prose(&smooth_euclid(line), names) }
}

/// The closing sentence: the echoed goal typeset like the PROVE card, and
/// `Adding the relations above` said as `Hence` when there is only one.
fn euclid_conclusion(line: &str, names: &Names, single: bool) -> Fact {
    let t = drop_value_echo(line.trim_end_matches('\u{220e}').trim());
    let t = t.trim().trim_end_matches('.').trim();
    let (lead, goal) = match t.rfind(" gives ") {
        Some(at) => (&t[..at + 7], &t[at + 7..]),
        None => (t, ""),
    };
    let combined = ["Adding the relations above gives ", "Combining the equations above gives ", "Combining the proportions above gives "];
    let lead = if single && combined.contains(&lead) { "Hence " } else { lead };
    let goal_raw_pts = names.disp_all(&names.mentioned(goal, false));
    let typeset = format!("{}{}.", names.rename_text(&fractions(lead), true), pretty_metric(goal, names));
    let mut f = Fact::new("prose", vec![crate::i18n::prose_en(&typeset)], Vec::new());
    f.ro = Some(vec![crate::i18n::prose_ro(&typeset)]);
    let mut pts = names.disp_all(&names.mentioned(lead, true));
    for p in goal_raw_pts {
        if !pts.contains(&p) {
            pts.push(p);
        }
    }
    f.points = pts;
    f
}

fn parse_euclid_proof(text: &str, problem: &Problem, names: &Names) -> ProofView {
    let introduced: Vec<String> = text
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
            let body = t.get(digits..)?.strip_prefix(". Let ")?;
            body.split_whitespace().next().map(str::to_string)
        })
        .collect();
    let names = &names.with_points(&introduced);
    let goal = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("Goal:"))
        .map(|g| pretty_metric(g.trim(), names));
    let none = HashSet::new();
    let mut steps: Vec<Step> = Vec::new();
    let mut closures: HashMap<usize, Vec<Step>> = HashMap::new();
    let mut last_line = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(r) = t.strip_prefix('\u{21b3}') {
            if let (Some(st), Some(owner)) = (ddar_line(r.trim(), problem, names, &none), steps.last()) {
                closures.entry(owner.n).or_default().push(st);
            }
            continue;
        }
        if let Some(r) = t.strip_prefix('\u{b7}') {
            if let Some(owner) = steps.last_mut() {
                owner.subs.push(euclid_sub(r.trim(), names));
            }
            continue;
        }
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() && t[digits.len()..].starts_with(". ") {
            let (body, deps) = split_from(&t[digits.len() + 2..]);
            let body = claim_multiplier(&smooth_euclid(&body), goal.as_deref());
            let (kind, rule, rule_name) = euclid_rule(&body);
            let rule_name_ro = rule_name.as_deref().and_then(crate::i18n::theorem_ro).map(str::to_string);
            steps.push(Step {
                n: digits.parse().unwrap_or(0),
                kind,
                rule,
                rule_name,
                rule_name_ro,
                fact: euclid_prose(&body, names),
                deps,
                subs: Vec::new(),
            });
        } else if t.ends_with('\u{220e}') {
            last_line = Some(t.to_string());
        }
    }
    let mut gone: HashSet<usize> = HashSet::new();
    let cited = cited_closure_facts(text, names);
    for st in steps.iter_mut() {
        if st.rule == "closure" {
            let lines = drop_restatements(closures.remove(&st.n).unwrap_or_default());
            let known: Vec<Vec<Vec<String>>> = lines.iter().filter(|s| s.kind == "given").filter_map(|s| cong_key(&s.fact)).collect();
            let mut derived: Vec<SubStep> = Vec::new();
            for s in lines.into_iter().filter(|s| s.kind == "step") {
                let (rule, fact) = match mirror_similarity(&s.fact) {
                    Some(f) => ("other", f),
                    None => (s.rule, s.fact),
                };
                let restated = cong_key(&fact).is_some_and(|k| known.contains(&k));
                let repeated = derived.iter().any(|d| d.fact.kind == fact.kind && d.fact.args == fact.args);
                if !restated && !repeated {
                    derived.push(SubStep { rule, rule_name: s.rule_name, rule_name_ro: s.rule_name_ro, fact });
                }
            }
            for (owner, claim) in &cited {
                let same = |d: &SubStep| (d.fact.kind == claim.kind && d.fact.args == claim.args) || (cong_key(claim).is_some() && cong_key(&d.fact) == cong_key(claim));
                if *owner == st.n && !derived.iter().any(same) {
                    derived.push(SubStep { rule: "derived", rule_name: None, rule_name_ro: None, fact: claim.clone() });
                }
            }
            if derived.is_empty() {
                gone.insert(st.n);
            }
            st.subs = derived;
        }
        for sub in st.subs.iter() {
            for p in &sub.fact.points {
                if !st.fact.points.contains(p) {
                    st.fact.points.push(p.clone());
                }
            }
        }
    }
    let kept: Vec<Step> = steps.into_iter().filter(|s| !gone.contains(&s.n)).collect();
    let renum: HashMap<usize, usize> = kept.iter().enumerate().map(|(i, s)| (s.n, i + 1)).collect();
    let steps: Vec<Step> = kept
        .into_iter()
        .map(|mut s| {
            s.n = renum[&s.n];
            s.deps = s.deps.iter().filter_map(|d| renum.get(d).copied()).collect();
            s
        })
        .collect();
    let relations = steps.iter().filter(|s| s.kind != "construction" && s.rule != "closure").count();
    let conclusion = last_line.map(|l| euclid_conclusion(&l, names, relations == 1));
    ProofView { steps, conclusion, style: "euclidean" }
}

fn split_disp(run: &str, points: &[String]) -> Option<Vec<String>> {
    let mut rest = run;
    let mut out = Vec::new();
    while !rest.is_empty() {
        let p = points.iter().filter(|p| rest.starts_with(p.as_str())).max_by_key(|p| p.len())?;
        out.push(p.clone());
        rest = &rest[p.len()..];
    }
    Some(out)
}

fn mirror_similarity(f: &Fact) -> Option<Fact> {
    if f.kind != "simtri" || f.args.len() != 2 {
        return None;
    }
    let (a, b) = (split_disp(&f.args[0], &f.points)?, split_disp(&f.args[1], &f.points)?);
    if a.len() != 3 || b.len() != 3 {
        return None;
    }
    let fixed: Vec<usize> = (0..3).filter(|&i| a[i] == b[i]).collect();
    let [fx] = fixed[..] else { return None };
    let (i, j) = match fx {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    if a[i] != b[j] || a[j] != b[i] {
        return None;
    }
    let (c, x, y) = (&a[fx], &a[i], &a[j]);
    Some(Fact::new("cong", vec![format!("{c}{x}"), format!("{c}{y}")], vec![c.clone(), x.clone(), y.clone()]))
}

fn cong_key(f: &Fact) -> Option<Vec<Vec<String>>> {
    if f.kind != "cong" || f.args.len() != 2 {
        return None;
    }
    let mut segs: Vec<Vec<String>> = f
        .args
        .iter()
        .map(|a| {
            let mut v = split_disp(a, &f.points)?;
            v.sort();
            (v.len() == 2).then_some(v)
        })
        .collect::<Option<_>>()?;
    segs.sort();
    Some(segs)
}

fn cited_closure_facts(text: &str, names: &Names) -> Vec<(usize, Fact)> {
    let marker = " (derived from the hypotheses)";
    let mut closure_steps: Vec<usize> = Vec::new();
    let mut out: Vec<(usize, Fact)> = Vec::new();
    for line in text.lines() {
        let t = line.trim().trim_start_matches('\u{b7}').trim();
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        let numbered = !digits.is_empty() && t[digits.len()..].starts_with(". ");
        let body = if numbered { &t[digits.len() + 2..] } else { t };
        if numbered && body.starts_with(CLOSURE_HEADING) {
            closure_steps.push(digits.parse().unwrap_or(0));
            continue;
        }
        let (body, deps) = split_from(body);
        let Some(at) = body.find(marker) else { continue };
        let owner = if numbered {
            deps.iter().copied().find(|d| closure_steps.contains(d))
        } else {
            closure_steps.last().copied()
        };
        let Some(owner) = owner else { continue };
        let raw = body[..at].trim();
        let claim = match raw.split_once(" = ").map(|(l, r)| (names.segment(l.trim()), names.segment(r.trim()))) {
            Some((Some(l), Some(r))) if l.len() == 2 && r.len() == 2 => {
                let mut pts: Vec<String> = Vec::new();
                for x in l.iter().chain(r.iter()) {
                    let d = names.get(x);
                    if !pts.contains(&d) {
                        pts.push(d);
                    }
                }
                Fact::new("cong", vec![names.get(&l[0]) + &names.get(&l[1]), names.get(&r[0]) + &names.get(&r[1])], pts)
            }
            _ => euclid_prose(raw, names),
        };
        if !out.iter().any(|(o, f)| *o == owner && f.args == claim.args) {
            out.push((owner, claim));
        }
    }
    out
}

// ------------------------------------------------------------------- note --

/// The engine's note, classified so the client can say it in the reader's
/// language. `raw` keeps the engine's own words for the "details" toggle.
#[derive(Serialize, Clone, Debug)]
pub struct Note {
    pub key: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<usize>,
    pub raw: String,
}

fn first_number_after(s: &str, marker: &str) -> Option<f64> {
    let at = s.find(marker)? + marker.len();
    let rest = s[at..].trim_start();
    let mut num = String::new();
    for (i, c) in rest.chars().enumerate() {
        let sign_ok = i == 0 || num.ends_with(['e', 'E']);
        if c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && sign_ok) || ((c == 'e' || c == 'E') && !num.is_empty()) {
            num.push(c);
        } else {
            break;
        }
    }
    num.parse().ok()
}

fn number_before(s: &str, marker: &str) -> Option<f64> {
    let at = s.find(marker)?;
    let head = s[..at].trim_end();
    let num: String = head
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    num.parse().ok()
}

pub fn classify_note(sol: &Solution) -> Note {
    let raw = sol.note.clone();
    let s = raw.as_str();
    let mut note = Note { key: "other", secs: None, n: None, runs: None, raw: raw.clone() };
    if s.contains("time limit") {
        note.key = "time_limit";
        note.secs = number_before(s, "s time limit").or_else(|| first_number_after(s, "within the "));
    } else if s.contains("rejected as unsound") {
        note.key = "unsound";
    } else if s.contains("could not be replayed") {
        note.key = "replay";
    } else if s.contains("does not hold in the figure") || s.contains("fails in a sampled figure") {
        note.key = "false";
    } else if s.contains("no classical Euclidean proof") {
        note.key = "no_euclid";
        note.n = sol.numeric_samples;
    } else if s.starts_with("metric prover") {
        note.key = "metric_error";
    } else if s.contains("budget") && !sol.proved {
        note.key = "budget";
        note.secs = number_before(s, "s budget");
        note.runs = number_before(s, " DDAR runs").map(|x| x as usize);
    } else if s.starts_with("shortest proof found") {
        note.key = "shortest";
        note.n = sol.examined;
    } else if s.starts_with("proved by DDAR") {
        note.key = "ddar";
    } else if s.starts_with("proved with") {
        note.key = "aux";
        note.n = number_before(s, " auxiliary").map(|x| x as usize);
        note.runs = number_before(s, " DDAR runs").map(|x| x as usize);
    } else if s.contains("classical Euclidean") {
        note.key = "euclid";
    }
    note
}

// ------------------------------------------------------- counterexamples --

/// What the sampled figure shows when the statement fails there.
#[derive(Serialize, Clone, Debug)]
pub struct Counter {
    /// `angle` (lhs° vs rhs°), `length` (two lengths), `values` (LHS vs RHS of
    /// an equation), `off_circle`, `off_line`.
    pub kind: &'static str,
    pub lhs: f64,
    pub rhs: f64,
    /// Typeset labels for the two sides, when meaningful.
    pub labels: Vec<String>,
}

fn dirn(a: Pt, b: Pt) -> f64 {
    (b.1 - a.1).atan2(b.0 - a.0)
}

/// Unsigned angle between two lines, in degrees, in [0, 90].
fn line_angle(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    let mut t = (dirn(a, b) - dirn(c, d)).to_degrees().rem_euclid(180.0);
    if t > 90.0 {
        t = 180.0 - t;
    }
    t
}

fn vertex_angle(at: &dyn Fn(u32) -> Pt, a: u32, b: u32, c: u32, d: u32) -> Option<f64> {
    let (mut a, mut b, mut c, mut d) = (a, b, c, d);
    if b == c || b == d {
        std::mem::swap(&mut a, &mut b);
    }
    if a == d {
        std::mem::swap(&mut c, &mut d);
    }
    if a != c || b == d {
        return None;
    }
    let (v, p, q) = (at(a), at(b), at(d));
    let (u1, u2) = ((p.0 - v.0, p.1 - v.1), (q.0 - v.0, q.1 - v.1));
    let n = (u1.0.hypot(u1.1)) * (u2.0.hypot(u2.1));
    if n < 1e-18 {
        return None;
    }
    Some(((u1.0 * u2.0 + u1.1 * u2.1) / n).clamp(-1.0, 1.0).acos().to_degrees())
}

/// Directed angle between lines mod 180°.
fn dir_angle(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    (dirn(c, d) - dirn(a, b)).to_degrees().rem_euclid(180.0)
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn counterexample(problem: &Problem, names: &Names, sol: &Solution) -> Option<Counter> {
    if sol.status != Status::Refuted {
        return None;
    }
    if let Some(ev) = &sol.numeric_evidence {
        let lhs = first_number_after(ev, "LHS =")?;
        let rhs = first_number_after(ev, "RHS =")?;
        let goal = sol.goal.clone().unwrap_or_default();
        let labels = goal
            .split_once('=')
            .map(|(l, r)| vec![pretty_metric(l, names), pretty_metric(r, names)])
            .unwrap_or_default();
        return Some(Counter { kind: "values", lhs, rhs, labels });
    }
    let goal = problem.goal.as_ref()?;
    let c = |i: usize| {
        let v = problem.points[goal.points[i] as usize].value;
        (v.x, v.y)
    };
    let n = |i: usize| names.get(problem.point_name(goal.points[i]));
    let seg = |i: usize| format!("{}{}", n(i), n(i + 1));
    match (goal.name.as_str(), goal.points.len()) {
        ("perp", 4) => Some(Counter {
            kind: "angle",
            lhs: line_angle(c(0), c(1), c(2), c(3)),
            rhs: 90.0,
            labels: vec![seg(0), seg(2)],
        }),
        ("para", 4) => Some(Counter {
            kind: "angle",
            lhs: line_angle(c(0), c(1), c(2), c(3)),
            rhs: 0.0,
            labels: vec![seg(0), seg(2)],
        }),
        ("cong", 4) => Some(Counter {
            kind: "length",
            lhs: dist(c(0), c(1)),
            rhs: dist(c(2), c(3)),
            labels: vec![seg(0), seg(2)],
        }),
        ("eqangle", 8) => {
            let f = fact_of_pred(problem, names, goal);
            let p = &goal.points;
            let at = |i: u32| {
                let v = problem.points[i as usize].value;
                (v.x, v.y)
            };
            let vertex = |k: usize| vertex_angle(&at, p[k], p[k + 1], p[k + 2], p[k + 3]);
            match (vertex(0), vertex(4)) {
                (Some(x), Some(y)) if (x - y).abs() > 0.05 => Some(Counter { kind: "angles", lhs: x, rhs: y, labels: f.args }),
                (Some(x), Some(_)) => Some(Counter { kind: "angles_oriented", lhs: x, rhs: x, labels: f.args }),
                _ => {
                    let a1 = dir_angle(c(0), c(1), c(2), c(3));
                    let a2 = dir_angle(c(4), c(5), c(6), c(7));
                    Some(Counter { kind: "line_angles", lhs: a1, rhs: a2, labels: f.args })
                }
            }
        }
        ("eqratio", 8) => {
            let r1 = dist(c(0), c(1)) / dist(c(2), c(3));
            let r2 = dist(c(4), c(5)) / dist(c(6), c(7));
            (r1.is_finite() && r2.is_finite()).then(|| Counter {
                kind: "ratios",
                lhs: r1,
                rhs: r2,
                labels: vec![format!("{} : {}", seg(0), seg(2)), format!("{} : {}", seg(4), seg(6))],
            })
        }
        ("coll", k) if k >= 3 => {
            let (a, b) = (c(0), c(1));
            let p = c(2);
            let len = dist(a, b).max(1e-12);
            let off = ((b.0 - a.0) * (a.1 - p.1) - (a.0 - p.0) * (b.1 - a.1)).abs() / len;
            Some(Counter { kind: "off_line", lhs: off, rhs: len, labels: vec![n(2), seg(0)] })
        }
        ("cyclic", k) if k >= 4 => {
            let (a, b, cc, d) = (c(0), c(1), c(2), c(3));
            let (ox, oy, r) = circumcircle(a, b, cc)?;
            let off = (dist((ox, oy), d) - r).abs();
            Some(Counter {
                kind: "off_circle",
                lhs: off,
                rhs: r,
                labels: vec![n(3), format!("{}{}{}", n(0), n(1), n(2))],
            })
        }
        _ => None,
    }
}

pub fn circumcircle(a: Pt, b: Pt, c: Pt) -> Option<(f64, f64, f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-12 {
        return None;
    }
    let sq = |p: Pt| p.0 * p.0 + p.1 * p.1;
    let ux = (sq(a) * (b.1 - c.1) + sq(b) * (c.1 - a.1) + sq(c) * (a.1 - b.1)) / d;
    let uy = (sq(a) * (c.0 - b.0) + sq(b) * (a.0 - c.0) + sq(c) * (b.0 - a.0)) / d;
    let r = dist((ux, uy), a);
    r.is_finite().then_some((ux, uy, r))
}

// ------------------------------------------------------------ evidence --

#[derive(Serialize, Clone, Debug)]
pub struct Evidence {
    pub samples: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

fn numeric_support(sol: &Solution) -> Option<Evidence> {
    if sol.status != Status::HoldsNumerically {
        return None;
    }
    let value = sol
        .numeric_evidence
        .as_deref()
        .and_then(|e| first_number_after(e, "value:"));
    Some(Evidence { samples: sol.numeric_samples.unwrap_or(0), value })
}

// ------------------------------------------------------------------ source --

/// Shapes a `.geo` program declares with several names at once
/// (`A B C = triangle`): their sides belong in the drawing.
fn source_polygons(src: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let Some((lhs, rhs)) = stmt.split_once('=') else { continue };
            let names: Vec<String> = lhs.split_whitespace().map(str::to_string).collect();
            if names.len() < 2 || !names.iter().all(|n| n.chars().all(is_name_char)) {
                continue;
            }
            let shape = rhs.trim().split(['(', ' ']).next().unwrap_or("");
            if matches!(
                shape,
                "triangle" | "iso_triangle" | "eq_triangle" | "segment" | "square" | "quadrilateral" | "right_triangle"
            ) {
                out.push(names);
            }
        }
    }
    out
}

/// `dist(P,Q) = 6` constraints in the construction lines: given lengths.
fn source_lengths(src: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with("prove") || line.starts_with("goal:") || line.starts_with('?') {
            continue;
        }
        let mut rest = line;
        while let Some(at) = rest.find("dist(") {
            let after = &rest[at + 5..];
            let Some(close) = after.find(')') else { break };
            let args: Vec<&str> = after[..close].split(',').map(str::trim).collect();
            let tail = after[close + 1..].trim_start();
            if args.len() == 2 {
                if let Some(t) = tail.strip_prefix('=') {
                    let num: String = t
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '/')
                        .collect();
                    if !num.is_empty() {
                        out.push((args[0].to_string(), args[1].to_string(), num));
                    }
                }
            }
            rest = &after[close + 1..];
        }
    }
    out
}

fn is_plain_length(c: &str) -> bool {
    let Some(rest) = c.trim().strip_prefix("dist(") else { return false };
    let Some((args, tail)) = rest.split_once(')') else { return false };
    let num = tail.trim_start().strip_prefix('=').map(str::trim).unwrap_or("");
    args.split(',').count() == 2 && !num.is_empty() && num.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '/')
}

fn source_point_metrics(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in src.lines() {
        for stmt in raw.split('#').next().unwrap_or("").split(';') {
            let Some((_, cons)) = stmt.split_once("point:") else { continue };
            for c in split_top(cons, &[","]) {
                let c = c.trim();
                let metric_only = ['^', '*', '+'].iter().any(|k| c.contains(*k)) || ["sqrt(", "sin(", "cos(", "tan("].iter().any(|k| c.contains(k));
                if c.contains('=') && metric_only && ["dist(", "angle(", "area("].iter().any(|k| c.contains(k)) && !is_plain_length(c) {
                    out.push(c.to_string());
                }
            }
        }
    }
    out
}

fn caret_powers(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let after_base = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == ')' || chars[i - 1] == '\u{2032}');
        if chars[i] == '^' && after_base && chars.get(i + 1).is_some_and(char::is_ascii_digit) {
            let mut j = i + 1;
            let mut n = String::new();
            while j < chars.len() && chars[j].is_ascii_digit() {
                n.push(chars[j]);
                j += 1;
            }
            out.push_str(&superscript(&n));
            i = j;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The goal as written in the program (`prove …`).
pub fn source_goal(src: &str) -> Option<String> {
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            if let Some(g) = s
                .strip_prefix("prove ")
                .or_else(|| s.strip_prefix("goal:"))
                .or_else(|| s.strip_prefix('?'))
            {
                return Some(g.trim().to_string());
            }
        }
    }
    None
}

/// The metric relations a program states with `assume` (`assume angle(B, A1, C)
/// + … = 480`), as written.
fn source_assumes(src: &str) -> Vec<String> {
    src.lines()
        .flat_map(|raw| raw.split('#').next().unwrap_or("").split(';').map(str::trim).map(str::to_string).collect::<Vec<_>>())
        .filter_map(|s| s.strip_prefix("assume ").map(|a| a.trim().to_string()))
        .filter(|a| a.contains('=') && ["dist(", "angle(", "area("].iter().any(|k| a.contains(k)))
        .collect()
}

/// A title from the program's leading comment (`# Stewart's theorem — …`).
pub fn source_title(src: &str) -> Option<String> {
    let line = src.lines().map(str::trim).find(|l| !l.is_empty())?;
    let text = line.strip_prefix('#')?.trim();
    let mut cut = text.len();
    for sep in [" \u{2014} ", " - ", " (", ": ", "; "] {
        if let Some(i) = text.find(sep) {
            cut = cut.min(i);
        }
    }
    let head = text[..cut].trim();
    let generic = ["THEOREM", "LEMMA", "PROBLEM", "EXAMPLE", "FACT"].iter().any(|g| head.eq_ignore_ascii_case(g));
    let picked = match (generic, text[cut..].trim_start().strip_prefix('(')) {
        (true, Some(rest)) => rest.split(')').next().unwrap_or(head).trim(),
        _ => head,
    };
    let mut t: String = picked.chars().take(80).collect();
    if picked.chars().count() > 80 {
        t = t.chars().take(79).collect::<String>() + "\u{2026}";
    }
    (!t.is_empty()).then_some(t)
}

// -------------------------------------------------------------------- aux --

#[derive(Serialize, Clone, Debug)]
pub struct AuxView {
    pub name: String,
    /// Construction kind (`midpoint`, `intersect`, `reflect`, …) for the
    /// client's sentence templates.
    pub kind: String,
    /// The construction's arguments, renamed (`["MB", "circumcircle(O,P,Q)"]`).
    pub args: Vec<String>,
    /// The engine's description with renamed points, as a fallback.
    pub text: String,
}

fn split_top(s: &str, seps: &[&str]) -> Vec<String> {
    let mut parts = vec![];
    let mut depth = 0;
    let mut cur = String::new();
    let mut i = 0;
    let b = s.as_bytes();
    'outer: while i < s.len() {
        let ch = b[i] as char;
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
        }
        if depth == 0 {
            for sep in seps {
                if s[i..].starts_with(sep) {
                    parts.push(std::mem::take(&mut cur).trim().to_string());
                    i += sep.len();
                    continue 'outer;
                }
            }
        }
        let c = s[i..].chars().next().unwrap();
        cur.push(c);
        i += c.len_utf8();
    }
    parts.push(cur.trim().to_string());
    parts
}

fn aux_view(raw: &str, names: &Names) -> AuxView {
    let (lhs, rhs) = raw.split_once(" = ").unwrap_or(("", raw));
    let name = names.get(lhs.trim());
    let rhs = rhs.trim();
    let (kind, inner) = match rhs.find('(') {
        Some(i) if rhs.ends_with(')') => (rhs[..i].to_string(), &rhs[i + 1..rhs.len() - 1]),
        _ => (String::from("other"), rhs),
    };
    let args = split_top(inner, &[",", " -> ", " over ", " in ", " on ", " of ", " to "])
        .into_iter()
        .filter(|a| !a.is_empty())
        .map(|a| names.rename_text(&a, false))
        .collect();
    AuxView { name, kind, args, text: names.rename_text(rhs, false) }
}

struct Introduced {
    raw: String,
    at: Pt,
    preds: Vec<(&'static str, Vec<String>)>,
    kind: &'static str,
    args: Vec<String>,
    text: String,
}

type Found = (Pt, Vec<(&'static str, Vec<String>)>, &'static str, Vec<String>);

fn line_meet(a: Pt, b: Pt, c: Pt, d: Pt) -> Option<Pt> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-12 * (r.0.hypot(r.1) * s.0.hypot(s.1)).max(1e-30) {
        return None;
    }
    let t = ((c.0 - a.0) * s.1 - (c.1 - a.1) * s.0) / den;
    Some((a.0 + r.0 * t, a.1 + r.1 * t))
}

struct Intro<'a> {
    fig: &'a Problem,
    nm: Names,
    done: &'a [Introduced],
    raw: String,
}

impl Intro<'_> {
    fn xy(&self, r: &str) -> Option<Pt> {
        coord(self.fig, r).or_else(|| self.done.iter().find(|i| i.raw == r).map(|i| i.at))
    }
    fn run(&self, s: &str, n: usize) -> Option<Vec<String>> {
        self.nm.segment(s.trim().trim_end_matches('.')).filter(|v| v.len() == n)
    }
    fn shown(&self, v: &[String]) -> String {
        v.iter().map(|r| self.nm.get(r)).collect()
    }
    fn listed(&self, v: &[String]) -> String {
        v.iter().map(|r| self.nm.get(r)).collect::<Vec<_>>().join(", ")
    }
    fn me(&self) -> String {
        self.raw.clone()
    }

    fn intersection(&self, r: &str) -> Option<Found> {
        let (l, m) = r.split_once(" and ")?;
        let (a, b) = (self.run(l, 2)?, self.run(m, 2)?);
        let x = line_meet(self.xy(&a[0])?, self.xy(&a[1])?, self.xy(&b[0])?, self.xy(&b[1])?)?;
        let preds = vec![("coll", vec![a[0].clone(), a[1].clone(), self.me()]), ("coll", vec![b[0].clone(), b[1].clone(), self.me()])];
        Some((x, preds, "intersect", vec![self.shown(&a), self.shown(&b)]))
    }

    fn second_meet(&self, r: &str) -> Option<Found> {
        let (l, circ) = r.split_once(" with the circle")?;
        let vr = self.run(l, 2)?;
        let through = circ.split_once(" through ").and_then(|(_, x)| self.run(x, 3));
        let centre = circ.trim().strip_prefix('(').and_then(|c| c.split(')').next()).and_then(|c| self.run(c, 1));
        let fig = self.fig;
        let c = match (&through, &centre) {
            (Some(t), _) => circumcircle(self.xy(&t[0])?, self.xy(&t[1])?, self.xy(&t[2])?).map(|(x, y, _)| (x, y))?,
            (None, Some(o)) => self.xy(&o[0])?,
            (None, None) => fig
                .preds
                .iter()
                .filter(|p| p.name == "cyclic" && p.points.len() >= 3)
                .map(|p| p.points.iter().map(|&i| fig.point_name(i).to_string()).collect::<Vec<_>>())
                .find(|on| on.contains(&vr[1]))
                .and_then(|on| circumcircle(self.xy(&on[0])?, self.xy(&on[1])?, self.xy(&on[2])?))
                .map(|(x, y, _)| (x, y))?,
        };
        let (v, r0) = (self.xy(&vr[0])?, self.xy(&vr[1])?);
        let d = (v.0 - r0.0, v.1 - r0.1);
        let dd = d.0 * d.0 + d.1 * d.1;
        if dd < 1e-18 {
            return None;
        }
        let tt = -2.0 * (d.0 * (r0.0 - c.0) + d.1 * (r0.1 - c.1)) / dd;
        let x = (r0.0 + tt * d.0, r0.1 + tt * d.1);
        let mut preds = vec![("coll", vec![vr[0].clone(), vr[1].clone(), self.me()])];
        let circle_arg = match (&through, &centre) {
            (Some(t), _) => {
                preds.push(("cyclic", vec![t[0].clone(), t[1].clone(), t[2].clone(), self.me()]));
                format!("circumcircle({})", self.listed(t))
            }
            (None, Some(o)) => {
                preds.push(("cong", vec![o[0].clone(), vr[1].clone(), o[0].clone(), self.me()]));
                format!("circle({}, {})", self.nm.get(&o[0]), self.nm.get(&vr[1]))
            }
            _ => String::from("circle"),
        };
        Some((x, preds, "intersect", vec![self.shown(&vr), circle_arg]))
    }

    fn foot(&self, r: &str) -> Option<Found> {
        let (p, l) = r.split_once(" to ")?;
        let (p, ab) = (self.run(p, 1)?, self.run(l, 2)?);
        let (q, a, b) = (self.xy(&p[0])?, self.xy(&ab[0])?, self.xy(&ab[1])?);
        let d = (b.0 - a.0, b.1 - a.1);
        let dd = d.0 * d.0 + d.1 * d.1;
        if dd < 1e-18 {
            return None;
        }
        let tt = ((q.0 - a.0) * d.0 + (q.1 - a.1) * d.1) / dd;
        let preds = vec![("coll", vec![ab[0].clone(), ab[1].clone(), self.me()]), ("perp", vec![p[0].clone(), self.me(), ab[0].clone(), ab[1].clone()])];
        Some(((a.0 + tt * d.0, a.1 + tt * d.1), preds, "foot", vec![self.nm.get(&p[0]), self.shown(&ab)]))
    }

    fn midpoint(&self, r: &str) -> Option<Found> {
        let ab = self.run(r, 2)?;
        let (a, b) = (self.xy(&ab[0])?, self.xy(&ab[1])?);
        let preds = vec![("coll", vec![ab[0].clone(), ab[1].clone(), self.me()]), ("cong", vec![self.me(), ab[0].clone(), self.me(), ab[1].clone()])];
        Some((((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), preds, "midpoint", vec![self.listed(&ab)]))
    }

    fn reflection(&self, r: &str) -> Option<Found> {
        let (p, o) = r.split_once(" in ")?;
        let (p, o) = (self.run(p, 1)?, self.run(o, 1)?);
        let (a, c) = (self.xy(&p[0])?, self.xy(&o[0])?);
        let preds = vec![("coll", vec![p[0].clone(), o[0].clone(), self.me()]), ("cong", vec![o[0].clone(), p[0].clone(), o[0].clone(), self.me()])];
        Some(((2.0 * c.0 - a.0, 2.0 * c.1 - a.1), preds, "reflect", vec![self.nm.get(&p[0]), self.nm.get(&o[0])]))
    }

    fn circumcentre(&self, r: &str) -> Option<Found> {
        let abc = self.run(r, 3)?;
        let (x, y, _) = circumcircle(self.xy(&abc[0])?, self.xy(&abc[1])?, self.xy(&abc[2])?)?;
        let preds = vec![
            ("cong", vec![self.me(), abc[0].clone(), self.me(), abc[1].clone()]),
            ("cong", vec![self.me(), abc[1].clone(), self.me(), abc[2].clone()]),
        ];
        Some(((x, y), preds, "circumcenter", vec![self.listed(&abc)]))
    }

    fn antipode(&self, r: &str) -> Option<Found> {
        let p = self.run(r.split_whitespace().next()?, 1)?;
        let (_, o) = r.rsplit_once(" through the centre ").or_else(|| r.rsplit_once(" through the center "))?;
        let o = self.run(o.trim_end_matches(')'), 1)?;
        let (a, c) = (self.xy(&p[0])?, self.xy(&o[0])?);
        let preds = vec![("coll", vec![p[0].clone(), o[0].clone(), self.me()]), ("cong", vec![o[0].clone(), p[0].clone(), o[0].clone(), self.me()])];
        Some(((2.0 * c.0 - a.0, 2.0 * c.1 - a.1), preds, "antipode", vec![self.nm.get(&p[0])]))
    }
}

fn proof_points(text: &str, fig: &Problem, names: &Names) -> Vec<Introduced> {
    let mut out: Vec<Introduced> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        let Some(body) = t.get(digits..).and_then(|b| b.strip_prefix(". Let ")) else { continue };
        let Some((raw, what)) = body.split_once(" be the ") else { continue };
        let raw = raw.trim().to_string();
        if raw.is_empty() || !raw.chars().all(is_name_char) || fig.points.iter().any(|p| p.name == raw) || out.iter().any(|i| i.raw == raw) {
            continue;
        }
        let known: Vec<String> = out.iter().map(|i| i.raw.clone()).chain(std::iter::once(raw.clone())).collect();
        let cx = Intro { fig, nm: names.with_points(&known), done: &out, raw: raw.clone() };
        let what = what.trim().trim_end_matches('.');
        let found = if let Some(r) = what.strip_prefix("intersection of ") {
            cx.intersection(r)
        } else if let Some(r) = what.strip_prefix("second meet of line ") {
            cx.second_meet(r)
        } else if let Some(r) = what.strip_prefix("foot of the perpendicular from ") {
            cx.foot(r)
        } else if let Some(r) = what.strip_prefix("midpoint of ") {
            cx.midpoint(r)
        } else if let Some(r) = what.strip_prefix("reflection of ") {
            cx.reflection(r)
        } else if let Some(r) = what.strip_prefix("circumcentre of ").or_else(|| what.strip_prefix("circumcenter of ")) {
            cx.circumcentre(r)
        } else if let Some(r) = what.strip_prefix("point diametrically opposite ") {
            cx.antipode(r)
        } else {
            None
        };
        let text = crate::i18n::prose_en(&cx.nm.rename_text(&circle_through(what, &cx.nm), true));
        if let Some((at, preds, kind, args)) = found.filter(|f| finite_pt(f.0)) {
            out.push(Introduced { raw, at, preds, kind, args, text });
        }
    }
    out
}

fn with_introduced(fig: &Problem, intro: &[Introduced]) -> Problem {
    let mut fig = fig.clone();
    for i in intro {
        fig.points.push(ddar::predicate::Point { name: i.raw.clone(), value: ddar::numerics::Vec2 { x: i.at.0, y: i.at.1 } });
    }
    for i in intro {
        for (name, pts) in &i.preds {
            let ids: Option<Vec<u32>> = pts.iter().map(|r| fig.points.iter().position(|p| &p.name == r).map(|k| k as u32)).collect();
            if let Some(points) = ids {
                fig.preds.push(Predicate { name: name.to_string(), points, constants: Vec::new() });
            }
        }
    }
    fig
}

// ------------------------------------------------------------------- view --

#[derive(Serialize, Clone, Debug)]
pub struct PointView {
    pub name: String,
    pub aux: bool,
}

/// Everything the web client shows for a solution, beyond the engine's own
/// fields.
#[derive(Serialize, Clone, Debug)]
pub struct View {
    pub points: Vec<PointView>,
    pub given: Vec<Fact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal: Option<Fact>,
    pub proof: ProofView,
    pub aux: Vec<AuxView>,
    pub note: Note,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<Counter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    /// A title from the program's own leading comment, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_title: Option<String>,
    /// The interactive figure (classes + `data-p` point lists; light colours
    /// as attributes, so it also renders standalone and in exports).
    pub svg: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub as_drawn: bool,
}

pub fn build(sol: &Solution) -> View {
    let original = Problem::parse(&sol.low_level).ok();
    let (fig_problem, aux_from) = match (&sol.figure, &original) {
        (Some(f), _) => (Some(f.problem.clone()), f.aux_from),
        (None, Some(p)) => (Some(p.clone()), None),
        (None, None) => (None, None),
    };
    let empty = Problem { points: vec![], preds: vec![], goal: None };
    let fig = fig_problem.unwrap_or_else(|| empty.clone());
    let introduced = match (&sol.proof, sol.method) {
        (Some(p), Method::Euclidean) if sol.proved => proof_points(p, &fig, &Names::build(&fig)),
        _ => Vec::new(),
    };
    let aux_from = if introduced.is_empty() { aux_from } else { aux_from.or(Some(fig.points.len())) };
    let fig = if introduced.is_empty() { fig } else { with_introduced(&fig, &introduced) };
    let names = Names::build(&fig);
    let orig = original.unwrap_or_else(|| fig.clone());

    let points: Vec<PointView> = fig
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| PointView { name: names.get(&p.name), aux: aux_from.is_some_and(|a| i >= a) })
        .collect();

    // Given: the original hypotheses (never the search's aux facts), minus the
    // placeholder `XY = XY` the compiler emits for fixed lengths — those are
    // restated from the program as `XY = 6`.
    let mut given: Vec<Fact> = Vec::new();
    for pred in &orig.preds {
        if is_tautology(pred) {
            continue;
        }
        let f = fact_of_pred(&orig, &names, pred);
        if !given.contains(&f) {
            given.push(f);
        }
    }
    let assumed: Vec<Fact> = source_assumes(&sol.input)
        .iter()
        .map(|a| Fact::new("formula", vec![pretty_metric(a, &names)], names.disp_all(&names.mentioned(a, false))))
        .collect();
    let engine_only: Vec<Fact> = orig
        .preds
        .iter()
        .filter(|p| p.name == "angeq" || fact_of_pred(&orig, &names, p).kind == "raw")
        .map(|p| fact_of_pred(&orig, &names, p))
        .collect();
    let mut as_written: Vec<(Fact, Fact)> = Vec::new();
    if !assumed.is_empty() && !engine_only.is_empty() {
        given.retain(|f| !engine_only.contains(f));
        for (k, f) in engine_only.iter().enumerate() {
            let user = if engine_only.len() == assumed.len() { assumed.get(k) } else { (assumed.len() == 1).then(|| &assumed[0]) };
            if let Some(u) = user {
                as_written.push((f.clone(), u.clone()));
            }
        }
        for f in &assumed {
            if !given.contains(f) {
                given.push(f.clone());
            }
        }
    }
    for (a, b, v) in source_lengths(&sol.input) {
        let seg = format!("{}{}", names.get(&a), names.get(&b));
        let f = Fact::new("length", vec![seg, v], vec![names.get(&a), names.get(&b)]);
        if !given.contains(&f) {
            given.push(f);
        }
    }
    for c in source_point_metrics(&sol.input) {
        let f = Fact::new("formula", vec![pretty_metric(&c, &names)], names.disp_all(&names.mentioned(&c, false)));
        if !given.contains(&f) {
            given.push(f);
        }
    }

    let goal = match orig.goal.as_ref() {
        Some(g) if g.name != "angeq" && !matches!(fact_of_pred(&orig, &names, g).kind, "raw") => Some(fact_of_pred(&orig, &names, g)),
        _ => sol
            .goal
            .clone()
            .filter(|g| g.contains('(') || g.contains('='))
            .or_else(|| source_goal(&sol.input))
            .map(|g| {
                let g = if g.contains("dist(") || g.contains("angle(") || g.contains("area(") {
                    g
                } else {
                    source_goal(&sol.input).unwrap_or(g)
                };
                Fact::new(
                    "formula",
                    vec![pretty_metric(&g, &names)],
                    names.disp_all(&names.mentioned(&g, false)),
                )
            }),
    };

    let proof = match (&sol.proof, sol.method) {
        (Some(p), Method::Euclidean) => parse_euclid_proof(p, &fig, &names),
        (Some(p), _) => {
            let aux_names: HashSet<String> = points.iter().filter(|p| p.aux).map(|p| p.name.clone()).collect();
            parse_ddar_proof(p, &fig, &names, &aux_names)
        }
        (None, _) => ProofView { steps: vec![], conclusion: None, style: "none" },
    };
    let mut proof = proof;
    for st in proof.steps.iter_mut() {
        if let Some((_, user)) = as_written.iter().find(|(e, _)| e.kind == st.fact.kind && e.args == st.fact.args) {
            st.fact = user.clone();
        }
    }
    if let (Some(c), Some(g)) = (proof.conclusion.as_mut(), goal.as_ref()) {
        if c.kind == "raw" || (c.kind == "formula" && g.kind == "formula" && c.points.iter().all(|p| g.points.contains(p))) {
            *c = g.clone();
        }
    }
    let mut aux: Vec<AuxView> = sol.aux_constructions.iter().map(|a| aux_view(a, &names)).collect();
    for i in &introduced {
        aux.push(AuxView { name: names.get(&i.raw), kind: i.kind.to_string(), args: i.args.clone(), text: i.text.clone() });
    }

    let extras = figure::Extras {
        polygons: source_polygons(&sol.input),
        metric_goal: if orig.goal.is_none() { sol.goal.clone() } else { None },
    };
    let as_drawn = sol.proved
        && proof
            .steps
            .iter()
            .any(|st| st.fact.kind == "prose" && st.fact.args.iter().any(|a| a.contains("(as drawn)")));
    let refuted = sol.status == Status::Refuted;
    let goal_in_figure = if refuted { crate::spread::Goal::Fails } else { crate::spread::Goal::Holds };
    let redrawn = if !as_drawn && introduced.is_empty() && (!refuted || aux_from.is_none()) {
        crate::spread::respread(&fig, aux_from, &sol.input, goal_in_figure)
    } else {
        None
    };
    let drawn = redrawn.as_ref().unwrap_or(&fig);
    let svg = if drawn.points.is_empty() {
        String::new()
    } else {
        figure::render(drawn, aux_from, &names, &extras)
    };
    View {
        as_drawn,
        points,
        given,
        goal,
        proof,
        aux,
        note: classify_note(sol),
        counterexample: counterexample(redrawn.as_ref().unwrap_or(&orig), &names, sol),
        evidence: numeric_support(sol),
        source_title: source_title(&sol.input),
        svg,
    }
}

/// Where a compile error sits in the program, and which sentence explains it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Diagnosis {
    pub key: &'static str,
    /// 1-based line; 0 when the error is not tied to a line.
    pub line: usize,
    /// 1-based column, in characters.
    pub col: usize,
    pub len: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub got: Option<usize>,
}

fn backticked(msg: &str) -> Option<String> {
    let a = msg.find('`')?;
    let b = msg[a + 1..].find('`')? + a + 1;
    Some(msg[a + 1..b].to_string())
}

fn quoted(msg: &str) -> Option<String> {
    let a = msg.find('\'')?;
    let b = msg[a + 1..].find('\'')? + a + 1;
    Some(msg[a + 1..b].to_string())
}

fn found_token(msg: &str) -> Option<Option<String>> {
    let at = msg.find("found ").map(|i| i + 6).or_else(|| msg.find("unexpected token ").map(|i| i + 17))?;
    let rest = msg[at..].trim_end();
    if rest.starts_with("None") {
        return Some(None);
    }
    let inner = rest.strip_prefix("Some(")?;
    let inner = inner.strip_suffix(')').unwrap_or(inner);
    Some(token_text(inner))
}

/// The source text of a lexer token written in its `Debug` form (`Op('*')`,
/// `Ident("x")`, `Num(2.0)`, `LParen`, …); `None` for an end-of-line token.
fn token_text(debug: &str) -> Option<String> {
    let wrapped = |prefix: &str| debug.strip_prefix(prefix).and_then(|x| x.strip_suffix(')'));
    if let Some(q) = wrapped("Ident(") {
        return Some(q.trim_matches('"').to_string());
    }
    if let Some(n) = wrapped("Num(") {
        return Some(n.strip_suffix(".0").unwrap_or(n).to_string());
    }
    if let Some(c) = wrapped("Op(") {
        return Some(c.trim_matches('\'').to_string());
    }
    Some(
        match debug {
            "Comma" => ",",
            "LParen" => "(",
            "RParen" => ")",
            "Eq" => "=",
            "Colon" => ":",
            "Star" => "*",
            "Plus" => "+",
            "Minus" => "-",
            "Slash" => "/",
            "Caret" => "^",
            "Question" => "?",
            "Sep" => return None,
            other => other,
        }
        .to_string(),
    )
}

fn word_positions(line: &str, word: &str) -> Vec<usize> {
    let chars: Vec<char> = line.chars().collect();
    let w: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    if w.is_empty() {
        return out;
    }
    let ident = w.iter().all(|c| is_name_char(*c));
    let mut i = 0;
    while i + w.len() <= chars.len() {
        if chars[i..i + w.len()] == w[..] {
            let before_ok = !ident || i == 0 || !is_name_char(chars[i - 1]);
            let after_ok = !ident || i + w.len() == chars.len() || !is_name_char(chars[i + w.len()]);
            if before_ok && after_ok {
                out.push(i);
            }
        }
        i += 1;
    }
    out
}

/// The metric prover's `angle got 2 arguments`: (function, expected, got).
fn metric_arity(msg: &str) -> Option<(String, usize, usize)> {
    let (f, rest) = msg.split_once(" got ")?;
    let got: usize = rest.strip_suffix(" arguments").or_else(|| rest.strip_suffix(" argument"))?.trim().parse().ok()?;
    let expected = match f {
        "dist" => 2,
        "angle" | "area" => 3,
        "sin" | "cos" | "tan" | "sqrt" => 1,
        _ => return None,
    };
    Some((f.to_string(), expected, got))
}

fn diagnosis_key(msg: &str) -> &'static str {
    let starts = |p: &str| msg.starts_with(p);
    if starts("expected `,` or `)`") {
        "expect_comma_paren"
    } else if starts("expected point name") || starts("expected a point name") {
        "expect_point"
    } else if starts("expected `,`, name, or `=`") {
        "expect_def"
    } else if starts("expected expression") || starts("expected a metric expression") {
        "expect_expr"
    } else if starts("expected a numeric exponent") || starts("expected numeric exponent") {
        "expect_exponent"
    } else if starts("unexpected token") || starts("expected ") {
        "unexpected_token"
    } else if starts("unexpected character") {
        "bad_char"
    } else if starts("unknown relation") {
        "unknown_relation"
    } else if starts("trailing tokens") {
        "trailing"
    } else if starts("unknown name") || starts("unknown point") {
        "unknown_name"
    } else if starts("unknown construction") {
        "unknown_construction"
    } else if (msg.contains(" expects ") && msg.contains(" argument")) || metric_arity(msg).is_some() {
        "arity"
    } else if msg.contains("is already defined") {
        "redefined"
    } else if msg.contains("is not a metric value here") && backticked(msg).is_some() {
        "unknown_relation"
    } else if starts("bad number") {
        "bad_number"
    } else if starts("could not build") || msg == "no attempt" {
        "degenerate"
    } else if starts("empty program") {
        "empty"
    } else if msg.contains("needs a `prove") || msg.contains("has no goal") {
        "no_goal"
    } else if msg.contains("`point:` defines exactly one point") {
        "point_one"
    } else if msg.contains("multiple goals") {
        "multiple_goals"
    } else {
        "other"
    }
}

const NON_POINT: &[&str] = &["circle", "circumcircle", "line", "perp_line", "para_line", "perp_bisector", "bisector", "ninepoints", "tangent"];

fn non_point_definition(input: &str, msg: &str) -> Option<Diagnosis> {
    if !msg.starts_with("expected a point") {
        return None;
    }
    input.lines().enumerate().find_map(|(i, l)| {
        let code = l.split('#').next().unwrap_or("");
        let (lhs, rhs) = code.split_once('=')?;
        if lhs.contains('(') || lhs.trim_start().starts_with("prove") {
            return None;
        }
        let head: String = rhs.trim_start().chars().take_while(|c| is_name_char(*c)).collect();
        if !NON_POINT.contains(&head.as_str()) || !rhs.trim_start()[head.len()..].trim_start().starts_with('(') {
            return None;
        }
        let col = lhs.chars().count() + 1 + (rhs.chars().count() - rhs.trim_start().chars().count());
        Some(Diagnosis { key: "not_a_point", line: i + 1, col: col + 1, len: head.chars().count(), token: Some(head), expected: None, got: None })
    })
}

fn trig_without_angle(input: &str, key: &str) -> Option<Diagnosis> {
    if !matches!(key, "unexpected_token" | "expect_comma_paren" | "expect_expr" | "trailing" | "other") {
        return None;
    }
    input.lines().enumerate().find_map(|(i, l)| {
        let code: Vec<char> = l.split('#').next().unwrap_or("").chars().collect();
        for f in ["sin", "cos", "tan"] {
            let fc: Vec<char> = f.chars().collect();
            for at in 0..code.len().saturating_sub(3) {
                if !code[at..].starts_with(&fc) || (at > 0 && is_name_char(code[at - 1])) {
                    continue;
                }
                let mut j = at + 3;
                while j < code.len() && code[j] == ' ' {
                    j += 1;
                }
                if code.get(j) != Some(&'(') {
                    continue;
                }
                let mut k = j + 1;
                while k < code.len() && code[k] == ' ' {
                    k += 1;
                }
                let arg: String = code[k..].iter().take_while(|c| is_name_char(**c)).collect();
                let after = code[k + arg.chars().count()..].iter().find(|c| **c != ' ');
                if !arg.is_empty() && arg != "angle" && after != Some(&'(') {
                    return Some(Diagnosis { key: "trig_angle", line: i + 1, col: k + 1, len: arg.chars().count(), token: Some(f.to_string()), expected: None, got: None });
                }
            }
        }
        None
    })
}

/// The engine's message with its lexer tokens written as source text
/// (`found Some(RParen)` becomes `found “)”`).
pub fn readable_engine_message(msg: &str) -> String {
    let mut out = String::new();
    let mut rest = msg;
    while let Some(at) = rest.find("Some(") {
        out.push_str(&rest[..at]);
        let inner_start = at + 5;
        let mut depth = 1;
        let mut end = None;
        let mut in_str = false;
        for (k, c) in rest[inner_start..].char_indices() {
            match c {
                '"' => in_str = !in_str,
                '(' if !in_str => depth += 1,
                ')' if !in_str => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(inner_start + k);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            out.push_str(&rest[at..]);
            rest = "";
            break;
        };
        out.push_str(&match token_text(&rest[inner_start..end]) {
            Some(t) => format!("\u{201c}{t}\u{201d}"),
            None => "a line break".to_string(),
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out.split(' ')
        .map(|w| {
            let core = w.trim_end_matches([',', '.', ';']);
            let tail = &w[core.len()..];
            match core {
                "None" => format!("the end of the line{tail}"),
                "Sep" => format!("a line break{tail}"),
                "LParen" | "RParen" | "Comma" | "Eq" | "Colon" | "Star" | "Plus" | "Minus" | "Slash" | "Caret" | "Question" => {
                    format!("\u{201c}{}\u{201d}{tail}", token_text(core).unwrap_or_default())
                }
                _ => w.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Locate a compile error in `input` and pick the sentence that explains it.
pub fn diagnose(input: &str, msg: &str) -> Diagnosis {
    let metric = msg.starts_with("metric prover: ");
    let msg = msg
        .trim_start_matches("compile error: ")
        .trim_start_matches("parse error: ")
        .trim_start_matches("metric prover: ")
        .trim();
    let mut key = diagnosis_key(msg);
    let named = backticked(msg).or_else(|| quoted(msg));
    if key == "unknown_name" && named.as_deref().is_some_and(|t| t.starts_with(|c: char| c.is_ascii_lowercase())) {
        let t = named.clone().unwrap_or_default();
        let whole_rhs = input.lines().any(|l| {
            let code = l.split('#').next().unwrap_or("");
            code.split_once('=').is_some_and(|(lhs, rhs)| !lhs.contains('(') && rhs.trim() == t)
        });
        key = if whole_rhs { "unknown_shape" } else { "unknown_construction" };
    }
    if key == "arity" && msg.contains("at least") {
        key = "arity_min";
    }
    if let Some(d) = non_point_definition(input, msg).or_else(|| trig_without_angle(input, key)) {
        return d;
    }
    let mut d = Diagnosis { key, line: 0, col: 0, len: 0, token: None, expected: None, got: None };
    let arity = metric_arity(msg);
    if let Some((_, e, g)) = &arity {
        d.expected = Some(*e);
        d.got = Some(*g);
    } else if key == "arity" || key == "arity_min" {
        let nums: Vec<usize> = msg
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|x| x.parse().ok())
            .collect();
        d.expected = nums.first().copied();
        d.got = nums.get(1).copied();
    }
    let by_name = matches!(
        key,
        "bad_char" | "unknown_relation" | "unknown_name" | "unknown_construction" | "unknown_shape" | "arity" | "arity_min" | "redefined" | "bad_number"
    );
    d.token = if let Some((f, _, _)) = &arity {
        Some(f.clone())
    } else if by_name {
        named
    } else {
        found_token(msg).flatten()
    };
    let lines: Vec<&str> = input.lines().collect();
    let code_of = |l: &str| l.split('#').next().unwrap_or("").to_string();
    let goal_line = || lines.iter().rposition(|l| code_of(l).trim_start().starts_with("prove"));
    let line_idx: Option<usize> = match key {
        "degenerate" | "empty" | "no_goal" | "other" => None,
        "multiple_goals" => {
            let goals: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| code_of(l).split(';').any(|s| s.trim_start().starts_with("prove")))
                .map(|(i, _)| i)
                .collect();
            d.token = Some("prove".to_string());
            goals.get(1).or(goals.first()).copied()
        }
        "trailing" => {
            let i = goal_line();
            if let Some(i) = i {
                let code = code_of(lines[i]);
                let (start, rhs) = match code.find('=') {
                    Some(eq) if msg.contains("right") => (eq + 1, &code[eq + 1..]),
                    _ => (0, code.split('=').next().unwrap_or("")),
                };
                let mut depth = 0i32;
                let mut at = None;
                for (k, c) in rhs.char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' if depth == 0 => {
                            at = Some(start + k);
                            break;
                        }
                        ')' => depth -= 1,
                        _ => {}
                    }
                }
                let at = at.unwrap_or_else(|| start + rhs.trim_end().rfind(' ').map_or(0, |k| k + 1));
                d.token = code[at..].split_whitespace().next().map(|w| w.chars().take(if w.starts_with(')') { 1 } else { w.len() }).collect());
            }
            i
        }
        _ if metric && by_name => d
            .token
            .as_ref()
            .and_then(|t| {
                goal_line()
                    .filter(|&i| !word_positions(&code_of(lines[i]), t).is_empty())
                    .or_else(|| lines.iter().position(|l| !word_positions(&code_of(l), t).is_empty()))
            })
            .or_else(goal_line),
        _ if metric => goal_line(),
        "redefined" => d.token.as_ref().and_then(|t| {
            let defs: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| {
                    code_of(l)
                        .split_once('=')
                        .is_some_and(|(lhs, _)| lhs.split_whitespace().any(|w| w == t))
                })
                .map(|(i, _)| i)
                .collect();
            defs.get(1).or(defs.first()).copied()
        }),
        _ if by_name => d
            .token
            .as_ref()
            .and_then(|t| lines.iter().position(|l| !word_positions(&code_of(l), t).is_empty())),
        _ => {
            let started = std::time::Instant::now();
            let mut hit = None;
            for i in 0..lines.len().min(120) {
                if started.elapsed() > std::time::Duration::from_millis(1500) {
                    break;
                }
                if code_of(lines[i]).trim().is_empty() {
                    continue;
                }
                let prefix = lines[..=i].join("\n");
                let failed = std::panic::catch_unwind(|| ddar::geo::compile(&prefix).err().map(|e| e.message().to_string()))
                    .ok()
                    .flatten();
                if failed.as_deref() == Some(msg) {
                    hit = Some(i);
                    break;
                }
            }
            hit
        }
    };
    if let Some(i) = line_idx {
        let code = code_of(lines[i]);
        let end = code.trim_end().chars().count() + 1;
        d.line = i + 1;
        let (col, len) = match &d.token {
            Some(t) => {
                let pos = word_positions(&code, t);
                let chars: Vec<char> = code.chars().collect();
                let after_operand = |p: usize| {
                    let mut k = p;
                    while k > 0 && chars[k - 1] == ' ' {
                        k -= 1;
                    }
                    k > 0 && k < p && (is_name_char(chars[k - 1]) || chars[k - 1] == ')')
                };
                let pick = if key == "trailing" || (metric && !by_name) {
                    pos.last().copied()
                } else if matches!(key, "expect_comma_paren" | "unexpected_token") {
                    pos.iter().copied().find(|&p| after_operand(p)).or_else(|| pos.last().copied())
                } else {
                    pos.first().copied()
                };
                pick.map_or((end, 1), |p| (p + 1, t.chars().count().max(1)))
            }
            None => (end, 1),
        };
        d.col = col;
        d.len = len;
    }
    d
}

/// The JSON a solve answers with: the engine's own fields unchanged (with the
/// figure replaced by the interactive one), the presentation `view`, and the
/// title (the caller's, else the program's leading comment).
pub fn solution_json(sol: &Solution, title: Option<&str>) -> serde_json::Value {
    let view = build(sol);
    let mut v = serde_json::to_value(sol).unwrap_or_default();
    v["svg"] = serde_json::Value::String(view.svg.clone());
    v["view"] = serde_json::to_value(&view).unwrap_or_default();
    if let Some(o) = v["view"].as_object_mut() {
        o.remove("svg");
    }
    let title = title.map(str::to_string).or(view.source_title.clone());
    v["title"] = title.map_or(serde_json::Value::Null, serde_json::Value::String);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{solve, SolveOptions};

    #[test]
    fn compile_errors_point_at_the_offending_token() {
        let cases = [
            ("A B C = triangle\nH = orthocenter(A B C)\nprove perp(A, H, B, C)", "expect_comma_paren", 2, 19),
            ("A B C = triangle\nH = orthocentre2(A, B, C)\nprove perp(A, H, B, C)", "unknown_construction", 2, 5),
            ("A B C = triangle\nH = orthocenter(A, B, C\nprove perp(A, H, B, C)", "expect_comma_paren", 2, 24),
        ];
        for (src, key, line, col) in cases {
            let msg = solve(src, &SolveOptions::default()).err().expect("must not compile");
            let d = diagnose(src, &msg);
            assert_eq!((d.key, d.line, d.col), (key, line, col), "{msg} → {d:?}");
        }
    }

    #[test]
    fn common_mistakes_get_their_own_message() {
        let cases = [
            ("A B C = triangle\nH = orthocenter(A, B, C)", "no_goal", 0),
            ("A B C = triangle\nM = midpoint(A, A)\nprove coll(A, B, M)", "degenerate", 0),
            ("A B C = triangle\nprove collinear(A, B, C)", "unknown_relation", 2),
        ];
        for (src, key, line) in cases {
            let msg = solve(src, &SolveOptions::default()).err().expect("must not compile");
            let d = diagnose(src, &msg);
            assert_eq!((d.key, d.line), (key, line), "{msg} → {d:?}");
        }
        let src = "B = free\nC = point: dist(B,C)=6.2.3\nprove dist(B,C)^2 = 36";
        let d = diagnose(src, "metric prover: bad number `6.2.3`");
        assert_eq!((d.key, d.line, d.col, d.token.as_deref()), ("bad_number", 2, 22, Some("6.2.3")));
    }

    #[test]
    fn multi_letter_names_keep_their_case() {
        assert_eq!(disp("Ma"), "Ma");
        assert_eq!(disp("A1"), "A\u{2081}");
        assert_eq!(disp("a1"), "A\u{2081}");
        assert_eq!(disp("ab"), "AB");
    }

    fn view(src: &str) -> (Solution, View) {
        let sol = solve(src, &SolveOptions::default()).expect("solve");
        let v = build(&sol);
        (sol, v)
    }

    fn dot(svg: &str, n: &str) -> (f64, f64) {
        let l = svg.lines().find(|l| l.contains("f-dot") && l.contains(&format!("data-p=\"{n}\""))).expect(n);
        let num = |k: &str| -> f64 {
            let i = l.find(&format!("{k}=\"")).unwrap() + k.len() + 2;
            l[i..].split('"').next().unwrap().parse().unwrap()
        };
        (num("cx"), num("cy"))
    }

    fn drawn_triangle(svg: &str) -> [f64; 3] {
        let (a, b, c) = (dot(svg, "A"), dot(svg, "B"), dot(svg, "C"));
        let ang = |o: (f64, f64), p: (f64, f64), q: (f64, f64)| {
            let (ux, uy, vx, vy) = (p.0 - o.0, p.1 - o.1, q.0 - o.0, q.1 - o.1);
            (ux * vy - uy * vx).abs().atan2(ux * vx + uy * vy).to_degrees()
        };
        [ang(a, b, c), ang(b, c, a), ang(c, a, b)]
    }

    #[test]
    fn every_triangle_example_is_drawn_acute_whatever_the_verdict() {
        for (src, status) in [
            ("A B C = triangle\nO = circumcenter(A, B, C)\nG = centroid(A, B, C)\nH = orthocenter(A, B, C)\nprove coll(O, G, H)", Status::Proved),
            ("A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)", Status::Refuted),
            ("A B C = triangle\nM = midpoint(B, C)\nprove area(A,B,M) = area(A,M,C)", Status::HoldsNumerically),
        ] {
            let (sol, v) = view(src);
            assert_eq!(sol.status, status, "{src}");
            let a = drawn_triangle(&v.svg);
            assert!(a.iter().all(|x| (30.0..=81.0).contains(x)), "{src}: drawn angles {a:?}");
        }
    }

    #[test]
    fn a_redrawn_counterexample_is_measured_in_the_drawn_figure() {
        let (_, v) = view("A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)");
        let c = v.counterexample.expect("counterexample");
        let at = |n: &str| dot(&v.svg, n);
        let (a, b, cc, m) = (at("A"), at("B"), at("C"), at("M"));
        let d1 = (cc.1 - m.1).atan2(cc.0 - m.0);
        let d2 = (b.1 - a.1).atan2(b.0 - a.0);
        let mut deg = (d1 - d2).to_degrees().rem_euclid(180.0);
        if deg > 90.0 {
            deg = 180.0 - deg;
        }
        assert!((deg - c.lhs).abs() < 0.5, "drawn {deg}, reported {}", c.lhs);
    }

    #[test]
    fn anonymous_reflection_gets_a_primed_name_everywhere() {
        let (sol, v) = view("A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))");
        assert!(sol.proved);
        let names: Vec<_> = v.points.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"H\u{2032}"), "{names:?}");
        let blob = serde_json::to_string(&v).unwrap();
        assert!(!blob.contains("_5") && !blob.contains("_\u{2085}"), "internal name leaked: {blob}");
        let goal = v.goal.unwrap();
        assert_eq!(goal.kind, "cyclic");
        assert_eq!(goal.args, vec!["A", "B", "C", "H\u{2032}"]);
        assert!(v.svg.contains(">H\u{2032}<"), "label missing from figure");
        assert!(v.proof.steps.iter().all(|s| s.n > 0));
        assert!(v.proof.steps.iter().any(|s| !s.deps.is_empty()));
    }

    #[test]
    fn fixed_lengths_replace_tautological_givens() {
        let src = "B = free\nC = point: dist(B,C)=6\nD = point: coll(B,D,C), dist(B,D)=2\n\
                   A = point: dist(A,B)=5, dist(A,C)=4\nprove dist(A,D)^2 = 14";
        let (_, v) = view(src);
        let plain: Vec<String> = v.given.iter().map(Fact::plain).collect();
        assert!(plain.contains(&"BC = 6".to_string()), "{plain:?}");
        assert!(!plain.iter().any(|f| f == "BC = BC" || f == "BD = BD"), "{plain:?}");
        assert_eq!(v.goal.unwrap().args, vec!["AD\u{b2} = 14"]);
        assert_eq!(v.proof.style, "euclidean");
        assert!(v.proof.steps.iter().any(|s| s.rule_name.as_deref() == Some("Stewart's theorem")));
        assert!(v.as_drawn, "the Stewart step reads betweenness off the figure");
        let stewart = v.proof.steps.iter().find(|s| s.rule_name.as_deref() == Some("Stewart's theorem")).unwrap();
        assert!(stewart.fact.args[0].contains("2\u{2044}3\u{2009}\u{b7}\u{2009}AB\u{b2}"), "{:?}", stewart.fact.args);
    }

    #[test]
    fn named_theorem_steps_state_what_they_derive() {
        let (_, v) = view("A B C = triangle\nI = incenter(A, B, C)\nD = meet(line(A, I), line(B, C))\nprove eqratio(D, B, D, C, A, B, A, C)");
        let bis: Vec<&Step> = v.proof.steps.iter().filter(|s| s.rule_name.as_deref() == Some("angle bisector theorem")).collect();
        assert!(!bis.is_empty(), "{:?}", v.proof.steps);
        for s in bis {
            assert_eq!(s.fact.kind, "eqratio", "{s:?}");
            assert_eq!(s.fact.args.len(), 4, "{s:?}");
            assert_eq!(s.rule_name_ro.as_deref(), Some("teorema bisectoarei"));
        }
        assert_eq!(
            theorem_fact("intercept theorem (parallel rungs)", &["A", "B", "M", "N", "D", "C"].map(String::from), &[None; 6]).unwrap().plain(),
            "AM : BN = MD : NC = AD : BC"
        );
        assert_eq!(theorem_fact("radical axis", &["X", "U", "V"].map(String::from), &[None; 3]).unwrap().plain(), "X, U, V are collinear");
    }

    #[test]
    fn an_unknown_shape_is_not_called_an_undefined_point() {
        let d = diagnose("A B C D = cyclic_quad\nprove cyclic(A, B, C, D)", "compile error: unknown name `cyclic_quad`");
        assert_eq!((d.key, d.line, d.col), ("unknown_shape", 1, 11));
        let d = diagnose("A B C = triangle\nH = orthocentre2(A, B, C)\nprove perp(A, H, B, C)", "compile error: unknown name `orthocentre2`");
        assert_eq!(d.key, "unknown_construction");
        let d = diagnose("A B C = triangle\nprove perp(A, X, B, C)", "compile error: unknown name `X`");
        assert_eq!(d.key, "unknown_name");
    }

    #[test]
    fn refuted_goal_reports_the_failing_angle() {
        let (sol, v) = view("A B C = triangle\nM = midpoint(A, B)\nprove perp(C, M, A, B)");
        assert_eq!(sol.status, Status::Refuted);
        let c = v.counterexample.expect("counterexample");
        assert_eq!(c.kind, "angle");
        assert!((c.lhs - 90.0).abs() > 1.0);
        assert_eq!(v.note.key, "false");
    }

    #[test]
    fn metric_text_is_typeset() {
        let names = Names::build(&Problem { points: vec![], preds: vec![], goal: None });
        assert_eq!(pretty_metric("dist(A, B) ^ 2 + dist(A,C)^2 = dist(B,C)^2", &names), "AB\u{b2} + AC\u{b2} = BC\u{b2}");
        assert_eq!(pretty_metric("area(A,B,M) = area(A,M,C)", &names), "[ABM] = [AMC]");
        assert_eq!(pretty_metric("angle(A,B,C) = 60", &names), "\u{2220}ABC = 60\u{b0}");
        assert_eq!(pretty_metric("angle(A,B,C) + angle(B,C,A) + angle(C,A,B) = 180", &names), "\u{2220}ABC + \u{2220}BCA + \u{2220}CAB = 180\u{b0}");
        assert_eq!(pretty_metric("angle(A,B,C) = 2 * angle(A,C,B)", &names), "\u{2220}ABC = 2 \u{b7} \u{2220}ACB");
        assert_eq!(pretty_metric("sin(angle(A,B,C)) = 1", &names), "sin\u{2220}ABC = 1");
        assert_eq!(pretty_metric("cos(angle(A,B,C)) ^ 2 + sin(angle(A,B,C)) ^ 2 = 1", &names), "cos\u{b2}\u{2220}ABC + sin\u{b2}\u{2220}ABC = 1");
    }

    #[test]
    fn titles_come_from_the_leading_comment() {
        assert_eq!(source_title("# Stewart's theorem (additive engine): x\nB = free").as_deref(), Some("Stewart's theorem"));
        assert_eq!(source_title("A B C = triangle"), None);
        assert_eq!(source_title("# THEOREM (Euler line): O, G, H are collinear").as_deref(), Some("Euler line"));
    }

    #[test]
    fn equal_chords_are_stated_as_an_equality() {
        let sol = solve("A B C = triangle\nprove coll(A, B, A)", &SolveOptions::default()).expect("solve");
        let problem = Problem::parse(&sol.low_level).expect("low level");
        let names = Names::build(&problem);
        let st = ddar_line("056. equal arcs \u{21d4} equal chords: AB and CA [003 & 004]", &problem, &names, &HashSet::new())
            .expect("a step");
        assert_eq!(st.rule, "arcchord");
        assert_eq!(st.fact.kind, "cong");
        assert_eq!(st.fact.plain(), "AB = CA");
        assert_eq!(st.deps, vec![3, 4]);
    }

    /// Every Euclidean proof the provers produce for these programs, as shown.
    fn euclidean_views() -> Vec<(String, View)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../alphageometry-rs/examples/metric");
        let mut programs: Vec<(String, String)> = std::fs::read_dir(&dir)
            .expect("examples/metric")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "geo"))
            .map(|p| (p.display().to_string(), std::fs::read_to_string(&p).unwrap()))
            .collect();
        programs.sort();
        for (name, src) in [
            ("law of sines", "A B C = triangle\nO = circumcenter(A, B, C)\nprove dist(B,C) = 2*dist(O,A)*sin(angle(B,A,C))"),
            ("law of cosines", "A = free\nB = free\nC = point: perp(A,B,A,C)\nH = foot(A, line(B,C))\nprove dist(A,B)*cos(angle(A,B,C)) = dist(B,H)"),
            ("supplementary", "A B C = triangle\nD = reflect(C, B)\nprove cos(angle(A,B,C)) = -cos(angle(A,B,D))"),
            (
                "sine areas",
                "A B C = triangle\nM = midpoint(A, C)\nD = meet(line(B, M), circumcircle(A, B, C))\nprove dist(A,C)*(dist(A,B)*dist(B,C) + dist(C,D)*dist(D,A)) = dist(B,D)*(dist(A,B)*dist(A,D) + dist(B,C)*dist(C,D))",
            ),
            (
                "euler oi",
                "A B C = triangle\nO = circumcenter(A, B, C)\nI = incenter(A, B, C)\nT = foot(I, line(B, C))\nprove dist(O, I)^2 = dist(O, A)^2 - 2*dist(O, A)*dist(I, T)",
            ),
        ] {
            programs.push((name.to_string(), src.to_string()));
        }
        programs
            .into_iter()
            .filter_map(|(name, src)| {
                let sol = solve(&src, &SolveOptions::default()).ok()?;
                (sol.proved && sol.method == Method::Euclidean).then(|| (name, build(&sol)))
            })
            .collect()
    }

    #[test]
    fn euclidean_proofs_show_every_fact_and_reach_the_goal() {
        let views = euclidean_views();
        assert!(views.len() >= 12, "only {} Euclidean proofs", views.len());
        for name in ["law of sines", "law of cosines", "supplementary", "sine areas", "euler oi"] {
            assert!(views.iter().any(|(n, _)| n == name), "{name} was not proved Euclidean");
        }
        assert!(views.iter().any(|(_, v)| v.proof.steps.iter().any(|s| s.rule == "closure" && !s.subs.is_empty())));
        assert!(views.iter().any(|(_, v)| v.proof.steps.iter().any(|s| s.rule == "trig" && !s.subs.is_empty())));
        for (name, v) in &views {
            for st in &v.proof.steps {
                let text = &st.fact.args[0];
                assert!(st.subs.is_empty() == !(text.ends_with(':') || text.contains("below:")), "{name}: {text} with {} sub-items", st.subs.len());
                assert!(!text.contains("[from"), "{name}: {text}");
                assert!(st.rule != "algebra", "{name}: {text}");
                assert!(st.deps.iter().all(|&d| d >= 1 && d < st.n), "{name}: {} cites {:?}", st.n, st.deps);
            }
            let concl = v.proof.conclusion.as_ref().unwrap_or_else(|| panic!("{name}: no conclusion"));
            let goal = &v.goal.as_ref().expect("goal").args[0];
            assert!(concl.args[0].contains(goal.as_str()), "{name}: {:?} does not end at {goal}", concl.args[0]);
            assert!(!concl.args[0].contains('*') && !concl.args[0].contains(" - ") && !concl.args[0].contains("dist("), "{name}: {}", concl.args[0]);
        }
    }

    #[test]
    fn euclidean_proofs_read_in_romanian_without_english_left_over() {
        const ENGLISH: &[&str] = &[
            " the ", " of ", " with ", " from ", "below", "above", " through ", "Law ", " law ", "Let ", " is ", " are ",
            " by ", " gives ", "which", "inside", "outside", "Multiply", "relations", "triangle", "circle ", " between ",
            " so ", "times", "derived", "(given)", "hypothes", "Adding", "Combining", "Hence", " and ", " lies ",
        ];
        for (name, v) in euclidean_views() {
            let mut texts: Vec<String> = Vec::new();
            for st in &v.proof.steps {
                texts.extend(st.fact.ro.clone().unwrap_or_default());
                for sub in &st.subs {
                    texts.extend(sub.fact.ro.clone().unwrap_or_default());
                }
                if st.rule == "theorem" {
                    assert!(st.rule_name_ro.is_some(), "{name}: no Romanian name for {:?}", st.rule_name);
                }
            }
            texts.extend(v.proof.conclusion.as_ref().and_then(|c| c.ro.clone()).unwrap_or_default());
            for t in texts {
                let padded = format!(" {t} ");
                for w in ENGLISH {
                    assert!(!padded.contains(w), "{name}: English {w:?} left in {t:?}");
                }
            }
        }
    }

    #[test]
    fn metric_arity_and_operator_errors_name_the_source_text() {
        let src = "A B C = triangle\nprove dist(B,C) = 2**dist(A,B)";
        let d = diagnose(src, "metric prover: unexpected token Some(Op('*'))");
        assert_eq!((d.token.as_deref(), d.line, d.col), (Some("*"), 2, 21), "{d:?}");
        let src = "A B C = triangle\nprove dist(B,C) = sin(angle(A,B))";
        let d = diagnose(src, "metric prover: angle got 2 arguments");
        assert_eq!((d.key, d.token.as_deref(), d.expected, d.got, d.line), ("arity", Some("angle"), Some(3), Some(2), 2), "{d:?}");
        for (src, msg) in [
            ("A B C = triangle\nprove dist(B,C) = 2**dist(A,B)", "metric prover: unexpected token Some(Op('*'))"),
            ("A B C = triangle\nprove dist(B,C) = 2*/dist(A,B)", "metric prover: unexpected token Some(Op('/'))"),
            ("A B C = triangle\nprove dist(B,C) = 2*(dist(A,B)", "metric prover: unexpected token Some(LParen)"),
            ("A B C = triangle\nprove dist(B,C) = 2 x", "metric prover: unexpected token Some(Ident(\"x\"))"),
        ] {
            let d = diagnose(src, msg);
            for lang in [crate::i18n::Lang::En, crate::i18n::Lang::Ro] {
                let text = crate::i18n::compile_message(lang, &d);
                assert!(!["Op(", "Some(", "Ident(", "LParen"].iter().any(|w| text.contains(w)), "{text}");
            }
        }
    }

    #[test]
    fn an_angle_sum_hypothesis_is_shown_as_written() {
        let src = "A B = segment\nC = eq_triangle(A, B)\nA1 = on_line(A, midpoint(B, C))\nB1 = on_line(B, midpoint(C, A))\n\
                   C1 = on_line(C, midpoint(A, B))\nassume angle(B, A1, C) + angle(C, B1, A) + angle(A, C1, B) = 480\nprove coll(A, B, C)";
        let sol = solve(src, &SolveOptions::default()).expect("compiles");
        let v = build(&sol);
        assert!(v.given.iter().all(|f| f.kind != "raw"), "{:?}", v.given);
        assert!(
            v.given.iter().any(|f| f.args.iter().any(|a| a.contains("\u{2220}BA\u{2081}C") && a.contains("480"))),
            "{:?}",
            v.given
        );
    }

    fn step_texts(v: &View) -> Vec<String> {
        v.proof
            .steps
            .iter()
            .flat_map(|s| std::iter::once(&s.fact).chain(s.subs.iter().map(|u| &u.fact)))
            .map(|f| f.args.join(" "))
            .chain(v.proof.conclusion.iter().map(|c| c.args.join(" ")))
            .collect()
    }

    #[test]
    fn engine_predicates_never_reach_the_proof_text() {
        let raw = ["angeq ", "eqangle ", "eqratio ", "aconst ", "rconst "];
        for src in [
            "A B C = triangle\nassume angle(A,B,C) + angle(B,C,A) = 100\nprove angle(C,A,B) = 80",
            "A B C = triangle\nprove angle(A,B,C) + angle(B,C,A) + angle(C,A,B) = 180",
        ] {
            let (_, v) = view(src);
            for t in step_texts(&v) {
                assert!(!raw.iter().any(|r| t.starts_with(r)), "{t}");
            }
        }
        let (_, v) = view("A B C = triangle\nassume angle(A,B,C) + angle(B,C,A) = 100\nprove angle(C,A,B) = 80");
        assert_eq!(v.proof.steps[0].fact.args, vec!["\u{2220}ABC + \u{2220}BCA = 100\u{b0}".to_string()]);
        assert_eq!(v.given[0].args, vec!["\u{2220}ABC + \u{2220}BCA = 100\u{b0}".to_string()]);
    }

    #[test]
    fn a_false_angle_sum_stays_unproved() {
        let (sol, _) = view("A B C = triangle\nprove angle(A,B,C) + angle(B,C,A) + angle(C,A,B) = 170");
        assert!(!sol.proved, "a triangle's angles never sum to 170°");
        let (sol, _) = view("A B C = triangle\nprove cos(angle(A,B,C)) ^ 2 + sin(angle(A,B,C)) ^ 2 = 2");
        assert!(!sol.proved);
    }

    #[test]
    fn trig_identities_read_as_written_by_hand() {
        let (sol, v) = view("A B C = triangle\nprove cos(angle(A,B,C)) ^ 2 + sin(angle(A,B,C)) ^ 2 = 1");
        assert!(sol.proved);
        assert_eq!(v.goal.as_ref().unwrap().args[0], "cos\u{b2}\u{2220}ABC + sin\u{b2}\u{2220}ABC = 1");
        assert_eq!(v.proof.steps[0].rule_name.as_deref(), Some("Pythagorean identity"));
        assert_eq!(v.proof.steps[0].rule_name_ro.as_deref(), Some("identitatea fundamentală a trigonometriei"));
    }

    #[test]
    fn multiplying_both_sides_shows_the_new_relation() {
        let (sol, v) = view("A=free\nB=free\nC = point: perp(A,B,A,C)\nH = foot(A, line(B,C))\nprove dist(A,B)*cos(angle(A,B,C)) = dist(B,H)");
        assert!(sol.proved);
        let texts = step_texts(&v);
        assert!(texts.iter().any(|t| t.contains("Multiply both sides of the claim by BH (BH \u{2260} 0): it becomes AB \u{b7} cos\u{2220}ABC \u{b7} BH = BH\u{b2}.")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("so cos\u{2220}AHB = 0, hence AH\u{b7}BH\u{b7}cos\u{2220}AHB = 0.")), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("both sides multiplied")), "{texts:?}");
    }

    #[test]
    fn closure_steps_show_the_facts_later_steps_cite() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../alphageometry-rs/examples/named/symmedian_ratio.geo")).unwrap();
        let (sol, v) = view(&src);
        assert!(sol.proved);
        let closure = v.proof.steps.iter().find(|s| s.rule == "closure").expect("closure step");
        let subs: Vec<String> = closure.subs.iter().map(|u| u.fact.args.join(" ")).collect();
        assert!(subs.iter().all(|t| !t.contains('\u{223c}')), "mirror similarities are shown as equal lengths: {subs:?}");
        assert!(closure.subs.iter().all(|u| u.fact.kind != "simtri"), "{subs:?}");
        assert!(subs.iter().any(|t| t.contains("\u{2220}ABT + \u{2220}ACB = 180\u{b0}")), "{subs:?}");
        assert!(subs.iter().any(|t| t.contains("\u{2220}BAC = \u{2220}CBT")), "{subs:?}");
    }

    #[test]
    fn points_a_proof_introduces_are_drawn() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../alphageometry-rs/examples/named/euler_formula_oi.geo")).unwrap();
        let (sol, v) = view(&src);
        assert!(sol.proved);
        assert!(v.points.iter().any(|p| p.name == "N" && p.aux), "{:?}", v.points);
        assert!(v.svg.contains(">N</text>"), "N is labelled in the figure");
        assert!(v.aux.iter().any(|a| a.name == "N" && a.kind == "intersect"), "{:?}", v.aux);
        let ro = crate::i18n::prose_ro(&v.proof.steps.iter().find(|s| s.rule == "theorem").unwrap().fact.args[0]);
        assert!(ro.contains("față de cercul (O"), "{ro}");
        assert!(v.proof.steps[0].fact.args[0].contains("through A, B, C"), "{:?}", v.proof.steps[0].fact.args);
    }

    #[test]
    fn non_point_definitions_and_bare_trig_arguments_are_explained() {
        let d = diagnose("A B C = triangle\nD = circle(A, B, C)\nprove coll(A, B, D)", "compile error: expected a point");
        assert_eq!((d.key, d.line, d.col, d.token.as_deref()), ("not_a_point", 2, 5, Some("circle")));
        let d = diagnose("A B C = triangle\nprove sin(A) = 1", "compile error: expected LParen, found Some(RParen)");
        assert_eq!((d.key, d.line, d.col, d.token.as_deref()), ("trig_angle", 2, 11, Some("sin")));
        let d = diagnose("A B C = triangle\nprove coll(A, B)", "compile error: `coll` expects at least 3 arguments, got 2");
        assert_eq!((d.key, d.expected, d.got), ("arity_min", Some(3), Some(2)));
        assert_eq!(
            readable_engine_message("compile error: unexpected token Some(Ident(\"degrees\"))"),
            "compile error: unexpected token \u{201c}degrees\u{201d}"
        );
        assert_eq!(readable_engine_message("compile error: expected LParen, found Some(RParen)"), "compile error: expected \u{201c}(\u{201d}, found \u{201c})\u{201d}");
        assert_eq!(readable_engine_message("metric prover: unexpected token None"), "metric prover: unexpected token the end of the line");
    }

    #[test]
    fn a_labelled_relation_is_multiplied_without_its_label() {
        let s = smooth_euclid("ABCD is convex in the figure, so its area splits along either diagonal: [ABC] + [ACD] = [ABD] + [BCD]; times BC.");
        assert_eq!(
            s,
            "ABCD is convex in the figure, so its area splits along either diagonal: [ABC] + [ACD] = [ABD] + [BCD], hence BC\u{b7}([ABC] + [ACD]) = BC\u{b7}([ABD] + [BCD])."
        );
    }

    fn interior(v: &View, a: &str, b: &str, c: &str) -> f64 {
        let (p, q, r) = (dot(&v.svg, a), dot(&v.svg, b), dot(&v.svg, c));
        let (u, w) = ((p.0 - q.0, p.1 - q.1), (r.0 - q.0, r.1 - q.1));
        ((u.0 * w.0 + u.1 * w.1) / (u.0.hypot(u.1) * w.0.hypot(w.1))).acos().to_degrees()
    }

    #[test]
    fn an_angle_counterexample_states_the_angles_drawn() {
        let (sol, v) = view("A B C = triangle\nprove eqangle(A, B, B, C, B, C, C, A)");
        assert!(!sol.proved);
        assert_eq!(sol.status, Status::Refuted);
        let c = v.counterexample.as_ref().expect("a counterexample");
        assert_eq!(c.kind, "angles");
        assert_eq!(c.labels, vec!["\u{2220}ABC".to_string(), "\u{2220}BCA".to_string()]);
        assert!((c.lhs - interior(&v, "A", "B", "C")).abs() < 0.3, "{c:?}");
        assert!((c.rhs - interior(&v, "B", "C", "A")).abs() < 0.3, "{c:?}");
        assert!(c.lhs + c.rhs < 180.0, "two angles of one triangle: {c:?}");
    }

    #[test]
    fn ratio_and_formula_goals_get_a_counterexample() {
        let (sol, v) = view("A B C = triangle\nM = midpoint(A, B)\nprove eqratio(A, M, M, B, A, C, C, B)");
        assert_eq!(sol.status, Status::Refuted);
        let c = v.counterexample.as_ref().expect("a ratio counterexample");
        assert_eq!((c.kind, c.labels.clone()), ("ratios", vec!["AM : MB".to_string(), "AC : CB".to_string()]));
        assert!((c.lhs - 1.0).abs() < 1e-9 && (c.rhs - 1.0).abs() > 1e-3, "{c:?}");
        let (sol, v) = view("A B C = triangle\nprove tan(angle(A,B,C)) = 1");
        assert_eq!(sol.status, Status::Refuted);
        let c = v.counterexample.as_ref().expect("a values counterexample");
        assert_eq!(c.kind, "values");
        assert!((c.rhs - 1.0).abs() < 1e-9 && (c.lhs - 1.0).abs() > 1e-3, "{c:?}");
        assert_eq!(first_number_after("LHS = -34.4 , RHS", "LHS ="), Some(-34.4));
        assert_eq!(first_number_after("LHS = 1.5e-3 ,", "LHS ="), Some(1.5e-3));
    }

    #[test]
    fn distance_relations_are_typeset_never_raw() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../alphageometry-rs/examples/named/monge_dalembert.geo")).unwrap();
        let (_, v) = view(&src);
        let all: Vec<&Fact> = v.given.iter().chain(v.proof.steps.iter().map(|s| &s.fact)).collect();
        assert!(all.iter().all(|f| f.kind != "raw"), "{all:?}");
        assert!(all.iter().any(|f| f.kind == "formula" && f.args[0].contains(" \u{b7} ")), "a product of lengths: {all:?}");
        let n = |i: u32| ["A", "B", "C", "D", "E", "F"][i as usize].to_string();
        let one = || ddar::rational::Rat::new(1, 1);
        let neg = || ddar::rational::Rat::new(-1, 1);
        let p = Predicate { name: "distmeq".into(), points: vec![0, 1, 2, 3, 0, 2, 1, 3], constants: vec![one(), one(), neg(), neg(), one()] };
        assert_eq!(dist_product_text(&n, &p).as_deref(), Some("AB \u{b7} CD = AC \u{b7} BD"));
        let p = Predicate { name: "distseq".into(), points: vec![0, 1, 1, 2, 0, 2], constants: vec![one(), one(), neg()] };
        assert_eq!(dist_sum_text(&n, &p).as_deref(), Some("AB + BC = AC"));
    }

    #[test]
    fn ddar_theorems_show_the_fact_they_derive() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let none = [None; 7];
        let f = theorem_fact("Menelaus' theorem", &s(&["A", "B", "C", "D", "E", "F"]), &none).unwrap();
        assert_eq!(f.plain(), "BD \u{b7} CE \u{b7} AF = DC \u{b7} EA \u{b7} FB");
        let f = theorem_fact("Menelaus' theorem (converse)", &s(&["A", "B", "C", "D", "E", "F"]), &none).unwrap();
        assert_eq!((f.kind, f.args.clone()), ("coll", s(&["D", "E", "F"])));
        let f = theorem_fact("Ceva's theorem (converse)", &s(&["A", "B", "C", "D", "E", "F", "P"]), &none).unwrap();
        assert_eq!((f.kind, f.args.clone()), ("concur", s(&["AD", "BE", "CF", "P"])));
        let f = theorem_fact("perpendicular \u{21d2} squared lengths (Pythagoras)", &s(&["C", "A", "C", "B"]), &none).unwrap();
        assert_eq!(f.plain(), "AB\u{b2} = CB\u{b2} + AC\u{b2}");
        assert!(!f.plain().contains("CC"), "{}", f.plain());
        let f = theorem_fact("perpendicular from squared lengths", &s(&["A", "B", "C", "D"]), &none).unwrap();
        assert_eq!((f.kind, f.args.clone()), ("perp", s(&["AB", "CD"])));
        let tri = [Some((0.0, 0.0)), Some((4.0, 0.0)), Some((0.0, 3.0))];
        let at = [Some((1.0, 1.0)), tri[0], tri[1], tri[2]];
        assert_eq!(theorem_fact("angle bisectors concur (incentre/excentre)", &s(&["I", "A", "B", "C"]), &at).unwrap().kind, "incenter");
        let at = [Some((-6.0, 6.0)), tri[0], tri[1], tri[2]];
        assert_eq!(theorem_fact("angle bisectors concur (incentre/excentre)", &s(&["J", "A", "B", "C"]), &at).unwrap().kind, "excenter");
        let f = theorem_fact("law of sines", &s(&["A", "B", "C"]), &none).unwrap();
        assert!(f.plain().starts_with("BC : sin\u{2220}BAC"), "{}", f.plain());
        let f = theorem_fact("sine of 30°: 1/2", &s(&["A", "B", "C"]), &none).unwrap();
        assert_eq!(f.plain(), "sin\u{2220}ABC = 1/2");
        let at = [Some((0.0, 0.0)), Some((1.0, 0.0)), Some((0.0, 0.0)), Some((2.0, 0.0))];
        let f = theorem_fact("lengths from squared lengths", &s(&["A", "B", "C", "D"]), &at).unwrap();
        assert_eq!((f.kind, f.args.clone()), ("rconst", s(&["CD", "AB", "2"])));
    }

    #[test]
    fn nested_line_constructors_read_as_words() {
        let a = serde_json::json!({ "kind": "intersect", "args": ["para(I, BA)", "CA"], "text": "intersect(para(I, BA), CA)" });
        let en = crate::render::aux_text(&a, crate::i18n::Lang::En);
        assert_eq!(en, "intersection of the parallel to BA through I and CA");
        let ro = crate::render::aux_text(&a, crate::i18n::Lang::Ro);
        assert_eq!(ro, "intersecția dintre paralela prin I la BA și CA");
        let t = serde_json::json!({ "kind": "intersect", "args": ["tangent_at(P, centre O)", "AB"], "text": "" });
        assert!(!crate::render::aux_text(&t, crate::i18n::Lang::En).contains('('));
    }

    #[test]
    fn imposed_squared_lengths_are_given_and_typeset() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../alphageometry-rs/examples/metric/universal_imposed_length.geo")).unwrap();
        let (sol, v) = view(&src);
        assert!(sol.proved);
        let given: Vec<String> = v.given.iter().map(Fact::plain).collect();
        assert!(given.contains(&"PA\u{b2} = 9".to_string()), "{given:?}");
        assert!(given.contains(&"PB\u{b2} = 16".to_string()), "{given:?}");
        for t in step_texts(&v) {
            assert!(!t.contains('^'), "{t}");
        }
        assert_eq!(caret_powers("PA^2 = 9 and x^ 2"), "PA\u{b2} = 9 and x^ 2");
    }

    #[test]
    fn a_second_goal_line_is_its_own_diagnosis() {
        let src = "A B C = triangle\nprove coll(A, B, C)\nprove coll(A, B, C)";
        let d = diagnose(src, "compile error: multiple goals specified");
        assert_eq!((d.key, d.line, d.col), ("multiple_goals", 3, 1));
        assert!(crate::i18n::compile_message(crate::i18n::Lang::Ro, &d).starts_with("Este permisă o singură concluzie"));
    }
}
