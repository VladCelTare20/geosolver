use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::i18n::{self, Lang};
use crate::render;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanView {
    pub version: u16,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub as_drawn: bool,
    #[serde(default)]
    pub setup: Vec<SetupLine>,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub metrics: Metrics,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Metrics {
    #[serde(default)]
    pub raw_steps: usize,
    #[serde(default)]
    pub blocks: usize,
    #[serde(default)]
    pub claims: usize,
    #[serde(default)]
    pub links: usize,
    #[serde(default)]
    pub sentences: usize,
    #[serde(default)]
    pub fallbacks: usize,
    #[serde(default)]
    pub words_en: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SetupLine {
    DirectedAngles,
    PositiveSines,
    Circle {
        name: String,
        #[serde(default)]
        through: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        centre: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        diameter: Option<[String; 2]>,
    },
    Aux {
        point: String,
        aux_index: usize,
    },
    Helper {
        point: String,
    },
    Fact {
        stmt: Stmt,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stmt {
    pub kind: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub points: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ro: Option<Vec<String>>,
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
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: u16,
    pub kind: BlockKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    #[default]
    Angle,
    Ratio,
    Length,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompKind {
    Ratio,
    Length,
    Trig,
}

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Sentence {
    Chain {
        terms: Vec<String>,
        links: Vec<Link>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        then: Option<Stmt>,
        #[serde(default = "yes")]
        directed: bool,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        as_drawn: bool,
    },
    Because {
        stmt: Stmt,
        reasons: Vec<Reason>,
    },
    Pooled {
        stmt: Stmt,
        reasons: Vec<Reason>,
        #[serde(default)]
        table: Table,
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
        step: usize,
        #[serde(default)]
        cites: Vec<usize>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Link {
    #[serde(default)]
    pub reasons: Vec<Reason>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub combination: Vec<Term>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub coef: String,
    pub eq: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reason {
    Hyp {
        stmt: Stmt,
    },
    Fact {
        stmt: Stmt,
    },
    Claim {
        n: u16,
    },
    Atom {
        key: AtomKey,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stmt: Option<Stmt>,
    },
    Engine {
        step: usize,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AtomKey {
    Inscribed,
    Thales,
    TangentChord,
    PerpBisector,
    Parallel,
    Radii,
    Isosceles,
    CentralAngle,
    PowerOfPoint,
    Midline,
    Orthocentre,
    Congruent,
    Similar,
    EqualArcs,
    LawOfSines,
    DoubleAngle,
    EqualSines,
    Pythagoras,
}

impl AtomKey {
    #[cfg(test)]
    pub const ALL: [AtomKey; 18] = [
        AtomKey::Inscribed,
        AtomKey::Thales,
        AtomKey::TangentChord,
        AtomKey::PerpBisector,
        AtomKey::Parallel,
        AtomKey::Radii,
        AtomKey::Isosceles,
        AtomKey::CentralAngle,
        AtomKey::PowerOfPoint,
        AtomKey::Midline,
        AtomKey::Orthocentre,
        AtomKey::Congruent,
        AtomKey::Similar,
        AtomKey::EqualArcs,
        AtomKey::LawOfSines,
        AtomKey::DoubleAngle,
        AtomKey::EqualSines,
        AtomKey::Pythagoras,
    ];

    pub fn key(self) -> &'static str {
        match self {
            AtomKey::Inscribed => "inscribed",
            AtomKey::Thales => "thales",
            AtomKey::TangentChord => "tangent_chord",
            AtomKey::PerpBisector => "perp_bisector",
            AtomKey::Parallel => "parallel",
            AtomKey::Radii => "radii",
            AtomKey::Isosceles => "isosceles",
            AtomKey::CentralAngle => "central_angle",
            AtomKey::PowerOfPoint => "power_of_point",
            AtomKey::Midline => "midline",
            AtomKey::Orthocentre => "orthocentre",
            AtomKey::Congruent => "congruent",
            AtomKey::Similar => "similar",
            AtomKey::EqualArcs => "equal_arcs",
            AtomKey::LawOfSines => "law_of_sines",
            AtomKey::DoubleAngle => "double_angle",
            AtomKey::EqualSines => "equal_sines",
            AtomKey::Pythagoras => "pythagoras",
        }
    }

    pub fn qualifies_stmt(self) -> bool {
        matches!(self, AtomKey::Parallel | AtomKey::Orthocentre)
    }

    pub fn arity(self) -> usize {
        match self {
            AtomKey::Inscribed | AtomKey::Radii | AtomKey::CentralAngle | AtomKey::EqualArcs | AtomKey::LawOfSines => 1,
            AtomKey::Thales | AtomKey::TangentChord | AtomKey::Isosceles | AtomKey::Midline => 3,
            AtomKey::PerpBisector => 4,
            AtomKey::Parallel | AtomKey::PowerOfPoint | AtomKey::Congruent | AtomKey::Similar => 2,
            AtomKey::Orthocentre | AtomKey::DoubleAngle | AtomKey::EqualSines | AtomKey::Pythagoras => 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TheoremKey {
    RadicalAxis,
    AngleBisector,
    AngleBisectorConverse,
    Intercept,
    Homothety,
    Monge,
    Menelaus,
    MenelausConverse,
    CevaConverse,
    BisectorConcurrency,
    Pythagoras,
    Stewart,
    LawOfSines,
    PowerOfPoint,
    SimAa,
    SimSas,
    SimSss,
    CongSas,
    CongSss,
}

impl TheoremKey {
    #[cfg(test)]
    pub const ALL: [TheoremKey; 19] = [
        TheoremKey::RadicalAxis,
        TheoremKey::AngleBisector,
        TheoremKey::AngleBisectorConverse,
        TheoremKey::Intercept,
        TheoremKey::Homothety,
        TheoremKey::Monge,
        TheoremKey::Menelaus,
        TheoremKey::MenelausConverse,
        TheoremKey::CevaConverse,
        TheoremKey::BisectorConcurrency,
        TheoremKey::Pythagoras,
        TheoremKey::Stewart,
        TheoremKey::LawOfSines,
        TheoremKey::PowerOfPoint,
        TheoremKey::SimAa,
        TheoremKey::SimSas,
        TheoremKey::SimSss,
        TheoremKey::CongSas,
        TheoremKey::CongSss,
    ];

    pub fn key(self) -> &'static str {
        match self {
            TheoremKey::RadicalAxis => "radical_axis",
            TheoremKey::AngleBisector => "angle_bisector",
            TheoremKey::AngleBisectorConverse => "angle_bisector_converse",
            TheoremKey::Intercept => "intercept",
            TheoremKey::Homothety => "homothety",
            TheoremKey::Monge => "monge",
            TheoremKey::Menelaus => "menelaus",
            TheoremKey::MenelausConverse => "menelaus_converse",
            TheoremKey::CevaConverse => "ceva_converse",
            TheoremKey::BisectorConcurrency => "bisector_concurrency",
            TheoremKey::Pythagoras => "pythagoras",
            TheoremKey::Stewart => "stewart",
            TheoremKey::LawOfSines => "law_of_sines",
            TheoremKey::PowerOfPoint => "power_of_point",
            TheoremKey::SimAa => "sim_aa",
            TheoremKey::SimSas => "sim_sas",
            TheoremKey::SimSss => "sim_sss",
            TheoremKey::CongSas => "cong_sas",
            TheoremKey::CongSss => "cong_sss",
        }
    }
}

pub const STMT_KINDS: [&str; 6] = ["coll", "cyclic", "lies_on", "bisects", "tangent", "radical_axis"];

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

fn stmt_vars(s: &Stmt) -> Vec<(&'static str, String)> {
    let a = |i: usize| s.args.get(i).cloned().unwrap_or_default();
    match s.kind.as_str() {
        "coll" | "cyclic" => vec![("pts", s.args.join(", "))],
        "lies_on" => vec![("p", a(0)), ("obj", a(1))],
        "bisects" => vec![("line", a(0)), ("angle", a(1))],
        "tangent" => vec![("line", a(0)), ("circle", a(1)), ("p", a(2))],
        "radical_axis" => vec![("line", a(0)), ("c1", a(1)), ("c2", a(2))],
        _ => vec![],
    }
}

fn stmt_text(s: &Stmt, lang: Lang, ctx: Ctx) -> String {
    if STMT_KINDS.contains(&s.kind.as_str()) {
        let mut base = format!("hp.fact.{}", s.kind);
        if s.kind == "lies_on" && ctx == Ctx::Statement {
            let many = s.args.first().is_some_and(|p| p.contains(','));
            base = format!("{base}.{}", if many { "other" } else { "one" });
        }
        if let Some(t) = first_of(lang, &context_keys(&base, ctx)) {
            return fill(t, &stmt_vars(s));
        }
    }
    render::fact_text(&serde_json::to_value(s).unwrap_or_default(), lang)
}

fn atom_text(key: AtomKey, args: &[String], lang: Lang, ctx: Ctx) -> String {
    let base = format!("hp.atom.{}", key.key());
    let t = first_of(lang, &context_keys(&base, ctx)).unwrap_or("");
    fill_args(t, args)
}

fn reason_text(r: &Reason, lang: Lang, ctx: Ctx) -> String {
    match r {
        Reason::Hyp { stmt } | Reason::Fact { stmt } => stmt_text(stmt, lang, ctx),
        Reason::Claim { n } => i18n::tf(lang, "hp.claim_ref", &[("n", n.to_string())]),
        Reason::Atom { key, args, stmt } => match stmt.as_ref().filter(|_| key.qualifies_stmt() && ctx == Ctx::Note) {
            Some(st) => format!("{}, {}", stmt_text(st, lang, ctx), atom_text(*key, args, lang, ctx)),
            None => atom_text(*key, args, lang, ctx),
        },
        Reason::Engine { step } => i18n::tf(lang, "hp.step_ref", &[("n", step.to_string())]),
    }
}

pub fn join_list(items: &[String], lang: Lang) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{}{}{}", items[..n - 1].join(", "), i18n::t(lang, "hp.and"), items[n - 1]),
    }
}

fn merged_texts(rs: &[Reason], lang: Lang, ctx: Ctx) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut merged: Vec<(AtomKey, usize, Vec<String>)> = Vec::new();
    for r in rs {
        match r {
            Reason::Atom { key, args, stmt: None } if key.arity() == 1 && args.len() == 1 => match merged.iter_mut().find(|(k, _, _)| k == key) {
                Some((_, at, group)) => {
                    group.push(args[0].clone());
                    out[*at] = atom_text(*key, &[join_list(group, lang)], lang, ctx);
                }
                None => {
                    merged.push((*key, out.len(), vec![args[0].clone()]));
                    out.push(reason_text(r, lang, ctx));
                }
            },
            _ => out.push(reason_text(r, lang, ctx)),
        }
    }
    out
}

fn reasons_list(rs: &[Reason], lang: Lang) -> String {
    let mut claims: Vec<u16> = rs
        .iter()
        .filter_map(|r| match r {
            Reason::Claim { n } => Some(*n),
            _ => None,
        })
        .collect();
    claims.sort_unstable();
    claims.dedup();
    let rest: Vec<Reason> = rs.iter().filter(|r| !matches!(r, Reason::Claim { .. })).cloned().collect();
    let mut out = merged_texts(&rest, lang, Ctx::List);
    if !claims.is_empty() {
        let label = if claims.len() == 1 {
            i18n::tf(lang, "hp.claim_ref", &[("n", claims[0].to_string())])
        } else {
            let ns: Vec<String> = claims.iter().map(u16::to_string).collect();
            i18n::tf(lang, "hp.claims_ref", &[("list", join_list(&ns, lang))])
        };
        out.insert(0, label);
    }
    join_list(&out, lang)
}

fn link_note(l: &Link, lang: Lang) -> String {
    merged_texts(&l.reasons, lang, Ctx::Note).join(", ")
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

fn let_line(point: &str, def: &str, lang: Lang) -> String {
    i18n::tf(lang, "hp.setup.let", &[("p", point.to_string()), ("def", article(def, lang))])
}

fn circle_line(name: &str, through: &[String], centre: Option<&str>, diameter: Option<&[String; 2]>, lang: Lang) -> String {
    let in_name: Vec<String> = name.trim_matches(|c| c == '(' || c == ')').chars().map(|c| c.to_string()).collect();
    let named_by_points = name.starts_with('(');
    let mut used: Vec<String> = Vec::new();
    let mut s = if let Some(d) = diameter {
        used.extend(d.iter().cloned());
        i18n::tf(lang, "hp.setup.circle_d", &[("name", name.to_string()), ("d", format!("{}{}", d[0], d[1]))])
    } else if let (true, Some(c)) = (named_by_points, centre) {
        used.extend(in_name.iter().cloned());
        i18n::tf(lang, "hp.setup.centre_of", &[("name", name.to_string()), ("c", c.to_string())])
    } else {
        let first: Vec<String> = through.iter().take(3).cloned().collect();
        used.extend(first.iter().cloned());
        i18n::tf(lang, "hp.setup.circle", &[("name", name.to_string()), ("pts", first.concat())])
    };
    if let Some(c) = centre.filter(|_| !(named_by_points && diameter.is_none())) {
        s.push_str(&i18n::tf(lang, "hp.setup.centre", &[("c", c.to_string())]));
    }
    let others: Vec<String> = through.iter().filter(|p| !used.contains(p) && !in_name.contains(p)).cloned().collect();
    if !others.is_empty() {
        let key = if others.len() == 1 { "hp.setup.on.one" } else { "hp.setup.on.other" };
        s.push_str(&i18n::tf(lang, key, &[("pts", others.join(", ")), ("name", name.to_string())]));
    }
    s.push('.');
    s
}

fn view_aux<'a>(view: &'a Value, field: &str, pred: impl Fn(usize, &Value) -> bool) -> Option<&'a Value> {
    view[field].as_array()?.iter().enumerate().find(|(i, a)| pred(*i, a)).map(|(_, a)| a)
}

pub fn setup_lines(h: &HumanView, view: &Value, lang: Lang) -> Vec<String> {
    let mut out = Vec::new();
    for line in &h.setup {
        let s = match line {
            SetupLine::DirectedAngles => i18n::t(lang, "hp.setup.directed").to_string(),
            SetupLine::PositiveSines => i18n::t(lang, "hp.setup.sines").to_string(),
            SetupLine::Circle { name, through, centre, diameter } => circle_line(name, through, centre.as_deref(), diameter.as_ref(), lang),
            SetupLine::Aux { point, aux_index } => match view_aux(view, "aux", |i, _| i == *aux_index) {
                Some(a) => let_line(point, &render::aux_text(a, lang), lang),
                None => continue,
            },
            SetupLine::Helper { point } => match view_aux(view, "helpers", |_, a| a["name"].as_str() == Some(point.as_str())) {
                Some(a) => let_line(point, &render::aux_text(a, lang), lang),
                None => continue,
            },
            SetupLine::Fact { stmt } => format!("{}.", capitalize(&stmt_text(stmt, lang, Ctx::Statement))),
        };
        out.push(s);
    }
    out
}

fn raw_step<'a>(view: &'a Value, n: usize) -> Option<&'a Value> {
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

fn rows(terms: &[String], links: &[Link], lang: Lang) -> Vec<Row> {
    let mut out = Vec::new();
    for (k, l) in links.iter().enumerate() {
        out.push(Row {
            lhs: if k == 0 { terms.first().cloned().unwrap_or_default() } else { String::new() },
            rhs: terms.get(k + 1).cloned().unwrap_or_default(),
            reason: link_note(l, lang),
        });
    }
    out
}

fn sentence_parts(s: &Sentence, view: &Value, lang: Lang, lead: Option<&str>) -> Vec<Part> {
    let lead = |text: String| match lead {
        Some(l) => format!("{l}{}", lower_first_word(&text)),
        None => text,
    };
    match s {
        Sentence::Chain { terms, links, then, as_drawn, .. } => {
            let so = then.as_ref().map(|t| stmt_text(t, lang, Ctx::Statement));
            if inline_chain(terms, links) {
                let notes: Vec<String> = links.iter().map(|l| link_note(l, lang)).filter(|n| !n.is_empty()).collect();
                let mut text = terms.join(" = ");
                if *as_drawn {
                    text = format!("{}{text}", i18n::t(lang, "hp.as_drawn_lead"));
                }
                if !notes.is_empty() {
                    text.push_str(&format!(" ({})", notes.join("; ")));
                }
                match so {
                    Some(t) => text.push_str(&i18n::tf(lang, "hp.so_inline", &[("stmt", t)])),
                    None => text.push('.'),
                }
                vec![Part::Text(lead(text))]
            } else {
                let mut parts = Vec::new();
                if *as_drawn {
                    parts.push(Part::Text(i18n::t(lang, "hp.as_drawn_display").to_string()));
                }
                parts.push(Part::Rows(rows(terms, links, lang)));
                if let Some(t) = so {
                    parts.push(Part::Text(i18n::tf(lang, "hp.so", &[("stmt", t)])));
                }
                parts
            }
        }
        Sentence::Because { stmt, reasons } => {
            let st = stmt_text(stmt, lang, Ctx::Statement);
            let rs = merged_texts(reasons, lang, Ctx::Note);
            let text = if rs.is_empty() { format!("{st}.") } else { i18n::tf(lang, "hp.because", &[("stmt", st), ("reasons", rs.join("; "))]) };
            vec![Part::Text(lead(capitalize(&text)))]
        }
        Sentence::Pooled { stmt, reasons, table, .. } => {
            let key = match table {
                Table::Angle => "hp.pooled.angle",
                Table::Ratio => "hp.pooled.ratio",
                Table::Length => "hp.pooled.length",
            };
            let text = i18n::tf(lang, key, &[("reasons", reasons_list(reasons, lang)), ("stmt", stmt_text(stmt, lang, Ctx::Statement))]);
            vec![Part::Text(lead(capitalize(&text)))]
        }
        Sentence::Theorem { key, stmt, reasons } => {
            let thm = i18n::t(lang, &format!("hp.thm.{}", key.key())).to_string();
            let st = stmt_text(stmt, lang, Ctx::Statement);
            let text = if reasons.is_empty() {
                i18n::tf(lang, "hp.theorem", &[("thm", thm), ("stmt", st)])
            } else {
                let rs = merged_texts(reasons, lang, Ctx::Note);
                i18n::tf(lang, "hp.theorem_with", &[("thm", thm), ("reasons", rs.join("; ")), ("stmt", st)])
            };
            vec![Part::Text(lead(capitalize(&text)))]
        }
        Sentence::Computation { terms, links, .. } => vec![Part::Rows(rows(terms, links, lang))],
        Sentence::Raw { step, .. } => {
            let text = match raw_step(view, *step) {
                Some(st) => i18n::tf(
                    lang,
                    "hp.raw",
                    &[("fact", render::fact_text(&st["fact"], lang)), ("rule", rule_of(st, lang)), ("n", step.to_string())],
                ),
                None => i18n::tf(lang, "hp.step_ref", &[("n", step.to_string())]),
            };
            vec![Part::Text(lead(capitalize(&text)))]
        }
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
    let claims = h.blocks.iter().filter(|b| b.kind == BlockKind::Claim).count();
    let mut blocks = Vec::new();
    for b in &h.blocks {
        let mut parts = Vec::new();
        for (i, s) in b.body.iter().enumerate() {
            let lead = (i == 0 && b.kind == BlockKind::Conclusion && claims > 0 && matches!(s, Sentence::Pooled { .. }))
                .then(|| i18n::t(lang, "hp.finally"));
            parts.extend(sentence_parts(s, view, lang, lead));
        }
        let (head, stmt, proof_label) = match b.kind {
            BlockKind::Claim => (
                Some(i18n::tf(lang, "hp.claim", &[("n", b.n.unwrap_or(0).to_string())])),
                Some(format!("{}.", capitalize(&stmt_text(&b.stmt, lang, Ctx::Statement)))),
                Some(i18n::t(lang, "hp.proof").to_string()),
            ),
            _ => (None, None, None),
        };
        blocks.push(DocBlock {
            kind: b.kind,
            head,
            stmt,
            proof_label,
            parts: merge_text(parts),
            end: b.kind == BlockKind::Conclusion,
            steps: b.engine_steps.clone(),
        });
    }
    let mut notes = Vec::new();
    if h.as_drawn {
        notes.push(i18n::t(lang, "hp.as_drawn").to_string());
    }
    Doc { setup: setup_lines(h, view, lang), notes, blocks }
}

#[cfg_attr(not(test), allow(dead_code))]
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

pub fn of_view(view: &Value) -> Option<HumanView> {
    let h = view.get("human").filter(|h| !h.is_null())?;
    serde_json::from_value(h.clone()).ok()
}

#[cfg(any(test, feature = "human-fixtures"))]
pub mod fixtures {
    use super::HumanView;
    use serde::Deserialize;
    use std::collections::BTreeMap;
    use std::path::Path;

    #[derive(Deserialize)]
    pub struct Fixture {
        #[cfg_attr(not(test), allow(dead_code))]
        pub name: String,
        pub programs: Vec<Program>,
        pub human: HumanView,
    }

    #[derive(Deserialize)]
    pub struct Program {
        pub geo: String,
        #[serde(default)]
        pub rename: BTreeMap<String, String>,
        #[serde(default)]
        pub setup: Option<Vec<super::SetupLine>>,
    }

    #[cfg_attr(not(feature = "human-fixtures"), allow(dead_code))]
    pub const ENV: &str = "AGSTUDIO_HUMAN_FIXTURES";

    pub fn program_key(src: &str) -> String {
        src.lines()
            .map(|l| l.split('#').next().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(""))
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn rename_value(v: &mut serde_json::Value, map: &BTreeMap<String, String>) {
        match v {
            serde_json::Value::String(s) => {
                for (from, to) in map {
                    *s = s.replace(from.as_str(), to);
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(|x| rename_value(x, map)),
            serde_json::Value::Object(o) => {
                for (k, x) in o.iter_mut() {
                    if k != "kind" && k != "key" && k != "comp" && k != "table" {
                        rename_value(x, map);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn load(path: &Path) -> Result<Fixture, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    #[cfg_attr(not(feature = "human-fixtures"), allow(dead_code))]
    pub fn load_dir(dir: &Path) -> Vec<Fixture> {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "json")).collect())
            .unwrap_or_default();
        paths.sort();
        paths.iter().filter_map(|p| load(p).ok()).collect()
    }

    pub fn for_program(fixtures: &[Fixture], src: &str) -> Option<HumanView> {
        let key = program_key(src);
        for f in fixtures {
            for p in &f.programs {
                if program_key(&p.geo) == key {
                    let mut h = f.human.clone();
                    if let Some(s) = &p.setup {
                        h.setup = s.clone();
                    }
                    let mut v = serde_json::to_value(&h).ok()?;
                    rename_value(&mut v, &p.rename);
                    return serde_json::from_value(v).ok();
                }
            }
        }
        None
    }

    #[cfg(feature = "human-fixtures")]
    pub fn from_env(src: &str) -> Option<HumanView> {
        let dir = std::env::var_os(ENV)?;
        for_program(&load_dir(Path::new(&dir)), src)
    }
}

#[cfg(test)]
pub mod tests {
    use super::fixtures::{self, Fixture};
    use super::*;
    use crate::engine::{solve, SolveOptions};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/human")
    }

    pub fn all() -> Vec<Fixture> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(dir())
            .expect("fixture dir")
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        paths.iter().map(|p| fixtures::load(p).unwrap_or_else(|e| panic!("{e}"))).collect()
    }

    fn solved(src: &str) -> Value {
        let sol = solve(src, &SolveOptions::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert!(sol.proved, "{src} must be proved");
        crate::present::solution_json(&sol, None)
    }

    pub fn with_human(mut v: Value, h: &HumanView) -> Value {
        v["view"]["human"] = serde_json::to_value(h).unwrap();
        v
    }

    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    pub fn rendered(name: &str) -> (Value, HumanView) {
        let fs = all();
        let f = fs.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("no fixture {name}"));
        let p = &f.programs[0];
        let v = solved(&p.geo);
        let h = fixtures::for_program(std::slice::from_ref(f), &p.geo).expect("fixture matches its own program");
        (v, h)
    }

    #[test]
    fn every_fixture_parses_and_round_trips() {
        let fs = all();
        let names: BTreeSet<&str> = fs.iter().map(|f| f.name.as_str()).collect();
        for want in ["euler-line", "imo-2004-p1", "imo-2023-p2", "orthocenter-reflection", "orthocenter-vertex-distance"] {
            assert!(names.contains(want), "missing fixture {want}");
        }
        for f in &fs {
            let v = serde_json::to_value(&f.human).unwrap();
            let back: HumanView = serde_json::from_value(v).unwrap();
            assert_eq!(back, f.human, "{}", f.name);
            assert!(f.human.blocks.last().is_some_and(|b| b.kind == BlockKind::Conclusion), "{}: last block is the conclusion", f.name);
        }
    }

    #[test]
    fn fixtures_point_at_real_steps_points_and_constructions() {
        for f in all() {
            for p in &f.programs {
                let v = solved(&p.geo);
                let view = &v["view"];
                let h = fixtures::for_program(std::slice::from_ref(&f), &p.geo).unwrap();
                let steps: BTreeSet<u64> = view["proof"]["steps"].as_array().unwrap().iter().filter_map(|s| s["n"].as_u64()).collect();
                let names: BTreeSet<String> = view["points"].as_array().unwrap().iter().filter_map(|p| p["name"].as_str().map(str::to_string)).collect();
                let mut claims_seen: BTreeSet<u16> = BTreeSet::new();
                for b in &h.blocks {
                    for n in &b.engine_steps {
                        assert!(steps.contains(&(*n as u64)), "{}: block {} cites step {n}, not in the derivation", f.name, b.id);
                    }
                    for pt in b.points.iter().chain(b.stmt.points.iter()) {
                        assert!(names.contains(pt), "{}: block {} names point {pt:?} the figure lacks ({names:?})", f.name, b.id);
                    }
                    let mut refs: Vec<u16> = Vec::new();
                    for s in &b.body {
                        let rs: Vec<&Reason> = match s {
                            Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => links.iter().flat_map(|l| l.reasons.iter()).collect(),
                            Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } | Sentence::Theorem { reasons, .. } => reasons.iter().collect(),
                            Sentence::Raw { step, .. } => {
                                assert!(steps.contains(&(*step as u64)), "{}: raw step {step}", f.name);
                                vec![]
                            }
                        };
                        for r in rs {
                            match r {
                                Reason::Claim { n } => refs.push(*n),
                                Reason::Atom { key, args, .. } => assert_eq!(args.len(), key.arity(), "{}: atom {key:?} arguments", f.name),
                                _ => {}
                            }
                        }
                    }
                    for n in refs {
                        assert!(claims_seen.contains(&n), "{}: block {} cites Claim {n} before it is stated", f.name, b.id);
                    }
                    if b.kind == BlockKind::Claim {
                        claims_seen.insert(b.n.expect("a claim has a number"));
                    }
                }
                for line in &h.setup {
                    match line {
                        SetupLine::Aux { aux_index, .. } => {
                            assert!(view["aux"].as_array().is_some_and(|a| *aux_index < a.len()), "{}: aux {aux_index}", f.name)
                        }
                        SetupLine::Helper { point } => assert!(
                            view["helpers"].as_array().is_some_and(|a| a.iter().any(|x| x["name"].as_str() == Some(point.as_str()))),
                            "{}: helper {point}",
                            f.name
                        ),
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn fixtures_render_as_their_expected_text_in_both_languages() {
        let bless = std::env::var_os("HP_BLESS").is_some();
        for f in all() {
            let (v, h) = rendered(&f.name);
            for lang in [Lang::En, Lang::Ro] {
                let got = text(&h, &v["view"], lang);
                let path = dir().join(format!("{}.{}.txt", f.name, lang.code()));
                if bless {
                    std::fs::write(&path, format!("{got}\n")).unwrap();
                    continue;
                }
                let want = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                assert_eq!(norm(&got), norm(&want), "{} ({}) differs from {}", f.name, lang.code(), path.display());
                assert!(got.trim_end().ends_with('\u{220e}'), "{}: the proof ends with ∎", f.name);
            }
        }
    }

    #[test]
    fn english_text_keeps_the_golden_chains_and_claims() {
        for f in all() {
            let (v, h) = rendered(&f.name);
            let en = text(&h, &v["view"], Lang::En);
            for b in &h.blocks {
                if b.kind == BlockKind::Claim {
                    assert!(en.contains(&format!("Claim {}.", b.n.unwrap())), "{}: Claim {:?}", f.name, b.n);
                }
                for s in &b.body {
                    if let Sentence::Chain { terms, .. } | Sentence::Computation { terms, .. } = s {
                        for t in terms {
                            assert!(en.contains(t.as_str()), "{}: chain term {t:?} missing from\n{en}", f.name);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn romanian_human_proofs_have_no_english_left_over() {
        const ENGLISH: &[&str] = &[
            " the ", " of ", " with ", " from ", "Claim", "Proof", "Let ", " is ", " are ", " by ", "By ", " gives ", " shows ", "which",
            " so ", " and ", " lies ", " lie ", "Finally", "Angle chasing", "respect", "inscribed", "tangent ", "diameter", "bisects",
            "bisecting", "concyclic", "collinear", "radii", "power of", "midline", "criterion", "theorem", "As drawn", "denotes",
            "directed", "centre", "circle ", "step ",
        ];
        for f in all() {
            let (v, h) = rendered(&f.name);
            let ro = text(&h, &v["view"], Lang::Ro);
            let padded = format!(" {ro} ");
            for w in ENGLISH {
                assert!(!padded.contains(w), "{}: English {w:?} left in\n{ro}", f.name);
            }
        }
    }

    #[test]
    fn every_atom_and_theorem_reads_as_words_in_all_four_catalogues() {
        let js = include_str!("../assets/i18n.js");
        let rs = include_str!("i18n.rs");
        let mut keys: Vec<String> = AtomKey::ALL.iter().map(|k| format!("hp.atom.{}", k.key())).collect();
        keys.extend(TheoremKey::ALL.iter().map(|k| format!("hp.thm.{}", k.key())));
        keys.extend(STMT_KINDS.iter().map(|k| if *k == "lies_on" { format!("hp.fact.{k}.one") } else { format!("hp.fact.{k}") }));
        for k in &keys {
            for lang in [Lang::En, Lang::Ro] {
                assert!(has(lang, k), "{k} missing from i18n.rs ({})", lang.code());
            }
            assert_eq!(js.matches(&format!("\"{k}\":")).count(), 2, "{k} in both i18n.js catalogues");
        }
        let in_rs: BTreeSet<&str> = rs.split('"').filter(|s| s.starts_with("hp.") && !s.contains(' ')).collect();
        assert!(in_rs.len() > 60, "{}", in_rs.len());
        for k in in_rs {
            for lang in [Lang::En, Lang::Ro] {
                assert!(has(lang, k), "{k} in only one server catalogue ({} missing)", lang.code());
            }
            assert_eq!(js.matches(&format!("\"{k}\":")).count(), 2, "{k} must be in both i18n.js catalogues");
        }
    }

    #[test]
    fn the_example_program_gets_the_fixture_with_its_own_point_names() {
        let fs = all();
        let src = "A B C = triangle\nH = orthocenter(A, B, C)\nprove cyclic(A, B, C, reflect(H, line(B, C)))";
        let h = fixtures::for_program(&fs, src).expect("the ortho example has a fixture");
        let json = serde_json::to_string(&h).unwrap();
        assert!(!json.contains("\"K\"") && json.contains("H\u{2032}"), "{json}");
        assert!(h.setup.iter().any(|s| matches!(s, SetupLine::Helper { point } if point == "H\u{2032}")));
        assert!(fixtures::for_program(&fs, "A B C = triangle\nprove coll(A, B, C)").is_none());
        let commented = "# Euler line\nA B C = triangle\nO = circumcenter(A, B, C)   # O\nG = centroid(A, B, C)\nH = orthocenter(A, B, C)\nprove coll(O, G, H)";
        assert!(fixtures::for_program(&fs, commented).is_some(), "comments and spacing do not matter");
    }

    #[test]
    fn the_report_shows_the_human_proof_and_the_derivation_only_on_request() {
        for name in ["orthocenter-reflection", "imo-2004-p1", "orthocenter-vertex-distance"] {
            let (v, h) = rendered(name);
            let plain = crate::render::report_from_json(&v, Lang::En, false);
            let v = with_human(v, &h);
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
}
