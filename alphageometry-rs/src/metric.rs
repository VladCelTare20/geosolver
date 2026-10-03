//! Metric goal verification: prove statements about concrete *quantities* —
//! specific lengths, angles, areas, and any algebraic combination of them
//! (sums of squares, products, ratios, √·) — that lie outside DDAR's
//! angle/ratio/linear-length algebra (e.g. `AC² + BD² = 144`, `AM = 3√3`,
//! `angle(A,B,C) = 45`).
//!
//! Method: build the figure at many independently re-sampled valid instances
//! (via [`crate::geo::build_instances`], which re-randomizes the free points for
//! each seed) and evaluate the goal equation numerically in each. A polynomial
//! identity that holds at enough generic instances holds identically
//! (Schwartz–Zippel), so agreement across dozens of random instances is a sound
//! certificate; a single disagreement is a concrete counterexample. This is a
//! *numerical* certificate, clearly labelled as such — not a DDAR-style
//! synthetic proof — but it decides the metric statements DDAR cannot express.

use crate::numerics::Vec2;
use std::collections::HashMap;

/// A metric expression over named points.
#[derive(Clone, Debug)]
pub enum MExpr {
    Num(f64),
    Dist(String, String),
    Angle(String, String, String),
    Area(String, String, String),
    Neg(Box<MExpr>),
    Sqrt(Box<MExpr>),
    Cos(Box<MExpr>),
    Sin(Box<MExpr>),
    Tan(Box<MExpr>),
    Add(Box<MExpr>, Box<MExpr>),
    Sub(Box<MExpr>, Box<MExpr>),
    Mul(Box<MExpr>, Box<MExpr>),
    Div(Box<MExpr>, Box<MExpr>),
    Pow(Box<MExpr>, f64),
}

type Coords<'a> = HashMap<&'a str, Vec2>;

impl MExpr {
    /// A compact human-readable rendering (for proof prose citing a hypothesis).
    pub(crate) fn to_display(&self) -> String {
        match self {
            MExpr::Num(v) => format!("{v}"),
            MExpr::Dist(a, b) => format!("|{a}{b}|"),
            MExpr::Angle(a, b, c) => format!("∠{a}{b}{c}"),
            MExpr::Area(a, b, c) => format!("[{a}{b}{c}]"),
            MExpr::Neg(x) => format!("-{}", x.to_display()),
            MExpr::Sqrt(x) => format!("√({})", x.to_display()),
            MExpr::Sin(x) => format!("sin({})", x.to_display()),
            MExpr::Cos(x) => format!("cos({})", x.to_display()),
            MExpr::Tan(x) => format!("tan({})", x.to_display()),
            MExpr::Add(x, y) => format!("{} + {}", x.to_display(), y.to_display()),
            MExpr::Sub(x, y) => format!("{} - {}", x.to_display(), y.to_display()),
            MExpr::Mul(x, y) => format!("{}·{}", x.to_display(), y.to_display()),
            MExpr::Div(x, y) => format!("{}/{}", x.to_display(), y.to_display()),
            MExpr::Pow(x, p) => format!("{}^{}", x.to_display(), p),
        }
    }

    pub(crate) fn eval(&self, m: &Coords) -> Result<f64, String> {
        let pt = |n: &str| {
            m.get(n)
                .copied()
                .ok_or_else(|| format!("unknown point '{n}'"))
        };
        Ok(match self {
            MExpr::Num(v) => *v,
            MExpr::Dist(a, b) => (pt(a)? - pt(b)?).norm(),
            MExpr::Angle(a, b, c) => {
                let (u, v) = (pt(a)? - pt(b)?, pt(c)? - pt(b)?);
                let cos = (u.dot(v) / (u.norm() * v.norm())).clamp(-1.0, 1.0);
                cos.acos().to_degrees()
            }
            MExpr::Area(a, b, c) => {
                let (u, v) = (pt(b)? - pt(a)?, pt(c)? - pt(a)?);
                0.5 * (u.x * v.y - u.y * v.x).abs()
            }
            MExpr::Neg(e) => -e.eval(m)?,
            MExpr::Sqrt(e) => e.eval(m)?.max(0.0).sqrt(),
            MExpr::Cos(e) => e.eval(m)?.to_radians().cos(),
            MExpr::Sin(e) => e.eval(m)?.to_radians().sin(),
            MExpr::Tan(e) => e.eval(m)?.to_radians().tan(),
            MExpr::Add(a, b) => a.eval(m)? + b.eval(m)?,
            MExpr::Sub(a, b) => a.eval(m)? - b.eval(m)?,
            MExpr::Mul(a, b) => a.eval(m)? * b.eval(m)?,
            MExpr::Div(a, b) => a.eval(m)? / b.eval(m)?,
            MExpr::Pow(a, p) => a.eval(m)?.powf(*p),
        })
    }
}

// ---------------------------------------------------------------------------
// Parser (recursive descent over a small arithmetic grammar)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char), // + - * / ^
    LParen,
    RParen,
    Comma,
    Eq,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Num(
                s[start..i]
                    .parse()
                    .map_err(|_| format!("bad number '{}'", &s[start..i]))?,
            ));
        } else if c.is_alphanumeric() || c == '_' || c == '\'' {
            let start = i;
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_alphanumeric() || ch == '_' || ch == '\'' {
                    i += 1;
                } else {
                    break;
                }
            }
            out.push(Tok::Ident(s[start..i].to_string()));
        } else {
            i += 1;
            match c {
                '+' | '-' | '*' | '/' | '^' => out.push(Tok::Op(c)),
                '(' => out.push(Tok::LParen),
                ')' => out.push(Tok::RParen),
                ',' => out.push(Tok::Comma),
                '=' => out.push(Tok::Eq),
                _ => return Err(format!("unexpected character '{c}'")),
            }
        }
    }
    Ok(out)
}

struct P {
    t: Vec<Tok>,
    i: usize,
    /// Same guard as `geo::Parser::depth` — bounds recursive-descent nesting
    /// so adversarial input parse-errors instead of stack-overflowing.
    depth: u32,
    /// Binary operators built so far (see [`MAX_EXPR_OPS`]).
    ops: u32,
}

const MAX_PARSE_DEPTH: u32 = 200;

/// Binary operators allowed in one metric relation. `a+b+c+…` parses
/// iteratively but builds a tree as deep as it is long, and every consumer
/// (evaluation, lowering, drop) recurses over it — an unbounded chain is a
/// stack overflow that aborts the whole process. Real goals use a few dozen.
pub(crate) const MAX_EXPR_OPS: u32 = 256;

/// Largest exponent magnitude accepted after `^`. The provers expand powers
/// by repeated multiplication; real problems stay in the teens
/// (`examples/metric/universal_imposed_ratio.geo` uses 19).
pub(crate) const MAX_EXPONENT: f64 = 32.0;

/// Validate an exponent read after `^`.
pub(crate) fn check_exponent(p: f64) -> Result<f64, String> {
    if p.is_finite() && p.abs() <= MAX_EXPONENT {
        Ok(p)
    } else {
        Err(format!("exponent {p} is out of range (|exponent| ≤ {MAX_EXPONENT})"))
    }
}

/// Count one binary operator against [`MAX_EXPR_OPS`].
pub(crate) fn count_op(ops: &mut u32) -> Result<(), String> {
    *ops += 1;
    if *ops > MAX_EXPR_OPS {
        Err(format!("expression too long (max {MAX_EXPR_OPS} operators)"))
    } else {
        Ok(())
    }
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn eat(&mut self, t: &Tok) -> Result<(), String> {
        if self.peek() == Some(t) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {t:?}, found {:?}", self.peek()))
        }
    }

    fn expr(&mut self) -> Result<MExpr, String> {
        let mut e = self.term()?;
        while let Some(Tok::Op(o @ ('+' | '-'))) = self.peek().cloned() {
            self.i += 1;
            count_op(&mut self.ops)?;
            let r = self.term()?;
            e = if o == '+' {
                MExpr::Add(Box::new(e), Box::new(r))
            } else {
                MExpr::Sub(Box::new(e), Box::new(r))
            };
        }
        Ok(e)
    }
    fn term(&mut self) -> Result<MExpr, String> {
        let mut e = self.power()?;
        while let Some(Tok::Op(o @ ('*' | '/'))) = self.peek().cloned() {
            self.i += 1;
            count_op(&mut self.ops)?;
            let r = self.power()?;
            e = if o == '*' {
                MExpr::Mul(Box::new(e), Box::new(r))
            } else {
                MExpr::Div(Box::new(e), Box::new(r))
            };
        }
        Ok(e)
    }
    fn power(&mut self) -> Result<MExpr, String> {
        let base = self.unary()?;
        if self.peek() == Some(&Tok::Op('^')) {
            self.i += 1;
            match self.next() {
                Some(Tok::Num(p)) => {
                    count_op(&mut self.ops)?;
                    Ok(MExpr::Pow(Box::new(base), check_exponent(p)?))
                }
                other => Err(format!("expected numeric exponent, found {other:?}")),
            }
        } else {
            Ok(base)
        }
    }
    fn unary(&mut self) -> Result<MExpr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = if self.peek() == Some(&Tok::Op('-')) {
            self.i += 1;
            self.unary().map(|e| MExpr::Neg(Box::new(e)))
        } else {
            self.atom()
        };
        self.depth -= 1;
        result
    }
    fn atom(&mut self) -> Result<MExpr, String> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(MExpr::Num(v)),
            Some(Tok::LParen) => {
                let e = self.expr()?;
                self.eat(&Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(id)) => {
                let low = id.to_ascii_lowercase();
                match low.as_str() {
                    "pi" => Ok(MExpr::Num(std::f64::consts::PI)),
                    "sqrt" | "cos" | "sin" | "tan" | "dist" | "angle" | "area" => {
                        self.eat(&Tok::LParen)?;
                        if matches!(low.as_str(), "sqrt" | "cos" | "sin" | "tan") {
                            let e = Box::new(self.expr()?);
                            self.eat(&Tok::RParen)?;
                            Ok(match low.as_str() {
                                "sqrt" => MExpr::Sqrt(e),
                                "cos" => MExpr::Cos(e),
                                "sin" => MExpr::Sin(e),
                                _ => MExpr::Tan(e),
                            })
                        } else {
                            let args = self.arg_names()?;
                            match (low.as_str(), args.len()) {
                                ("dist", 2) => Ok(MExpr::Dist(args[0].clone(), args[1].clone())),
                                ("angle", 3) => Ok(MExpr::Angle(
                                    args[0].clone(),
                                    args[1].clone(),
                                    args[2].clone(),
                                )),
                                ("area", 3) => Ok(MExpr::Area(
                                    args[0].clone(),
                                    args[1].clone(),
                                    args[2].clone(),
                                )),
                                _ => Err(format!("{low} got {} arguments", args.len())),
                            }
                        }
                    }
                    // A bare identifier is a point name only inside dist/angle/area;
                    // on its own it is meaningless in a metric expression.
                    _ => Err(format!(
                        "unexpected name '{id}' (use dist(A,B), angle(A,B,C), area(A,B,C))"
                    )),
                }
            }
            other => Err(format!("unexpected token {other:?}")),
        }
    }
    fn arg_names(&mut self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        loop {
            match self.next() {
                Some(Tok::Ident(n)) => names.push(n),
                other => return Err(format!("expected a point name, found {other:?}")),
            }
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                other => return Err(format!("expected ',' or ')', found {other:?}")),
            }
        }
        Ok(names)
    }
}

/// Parse `lhs = rhs` into two expressions.
pub(crate) fn parse_equation(s: &str) -> Result<(MExpr, MExpr), String> {
    let toks = lex(s)?;
    let eq = toks
        .iter()
        .position(|t| *t == Tok::Eq)
        .ok_or("goal must be an equation `<expr> = <expr>`")?;
    let mut lp = P {
        t: toks[..eq].to_vec(),
        i: 0,
        depth: 0,
        ops: 0,
    };
    let lhs = lp.expr()?;
    if lp.i != lp.t.len() {
        return Err("trailing tokens on left-hand side".into());
    }
    let mut rp = P {
        t: toks[eq + 1..].to_vec(),
        i: 0,
        depth: 0,
        ops: 0,
    };
    let rhs = rp.expr()?;
    if rp.i != rp.t.len() {
        return Err("trailing tokens on right-hand side".into());
    }
    Ok((lhs, rhs))
}

// ---------------------------------------------------------------------------
// Solving: classical Euclidean proof first, numerical certificate as a fallback
// ---------------------------------------------------------------------------

/// Why [`solve`] / [`verify`] did not establish a goal. Any `Err` means "not
/// proved"; an `Ok` is always a proof or a passing numerical certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricError {
    /// The goal was checked numerically and does not hold (or evaluates to a
    /// non-finite value) in some valid instance. Carries the full report,
    /// including the counterexample.
    Refuted(String),
    /// The goal or construction could not be processed (parse error, no
    /// valid instance, unsupported form, …).
    Failed(String),
}

impl MetricError {
    pub fn is_refuted(&self) -> bool {
        matches!(self, MetricError::Refuted(_))
    }

    pub fn message(&self) -> &str {
        match self {
            MetricError::Refuted(m) | MetricError::Failed(m) => m,
        }
    }
}

impl std::fmt::Display for MetricError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for MetricError {}

impl From<String> for MetricError {
    fn from(m: String) -> Self {
        MetricError::Failed(m)
    }
}

/// Solve a metric (length) goal about a coordinate-free construction.
///
/// First tries a **classical Euclidean proof** — a numbered synthetic deduction
/// citing named theorems (Pythagoras, the perpendicular-from-the-centre lemma,
/// Apollonius's median theorem, the perpendicular-chords relation, …) — via
/// [`crate::synthetic`]. This is what a human writes, and what the user asked
/// for. If the goal is outside the current theorem library (a product/ratio of
/// unsquared lengths, an area, a transcendental angle sum), it falls back to the
/// numerical certificate over `n` re-sampled instances (see [`verify`]), clearly
/// labelled as such. Coordinate/Wu proofs are never presented as the answer.
///
/// Contract: `Ok(report)` only when the goal is proved or numerically
/// verified. A goal that fails numerically is `Err(MetricError::Refuted)` —
/// including a proof whose goal does not hold in its own figure; anything that
/// could not be decided is `Err(MetricError::Failed)`.
pub fn solve(cons_src: &str, goal: &str, n: usize) -> Result<String, MetricError> {
    use crate::synthetic::Outcome;
    // 1. Additive (squared-length) Euclidean prover.
    let reason = match crate::synthetic::prove_euclidean(cons_src, goal) {
        Ok(Outcome::Proved(proof)) => return checked_proof(cons_src, goal, proof),
        Ok(Outcome::Unhandled(reason)) => reason,
        Err(e) => return Err(e.into()),
    };
    // 2. Multiplicative (ratio/product) Euclidean prover — includes the
    //    sum-of-products cases (Ptolemy) via auxiliary constructions.
    match crate::ratio::prove_ratio(cons_src, goal) {
        Ok(Outcome::Proved(proof)) => return checked_proof(cons_src, goal, proof),
        Ok(Outcome::Unhandled(_)) => {}
        Err(e) => return Err(e.into()),
    }
    // 3. Sound numerical certificate, clearly labelled (last resort). Coordinate
    //    / algebraic proofs are deliberately NOT used — only synthetic Euclidean
    //    proofs are ever presented. A failing goal is `Err(Refuted)`.
    let certificate = verify(cons_src, goal, n)?;
    Ok(format!(
        "{certificate}\n\n(No classical Euclidean proof was produced — {reason}. \
         The result above is a numerical certificate.)"
    ))
}

/// Backstop for the theorem provers: a derivation is only accepted if the goal
/// actually holds in the figure it was derived on (the first valid instance,
/// which `build_algebraic` also uses). Proofs are configuration-relative —
/// Ptolemy's equality, say, needs the drawn convex order — so this checks that
/// one figure, not every re-sampled instance.
fn checked_proof(cons_src: &str, goal: &str, proof: String) -> Result<String, MetricError> {
    let (lhs, rhs) = parse_equation(goal)?;
    let fig = crate::geo::build_algebraic(cons_src)?;
    let map: Coords = fig
        .names
        .iter()
        .map(String::as_str)
        .zip(fig.coords.iter().copied())
        .collect();
    let (l, r) = (lhs.eval(&map)?, rhs.eval(&map)?);
    if l.is_finite() && r.is_finite() && (l - r).abs() / (1.0 + r.abs()) < 1e-6 {
        return Ok(proof);
    }
    Err(MetricError::Refuted(format!(
        "NOT VERIFIED — the equation fails in the figure itself\n  {goal}\n  \
         LHS = {} , RHS = {}  (a derivation was found but rejected: it cannot be sound)",
        fmt(l),
        fmt(r)
    )))
}

// ---------------------------------------------------------------------------
// Verification (numerical certificate)
// ---------------------------------------------------------------------------

/// Verify the metric equation `goal` over the figure described by `cons_src`
/// (a coordinate-free construction program), across up to `n` re-sampled
/// instances. `Ok` carries the certificate; a goal that fails — or evaluates
/// to a non-finite value — in any instance is `Err(MetricError::Refuted)`
/// with the counterexample report.
pub fn verify(cons_src: &str, goal: &str, n: usize) -> Result<String, MetricError> {
    let (lhs, rhs) = parse_equation(goal)?;
    let instances = crate::geo::build_instances(cons_src, n)?;

    let (mut worst_rel, mut worst): (f64, Option<(f64, f64)>) = (0.0, None);
    let mut lhs_vals = Vec::new();
    for inst in &instances {
        let map: Coords = inst.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        let l = lhs.eval(&map)?;
        let r = rhs.eval(&map)?;
        lhs_vals.push(l);
        // NaN compares false with everything, so test for "not held" rather
        // than "worse than the worst so far".
        let rel = if l.is_finite() && r.is_finite() {
            (l - r).abs() / (1.0 + r.abs())
        } else {
            f64::INFINITY
        };
        if worst.is_none() || rel > worst_rel {
            worst_rel = rel;
            worst = Some((l, r));
        }
    }

    let held = worst_rel < 1e-6;
    let n = instances.len();
    let mut out = String::new();
    if !held {
        let (l, r) = worst.unwrap_or((f64::NAN, f64::NAN));
        out.push_str(&format!("NOT VERIFIED — the equation fails\n  {goal}\n"));
        out.push_str(&format!(
            "  counterexample instance: LHS = {} , RHS = {}  (relative error {:.1e})",
            fmt(l),
            fmt(r),
            worst_rel
        ));
        return Err(MetricError::Refuted(out));
    }
    let mean_lhs = lhs_vals.iter().sum::<f64>() / n as f64;
    // spread of the LHS across instances — tells the reader whether the quantity
    // genuinely varied (a strong test) or was fixed by the construction.
    let spread = lhs_vals
        .iter()
        .fold(0.0f64, |m, &v| m.max((v - mean_lhs).abs()));
    out.push_str(&format!(
        "VERIFIED (numerically, over {n} independently re-sampled instances)\n"
    ));
    out.push_str(&format!("  {goal}\n"));
    out.push_str(&format!(
        "  value: {} ≈ {}   (max relative error {:.1e}",
        fmt(mean_lhs),
        pretty(mean_lhs),
        worst_rel
    ));
    if spread > 1e-6 {
        out.push_str(&format!(
            ", LHS varied over {:.3}..{:.3} across instances — a genuine identity)",
            lhs_vals.iter().cloned().fold(f64::INFINITY, f64::min),
            lhs_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        ));
    } else {
        out.push_str(", quantity fixed by the construction)");
    }
    Ok(out)
}

fn fmt(v: f64) -> String {
    format!("{v:.6}")
}

/// Try to render `v` as a tidy closed form: an integer, a simple fraction, or
/// `a√b` (recognizing `v² ≈ rational`). Falls back to a decimal.
fn pretty(v: f64) -> String {
    if v.abs() < 1e-9 {
        return "0".into();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let a = v.abs();
    // integer
    if (a - a.round()).abs() < 1e-6 {
        return format!("{sign}{}", a.round() as i64);
    }
    // small rational
    for q in 2..=12i64 {
        let p = a * q as f64;
        if (p - p.round()).abs() < 1e-6 {
            return format!("{sign}{}/{}", p.round() as i64, q);
        }
    }
    // a√b : v² should be rational; try v² ≈ m/q
    let sq = a * a;
    for q in 1..=12i64 {
        let m = sq * q as f64;
        if (m - m.round()).abs() < 1e-5 {
            let m = m.round() as i64;
            // simplify sqrt(m/q) = sqrt(m*q)/q
            let (outside, inside) = simplify_sqrt(m * q);
            let denom = q;
            return if inside == 1 {
                format!("{sign}{outside}/{denom}")
            } else if outside == 1 && denom == 1 {
                format!("{sign}√{inside}")
            } else if denom == 1 {
                format!("{sign}{outside}√{inside}")
            } else {
                format!("{sign}{outside}√{inside}/{denom}")
            };
        }
    }
    format!("{sign}{a:.6}")
}

/// `√n = outside·√inside` with `inside` square-free.
fn simplify_sqrt(n: i64) -> (i64, i64) {
    let (mut outside, mut inside) = (1i64, n.max(0));
    let mut f = 2i64;
    while f * f <= inside {
        while inside % (f * f) == 0 {
            inside /= f * f;
            outside *= f;
        }
        f += 1;
    }
    (outside, inside)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_distances_and_powers() {
        let (l, r) = parse_equation("dist(A,B)^2 + dist(B,C)^2 = dist(A,C)^2").unwrap();
        let mut m: Coords = HashMap::new();
        m.insert("A", Vec2::new(0.0, 0.0));
        m.insert("B", Vec2::new(3.0, 0.0));
        m.insert("C", Vec2::new(3.0, 4.0));
        assert!((l.eval(&m).unwrap() - r.eval(&m).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn angle_in_degrees() {
        let (l, _) = parse_equation("angle(A,B,C) = 90").unwrap();
        let mut m: Coords = HashMap::new();
        m.insert("A", Vec2::new(1.0, 0.0));
        m.insert("B", Vec2::new(0.0, 0.0));
        m.insert("C", Vec2::new(0.0, 1.0));
        assert!((l.eval(&m).unwrap() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn pretty_forms() {
        assert_eq!(pretty(144.0), "144");
        assert_eq!(pretty(27.0f64.sqrt()), "3√3");
        assert_eq!(pretty(0.5), "1/2");
    }

    /// Same class of bug as `geo.rs`'s parser (see that file's
    /// `deeply_nested_unary_minus_is_rejected_not_accepted`): this is a
    /// second, independent recursive-descent parser and needs its own cap.
    #[test]
    fn deeply_nested_unary_minus_is_rejected_not_accepted() {
        let lhs = "dist(A, B)";
        let mut rhs = String::new();
        rhs.push_str(&"-".repeat(500));
        rhs.push('5');
        let src = format!("{lhs} = {rhs}");
        let result = parse_equation(&src);
        assert!(
            result.is_err(),
            "500 levels of unary-minus nesting should be rejected by a depth cap"
        );
        assert!(result.unwrap_err().contains("nested too deeply"));
    }
}
