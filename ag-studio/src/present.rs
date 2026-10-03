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
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else if let Some(d) = ch.to_digit(10) {
            out.push(char::from_u32(0x2080 + d).unwrap_or(ch));
        } else if ch == '\'' {
            out.push('\u{2032}');
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

    pub fn knows(&self, raw: &str) -> bool {
        self.map.contains_key(raw)
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
}

impl Fact {
    fn new(kind: &'static str, args: Vec<String>, points: Vec<String>) -> Fact {
        Fact { kind, args, points }
    }
    /// A plain-text English rendering (exports, MCP, tests).
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
            "eqratio" => format!("{} : {} = {} : {}", a(0), a(1), a(2), a(3)),
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
    out.trim().to_string()
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
    pub fact: Fact,
    /// Step numbers this step cites.
    pub deps: Vec<usize>,
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

fn parse_ddar_proof(text: &str, problem: &Problem, names: &Names) -> ProofView {
    let mut steps = Vec::new();
    let mut conclusion = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix('\u{220e}') {
            conclusion = fact_of_pred_text(problem, names, rest.trim());
            continue;
        }
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.is_empty() || !t[digits.len()..].starts_with(". ") {
            continue;
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
        } else if let Some((rule, stmt)) = body.split_once(": ") {
            match rule {
                "assumption" | "construction" => {
                    let f = fact_of_pred_text(problem, names, stmt).unwrap_or_else(|| formula(stmt));
                    ("given", if rule == "assumption" { "given" } else { "construction" }, None, f)
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
                    let v: Vec<String> = stmt.split_whitespace().map(|x| names.get(x)).collect();
                    ("step", "concyclic", None, Fact::new("cyclic", v.clone(), v))
                }
                "segment arithmetic" => {
                    let s = stmt.trim_end_matches(" (add/mul transfer)");
                    ("step", "transfer", None, formula(s))
                }
                "equal arcs \u{21d4} equal chords" => {
                    let s = stmt.replace(" and ", ", ");
                    ("step", "arcchord", None, formula(&s))
                }
                other => {
                    let v: Vec<String> = stmt.split_whitespace().map(|x| names.get(x)).collect();
                    (
                        "step",
                        "theorem",
                        Some(other.to_string()),
                        Fact::new("points", v.clone(), v),
                    )
                }
            }
        } else {
            ("step", "other", None, formula(&body))
        };
        steps.push(Step { n, kind, rule, rule_name, fact, deps });
    }
    ProofView { steps, conclusion, style: "ddar" }
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

fn parse_euclid_proof(text: &str, names: &Names) -> ProofView {
    let mut steps = Vec::new();
    let mut conclusion = None;
    for line in text.lines() {
        let t = line.trim();
        let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
        let prose = |s: &str| {
            let typeset = pretty_metric_inline(s, names);
            let shown = names.rename_text(&typeset, true);
            Fact::new("prose", vec![shown], names.disp_all(&names.mentioned(s, true)))
        };
        if !digits.is_empty() && t[digits.len()..].starts_with(". ") {
            let body = &t[digits.len() + 2..];
            let rule_name = cited_theorem(body);
            let (kind, rule) = match (&rule_name, body.contains("(given)")) {
                (Some(_), _) => ("step", "theorem"),
                (None, true) => ("given", "given"),
                (None, false) => ("step", "algebra"),
            };
            steps.push(Step {
                n: digits.parse().unwrap_or(0),
                kind,
                rule,
                rule_name,
                fact: prose(body),
                deps: Vec::new(),
            });
        } else if t.starts_with("Combining") {
            conclusion = Some(prose(t.trim_end_matches('\u{220e}').trim()));
        }
    }
    ProofView { steps, conclusion, style: "euclidean" }
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
    let num: String = s[at..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
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
            let a1 = dir_angle(c(0), c(1), c(2), c(3));
            let a2 = dir_angle(c(4), c(5), c(6), c(7));
            Some(Counter { kind: "angles", lhs: a1, rhs: a2, labels: f.args })
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
    let t: String = text[..cut].trim().chars().take(80).collect();
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
    for (a, b, v) in source_lengths(&sol.input) {
        let seg = format!("{}{}", names.get(&a), names.get(&b));
        let f = Fact::new("length", vec![seg, v], vec![names.get(&a), names.get(&b)]);
        if !given.contains(&f) {
            given.push(f);
        }
    }

    let goal = match orig.goal.as_ref() {
        Some(g) if !matches!(fact_of_pred(&orig, &names, g).kind, "raw") => Some(fact_of_pred(&orig, &names, g)),
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
        (Some(p), Method::Euclidean) => parse_euclid_proof(p, &names),
        (Some(p), _) => parse_ddar_proof(p, &fig, &names),
        (None, _) => ProofView { steps: vec![], conclusion: None, style: "none" },
    };
    let aux = sol.aux_constructions.iter().map(|a| aux_view(a, &names)).collect();

    let extras = figure::Extras {
        polygons: source_polygons(&sol.input),
        metric_goal: if orig.goal.is_none() { sol.goal.clone() } else { None },
    };
    let svg = if fig.points.is_empty() {
        String::new()
    } else {
        figure::render(&fig, aux_from, &names, &extras)
    };

    View {
        points,
        given,
        goal,
        proof,
        aux,
        note: classify_note(sol),
        counterexample: counterexample(&orig, &names, sol),
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

fn found_token(msg: &str) -> Option<Option<String>> {
    let at = msg.find("found ")? + 6;
    let rest = &msg[at..];
    if rest.starts_with("None") {
        return Some(None);
    }
    let inner = rest.strip_prefix("Some(")?.trim_end_matches(')');
    let tok = if let Some(q) = inner.strip_prefix("Ident(\"") {
        q.trim_end_matches('"').to_string()
    } else if let Some(n) = inner.strip_prefix("Num(") {
        n.trim_end_matches(".0").to_string()
    } else {
        match inner {
            "Comma" => ",".into(),
            "LParen" => "(".into(),
            "RParen" => ")".into(),
            "Eq" => "=".into(),
            "Colon" => ":".into(),
            "Star" => "*".into(),
            "Plus" => "+".into(),
            "Minus" => "-".into(),
            "Slash" => "/".into(),
            "Caret" => "^".into(),
            "Question" => "?".into(),
            "Sep" => return Some(None),
            other => other.to_string(),
        }
    };
    Some(Some(tok))
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
    } else if starts("expected a numeric exponent") {
        "expect_exponent"
    } else if starts("unexpected token") || starts("expected ") {
        "unexpected_token"
    } else if starts("unexpected character") {
        "bad_char"
    } else if starts("unknown relation") {
        "unknown_relation"
    } else if starts("unknown name") {
        "unknown_name"
    } else if starts("unknown construction") {
        "unknown_construction"
    } else if msg.contains(" expects ") && msg.contains(" argument") {
        "arity"
    } else if msg.contains("is already defined") {
        "redefined"
    } else if starts("bad number") {
        "bad_number"
    } else if starts("could not build") {
        "degenerate"
    } else if starts("empty program") {
        "empty"
    } else if msg.contains("needs a `prove") {
        "no_goal"
    } else if msg.contains("`point:` defines exactly one point") {
        "point_one"
    } else {
        "other"
    }
}

/// Locate a compile error in `input` and pick the sentence that explains it.
pub fn diagnose(input: &str, msg: &str) -> Diagnosis {
    let msg = msg
        .trim_start_matches("compile error: ")
        .trim_start_matches("parse error: ")
        .trim();
    let key = diagnosis_key(msg);
    let mut d = Diagnosis { key, line: 0, col: 0, len: 0, token: None, expected: None, got: None };
    if key == "arity" {
        let nums: Vec<usize> = msg
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|x| x.parse().ok())
            .collect();
        d.expected = nums.first().copied();
        d.got = nums.get(1).copied();
    }
    let by_name = matches!(
        key,
        "bad_char" | "unknown_relation" | "unknown_name" | "unknown_construction" | "arity" | "redefined" | "bad_number"
    );
    d.token = if by_name { backticked(msg) } else { found_token(msg).flatten() };
    let lines: Vec<&str> = input.lines().collect();
    let code_of = |l: &str| l.split('#').next().unwrap_or("").to_string();
    let line_idx: Option<usize> = match key {
        "degenerate" | "empty" | "no_goal" | "other" => None,
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
                let pick = if matches!(key, "expect_comma_paren" | "unexpected_token") {
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

    fn view(src: &str) -> (Solution, View) {
        let sol = solve(src, &SolveOptions::default()).expect("solve");
        let v = build(&sol);
        (sol, v)
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
        assert_eq!(pretty_metric("angle(A,B,C) = 60", &names), "\u{2220}ABC = 60");
    }

    #[test]
    fn titles_come_from_the_leading_comment() {
        assert_eq!(source_title("# Stewart's theorem (additive engine): x\nB = free").as_deref(), Some("Stewart's theorem"));
        assert_eq!(source_title("A B C = triangle"), None);
    }
}
