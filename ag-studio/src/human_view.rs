use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::i18n::{self, Lang};
use crate::render;
use ddar::human as eng;
use ddar::predicate::PointId;
use ddar::proof::FactId;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanView {
    pub version: u16,
    #[serde(default)]
    pub as_drawn: bool,
    #[serde(default)]
    pub setup: Vec<SetupLine>,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub metrics: Metrics,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Metrics {
    pub raw_steps: usize,
    pub derived: usize,
    pub blocks: usize,
    pub claims: usize,
    pub sentences: usize,
    pub chain_links: usize,
    pub chains: usize,
    pub pure_chains: usize,
    pub pooled: usize,
    pub fallback_blocks: usize,
    pub words_en: usize,
    pub lines_en: usize,
    pub citations: usize,
    pub aux_shown: usize,
    pub human_cost: usize,
    pub reproved: usize,
    pub silent: usize,
    pub pruned: usize,
    pub represented: usize,
    pub theorem: usize,
    pub fallback_facts: usize,
    pub check_violations: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupLine {
    DirectedAngles,
    Circle {
        #[serde(default)]
        name: String,
        #[serde(default)]
        through: Vec<String>,
        #[serde(default)]
        centre: Option<String>,
        #[serde(default)]
        diameter: Option<[String; 2]>,
    },
    Aux {
        point: String,
        aux_index: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wording: Option<AuxWording>,
    },
    Helper {
        point: String,
        meaning: HelperMeaning,
    },
    Notation {
        triangle: [String; 3],
        #[serde(default)]
        circumcentre: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuxWording {
    pub key: String,
    #[serde(default)]
    pub args: Vec<AuxArg>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AuxArg {
    pub key: String,
    #[serde(default)]
    pub pts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HelperMeaning {
    Midpoint { of: [String; 2] },
    Reflection { of: String, line: [String; 2] },
    Perp { through: String, to: [String; 2] },
    Para { through: String, to: [String; 2] },
    Point,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub points: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Claim,
    Step,
    Conclusion,
    Raw,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Block {
    pub id: u16,
    pub kind: BlockKind,
    #[serde(default)]
    pub n: Option<u16>,
    pub stmt: Stmt,
    #[serde(default)]
    pub body: Vec<Sentence>,
    #[serde(default)]
    pub engine_steps: Vec<usize>,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default)]
    pub objects: Vec<ObjRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ObjRef {
    Circle(Vec<String>),
    Line(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompKind {
    Ratio,
    Length,
    Trig,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sentence {
    Chain {
        terms: Vec<String>,
        links: Vec<Link>,
        #[serde(default)]
        then: Option<Stmt>,
        directed: bool,
    },
    Because {
        stmt: Stmt,
        #[serde(default)]
        reasons: Vec<Reason>,
        #[serde(default)]
        combination: Vec<Term>,
    },
    Pooled {
        stmt: Stmt,
        #[serde(default)]
        reasons: Vec<Reason>,
        #[serde(default)]
        combination: Vec<Term>,
    },
    Theorem {
        key: TheoremKey,
        stmt: Stmt,
        #[serde(default)]
        reasons: Vec<Reason>,
    },
    Computation {
        comp: CompKind,
        terms: Vec<String>,
        links: Vec<Link>,
    },
    Raw {
        step: Option<usize>,
        #[serde(default)]
        cites: Vec<usize>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Link {
    #[serde(default)]
    pub reasons: Vec<Reason>,
    #[serde(default)]
    pub combination: Vec<Term>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Term {
    pub reason: u16,
    pub row: u16,
    pub coef: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reason {
    Hyp { stmt: Stmt, step: Option<usize> },
    Claim { n: u16, block: u16, step: Option<usize> },
    Atom { key: AtomKey, args: Vec<String>, stmt: Stmt, from: Vec<u16>, circle: Option<String> },
    Fact { stmt: Stmt, step: Option<usize>, block: Option<u16>, because: Vec<Reason> },
    Engine { step: Option<usize> },
    Lemma { stmt: Stmt, block: u16, sentence: u16 },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TaggedReason {
    Hyp {
        stmt: Stmt,
        #[serde(default)]
        step: Option<usize>,
    },
    Claim {
        n: u16,
        block: u16,
        #[serde(default)]
        step: Option<usize>,
    },
    Atom {
        key: AtomKey,
        #[serde(default)]
        args: Vec<String>,
        stmt: Stmt,
        #[serde(default)]
        from: Vec<u16>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        circle: Option<String>,
    },
    Engine {
        #[serde(default)]
        step: Option<usize>,
    },
}

#[derive(Serialize, Deserialize)]
struct FactReason {
    #[serde(flatten)]
    stmt: Stmt,
    #[serde(default)]
    step: Option<usize>,
    #[serde(default)]
    block: Option<u16>,
    #[serde(default)]
    because: Vec<Reason>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    lemma: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sentence: Option<u16>,
}

const TAGGED: [&str; 4] = ["hyp", "claim", "atom", "engine"];

impl Serialize for Reason {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.clone() {
            Reason::Hyp { stmt, step } => TaggedReason::Hyp { stmt, step }.serialize(s),
            Reason::Claim { n, block, step } => TaggedReason::Claim { n, block, step }.serialize(s),
            Reason::Atom { key, args, stmt, from, circle } => TaggedReason::Atom { key, args, stmt, from, circle }.serialize(s),
            Reason::Engine { step } => TaggedReason::Engine { step }.serialize(s),
            Reason::Fact { stmt, step, block, because } => FactReason { stmt, step, block, because, lemma: false, sentence: None }.serialize(s),
            Reason::Lemma { stmt, block, sentence } => FactReason { stmt, step: None, block: Some(block), because: vec![], lemma: true, sentence: Some(sentence) }.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Reason {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Reason, D::Error> {
        use serde::de::Error;
        let v = Value::deserialize(d)?;
        let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
        if TAGGED.contains(&kind) {
            Ok(match TaggedReason::deserialize(v).map_err(D::Error::custom)? {
                TaggedReason::Hyp { stmt, step } => Reason::Hyp { stmt, step },
                TaggedReason::Claim { n, block, step } => Reason::Claim { n, block, step },
                TaggedReason::Atom { key, args, stmt, from, circle } => Reason::Atom { key, args, stmt, from, circle },
                TaggedReason::Engine { step } => Reason::Engine { step },
            })
        } else {
            let f = FactReason::deserialize(v).map_err(D::Error::custom)?;
            Ok(match (f.lemma, f.block, f.sentence) {
                (true, Some(block), Some(sentence)) => Reason::Lemma { stmt: f.stmt, block, sentence },
                _ => Reason::Fact { stmt: f.stmt, step: f.step, block: f.block, because: f.because },
            })
        }
    }
}

macro_rules! closed_keys {
    ($name:ident, $eng:ty, { $($v:ident => $s:literal),* $(,)? }) => {
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($v),* }
        impl $name {
            #[cfg_attr(not(test), allow(dead_code))]
            pub const ALL: &'static [$name] = &[$($name::$v),*];
            pub fn key(self) -> &'static str {
                match self { $($name::$v => $s),* }
            }
            fn of(k: $eng) -> $name {
                match k.as_str() {
                    $($s => $name::$v,)*
                    _ => $name::fallback(),
                }
            }
        }
    };
}

closed_keys!(AtomKey, eng::AtomKey, {
    Inscribed => "inscribed",
    Thales => "thales",
    TangentChord => "tangent_chord",
    PerpBisector => "perp_bisector",
    Parallel => "parallel",
    Radii => "radii",
    Isosceles => "isosceles",
    CentralAngle => "central_angle",
    PowerOfPoint => "power_of_point",
    Midline => "midline",
    Orthocentre => "orthocentre",
});

impl AtomKey {
    fn fallback() -> AtomKey {
        AtomKey::Inscribed
    }
}

closed_keys!(TheoremKey, eng::TheoremKey, {
    RadicalAxis => "radical_axis",
    ArcChord => "arc_chord",
    AngleBisectorThm => "angle_bisector_thm",
    AngleBisectorThmConverse => "angle_bisector_thm_converse",
    Intercept => "intercept",
    Homothety => "homothety",
    Monge => "monge",
    Menelaus => "menelaus",
    MenelausConverse => "menelaus_converse",
    CevaConverse => "ceva_converse",
    BisectorConcurrency => "bisector_concurrency",
    TriangleEquality => "triangle_equality",
    Pythagoras => "pythagoras",
    PerpFromSquares => "perp_from_squares",
    SquaresOfRatio => "squares_of_ratio",
    Stewart => "stewart",
    LengthsFromSquares => "lengths_from_squares",
    LawOfSines => "law_of_sines",
    EqualSines => "equal_sines",
    DoubleAngle => "double_angle",
    TripleAngle => "triple_angle",
    SineConst => "sine_const",
    SinesConverse => "sines_converse",
    PointMerge => "point_merge",
    TangentMerge => "tangent_merge",
    Congruence => "congruence",
    Similarity => "similarity",
    Collinear => "collinear",
    Concyclic => "concyclic",
    Other => "other",
});

impl TheoremKey {
    fn fallback() -> TheoremKey {
        TheoremKey::Other
    }
}

pub struct Naming<'a> {
    pub point: &'a dyn Fn(PointId) -> String,
    pub step: &'a dyn Fn(FactId) -> Option<usize>,
}

fn names(nm: &Naming, ps: &[PointId]) -> Vec<String> {
    ps.iter().map(|&p| (nm.point)(p)).collect()
}

fn leaf_stmt(nm: &Naming, s: &eng::Stmt) -> Stmt {
    let namer = eng::view::Namer { name: nm.point, step: nm.step };
    serde_json::from_value(eng::view::stmt(&namer, s)).unwrap_or(Stmt { kind: "formula".into(), args: vec![], points: vec![] })
}

fn leaf_term(nm: &Naming, e: &eng::Expr) -> String {
    let namer = eng::view::Namer { name: nm.point, step: nm.step };
    eng::view::term(&namer, e)
}

fn hyp_stmt(nm: &Naming, s: &eng::Stmt) -> Stmt {
    if let eng::Stmt::EqAngle { lhs: eng::Expr::Angle { a, b, c, .. }, rhs: eng::Expr::Angle { a: a2, b: b2, c: c2, .. } } = s {
        if b == b2 && c == a2 && a != c2 {
            let line = names(nm, &[*b, *c]).concat();
            let angle = format!("\u{2220}{}", names(nm, &[*a, *b, *c2]).concat());
            let mut points = names(nm, &[*a, *b, *c, *c2]);
            points.sort();
            points.dedup();
            return Stmt { kind: "bisects".into(), args: vec![line, angle], points };
        }
    }
    leaf_stmt(nm, s)
}

fn term_of(t: &eng::Term) -> Term {
    Term { reason: t.reason, row: t.row, coef: t.coef.to_string() }
}

fn reason_of(nm: &Naming, r: &eng::Reason) -> Reason {
    match r {
        eng::Reason::Hyp { stmt, fact } => Reason::Hyp { stmt: hyp_stmt(nm, stmt), step: (nm.step)(*fact) },
        eng::Reason::Claim { n, block, fact } => Reason::Claim { n: *n, block: *block, step: (nm.step)(*fact) },
        eng::Reason::Atom { key, stmt, args, from } => Reason::Atom {
            key: AtomKey::of(*key),
            args: names(nm, args),
            stmt: leaf_stmt(nm, stmt),
            from: from.clone(),
            circle: match key {
                eng::AtomKey::Inscribed => eng::text::circle_name(args),
                eng::AtomKey::TangentChord => args.get(4).and_then(|&o| eng::text::circle_by_centre(o)),
                _ => None,
            },
        },
        eng::Reason::Fact { stmt, fact, block, because } => Reason::Fact {
            stmt: leaf_stmt(nm, stmt),
            step: (nm.step)(*fact),
            block: *block,
            because: because.iter().map(|b| reason_of(nm, b)).collect(),
        },
        eng::Reason::Engine { fact } => Reason::Engine { step: (nm.step)(*fact) },
        eng::Reason::Lemma { stmt, block, sentence } => Reason::Lemma { stmt: leaf_stmt(nm, stmt), block: *block, sentence: *sentence },
    }
}

fn reasons_of(nm: &Naming, rs: &[eng::Reason]) -> Vec<Reason> {
    rs.iter().map(|r| reason_of(nm, r)).collect()
}

fn link_of(nm: &Naming, l: &eng::Link) -> Link {
    Link { reasons: reasons_of(nm, &l.reasons), combination: l.combination.iter().map(term_of).collect() }
}

fn sentence_of(nm: &Naming, s: &eng::Sentence) -> Sentence {
    match s {
        eng::Sentence::Chain { terms, links, then, directed } => Sentence::Chain {
            terms: terms.iter().map(|e| leaf_term(nm, e)).collect(),
            links: links.iter().map(|l| link_of(nm, l)).collect(),
            then: then.as_ref().map(|t| leaf_stmt(nm, t)),
            directed: *directed,
        },
        eng::Sentence::Because { stmt, reasons, combination } => Sentence::Because {
            stmt: leaf_stmt(nm, stmt),
            reasons: reasons_of(nm, reasons),
            combination: combination.iter().map(term_of).collect(),
        },
        eng::Sentence::Pooled { stmt, reasons, combination } => Sentence::Pooled {
            stmt: leaf_stmt(nm, stmt),
            reasons: reasons_of(nm, reasons),
            combination: combination.iter().map(term_of).collect(),
        },
        eng::Sentence::Theorem { key, stmt, reasons } => Sentence::Theorem { key: TheoremKey::of(*key), stmt: leaf_stmt(nm, stmt), reasons: reasons_of(nm, reasons) },
        eng::Sentence::Computation { comp, terms, links } => Sentence::Computation {
            comp: match comp {
                eng::CompKind::Ratio => CompKind::Ratio,
                eng::CompKind::Length => CompKind::Length,
                eng::CompKind::Trig => CompKind::Trig,
            },
            terms: terms.iter().map(|e| leaf_term(nm, e)).collect(),
            links: links.iter().map(|l| link_of(nm, l)).collect(),
        },
        eng::Sentence::Raw { engine_fact, cites } => {
            let mut c: Vec<usize> = cites.iter().filter_map(|f| (nm.step)(*f)).collect();
            c.sort_unstable();
            c.dedup();
            Sentence::Raw { step: (nm.step)(*engine_fact), cites: c }
        }
    }
}

fn setup_of(nm: &Naming, s: &eng::SetupLine) -> SetupLine {
    let p = |x: PointId| (nm.point)(x);
    match s {
        eng::SetupLine::DirectedAngles => SetupLine::DirectedAngles,
        eng::SetupLine::Circle { name, through, centre, diameter } => SetupLine::Circle {
            name: name.clone(),
            through: names(nm, through),
            centre: centre.map(p),
            diameter: diameter.map(|d| [p(d.0), p(d.1)]),
        },
        eng::SetupLine::Aux { point, aux_index, wording } => SetupLine::Aux {
            point: p(*point),
            aux_index: *aux_index,
            wording: wording.as_ref().map(|w| AuxWording { key: w.key.to_string(), args: w.args.iter().map(|a| AuxArg { key: a.key.to_string(), pts: names(nm, &a.pts) }).collect() }),
        },
        eng::SetupLine::Notation { triangle, circumcentre } => SetupLine::Notation { triangle: [p(triangle.0), p(triangle.1), p(triangle.2)], circumcentre: circumcentre.map(p) },
        eng::SetupLine::Helper { point, meaning } => SetupLine::Helper {
            point: p(*point),
            meaning: match meaning {
                eng::HelperMeaning::Midpoint { of } => HelperMeaning::Midpoint { of: [p(of.0), p(of.1)] },
                eng::HelperMeaning::Reflection { of, line } => HelperMeaning::Reflection { of: p(*of), line: [p(line.0), p(line.1)] },
                eng::HelperMeaning::Perp { through, to } => HelperMeaning::Perp { through: p(*through), to: [p(to.0), p(to.1)] },
                eng::HelperMeaning::Para { through, to } => HelperMeaning::Para { through: p(*through), to: [p(to.0), p(to.1)] },
                eng::HelperMeaning::Point => HelperMeaning::Point,
            },
        },
    }
}

fn block_of(nm: &Naming, b: &eng::Block) -> Block {
    let (kind, n) = match b.kind {
        eng::BlockKind::Claim(n) => (BlockKind::Claim, Some(n)),
        eng::BlockKind::Step => (BlockKind::Step, None),
        eng::BlockKind::Conclusion => (BlockKind::Conclusion, None),
        eng::BlockKind::Raw => (BlockKind::Raw, None),
    };
    let mut steps: Vec<usize> = b.engine_facts.iter().filter_map(|&f| (nm.step)(f)).collect();
    steps.sort_unstable();
    steps.dedup();
    let mut points = names(nm, &b.points);
    dedup_keep(&mut points);
    Block {
        id: b.id,
        kind,
        n,
        stmt: leaf_stmt(nm, &b.stmt),
        body: b.body.iter().map(|s| sentence_of(nm, s)).collect(),
        engine_steps: steps,
        points,
        objects: b
            .objects
            .iter()
            .map(|o| match o {
                eng::ObjRef::Circle { through } => ObjRef::Circle(names(nm, through)),
                eng::ObjRef::Line { through } => ObjRef::Line(names(nm, through)),
            })
            .collect(),
    }
}

fn dedup_keep(v: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::new();
    v.retain(|x| {
        let new = !seen.contains(x);
        if new {
            seen.push(x.clone());
        }
        new
    });
}

pub fn from_engine(hp: &eng::HumanProof, nm: &Naming) -> Option<HumanView> {
    if !hp.available || hp.blocks.last().is_none_or(|b| b.kind != eng::BlockKind::Conclusion) {
        return None;
    }
    eng::text::with_notation(&hp.setup, || {
        Some(HumanView {
            version: hp.version,
            as_drawn: hp.as_drawn,
            setup: hp.setup.iter().map(|s| setup_of(nm, s)).collect(),
            blocks: hp.blocks.iter().map(|b| block_of(nm, b)).collect(),
            metrics: serde_json::to_value(&hp.metrics).ok().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default(),
        })
    })
}

impl HumanView {
    pub fn math_names(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.setup {
            match s {
                SetupLine::Circle { name, .. } if !name.is_empty() && !name.starts_with('(') => out.push(name.clone()),
                SetupLine::Notation { circumcentre: Some(_), .. } => out.push("R".into()),
                _ => {}
            }
        }
        out
    }
}

pub fn of_view(view: &Value) -> Option<HumanView> {
    let h = view.get("human").filter(|h| !h.is_null())?;
    serde_json::from_value(h.clone()).ok()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Statement,
    Note,
    List,
}

fn has(lang: Lang, key: &str) -> bool {
    i18n::t(lang, key) != i18n::t(lang, "__missing__")
}

fn first_of(lang: Lang, keys: &[String]) -> Option<&'static str> {
    keys.iter().find(|k| has(lang, k)).map(|k| i18n::t(lang, k))
}

fn fill(template: &str, vars: &[(&str, String)]) -> String {
    let mut s = template.to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn fill_args(template: &str, args: &[String]) -> String {
    let mut s = template.to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) if f.is_lowercase() => f.to_uppercase().chain(c).collect(),
        _ => s.to_string(),
    }
}

fn context_keys(base: &str, ctx: Ctx) -> Vec<String> {
    match ctx {
        Ctx::Statement => vec![base.to_string()],
        Ctx::Note => vec![format!("{base}.r"), base.to_string()],
        Ctx::List => vec![format!("{base}.np"), format!("{base}.r"), base.to_string()],
    }
}

pub fn join_list(items: &[String], lang: Lang) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{}{}{}", items[..n - 1].join(", "), i18n::t(lang, "hp.and"), items[n - 1]),
    }
}

struct Cx<'a> {
    h: &'a HumanView,
    view: &'a Value,
    lang: Lang,
    cur: std::cell::Cell<u16>,
}

fn circle_display(name: &str, through: &[String]) -> String {
    if name.is_empty() {
        format!("({})", through.iter().take(3).cloned().collect::<String>())
    } else {
        name.to_string()
    }
}

type CircleLine<'a> = (&'a String, &'a Vec<String>, &'a Option<String>, &'a Option<[String; 2]>);

impl Cx<'_> {
    fn t(&self, key: &str) -> &'static str {
        i18n::t(self.lang, key)
    }

    fn tf(&self, key: &str, vars: &[(&str, String)]) -> String {
        i18n::tf(self.lang, key, vars)
    }

    fn named_circle(&self, pts: &[String], centre: Option<&str>) -> Option<String> {
        self.h.setup.iter().find_map(|s| match s {
            SetupLine::Circle { name, through, centre: c, .. } => {
                let on = pts.iter().all(|p| through.contains(p));
                let by_centre = centre.is_some() && c.as_deref() == centre;
                ((by_centre && on) || (centre.is_none() && pts.len() >= 3 && on)).then(|| circle_display(name, through))
            }
            _ => None,
        })
    }

    fn seen_circle(&self, pts: &[String]) -> Option<String> {
        let mut found: Option<Vec<String>> = None;
        let mut consider = |c: &[String]| {
            if c.len() >= 4 && pts.iter().all(|p| c.contains(p)) && found.as_ref().is_none_or(|f| c.len() < f.len()) {
                found = Some(c.to_vec());
            }
        };
        for b in &self.h.blocks {
            for o in &b.objects {
                if let ObjRef::Circle(c) = o {
                    consider(c);
                }
            }
            for s in &b.body {
                let rs: Vec<&Reason> = match s {
                    Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().flat_map(|l| l.reasons.iter()).collect(),
                    Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => reasons.iter().collect(),
                    Sentence::Raw { .. } => vec![],
                };
                for r in rs {
                    if let Reason::Atom { key: AtomKey::Inscribed, args, .. } = r {
                        consider(args);
                    }
                }
            }
        }
        found.map(|c| format!("({})", c.concat()))
    }

    fn circle_ref(&self, pts: &[String], centre: Option<&str>) -> String {
        if let Some(n) = self.named_circle(pts, centre) {
            return n;
        }
        if pts.len() >= 3 && pts.len() < 4 {
            if let Some(n) = self.seen_circle(pts) {
                return n;
            }
        }
        match centre {
            Some(c) if pts.len() < 3 => self.tf("hp.circle.centred", &[("c", c.to_string())]),
            _ => format!("({})", pts.concat()),
        }
    }

    fn claim_of_block(&self, id: u16) -> Option<u16> {
        self.h.blocks.iter().find(|b| b.id == id && b.kind == BlockKind::Claim).and_then(|b| b.n)
    }

    fn rule_name(&self, r: &str) -> String {
        match self.lang {
            Lang::En => i18n::prose_en(r),
            Lang::Ro => i18n::theorem_ro(r).map(str::to_string).unwrap_or_else(|| i18n::prose_ro(r)),
        }
    }

    fn formula(&self, text: &str) -> String {
        if let Some((name, tri)) = formula_in_triangle(text) {
            return self.tf("hp.formula.in", &[("rule", self.rule_name(name)), ("tri", tri.to_string())]);
        }
        let (rule, body) = match text.split_once(": ") {
            Some((r, b)) if !r.contains('=') => (Some(r), b),
            _ => (None, text),
        };
        match self.lang {
            Lang::En => match rule {
                Some(r) => format!("{}: {body}", i18n::prose_en(r)),
                None => body.to_string(),
            },
            Lang::Ro => {
                let body = body.replacen("in \u{25b3}", "\u{ee}n \u{25b3}", 1);
                match rule {
                    Some(r) => format!("{}: {body}", i18n::theorem_ro(r).map(str::to_string).unwrap_or_else(|| i18n::prose_ro(r))),
                    None => body,
                }
            }
        }
    }

    fn stmt(&self, s: &Stmt, ctx: Ctx) -> String {
        let a = |i: usize| s.args.get(i).cloned().unwrap_or_default();
        if s.kind == "cyclic" && ctx != Ctx::Statement {
            if let Some(name) = self.named_circle(&s.args, None).filter(|n| !n.starts_with('(')) {
                if let Some(t) = first_of(self.lang, &context_keys("hp.fact.cyclic_on", ctx)) {
                    return fill(t, &[("pts", s.args.join(", ")), ("circle", name)]);
                }
            }
        }
        let vars: Option<Vec<(&str, String)>> = match s.kind.as_str() {
            "coll" | "cyclic" => Some(vec![("pts", s.args.join(", "))]),
            "bisects" => Some(vec![("line", a(0)), ("angle", a(1))]),
            "tangent" => Some(vec![("line", a(0)), ("p", a(1)), ("circle", self.circle_ref(&[], Some(&a(2))))]),
            "oncircle" => {
                let on: Vec<String> = s.points.iter().filter(|p| **p != a(0)).cloned().collect();
                Some(vec![("p", a(0)), ("circle", self.circle_ref(&on, None))])
            }
            "radical_axis" => Some(vec![("p", a(0)), ("line", format!("{}{}", a(1), a(2)))]),
            "coincide" => Some(vec![("a", a(0)), ("b", a(1))]),
            _ => None,
        };
        if let Some(vars) = vars {
            if let Some(t) = first_of(self.lang, &context_keys(&format!("hp.fact.{}", s.kind), ctx)) {
                return fill(t, &vars);
            }
        }
        if s.kind == "formula" {
            return self.formula(&a(0));
        }
        render::fact_text(&serde_json::to_value(s).unwrap_or_default(), self.lang)
    }

    fn equal_from(&self, args: &[String]) -> String {
        args.iter().skip(1).map(|p| format!("{}{p}", args[0])).collect::<Vec<_>>().join(" = ")
    }

    fn atom(&self, key: AtomKey, args: &[String], stmt: &Stmt, circle: Option<&str>, ctx: Ctx) -> (String, Option<String>) {
        let g = |i: usize| args.get(i).cloned().unwrap_or_default();
        let sl = |r: std::ops::Range<usize>| args.get(r).map(|x| x.to_vec()).unwrap_or_default();
        let named = |pts: &[String], c: Option<&str>| self.named_circle(pts, c);
        let (base, shown, merge): (&str, Vec<String>, Option<String>) = match key {
            AtomKey::Inscribed => {
                let c = circle.map(str::to_string).unwrap_or_else(|| self.circle_ref(args, None));
                ("inscribed", vec![c.clone()], Some(c))
            }
            AtomKey::Thales => match named(&[g(0), g(1), g(2)], Some(&g(3))) {
                Some(c) => ("thales", vec![g(0), g(1), c], None),
                None => ("thales.bare", vec![g(0), g(1)], None),
            },
            AtomKey::TangentChord => ("tangent_chord", vec![g(0), g(1), circle.map(str::to_string).unwrap_or_else(|| self.circle_ref(&[g(0), g(2), g(3)], Some(&g(4))))], None),
            AtomKey::PerpBisector => ("perp_bisector", sl(0..4), None),
            AtomKey::Parallel => ("parallel", vec![g(4), g(5)], None),
            AtomKey::Radii => match named(&args[1.min(args.len())..], Some(&g(0))) {
                Some(c) => ("radii", vec![c.clone()], Some(c)),
                None if args.len() > 3 => return (self.equal_from(args), None),
                None => return (self.stmt(stmt, Ctx::Statement), None),
            },
            AtomKey::Isosceles if args.len() > 3 => return (self.equal_from(args), None),
            AtomKey::Isosceles => ("isosceles", sl(0..3), None),
            AtomKey::CentralAngle => match named(&[g(1), g(2), g(3)], Some(&g(0))) {
                Some(c) => ("central_angle", vec![c.clone()], Some(c)),
                None => ("central_angle.bare", sl(0..4), None),
            },
            AtomKey::PowerOfPoint => ("power_of_point", vec![g(0), self.circle_ref(&sl(1..5), None)], None),
            AtomKey::Midline => {
                let mut tri = vec![g(4), g(2), g(3)];
                tri.sort();
                ("midline", vec![g(0), g(1), tri.concat()], None)
            }
            AtomKey::Orthocentre => ("orthocentre", vec![], None),
        };
        let t = first_of(self.lang, &context_keys(&format!("hp.atom.{base}"), ctx)).unwrap_or("");
        (fill_args(t, &shown), merge.map(|m| format!("{}|{m}", key.key())))
    }

    fn equal_lengths_centre(&self, key: AtomKey, args: &[String], from: &[u16]) -> Option<String> {
        let unnamed = match key {
            AtomKey::Radii => self.named_circle(&args[1.min(args.len())..], args.first().map(String::as_str)).is_none(),
            AtomKey::Isosceles => true,
            _ => false,
        };
        (unnamed && from.is_empty() && args.len() >= 3).then(|| args[0].clone())
    }

    fn atom_merged(&self, key: AtomKey, circles: &[String], ctx: Ctx) -> String {
        let t = first_of(self.lang, &context_keys(&format!("hp.atom.{}", key.key()), ctx)).unwrap_or("");
        fill_args(t, &[join_list(circles, self.lang)])
    }

    fn from_claims(&self, from: &[u16]) -> Vec<u16> {
        let mut ns: Vec<u16> = from.iter().filter_map(|b| self.claim_of_block(*b)).collect();
        ns.sort_unstable();
        ns.dedup();
        ns
    }

    fn claims_label(&self, ns: &[u16]) -> String {
        if ns.len() == 1 {
            self.tf("hp.claim_ref", &[("n", ns[0].to_string())])
        } else {
            let list: Vec<String> = ns.iter().map(u16::to_string).collect();
            self.tf("hp.claims_ref", &[("list", join_list(&list, self.lang))])
        }
    }

    fn reason(&self, r: &Reason, ctx: Ctx) -> String {
        match r {
            Reason::Hyp { stmt, .. } => self.stmt(stmt, ctx),
            Reason::Fact { stmt, because, .. } => {
                let base = self.stmt(stmt, ctx);
                let inner: Vec<String> = self.because_texts(stmt, because);
                if inner.is_empty() {
                    base
                } else {
                    format!("{base} ({})", inner.join("; "))
                }
            }
            Reason::Claim { n, .. } => self.claims_label(&[*n]),
            Reason::Atom { key, args, stmt, from, circle } => {
                let (mut text, _) = self.atom(*key, args, stmt, circle.as_deref(), ctx);
                if matches!(key, AtomKey::Parallel | AtomKey::Orthocentre) && ctx == Ctx::Note {
                    text = format!("{}, {text}", self.stmt(stmt, ctx));
                }
                let ns = self.from_claims(from);
                if !ns.is_empty() {
                    text = format!("{text} ({})", self.claims_label(&ns));
                }
                text
            }
            Reason::Engine { step } => self.tf("hp.step_ref", &[("n", step.map(|s| s.to_string()).unwrap_or_default())]),
            Reason::Lemma { stmt, block, .. } => {
                let st = self.stmt(stmt, Ctx::Statement);
                match self.claim_of_block(*block).filter(|_| *block != self.cur.get()) {
                    Some(n) => self.tf("hp.lemma.claim", &[("stmt", st), ("claim", self.claims_label(&[n]))]),
                    None => self.tf("hp.lemma", &[("stmt", st)]),
                }
            }
        }
    }

    fn because_texts(&self, own: &Stmt, because: &[Reason]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for b in because {
            let t = match b {
                Reason::Atom { stmt, from, .. } if same_stmt(stmt, own) => {
                    let ns = self.from_claims(from);
                    if ns.is_empty() {
                        continue;
                    }
                    self.claims_label(&ns)
                }
                Reason::Fact { stmt, because: bb, .. } if same_stmt(stmt, own) && bb.is_empty() => continue,
                Reason::Hyp { stmt, .. } if same_stmt(stmt, own) => continue,
                _ => self.reason(b, Ctx::Note),
            };
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    fn texts(&self, rs: &[Reason], ctx: Ctx, skip_claims: bool) -> Vec<String> {
        let facts: Vec<&Stmt> = rs.iter().filter_map(|r| if let Reason::Fact { stmt, .. } = r { Some(stmt) } else { None }).collect();
        let mut out: Vec<String> = Vec::new();
        let mut groups: Vec<(String, usize, Vec<String>, AtomKey)> = Vec::new();
        for r in rs {
            if skip_claims && matches!(r, Reason::Claim { .. }) {
                continue;
            }
            if let Reason::Lemma { stmt, block, .. } = r {
                if *block == self.cur.get() || self.claim_of_block(*block).is_none() {
                    let st = self.stmt(stmt, Ctx::Statement);
                    match groups.iter_mut().find(|g| g.0 == "lemma") {
                        Some((_, at, list, _)) => {
                            if !list.contains(&st) {
                                list.push(st);
                            }
                            out[*at] = self.tf(if list.len() > 1 { "hp.lemma.many" } else { "hp.lemma" }, &[("stmt", join_list(list, self.lang))]);
                        }
                        None => {
                            groups.push(("lemma".into(), out.len(), vec![st.clone()], AtomKey::Inscribed));
                            out.push(self.tf("hp.lemma", &[("stmt", st)]));
                        }
                    }
                    continue;
                }
            }
            if let Reason::Atom { key, args, stmt, from, circle } = r {
                if facts.iter().any(|s| same_stmt(s, stmt)) {
                    continue;
                }
                if let Some(centre) = self.equal_lengths_centre(*key, args, from) {
                    let group = format!("={centre}");
                    match groups.iter_mut().find(|g| g.0 == group) {
                        Some((_, at, pts, _)) => {
                            for p in &args[1..] {
                                if !pts.contains(p) {
                                    pts.push(p.clone());
                                }
                            }
                            out[*at] = self.equal_from(pts);
                        }
                        None => {
                            let pts = args.to_vec();
                            groups.push((group, out.len(), pts.clone(), *key));
                            out.push(self.equal_from(&pts));
                        }
                    }
                    continue;
                }
                let (_, merge) = self.atom(*key, args, stmt, circle.as_deref(), ctx);
                if let (Some(m), true) = (merge, from.is_empty()) {
                    let (group, circle) = m.split_once('|').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or_default();
                    match groups.iter_mut().find(|g| g.0 == group) {
                        Some((_, at, circles, k)) => {
                            if !circles.contains(&circle) {
                                circles.push(circle);
                            }
                            out[*at] = self.atom_merged(*k, circles, ctx);
                        }
                        None => {
                            groups.push((group, out.len(), vec![circle], *key));
                            out.push(self.reason(r, ctx));
                        }
                    }
                    continue;
                }
            }
            let t = self.reason(r, ctx);
            if !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    fn reasons_list(&self, rs: &[Reason]) -> String {
        let mut claims: Vec<u16> = rs.iter().filter_map(|r| if let Reason::Claim { n, .. } = r { Some(*n) } else { None }).collect();
        claims.sort_unstable();
        claims.dedup();
        let mut out = self.texts(rs, Ctx::List, true);
        if !claims.is_empty() {
            out.insert(0, self.claims_label(&claims));
        }
        join_list(&out, self.lang)
    }

    fn link_note(&self, l: &Link) -> String {
        self.texts(&l.reasons, Ctx::Note, false).join(", ")
    }
}

fn point_set(s: &Stmt) -> Vec<String> {
    let mut p = s.points.clone();
    p.sort();
    p.dedup();
    p
}

pub fn same_stmt(a: &Stmt, b: &Stmt) -> bool {
    if a.kind != b.kind || point_set(a) != point_set(b) {
        return false;
    }
    let unordered = matches!(a.kind.as_str(), "cong" | "perp" | "para" | "coll" | "cyclic");
    let norm = |s: &Stmt| {
        let mut v: Vec<String> = s
            .args
            .iter()
            .map(|x| {
                let mut c: Vec<char> = x.chars().collect();
                if unordered {
                    c.sort_unstable();
                }
                c.into_iter().collect()
            })
            .collect();
        if unordered || s.kind == "eqangle" {
            v.sort();
        }
        v
    };
    norm(a) == norm(b)
}

fn formula_in_triangle(text: &str) -> Option<(&str, &str)> {
    let (name, tri) = text.split_once(" in \u{25b3}")?;
    (!name.contains([':', '=']) && !tri.contains(' ')).then_some((name, tri))
}

pub fn restates_chain(terms: &[String], then: &Stmt) -> bool {
    let (Some(first), Some(last)) = (terms.first(), terms.last()) else { return false };
    matches!(then.kind.as_str(), "eqangle" | "cong" | "eq") && then.args.len() == 2 && ((then.args[0] == *first && then.args[1] == *last) || (then.args[0] == *last && then.args[1] == *first))
}

pub fn inline_chain(terms: &[String], links: &[Link]) -> bool {
    links.len() <= 2 && terms.iter().map(|t| t.chars().count() + 3).sum::<usize>() <= 44
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub lhs: String,
    pub rhs: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Text(String),
    Rows(Vec<Row>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocBlock {
    pub kind: BlockKind,
    pub head: Option<String>,
    pub stmt: Option<String>,
    pub proof_label: Option<String>,
    pub parts: Vec<Part>,
    pub end: bool,
    pub steps: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Doc {
    pub setup: Vec<String>,
    pub notes: Vec<String>,
    pub blocks: Vec<DocBlock>,
}

fn article(def: &str, lang: Lang) -> String {
    if lang == Lang::En && !["a ", "an ", "the "].iter().any(|a| def.starts_with(a)) {
        format!("the {def}")
    } else {
        def.to_string()
    }
}

fn view_aux<'a>(view: &'a Value, field: &str, pred: impl Fn(usize, &Value) -> bool) -> Option<&'a Value> {
    view[field].as_array()?.iter().enumerate().find(|(i, a)| pred(*i, a)).map(|(_, a)| a)
}

impl Cx<'_> {
    fn let_line(&self, point: &str, def: &str) -> String {
        self.tf("hp.setup.let", &[("p", point.to_string()), ("def", article(def, self.lang))])
    }

    fn on_clause(&self, pts: &[String], name: &str) -> String {
        let key = if pts.len() == 1 { "hp.setup.on.one" } else { "hp.setup.on.other" };
        self.tf(key, &[("pts", pts.join(", ")), ("name", name.to_string())])
    }

    fn main_triangle(&self) -> Option<Vec<String>> {
        let pts: Vec<String> = self.view["points"].as_array()?.iter().take(3).filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
        (pts.len() == 3).then_some(pts)
    }

    fn circles_sentence(&self, circles: &[CircleLine]) -> Option<String> {
        let mut clauses: Vec<String> = Vec::new();
        let mut by_points: Vec<(String, Vec<String>, Option<String>)> = Vec::new();
        for (name, through, centre, diameter) in circles {
            let shown = circle_display(name, through);
            if let Some(d) = diameter {
                let mut s = self.tf("hp.setup.circle_d", &[("name", shown.clone()), ("d", d.concat())]);
                if let Some(c) = centre {
                    s.push_str(&self.tf("hp.setup.centre", &[("c", c.clone())]));
                }
                let others: Vec<String> = through.iter().filter(|p| !d.contains(p)).cloned().collect();
                if !others.is_empty() {
                    s.push_str(&self.on_clause(&others, &shown));
                }
                clauses.push(format!("{s}."));
            } else if shown.starts_with('(') {
                let in_name: Vec<String> = shown.trim_matches(|c| c == '(' || c == ')').chars().map(|c| c.to_string()).collect();
                let extra: Vec<String> = through.iter().filter(|p| !in_name.contains(p)).cloned().collect();
                by_points.push((shown, extra, (*centre).clone()));
            } else {
                let tri = self.main_triangle().filter(|t| t.iter().all(|p| through.contains(p)));
                let first: Vec<String> = tri.clone().unwrap_or_else(|| through.iter().take(3).cloned().collect());
                let key = if tri.is_some() { "hp.setup.circumcircle" } else { "hp.setup.circle" };
                let mut s = self.tf(key, &[("name", shown.clone()), ("pts", first.concat())]);
                if let Some(c) = centre {
                    s.push_str(&self.tf("hp.setup.centre", &[("c", c.clone())]));
                }
                let others: Vec<String> = through.iter().filter(|p| !first.contains(p)).cloned().collect();
                if !others.is_empty() {
                    s.push_str(&self.on_clause(&others, &shown));
                }
                clauses.push(format!("{s}."));
            }
        }
        if !by_points.is_empty() {
            let names: Vec<String> = by_points.iter().map(|p| p.0.clone()).collect();
            let centres: Vec<String> = by_points.iter().filter_map(|p| p.2.clone()).collect();
            let shared = by_points.iter().all(|p| p.1 == by_points[0].1) && !by_points[0].1.is_empty();
            let mut s = if shared {
                let key = if by_points[0].1.len() == 1 { "hp.setup.lie.one" } else { "hp.setup.lie.other" };
                self.tf(key, &[("pts", join_list(&by_points[0].1, self.lang)), ("circles", join_list(&names, self.lang))])
            } else {
                self.tf("hp.setup.circles", &[("circles", join_list(&names, self.lang))])
            };
            if centres.len() == by_points.len() {
                let key = if centres.len() == 1 { "hp.setup.centres.one" } else { "hp.setup.centres.other" };
                s.push_str(&self.tf(key, &[("c", join_list(&centres, self.lang))]));
            }
            if !shared {
                for p in by_points.iter().filter(|p| !p.1.is_empty()) {
                    s.push_str(&self.on_clause(&p.1, &p.0));
                }
            }
            clauses.push(format!("{}.", capitalize(&s)));
        }
        (!clauses.is_empty()).then(|| clauses.join(" "))
    }

    fn notation_line(&self, triangle: &[String; 3], circumcentre: Option<&str>) -> String {
        let vars = vec![("a", triangle[0].clone()), ("b", triangle[1].clone()), ("c", triangle[2].clone()), ("tri", triangle.concat())];
        match circumcentre {
            Some(o) => {
                let r = triangle.iter().map(|v| format!("{o}{v}")).collect::<Vec<_>>().join(" = ");
                self.tf("hp.setup.notation_r", &[vars, vec![("R", "R".to_string()), ("r", r)]].concat())
            }
            None => self.tf("hp.setup.notation", &vars),
        }
    }

    fn wording_arg(&self, a: &AuxArg, flat: bool) -> Vec<String> {
        let p = |i: usize| a.pts.get(i).cloned().unwrap_or_default();
        let from = |i: usize| a.pts.get(i..).unwrap_or_default().concat();
        let fill = |key: &str, args: &[String]| fill_args(self.t(key), args);
        match a.key.as_str() {
            "point" => vec![p(0)],
            "segment" | "triangle" if flat => a.pts.clone(),
            "segment" | "triangle" | "line" => vec![a.pts.concat()],
            "aux.line.para" | "aux.line.perp" => vec![fill(&a.key, &[p(0), from(1)])],
            "aux.line.tangent_at" => vec![fill(&a.key, &[p(0), p(1)])],
            "aux.line.isogonal" if a.pts.len() == 5 => vec![fill(&a.key, &[a.pts[..2].concat(), from(2)])],
            "aux.circumcircle" => vec![self.named_circle(&a.pts, None).unwrap_or_else(|| if a.pts.len() == 3 { fill("aux.circumcircle", &a.pts) } else { format!("({})", a.pts.concat()) })],
            "aux.circle" => vec![self.named_circle(a.pts.get(1..).unwrap_or_default(), Some(&p(0))).unwrap_or_else(|| fill("aux.circle", &[p(0), p(1)]))],
            _ => vec![a.pts.concat()],
        }
    }

    fn wording_def(&self, point: &str, w: &AuxWording) -> Option<String> {
        let suffix = w.key.strip_prefix("aux.")?;
        let flat = matches!(suffix, "midpoint" | "circumcenter" | "orthocenter" | "incenter" | "centroid" | "excenter" | "excenter_any" | "parallelogram" | "arc_midpoint");
        let mut args: Vec<String> = w.args.iter().flat_map(|a| self.wording_arg(a, flat)).collect();
        let mut key = format!("aux.{suffix}");
        match suffix {
            "excenter" if args.len() == 4 => {
                let side: String = args[..3].iter().filter(|v| **v != args[3]).cloned().collect();
                args.push(side);
            }
            "intersect2" if w.args.first().is_some_and(|a| a.key != "line") => key = "aux.intersect2_shape".into(),
            _ => {}
        }
        let template = first_of(self.lang, &[format!("hp.{key}"), key])?;
        let text = fill_args(template, &args).replace("{p}", point);
        (!text.contains('{')).then_some(text)
    }

    fn aux_line(&self, point: &str, aux_index: usize, wording: Option<&AuxWording>) -> Option<String> {
        let view = view_aux(self.view, "aux", |i, _| i == aux_index);
        let def = match wording.and_then(|w| self.wording_def(point, w)) {
            Some(d) => match view.and_then(|a| a["same"].as_str()) {
                Some(same) => d + &self.tf("aux.same", &[("p", same.to_string())]),
                None => d,
            },
            None => render::aux_text(view?, self.lang),
        };
        Some(self.let_line(point, &def))
    }

    fn setup_lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        let setup = &self.h.setup;
        let mut i = 0;
        while i < setup.len() {
            let s = match &setup[i] {
                SetupLine::DirectedAngles => Some(self.t("hp.setup.directed").to_string()),
                SetupLine::Notation { triangle, circumcentre } => Some(self.notation_line(triangle, circumcentre.as_deref())),
                SetupLine::Circle { .. } => {
                    let later: Vec<&String> = setup[i..].iter().filter_map(|l| if let SetupLine::Aux { point, .. } = l { Some(point) } else { None }).collect();
                    let mut run: Vec<(String, Vec<String>, Option<String>, Option<[String; 2]>)> = Vec::new();
                    while let Some(SetupLine::Circle { name, through, centre, diameter }) = setup.get(i) {
                        let kept: Vec<String> = through.iter().filter(|p| !later.contains(p)).cloned().collect();
                        run.push((name.clone(), if kept.len() >= 3 { kept } else { through.clone() }, centre.clone(), diameter.clone()));
                        i += 1;
                    }
                    i -= 1;
                    let lines: Vec<CircleLine> = run.iter().map(|(n, t, c, d)| (n, t, c, d)).collect();
                    self.circles_sentence(&lines)
                }
                SetupLine::Aux { point, aux_index, wording } => self.aux_line(point, *aux_index, wording.as_ref()),
                SetupLine::Helper { point, meaning } => match meaning {
                    HelperMeaning::Midpoint { of } => Some(self.let_line(point, &self.tf("hp.helper.midpoint", &[("seg", of.concat())]))),
                    HelperMeaning::Reflection { of, line } => Some(self.let_line(point, &self.tf("hp.helper.reflection", &[("p", of.clone()), ("line", line.concat())]))),
                    HelperMeaning::Perp { through, to } => Some(self.let_line(point, &self.tf("hp.helper.perp", &[("p", through.clone()), ("line", to.concat())]))),
                    HelperMeaning::Para { through, to } => Some(self.let_line(point, &self.tf("hp.helper.para", &[("p", through.clone()), ("line", to.concat())]))),
                    HelperMeaning::Point => view_aux(self.view, "helpers", |_, a| a["name"].as_str() == Some(point.as_str())).map(|a| self.let_line(point, &render::aux_text(a, self.lang))),
                },
            };
            out.extend(s);
            i += 1;
        }
        out
    }

    fn rows(&self, terms: &[String], links: &[Link]) -> Vec<Row> {
        links
            .iter()
            .enumerate()
            .map(|(k, l)| Row {
                lhs: if k == 0 { terms.first().cloned().unwrap_or_default() } else { String::new() },
                rhs: terms.get(k + 1).cloned().unwrap_or_default(),
                reason: self.link_note(l),
            })
            .collect()
    }

    fn because(&self, own: &Stmt, stmt: &Stmt, reasons: &[Reason]) -> Option<String> {
        let st = self.stmt(stmt, Ctx::Statement);
        if reasons.is_empty() {
            return Some(self.tf("hp.hence", &[("stmt", st)]));
        }
        let inner: &[Reason] = match reasons {
            [Reason::Fact { stmt: s2, because, .. }] if same_stmt(s2, stmt) && !because.is_empty() => because,
            _ => reasons,
        };
        let restating = inner.iter().all(|r| match r {
            Reason::Fact { stmt: s2, .. } | Reason::Hyp { stmt: s2, .. } => same_stmt(s2, stmt),
            Reason::Atom { key: AtomKey::Radii, stmt: s2, from, .. } => same_stmt(s2, stmt) && self.from_claims(from).is_empty(),
            _ => false,
        });
        if restating {
            return (!same_stmt(own, stmt)).then(|| format!("{}.", capitalize(&st)));
        }
        let rs = self.texts(inner, Ctx::Note, false);
        Some(if rs.is_empty() { format!("{}.", capitalize(&st)) } else { capitalize(&self.tf("hp.because", &[("stmt", st), ("reasons", rs.join("; "))])) })
    }

    fn pooled_key(stmt: &Stmt) -> &'static str {
        match stmt.kind.as_str() {
            "coll" | "cyclic" | "perp" | "para" | "eqangle" | "aconst" | "bisects" | "tangent" => "hp.pooled.angle",
            "eq" if stmt.args.iter().any(|a| a.contains('\u{b2}')) => "hp.pooled.length",
            _ => "hp.pooled.ratio",
        }
    }

    fn sentence_parts(&self, own: &Stmt, s: &Sentence, lead: Option<&str>) -> Vec<Part> {
        let lead = |text: String| match lead {
            Some(l) => format!("{l}{}", lower_first_word(&text)),
            None => text,
        };
        match s {
            Sentence::Chain { terms, links, then, directed } => {
                let so = then.as_ref().filter(|t| !restates_chain(terms, t)).map(|t| self.stmt(t, Ctx::Statement));
                let as_drawn = !directed;
                if inline_chain(terms, links) {
                    let notes: Vec<String> = links.iter().map(|l| self.link_note(l)).filter(|n| !n.is_empty()).collect();
                    let mut text = terms.join(" = ");
                    if as_drawn {
                        text = format!("{}{text}", self.t("hp.as_drawn_lead"));
                    }
                    if !notes.is_empty() {
                        text.push_str(&format!(" ({})", notes.join("; ")));
                    }
                    match so {
                        Some(t) => text.push_str(&self.tf("hp.so_inline", &[("stmt", t)])),
                        None => text.push('.'),
                    }
                    vec![Part::Text(lead(text))]
                } else {
                    let mut parts = Vec::new();
                    if as_drawn {
                        parts.push(Part::Text(self.t("hp.as_drawn_display").to_string()));
                    }
                    parts.push(Part::Rows(self.rows(terms, links)));
                    if let Some(t) = so {
                        parts.push(Part::Text(self.tf("hp.so", &[("stmt", t)])));
                    }
                    parts
                }
            }
            Sentence::Because { stmt, reasons, .. } => match self.because(own, stmt, reasons) {
                Some(t) => vec![Part::Text(lead(t))],
                None => vec![],
            },
            Sentence::Pooled { stmt, reasons, .. } => {
                let text = self.tf(Self::pooled_key(stmt), &[("reasons", self.reasons_list(reasons)), ("stmt", self.stmt(stmt, Ctx::Statement))]);
                vec![Part::Text(lead(capitalize(&text)))]
            }
            Sentence::Theorem { key, stmt, reasons } => {
                let thm = self.t(&format!("hp.thm.{}", key.key())).to_string();
                let rs = self.texts(reasons, Ctx::Note, false);
                let in_tri = (stmt.kind == "formula").then(|| stmt.args.first().and_then(|a| formula_in_triangle(a))).flatten();
                let text = if let Some((_, tri)) = in_tri {
                    let tri = format!("\u{25b3}{tri}");
                    if rs.is_empty() {
                        self.tf("hp.apply_in", &[("thm", thm), ("tri", tri)])
                    } else {
                        self.tf("hp.apply_in_with", &[("thm", thm), ("reasons", rs.join("; ")), ("tri", tri)])
                    }
                } else if stmt.kind == "formula" {
                    let f = self.stmt(stmt, Ctx::Statement);
                    let rest = f.split_once(": ").map(|x| x.1.to_string()).unwrap_or(f);
                    if rs.is_empty() {
                        self.tf("hp.apply", &[("thm", thm), ("stmt", rest)])
                    } else {
                        self.tf("hp.apply_with", &[("thm", thm), ("reasons", rs.join("; ")), ("stmt", rest)])
                    }
                } else {
                    let st = self.stmt(stmt, Ctx::Statement);
                    if rs.is_empty() {
                        self.tf("hp.theorem", &[("thm", thm), ("stmt", st)])
                    } else {
                        self.tf("hp.theorem_with", &[("thm", thm), ("reasons", rs.join("; ")), ("stmt", st)])
                    }
                };
                vec![Part::Text(lead(capitalize(&text)))]
            }
            Sentence::Computation { terms, links, .. } => vec![Part::Rows(self.rows(terms, links))],
            Sentence::Raw { step, .. } => {
                let n = step.map(|s| s.to_string()).unwrap_or_default();
                let text = match step.and_then(|s| raw_step(self.view, s)) {
                    Some(st) => self.tf("hp.raw", &[("fact", render::fact_text(&st["fact"], self.lang)), ("rule", rule_of(st, self.lang)), ("n", n)]),
                    None => self.tf("hp.step_ref", &[("n", n)]),
                };
                vec![Part::Text(lead(capitalize(&text)))]
            }
        }
    }
}

fn raw_step(view: &Value, n: usize) -> Option<&Value> {
    view["proof"]["steps"].as_array()?.iter().find(|s| s["n"].as_u64() == Some(n as u64))
}

fn rule_of(step: &Value, lang: Lang) -> String {
    match (step["rule"].as_str().unwrap_or("other"), step["rule_name"].as_str()) {
        ("theorem", Some(name)) => match (lang, step["rule_name_ro"].as_str()) {
            (Lang::Ro, Some(ro)) => ro.to_string(),
            _ => name.to_string(),
        },
        (key, _) => i18n::t(lang, &format!("rule.{key}")).to_string(),
    }
}

fn lower_first_word(s: &str) -> String {
    let mut c = s.chars();
    match (c.next(), s.chars().nth(1)) {
        (Some(f), Some(g)) if f.is_uppercase() && g.is_lowercase() => f.to_lowercase().chain(c).collect(),
        _ => s.to_string(),
    }
}

fn merge_text(parts: Vec<Part>) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for p in parts {
        match (out.last_mut(), p) {
            (Some(Part::Text(prev)), Part::Text(t)) => {
                prev.push(' ');
                prev.push_str(&t);
            }
            (_, p) => out.push(p),
        }
    }
    out
}

pub fn doc(h: &HumanView, view: &Value, lang: Lang) -> Doc {
    let cx = Cx { h, view, lang, cur: std::cell::Cell::new(0) };
    let claims = h.blocks.iter().filter(|b| b.kind == BlockKind::Claim).count();
    let mut blocks = Vec::new();
    for b in &h.blocks {
        cx.cur.set(b.id);
        let mut parts = Vec::new();
        for (i, s) in b.body.iter().enumerate() {
            let lead = (i == 0 && b.kind == BlockKind::Conclusion && claims > 0 && matches!(s, Sentence::Pooled { .. })).then(|| cx.t("hp.finally"));
            parts.extend(cx.sentence_parts(&b.stmt, s, lead));
        }
        let (head, stmt, proof_label) = match b.kind {
            BlockKind::Claim => (
                Some(cx.tf("hp.claim", &[("n", b.n.unwrap_or(0).to_string())])),
                Some(format!("{}.", capitalize(&cx.stmt(&b.stmt, Ctx::Statement)))),
                Some(cx.t("hp.proof").to_string()),
            ),
            _ => (None, None, None),
        };
        blocks.push(DocBlock { kind: b.kind, head, stmt, proof_label, parts: merge_text(parts), end: b.kind == BlockKind::Conclusion, steps: b.engine_steps.clone() });
    }
    let mut notes = Vec::new();
    if h.as_drawn {
        notes.push(cx.t("hp.as_drawn").to_string());
    }
    Doc { setup: cx.setup_lines(), notes, blocks }
}

pub fn text(h: &HumanView, view: &Value, lang: Lang) -> String {
    let d = doc(h, view, lang);
    let mut out: Vec<String> = Vec::new();
    let mut setup = d.setup.clone();
    setup.extend(d.notes.iter().cloned());
    if !setup.is_empty() {
        out.push(setup.join(" "));
    }
    for b in &d.blocks {
        let mut para: Vec<String> = Vec::new();
        if let (Some(h), Some(s)) = (&b.head, &b.stmt) {
            out.push(format!("{h} {s}"));
        }
        let mut first = true;
        for p in &b.parts {
            match p {
                Part::Text(t) => {
                    let lead = match (&b.proof_label, first) {
                        (Some(l), true) => format!("{l} "),
                        _ => String::new(),
                    };
                    para.push(format!("{lead}{t}"));
                }
                Part::Rows(rows) => {
                    if let (Some(l), true) = (&b.proof_label, first) {
                        para.push(l.clone());
                    }
                    if !para.is_empty() {
                        out.push(para.join(" "));
                        para.clear();
                    }
                    let w = rows.iter().map(|r| r.lhs.chars().count()).max().unwrap_or(0);
                    let table: Vec<String> = rows
                        .iter()
                        .map(|r| {
                            let pad = " ".repeat(w.saturating_sub(r.lhs.chars().count()));
                            let reason = if r.reason.is_empty() { String::new() } else { format!("    [{}]", r.reason) };
                            format!("{}{pad} = {}{reason}", r.lhs, r.rhs)
                        })
                        .collect();
                    out.push(table.join("\n"));
                }
            }
            first = false;
        }
        if b.end {
            para.push("\u{220e}".to_string());
        }
        if !para.is_empty() {
            out.push(para.join(" "));
        }
    }
    out.join("\n\n")
}

#[derive(Clone, Copy)]
pub struct LineAlias {
    pub helper: PointId,
    pub vertex: PointId,
    pub on: PointId,
    pub same_ray: bool,
}

enum Use<'a> {
    Line(PointId),
    Ray { vertex: PointId, directed: bool },
    Coll(&'a [PointId]),
    Free,
    Fixed,
}

fn walk_line(l: &mut (PointId, PointId), f: &mut dyn FnMut(Use, &mut PointId)) {
    let (a, b) = *l;
    f(Use::Line(b), &mut l.0);
    f(Use::Line(a), &mut l.1);
}

fn walk_expr(e: &mut eng::Expr, f: &mut dyn FnMut(Use, &mut PointId)) {
    match e {
        eng::Expr::Angle { a, b, c, directed } => {
            let (v, d) = (*b, *directed);
            f(Use::Ray { vertex: v, directed: d }, a);
            f(Use::Fixed, b);
            f(Use::Ray { vertex: v, directed: d }, c);
        }
        eng::Expr::LineAngle { l1, l2, .. } => {
            walk_line(l1, f);
            walk_line(l2, f);
        }
        eng::Expr::Lin { terms } => terms.iter_mut().for_each(|(_, x)| walk_expr(x, f)),
        eng::Expr::Seg { a, b } | eng::Expr::Sq { a, b } => {
            f(Use::Fixed, a);
            f(Use::Fixed, b);
        }
        eng::Expr::Prod { factors } => factors.iter_mut().for_each(|(x, _)| walk_expr(x, f)),
        eng::Expr::Sin { angle } | eng::Expr::Cos { angle } => walk_expr(angle, f),
        eng::Expr::Const { .. } | eng::Expr::Num { .. } => {}
    }
}

fn walk_tri(t: &mut (PointId, PointId, PointId), f: &mut dyn FnMut(Use, &mut PointId)) {
    f(Use::Fixed, &mut t.0);
    f(Use::Fixed, &mut t.1);
    f(Use::Fixed, &mut t.2);
}

fn walk_stmt(s: &mut eng::Stmt, f: &mut dyn FnMut(Use, &mut PointId)) {
    match s {
        eng::Stmt::Coll { pts } => {
            let all = pts.clone();
            for p in pts.iter_mut() {
                f(Use::Coll(&all), p);
            }
        }
        eng::Stmt::Perp { l1, l2 } | eng::Stmt::Para { l1, l2 } => {
            walk_line(l1, f);
            walk_line(l2, f);
        }
        eng::Stmt::EqAngle { lhs, rhs } | eng::Stmt::Eq { lhs, rhs } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        eng::Stmt::AngleConst { angle, .. } => walk_expr(angle, f),
        eng::Stmt::Sim { t1, t2, .. } | eng::Stmt::Congruent { t1, t2, .. } => {
            walk_tri(t1, f);
            walk_tri(t2, f);
        }
        eng::Stmt::Cyclic { pts } | eng::Stmt::Formula { pts, .. } => pts.iter_mut().for_each(|p| f(Use::Fixed, p)),
        eng::Stmt::Cong { s1, s2 } | eng::Stmt::RatioConst { s1, s2, .. } => {
            for p in [&mut s1.0, &mut s1.1, &mut s2.0, &mut s2.1] {
                f(Use::Fixed, p);
            }
        }
        eng::Stmt::EqRatio { segs } => segs.iter_mut().for_each(|x| {
            f(Use::Fixed, &mut x.0);
            f(Use::Fixed, &mut x.1);
        }),
        eng::Stmt::OnCircle { p, circle } => {
            f(Use::Fixed, p);
            circle.iter_mut().for_each(|q| f(Use::Fixed, q));
        }
        eng::Stmt::Tangent { p, line, circle } => {
            f(Use::Fixed, p);
            walk_line(line, f);
            circle.iter_mut().for_each(|q| f(Use::Fixed, q));
        }
        eng::Stmt::RadicalAxis { x, u, v } => {
            for p in [x, u, v] {
                f(Use::Fixed, p);
            }
        }
        eng::Stmt::Coincide { a, b } => {
            f(Use::Fixed, a);
            f(Use::Fixed, b);
        }
    }
}

fn walk_reason(r: &mut eng::Reason, f: &mut dyn FnMut(Use, &mut PointId)) {
    match r {
        eng::Reason::Hyp { stmt, .. } => walk_stmt(stmt, f),
        eng::Reason::Atom { stmt, args, .. } => {
            walk_stmt(stmt, f);
            args.iter_mut().for_each(|p| f(Use::Fixed, p));
        }
        eng::Reason::Fact { stmt, because, .. } => {
            walk_stmt(stmt, f);
            because.iter_mut().for_each(|b| walk_reason(b, f));
        }
        eng::Reason::Lemma { stmt, .. } => walk_stmt(stmt, f),
        eng::Reason::Claim { .. } | eng::Reason::Engine { .. } => {}
    }
}

fn walk_link(l: &mut eng::Link, f: &mut dyn FnMut(Use, &mut PointId)) {
    l.reasons.iter_mut().for_each(|r| walk_reason(r, f));
}

fn walk_proof(hp: &mut eng::HumanProof, f: &mut dyn FnMut(Use, &mut PointId)) {
    for b in hp.blocks.iter_mut() {
        walk_stmt(&mut b.stmt, f);
        b.points.iter_mut().for_each(|p| f(Use::Free, p));
        for o in b.objects.iter_mut() {
            match o {
                eng::ObjRef::Circle { through } | eng::ObjRef::Line { through } => through.iter_mut().for_each(|p| f(Use::Free, p)),
            }
        }
        for s in b.body.iter_mut() {
            match s {
                eng::Sentence::Chain { terms, links, then, .. } => {
                    terms.iter_mut().for_each(|e| walk_expr(e, f));
                    links.iter_mut().for_each(|l| walk_link(l, f));
                    if let Some(t) = then {
                        walk_stmt(t, f);
                    }
                }
                eng::Sentence::Because { stmt, reasons, .. } | eng::Sentence::Pooled { stmt, reasons, .. } | eng::Sentence::Theorem { stmt, reasons, .. } => {
                    walk_stmt(stmt, f);
                    reasons.iter_mut().for_each(|r| walk_reason(r, f));
                }
                eng::Sentence::Computation { terms, links, .. } => {
                    terms.iter_mut().for_each(|e| walk_expr(e, f));
                    links.iter_mut().for_each(|l| walk_link(l, f));
                }
                eng::Sentence::Raw { .. } => {}
            }
        }
    }
    for s in hp.setup.iter_mut() {
        match s {
            eng::SetupLine::Circle { through, centre, .. } => {
                through.iter_mut().for_each(|p| f(Use::Fixed, p));
                if let Some(c) = centre {
                    f(Use::Fixed, c);
                }
            }
            eng::SetupLine::Aux { point, wording, .. } => {
                f(Use::Fixed, point);
                for a in wording.iter_mut().flat_map(|w| w.args.iter_mut()) {
                    if a.key == "line" && a.pts.len() == 2 {
                        let (p, q) = (a.pts[0], a.pts[1]);
                        f(Use::Line(q), &mut a.pts[0]);
                        f(Use::Line(p), &mut a.pts[1]);
                    } else {
                        a.pts.iter_mut().for_each(|p| f(Use::Fixed, p));
                    }
                }
            }
            eng::SetupLine::Notation { triangle, circumcentre } => {
                walk_tri(triangle, f);
                if let Some(c) = circumcentre {
                    f(Use::Fixed, c);
                }
            }
            eng::SetupLine::Helper { meaning: eng::HelperMeaning::Perp { through, to } | eng::HelperMeaning::Para { through, to }, .. } => {
                f(Use::Fixed, through);
                walk_line(to, f);
            }
            eng::SetupLine::Helper { .. } | eng::SetupLine::DirectedAngles => {}
        }
    }
}

fn alias_fits(hp: &mut eng::HumanProof, a: &LineAlias) -> bool {
    let mut ok = true;
    walk_proof(hp, &mut |u, p| {
        if *p != a.helper {
            return;
        }
        ok &= match u {
            Use::Line(other) => other == a.vertex,
            Use::Ray { vertex, directed } => vertex == a.vertex && (directed || a.same_ray),
            Use::Coll(all) => all.contains(&a.vertex),
            Use::Free => true,
            Use::Fixed => false,
        };
    });
    ok
}

fn dedup_ids(v: &mut Vec<PointId>) {
    let mut seen: Vec<PointId> = Vec::new();
    v.retain(|x| {
        let new = !seen.contains(x);
        if new {
            seen.push(*x);
        }
        new
    });
}

fn dedup_colls(hp: &mut eng::HumanProof) {
    fn stmt(s: &mut eng::Stmt) {
        if let eng::Stmt::Coll { pts } = s {
            dedup_ids(pts);
        }
    }
    fn reason(r: &mut eng::Reason) {
        match r {
            eng::Reason::Hyp { stmt: s, .. } | eng::Reason::Atom { stmt: s, .. } | eng::Reason::Lemma { stmt: s, .. } => stmt(s),
            eng::Reason::Fact { stmt: s, because, .. } => {
                stmt(s);
                because.iter_mut().for_each(reason);
            }
            _ => {}
        }
    }
    for b in hp.blocks.iter_mut() {
        stmt(&mut b.stmt);
        dedup_ids(&mut b.points);
        for o in b.objects.iter_mut() {
            match o {
                eng::ObjRef::Circle { through } | eng::ObjRef::Line { through } => dedup_ids(through),
            }
        }
        for s in b.body.iter_mut() {
            match s {
                eng::Sentence::Chain { links, then, .. } => {
                    links.iter_mut().for_each(|l| l.reasons.iter_mut().for_each(reason));
                    if let Some(t) = then {
                        stmt(t);
                    }
                }
                eng::Sentence::Because { stmt: st, reasons, .. } | eng::Sentence::Pooled { stmt: st, reasons, .. } | eng::Sentence::Theorem { stmt: st, reasons, .. } => {
                    stmt(st);
                    reasons.iter_mut().for_each(reason);
                }
                eng::Sentence::Computation { links, .. } => links.iter_mut().for_each(|l| l.reasons.iter_mut().for_each(reason)),
                eng::Sentence::Raw { .. } => {}
            }
        }
    }
}

pub fn alias_lines(hp: &eng::HumanProof, aliases: &[LineAlias]) -> eng::HumanProof {
    let mut out = hp.clone();
    for a in aliases {
        if !alias_fits(&mut out, a) {
            continue;
        }
        walk_proof(&mut out, &mut |_, p| {
            if *p == a.helper {
                *p = a.on;
            }
        });
        out.setup.retain(|s| !matches!(s, eng::SetupLine::Helper { point, .. } if *point == a.helper));
    }
    dedup_colls(&mut out);
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::engine::{solve, solve_best, SolveOptions};
    use serde_json::json;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::time::Duration;

    #[derive(Serialize, Deserialize)]
    pub struct Fixture {
        pub name: String,
        pub program: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub best_secs: Option<u64>,
        pub view: Value,
        pub human: HumanView,
    }

    pub const FAST: [&str; 4] = ["orthocenter-reflection", "euler-line", "orthocenter-vertex-distance", "imo-2004-p1"];
    pub const ALL: [&str; 5] = ["orthocenter-reflection", "euler-line", "orthocenter-vertex-distance", "imo-2004-p1", "imo-2023-p2"];

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/human")
    }

    pub fn load(name: &str) -> Fixture {
        let path = dir().join(format!("{name}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn solution(program: &str, best_secs: Option<u64>) -> Value {
        let opts = SolveOptions::default();
        let sol = match best_secs {
            Some(s) => solve_best(program, &opts, Duration::from_secs(s)),
            None => solve(program, &opts),
        }
        .unwrap_or_else(|e| panic!("{program}: {e}"));
        assert!(sol.proved, "{program} must be proved");
        crate::present::solution_json(&sol, None)
    }

    fn view_subset(v: &Value) -> Value {
        json!({
            "points": v["points"],
            "aux": v["aux"],
            "helpers": v.get("helpers").cloned().unwrap_or(json!([])),
            "proof": {"style": v["proof"]["style"], "steps": v["proof"]["steps"]},
        })
    }

    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    pub fn rendered(name: &str) -> (Value, HumanView) {
        let f = load(name);
        let v = solution(&f.program, f.best_secs);
        let h = of_view(&v["view"]).unwrap_or_else(|| panic!("{name}: the engine wrote no human proof"));
        (v, h)
    }

    pub fn with_human(mut v: Value, h: &HumanView) -> Value {
        v["view"]["human"] = serde_json::to_value(h).unwrap();
        v
    }

    fn check_or_bless(name: &str) {
        let f = load(name);
        let v = solution(&f.program, f.best_secs);
        let h = of_view(&v["view"]).unwrap_or_else(|| panic!("{name}: the engine wrote no human proof"));
        if std::env::var_os("HP_BLESS").is_some() {
            let out = Fixture { name: f.name.clone(), program: f.program.clone(), best_secs: f.best_secs, view: view_subset(&v["view"]), human: h.clone() };
            std::fs::write(dir().join(format!("{name}.json")), format!("{}\n", serde_json::to_string_pretty(&out).unwrap())).unwrap();
            for lang in [Lang::En, Lang::Ro] {
                std::fs::write(dir().join(format!("{name}.{}.txt", lang.code())), format!("{}\n", text(&h, &out.view, lang))).unwrap();
            }
            return;
        }
        assert_eq!(
            serde_json::to_value(&h).unwrap(),
            serde_json::to_value(&f.human).unwrap(),
            "{name}: the engine's human proof changed; review it and rerun with HP_BLESS=1 to regenerate tests/fixtures/human"
        );
        assert_eq!(view_subset(&v["view"]), f.view, "{name}: the derivation the fixture points into changed; HP_BLESS=1 regenerates it");
    }

    #[test]
    fn fixtures_are_the_engines_current_output() {
        for name in FAST {
            check_or_bless(name);
        }
    }

    #[test]
    #[ignore = "a 30 s shortest-proof search; run with --ignored (HP_BLESS=1 regenerates)"]
    fn the_shortest_proof_fixture_is_the_engines_current_output() {
        check_or_bless("imo-2023-p2");
    }

    #[test]
    fn fixtures_render_as_their_expected_text_in_both_languages() {
        for name in ALL {
            let f = load(name);
            for lang in [Lang::En, Lang::Ro] {
                let got = text(&f.human, &f.view, lang);
                let path = dir().join(format!("{name}.{}.txt", lang.code()));
                let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(norm(&got), norm(&want), "{name} ({}) differs from {}", lang.code(), path.display());
                assert!(got.trim_end().ends_with('\u{220e}'), "{name}: the proof ends with ∎");
                assert!(!got.contains("{") && !got.contains(i18n::t(lang, "__missing__")), "{name}: an unfilled template in\n{got}");
            }
        }
    }

    #[test]
    fn fixtures_round_trip_in_the_engine_json_shape() {
        for name in ALL {
            let f = load(name);
            let v = serde_json::to_value(&f.human).unwrap();
            let back: HumanView = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(back, f.human, "{name}");
            assert!(f.human.blocks.last().is_some_and(|b| b.kind == BlockKind::Conclusion), "{name}: last block is the conclusion");
            let raw = serde_json::to_string(&v).unwrap();
            for engine_only in ["\"engine_facts\"", "\"engine_line\"", "\"table\"", "\"eq\":\""] {
                assert!(!raw.contains(engine_only), "{name}: {engine_only} is not part of the engine's JSON");
            }
        }
    }

    fn all_reasons(h: &HumanView) -> Vec<Reason> {
        fn add(out: &mut Vec<Reason>, r: &Reason) {
            out.push(r.clone());
            if let Reason::Fact { because, .. } = r {
                because.iter().for_each(|b| add(out, b));
            }
        }
        let mut out = Vec::new();
        for b in &h.blocks {
            for s in &b.body {
                let rs: Vec<&Reason> = match s {
                    Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().flat_map(|l| l.reasons.iter()).collect(),
                    Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => reasons.iter().collect(),
                    Sentence::Raw { .. } => vec![],
                };
                rs.into_iter().for_each(|r| add(&mut out, r));
            }
        }
        out
    }

    fn engine_internal(name: &str) -> bool {
        name.contains('_') || name.chars().next().is_some_and(char::is_lowercase)
    }

    #[test]
    fn fixtures_point_at_real_steps_and_name_only_shown_points() {
        for name in ALL {
            let f = load(name);
            let steps: BTreeSet<u64> = f.view["proof"]["steps"].as_array().unwrap().iter().filter_map(|s| s["n"].as_u64()).collect();
            let names: BTreeSet<String> = f.view["points"].as_array().unwrap().iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
            let mut claims_seen: BTreeSet<u16> = BTreeSet::new();
            for b in &f.human.blocks {
                assert!(!b.engine_steps.is_empty() || b.kind == BlockKind::Conclusion, "{name}: block {} has no derivation chips", b.id);
                for n in &b.engine_steps {
                    assert!(steps.contains(&(*n as u64)), "{name}: block {} cites step {n}, not in the displayed derivation", b.id);
                }
                for pt in b.points.iter().chain(b.stmt.points.iter()) {
                    assert!(names.contains(pt), "{name}: block {} names point {pt:?} the figure lacks ({names:?})", b.id);
                    assert!(!engine_internal(pt), "{name}: engine-internal name {pt:?}");
                }
                if b.kind == BlockKind::Claim {
                    claims_seen.insert(b.n.expect("a claim has a number"));
                }
            }
            for r in all_reasons(&f.human) {
                match r {
                    Reason::Claim { n, .. } => assert!(claims_seen.contains(&n), "{name}: cites Claim {n}, which is never stated"),
                    Reason::Hyp { step: Some(n), .. } | Reason::Fact { step: Some(n), .. } | Reason::Engine { step: Some(n) } => {
                        assert!(steps.contains(&(n as u64)), "{name}: a reason cites step {n}, not in the displayed derivation")
                    }
                    Reason::Atom { args, .. } => {
                        for a in &args {
                            assert!(names.contains(a) && !engine_internal(a), "{name}: atom argument {a:?}");
                        }
                    }
                    _ => {}
                }
            }
            for lang in [Lang::En, Lang::Ro] {
                let t = text(&f.human, &f.view, lang);
                for bad in ["P\u{2081}", "P\u{2082}", "_"] {
                    assert!(!t.contains(bad), "{name}: helper name {bad:?} reached the text:\n{t}");
                }
            }
        }
    }

    #[test]
    fn english_text_keeps_the_chains_and_claims() {
        for name in ALL {
            let f = load(name);
            let en = text(&f.human, &f.view, Lang::En);
            for b in &f.human.blocks {
                if b.kind == BlockKind::Claim {
                    assert!(en.contains(&format!("Claim {}.", b.n.unwrap())), "{name}: Claim {:?}", b.n);
                }
                for s in &b.body {
                    if let Sentence::Chain { terms, .. } | Sentence::Computation { terms, .. } = s {
                        for t in terms {
                            assert!(en.contains(t.as_str()), "{name}: chain term {t:?} missing from\n{en}");
                        }
                    }
                }
            }
        }
    }

    const ENGLISH: &[&str] = &[
        " the ", " of ", " with ", " from ", "Claim", "Proof", "Let ", " is ", " are ", " by ", "By ", " gives ", " shows ", "which", " so ", " and ",
        " lies ", " lie ", "Finally", "Angle chasing", "respect", "inscribed", "tangent ", "diameter", "bisects", "bisecting", "concyclic", "collinear",
        "radii", "power of", "midline", "criterion", "theorem", "As drawn", "denotes", "directed", "centre", "circle ", "step ", "Hence", "Apply",
        "law of", "sine of", "equal ", " in \u{25b3}", "something went wrong",
    ];

    #[test]
    fn romanian_human_proofs_have_no_english_left_over() {
        for name in ALL {
            let f = load(name);
            let ro = text(&f.human, &f.view, Lang::Ro);
            let padded = format!(" {ro} ");
            for w in ENGLISH {
                assert!(!padded.contains(w), "{name}: English {w:?} left in\n{ro}");
            }
        }
    }

    fn atom_stmt() -> Stmt {
        Stmt { kind: "perp".into(), args: vec!["AB".into(), "CD".into()], points: vec!["A".into(), "B".into(), "C".into(), "D".into()] }
    }

    fn sample_atom(key: AtomKey) -> Reason {
        let args: Vec<String> = ["A", "B", "C", "D", "E", "F"].iter().map(|s| s.to_string()).collect();
        Reason::Atom { key, args, stmt: atom_stmt(), from: vec![], circle: None }
    }

    fn one_block(body: Vec<Sentence>, setup: Vec<SetupLine>) -> HumanView {
        HumanView {
            version: 1,
            as_drawn: false,
            setup,
            blocks: vec![Block { id: 1, kind: BlockKind::Conclusion, n: None, stmt: atom_stmt(), body, engine_steps: vec![1], points: vec![], objects: vec![] }],
            metrics: Metrics::default(),
        }
    }

    #[test]
    fn every_atom_theorem_and_statement_reads_as_words_in_both_languages() {
        let view = json!({"points": [], "aux": [], "proof": {"steps": []}});
        let stmts: Vec<Stmt> = ["coll", "cyclic", "perp", "para", "eqangle", "aconst", "cong", "eqratio", "rconst", "simtri", "contri", "oncircle", "tangent", "radical_axis", "coincide", "formula", "eq", "bisects"]
            .iter()
            .map(|k| Stmt { kind: k.to_string(), args: vec!["AB".into(), "CD".into(), "2".into()], points: vec!["A".into(), "B".into(), "C".into(), "D".into()] })
            .collect();
        let mut body: Vec<Sentence> = AtomKey::ALL.iter().map(|k| Sentence::Because { stmt: atom_stmt(), reasons: vec![sample_atom(*k), sample_atom(*k)], combination: vec![] }).collect();
        body.extend(TheoremKey::ALL.iter().map(|k| Sentence::Theorem { key: *k, stmt: atom_stmt(), reasons: vec![] }));
        body.extend(stmts.iter().map(|s| Sentence::Pooled { stmt: s.clone(), reasons: vec![Reason::Hyp { stmt: s.clone(), step: None }], combination: vec![] }));
        let h = one_block(body, vec![SetupLine::DirectedAngles]);
        for lang in [Lang::En, Lang::Ro] {
            let t = text(&h, &view, lang);
            assert!(!t.contains('{') && !t.contains(i18n::t(lang, "__missing__")), "{lang:?}: a key is missing:\n{t}");
            if lang == Lang::Ro {
                let padded = format!(" {t} ");
                for w in ENGLISH {
                    assert!(!padded.contains(w), "English {w:?} left in\n{t}");
                }
            }
        }
        let js = include_str!("../assets/i18n.js");
        let rs = include_str!("i18n.rs");
        let in_rs: BTreeSet<&str> = rs.split('"').filter(|s| s.starts_with("hp.") && !s.contains(' ')).collect();
        assert!(in_rs.len() > 60, "{}", in_rs.len());
        for k in AtomKey::ALL.iter().map(|k| format!("hp.atom.{}", k.key())).chain(TheoremKey::ALL.iter().map(|k| format!("hp.thm.{}", k.key()))) {
            assert!(in_rs.contains(k.as_str()), "{k} missing from i18n.rs");
        }
        for k in in_rs {
            for lang in [Lang::En, Lang::Ro] {
                assert!(has(lang, k), "{k} in only one server catalogue ({} missing)", lang.code());
            }
            assert_eq!(js.matches(&format!("\"{k}\":")).count(), 2, "{k} must be in both i18n.js catalogues");
        }
    }

    #[test]
    fn point_named_circles_are_one_sentence() {
        let c = |name: &str, through: &[&str], centre: &str| SetupLine::Circle {
            name: name.into(),
            through: through.iter().map(|s| s.to_string()).collect(),
            centre: Some(centre.into()),
            diameter: None,
        };
        let h = one_block(vec![], vec![c("(BMR)", &["B", "M", "R", "P"], "O\u{2081}"), c("(CNR)", &["C", "N", "R", "P"], "O\u{2082}")]);
        let view = json!({"points": [], "aux": [], "proof": {"steps": []}});
        let d = doc(&h, &view, Lang::En);
        assert_eq!(d.setup, vec!["P lies on (BMR) and (CNR), with centers O\u{2081} and O\u{2082} respectively.".to_string()]);
        let ro = doc(&h, &view, Lang::Ro);
        assert_eq!(ro.setup, vec!["P se află pe (BMR) și (CNR), cu centrele O\u{2081} și O\u{2082}, respectiv.".to_string()]);
    }

    #[test]
    fn a_block_never_gives_its_own_statement_as_its_reason() {
        let cong = Stmt { kind: "contri".into(), args: vec!["OBF".into(), "ONF".into()], points: vec!["B".into(), "F".into(), "N".into(), "O".into()] };
        let bis = Stmt { kind: "bisects".into(), args: vec!["FO".into(), "\u{2220}BFN".into()], points: vec!["B".into(), "F".into(), "N".into(), "O".into()] };
        let own = Sentence::Because { stmt: cong.clone(), reasons: vec![Reason::Fact { stmt: cong.clone(), step: None, block: None, because: vec![] }], combination: vec![] };
        let used = Sentence::Because { stmt: bis.clone(), reasons: vec![Reason::Fact { stmt: cong.clone(), step: None, block: None, because: vec![] }], combination: vec![] };
        let mut h = one_block(vec![own, used], vec![]);
        h.blocks[0].stmt = cong;
        let t = text(&h, &json!({"points": [], "aux": [], "proof": {"steps": []}}), Lang::En);
        assert!(!t.contains("\u{25b3}OBF \u{2245} \u{25b3}ONF (\u{25b3}OBF \u{2245} \u{25b3}ONF)") && !t.starts_with("\u{25b3}OBF"), "{t}");
        assert!(t.contains("FO bisects \u{2220}BFN (\u{25b3}OBF \u{2245} \u{25b3}ONF)"), "{t}");
    }

    #[test]
    fn the_imo_2004_setup_names_the_bisector_not_its_helper_point() {
        let f = load("imo-2004-p1");
        let en = text(&f.human, &f.view, Lang::En);
        assert!(en.contains("perpendicular from B to AR"), "{en}");
        assert!(en.contains("AR bisects \u{2220}BAC"), "{en}");
    }

    #[test]
    fn the_report_shows_the_human_proof_and_the_derivation_only_on_request() {
        for name in ["orthocenter-reflection", "imo-2004-p1", "orthocenter-vertex-distance"] {
            let (v, _) = rendered(name);
            let mut plain = v.clone();
            plain["view"]["human"] = Value::Null;
            let plain = crate::render::report_from_json(&plain, Lang::En, false);
            for lang in [Lang::En, Lang::Ro] {
                let short = crate::render::report_from_json(&v, lang, false);
                let full = crate::render::report_from_json(&v, lang, true);
                let derivation = i18n::t(lang, "report.hp.derivation").to_uppercase();
                let proof = i18n::t(lang, "report.hp.proof").to_uppercase();
                assert!(short.contains(&format!(">{proof}<")), "{name}: the human proof section is there");
                assert!(!short.contains(&derivation), "{name}: no appendix unless asked");
                assert!(full.contains(&derivation), "{name}: the appendix when asked");
                assert!(full.len() > short.len());
                assert!(short.contains('\u{220e}'));
                assert!(crate::render::report_pdf_from_json(&v, lang, true).is_ok_and(|b| b.starts_with(b"%PDF")));
            }
            assert!(plain.contains(&i18n::t(Lang::En, "report.proof").to_uppercase()), "{name}: without a human proof the report is as before");
            assert!(!plain.contains(&i18n::t(Lang::En, "report.hp.derivation").to_uppercase()));
        }
    }


    fn st(kind: &str, args: &[&str], points: &[&str]) -> Stmt {
        Stmt { kind: kind.into(), args: args.iter().map(|s| s.to_string()).collect(), points: points.iter().map(|s| s.to_string()).collect() }
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn arg(key: &str, pts: &[&str]) -> AuxArg {
        AuxArg { key: key.into(), pts: strs(pts) }
    }

    fn aux_samples() -> Vec<(&'static str, Vec<AuxArg>)> {
        vec![
            ("aux.midpoint", vec![arg("segment", &["B", "C"])]),
            ("aux.foot", vec![arg("point", &["A"]), arg("line", &["B", "C"])]),
            ("aux.reflect", vec![arg("point", &["H"]), arg("line", &["B", "C"])]),
            ("aux.reflect", vec![arg("point", &["A"]), arg("point", &["O"])]),
            ("aux.circumcenter", vec![arg("triangle", &["A", "B", "H"])]),
            ("aux.orthocenter", vec![arg("triangle", &["A", "B", "C"])]),
            ("aux.incenter", vec![arg("triangle", &["A", "B", "C"])]),
            ("aux.excenter", vec![arg("triangle", &["A", "B", "C"]), arg("point", &["A"])]),
            ("aux.excenter_any", vec![arg("triangle", &["A", "B", "C"])]),
            ("aux.centroid", vec![arg("triangle", &["A", "B", "C"])]),
            ("aux.bisector_foot", vec![arg("point", &["A"]), arg("triangle", &["A", "B", "C"])]),
            ("aux.antipode_on", vec![arg("point", &["A"]), arg("aux.circumcircle", &["A", "B", "C"])]),
            ("aux.antipode_on", vec![arg("point", &["D"]), arg("aux.circumcircle", &["D", "E", "F"])]),
            ("aux.parallelogram", vec![arg("triangle", &["A", "B", "C"])]),
            ("aux.incircle_touch", vec![arg("point", &["A"]), arg("triangle", &["A", "B", "C"])]),
            ("aux.excircle_touch", vec![arg("point", &["A"]), arg("triangle", &["A", "B", "C"])]),
            ("aux.spiral_center", vec![arg("segment", &["A", "B"]), arg("segment", &["C", "D"])]),
            ("aux.isogonal", vec![arg("point", &["P"]), arg("triangle", &["A", "B", "C"])]),
            ("aux.inverse", vec![arg("point", &["P"]), arg("aux.circle", &["O", "A"])]),
            ("aux.pole", vec![arg("line", &["B", "C"]), arg("aux.circumcircle", &["A", "B", "C"])]),
            ("aux.tangent", vec![arg("point", &["P"]), arg("aux.circle", &["Q", "D"])]),
            ("aux.arc_midpoint", vec![arg("segment", &["B", "C"]), arg("aux.circumcircle", &["A", "B", "C"])]),
            ("aux.harmonic", vec![arg("point", &["D"]), arg("point", &["B"]), arg("point", &["C"])]),
            ("aux.intersect", vec![arg("aux.line.para", &["D", "B", "C"]), arg("line", &["A", "B"])]),
            ("aux.intersect", vec![arg("aux.line.perp", &["H", "A", "C"]), arg("aux.line.tangent_at", &["A", "O"])]),
            ("aux.intersect", vec![arg("aux.line.isogonal", &["A", "P", "B", "A", "C"]), arg("line", &["B", "C"])]),
            ("aux.intersect2", vec![arg("line", &["A", "D"]), arg("aux.circumcircle", &["A", "B", "C"]), arg("point", &["A"])]),
            ("aux.intersect2", vec![arg("aux.circumcircle", &["A", "B", "C"]), arg("aux.circle", &["Q", "D"]), arg("point", &["D"])]),
        ]
    }

    fn synthetic() -> (HumanView, Value) {
        let letters = ["A", "B", "C", "D", "E", "F", "G", "H", "O", "Q", "P", "X", "Y", "Z", "U", "V", "W", "K", "L", "M", "N", "S", "T"];
        let view = json!({"points": letters.iter().map(|n| json!({"name": n, "aux": false})).collect::<Vec<_>>(), "aux": [], "helpers": [], "proof": {"style": "ddar", "steps": []}});
        let names = ["X", "Y", "Z", "W", "L", "M", "N", "T"];
        let mut setup = vec![
            SetupLine::DirectedAngles,
            SetupLine::Notation { triangle: ["A".into(), "B".into(), "C".into()], circumcentre: Some("O".into()) },
            SetupLine::Notation { triangle: ["D".into(), "E".into(), "F".into()], circumcentre: None },
            SetupLine::Circle { name: "Ω".into(), through: strs(&["A", "B", "C", "S"]), centre: Some("O".into()), diameter: None },
            SetupLine::Circle { name: "ω₁".into(), through: strs(&["D", "E", "F", "G"]), centre: None, diameter: None },
            SetupLine::Helper { point: "U".into(), meaning: HelperMeaning::Perp { through: "A".into(), to: ["B".into(), "C".into()] } },
            SetupLine::Helper { point: "V".into(), meaning: HelperMeaning::Para { through: "D".into(), to: ["E".into(), "F".into()] } },
        ];
        for (i, (key, args)) in aux_samples().into_iter().enumerate() {
            setup.push(SetupLine::Aux { point: names[i % names.len()].into(), aux_index: i, wording: Some(AuxWording { key: key.into(), args }) });
        }
        let lemma_stmt = st("eqangle", &["∡ABX", "∡XCA"], &["A", "B", "C", "X"]);
        let atom = |key: AtomKey, args: &[&str], s: Stmt, circle: Option<&str>| Reason::Atom { key, args: strs(args), stmt: s, from: vec![], circle: circle.map(str::to_string) };
        let claim = Block {
            id: 1,
            kind: BlockKind::Claim,
            n: Some(1),
            stmt: lemma_stmt.clone(),
            body: vec![Sentence::Chain {
                terms: strs(&["∡ABX", "∡ACX", "∡XCA"]),
                links: vec![
                    Link { reasons: vec![atom(AtomKey::Inscribed, &["A", "B", "C", "X"], st("eqangle", &["∡ABX", "∡ACX"], &["A", "B", "C", "X"]), Some("Ω"))], combination: vec![] },
                    Link { reasons: vec![Reason::Hyp { stmt: st("cyclic", &["A", "B", "C", "X"], &["A", "B", "C", "X"]), step: None }], combination: vec![] },
                ],
                then: Some(lemma_stmt.clone()),
                directed: true,
            }],
            engine_steps: vec![],
            points: strs(&["A", "B", "C", "X"]),
            objects: vec![],
        };
        let radii = st("cong", &["QD", "QE"], &["D", "E", "Q"]);
        let body = vec![
            Sentence::Chain {
                terms: strs(&["∡DEG", "∡DFG", "∡QDE + 90°"]),
                links: vec![
                    Link { reasons: vec![Reason::Lemma { stmt: st("eqangle", &["∡DEG", "∡DFG"], &["D", "E", "F", "G"]), block: 2, sentence: 1 }], combination: vec![] },
                    Link {
                        reasons: vec![
                            atom(AtomKey::Radii, &["Q", "D", "E", "F", "G"], radii.clone(), None),
                            atom(AtomKey::Radii, &["O", "A", "B", "C"], st("cong", &["OA", "OB"], &["A", "B", "O"]), None),
                            atom(AtomKey::Isosceles, &["H", "A", "B", "C"], st("cong", &["HA", "HB"], &["A", "B", "H"]), None),
                            atom(AtomKey::Isosceles, &["K", "A", "B"], st("cong", &["KA", "KB"], &["A", "B", "K"]), None),
                            atom(AtomKey::Radii, &["K", "A", "C"], st("cong", &["KA", "KC"], &["A", "C", "K"]), None),
                            atom(AtomKey::TangentChord, &["A", "Y", "B", "C", "O"], st("eqangle", &["∡YAB", "∡ACB"], &["A", "B", "C", "Y"]), Some("Ω")),
                        ],
                        combination: vec![],
                    },
                ],
                then: Some(st("eqangle", &["∡DEG", "∡QDE + 90°"], &["D", "E", "G", "Q"])),
                directed: true,
            },
            Sentence::Because { stmt: st("eqangle", &["∡DEG", "∡DFG"], &["D", "E", "F", "G"]), reasons: vec![Reason::Hyp { stmt: st("cyclic", &["D", "E", "F", "G"], &["D", "E", "F", "G"]), step: None }], combination: vec![] },
            Sentence::Because { stmt: st("perp", &["AB", "CX"], &["A", "B", "C", "X"]), reasons: vec![Reason::Lemma { stmt: lemma_stmt.clone(), block: 1, sentence: 0 }, Reason::Claim { n: 1, block: 1, step: None }], combination: vec![] },
            Sentence::Theorem { key: TheoremKey::LawOfSines, stmt: st("formula", &["law of sines in △ABC"], &["A", "B", "C"]), reasons: vec![] },
            Sentence::Theorem { key: TheoremKey::LawOfSines, stmt: st("formula", &["law of sines in △DEF"], &["D", "E", "F"]), reasons: vec![Reason::Hyp { stmt: st("perp", &["DE", "EF"], &["D", "E", "F"]), step: None }] },
            Sentence::Computation {
                comp: CompKind::Trig,
                terms: strs(&["AB", "2R·sin C"]),
                links: vec![Link { reasons: vec![Reason::Fact { stmt: st("formula", &["law of sines in △ABC"], &["A", "B", "C"]), step: None, block: None, because: vec![] }, Reason::Fact { stmt: st("eq", &["sin∠AXB", "sin C"], &["A", "B", "C", "X"]), step: None, block: None, because: vec![] }], combination: vec![] }],
            },
            Sentence::Chain {
                terms: strs(&["∡EDF", "∡EGF"]),
                links: vec![Link { reasons: vec![Reason::Hyp { stmt: st("cyclic", &["D", "E", "F", "G"], &["D", "E", "F", "G"]), step: None }], combination: vec![] }],
                then: Some(st("eqangle", &["∡EGF", "∡EDF"], &["D", "E", "F", "G"])),
                directed: true,
            },
        ];
        let conclusion = Block { id: 2, kind: BlockKind::Conclusion, n: None, stmt: st("perp", &["AB", "CX"], &["A", "B", "C", "X"]), body, engine_steps: vec![], points: vec![], objects: vec![] };
        (HumanView { version: 1, as_drawn: false, setup, blocks: vec![claim, conclusion], metrics: Metrics::default() }, view)
    }

    #[test]
    fn every_q2_field_reads_as_words_in_both_languages() {
        let (h, view) = synthetic();
        let covered: BTreeSet<&str> = aux_samples().iter().map(|s| s.0).collect();
        for k in ddar::human::aux::KEYS.iter().chain(["aux.intersect2"].iter()) {
            assert!(covered.contains(k), "{k}: the engine emits this wording key; add a sample");
        }
        let cx_lines = |lang: Lang| doc(&h, &view, lang).setup;
        for lang in [Lang::En, Lang::Ro] {
            let setup = cx_lines(lang);
            let aux = aux_samples().len();
            assert_eq!(setup.len(), 1 + 2 + 1 + 2 + aux, "{lang:?}: a setup line was dropped:\n{}", setup.join("\n"));
            let t = text(&h, &view, lang);
            for raw in ["aux.", "{", "(", "intersect(", "circumcircle(", "_"] {
                let found = if raw == "(" { t.contains("foot(") || t.contains("midpoint(") || t.contains("circle(") } else { t.contains(raw) };
                assert!(!found, "{lang:?}: raw syntax {raw:?} in\n{t}");
            }
            for want in ["Ω", "ω₁"] {
                assert!(t.contains(want), "{lang:?}: {want} missing from\n{t}");
            }
            if lang == Lang::Ro {
                let padded = format!(" {t} ");
                for w in ENGLISH.iter().chain([" above", "Write ", " point other", "law of sines", "midpoint", "foot of", "antipode", "parallelogram"].iter()) {
                    assert!(!padded.contains(w), "English {w:?} left in\n{t}");
                }
            }
        }
        let en = text(&h, &view, Lang::En);
        for want in [
            "Write A, B, C for the angles of triangle ABC and R = OA = OB = OC for its circumradius.",
            "Write D, E, F for the angles of triangle DEF.",
            "Let Ω be the circumcircle of triangle ABC, with center O; S lies on Ω. Let ω₁ be the circle (DEF); G lies on ω₁.",
            "Let U be a point other than A on the perpendicular from A to BC.",
            "Let V be a point other than D on the parallel to EF through D.",
            "the point such that ABC",
            "the midpoint of arc BC of Ω",
            "antipode of A on Ω",
            "∡DEG = ∡DFG, shown above",
            "∡ABX = ∡XCA, shown in the proof of Claim 1",
            "QD = QE = QF = QG",
            "radii of Ω",
            "HA = HB = HC",
            "KA = KB = KC",
            "tangent to Ω",
            "D, E, F, G on ω₁",
            "Apply the law of sines in △ABC.",
            "Apply the law of sines in △DEF (DE ⟂ EF).",
            "[law of sines in △ABC, sin∠AXB = sin C]",
            "∡EDF = ∡EGF (D, E, F, G on ω₁).",
        ] {
            assert!(en.contains(want), "{want:?} missing from\n{en}");
        }
        let ro = text(&h, &view, Lang::Ro);
        for want in ["teorema sinusurilor în △ABC", ", arătată mai sus", "Notăm cu A, B, C unghiurile triunghiului ABC și cu R = OA = OB = OC raza cercului circumscris.", "Fie Ω cercul circumscris triunghiului ABC"] {
            assert!(ro.contains(want), "{want:?} missing from\n{ro}");
        }
        let path = dir().join("synthetic-wording.json");
        if std::env::var_os("HP_BLESS").is_some() {
            let out = Fixture { name: "synthetic-wording".into(), program: String::new(), best_secs: None, view: view.clone(), human: h.clone() };
            std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&out).unwrap())).unwrap();
            for lang in [Lang::En, Lang::Ro] {
                std::fs::write(dir().join(format!("synthetic-wording.{}.txt", lang.code())), format!("{}\n", text(&h, &view, lang))).unwrap();
            }
        }
        let f = load("synthetic-wording");
        assert_eq!(f.human, h, "{}: HP_BLESS=1 rewrites it", path.display());
        for lang in [Lang::En, Lang::Ro] {
            let want = std::fs::read_to_string(dir().join(format!("synthetic-wording.{}.txt", lang.code()))).unwrap();
            assert_eq!(norm(&text(&f.human, &f.view, lang)), norm(&want), "synthetic-wording ({})", lang.code());
        }
    }

    #[test]
    fn lemma_reasons_and_new_setup_lines_round_trip() {
        let (h, _) = synthetic();
        let v = serde_json::to_value(&h).unwrap();
        let back: HumanView = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(back, h);
        let raw = serde_json::to_string(&v).unwrap();
        assert!(raw.contains("\"lemma\":true") && raw.contains("\"sentence\":1") && raw.contains("\"kind\":\"notation\"") && raw.contains("\"wording\":{"), "{raw}");
        let fact: Reason = serde_json::from_value(json!({"kind": "cong", "args": ["AB", "CD"], "points": ["A", "B", "C", "D"], "step": 3, "block": null, "because": []})).unwrap();
        assert!(matches!(fact, Reason::Fact { step: Some(3), .. }), "a plain fact reason stays a fact");
    }

    fn js_values(js: &str, key: &str) -> Vec<String> {
        let probe = format!("\"{key}\": ");
        js.match_indices(&probe)
            .filter_map(|(i, _)| {
                let rest = &js[i + probe.len()..];
                let mut de = serde_json::Deserializer::from_str(rest).into_iter::<String>();
                de.next().and_then(Result::ok)
            })
            .collect()
    }

    #[test]
    fn every_proof_wording_key_is_romanian_in_romanian_and_the_same_in_both_catalogues() {
        let rs = include_str!("i18n.rs");
        let js = include_str!("../assets/i18n.js");
        let mut keys: BTreeSet<String> = rs.split('"').filter(|s| s.starts_with("hp.") && !s.contains(' ')).map(str::to_string).collect();
        for k in ddar::human::aux::KEYS.iter().chain(["aux.intersect2", "aux.intersect2_shape", "aux.same", "aux.circumcircle", "aux.circle", "aux.line.para", "aux.line.perp", "aux.line.tangent_at", "aux.line.isogonal"].iter()) {
            keys.insert(k.to_string());
        }
        for k in &keys {
            let (en, ro) = (i18n::t(Lang::En, k), i18n::t(Lang::Ro, k));
            assert!(has(Lang::En, k) && has(Lang::Ro, k), "{k}: missing from a server catalogue");
            let padded = format!(" {ro} ");
            for w in ENGLISH.iter().filter(|w| !["respect", "centre"].contains(*w)).chain([" above", "Write ", "point other", "midpoint", "foot of", "antipode", "parallelogram", " center", "circumcircle", "intersection", "respect to", "respectively"].iter()) {
                assert!(!padded.contains(w), "{k}: English {w:?} in the Romanian text {ro:?}");
            }
            assert_eq!(js_values(js, k), vec![en.to_string(), ro.to_string()], "{k}: i18n.js must carry the same EN and RO text as i18n.rs");
        }
    }

    #[test]
    fn a_missing_human_proof_is_none() {
        assert!(of_view(&json!({"human": null})).is_none());
        assert!(of_view(&json!({})).is_none());
    }
}
