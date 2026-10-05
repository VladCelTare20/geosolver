use crate::predicate::PointId;
use crate::proof::FactId;
use crate::rational::Rat;
use serde::{Serialize, Serializer};

impl Serialize for Rat {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Line = (PointId, PointId);
pub type Tri = (PointId, PointId, PointId);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HumanProof {
    pub version: u16,
    pub available: bool,
    pub setup: Vec<SetupLine>,
    pub blocks: Vec<Block>,
    pub as_drawn: bool,
    pub metrics: Metrics,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SetupLine {
    DirectedAngles,
    Circle { name: String, through: Vec<PointId>, centre: Option<PointId>, diameter: Option<Line> },
    Aux { point: PointId, aux_index: usize },
    Helper { point: PointId, meaning: HelperMeaning },
    Notation { triangle: Tri, circumcentre: Option<PointId> },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HelperMeaning {
    Midpoint { of: Line },
    Reflection { of: PointId, line: Line },
    Point,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Block {
    pub id: u16,
    pub kind: BlockKind,
    pub stmt: Stmt,
    pub body: Vec<Sentence>,
    pub engine_facts: Vec<FactId>,
    pub points: Vec<PointId>,
    pub objects: Vec<ObjRef>,
    #[serde(skip)]
    pub horizon: FactId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "n", rename_all = "snake_case")]
pub enum BlockKind {
    Claim(u16),
    Step,
    Conclusion,
    Raw,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjRef {
    Circle { through: Vec<PointId> },
    Line { through: Vec<PointId> },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Sentence {
    Chain { terms: Vec<Expr>, links: Vec<Link>, then: Option<Stmt>, directed: bool },
    Because { stmt: Stmt, reasons: Vec<Reason>, combination: Vec<Term> },
    Pooled { stmt: Stmt, reasons: Vec<Reason>, combination: Vec<Term> },
    Theorem { key: TheoremKey, stmt: Stmt, reasons: Vec<Reason> },
    Computation { comp: CompKind, terms: Vec<Expr>, links: Vec<Link> },
    Raw { engine_fact: FactId, cites: Vec<FactId> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompKind {
    Ratio,
    Length,
    Trig,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Link {
    pub reasons: Vec<Reason>,
    pub combination: Vec<Term>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Term {
    pub reason: u16,
    pub row: u16,
    pub coef: Rat,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reason {
    Hyp { stmt: Stmt, fact: FactId },
    Claim { n: u16, block: u16, fact: FactId },
    Atom { key: AtomKey, stmt: Stmt, args: Vec<PointId>, from: Vec<u16> },
    Fact { stmt: Stmt, fact: FactId, block: Option<u16>, because: Vec<Reason> },
    Engine { fact: FactId },
    Lemma { stmt: Stmt, block: u16, sentence: u16 },
}

pub const PENDING_BLOCK: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
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
}

impl AtomKey {
    pub const ALL: [AtomKey; 11] = [
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
    ];
    pub fn as_str(self) -> &'static str {
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
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TheoremKey {
    RadicalAxis,
    ArcChord,
    AngleBisectorThm,
    AngleBisectorThmConverse,
    Intercept,
    Homothety,
    Monge,
    Menelaus,
    MenelausConverse,
    CevaConverse,
    BisectorConcurrency,
    TriangleEquality,
    Pythagoras,
    PerpFromSquares,
    SquaresOfRatio,
    Stewart,
    LengthsFromSquares,
    LawOfSines,
    EqualSines,
    DoubleAngle,
    TripleAngle,
    SineConst,
    SinesConverse,
    PointMerge,
    TangentMerge,
    Congruence,
    Similarity,
    Collinear,
    Concyclic,
    Other,
}

impl TheoremKey {
    pub const ALL: [TheoremKey; 30] = [
        TheoremKey::RadicalAxis,
        TheoremKey::ArcChord,
        TheoremKey::AngleBisectorThm,
        TheoremKey::AngleBisectorThmConverse,
        TheoremKey::Intercept,
        TheoremKey::Homothety,
        TheoremKey::Monge,
        TheoremKey::Menelaus,
        TheoremKey::MenelausConverse,
        TheoremKey::CevaConverse,
        TheoremKey::BisectorConcurrency,
        TheoremKey::TriangleEquality,
        TheoremKey::Pythagoras,
        TheoremKey::PerpFromSquares,
        TheoremKey::SquaresOfRatio,
        TheoremKey::Stewart,
        TheoremKey::LengthsFromSquares,
        TheoremKey::LawOfSines,
        TheoremKey::EqualSines,
        TheoremKey::DoubleAngle,
        TheoremKey::TripleAngle,
        TheoremKey::SineConst,
        TheoremKey::SinesConverse,
        TheoremKey::PointMerge,
        TheoremKey::TangentMerge,
        TheoremKey::Congruence,
        TheoremKey::Similarity,
        TheoremKey::Collinear,
        TheoremKey::Concyclic,
        TheoremKey::Other,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            TheoremKey::RadicalAxis => "radical_axis",
            TheoremKey::ArcChord => "arc_chord",
            TheoremKey::AngleBisectorThm => "angle_bisector_thm",
            TheoremKey::AngleBisectorThmConverse => "angle_bisector_thm_converse",
            TheoremKey::Intercept => "intercept",
            TheoremKey::Homothety => "homothety",
            TheoremKey::Monge => "monge",
            TheoremKey::Menelaus => "menelaus",
            TheoremKey::MenelausConverse => "menelaus_converse",
            TheoremKey::CevaConverse => "ceva_converse",
            TheoremKey::BisectorConcurrency => "bisector_concurrency",
            TheoremKey::TriangleEquality => "triangle_equality",
            TheoremKey::Pythagoras => "pythagoras",
            TheoremKey::PerpFromSquares => "perp_from_squares",
            TheoremKey::SquaresOfRatio => "squares_of_ratio",
            TheoremKey::Stewart => "stewart",
            TheoremKey::LengthsFromSquares => "lengths_from_squares",
            TheoremKey::LawOfSines => "law_of_sines",
            TheoremKey::EqualSines => "equal_sines",
            TheoremKey::DoubleAngle => "double_angle",
            TheoremKey::TripleAngle => "triple_angle",
            TheoremKey::SineConst => "sine_const",
            TheoremKey::SinesConverse => "sines_converse",
            TheoremKey::PointMerge => "point_merge",
            TheoremKey::TangentMerge => "tangent_merge",
            TheoremKey::Congruence => "congruence",
            TheoremKey::Similarity => "similarity",
            TheoremKey::Collinear => "collinear",
            TheoremKey::Concyclic => "concyclic",
            TheoremKey::Other => "other",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Stmt {
    Coll { pts: Vec<PointId> },
    Cyclic { pts: Vec<PointId> },
    Perp { l1: Line, l2: Line },
    Para { l1: Line, l2: Line },
    EqAngle { lhs: Expr, rhs: Expr },
    AngleConst { angle: Expr, degrees: Rat },
    Cong { s1: Line, s2: Line },
    EqRatio { segs: Vec<Line> },
    RatioConst { s1: Line, s2: Line, value: Rat },
    Sim { t1: Tri, t2: Tri, opposite: bool },
    Congruent { t1: Tri, t2: Tri, opposite: bool },
    OnCircle { p: PointId, circle: Vec<PointId> },
    Tangent { p: PointId, line: Line, circle: Vec<PointId> },
    RadicalAxis { x: PointId, u: PointId, v: PointId },
    Coincide { a: PointId, b: PointId },
    Formula { text: String, pts: Vec<PointId> },
    Eq { lhs: Expr, rhs: Expr },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Angle { a: PointId, b: PointId, c: PointId, directed: bool },
    LineAngle { l1: Line, l2: Line, directed: bool },
    Const { degrees: Rat },
    Lin { terms: Vec<(Rat, Expr)> },
    Seg { a: PointId, b: PointId },
    Sq { a: PointId, b: PointId },
    Prod { factors: Vec<(Expr, i32)> },
    Sin { angle: Box<Expr> },
    Cos { angle: Box<Expr> },
    Num { value: Rat },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
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
    #[serde(skip)]
    pub timed_out: bool,
    #[serde(skip)]
    pub micros: u64,
}
