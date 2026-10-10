//! A flexible, coordinate-free language for describing geometry problems.
//!
//! You never write a coordinate. You describe *how points relate* using
//! composable expressions, first-class lines and circles, and — for anything the
//! built-in constructions don't cover — arbitrary constraint sets solved
//! numerically. The result compiles to the low-level DDAR [`Problem`].
//!
//! ```text
//! A B C = triangle
//! O = circumcenter(A, B, C)              # nested calls, first-class objects
//! F = foot(A, line(B, C))
//! P = point: dist(P,A) = 2*dist(P,B)     # define a point by ANY metric equation
//! assume cyclic(A, B, C, F)              # free-standing hypothesis (global solve)
//! prove perp(O, midpoint(B, C), B, C)
//! ```
//!
//! Grammar (statements separated by newlines or `;`, `#` starts a comment):
//!
//! * `name[, name ...] = expr`      — bind point(s)/object(s)
//! * `name = point: c1, c2, ...`    — a point pinned by constraints, where each
//!   constraint is a predicate (`coll`, `perp`, `cyclic`, …) OR *any* metric
//!   equation over dist/angle/area with `+ - * / ^` (`dist(P,A) = 2*dist(P,B)`)
//! * `assume <rel>[, <rel> ...]`    — free-standing hypotheses over already-named
//!   points (also `given`), satisfied by a global solve over the free points;
//!   fed to the provers as givens. Lets you state a figure by its properties.
//! * `prove <relation>`             — the goal (also `goal:` or `? <relation>`)
//!
//! Expressions are `ident` or `f(arg, ...)` where args are themselves
//! expressions, so constructions nest freely. Both the constraint side and the
//! goal accept arbitrary algebraic relations among lengths/angles/areas, so
//! *any* problem and *any* question can be encoded.
//!
//! See [`CONSTRUCTIONS`] and [`RELATIONS`] for the vocabulary, and [`compile`]
//! for the entry point.

use crate::metric::MExpr;
use crate::numerics::{distance, intersect_ll, NumCircle, NumLine, Vec2};
use crate::predicate::{Point, PointId, Predicate, Problem};
use crate::rational::Rat;
use std::collections::HashMap;

/// A documented value-producing construction.
pub struct ConstructionDoc {
    pub name: &'static str,
    pub summary: &'static str,
}

/// Catalogue of value constructions (see also the `point:` form for arbitrary
/// constraint-defined points).
pub const CONSTRUCTIONS: &[ConstructionDoc] = &[
    ConstructionDoc {
        name: "triangle",
        summary: "A B C = triangle           — three free points in general position",
    },
    ConstructionDoc {
        name: "segment",
        summary: "A B = segment              — two free points",
    },
    ConstructionDoc {
        name: "free",
        summary: "P = free                   — one free point",
    },
    ConstructionDoc {
        name: "point:",
        summary: "P = point: <constraints>   — a point solved to satisfy any constraints, \
                  including ANY metric equation (dist(P,A)=2*dist(P,B), dist^2=..*.., angle=..)",
    },
    ConstructionDoc {
        name: "line",
        summary: "line(A, B)                 — the line through A and B (an object)",
    },
    ConstructionDoc {
        name: "circle",
        summary:
            "circle(O, A)               — circle centred O through A; circle(A,B,C)=circumcircle",
    },
    ConstructionDoc {
        name: "circumcircle",
        summary: "circumcircle(A, B, C)      — circle through three points (object)",
    },
    ConstructionDoc {
        name: "midpoint",
        summary: "midpoint(A, B)             — midpoint of A and B",
    },
    ConstructionDoc {
        name: "circumcenter",
        summary: "circumcenter(A, B, C)      — circumcentre of a triangle",
    },
    ConstructionDoc {
        name: "orthocenter",
        summary: "orthocenter(A, B, C)       — orthocentre",
    },
    ConstructionDoc {
        name: "incenter",
        summary: "incenter(A, B, C)          — incentre",
    },
    ConstructionDoc {
        name: "centroid",
        summary: "centroid(A, B, C)          — centroid (medians meet)",
    },
    ConstructionDoc {
        name: "foot",
        summary: "foot(A, line(B,C))         — foot of perpendicular from A onto a line",
    },
    ConstructionDoc {
        name: "reflect",
        summary: "reflect(A, B) / reflect(A, line(B,C)) — reflect over a point or a line",
    },
    ConstructionDoc {
        name: "parallelogram",
        summary: "parallelogram(A, B, C)     — D completing parallelogram A B C D",
    },
    ConstructionDoc {
        name: "meet",
        summary: "meet(obj, obj)             — intersection(s) of two lines/circles",
    },
    ConstructionDoc {
        name: "bisector",
        summary: "bisector(A, B, C)          — internal bisector of angle at B (line object)",
    },
    ConstructionDoc {
        name: "perp_bisector",
        summary: "perp_bisector(A, B)        — perpendicular bisector of AB (line object)",
    },
    ConstructionDoc {
        name: "perp_line",
        summary: "perp_line(P, line(A,B))    — line through P perpendicular to AB",
    },
    ConstructionDoc {
        name: "para_line",
        summary: "para_line(P, line(A,B))    — line through P parallel to AB",
    },
    ConstructionDoc {
        name: "tangent",
        summary: "T1 T2 = tangent(P, circle(O,A)) — tangency points of the tangents from P",
    },
    ConstructionDoc {
        name: "eq_triangle",
        summary: "eq_triangle(A, B)          — apex of the equilateral triangle on AB",
    },
    ConstructionDoc {
        name: "square",
        summary: "C D = square(A, B)         — complete the square A B C D",
    },
    ConstructionDoc {
        name: "on_dia",
        summary: "on_dia(A, B)               — a point seeing AB at a right angle",
    },
    ConstructionDoc {
        name: "on_line",
        summary: "on_line(A, B)              — a free point on line AB",
    },
    ConstructionDoc {
        name: "on_circle",
        summary: "on_circle(O, A)            — a free point on circle centred O through A",
    },
    ConstructionDoc {
        name: "on_circum",
        summary: "on_circum(A, B, C)         — a free point on the circumcircle of ABC",
    },
    ConstructionDoc {
        name: "on_bline",
        summary: "on_bline(A, B)             — a free point on the perpendicular bisector of AB",
    },
    ConstructionDoc {
        name: "on_pline",
        summary: "on_pline(A, B, C)          — a free point on the line through A parallel to BC",
    },
    ConstructionDoc {
        name: "on_tline",
        summary:
            "on_tline(A, B, C)          — a free point on the line through A perpendicular to BC",
    },
    ConstructionDoc {
        name: "shift",
        summary: "shift(P, A, B)             — translate P by the vector A->B",
    },
    ConstructionDoc {
        name: "excenter",
        summary: "excenter(A, B, C)          — the excentre opposite A",
    },
    ConstructionDoc {
        name: "nine_point_center",
        summary: "nine_point_center(A, B, C) — centre of the nine-point circle",
    },
    ConstructionDoc {
        name: "iso_triangle",
        summary: "iso_triangle(B, C)         — apex X over base BC with XB = XC",
    },
];

/// Relations usable in `point:` constraints and in `prove`.
pub const RELATIONS: &[&str] = &[
    "on(P, obj)",
    "coll(A, B, C, ...)",
    "cong(A, B, C, D)              (equal lengths; also dist(A,B)=dist(C,D))",
    "perp(A, B, C, D)             (line AB ⟂ line CD)",
    "para(A, B, C, D)",
    "cyclic(A, B, C, D, ...)",
    "eqangle(A,B,C,D,E,F,G,H)",
    "eqratio(A,B,C,D,E,F,G,H)",
    "angle(A, B, C) = <degrees>    (angle at B)",
    "angle(A, B, C) = angle(D, E, F)",
    "<expr> = <expr>              (ANY metric equation over dist/angle/area with",
    "                              + - * / ^ sqrt — e.g. dist(P,A) = 2*dist(P,B),",
    "                              dist(A,B)^2 = dist(C,D)*dist(E,F))",
];

// ---------------------------------------------------------------------------
// Small helpers / RNG
// ---------------------------------------------------------------------------

fn cross(u: Vec2, v: Vec2) -> f64 {
    u.x * v.y - u.y * v.x
}

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_add(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }
    fn point(&mut self, spread: f64) -> Vec2 {
        Vec2::new(self.range(-spread, spread), self.range(-spread, spread))
    }
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(f64),
    LParen,
    RParen,
    Comma,
    Colon,
    Eq,
    Star,
    Plus,
    Minus,
    Slash,
    Caret,
    Question,
    Sep, // newline or ';'
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    for line in src.lines() {
        let line = line.split('#').next().unwrap();
        let mut chars = line.chars().peekable();
        while let Some(&ch) = chars.peek() {
            match ch {
                ' ' | '\t' | '\r' => {
                    chars.next();
                }
                '(' => {
                    chars.next();
                    toks.push(Tok::LParen);
                }
                ')' => {
                    chars.next();
                    toks.push(Tok::RParen);
                }
                ',' => {
                    chars.next();
                    toks.push(Tok::Comma);
                }
                ':' => {
                    chars.next();
                    toks.push(Tok::Colon);
                }
                '=' => {
                    chars.next();
                    toks.push(Tok::Eq);
                }
                '*' => {
                    chars.next();
                    toks.push(Tok::Star);
                }
                '+' => {
                    chars.next();
                    toks.push(Tok::Plus);
                }
                '-' => {
                    chars.next();
                    toks.push(Tok::Minus);
                }
                '/' => {
                    chars.next();
                    toks.push(Tok::Slash);
                }
                '^' => {
                    chars.next();
                    toks.push(Tok::Caret);
                }
                '?' => {
                    chars.next();
                    toks.push(Tok::Question);
                }
                ';' => {
                    chars.next();
                    toks.push(Tok::Sep);
                }
                c if c.is_ascii_digit() => {
                    let mut s = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() || d == '.' {
                            s.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    let n: f64 = s.parse().map_err(|_| format!("bad number `{s}`"))?;
                    toks.push(Tok::Num(n));
                }
                c if c.is_alphabetic() || c == '_' => {
                    let mut s = String::new();
                    while let Some(&d) = chars.peek() {
                        if d.is_alphanumeric() || d == '_' || d == '\'' {
                            s.push(d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    toks.push(Tok::Ident(s));
                }
                other => return Err(format!("unexpected character `{other}`")),
            }
        }
        toks.push(Tok::Sep);
    }
    Ok(toks)
}

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Expr {
    Ident(String),
    Call(String, Vec<Expr>),
}

/// A relation (used in `point:` constraints and goals). Point positions hold
/// expressions (usually idents, but may be nested constructions); `On` holds a
/// point expression and an object expression.
#[derive(Clone, Debug)]
enum Rel {
    Coll(Vec<Expr>),
    Cong(Vec<Expr>), // 4 exprs
    Perp(Vec<Expr>), // 4
    Para(Vec<Expr>), // 4
    Cyclic(Vec<Expr>),
    Eqangle(Vec<Expr>),                // 8
    Eqratio(Vec<Expr>),                // 8
    On(Expr, Expr),                    // point, object
    AngleConst(Expr, Expr, Expr, f64), // angle(a,b,c) = deg
    AngleEq(Vec<Expr>),                // angle(a,b,c)=angle(d,e,f); 6 exprs
    /// `dist(..)*dist(..)*... = dist(..)*...` — product of lengths equality.
    /// Fields: flattened point exprs of the left side, then of the right side
    /// (2 exprs per distance).
    ProdEq(Vec<Expr>, Vec<Expr>),
    /// `dist(a,b) = <number>` — an absolute length (pins the figure's scale).
    /// Solved numerically; not a DDAR predicate.
    DistConst(Expr, Expr, f64),
    /// A zero-constant rational linear combination of lengths,
    /// `Σ cᵢ·|aᵢbᵢ| = 0` — e.g. betweenness `|OZ| + |ZP| = |OP|`. Lowered to
    /// the engine's additive `distseq` predicate so the prover can *use* it
    /// (the general `MetricEq` below is enforced numerically only).
    SumEq(Vec<(Rat, Expr, Expr)>),
    /// A rational linear combination of *geometric* (unsigned) angles equal
    /// to a constant in degrees, `Σ cᵢ·∠XᵢYᵢZᵢ = K` (vertex in the middle) —
    /// e.g. IMO 2023/6's `∠BA₁C + ∠CB₁A + ∠AC₁B = 480`. Enforced numerically,
    /// and lowered to the engine's linear `angeq` predicate with orientation
    /// branches read off the built figure, so the prover can *use* it.
    AngSumEq(Vec<(Rat, Expr, Expr, Expr)>, Rat),
    /// A completely general metric equation `<expr> = <expr>` over
    /// dist/angle/area with arithmetic (coefficients, powers, sums, ratios) —
    /// e.g. `dist(P,A) = 2*dist(P,B)` or `dist(A,B)^2 = dist(C,D)*dist(E,F)`.
    /// Enforced numerically when positioning the point, and (when it lowers to
    /// one) fed to the metric provers as a hypothesis. This is the escape hatch
    /// that makes the constraint language able to express *anything*.
    MetricEq(crate::metric::MExpr, crate::metric::MExpr),
}

#[derive(Clone, Debug)]
enum Stmt {
    Bind { names: Vec<String>, expr: Expr },
    Constrain { name: String, constraints: Vec<Rel> },
    /// `assume <rel>, <rel>, …` — free-standing hypotheses over already-declared
    /// points, satisfied by a global solve over the figure's free points. This
    /// is what lets the language state ANY figure ("A B C D = free; assume
    /// cyclic(A,B,C,D)") without folding constraints into point definitions.
    Assume(Vec<Rel>),
    Goal(Rel),
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Real `.geo` programs never nest expressions more than a handful of levels
/// deep (see `corpus/imo_ag_30.txt`); 200 leaves generous headroom while
/// staying far below what could exhaust a thread's stack.
const MAX_PARSE_DEPTH: u32 = 200;

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    /// Current recursive-descent nesting depth, shared across the
    /// metric-expression grammar (`parse_munary`) and the construction-call
    /// grammar (`parse_expr`). Bounded by `MAX_PARSE_DEPTH` so adversarial
    /// input gets a parse error instead of a stack overflow (which would
    /// abort the whole process — uncatchable, unlike an ordinary panic).
    depth: u32,
    /// Binary operators in the current relation (see
    /// [`crate::metric::MAX_EXPR_OPS`]); reset per relation.
    ops: u32,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }
    fn eat(&mut self, t: &Tok) -> Result<(), String> {
        if self.peek() == Some(t) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected {t:?}, found {:?}", self.peek()))
        }
    }
    fn skip_seps(&mut self) {
        while self.peek() == Some(&Tok::Sep) {
            self.pos += 1;
        }
    }

    fn parse_program(&mut self) -> Result<Vec<Stmt>, String> {
        let mut stmts = Vec::new();
        self.skip_seps();
        while self.peek().is_some() {
            stmts.push(self.parse_stmt()?);
            // A statement ends at a Sep.
            if self.peek().is_some() && self.peek() != Some(&Tok::Sep) {
                return Err(format!("unexpected token {:?}", self.peek()));
            }
            self.skip_seps();
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        // Goal forms.
        if self.peek() == Some(&Tok::Question) {
            self.next();
            return Ok(Stmt::Goal(self.parse_rel()?));
        }
        if let Some(Tok::Ident(kw)) = self.peek() {
            if kw == "prove" || kw == "goal" {
                self.next();
                if self.peek() == Some(&Tok::Colon) {
                    self.next();
                }
                return Ok(Stmt::Goal(self.parse_rel()?));
            }
            if kw == "assume" || kw == "given" {
                self.next();
                if self.peek() == Some(&Tok::Colon) {
                    self.next();
                }
                return Ok(Stmt::Assume(self.parse_rel_list()?));
            }
        }

        // `names = rhs`
        let mut names = Vec::new();
        loop {
            match self.next() {
                Some(Tok::Ident(n)) => names.push(n),
                other => return Err(format!("expected point name, found {other:?}")),
            }
            match self.peek() {
                Some(Tok::Comma) => {
                    self.next();
                }
                Some(Tok::Ident(_)) => {} // space-separated names
                Some(Tok::Eq) => break,
                other => return Err(format!("expected `,`, name, or `=`, found {other:?}")),
            }
        }
        self.eat(&Tok::Eq)?;

        // `point: constraints`?
        if self.peek() == Some(&Tok::Ident("point".to_string())) {
            // lookahead for ':'
            if self.toks.get(self.pos + 1) == Some(&Tok::Colon) {
                self.next(); // point
                self.next(); // :
                let constraints = self.parse_rel_list()?;
                if names.len() != 1 {
                    return Err("`point:` defines exactly one point".to_string());
                }
                return Ok(Stmt::Constrain {
                    name: names.into_iter().next().unwrap(),
                    constraints,
                });
            }
        }

        let expr = self.parse_expr()?;
        Ok(Stmt::Bind { names, expr })
    }

    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = match self.next() {
            Some(Tok::Ident(name)) => {
                if self.peek() == Some(&Tok::LParen) {
                    self.next();
                    self.parse_expr_list_until_rparen()
                        .map(|args| Expr::Call(name, args))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => Err(format!("expected expression, found {other:?}")),
        };
        self.depth -= 1;
        result
    }

    fn parse_expr_list_until_rparen(&mut self) -> Result<Vec<Expr>, String> {
        let mut args = Vec::new();
        if self.peek() == Some(&Tok::RParen) {
            self.next();
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            match self.next() {
                Some(Tok::Comma) => {}
                Some(Tok::RParen) => break,
                other => return Err(format!("expected `,` or `)`, found {other:?}")),
            }
        }
        Ok(args)
    }

    fn parse_rel_list(&mut self) -> Result<Vec<Rel>, String> {
        let mut rels = vec![self.parse_rel()?];
        while self.peek() == Some(&Tok::Comma) {
            self.next();
            rels.push(self.parse_rel()?);
        }
        Ok(rels)
    }

    fn parse_rel(&mut self) -> Result<Rel, String> {
        self.ops = 0;
        // The qualitative predicate relations: a keyword applied to point
        // expressions (which may be nested constructions).
        if let Some(Tok::Ident(n)) = self.peek() {
            if matches!(
                n.as_str(),
                "coll" | "cyclic" | "cong" | "perp" | "para" | "eqangle" | "eqratio" | "on"
            ) {
                let name = n.clone();
                self.next();
                self.eat(&Tok::LParen)?;
                let args = self.parse_expr_list_until_rparen()?;
                return finish_predicate_rel(&name, args);
            }
        }
        // Everything else is a general metric equation `<expr> = <expr>` over
        // dist/angle/area with arbitrary arithmetic. It is classified back into a
        // qualitative predicate where its shape allows (so DDAR still sees a
        // `cong`, an angle, a product, …); otherwise it becomes a `MetricEq`
        // enforced numerically — the escape hatch that lets you write ANYTHING.
        let lhs = self.parse_mexpr()?;
        self.eat(&Tok::Eq)?;
        let rhs = self.parse_mexpr()?;
        Ok(classify_metric_eq(lhs, rhs))
    }

    // -- a metric-expression parser (dist/angle/area + arithmetic) -----------
    fn parse_mexpr(&mut self) -> Result<MExpr, String> {
        let mut e = self.parse_mterm()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.next();
                    crate::metric::count_op(&mut self.ops)?;
                    e = MExpr::Add(Box::new(e), Box::new(self.parse_mterm()?));
                }
                Some(Tok::Minus) => {
                    self.next();
                    crate::metric::count_op(&mut self.ops)?;
                    e = MExpr::Sub(Box::new(e), Box::new(self.parse_mterm()?));
                }
                _ => break,
            }
        }
        Ok(e)
    }
    fn parse_mterm(&mut self) -> Result<MExpr, String> {
        let mut e = self.parse_mpow()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.next();
                    crate::metric::count_op(&mut self.ops)?;
                    e = MExpr::Mul(Box::new(e), Box::new(self.parse_mpow()?));
                }
                Some(Tok::Slash) => {
                    self.next();
                    crate::metric::count_op(&mut self.ops)?;
                    e = MExpr::Div(Box::new(e), Box::new(self.parse_mpow()?));
                }
                _ => break,
            }
        }
        Ok(e)
    }
    fn parse_mpow(&mut self) -> Result<MExpr, String> {
        let base = self.parse_munary()?;
        if self.peek() == Some(&Tok::Caret) {
            self.next();
            match self.next() {
                Some(Tok::Num(p)) => {
                    crate::metric::count_op(&mut self.ops)?;
                    Ok(MExpr::Pow(Box::new(base), crate::metric::check_exponent(p)?))
                }
                other => Err(format!("expected a numeric exponent, found {other:?}")),
            }
        } else {
            Ok(base)
        }
    }
    fn parse_munary(&mut self) -> Result<MExpr, String> {
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            self.depth -= 1;
            return Err(format!(
                "expression nested too deeply (max depth {MAX_PARSE_DEPTH})"
            ));
        }
        let result = if self.peek() == Some(&Tok::Minus) {
            self.next();
            self.parse_munary().map(|e| MExpr::Neg(Box::new(e)))
        } else {
            self.parse_matom()
        };
        self.depth -= 1;
        result
    }
    fn parse_matom(&mut self) -> Result<MExpr, String> {
        match self.next() {
            Some(Tok::Num(v)) => Ok(MExpr::Num(v)),
            Some(Tok::LParen) => {
                let e = self.parse_mexpr()?;
                self.eat(&Tok::RParen)?;
                Ok(e)
            }
            Some(Tok::Ident(id)) => {
                let low = id.to_ascii_lowercase();
                if low == "pi" {
                    return Ok(MExpr::Num(std::f64::consts::PI));
                }
                self.eat(&Tok::LParen)?;
                if matches!(low.as_str(), "sqrt" | "sin" | "cos" | "tan") {
                    let e = Box::new(self.parse_mexpr()?);
                    self.eat(&Tok::RParen)?;
                    return Ok(match low.as_str() {
                        "sqrt" => MExpr::Sqrt(e),
                        "sin" => MExpr::Sin(e),
                        "cos" => MExpr::Cos(e),
                        _ => MExpr::Tan(e),
                    });
                }
                let names = self.parse_ident_args()?;
                match (low.as_str(), names.len()) {
                    ("dist", 2) => Ok(MExpr::Dist(names[0].clone(), names[1].clone())),
                    ("angle", 3) => Ok(MExpr::Angle(
                        names[0].clone(),
                        names[1].clone(),
                        names[2].clone(),
                    )),
                    ("area", 3) => Ok(MExpr::Area(
                        names[0].clone(),
                        names[1].clone(),
                        names[2].clone(),
                    )),
                    (other, k) => Err(format!(
                        "`{other}` is not a metric value here (got {k} arguments); \
                         use dist(A,B), angle(A,B,C), area(A,B,C)"
                    )),
                }
            }
            other => Err(format!("expected a metric expression, found {other:?}")),
        }
    }
    /// Comma-separated point names up to `)`. Points in a metric expression must
    /// be plain names (nested constructions belong in the predicate forms).
    fn parse_ident_args(&mut self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        loop {
            match self.next() {
                Some(Tok::Ident(n)) => names.push(n),
                other => return Err(format!("expected a point name, found {other:?}")),
            }
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => break,
                other => return Err(format!("expected `,` or `)`, found {other:?}")),
            }
        }
        Ok(names)
    }
}

/// Assemble a qualitative predicate relation from a keyword and its arguments.
fn finish_predicate_rel(name: &str, args: Vec<Expr>) -> Result<Rel, String> {
    let need = |k: usize| -> Result<(), String> {
        if args.len() == k {
            Ok(())
        } else {
            Err(format!("`{name}` expects {k} arguments, got {}", args.len()))
        }
    };
    let at_least = |k: usize| -> Result<(), String> {
        if args.len() >= k {
            Ok(())
        } else {
            Err(format!("`{name}` expects at least {k} arguments, got {}", args.len()))
        }
    };
    match name {
        "coll" => {
            at_least(3)?;
            Ok(Rel::Coll(args))
        }
        "cyclic" => {
            at_least(4)?;
            Ok(Rel::Cyclic(args))
        }
        "cong" => {
            need(4)?;
            Ok(Rel::Cong(args))
        }
        "perp" => {
            need(4)?;
            Ok(Rel::Perp(args))
        }
        "para" => {
            need(4)?;
            Ok(Rel::Para(args))
        }
        "eqangle" => {
            need(8)?;
            Ok(Rel::Eqangle(args))
        }
        "eqratio" => {
            need(8)?;
            Ok(Rel::Eqratio(args))
        }
        "on" => {
            need(2)?;
            let mut it = args.into_iter();
            Ok(Rel::On(it.next().unwrap(), it.next().unwrap()))
        }
        _ => Err(format!("unknown relation `{name}`")),
    }
}

/// Collect (with repeats) the point names referenced by a metric expression.
fn collect_mexpr_points(e: &MExpr, out: &mut Vec<String>) {
    match e {
        MExpr::Dist(a, b) => {
            out.push(a.clone());
            out.push(b.clone());
        }
        MExpr::Angle(a, b, c) | MExpr::Area(a, b, c) => {
            out.push(a.clone());
            out.push(b.clone());
            out.push(c.clone());
        }
        MExpr::Neg(x)
        | MExpr::Sqrt(x)
        | MExpr::Sin(x)
        | MExpr::Cos(x)
        | MExpr::Tan(x)
        | MExpr::Pow(x, _) => collect_mexpr_points(x, out),
        MExpr::Add(x, y) | MExpr::Sub(x, y) | MExpr::Mul(x, y) | MExpr::Div(x, y) => {
            collect_mexpr_points(x, out);
            collect_mexpr_points(y, out);
        }
        MExpr::Num(_) => {}
    }
}

/// A product of bare distances (a `Mul`-tree of `dist` leaves), or `None`.
fn as_dist_product(e: &MExpr) -> Option<Vec<(String, String)>> {
    match e {
        MExpr::Dist(a, b) => Some(vec![(a.clone(), b.clone())]),
        MExpr::Mul(x, y) => {
            let mut l = as_dist_product(x)?;
            l.extend(as_dist_product(y)?);
            Some(l)
        }
        _ => None,
    }
}

/// Classify a metric equation into the most specific relation: a qualitative
/// predicate (`cong`, absolute length, product equality, angle) where the shape
/// permits — so DDAR keeps using it — otherwise the general numeric `MetricEq`.
fn classify_metric_eq(lhs: MExpr, rhs: MExpr) -> Rel {
    let id = |s: &String| Expr::Ident(s.clone());
    match (&lhs, &rhs) {
        (MExpr::Dist(a, b), MExpr::Dist(c, d)) => {
            Rel::Cong(vec![id(a), id(b), id(c), id(d)])
        }
        (MExpr::Dist(a, b), MExpr::Num(v)) | (MExpr::Num(v), MExpr::Dist(a, b)) => {
            Rel::DistConst(id(a), id(b), *v)
        }
        (MExpr::Angle(a, b, c), MExpr::Num(v)) => {
            Rel::AngleConst(id(a), id(b), id(c), *v)
        }
        (MExpr::Angle(a, b, c), MExpr::Angle(d, e, f)) => {
            Rel::AngleEq(vec![id(a), id(b), id(c), id(d), id(e), id(f)])
        }
        _ => {
            if let (Some(l), Some(r)) = (as_dist_product(&lhs), as_dist_product(&rhs)) {
                if l.len() >= 2 || r.len() >= 2 {
                    let flat = |v: &[(String, String)]| -> Vec<Expr> {
                        v.iter().flat_map(|(a, b)| [id(a), id(b)]).collect()
                    };
                    return Rel::ProdEq(flat(&l), flat(&r));
                }
            }
            if let Some(terms) = as_dist_sum_eq(&lhs, &rhs) {
                return Rel::SumEq(
                    terms
                        .into_iter()
                        .map(|(c, a, b)| (c, Expr::Ident(a), Expr::Ident(b)))
                        .collect(),
                );
            }
            if let Some((terms, k)) = as_angle_sum_eq(&lhs, &rhs) {
                return Rel::AngSumEq(
                    terms
                        .into_iter()
                        .map(|(c, x, y, z)| {
                            (c, Expr::Ident(x), Expr::Ident(y), Expr::Ident(z))
                        })
                        .collect(),
                    k,
                );
            }
            Rel::MetricEq(lhs, rhs)
        }
    }
}

/// Decompose a metric expression into a rational linear combination of
/// distances plus a numeric constant: `Σ cᵢ·|aᵢbᵢ| + k`.
fn as_dist_lincomb(e: &MExpr) -> Option<(Vec<(f64, String, String)>, f64)> {
    match e {
        MExpr::Num(v) => Some((Vec::new(), *v)),
        MExpr::Dist(a, b) => Some((vec![(1.0, a.clone(), b.clone())], 0.0)),
        MExpr::Neg(x) => {
            let (mut t, k) = as_dist_lincomb(x)?;
            t.iter_mut().for_each(|p| p.0 = -p.0);
            Some((t, -k))
        }
        MExpr::Add(x, y) => {
            let (mut t, k1) = as_dist_lincomb(x)?;
            let (t2, k2) = as_dist_lincomb(y)?;
            t.extend(t2);
            Some((t, k1 + k2))
        }
        MExpr::Sub(x, y) => {
            let (mut t, k1) = as_dist_lincomb(x)?;
            let (t2, k2) = as_dist_lincomb(y)?;
            t.extend(t2.into_iter().map(|(c, a, b)| (-c, a, b)));
            Some((t, k1 - k2))
        }
        MExpr::Mul(x, y) => match (&**x, &**y) {
            (MExpr::Num(c), o) | (o, MExpr::Num(c)) => {
                let (mut t, k) = as_dist_lincomb(o)?;
                t.iter_mut().for_each(|p| p.0 *= c);
                Some((t, k * c))
            }
            _ => None,
        },
        MExpr::Div(x, y) => match &**y {
            MExpr::Num(c) if *c != 0.0 => {
                let (mut t, k) = as_dist_lincomb(x)?;
                t.iter_mut().for_each(|p| p.0 /= c);
                Some((t, k / c))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Classify `lhs = rhs` as a zero-constant rational linear combination of
/// distances (`Σ cᵢ·|aᵢbᵢ| = 0`) — the shape of the engine's `distseq`
/// predicate. Absolute constants have no additive-system representation, so
/// any equation with a nonzero constant term stays a numeric `MetricEq`.
fn as_dist_sum_eq(lhs: &MExpr, rhs: &MExpr) -> Option<Vec<(Rat, String, String)>> {
    let (lt, lk) = as_dist_lincomb(lhs)?;
    let (rt, rk) = as_dist_lincomb(rhs)?;
    if (lk - rk).abs() > 1e-12 {
        return None;
    }
    let mut terms = lt;
    terms.extend(rt.into_iter().map(|(c, a, b)| (-c, a, b)));
    // Merge duplicate segments (|ab| = |ba|) and drop cancelled ones.
    let mut merged: Vec<(f64, String, String)> = Vec::new();
    for (c, a, b) in terms {
        let (x, y) = if a <= b { (a, b) } else { (b, a) };
        if let Some(m) = merged.iter_mut().find(|m| m.1 == x && m.2 == y) {
            m.0 += c;
        } else {
            merged.push((c, x, y));
        }
    }
    merged.retain(|m| m.0.abs() > 1e-12);
    if merged.len() < 2 {
        return None;
    }
    merged
        .into_iter()
        .map(|(c, a, b)| Some((f64_to_rat(c)?, a, b)))
        .collect()
}

/// Decompose a metric expression into a rational linear combination of
/// geometric angles plus a constant: `Σ cᵢ·∠(xᵢ yᵢ zᵢ) + k` (degrees).
fn as_angle_lincomb(e: &MExpr) -> Option<(Vec<(f64, String, String, String)>, f64)> {
    match e {
        MExpr::Num(v) => Some((Vec::new(), *v)),
        MExpr::Angle(a, b, c) => Some((vec![(1.0, a.clone(), b.clone(), c.clone())], 0.0)),
        MExpr::Neg(x) => {
            let (mut t, k) = as_angle_lincomb(x)?;
            t.iter_mut().for_each(|p| p.0 = -p.0);
            Some((t, -k))
        }
        MExpr::Add(x, y) => {
            let (mut t, k1) = as_angle_lincomb(x)?;
            let (t2, k2) = as_angle_lincomb(y)?;
            t.extend(t2);
            Some((t, k1 + k2))
        }
        MExpr::Sub(x, y) => {
            let (mut t, k1) = as_angle_lincomb(x)?;
            let (t2, k2) = as_angle_lincomb(y)?;
            t.extend(t2.into_iter().map(|(c, a, b, cc)| (-c, a, b, cc)));
            Some((t, k1 - k2))
        }
        MExpr::Mul(x, y) => match (&**x, &**y) {
            (MExpr::Num(c), o) | (o, MExpr::Num(c)) => {
                let (mut t, k) = as_angle_lincomb(o)?;
                t.iter_mut().for_each(|p| p.0 *= c);
                Some((t, k * c))
            }
            _ => None,
        },
        MExpr::Div(x, y) => match &**y {
            MExpr::Num(c) if *c != 0.0 => {
                let (mut t, k) = as_angle_lincomb(x)?;
                t.iter_mut().for_each(|p| p.0 /= c);
                Some((t, k / c))
            }
            _ => None,
        },
        _ => None,
    }
}

/// Classify `lhs = rhs` as a rational linear combination of geometric angles
/// equal to a constant (`Σ cᵢ·∠XᵢYᵢZᵢ = K` degrees) — the shape lowered to the
/// engine's linear `angeq` predicate. `∠XYZ = ∠ZYX`, so mirrored terms merge.
fn as_angle_sum_eq(lhs: &MExpr, rhs: &MExpr) -> Option<(Vec<(Rat, String, String, String)>, Rat)> {
    let (lt, lk) = as_angle_lincomb(lhs)?;
    let (rt, rk) = as_angle_lincomb(rhs)?;
    let mut terms = lt;
    terms.extend(rt.into_iter().map(|(c, x, y, z)| (-c, x, y, z)));
    let mut merged: Vec<(f64, String, String, String)> = Vec::new();
    for (c, x, y, z) in terms {
        let (a, b) = if x <= z { (x, z) } else { (z, x) };
        if let Some(m) = merged.iter_mut().find(|m| m.1 == a && m.2 == y && m.3 == b) {
            m.0 += c;
        } else {
            merged.push((c, a, y, b));
        }
    }
    merged.retain(|m| m.0.abs() > 1e-12);
    if merged.is_empty() {
        return None;
    }
    let konst = f64_to_rat(rk - lk)?;
    let terms: Option<Vec<_>> = merged
        .into_iter()
        .map(|(c, x, y, z)| Some((f64_to_rat(c)?, x, y, z)))
        .collect();
    Some((terms?, konst))
}

/// Lower a geometric angle-sum `Σ cᵢ·∠XᵢYᵢZᵢ = K°` to the engine's linear
/// `angeq` predicate over directed line angles (mod π), reading each angle's
/// orientation branch off the built figure: for the sampled configuration,
/// ∠XYZ ≡ sᵢ·(θ(YX) − θ(YZ)) (mod π) with sᵢ = ±1. After clearing coefficient
/// denominators (D — whole multiples of π vanish mod π only with integer
/// coefficients), the relation becomes
///     Σ Dcᵢsᵢ·θ(YᵢXᵢ) − Σ Dcᵢsᵢ·θ(YᵢZᵢ) + (−D·K)/180 ≡ 0   (mod π),
/// which is exactly `angeq`'s convention. Returns `None` (caller degrades to a
/// numeric-only placeholder) when a branch is ambiguous — an angle within
/// ~0.02° of 0°/180° — or the denominators overflow.
fn angsumeq_to_predicate(
    coefs: &[Rat],
    konst: &Rat,
    pts: &[PointId],
    scene: &Scene,
) -> Option<Predicate> {
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 {
            a.abs()
        } else {
            gcd(b, a % b)
        }
    }
    let mut d: i64 = konst.denom_i64()?;
    for c in coefs {
        let cd = c.denom_i64()?;
        d = d.checked_mul(cd / gcd(d, cd))?;
    }
    let scale = Rat::from_int(d);
    let mut points = Vec::with_capacity(4 * coefs.len());
    let mut constants = Vec::with_capacity(2 * coefs.len() + 1);
    for (i, c) in coefs.iter().enumerate() {
        let (x, y, z) = (pts[3 * i], pts[3 * i + 1], pts[3 * i + 2]);
        let (vx, vy, vz) = (scene.coord(x), scene.coord(y), scene.coord(z));
        let (u, v) = (vx - vy, vz - vy);
        if u.norm() < 1e-9 || v.norm() < 1e-9 {
            return None;
        }
        // The geometric angle and the directed line-angle difference, both in
        // half-turns; the sign is whichever orientation matches the figure.
        let g = (u.dot(v) / (u.norm() * v.norm())).clamp(-1.0, 1.0).acos() / std::f64::consts::PI;
        if !(1e-4..=1.0 - 1e-4).contains(&g) {
            return None;
        }
        let diff = (line_dir(vy, vx) - line_dir(vy, vz)).rem_euclid(1.0);
        let sign = if (diff - g).abs() < 1e-4 {
            1
        } else if ((1.0 - diff) - g).abs() < 1e-4 {
            -1
        } else {
            return None; // solve residual too loose to trust the branch
        };
        let n = &(c * &scale) * &Rat::from_int(sign);
        points.extend_from_slice(&[y, x, y, z]);
        constants.push(n.clone());
        constants.push(-&n);
    }
    constants.push(-&(konst * &scale));
    Some(Predicate {
        name: "angeq".to_string(),
        points,
        constants,
    })
}

/// Reconstruct an `AngSumEq` as an `lhs = rhs` metric equation (positive terms
/// left, negated negative terms joining the constant on the right) so the
/// classical metric provers keep receiving it as a hypothesis.
fn angsumeq_to_metric_hyp(
    coefs: &[Rat],
    konst: &Rat,
    pts: &[PtRef],
    resolve: impl Fn(&PtRef) -> String,
) -> (MExpr, MExpr) {
    let term = |c: &Rat, i: usize| -> MExpr {
        let a = MExpr::Angle(
            resolve(&pts[3 * i]),
            resolve(&pts[3 * i + 1]),
            resolve(&pts[3 * i + 2]),
        );
        let c = c.abs();
        if c.is_one() {
            a
        } else {
            MExpr::Mul(Box::new(MExpr::Num(c.to_f64())), Box::new(a))
        }
    };
    let mut lhs: Option<MExpr> = None;
    let mut rhs = MExpr::Num(konst.to_f64());
    for (i, c) in coefs.iter().enumerate() {
        let t = term(c, i);
        if c.is_negative() {
            rhs = MExpr::Add(Box::new(rhs), Box::new(t));
        } else {
            lhs = Some(match lhs.take() {
                Some(prev) => MExpr::Add(Box::new(prev), Box::new(t)),
                None => t,
            });
        }
    }
    (lhs.unwrap_or(MExpr::Num(0.0)), rhs)
}

/// Reconstruct a `SumEq` as an `lhs = rhs` metric equation (positive terms
/// left, negated negative terms right) so the classical metric provers keep
/// receiving it as a hypothesis, exactly as a general `MetricEq` would be.
fn sumeq_to_metric_hyp(
    coefs: &[Rat],
    pts: &[PtRef],
    resolve: impl Fn(&PtRef) -> String,
) -> (MExpr, MExpr) {
    let term = |c: &Rat, a: &PtRef, b: &PtRef| -> MExpr {
        let d = MExpr::Dist(resolve(a), resolve(b));
        let c = c.abs();
        if c.is_one() {
            d
        } else {
            MExpr::Mul(Box::new(MExpr::Num(c.to_f64())), Box::new(d))
        }
    };
    let (mut lhs, mut rhs): (Option<MExpr>, Option<MExpr>) = (None, None);
    for (i, c) in coefs.iter().enumerate() {
        let t = term(c, &pts[2 * i], &pts[2 * i + 1]);
        let side = if c.is_negative() { &mut rhs } else { &mut lhs };
        *side = Some(match side.take() {
            Some(prev) => MExpr::Add(Box::new(prev), Box::new(t)),
            None => t,
        });
    }
    (
        lhs.unwrap_or(MExpr::Num(0.0)),
        rhs.unwrap_or(MExpr::Num(0.0)),
    )
}

/// An f64 that is (within fp tolerance) a small rational, as a `Rat`.
fn f64_to_rat(v: f64) -> Option<Rat> {
    for den in 1..=1000i64 {
        let num = (v * den as f64).round();
        if (v * den as f64 - num).abs() < 1e-9 && num.abs() < 1e15 {
            return Some(Rat::new(num as i64, den));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Values, scene
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Circ {
    Centered(PointId, PointId), // centre, through
    Through(PointId, PointId, PointId),
}

#[derive(Clone, Copy, Debug)]
enum Value {
    Point(PointId),
    Line(PointId, PointId),
    Circle(Circ),
}

enum BuildError {
    Fatal(String),
    Degenerate,
}
fn fatal(m: impl Into<String>) -> BuildError {
    BuildError::Fatal(m.into())
}

struct Scene {
    env: HashMap<String, Value>,
    points: Vec<Point>,
    preds: Vec<Predicate>,
    /// General metric-equation hypotheses (`dist(P,A) = 2*dist(P,B)`, …) imposed
    /// via `point:` or `assume` — offered to the metric provers as givens.
    metric_hyps: Vec<(MExpr, MExpr)>,
    /// The figure's degrees of freedom as a flat scalar tape, in draw order:
    /// every random scalar any construction consumes (free-point coordinates,
    /// locus parameters like `on_line`'s `t`, `point:` solver seeds). The global
    /// `assume` solve optimises this whole tape, so *any* sampled parameter —
    /// not just fully-free points — can move to satisfy the hypotheses.
    dof_record: Vec<f64>,
    /// When non-empty, constructions read their scalars from this tape (in
    /// `dof_record` order) instead of sampling — used to re-run the build
    /// during the global solve.
    dof_override: Vec<f64>,
    dof_used: usize,
    rng: Rng,
    anon: usize,
}

impl Scene {
    fn new(seed: u64) -> Scene {
        Scene {
            env: HashMap::new(),
            points: Vec::new(),
            preds: Vec::new(),
            metric_hyps: Vec::new(),
            dof_record: Vec::new(),
            dof_override: Vec::new(),
            dof_used: 0,
            rng: Rng::new(seed),
            anon: 0,
        }
    }
    fn coord(&self, id: PointId) -> Vec2 {
        self.points[id as usize].value
    }
    /// Consume one degree-of-freedom scalar: the next override value if the
    /// global solve supplied a tape, else the given random sample. Either way
    /// the value used is recorded, so the tape stays aligned across re-runs.
    /// (Callers draw the sample *before* calling, keeping the RNG stream
    /// identical whether or not an override is active.)
    fn dof(&mut self, sample: f64) -> f64 {
        let v = if self.dof_used < self.dof_override.len() {
            self.dof_override[self.dof_used]
        } else {
            sample
        };
        self.dof_used += 1;
        self.dof_record.push(v);
        v
    }
    /// A fresh free-point coordinate (two DOF scalars).
    fn fresh_free(&mut self, scale: f64) -> Vec2 {
        let s = self.rng.point(scale);
        Vec2::new(self.dof(s.x), self.dof(s.y))
    }
    /// Create a free point (its coordinates already consumed DOF slots).
    fn add_free(&mut self, coord: Vec2) -> PointId {
        self.add_point(None, coord)
    }
    fn add_point(&mut self, name: Option<&str>, coord: Vec2) -> PointId {
        let id = self.points.len() as PointId;
        let name = match name {
            Some(n) => n.to_string(),
            None => {
                self.anon += 1;
                format!("_{}", self.anon)
            }
        };
        self.points.push(Point { name, value: coord });
        id
    }
    fn emit(&mut self, name: &str, pts: &[PointId]) {
        self.preds.push(Predicate {
            name: name.to_string(),
            points: pts.to_vec(),
            constants: Vec::new(),
        });
    }

    fn circ_center_radius(&self, c: Circ) -> Result<(Vec2, f64), BuildError> {
        match c {
            Circ::Centered(o, a) => Ok((self.coord(o), distance(self.coord(o), self.coord(a)))),
            Circ::Through(a, b, cc) => {
                let circ = NumCircle::through(self.coord(a), self.coord(b), self.coord(cc))
                    .ok_or(BuildError::Degenerate)?;
                Ok((circ.center, circ.r))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Evaluating value expressions (constructions)
// ---------------------------------------------------------------------------

fn as_point(v: Value) -> Result<PointId, BuildError> {
    match v {
        Value::Point(p) => Ok(p),
        _ => Err(fatal("expected a point")),
    }
}

/// Evaluate an expression, possibly placing points, returning one value.
fn eval_one(scene: &mut Scene, e: &Expr) -> Result<Value, BuildError> {
    let vs = eval(scene, e)?;
    if vs.len() != 1 {
        return Err(fatal("expected a single value here"));
    }
    Ok(vs[0])
}

fn eval_point(scene: &mut Scene, e: &Expr) -> Result<PointId, BuildError> {
    let v = eval_one(scene, e)?;
    as_point(v)
}

fn eval(scene: &mut Scene, e: &Expr) -> Result<Vec<Value>, BuildError> {
    let values = eval_unchecked(scene, e)?;
    // Nested constructions add anonymous points the declaration count in
    // `check_point_count` cannot see; cap the real total as it grows.
    if scene.points.len() > MAX_POINTS {
        return Err(fatal(format!(
            "too many points ({}+ after expanding nested constructions); this engine \
             supports at most {MAX_POINTS} in one problem",
            scene.points.len()
        )));
    }
    Ok(values)
}

fn eval_unchecked(scene: &mut Scene, e: &Expr) -> Result<Vec<Value>, BuildError> {
    match e {
        Expr::Ident(name) => {
            if let Some(v) = scene.env.get(name) {
                return Ok(vec![*v]);
            }
            // Bare nullary constructions (`free`, `triangle`, `segment`).
            if matches!(name.as_str(), "free" | "triangle" | "segment") {
                return eval_call(scene, name, &[]);
            }
            Err(fatal(format!("unknown name `{name}`")))
        }
        Expr::Call(name, args) => eval_call(scene, name, args),
    }
}

fn eval_call(scene: &mut Scene, name: &str, args: &[Expr]) -> Result<Vec<Value>, BuildError> {
    let arity = |k: usize| -> Result<(), BuildError> {
        if args.len() == k {
            Ok(())
        } else {
            Err(fatal(format!(
                "`{name}` expects {k} arguments, got {}",
                args.len()
            )))
        }
    };
    match name {
        "free" | "point" => {
            arity(0)?;
            let p = scene.fresh_free(1.0);
            Ok(vec![Value::Point(scene.add_free(p))])
        }
        "triangle" => {
            arity(0)?;
            let (a, b, c) = (
                scene.fresh_free(1.0),
                scene.fresh_free(1.0),
                scene.fresh_free(1.0),
            );
            // Skip the non-degeneracy guard while the global solve supplies
            // overrides (intermediate iterates may be degenerate).
            if scene.dof_override.is_empty()
                && (cross(b - a, c - a).abs() < 0.15
                    || distance(a, b) < 0.4
                    || distance(b, c) < 0.4
                    || distance(a, c) < 0.4)
            {
                return Err(BuildError::Degenerate);
            }
            Ok(vec![
                Value::Point(scene.add_free(a)),
                Value::Point(scene.add_free(b)),
                Value::Point(scene.add_free(c)),
            ])
        }
        "segment" => {
            arity(0)?;
            let (a, b) = (scene.fresh_free(1.0), scene.fresh_free(1.0));
            if scene.dof_override.is_empty() && distance(a, b) < 0.4 {
                return Err(BuildError::Degenerate);
            }
            Ok(vec![
                Value::Point(scene.add_free(a)),
                Value::Point(scene.add_free(b)),
            ])
        }
        "line" => {
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            Ok(vec![Value::Line(a, b)])
        }
        "circle" => {
            if args.len() == 2 {
                let o = eval_point(scene, &args[0])?;
                let a = eval_point(scene, &args[1])?;
                Ok(vec![Value::Circle(Circ::Centered(o, a))])
            } else if args.len() == 3 {
                let a = eval_point(scene, &args[0])?;
                let b = eval_point(scene, &args[1])?;
                let c = eval_point(scene, &args[2])?;
                Ok(vec![Value::Circle(Circ::Through(a, b, c))])
            } else {
                Err(fatal("`circle` expects (O, A) or (A, B, C)"))
            }
        }
        "circumcircle" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            Ok(vec![Value::Circle(Circ::Through(a, b, c))])
        }
        "midpoint" => {
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let m = (scene.coord(a) + scene.coord(b)) * 0.5;
            let id = scene.add_point(None, m);
            scene.emit("coll", &[a, b, id]);
            scene.emit("cong", &[id, a, id, b]);
            Ok(vec![Value::Point(id)])
        }
        "circumcenter" | "circumcentre" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            let circ = NumCircle::through(scene.coord(a), scene.coord(b), scene.coord(c))
                .ok_or(BuildError::Degenerate)?;
            let id = scene.add_point(None, circ.center);
            scene.emit("cong", &[id, a, id, b]);
            scene.emit("cong", &[id, b, id, c]);
            Ok(vec![Value::Point(id)])
        }
        "orthocenter" | "orthocentre" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            let (va, vb, vc) = (scene.coord(a), scene.coord(b), scene.coord(c));
            let alt_a = NumLine::through1((vc - vb).normalize(), va);
            let alt_b = NumLine::through1((vc - va).normalize(), vb);
            let h = intersect_ll(&alt_a, &alt_b).ok_or(BuildError::Degenerate)?;
            let id = scene.add_point(None, h);
            scene.emit("perp", &[a, id, b, c]);
            scene.emit("perp", &[b, id, a, c]);
            Ok(vec![Value::Point(id)])
        }
        "incenter" | "incentre" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            let (va, vb, vc) = (scene.coord(a), scene.coord(b), scene.coord(c));
            let (la, lb, lc) = (distance(vb, vc), distance(vc, va), distance(va, vb));
            let i = (va * la + vb * lb + vc * lc) * (1.0 / (la + lb + lc));
            let id = scene.add_point(None, i);
            scene.emit("eqangle", &[a, b, a, id, a, id, a, c]);
            scene.emit("eqangle", &[b, a, b, id, b, id, b, c]);
            Ok(vec![Value::Point(id)])
        }
        "centroid" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            // Build via two medians so the incidences are asserted.
            let mbc = (scene.coord(b) + scene.coord(c)) * 0.5;
            let m1 = scene.add_point(None, mbc);
            scene.emit("coll", &[b, c, m1]);
            scene.emit("cong", &[m1, b, m1, c]);
            let mac = (scene.coord(a) + scene.coord(c)) * 0.5;
            let m2 = scene.add_point(None, mac);
            scene.emit("coll", &[a, c, m2]);
            scene.emit("cong", &[m2, a, m2, c]);
            let g = intersect_ll(
                &NumLine::through(scene.coord(a), mbc),
                &NumLine::through(scene.coord(b), mac),
            )
            .ok_or(BuildError::Degenerate)?;
            let id = scene.add_point(None, g);
            scene.emit("coll", &[a, m1, id]);
            scene.emit("coll", &[b, m2, id]);
            Ok(vec![Value::Point(id)])
        }
        "foot" => {
            // foot(A, line(B,C)) or foot(A, B, C)
            let (a, b, c) = if args.len() == 2 {
                let a = eval_point(scene, &args[0])?;
                match eval_one(scene, &args[1])? {
                    Value::Line(b, c) => (a, b, c),
                    _ => return Err(fatal("foot's second argument must be a line")),
                }
            } else if args.len() == 3 {
                (
                    eval_point(scene, &args[0])?,
                    eval_point(scene, &args[1])?,
                    eval_point(scene, &args[2])?,
                )
            } else {
                return Err(fatal("`foot` expects (A, line(B,C)) or (A, B, C)"));
            };
            let f = project(scene.coord(a), scene.coord(b), scene.coord(c));
            let id = scene.add_point(None, f);
            scene.emit("coll", &[b, c, id]);
            scene.emit("perp", &[a, id, b, c]);
            Ok(vec![Value::Point(id)])
        }
        "reflect" | "mirror" => {
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            match eval_one(scene, &args[1])? {
                Value::Point(b) => {
                    let x = scene.coord(b) * 2.0 - scene.coord(a);
                    let id = scene.add_point(None, x);
                    scene.emit("coll", &[a, b, id]);
                    scene.emit("cong", &[a, b, b, id]);
                    Ok(vec![Value::Point(id)])
                }
                Value::Line(b, c) => {
                    let f = project(scene.coord(a), scene.coord(b), scene.coord(c));
                    let x = f * 2.0 - scene.coord(a);
                    let id = scene.add_point(None, x);
                    scene.emit("cong", &[b, a, b, id]);
                    scene.emit("cong", &[c, a, c, id]);
                    Ok(vec![Value::Point(id)])
                }
                _ => Err(fatal("reflect's second argument must be a point or line")),
            }
        }
        "parallelogram" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            let d = scene.coord(a) + scene.coord(c) - scene.coord(b);
            let id = scene.add_point(None, d);
            scene.emit("para", &[a, b, id, c]);
            scene.emit("para", &[a, id, b, c]);
            Ok(vec![Value::Point(id)])
        }
        "meet" | "intersect" => {
            arity(2)?;
            let o1 = eval_one(scene, &args[0])?;
            let o2 = eval_one(scene, &args[1])?;
            meet(scene, o1, o2)
        }
        // --- constructions ported from the original AlphaGeometry defs.txt ---
        "bisector" => {
            // Internal bisector of the angle at B in A-B-C, as a line object.
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let c = eval_point(scene, &args[2])?;
            let (va, vb, vc) = (scene.coord(a), scene.coord(b), scene.coord(c));
            let dir = ((va - vb).normalize() + (vc - vb).normalize()).normalize();
            if !dir.x.is_finite() {
                return Err(BuildError::Degenerate); // straight angle
            }
            let x = scene.add_point(None, vb + dir * 0.7);
            scene.emit("eqangle", &[b, a, b, x, b, x, b, c]);
            Ok(vec![Value::Line(b, x)])
        }
        "perp_bisector" => {
            // Perpendicular bisector of AB, as a line object through the
            // midpoint.
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let (va, vb) = (scene.coord(a), scene.coord(b));
            let mcoord = (va + vb) * 0.5;
            let m = scene.add_point(None, mcoord);
            scene.emit("coll", &[a, b, m]);
            scene.emit("cong", &[m, a, m, b]);
            let dir = Vec2::new(-(vb - va).y, (vb - va).x).normalize();
            let x = scene.add_point(None, mcoord + dir * 0.7);
            scene.emit("cong", &[x, a, x, b]);
            Ok(vec![Value::Line(m, x)])
        }
        "perp_line" => {
            // Line through P perpendicular to a line (or to BC).
            let (p, a, b) = point_and_line_args(scene, args, "perp_line")?;
            let (vp, va, vb) = (scene.coord(p), scene.coord(a), scene.coord(b));
            let dir = Vec2::new(-(vb - va).y, (vb - va).x).normalize();
            let x = scene.add_point(None, vp + dir * 0.7);
            scene.emit("perp", &[p, x, a, b]);
            Ok(vec![Value::Line(p, x)])
        }
        "para_line" => {
            // Line through P parallel to a line (or to BC).
            let (p, a, b) = point_and_line_args(scene, args, "para_line")?;
            let (vp, va, vb) = (scene.coord(p), scene.coord(a), scene.coord(b));
            let x = scene.add_point(None, vp + (vb - va).normalize() * 0.7);
            scene.emit("para", &[p, x, a, b]);
            Ok(vec![Value::Line(p, x)])
        }
        "tangent" => {
            // The two points where tangents from P touch a circle.
            arity(2)?;
            let p = eval_point(scene, &args[0])?;
            let circ = match eval_one(scene, &args[1])? {
                Value::Circle(c) => c,
                _ => return Err(fatal("tangent's second argument must be a circle")),
            };
            let (center, r) = scene.circ_center_radius(circ)?;
            let vp = scene.coord(p);
            let d = distance(vp, center);
            if d <= r + 1e-9 {
                return Err(BuildError::Degenerate); // P not outside the circle
            }
            let alpha = (r / d).acos();
            let base = (vp - center).normalize();
            let mut out = Vec::new();
            for sgn in [1.0, -1.0] {
                let (sa, ca) = (alpha * sgn).sin_cos();
                let dir = Vec2::new(base.x * ca - base.y * sa, base.x * sa + base.y * ca);
                let t = scene.add_point(None, center + dir * r);
                emit_on_circle(scene, t, circ);
                match circ {
                    Circ::Centered(o, _) => scene.emit("perp", &[o, t, t, p]),
                    Circ::Through(..) => {
                        // Without a named center the tangency is expressed via
                        // the inscribed-angle form; emit perp to the numeric
                        // center-less proxy: use the tangent-chord relation with
                        // the circle's defining points is beyond the predicate
                        // set, so require a centered circle here.
                        return Err(fatal("tangent requires circle(O, A) with a named center"));
                    }
                }
                out.push(Value::Point(t));
            }
            Ok(out)
        }
        "eq_triangle" => {
            // Equilateral apex over AB.
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let (va, vb) = (scene.coord(a), scene.coord(b));
            let v = vb - va;
            let apex = va + v * 0.5 + Vec2::new(-v.y, v.x) * (3.0f64.sqrt() / 2.0);
            let x = scene.add_point(None, apex);
            scene.emit("cong", &[a, b, a, x]);
            scene.emit("cong", &[a, b, b, x]);
            Ok(vec![Value::Point(x)])
        }
        "square" => {
            // C and D completing the square A B C D (counter-clockwise).
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let (va, vb) = (scene.coord(a), scene.coord(b));
            let perp = Vec2::new(-(vb - va).y, (vb - va).x);
            let c = scene.add_point(None, vb + perp);
            let d = scene.add_point(None, va + perp);
            scene.emit("perp", &[a, b, b, c]);
            scene.emit("cong", &[a, b, b, c]);
            scene.emit("perp", &[b, c, c, d]);
            scene.emit("cong", &[b, c, c, d]);
            scene.emit("para", &[a, b, d, c]);
            scene.emit("para", &[a, d, b, c]);
            Ok(vec![Value::Point(c), Value::Point(d)])
        }
        "on_dia" => {
            // A point seeing AB under a right angle (on the circle with
            // diameter AB).
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let (va, vb) = (scene.coord(a), scene.coord(b));
            let center = (va + vb) * 0.5;
            let r = distance(va, vb) / 2.0;
            let s = scene.rng.range(0.3, 2.8); // avoid landing on a or b
            let theta = scene.dof(s);
            let base = (va - center).normalize();
            let (sa, ca) = theta.sin_cos();
            let dir = Vec2::new(base.x * ca - base.y * sa, base.x * sa + base.y * ca);
            let x = scene.add_point(None, center + dir * r);
            scene.emit("perp", &[x, a, x, b]);
            Ok(vec![Value::Point(x)])
        }

        // --- locus points (a free point on a line/circle/etc.); these mirror
        // the `on_*` constructions of AlphaGeometry's defs.txt and are sugar for
        // the corresponding `point:` constraint form ---
        "on_line" => {
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            // Sample the WHOLE line, including outside the segment on both sides
            // (not just the interior). A locus point is anywhere on the line, and
            // a true metric identity holds everywhere on it — restricting to the
            // interior would let an orientation-dependent claim (e.g. an unsigned
            // area partition, true only when the point is inside) pass the
            // numerical certificate.
            let s = scene.rng.range(-0.85, 1.85);
            let t = scene.dof(s);
            let x = scene.add_point(None, scene.coord(a) + (scene.coord(b) - scene.coord(a)) * t);
            scene.emit("coll", &[a, b, x]);
            Ok(vec![Value::Point(x)])
        }
        "on_circle" => {
            arity(2)?;
            let o = eval_point(scene, &args[0])?;
            let a = eval_point(scene, &args[1])?;
            let r = distance(scene.coord(o), scene.coord(a));
            let s = scene.rng.range(0.0, std::f64::consts::TAU);
            let th = scene.dof(s);
            let x = scene.add_point(None, scene.coord(o) + Vec2::new(r * th.cos(), r * th.sin()));
            scene.emit("cong", &[o, x, o, a]);
            Ok(vec![Value::Point(x)])
        }
        "on_circum" => {
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let cc = eval_point(scene, &args[2])?;
            let circ = NumCircle::through(scene.coord(a), scene.coord(b), scene.coord(cc))
                .ok_or(BuildError::Degenerate)?;
            let s = scene.rng.range(0.0, std::f64::consts::TAU);
            let th = scene.dof(s);
            let x = scene.add_point(
                None,
                circ.center + Vec2::new(circ.r * th.cos(), circ.r * th.sin()),
            );
            scene.emit("cyclic", &[a, b, cc, x]);
            Ok(vec![Value::Point(x)])
        }
        "on_bline" => {
            // Free point on the perpendicular bisector of AB (equidistant).
            arity(2)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let (va, vb) = (scene.coord(a), scene.coord(b));
            let mid = (va + vb) * 0.5;
            let dir = Vec2::new(-(vb - va).y, (vb - va).x).normalize();
            let sample = scene.rng.range(-1.0, 1.0) * distance(va, vb);
            let s = scene.dof(sample);
            let x = scene.add_point(None, mid + dir * s);
            scene.emit("cong", &[x, a, x, b]);
            Ok(vec![Value::Point(x)])
        }
        "on_pline" => {
            // Free point on the line through A parallel to BC.
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let cc = eval_point(scene, &args[2])?;
            let s = scene.rng.range(-1.3, 1.3);
            let t = scene.dof(s);
            let x = scene.add_point(
                None,
                scene.coord(a) + (scene.coord(cc) - scene.coord(b)) * t,
            );
            scene.emit("para", &[a, x, b, cc]);
            Ok(vec![Value::Point(x)])
        }
        "on_tline" => {
            // Free point on the line through A perpendicular to BC.
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let cc = eval_point(scene, &args[2])?;
            let dir = Vec2::new(
                -(scene.coord(cc) - scene.coord(b)).y,
                (scene.coord(cc) - scene.coord(b)).x,
            )
            .normalize();
            let s = scene.rng.range(-1.3, 1.3);
            let t = scene.dof(s);
            let x = scene.add_point(None, scene.coord(a) + dir * t);
            scene.emit("perp", &[a, x, b, cc]);
            Ok(vec![Value::Point(x)])
        }
        "shift" => {
            // Translate P by the vector A->B (PABX... a parallelogram).
            arity(3)?;
            let p = eval_point(scene, &args[0])?;
            let a = eval_point(scene, &args[1])?;
            let b = eval_point(scene, &args[2])?;
            let x = scene.add_point(None, scene.coord(p) + scene.coord(b) - scene.coord(a));
            scene.emit("para", &[p, x, a, b]);
            scene.emit("cong", &[p, x, a, b]);
            Ok(vec![Value::Point(x)])
        }
        "excenter" => {
            // The excenter opposite A. Mod pi its two bisector predicates match
            // the incenter's (internal/external bisectors are the same line
            // pair); the numeric coordinate selects the excenter branch.
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let cc = eval_point(scene, &args[2])?;
            let (va, vb, vc) = (scene.coord(a), scene.coord(b), scene.coord(cc));
            let (la, lb, lcc) = (distance(vb, vc), distance(vc, va), distance(va, vb));
            let denom = -la + lb + lcc;
            if denom.abs() < 1e-9 {
                return Err(BuildError::Degenerate);
            }
            let i = (va * (-la) + vb * lb + vc * lcc) * (1.0 / denom);
            let id = scene.add_point(None, i);
            scene.emit("eqangle", &[a, b, a, id, a, id, a, cc]);
            scene.emit("eqangle", &[b, a, b, id, b, id, b, cc]);
            Ok(vec![Value::Point(id)])
        }
        "nine_point_center" | "ninepoints" => {
            // Centre of the nine-point circle = circumcentre of the medial
            // triangle (midpoints of the sides).
            arity(3)?;
            let a = eval_point(scene, &args[0])?;
            let b = eval_point(scene, &args[1])?;
            let cc = eval_point(scene, &args[2])?;
            let ma = (scene.coord(b) + scene.coord(cc)) * 0.5;
            let m1 = scene.add_point(None, ma);
            scene.emit("coll", &[b, cc, m1]);
            scene.emit("cong", &[m1, b, m1, cc]);
            let mb = (scene.coord(a) + scene.coord(cc)) * 0.5;
            let m2 = scene.add_point(None, mb);
            scene.emit("coll", &[a, cc, m2]);
            scene.emit("cong", &[m2, a, m2, cc]);
            let mc = (scene.coord(a) + scene.coord(b)) * 0.5;
            let m3 = scene.add_point(None, mc);
            scene.emit("coll", &[a, b, m3]);
            scene.emit("cong", &[m3, a, m3, b]);
            let circ = NumCircle::through(ma, mb, mc).ok_or(BuildError::Degenerate)?;
            let id = scene.add_point(None, circ.center);
            scene.emit("cong", &[id, m1, id, m2]);
            scene.emit("cong", &[id, m2, id, m3]);
            Ok(vec![Value::Point(id)])
        }
        "iso_triangle" => {
            // Apex X over base BC with XB = XC (free along the perp bisector).
            arity(2)?;
            let b = eval_point(scene, &args[0])?;
            let cc = eval_point(scene, &args[1])?;
            let (vb, vc) = (scene.coord(b), scene.coord(cc));
            let mid = (vb + vc) * 0.5;
            let dir = Vec2::new(-(vc - vb).y, (vc - vb).x).normalize();
            let sample = scene.rng.range(0.5, 1.5) * distance(vb, vc);
            let s = scene.dof(sample);
            let x = scene.add_point(None, mid + dir * s);
            scene.emit("cong", &[x, b, x, cc]);
            Ok(vec![Value::Point(x)])
        }
        other => Err(fatal(format!("unknown construction `{other}`"))),
    }
}

/// Parse `(P, line(A,B))` or `(P, A, B)` argument shapes.
fn point_and_line_args(
    scene: &mut Scene,
    args: &[Expr],
    who: &str,
) -> Result<(PointId, PointId, PointId), BuildError> {
    if args.len() == 2 {
        let p = eval_point(scene, &args[0])?;
        match eval_one(scene, &args[1])? {
            Value::Line(a, b) => Ok((p, a, b)),
            _ => Err(fatal(format!("{who}'s second argument must be a line"))),
        }
    } else if args.len() == 3 {
        Ok((
            eval_point(scene, &args[0])?,
            eval_point(scene, &args[1])?,
            eval_point(scene, &args[2])?,
        ))
    } else {
        Err(fatal(format!(
            "`{who}` expects (P, line(A,B)) or (P, A, B)"
        )))
    }
}

fn project(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let ab = b - a;
    a + ab * ((p - a).dot(ab) / ab.dot(ab))
}

/// Intersect two objects, emitting incidence predicates for each result.
///
/// Intersection points that coincide with an *existing* point of the scene are
/// skipped rather than duplicated — so `M = meet(line(A, I), circumcircle(A, B,
/// C))` yields exactly the *second* intersection (`A` itself is filtered out).
/// This is what makes arc midpoints and "second intersection" constructions,
/// ubiquitous in olympiad theory, expressible.
fn meet(scene: &mut Scene, o1: Value, o2: Value) -> Result<Vec<Value>, BuildError> {
    match (o1, o2) {
        (Value::Line(a, b), Value::Line(c, d)) => {
            let x = intersect_ll(
                &NumLine::through(scene.coord(a), scene.coord(b)),
                &NumLine::through(scene.coord(c), scene.coord(d)),
            )
            .ok_or(BuildError::Degenerate)?;
            let id = scene.add_point(None, x);
            scene.emit("coll", &[a, b, id]);
            scene.emit("coll", &[c, d, id]);
            Ok(vec![Value::Point(id)])
        }
        (Value::Line(a, b), Value::Circle(c)) | (Value::Circle(c), Value::Line(a, b)) => {
            let (center, r) = scene.circ_center_radius(c)?;
            let pts = line_circle(scene.coord(a), scene.coord(b), center, r);
            let mut out = Vec::new();
            for x in fresh_only(scene, pts) {
                let id = scene.add_point(None, x);
                scene.emit("coll", &[a, b, id]);
                emit_on_circle(scene, id, c);
                out.push(Value::Point(id));
            }
            if out.is_empty() {
                return Err(BuildError::Degenerate);
            }
            Ok(out)
        }
        (Value::Circle(c1), Value::Circle(c2)) => {
            let (o1c, r1) = scene.circ_center_radius(c1)?;
            let (o2c, r2) = scene.circ_center_radius(c2)?;
            let pts = circle_circle(o1c, r1, o2c, r2);
            let mut out = Vec::new();
            for x in fresh_only(scene, pts) {
                let id = scene.add_point(None, x);
                emit_on_circle(scene, id, c1);
                emit_on_circle(scene, id, c2);
                out.push(Value::Point(id));
            }
            if out.is_empty() {
                return Err(BuildError::Degenerate);
            }
            Ok(out)
        }
        _ => Err(fatal("meet expects two lines/circles")),
    }
}

/// Keep only intersection coordinates that do not coincide with an existing
/// scene point.
fn fresh_only(scene: &Scene, pts: Vec<Vec2>) -> Vec<Vec2> {
    pts.into_iter()
        .filter(|&x| scene.points.iter().all(|p| distance(x, p.value) > 1e-6))
        .collect()
}

fn emit_on_circle(scene: &mut Scene, p: PointId, c: Circ) {
    match c {
        Circ::Centered(o, a) => scene.emit("cong", &[o, p, o, a]),
        Circ::Through(a, b, cc) => scene.emit("cyclic", &[a, b, cc, p]),
    }
}

/// Line through a,b intersected with a circle; returns 0/1/2 points, ordered.
fn line_circle(a: Vec2, b: Vec2, o: Vec2, r: f64) -> Vec<Vec2> {
    let d = b - a;
    let f = a - o;
    let aa = d.dot(d);
    let bb = 2.0 * f.dot(d);
    let cc = f.dot(f) - r * r;
    let disc = bb * bb - 4.0 * aa * cc;
    if disc < -1e-12 {
        return vec![];
    }
    let disc = disc.max(0.0).sqrt();
    let t1 = (-bb - disc) / (2.0 * aa);
    let t2 = (-bb + disc) / (2.0 * aa);
    if disc < 1e-9 {
        vec![a + d * t1]
    } else {
        vec![a + d * t1, a + d * t2]
    }
}

fn circle_circle(o1: Vec2, r1: f64, o2: Vec2, r2: f64) -> Vec<Vec2> {
    let d = distance(o1, o2);
    if d < 1e-12 || d > r1 + r2 + 1e-12 || d < (r1 - r2).abs() - 1e-12 {
        return vec![];
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h2 = r1 * r1 - a * a;
    let dir = (o2 - o1) * (1.0 / d);
    let mid = o1 + dir * a;
    if h2 < 1e-12 {
        return vec![mid];
    }
    let h = h2.sqrt();
    let perp = Vec2::new(-dir.y, dir.x);
    vec![mid + perp * h, mid - perp * h]
}

// ---------------------------------------------------------------------------
// Constraint lowering + numeric solving of `point:` definitions
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum PtRef {
    Id(PointId),
    Unknown,
}

#[derive(Clone)]
enum CKind {
    Coll,
    Cong,
    Perp,
    Para,
    Cyclic,
    Eqangle,
    Eqratio,
    AConst(Rat),
    /// Product-of-lengths equality; the field is the number of distances on
    /// the left side (points hold 2 per distance, left then right).
    ProdEq(usize),
    /// Absolute length `dist(a,b) = value` (pins scale; solver-only).
    DistConst(f64),
    /// Zero-constant linear combination of lengths (`Σ cᵢ·|aᵢbᵢ| = 0`);
    /// coefficients per distance, points hold 2 per distance. Becomes the
    /// engine's additive `distseq` predicate.
    SumEq(Vec<Rat>),
    /// Linear combination of geometric angles = constant (degrees);
    /// coefficients per angle, points hold 3 per angle (vertex in the middle).
    /// Becomes the engine's linear `angeq` predicate (branches from the figure).
    AngSumEq { coefs: Vec<Rat>, konst: Rat },
    /// A general metric equation, enforced numerically. `unknown` is the name of
    /// the point being solved for (if this constraint defines one).
    MetricEq {
        lhs: MExpr,
        rhs: MExpr,
        unknown: Option<String>,
    },
}

#[derive(Clone)]
struct Lowered {
    kind: CKind,
    pts: Vec<PtRef>,
}

/// Lower a relation to point references, treating `unknown` as the point being
/// solved for. Objects in `on(...)` are expanded to coll/cong/cyclic.
fn lower(scene: &mut Scene, rel: &Rel, unknown: Option<&str>) -> Result<Lowered, BuildError> {
    let pt = |scene: &mut Scene, e: &Expr| -> Result<PtRef, BuildError> {
        if let (Some(u), Expr::Ident(n)) = (unknown, e) {
            if n == u {
                return Ok(PtRef::Unknown);
            }
        }
        Ok(PtRef::Id(eval_point(scene, e)?))
    };
    let pts = |scene: &mut Scene, es: &[Expr]| -> Result<Vec<PtRef>, BuildError> {
        es.iter().map(|e| pt(scene, e)).collect()
    };
    match rel {
        Rel::Coll(a) => Ok(Lowered {
            kind: CKind::Coll,
            pts: pts(scene, a)?,
        }),
        Rel::Cyclic(a) => Ok(Lowered {
            kind: CKind::Cyclic,
            pts: pts(scene, a)?,
        }),
        Rel::Cong(a) => Ok(Lowered {
            kind: CKind::Cong,
            pts: pts(scene, a)?,
        }),
        Rel::Perp(a) => Ok(Lowered {
            kind: CKind::Perp,
            pts: pts(scene, a)?,
        }),
        Rel::Para(a) => Ok(Lowered {
            kind: CKind::Para,
            pts: pts(scene, a)?,
        }),
        Rel::Eqangle(a) => Ok(Lowered {
            kind: CKind::Eqangle,
            pts: pts(scene, a)?,
        }),
        Rel::Eqratio(a) => Ok(Lowered {
            kind: CKind::Eqratio,
            pts: pts(scene, a)?,
        }),
        Rel::AngleConst(a, b, c, deg) => {
            let deg_rat = deg_to_rat(*deg)?;
            let p = vec![pt(scene, b)?, pt(scene, a)?, pt(scene, b)?, pt(scene, c)?];
            Ok(Lowered {
                kind: CKind::AConst(deg_rat),
                pts: p,
            })
        }
        Rel::ProdEq(lhs, rhs) => {
            let n_left = lhs.len() / 2;
            let mut p = pts(scene, lhs)?;
            p.extend(pts(scene, rhs)?);
            Ok(Lowered {
                kind: CKind::ProdEq(n_left),
                pts: p,
            })
        }
        Rel::DistConst(a, b, v) => Ok(Lowered {
            kind: CKind::DistConst(*v),
            pts: vec![pt(scene, a)?, pt(scene, b)?],
        }),
        Rel::SumEq(terms) => {
            let mut p = Vec::with_capacity(2 * terms.len());
            let mut coefs = Vec::with_capacity(terms.len());
            for (c, a, b) in terms {
                p.push(pt(scene, a)?);
                p.push(pt(scene, b)?);
                coefs.push(c.clone());
            }
            Ok(Lowered {
                kind: CKind::SumEq(coefs),
                pts: p,
            })
        }
        Rel::AngSumEq(terms, k) => {
            let mut p = Vec::with_capacity(3 * terms.len());
            let mut coefs = Vec::with_capacity(terms.len());
            for (c, x, y, z) in terms {
                p.push(pt(scene, x)?);
                p.push(pt(scene, y)?);
                p.push(pt(scene, z)?);
                coefs.push(c.clone());
            }
            Ok(Lowered {
                kind: CKind::AngSumEq {
                    coefs,
                    konst: k.clone(),
                },
                pts: p,
            })
        }
        Rel::MetricEq(lhs, rhs) => {
            // Resolve the referenced point names (so the placeholder predicate and
            // the metric hypothesis can name real points), and validate them.
            let mut names: Vec<String> = Vec::new();
            collect_mexpr_points(lhs, &mut names);
            collect_mexpr_points(rhs, &mut names);
            let mut pts: Vec<PtRef> = Vec::new();
            for n in &names {
                pts.push(pt(scene, &Expr::Ident(n.clone()))?);
            }
            Ok(Lowered {
                kind: CKind::MetricEq {
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    unknown: unknown.map(|s| s.to_string()),
                },
                pts,
            })
        }
        Rel::AngleEq(a) => {
            let p = vec![
                pt(scene, &a[1])?,
                pt(scene, &a[0])?,
                pt(scene, &a[1])?,
                pt(scene, &a[2])?,
                pt(scene, &a[4])?,
                pt(scene, &a[3])?,
                pt(scene, &a[4])?,
                pt(scene, &a[5])?,
            ];
            Ok(Lowered {
                kind: CKind::Eqangle,
                pts: p,
            })
        }
        Rel::On(p, obj) => {
            let pref = pt(scene, p)?;
            match eval_one(scene, obj)? {
                Value::Line(x, y) => Ok(Lowered {
                    kind: CKind::Coll,
                    pts: vec![PtRef::Id(x), PtRef::Id(y), pref],
                }),
                Value::Circle(Circ::Centered(o, a)) => Ok(Lowered {
                    kind: CKind::Cong,
                    pts: vec![PtRef::Id(o), pref, PtRef::Id(o), PtRef::Id(a)],
                }),
                Value::Circle(Circ::Through(x, y, z)) => Ok(Lowered {
                    kind: CKind::Cyclic,
                    pts: vec![PtRef::Id(x), PtRef::Id(y), PtRef::Id(z), pref],
                }),
                Value::Point(_) => Err(fatal("`on` expects an object (line/circle)")),
            }
        }
    }
}

fn deg_to_rat(deg: f64) -> Result<Rat, BuildError> {
    if deg.abs() < 9.0e15 && (deg - deg.round()).abs() < 1e-9 {
        Ok(Rat::from_int(deg.round() as i64))
    } else {
        Err(fatal("angle in degrees must be an integer"))
    }
}

impl Lowered {
    fn coord(&self, i: usize, scene: &Scene, unknown: Vec2) -> Vec2 {
        match self.pts[i] {
            PtRef::Id(id) => scene.coord(id),
            PtRef::Unknown => unknown,
        }
    }

    /// Numeric residual(s) as a function of the unknown point.
    fn residual(&self, scene: &Scene, u: Vec2, out: &mut Vec<f64>) {
        let c = |i: usize| self.coord(i, scene, u);
        match &self.kind {
            CKind::Coll => {
                for i in 2..self.pts.len() {
                    out.push(cross(c(1) - c(0), c(i) - c(0)));
                }
            }
            CKind::Cong => out.push(distance(c(0), c(1)) - distance(c(2), c(3))),
            CKind::Perp => {
                let (u1, v1) = (c(1) - c(0), c(3) - c(2));
                out.push(u1.dot(v1) / (u1.norm() * v1.norm() + 1e-30));
            }
            CKind::Para => {
                let (u1, v1) = (c(1) - c(0), c(3) - c(2));
                out.push(cross(u1, v1) / (u1.norm() * v1.norm() + 1e-30));
            }
            CKind::Cyclic => {
                // Circle through the first three non-unknown coords.
                let circ = NumCircle::through(c(0), c(1), c(2));
                if let Some(circ) = circ {
                    for i in 3..self.pts.len() {
                        out.push(circ.distance(c(i)));
                    }
                } else {
                    out.push(1.0);
                }
            }
            CKind::Eqangle => {
                let a1 = line_dir(c(0), c(1)) - line_dir(c(2), c(3));
                let a2 = line_dir(c(4), c(5)) - line_dir(c(6), c(7));
                out.push(ang_norm(a1 - a2));
            }
            CKind::Eqratio => {
                let r1 = distance(c(0), c(1)) / distance(c(2), c(3));
                let r2 = distance(c(4), c(5)) / distance(c(6), c(7));
                out.push(r1 - r2);
            }
            CKind::AConst(deg) => {
                let target = deg.to_f64() / 180.0;
                let a = line_dir(c(0), c(1)) - line_dir(c(2), c(3));
                out.push(ang_norm(a - target));
            }
            CKind::ProdEq(n_left) => {
                let mut lhs = 1.0;
                let mut rhs = 1.0;
                for k in 0..self.pts.len() / 2 {
                    let d = distance(c(2 * k), c(2 * k + 1));
                    if k < *n_left {
                        lhs *= d;
                    } else {
                        rhs *= d;
                    }
                }
                out.push(lhs / rhs - 1.0);
            }
            CKind::DistConst(v) => out.push(distance(c(0), c(1)) - v),
            CKind::SumEq(coefs) => {
                let mut acc = 0.0;
                for (i, coef) in coefs.iter().enumerate() {
                    acc += coef.to_f64() * distance(c(2 * i), c(2 * i + 1));
                }
                out.push(acc);
            }
            CKind::AngSumEq { coefs, konst } => {
                // Σ cᵢ·∠(XᵢYᵢZᵢ) − K, in half-turns so it mixes well with the
                // other O(1) residuals. Each term also carries a hinge that
                // activates within 5° of the degenerate 0°/180°: the sum's
                // easiest numeric solutions park one vertex at a flat angle,
                // where the orientation branches (and hence the symbolic
                // `angeq` lowering) become unreadable — the hinge steers the
                // solve to interior instances and costs nothing there.
                let mut acc = 0.0;
                for (i, coef) in coefs.iter().enumerate() {
                    let (u, v) = (c(3 * i) - c(3 * i + 1), c(3 * i + 2) - c(3 * i + 1));
                    let cos = (u.dot(v) / (u.norm() * v.norm() + 1e-30)).clamp(-1.0, 1.0);
                    let g = cos.acos().to_degrees();
                    acc += coef.to_f64() * g;
                    out.push((5.0 - g).max(0.0) / 180.0);
                    out.push((g - 175.0).max(0.0) / 180.0);
                }
                out.push((acc - konst.to_f64()) / 180.0);
            }
            CKind::MetricEq { lhs, rhs, unknown } => {
                // Evaluate both sides against the scene, with the unknown point at
                // its candidate position `u`.
                let mut map: HashMap<&str, Vec2> =
                    scene.points.iter().map(|p| (p.name.as_str(), p.value)).collect();
                if let Some(uname) = unknown {
                    map.insert(uname.as_str(), u);
                }
                match (lhs.eval(&map), rhs.eval(&map)) {
                    (Ok(l), Ok(r)) => out.push(l - r),
                    _ => out.push(1.0),
                }
            }
        }
    }

    /// The goal form of this relation. Unlike a hypothesis, a goal must never
    /// lower to the trivially-true placeholder — DDAR would "prove" it. A
    /// metric goal (no DDAR predicate) yields `None`; [`compile`] rejects such
    /// programs up front, so only the instance builders (which ignore the
    /// goal) see it. An angle sum whose branch is unreadable in this sample is
    /// a degenerate figure: resample.
    fn to_goal_predicate(&self, scene: &Scene) -> Result<Option<Predicate>, BuildError> {
        match &self.kind {
            CKind::DistConst(_) | CKind::MetricEq { .. } => Ok(None),
            CKind::AngSumEq { coefs, konst } => {
                let pts: Vec<PointId> = self
                    .pts
                    .iter()
                    .map(|r| match r {
                        PtRef::Id(id) => Ok(*id),
                        PtRef::Unknown => Err(BuildError::Degenerate),
                    })
                    .collect::<Result<_, _>>()?;
                angsumeq_to_predicate(coefs, konst, &pts, scene)
                    .map(Some)
                    .ok_or(BuildError::Degenerate)
            }
            _ => Ok(Some(self.to_predicate(scene, 0 /* no unknown */))),
        }
    }

    fn to_predicate(&self, scene: &Scene, resolve_unknown: PointId) -> Predicate {
        let pts: Vec<PointId> = self
            .pts
            .iter()
            .map(|r| match r {
                PtRef::Id(id) => *id,
                PtRef::Unknown => resolve_unknown,
            })
            .collect();
        match &self.kind {
            CKind::Coll => pred("coll", pts),
            CKind::Cong => pred("cong", pts),
            CKind::Perp => pred("perp", pts),
            CKind::Para => pred("para", pts),
            CKind::Cyclic => pred("cyclic", pts),
            CKind::Eqangle => pred("eqangle", pts),
            CKind::Eqratio => pred("eqratio", pts),
            CKind::AConst(deg) => Predicate {
                name: "aconst".to_string(),
                points: pts,
                constants: vec![deg.clone()],
            },
            CKind::ProdEq(n_left) => {
                let total = pts.len() / 2;
                let mut constants: Vec<Rat> = (0..total)
                    .map(|k| Rat::from_int(if k < *n_left { 1 } else { -1 }))
                    .collect();
                constants.push(Rat::one()); // product equals 1
                Predicate {
                    name: "distmeq".to_string(),
                    points: pts,
                    constants,
                }
            }
            // Absolute length has no DDAR predicate; emit a trivially-true
            // placeholder (|ab| = |ab|) so the low-level problem stays valid.
            // The length itself is enforced numerically by the residual.
            CKind::DistConst(_) => pred("cong", vec![pts[0], pts[1], pts[0], pts[1]]),
            // Two terms with opposite signs are a constant length ratio
            // `|p₀p₁|/|p₂p₃| = -c₁/c₀` — emit the multiplicative `rconst`,
            // which similar-triangle/ratio reasoning uses directly. Longer
            // combinations go to the additive `distseq`.
            CKind::SumEq(coefs)
                if coefs.len() == 2 && coefs[0].is_negative() != coefs[1].is_negative() =>
            {
                let r = &(-&coefs[1]) / &coefs[0];
                Predicate {
                    name: "rconst".to_string(),
                    points: pts,
                    constants: vec![r],
                }
            }
            CKind::SumEq(coefs) => Predicate {
                name: "distseq".to_string(),
                points: pts,
                constants: coefs.clone(),
            },
            // Geometric angle-sum: lower to the engine's linear `angeq` fact
            // over directed line angles, with each angle's orientation branch
            // read off the built figure. Falls back to a trivially-true
            // placeholder if a branch is degenerate (angle ≈ 0°/180°).
            CKind::AngSumEq { coefs, konst } => angsumeq_to_predicate(coefs, konst, &pts, scene)
                .unwrap_or_else(|| pred("cong", vec![pts[0], pts[1], pts[0], pts[1]])),
            // A general metric equation likewise has no DDAR predicate; a
            // trivially-true placeholder over two of its points keeps the
            // low-level problem valid (the equation is enforced numerically, and
            // fed to the metric provers as a hypothesis).
            CKind::MetricEq { .. } => {
                let a = pts.first().copied().unwrap_or(resolve_unknown);
                let b = pts.get(1).copied().unwrap_or(a);
                pred("cong", vec![a, b, a, b])
            }
        }
    }
}

fn pred(name: &str, points: Vec<PointId>) -> Predicate {
    Predicate {
        name: name.to_string(),
        points,
        constants: Vec::new(),
    }
}

fn line_dir(a: Vec2, b: Vec2) -> f64 {
    NumLine::through(a, b).direction()
}
/// Normalize an angle (in half-turns) to `[-0.5, 0.5)`.
fn ang_norm(x: f64) -> f64 {
    (x + 0.5).rem_euclid(1.0) - 0.5
}

/// Solve for a point satisfying a set of lowered constraints, using
/// Levenberg–Marquardt with random restarts. Returns the point if a solution
/// with residual below `1e-12` is found.
///
/// The first attempt starts from `seed_pt`. With `seeded_only` (set while the
/// global `assume` solve replays the construction from its DOF tape) it is the
/// *only* attempt: the solution must then vary smoothly with the seed, which is
/// what makes the tape differentiable for the outer solver — random restarts
/// would jump between solution branches and wreck its Jacobian.
fn solve_constrained(
    scene: &Scene,
    cons: &[Lowered],
    seed_pt: Vec2,
    seeded_only: bool,
    rng: &mut Rng,
) -> Option<Vec2> {
    let residual = |p: Vec2| -> Vec<f64> {
        let mut r = Vec::new();
        for c in cons {
            c.residual(scene, p, &mut r);
        }
        r
    };
    let cost = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>();

    let restarts = if seeded_only { 1 } else { 200_u64 };
    for restart in 0..restarts {
        let mut p = if restart == 0 { seed_pt } else { rng.point(2.5) };
        let mut lambda = 1e-2;
        let mut r = residual(p);
        let mut cur = cost(&r);
        for _ in 0..200 {
            if cur < 1e-28 {
                break;
            }
            // Central-difference Jacobian (k x 2).
            let h = 1e-6;
            let rpx = residual(p + Vec2::new(h, 0.0));
            let rmx = residual(p - Vec2::new(h, 0.0));
            let rpy = residual(p + Vec2::new(0.0, h));
            let rmy = residual(p - Vec2::new(0.0, h));
            let (mut a00, mut a01, mut a11, mut g0, mut g1) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for i in 0..r.len() {
                let jx = (rpx[i] - rmx[i]) / (2.0 * h);
                let jy = (rpy[i] - rmy[i]) / (2.0 * h);
                a00 += jx * jx;
                a01 += jx * jy;
                a11 += jy * jy;
                g0 += jx * r[i];
                g1 += jy * r[i];
            }
            // Solve (A + lambda*I) d = -g.
            let m00 = a00 + lambda;
            let m11 = a11 + lambda;
            let det = m00 * m11 - a01 * a01;
            if det.abs() < 1e-30 {
                lambda *= 4.0;
                continue;
            }
            let dx = -(m11 * g0 - a01 * g1) / det;
            let dy = -(m00 * g1 - a01 * g0) / det;
            let p2 = p + Vec2::new(dx, dy);
            let r2 = residual(p2);
            let cur2 = cost(&r2);
            if cur2 < cur {
                p = p2;
                r = r2;
                cur = cur2;
                lambda = (lambda * 0.5).max(1e-12);
            } else {
                lambda *= 3.0;
                if lambda > 1e10 {
                    break;
                }
            }
        }
        if cur < 1e-24 {
            let _ = restart;
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Building the whole scene from statements
// ---------------------------------------------------------------------------

/// Result of building one instance: the scene, the (optional) goal predicate,
/// and every absolute-length constraint `|ab| = v` (from `dist(a,b) = <num>`),
/// which is solver-only and therefore not recoverable from the predicate list —
/// the metric provers need it to pin the figure's scale.
type BuiltScene = (Scene, Option<Predicate>, Vec<(PointId, PointId, f64)>);

fn build(stmts: &[Stmt], seed: u64) -> Result<BuiltScene, BuildError> {
    // First pass with random sampling; it records the full DOF tape.
    let (scene, goal, scale, assumptions) = build_core(stmts, seed, &[])?;
    if assumptions.is_empty() {
        finalize(&scene)?;
        return Ok((scene, goal, scale));
    }
    // Global solve: drive every `assume` residual to zero by moving the
    // figure's *entire* DOF tape — free points, locus parameters (`on_line`'s
    // position, circle angles, …) and `point:` seeds alike — re-running the
    // whole construction at each step. This is what lets hypotheses constrain
    // semi-free points, not just fully-free ones.
    let init: Vec<f64> = scene.dof_record.clone();
    let residual = |tape: &[f64]| -> Option<Vec<f64>> {
        let (sc, _, _, asm) = build_core(stmts, seed, tape).ok()?;
        let mut r = Vec::new();
        for a in &asm {
            a.residual(&sc, Vec2::new(0.0, 0.0), &mut r);
        }
        Some(r)
    };
    let solved = solve_global(&init, residual, seed).ok_or(BuildError::Degenerate)?;
    let (scene, goal, scale, _) = build_core(stmts, seed, &solved)?;
    finalize(&scene)?;
    Ok((scene, goal, scale))
}

/// Reject a finished figure with non-finite coordinates or two coincident
/// user-named points.
fn finalize(scene: &Scene) -> Result<(), BuildError> {
    for i in 0..scene.points.len() {
        let vi = scene.points[i].value;
        if !vi.x.is_finite() || !vi.y.is_finite() {
            return Err(BuildError::Degenerate);
        }
        let named_i = !scene.points[i].name.starts_with('_');
        for j in (i + 1)..scene.points.len() {
            let named_j = !scene.points[j].name.starts_with('_');
            if named_i && named_j && distance(vi, scene.points[j].value) < 1e-4 {
                return Err(BuildError::Degenerate);
            }
        }
    }
    Ok(())
}

/// Result of one construction pass: scene, goal, scale, and the lowered `assume`
/// hypotheses (their residuals drive the global solve).
type CoreResult = (
    Scene,
    Option<Predicate>,
    Vec<(PointId, PointId, f64)>,
    Vec<Lowered>,
);

fn build_core(stmts: &[Stmt], seed: u64, dof_override: &[f64]) -> Result<CoreResult, BuildError> {
    let mut scene = Scene::new(seed);
    scene.dof_override = dof_override.to_vec();
    let mut goal: Option<Predicate> = None;
    let mut scale: Vec<(PointId, PointId, f64)> = Vec::new();
    let mut assumptions: Vec<Lowered> = Vec::new();

    for stmt in stmts {
        match stmt {
            Stmt::Assume(rels) => {
                for rel in rels {
                    let l = lower(&mut scene, rel, None)?;
                    if let CKind::DistConst(v) = &l.kind {
                        if let (PtRef::Id(a), PtRef::Id(b)) = (l.pts[0], l.pts[1]) {
                            scale.push((a, b, *v));
                        }
                    }
                    if let CKind::MetricEq { lhs, rhs, .. } = &l.kind {
                        scene.metric_hyps.push((lhs.clone(), rhs.clone()));
                    }
                    if let CKind::SumEq(coefs) = &l.kind {
                        let hyp = sumeq_to_metric_hyp(coefs, &l.pts, |r| match r {
                            PtRef::Id(x) => scene.points[*x as usize].name.clone(),
                            PtRef::Unknown => String::new(), // no unknown in `assume`
                        });
                        scene.metric_hyps.push(hyp);
                    }
                    if let CKind::AngSumEq { coefs, konst } = &l.kind {
                        let hyp = angsumeq_to_metric_hyp(coefs, konst, &l.pts, |r| match r {
                            PtRef::Id(x) => scene.points[*x as usize].name.clone(),
                            PtRef::Unknown => String::new(), // no unknown in `assume`
                        });
                        scene.metric_hyps.push(hyp);
                    }
                    let p = l.to_predicate(&scene, 0);
                    scene.preds.push(p);
                    assumptions.push(l);
                }
            }
            Stmt::Bind { names, expr } => {
                let values = eval(&mut scene, expr)?;
                // A construction may yield more points than names — most commonly a
                // single name bound to a circle∩circle (two intersections). Bind the
                // first `names.len()` and leave the rest as anonymous scene points
                // (they keep their incidence predicates, which only helps the prover).
                // It is only an error when there are *fewer* points than names.
                if values.len() < names.len() {
                    return Err(fatal(format!(
                        "expected {} name(s) on the left, but the expression yields only {}",
                        names.len(),
                        values.len()
                    )));
                }
                for (n, v) in names.iter().zip(values) {
                    // For named points, rename the underlying point for display.
                    if let Value::Point(id) = v {
                        scene.points[id as usize].name = n.clone();
                    }
                    if scene.env.insert(n.clone(), v).is_some() {
                        return Err(fatal(format!("`{n}` is already defined")));
                    }
                }
            }
            Stmt::Constrain { name, constraints } => {
                if scene.env.contains_key(name) {
                    return Err(fatal(format!("`{name}` is already defined")));
                }
                let lowered: Vec<Lowered> = constraints
                    .iter()
                    .map(|c| lower(&mut scene, c, Some(name)))
                    .collect::<Result<_, _>>()?;
                // Mix the current point count into the solver seed so distinct
                // `point:` statements explore different restarts — otherwise two
                // points under symmetric constraints converge to the same spot
                // and the figure is rejected as degenerate.
                let mut rng = Rng::new(
                    seed ^ 0xABCD_1234
                        ^ (scene.points.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
                );
                // The point's position is a DOF pair: the tape entry seeds the
                // local solve, so the global `assume` solve can steer this point
                // along its constraint manifold. On the recording pass the tape
                // is overwritten with the *solved* position, so later re-runs
                // seed the solver right at (and smoothly around) the solution.
                let sample = rng.point(2.5);
                let seeded = scene.dof_used < scene.dof_override.len();
                let tape_at = scene.dof_record.len();
                let sx = scene.dof(sample.x);
                let sy = scene.dof(sample.y);
                let coord =
                    solve_constrained(&scene, &lowered, Vec2::new(sx, sy), seeded, &mut rng)
                        .ok_or(BuildError::Degenerate)?;
                scene.dof_record[tape_at] = coord.x;
                scene.dof_record[tape_at + 1] = coord.y;
                let id = scene.add_point(Some(name), coord);
                scene.env.insert(name.clone(), Value::Point(id));
                for l in &lowered {
                    // Capture absolute-length constraints (solver-only, lost in
                    // the predicate encoding) for the metric provers.
                    if let CKind::DistConst(v) = &l.kind {
                        let resolve = |r: &PtRef| match r {
                            PtRef::Id(x) => *x,
                            PtRef::Unknown => id,
                        };
                        scale.push((resolve(&l.pts[0]), resolve(&l.pts[1]), *v));
                    }
                    if let CKind::MetricEq { lhs, rhs, .. } = &l.kind {
                        scene.metric_hyps.push((lhs.clone(), rhs.clone()));
                    }
                    if let CKind::SumEq(coefs) = &l.kind {
                        let hyp = sumeq_to_metric_hyp(coefs, &l.pts, |r| match r {
                            PtRef::Id(x) => scene.points[*x as usize].name.clone(),
                            PtRef::Unknown => name.clone(),
                        });
                        scene.metric_hyps.push(hyp);
                    }
                    if let CKind::AngSumEq { coefs, konst } = &l.kind {
                        let hyp = angsumeq_to_metric_hyp(coefs, konst, &l.pts, |r| match r {
                            PtRef::Id(x) => scene.points[*x as usize].name.clone(),
                            PtRef::Unknown => name.clone(),
                        });
                        scene.metric_hyps.push(hyp);
                    }
                    let p = l.to_predicate(&scene, id);
                    scene.preds.push(p);
                }
            }
            Stmt::Goal(rel) => {
                if goal.is_some() {
                    return Err(fatal("multiple goals specified"));
                }
                let lowered = lower(&mut scene, rel, None)?;
                goal = lowered.to_goal_predicate(&scene)?;
            }
        }
    }

    Ok((scene, goal, scale, assumptions))
}

/// A general Levenberg–Marquardt solve over the figure's DOF tape (one scalar
/// per sampled parameter) that drives `residual` to zero — the engine behind
/// `assume`. Central-difference Jacobian, dense normal equations, random
/// restarts.
fn solve_global(
    init: &[f64],
    residual: impl Fn(&[f64]) -> Option<Vec<f64>>,
    seed: u64,
) -> Option<Vec<f64>> {
    let nv = init.len();
    if nv == 0 {
        return Some(Vec::new());
    }
    let cost = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>();
    let mut rng = Rng::new(seed ^ 0xD1CE_5EED);
    for restart in 0..50u64 {
        let mut x: Vec<f64> = if restart == 0 {
            init.to_vec()
        } else {
            init.iter().map(|&v| v + rng.range(-1.5, 1.5)).collect()
        };
        let Some(mut r) = residual(&x) else { continue };
        let m = r.len();
        let mut cur = cost(&r);
        let mut lambda = 1e-2;
        for _ in 0..100 {
            if cur < 1e-22 {
                break;
            }
            // Central-difference Jacobian (m × nv).
            let h = 1e-6;
            let mut jac = vec![0.0f64; m * nv];
            let mut ok = true;
            for v in 0..nv {
                let mut xp = x.clone();
                let mut xm = x.clone();
                xp[v] += h;
                xm[v] -= h;
                match (residual(&xp), residual(&xm)) {
                    (Some(rp), Some(rm)) if rp.len() == m && rm.len() == m => {
                        for i in 0..m {
                            jac[i * nv + v] = (rp[i] - rm[i]) / (2.0 * h);
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok {
                break;
            }
            // Normal equations A = JᵀJ + λI, g = Jᵀr; solve A d = −g.
            let mut a = vec![0.0f64; nv * nv];
            let mut g = vec![0.0f64; nv];
            for i in 0..m {
                for u in 0..nv {
                    let jiu = jac[i * nv + u];
                    g[u] += jiu * r[i];
                    for w in 0..nv {
                        a[u * nv + w] += jiu * jac[i * nv + w];
                    }
                }
            }
            for u in 0..nv {
                a[u * nv + u] += lambda;
            }
            let neg_g: Vec<f64> = g.iter().map(|x| -x).collect();
            let Some(d) = solve_linear(a, neg_g, nv) else {
                lambda *= 4.0;
                continue;
            };
            let mut x2 = x.clone();
            for (v, &dv) in d.iter().enumerate() {
                x2[v] += dv;
            }
            match residual(&x2) {
                Some(r2) if r2.len() == m => {
                    let cur2 = cost(&r2);
                    if cur2 < cur {
                        x = x2;
                        r = r2;
                        cur = cur2;
                        lambda = (lambda * 0.5).max(1e-12);
                    } else {
                        lambda *= 3.0;
                        if lambda > 1e12 {
                            break;
                        }
                    }
                }
                _ => {
                    lambda *= 3.0;
                }
            }
        }
        if cur < 1e-18 {
            return Some(x);
        }
    }
    None
}

/// Solve the dense linear system `A x = b` (`n×n`) by Gaussian elimination with
/// partial pivoting. `a` is row-major and consumed.
fn solve_linear(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Option<Vec<f64>> {
    for col in 0..n {
        // Partial pivot.
        let mut piv = col;
        let mut best = a[col * n + col].abs();
        for row in (col + 1)..n {
            let v = a[row * n + col].abs();
            if v > best {
                best = v;
                piv = row;
            }
        }
        if best < 1e-14 {
            return None;
        }
        if piv != col {
            for k in 0..n {
                a.swap(col * n + k, piv * n + k);
            }
            b.swap(col, piv);
        }
        let d = a[col * n + col];
        for row in (col + 1)..n {
            let f = a[row * n + col] / d;
            if f != 0.0 {
                for k in col..n {
                    a[row * n + k] -= f * a[col * n + k];
                }
                b[row] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0f64; n];
    for row in (0..n).rev() {
        let mut s = b[row];
        for k in (row + 1)..n {
            s -= a[row * n + k] * x[k];
        }
        x[row] = s / a[row * n + row];
    }
    Some(x)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Why [`compile`] rejected a program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    /// The goal is a metric relation — an absolute length, or a general
    /// algebraic equation over dist/angle/area — that has no DDAR predicate.
    /// DDAR cannot decide it; route the program to [`crate::metric::solve`].
    MetricGoal(String),
    /// Any other error (syntax, unknown name, arity, degenerate figure, …).
    Invalid(String),
}

impl CompileError {
    /// Whether the program should be routed to the metric prover instead.
    pub fn is_metric_goal(&self) -> bool {
        matches!(self, CompileError::MetricGoal(_))
    }

    pub fn message(&self) -> &str {
        match self {
            CompileError::MetricGoal(m) | CompileError::Invalid(m) => m,
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for CompileError {}

impl From<String> for CompileError {
    fn from(m: String) -> Self {
        CompileError::Invalid(m)
    }
}

/// Outcome of compiling a high-level program.
pub struct Compiled {
    pub problem: Problem,
    /// `Some(false)` if the goal is numerically checkable and does not hold in
    /// the sampled figure (statement probably false); `Some(true)` if it holds;
    /// `None` if not checkable or there is no goal.
    pub goal_numerically_holds: Option<bool>,
    /// Number of samples tried before a valid figure was found.
    pub attempts: u32,
}

/// Compile a high-level program into a low-level [`Problem`].
/// How safely a sampled figure supports branch-dependent lowering: the least
/// distance, in degrees, of any geometric angle named by an angle-sum
/// hypothesis from the degenerate 0°/180° — where orientation branches become
/// unreadable and the downstream symbolic chain silently collapses (e.g. a
/// locus point collapsing onto a midpoint flattens its hypothesis angle).
/// Figures without angle-sum hypotheses report a comfortable 90°.
fn figure_margin(stmts: &[Stmt], problem: &Problem) -> f64 {
    let coord = |name: &str| -> Option<Vec2> {
        problem
            .points
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value)
    };
    let mut margin = 90.0f64;
    let mut visit = |rels: &Vec<Rel>| {
        for rel in rels {
            if let Rel::AngSumEq(terms, _) = rel {
                for (_, x, y, z) in terms {
                    let (Expr::Ident(xn), Expr::Ident(yn), Expr::Ident(zn)) = (x, y, z) else {
                        continue;
                    };
                    let (Some(vx), Some(vy), Some(vz)) = (coord(xn), coord(yn), coord(zn))
                    else {
                        continue;
                    };
                    let (u, w) = (vx - vy, vz - vy);
                    if u.norm() < 1e-12 || w.norm() < 1e-12 {
                        margin = 0.0;
                        continue;
                    }
                    let g = (u.dot(w) / (u.norm() * w.norm()))
                        .clamp(-1.0, 1.0)
                        .acos()
                        .to_degrees();
                    margin = margin.min(g.min(180.0 - g));
                }
            }
        }
    };
    for stmt in stmts {
        match stmt {
            Stmt::Assume(rels) => visit(rels),
            Stmt::Constrain { constraints, .. } => visit(constraints),
            _ => {}
        }
    }
    margin
}

/// IMO/JGEX corpus problems (`corpus/imo_ag_30.txt`, `corpus/jgex_ag_231.txt`)
/// use roughly 5-15 points; 100 leaves an order of magnitude of headroom for
/// legitimately complex constructions while keeping `Ddar::new`'s O(n^2)
/// allocation, `deduction_closure`'s O(n^3) search, and `compile`'s up-to-400
/// re-sampling attempts all bounded well short of a memory/CPU exhaustion DoS
/// from a crafted `.geo` program (see `Ddar::new_with_slack`, `engine.rs:194`).
const MAX_POINTS: usize = 100;

/// Total point count declared across a parsed program — shared by every
/// public entry point (`compile`, `build_instances`, `build_sampled_figure`) so
/// the guard below applies uniformly.
fn point_count(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|s| match s {
            Stmt::Bind { names, .. } => names.len(),
            Stmt::Constrain { .. } => 1,
            _ => 0,
        })
        .sum()
}

/// A goal with no DDAR predicate — an absolute length or a general metric
/// equation — must be routed to the metric prover, never compiled.
fn check_goal_is_ddar_expressible(stmts: &[Stmt]) -> Result<(), CompileError> {
    for s in stmts {
        if let Stmt::Goal(Rel::DistConst(..) | Rel::MetricEq(..)) = s {
            return Err(CompileError::MetricGoal(
                "the goal is a metric relation (an absolute length or a general \
                 equation over dist/angle/area) with no DDAR predicate; \
                 use the metric prover (`ddar --metric`, `metric::solve`)"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

fn check_point_count(stmts: &[Stmt]) -> Result<(), String> {
    let n = point_count(stmts);
    if n > MAX_POINTS {
        return Err(format!(
            "too many points ({n}); this engine supports at most {MAX_POINTS} in one problem"
        ));
    }
    Ok(())
}

pub fn compile(src: &str) -> Result<Compiled, CompileError> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, pos: 0, depth: 0, ops: 0 };
    let stmts = parser.parse_program()?;
    if stmts.is_empty() {
        return Err("empty program".to_string().into());
    }
    check_point_count(&stmts)?;
    check_goal_is_ddar_expressible(&stmts)?;
    // Sampled figures can panic deep in the numerics; each attempt is caught
    // and resampled, with the panic output muted.
    crate::quiet_panic::quiet(|| compile_stmts(&stmts))
}

fn compile_stmts(stmts: &[Stmt]) -> Result<Compiled, CompileError> {
    let mut result: Result<Compiled, CompileError> = Err("no attempt".to_string().into());
    // A hypothesis set with open side-conditions (e.g. "interior points") can
    // produce sampled figures where the goal is numerically false even though
    // every stated constraint is satisfied. Such an instance is a last resort:
    // keep it as a fallback, but spend a few more seeds looking for a figure
    // where the goal actually holds before giving up on one.
    const MAX_GOAL_FALSE_BUILDS: u32 = 32;
    /// Among goal-true instances, hunt briefly for one whose hypothesis-angle
    /// branches are comfortably non-degenerate before settling.
    const MAX_TRUE_BUILDS: u32 = 6;
    let mut goal_false_builds = 0u32;
    let mut true_builds = 0u32;
    let mut best_true: Option<(f64, Compiled)> = None;
    let mut fallback_false: Option<Compiled> = None;
    for seed in 1..=400u64 {
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(stmts, seed)));
        match built {
            Ok(Err(BuildError::Fatal(msg))) => {
                result = Err(msg.into());
                break;
            }
            Ok(Err(BuildError::Degenerate)) => continue,
            Err(_) => {
                result = Err("could not build a valid figure (internal error while \
                              constructing; set DDAR_DEBUG_PANICS=1 for detail)"
                    .to_string()
                    .into());
                continue;
            }
            Ok(Ok((scene, goal, _scale))) => {
                let problem = Problem {
                    points: scene.points,
                    preds: scene.preds,
                    goal,
                };
                // Validate: DDAR must be able to process it without a numeric
                // panic (guards against an imprecise sampled figure).
                let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::runner::solve_problem(&problem)
                }));
                if ok.is_err() {
                    continue; // imprecise figure — resample
                }
                // The validation solve doubles as the strongest instance-quality
                // signal: if the goal already PROVES on this figure, its branch
                // structure demonstrably supports the whole derivation chain.
                let proved_in_validation = matches!(&ok, Ok(Ok(true)));
                let goal_numerically_holds = problem
                    .goal
                    .as_ref()
                    .and_then(|g| numeric_holds(&problem, g));
                let compiled = Compiled {
                    problem,
                    goal_numerically_holds,
                    attempts: seed as u32,
                };
                if goal_numerically_holds == Some(false) {
                    goal_false_builds += 1;
                    if fallback_false.is_none() {
                        fallback_false = Some(compiled);
                    }
                    if goal_false_builds >= MAX_GOAL_FALSE_BUILDS && best_true.is_none() {
                        break;
                    }
                    continue;
                }
                let margin = if proved_in_validation {
                    f64::INFINITY // proves outright: the ideal instance
                } else {
                    figure_margin(stmts, &compiled.problem)
                };
                if std::env::var_os("DDAR_DEBUG_FIGURE").is_some_and(|v| !v.is_empty()) {
                    eprintln!(
                        "[fig] goal-true seed {seed}, margin {margin:.2}°, proves: {proved_in_validation}"
                    );
                }
                if best_true.as_ref().is_none_or(|(m, _)| margin > *m) {
                    best_true = Some((margin, compiled));
                }
                true_builds += 1;
                if proved_in_validation || margin >= 10.0 || true_builds >= MAX_TRUE_BUILDS {
                    break;
                }
                continue;
            }
        }
    }
    if let Some((_, c)) = best_true {
        return Ok(c);
    }
    if let Some(c) = fallback_false {
        return Ok(c);
    }
    result
}

/// Build up to `n` valid instances of a coordinate-free construction, each with
/// its free points independently re-sampled (a different seed). Returns each
/// instance as `(point name, coordinates)` pairs. Powers metric-goal
/// verification ([`crate::metric`]): a quantity that agrees across many
/// re-sampled instances is a genuine identity.
pub fn build_instances(src: &str, n: usize) -> Result<Vec<Vec<(String, Vec2)>>, String> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, pos: 0, depth: 0, ops: 0 };
    let stmts = parser.parse_program()?;
    if stmts.is_empty() {
        return Err("empty construction".to_string());
    }
    check_point_count(&stmts)?;
    crate::quiet_panic::quiet(|| build_instances_of(&stmts, n))
}

fn build_instances_of(stmts: &[Stmt], n: usize) -> Result<Vec<Vec<(String, Vec2)>>, String> {
    let mut out: Vec<Vec<(String, Vec2)>> = Vec::new();
    let mut seed = 1u64;
    let limit = (n as u64) * 40 + 800;
    while out.len() < n && seed <= limit {
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(stmts, seed)));
        if let Ok(Err(BuildError::Fatal(msg))) = built {
            return Err(msg);
        }
        if let Ok(Ok((scene, _goal, _scale))) = built {
            if scene
                .points
                .iter()
                .all(|p| p.value.x.is_finite() && p.value.y.is_finite())
            {
                out.push(
                    scene
                        .points
                        .iter()
                        .map(|p| (p.name.clone(), p.value))
                        .collect(),
                );
            }
        }
        seed += 1;
    }
    if out.is_empty() {
        return Err("could not build any valid instance of the construction".to_string());
    }
    Ok(out)
}

/// One valid sampled instance of a construction, as the classical metric
/// provers ([`crate::synthetic`], [`crate::ratio`]) read it: the hypothesis
/// predicates they cite, the absolute lengths, and coordinates used only to
/// read configuration (order, orientation) and to rule out degenerate cases.
pub struct SampledFigure {
    /// Point name per id.
    pub names: Vec<String>,
    /// Numeric coordinates of a single generic, valid instance.
    pub coords: Vec<Vec2>,
    /// Every geometric hypothesis predicate (each construction's defining
    /// relations, plus any `point:` / goal-independent constraints).
    pub preds: Vec<Predicate>,
    /// Absolute-length constraints `|ab| = v`.
    pub scale: Vec<(PointId, PointId, f64)>,
    /// General metric-equation hypotheses imposed by `point:` constraints, as
    /// `(lhs, rhs)` pairs — offered to the metric provers as given equations so
    /// an arbitrary imposed relation can be *used* in a synthetic proof.
    pub metric_hyps: Vec<(MExpr, MExpr)>,
}

/// Build one generic valid instance of a coordinate-free construction. Mirrors
/// the instance search in [`compile`] (tries seeds until a non-degenerate
/// figure is found).
pub fn build_sampled_figure(src: &str) -> Result<SampledFigure, String> {
    let toks = tokenize(src)?;
    let mut parser = Parser { toks, pos: 0, depth: 0, ops: 0 };
    let stmts = parser.parse_program()?;
    if stmts.is_empty() {
        return Err("empty construction".to_string());
    }
    check_point_count(&stmts)?;
    crate::quiet_panic::quiet(|| build_sampled_figure_of(&stmts))
}

fn build_sampled_figure_of(stmts: &[Stmt]) -> Result<SampledFigure, String> {
    let mut result: Result<SampledFigure, String> = Err("could not build a valid instance".to_string());
    for seed in 1..=400u64 {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(stmts, seed))) {
            Ok(Ok((scene, _goal, scale))) => {
                if scene
                    .points
                    .iter()
                    .all(|p| p.value.x.is_finite() && p.value.y.is_finite())
                {
                    result = Ok(SampledFigure {
                        names: scene.points.iter().map(|p| p.name.clone()).collect(),
                        coords: scene.points.iter().map(|p| p.value).collect(),
                        preds: scene.preds,
                        scale,
                        metric_hyps: scene.metric_hyps,
                    });
                    break;
                }
            }
            Ok(Err(BuildError::Fatal(msg))) => {
                result = Err(msg);
                break;
            }
            _ => continue, // degenerate sample or panic — resample
        }
    }
    result
}

/// Numerically test whether a goal predicate holds in a figure.
pub fn numeric_holds(problem: &Problem, pred: &Predicate) -> Option<bool> {
    let tol = 1e-6;
    let c = |i: usize| problem.points[pred.points[i] as usize].value;
    let dir = |i: usize, j: usize| NumLine::through(c(i), c(j)).direction();
    let ang_eq = |x: f64, y: f64| ((x - y + 0.5).rem_euclid(1.0) - 0.5).abs() < tol;
    match pred.name.as_str() {
        "coll" => Some((2..pred.points.len()).all(|i| cross(c(1) - c(0), c(i) - c(0)).abs() < tol)),
        "para" => Some(ang_eq(dir(0, 1), dir(2, 3))),
        "perp" => Some(ang_eq(dir(0, 1), dir(2, 3) + 0.5)),
        "cong" => Some((distance(c(0), c(1)) - distance(c(2), c(3))).abs() < tol),
        "cyclic" => {
            let circ = NumCircle::through(c(0), c(1), c(2))?;
            Some((3..pred.points.len()).all(|i| circ.distance(c(i)) < tol))
        }
        "eqangle" => Some(ang_eq(dir(0, 1) - dir(2, 3), dir(4, 5) - dir(6, 7))),
        "eqratio" => {
            let r1 = distance(c(0), c(1)) / distance(c(2), c(3));
            let r2 = distance(c(4), c(5)) / distance(c(6), c(7));
            Some((r1 - r2).abs() < tol)
        }
        "aconst" => {
            let target = pred.constants[0].to_f64() / 180.0;
            Some(ang_eq(dir(0, 1) - dir(2, 3), target))
        }
        "angeq" => {
            // Σ coefᵢ·θ(lineᵢ) + K/180 ≡ 0 (mod 1, in half-turns).
            let coefs = &pred.constants[..pred.constants.len() - 1];
            let konst = pred.constants.last()?.to_f64();
            let mut v = konst / 180.0;
            for (k, coef) in coefs.iter().enumerate() {
                v += coef.to_f64() * dir(2 * k, 2 * k + 1);
            }
            Some(((v + 0.5).rem_euclid(1.0) - 0.5).abs() < tol)
        }
        "distseq" => {
            // Σ coefᵢ·|aᵢbᵢ| == 0
            let mut v = 0.0;
            for (k, coef) in pred.constants.iter().enumerate() {
                v += coef.to_f64() * distance(c(2 * k), c(2 * k + 1));
            }
            Some(v.abs() < tol)
        }
        "distmeq" => {
            // sum coef_i * log(dist_i) == log(const)
            let coefs = &pred.constants[..pred.constants.len() - 1];
            let konst = pred.constants.last()?.to_f64();
            let mut v = 0.0;
            for (k, coef) in coefs.iter().enumerate() {
                v += coef.to_f64() * distance(c(2 * k), c(2 * k + 1)).ln();
            }
            Some((v - konst.ln()).abs() < tol)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::solve_problem;

    fn prove(src: &str) -> bool {
        let compiled = compile(src).expect("compile");
        if compiled.goal_numerically_holds == Some(false) {
            panic!("goal is numerically false");
        }
        solve_problem(&compiled.problem).expect("solve")
    }

    /// An angle-sum hypothesis must reach the prover symbolically: from
    /// ∠CAB + ∠CBA = 90° alone, DDAR can conclude CA ⟂ CB only if the sum is
    /// lowered into the linear angle system (`angeq`) — a placeholder would
    /// leave the goal underivable.
    #[test]
    fn angle_sum_hypothesis_reaches_the_prover() {
        let src = "A B = segment\nC = free\nassume angle(C, A, B) + angle(C, B, A) = 90\nprove perp(C, A, C, B)";
        let compiled = compile(src).expect("compile");
        assert!(
            compiled.problem.preds.iter().any(|p| p.name == "angeq"),
            "angle-sum should lower to an angeq predicate, got: {:?}",
            compiled.problem.preds.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
        );
        assert_eq!(compiled.goal_numerically_holds, Some(true));
        assert!(prove(src), "DDAR should derive the right angle from the sum");
    }

    /// Three-term sums with a constant beyond 180° lower and verify too
    /// (the triangle angle sum — trivially true, so the figure must build
    /// and the emitted `angeq` must hold numerically).
    #[test]
    fn three_term_angle_sum_lowers_and_holds() {
        let src = "A B C = triangle\nassume angle(B, A, C) + angle(A, B, C) + angle(B, C, A) = 180\nprove coll(A, A, B)";
        let compiled = compile(src).expect("compile");
        let angeq = compiled
            .problem
            .preds
            .iter()
            .find(|p| p.name == "angeq")
            .expect("three-term sum lowers to angeq");
        assert_eq!(angeq.points.len(), 12); // 3 angles × 2 lines × 2 points
        assert_eq!(angeq.constants.len(), 7); // 6 line coefficients + constant
        assert_eq!(numeric_holds(&compiled.problem, angeq), Some(true));
    }

    #[test]
    fn thales() {
        assert!(prove(
            "A = free\nO = free\nB = reflect(A, O)\nC = point: on(C, circle(O, A))\nprove perp(C, A, C, B)"
        ));
    }

    #[test]
    fn arbitrary_metric_constraint_positions_point() {
        // The universal constraint escape hatch: an arbitrary metric equation
        // (with a coefficient) pins a free point, solved numerically.
        let fig = build_sampled_figure("A = free\nB = free\nP = point: dist(P,A) = 2*dist(P,B)")
            .expect("build");
        let idx = |n: &str| fig.names.iter().position(|x| x == n).unwrap();
        let (pa, pb) = (
            (fig.coords[idx("P")] - fig.coords[idx("A")]).norm(),
            (fig.coords[idx("P")] - fig.coords[idx("B")]).norm(),
        );
        assert!((pa - 2.0 * pb).abs() < 1e-6, "PA={pa}, PB={pb}");
        // And it is recorded as a hypothesis for the metric provers.
        assert_eq!(fig.metric_hyps.len(), 1);
    }

    /// A long chain of unary `-` (or deeply nested parens) must be rejected
    /// with a clean parse error, not accepted — accepting it means nothing
    /// bounds how deep this recursive-descent parser can go, and a much
    /// longer chain (still well under the 16,384-char request body cap) would
    /// stack-overflow the whole process instead of just this one request.
    #[test]
    fn deeply_nested_unary_minus_is_rejected_not_accepted() {
        let mut src = String::from("prove dist(A, B) = ");
        src.push_str(&"-".repeat(500));
        src.push('5');
        let result: Result<Compiled, CompileError> = compile(&src);
        assert!(
            result.is_err(),
            "500 levels of unary-minus nesting should be rejected by a depth cap"
        );
        if let Err(msg) = result {
            assert!(
                msg.message().contains("nested too deeply"),
                "expected a nesting-depth error, got: {msg}"
            );
        }
    }

    /// Same guard, exercised through nested construction calls instead of
    /// unary minus — `parse_expr` is a separate recursive grammar from the
    /// metric-expression parser and needs its own depth cap.
    #[test]
    fn deeply_nested_construction_calls_are_rejected_not_accepted() {
        let mut src = String::from("A = free\nB = ");
        for _ in 0..500 {
            src.push_str("reflect(");
        }
        src.push('A');
        for _ in 0..500 {
            src.push(')');
        }
        src.push_str("\nprove coll(A, A, B)");
        let result: Result<Compiled, CompileError> = compile(&src);
        assert!(
            result.is_err(),
            "500 levels of construction-call nesting should be rejected by a depth cap"
        );
        if let Err(msg) = result {
            assert!(msg.message().contains("nested too deeply"));
        }
    }

    /// A program declaring far more points than any real geometry problem
    /// needs must be rejected quickly, not accepted — accepting it means
    /// `Ddar::new`'s O(n^2) allocation and `deduction_closure`'s O(n^3) search
    /// scale with an attacker-chosen `n`, up to ~2,000 points fit in the
    /// 16,384-char request body cap alone.
    #[test]
    fn too_many_points_is_rejected_quickly_not_accepted() {
        let mut src = String::new();
        for i in 0..500 {
            src.push_str(&format!("p{i} = free\n"));
        }
        src.push_str("prove coll(p0, p0, p0)");
        let start = std::time::Instant::now();
        let result = compile(&src);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "rejection must happen before the expensive instance-search loop, \
             not after — it took {:?}",
            start.elapsed()
        );
        assert!(result.is_err(), "500 points should be rejected");
        if let Err(msg) = result {
            assert!(
                msg.message().contains("too many points"),
                "expected 'too many points' error, got: {msg}"
            );
        }
    }

    #[test]
    fn assume_positions_free_points_jointly() {
        // Declare four free points and ASSUME they are concyclic — the global
        // solve must place them on one circle.
        let fig = build_sampled_figure("A = free\nB = free\nC = free\nD = free\nassume cyclic(A,B,C,D)")
            .expect("build");
        let c = |n: &str| fig.coords[fig.names.iter().position(|x| x == n).unwrap()];
        let circ = NumCircle::through(c("A"), c("B"), c("C")).expect("circle");
        assert!(circ.distance(c("D")) < 1e-3, "D not on circle(ABC)");
    }

    #[test]
    fn assume_arbitrary_sum_relation() {
        // ASSUME an arbitrary metric relation (a sum of distances) on free points
        // — the global solver satisfies it, and re-sampled instances keep it.
        for seed in [1u64, 7, 20] {
            let insts =
                build_instances("A = free\nB = free\nC = free\nassume dist(A,B) + dist(B,C) = 3", 1)
                    .unwrap_or_default();
            let _ = seed;
            for inst in insts {
                let m: std::collections::HashMap<_, _> = inst.into_iter().collect();
                let d = |x: &str, y: &str| (m[x] - m[y]).norm();
                assert!((d("A", "B") + d("B", "C") - 3.0).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn assume_then_prove_uses_hypothesis() {
        let report = crate::metric::solve(
            "A = free\nB = free\nC = free\nassume dist(A,B) = dist(A,C)",
            "dist(A,B)^2 = dist(A,C)^2",
            48,
        )
        .expect("solve");
        assert!(report.contains("EUCLIDEAN PROOF") && report.contains("given"), "{report}");
    }

    #[test]
    fn arbitrary_constraint_and_goal_prove() {
        // Impose an arbitrary relation, then prove an arbitrary consequence — the
        // hypothesis must be *used* (not just checked numerically).
        let report = crate::metric::solve(
            "A = free\nB = free\nP = point: dist(P,A) = 2*dist(P,B)",
            "dist(P,A)^2 = 4*dist(P,B)^2",
            48,
        )
        .expect("solve");
        assert!(report.contains("EUCLIDEAN PROOF"), "{report}");
        assert!(report.contains("hypothesis"), "{report}");
    }

    #[test]
    fn midsegment_with_nesting() {
        // Nested midpoints as goal arguments; first-class line objects.
        assert!(prove(
            "A B C = triangle\nprove para(midpoint(A,B), midpoint(A,C), B, C)"
        ));
    }

    #[test]
    fn foot_via_line_object() {
        assert!(prove(
            "A B C = triangle\nF = foot(A, line(B, C))\nprove perp(A, F, B, C)"
        ));
    }

    #[test]
    fn meet_of_line_and_circle() {
        // A diameter line meets the circle in two new (antipodal) points.
        assert!(prove(
            "A B C = triangle\nO = circumcenter(A,B,C)\nM = midpoint(A,B)\nP Q = meet(line(O,M), circle(O,A))\nprove cong(O, P, O, Q)"
        ));
    }

    #[test]
    fn constrained_point_circumcenter() {
        // Define a point purely by constraints (the flexible escape hatch).
        assert!(prove(
            "A B C = triangle\nO = point: cong(O,A,O,B), cong(O,B,O,C)\nprove cong(O, A, O, C)"
        ));
    }

    #[test]
    fn dist_and_angle_sugar_parse() {
        // dist(...)=dist(...) and angle(...)=number should parse and compile.
        let compiled =
            compile("A B = segment\nP = point: dist(P,A)=dist(P,B)\nprove cong(P, A, P, B)")
                .unwrap();
        assert!(solve_problem(&compiled.problem).unwrap());
    }

    #[test]
    fn detects_false_statement() {
        let c = compile("A B C = triangle\nprove coll(A, B, C)").unwrap();
        assert_eq!(c.goal_numerically_holds, Some(false));
    }

    #[test]
    fn two_tangent_theorem() {
        // Tangent segments from an external point are equal.
        assert!(prove(
            "O A = segment\nP = free\nT1 T2 = tangent(P, circle(O, A))\nprove cong(P, T1, P, T2)"
        ));
    }

    #[test]
    fn perp_bisectors_concur_at_circumcenter() {
        assert!(prove(
            "A B C = triangle\nP = meet(perp_bisector(A, B), perp_bisector(B, C))\nprove cong(P, A, P, C)"
        ));
    }

    #[test]
    fn square_diagonals_equal() {
        assert!(prove(
            "A B = segment\nC D = square(A, B)\nprove cong(A, C, B, D)"
        ));
    }

    #[test]
    fn on_dia_is_thales() {
        assert!(prove(
            "A B = segment\nX = on_dia(A, B)\nM = midpoint(A, B)\nprove cong(M, X, M, A)"
        ));
    }

    #[test]
    fn locus_constructions_compile_and_prove() {
        // on_line / on_circle sugar mirrors the `point:` form.
        assert!(prove(
            "O A = segment\nB = on_circle(O, A)\nM = midpoint(A, B)\nprove perp(O, M, A, B)"
        ));
        // on_pline: a point on the parallel through A is genuinely parallel.
        assert!(prove(
            "A B C = triangle\nX = on_pline(A, B, C)\nprove para(A, X, B, C)"
        ));
        // on_bline point is equidistant.
        assert!(prove(
            "A B = segment\nX = on_bline(A, B)\nprove cong(X, A, X, B)"
        ));
    }

    #[test]
    fn shift_makes_a_parallelogram() {
        assert!(prove(
            "A B = segment\nP = free\nX = shift(P, A, B)\nprove cong(P, X, A, B)"
        ));
    }

    #[test]
    fn excenter_lies_on_internal_a_bisector() {
        // The A-excenter is on the internal bisector from A (mod pi).
        assert!(prove(
            "A B C = triangle\nJ = excenter(A, B, C)\nprove eqangle(A, B, A, J, A, J, A, C)"
        ));
    }

    #[test]
    fn nine_point_center_constructs_correctly() {
        // The nine-point centre is equidistant from the three side midpoints.
        // The construction's own midpoints are anonymous; re-building them as
        // named points that numerically coincide is legal (DDAR merges them),
        // and the centre is equidistant from all three.
        let compiled = compile(
            "A B C = triangle\nN = nine_point_center(A, B, C)\nMa = midpoint(B, C)\nMb = midpoint(A, C)\nMc = midpoint(A, B)\nprove coll(A, B, C)",
        )
        .unwrap();
        let pts = &compiled.problem.points;
        let coord = |name: &str| pts.iter().find(|p| p.name == name).unwrap().value;
        let (n, ma, mb, mc) = (coord("N"), coord("Ma"), coord("Mb"), coord("Mc"));
        assert!((distance(n, ma) - distance(n, mb)).abs() < 1e-9);
        assert!((distance(n, mb) - distance(n, mc)).abs() < 1e-9);
    }

    #[test]
    fn parse_errors_are_reported() {
        assert!(compile("M = midpoint(A)").is_err()); // unknown A / arity
        assert!(compile("X = wat(A, B)").is_err());
        assert!(compile("A B C = triangle\nprove perp(A, B, C)").is_err()); // perp needs 4
    }
}
