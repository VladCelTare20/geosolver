pub mod audit;
pub mod detect;

use crate::human::model::{AtomKey, TheoremKey};
use crate::proof::{Fact, Reason};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Thm {
    AngleCalculus,
    SegmentArithmetic,
    CircleDefinition,
    TrigIdentities,
    InscribedAngle,
    SimilarityAa,
    SimilaritySas,
    SimilaritySss,
    SimilaritySsa,
    TriangleEquality,
    LawOfSines,
    CongruentSss,
    CongruentSas,
    CongruentAsa,
    CongruentRhs,
    Isosceles,
    IsoscelesConverse,
    PerpBisector,
    PerpBisectorLocus,
    Circumcentre,
    Orthocentre,
    Incentre,
    Midline,
    MidlineConverse,
    Centroid,
    Intercept,
    InterceptConverse,
    BisectorThm,
    ExtBisectorThm,
    BisectorConverse,
    ExtBisectorConverse,
    Menelaus,
    MenelausConverse,
    Ceva,
    CevaConverse,
    GeometricMean,
    Pythagoras,
    PythagorasConverse,
    Stewart,
    Apollonius,
    ParallelogramLaw,
    BritishFlag,
    PerpFromCentre,
    Thales,
    ThalesConverse,
    MedianHypotenuse,
    CentralAngle,
    TangentChord,
    InscribedConverse,
    ArcChord,
    ParallelChords,
    PowerOfPoint,
    TangentSecant,
    PowerConverse,
    EqualTangents,
    RadicalAxis,
    Homothety,
    Monge,
    Simson,
    Miquel,
    Reim,
    EulerLine,
    NinePointCircle,
    ExtLawOfSines,
    SinesConverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Calculus,
    Axiom,
    Theorem,
}

pub struct Info {
    pub thm: Thm,
    pub key: &'static str,
    pub en: &'static str,
    pub ro: &'static str,
    pub kind: Kind,
    pub avoidable: bool,
    pub uses: &'static [(Thm, &'static str)],
    pub cited_by: &'static str,
}

use Kind::*;
use Thm::*;

pub const INFO: &[Info] = &[
    Info {
        thm: AngleCalculus,
        key: "angle_calculus",
        en: "angle chasing (parallels, perpendiculars, angle sums)",
        ro: "calculul cu unghiuri (paralele, perpendiculare, suma unghiurilor)",
        kind: Calculus,
        avoidable: false,
        uses: &[],
        cited_by: "the angle table; Reason::Collinear; atoms Parallel, AngleAddition, AngleSum, ExteriorAngle, Supplementary, VerticalAngles, CommonAngle, AlternateAngles, CorrespondingAngles; TheoremKey::Collinear",
    },
    Info {
        thm: SegmentArithmetic,
        key: "segment_arithmetic",
        en: "segment and ratio arithmetic",
        ro: "aritmetica segmentelor și a rapoartelor",
        kind: Calculus,
        avoidable: false,
        uses: &[],
        cited_by: "the ratio, sum and squared-length tables; Reason::TransferAddMul; engine \"squares of proportional lengths\", \"lengths from squared lengths\"",
    },
    Info {
        thm: CircleDefinition,
        key: "circle_definition",
        en: "the definition of a circle (equal radii)",
        ro: "definiția cercului (raze egale)",
        kind: Calculus,
        avoidable: false,
        uses: &[],
        cited_by: "Reason::EqualRadius, PointMerge, TangentMerge; atom Radii; TheoremKey::CircleMerge, PointMerge, TangentMerge",
    },
    Info {
        thm: TrigIdentities,
        key: "trig_identities",
        en: "trigonometric identities",
        ro: "identități trigonometrice",
        kind: Calculus,
        avoidable: false,
        uses: &[],
        cited_by: "engine \"equal or supplementary angles have equal sines\", \"sine of …\", \"double-angle formula\", \"triple-angle formula\"; atoms SineSupplement, SineComplement",
    },
    Info {
        thm: InscribedAngle,
        key: "inscribed_angle",
        en: "the inscribed angle theorem",
        ro: "teorema unghiului înscris",
        kind: Axiom,
        avoidable: false,
        uses: &[],
        cited_by: "the circle rows of every Reason::Concyclic fact; atoms Inscribed, CyclicOpposite, CyclicExterior",
    },
    Info {
        thm: SimilarityAa,
        key: "similarity_aa",
        en: "the AA similarity criterion",
        ro: "cazul de asemănare UU",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "Reason::SimilarTriangles found by equal angles (engine search_similar, aa)",
    },
    Info {
        thm: SimilaritySas,
        key: "similarity_sas",
        en: "the SAS similarity criterion",
        ro: "cazul de asemănare LUL",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "Reason::SimilarTriangles found by an angle and the ratio of its sides (sas)",
    },
    Info {
        thm: SimilaritySss,
        key: "similarity_sss",
        en: "the SSS similarity criterion",
        ro: "cazul de asemănare LLL",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "Reason::SimilarTriangles found by three proportional sides (sss)",
    },
    Info {
        thm: SimilaritySsa,
        key: "similarity_ssa",
        en: "the SSA similarity criterion (the angle opposite the longer side)",
        ro: "cazul de asemănare LLU (unghiul opus laturii mai lungi)",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "Reason::SimilarTriangles found by two sides and the angle opposite the longer one (ssa)",
    },
    Info {
        thm: TriangleEquality,
        key: "triangle_equality",
        en: "the equality case of the triangle inequality",
        ro: "cazul de egalitate al inegalității triunghiului",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "engine \"equality case of the triangle inequality\"; TheoremKey::TriangleEquality",
    },
    Info {
        thm: LawOfSines,
        key: "law_of_sines",
        en: "the law of sines",
        ro: "teorema sinusurilor",
        kind: Axiom,
        avoidable: true,
        uses: &[],
        cited_by: "engine \"law of sines\" (trig.rs); atom LawOfSines; TheoremKey::LawOfSines; ratio prover law-of-sines rows",
    },
    Info {
        thm: CongruentSss,
        key: "congruent_sss",
        en: "the SSS congruence criterion",
        ro: "cazul de congruență LLL",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySss, "an SSS similarity whose ratio is 1 (how the engine finds it)")],
        cited_by: "Reason::SimilarTriangles with ratio 1 found by sss; atom CongruentSss; metric rule_congruent",
    },
    Info {
        thm: CongruentSas,
        key: "congruent_sas",
        en: "the SAS congruence criterion",
        ro: "cazul de congruență LUL",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySas, "an SAS similarity whose ratio is 1")],
        cited_by: "Reason::SimilarTriangles with ratio 1 found by sas; atom CongruentSas",
    },
    Info {
        thm: CongruentAsa,
        key: "congruent_asa",
        en: "the ASA congruence criterion",
        ro: "cazul de congruență ULU",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "an AA similarity with one pair of equal corresponding sides")],
        cited_by: "Reason::SimilarTriangles with ratio 1 found by aa; atom CongruentAsa",
    },
    Info {
        thm: CongruentRhs,
        key: "congruent_rhs",
        en: "the hypotenuse–leg (RHS) congruence criterion",
        ro: "cazul de congruență IC (ipotenuză–catetă)",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySsa, "an SSA similarity (right angle opposite the hypotenuse) whose ratio is 1")],
        cited_by: "Reason::SimilarTriangles with ratio 1 found by ssa; atom CongruentRhs",
    },
    Info {
        thm: Isosceles,
        key: "isosceles",
        en: "the isosceles triangle theorem (equal sides give equal base angles)",
        ro: "teorema triunghiului isoscel (laturi egale, unghiuri egale la bază)",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentSas, "Pappus: △OAB ≅ △OBA (SAS), the engine's self-similarity")],
        cited_by: "Reason::SimilarTriangles of a triangle with itself found by sas/sss/ssa; atom Isosceles",
    },
    Info {
        thm: IsoscelesConverse,
        key: "isosceles_converse",
        en: "the converse of the isosceles triangle theorem",
        ro: "reciproca teoremei triunghiului isoscel",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentAsa, "△OAB ≅ △OBA (ASA) from the equal base angles")],
        cited_by: "Reason::SimilarTriangles of a triangle with itself found by aa; atom IsoscelesConverse",
    },
    Info {
        thm: PerpBisector,
        key: "perp_bisector",
        en: "the perpendicular bisector theorem (two points equidistant from A and B lie on its perpendicular bisector)",
        ro: "teorema mediatoarei (două puncte egal depărtate de A și B determină mediatoarea lui AB)",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (CongruentSss, "△XAY ≅ △XBY (SSS)"),
            (Isosceles, "the base angles of the two isosceles triangles"),
        ],
        cited_by: "atoms PerpBisector, Reflection",
    },
    Info {
        thm: PerpBisectorLocus,
        key: "perp_bisector_locus",
        en: "the perpendicular bisector locus (a point of the perpendicular bisector is equidistant from the endpoints)",
        ro: "proprietatea mediatoarei (orice punct al mediatoarei este egal depărtat de capetele segmentului)",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentSas, "△XMA ≅ △XMB (SAS)")],
        cited_by: "atom PerpBisectorLocus",
    },
    Info {
        thm: Circumcentre,
        key: "circumcentre",
        en: "the concurrency of the perpendicular bisectors (circumcentre)",
        ro: "concurența mediatoarelor (centrul cercului circumscris)",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (PerpBisectorLocus, "the meet of two perpendicular bisectors is equidistant from the three vertices"),
            (PerpBisector, "so it lies on the third"),
        ],
        cited_by: "statement only (no rule cites it)",
    },
    Info {
        thm: Orthocentre,
        key: "orthocentre",
        en: "the concurrency of the altitudes (orthocentre)",
        ro: "concurența înălțimilor (ortocentrul)",
        kind: Theorem,
        avoidable: true,
        uses: &[(Circumcentre, "the altitudes are the perpendicular bisectors of the triangle whose side midpoints are A, B, C")],
        cited_by: "atom Orthocentre",
    },
    Info {
        thm: Incentre,
        key: "incentre",
        en: "the concurrency of the angle bisectors (incentre and excentres)",
        ro: "concurența bisectoarelor (centrul cercului înscris și centrele cercurilor exînscrise)",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentAsa, "a point of two bisectors is equidistant from the three sides (congruent right triangles)")],
        cited_by: "engine \"angle bisectors concur (incentre/excentre)\"; TheoremKey::BisectorConcurrency",
    },
    Info {
        thm: Midline,
        key: "midline",
        en: "the midline theorem",
        ro: "teorema liniei mijlocii",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySas, "△AMN ∼ △ABC (SAS, ratio 1 : 2)")],
        cited_by: "atom Midline",
    },
    Info {
        thm: MidlineConverse,
        key: "midline_converse",
        en: "the converse of the midline theorem",
        ro: "reciproca teoremei liniei mijlocii",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "MN ∥ BC gives △AMN ∼ △ABC (AA)")],
        cited_by: "atom MidlineConverse",
    },
    Info {
        thm: Centroid,
        key: "centroid",
        en: "the concurrency of the medians and the 2 : 1 ratio (centroid)",
        ro: "concurența medianelor și raportul 2 : 1 (centrul de greutate)",
        kind: Theorem,
        avoidable: true,
        uses: &[(Midline, "MₐM_b is a midline, so △GMₐM_b ∼ △GAB with ratio 1 : 2")],
        cited_by: "atom Centroid",
    },
    Info {
        thm: Intercept,
        key: "intercept",
        en: "the intercept theorem (basic proportionality)",
        ro: "teorema lui Thales (paralela la o latură a triunghiului)",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "DE ∥ BC gives △ADE ∼ △ABC (AA)")],
        cited_by: "engine \"intercept theorem (parallel rungs)\"; atom Intercept; TheoremKey::Intercept; ratio prover basic proportionality",
    },
    Info {
        thm: InterceptConverse,
        key: "intercept_converse",
        en: "the converse of the intercept theorem",
        ro: "reciproca teoremei lui Thales",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySas, "△OAC ∼ △OBD (SAS) gives equal corresponding angles")],
        cited_by: "atom InterceptConverse",
    },
    Info {
        thm: BisectorThm,
        key: "angle_bisector",
        en: "the angle bisector theorem",
        ro: "teorema bisectoarei",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "the perpendiculars from B and C to the bisector give similar right triangles")],
        cited_by: "engine \"angle bisector theorem\" (foot inside the side); atom BisectorRatio; TheoremKey::AngleBisectorThm; ratio prover",
    },
    Info {
        thm: ExtBisectorThm,
        key: "ext_angle_bisector",
        en: "the external angle bisector theorem",
        ro: "teorema bisectoarei exterioare",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "the perpendiculars from B and C to the bisector give similar right triangles")],
        cited_by: "engine \"angle bisector theorem\" (foot outside the side); atom ExtBisectorRatio; TheoremKey::ExtAngleBisectorThm",
    },
    Info {
        thm: BisectorConverse,
        key: "angle_bisector_converse",
        en: "the converse of the angle bisector theorem",
        ro: "reciproca teoremei bisectoarei",
        kind: Theorem,
        avoidable: true,
        uses: &[(BisectorThm, "the point that divides BC in the ratio AB : AC is unique, so it is the foot of the bisector")],
        cited_by: "engine \"angle bisector theorem (converse)\" (inside); atom BisectorConverse; TheoremKey::AngleBisectorThmConverse",
    },
    Info {
        thm: ExtBisectorConverse,
        key: "ext_angle_bisector_converse",
        en: "the converse of the external angle bisector theorem",
        ro: "reciproca teoremei bisectoarei exterioare",
        kind: Theorem,
        avoidable: true,
        uses: &[(ExtBisectorThm, "the point that divides BC externally in the ratio AB : AC is unique")],
        cited_by: "engine \"angle bisector theorem (converse)\" (outside); atom ExtBisectorConverse; TheoremKey::ExtAngleBisectorThmConverse",
    },
    Info {
        thm: Menelaus,
        key: "menelaus",
        en: "Menelaus' theorem",
        ro: "teorema lui Menelaus",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "the perpendiculars from the vertices to the transversal give similar right triangles")],
        cited_by: "engine \"Menelaus' theorem\"; atom Menelaus; TheoremKey::Menelaus; ratio prover",
    },
    Info {
        thm: MenelausConverse,
        key: "menelaus_converse",
        en: "the converse of Menelaus' theorem",
        ro: "reciproca teoremei lui Menelaus",
        kind: Theorem,
        avoidable: true,
        uses: &[(Menelaus, "the point dividing a side in a given ratio is unique")],
        cited_by: "engine \"Menelaus' theorem (converse)\"; atom MenelausConverse; TheoremKey::MenelausConverse",
    },
    Info {
        thm: Ceva,
        key: "ceva",
        en: "Ceva's theorem",
        ro: "teorema lui Ceva",
        kind: Theorem,
        avoidable: true,
        uses: &[(Menelaus, "Menelaus in the two sub-triangles cut by a cevian")],
        cited_by: "atom Ceva; ratio prover",
    },
    Info {
        thm: CevaConverse,
        key: "ceva_converse",
        en: "the converse of Ceva's theorem",
        ro: "reciproca teoremei lui Ceva",
        kind: Theorem,
        avoidable: true,
        uses: &[(Ceva, "the point dividing a side in a given ratio is unique")],
        cited_by: "engine \"Ceva's theorem (converse)\"; atom CevaConverse; TheoremKey::CevaConverse",
    },
    Info {
        thm: GeometricMean,
        key: "geometric_mean",
        en: "the leg and altitude theorems in a right triangle",
        ro: "teorema catetei și teorema înălțimii",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilarityAa, "the altitude to the hypotenuse cuts two triangles similar to the whole one")],
        cited_by: "metric rule_leg (synthetic.rs)",
    },
    Info {
        thm: Pythagoras,
        key: "pythagoras",
        en: "the Pythagorean theorem",
        ro: "teorema lui Pitagora",
        kind: Theorem,
        avoidable: true,
        uses: &[(GeometricMean, "AB² = BD·BC and AC² = CD·CB (leg theorem), and BD + DC = BC")],
        cited_by: "engine \"perpendicular ⇒ squared lengths (Pythagoras)\"; atom Pythagoras; TheoremKey::Pythagoras; metric rule_pythagoras",
    },
    Info {
        thm: PythagorasConverse,
        key: "pythagoras_converse",
        en: "the converse of the Pythagorean theorem",
        ro: "reciproca teoremei lui Pitagora",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Pythagoras, "the right triangle with the same legs has the same hypotenuse"),
            (CongruentSss, "so the two triangles are congruent"),
        ],
        cited_by: "engine \"perpendicular from squared lengths\"; atom PythagorasConverse; TheoremKey::PerpFromSquares",
    },
    Info {
        thm: Stewart,
        key: "stewart",
        en: "Stewart's theorem",
        ro: "teorema lui Stewart",
        kind: Theorem,
        avoidable: true,
        uses: &[(Pythagoras, "the foot of the altitude from A and Pythagoras in three right triangles")],
        cited_by: "engine \"Stewart's theorem\"; TheoremKey::Stewart; metric rule_stewart",
    },
    Info {
        thm: Apollonius,
        key: "apollonius",
        en: "Apollonius' theorem (the length of a median)",
        ro: "teorema medianei",
        kind: Theorem,
        avoidable: true,
        uses: &[(Stewart, "Stewart's theorem with BD : DC = 1 : 1")],
        cited_by: "engine \"Stewart's theorem\" with t = 1/2; metric rule_median",
    },
    Info {
        thm: ParallelogramLaw,
        key: "parallelogram_law",
        en: "the parallelogram law",
        ro: "teorema paralelogramului (suma pătratelor diagonalelor)",
        kind: Theorem,
        avoidable: true,
        uses: &[(Apollonius, "the median theorem in the triangles cut by a diagonal")],
        cited_by: "metric rule_parallelogram",
    },
    Info {
        thm: BritishFlag,
        key: "british_flag",
        en: "the British flag theorem",
        ro: "teorema steagului britanic",
        kind: Theorem,
        avoidable: true,
        uses: &[(Pythagoras, "the perpendiculars from P to the sides: Pythagoras four times")],
        cited_by: "metric rule_british_flag",
    },
    Info {
        thm: PerpFromCentre,
        key: "perp_from_centre",
        en: "the perpendicular from the centre to a chord bisects it",
        ro: "perpendiculara din centru pe o coardă o înjumătățește",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentRhs, "△OMA ≅ △OMB (hypotenuse–leg)")],
        cited_by: "metric rule_perpendicular_from_centre",
    },
    Info {
        thm: Thales,
        key: "thales",
        en: "Thales' theorem (an angle inscribed in a semicircle is right)",
        ro: "unghiul înscris într-un semicerc este drept",
        kind: Theorem,
        avoidable: true,
        uses: &[(Isosceles, "the isosceles triangles OAZ, OBZ and the angle sum of △ABZ")],
        cited_by: "atom Thales; metric rule_thales",
    },
    Info {
        thm: ThalesConverse,
        key: "thales_converse",
        en: "the converse of Thales' theorem",
        ro: "reciproca: vârful unui unghi drept se află pe cercul de diametru ipotenuza",
        kind: Theorem,
        avoidable: true,
        uses: &[(InscribedConverse, "two right angles on one segment are equal inscribed angles")],
        cited_by: "TheoremKey::ThalesConverse",
    },
    Info {
        thm: MedianHypotenuse,
        key: "median_hypotenuse",
        en: "the median to the hypotenuse is half the hypotenuse",
        ro: "mediana corespunzătoare ipotenuzei este jumătate din ipotenuză",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Midline, "the midline from M is parallel to one leg, hence perpendicular to the other"),
            (PerpBisectorLocus, "so M lies on that leg's perpendicular bisector"),
        ],
        cited_by: "atom MedianHypotenuse",
    },
    Info {
        thm: CentralAngle,
        key: "central_angle",
        en: "the central angle theorem",
        ro: "teorema unghiului la centru",
        kind: Theorem,
        avoidable: true,
        uses: &[(Isosceles, "the isosceles triangles OAZ, OBZ and the exterior angle")],
        cited_by: "atoms CentralAngle, RadiusChord",
    },
    Info {
        thm: TangentChord,
        key: "tangent_chord",
        en: "the tangent–chord angle theorem",
        ro: "teorema unghiului dintre tangentă și coardă",
        kind: Theorem,
        avoidable: true,
        uses: &[(CentralAngle, "the tangent is ⟂ OA, so its angle with AB is half of ∠AOB")],
        cited_by: "atom TangentChord",
    },
    Info {
        thm: InscribedConverse,
        key: "inscribed_converse",
        en: "the converse of the inscribed angle theorem (a cyclic quadrilateral from equal or supplementary angles)",
        ro: "reciproca teoremei unghiului înscris (patrulater inscriptibil)",
        kind: Theorem,
        avoidable: true,
        uses: &[(InscribedAngle, "by contradiction with the inscribed angle theorem")],
        cited_by: "Reason::Concyclic found by search_concyclic; TheoremKey::InscribedConverse, OppositeAnglesConverse, ExteriorAngleConverse, Concyclic",
    },
    Info {
        thm: ArcChord,
        key: "arc_chord",
        en: "equal chords subtend equal angles",
        ro: "coarde egale subîntind unghiuri înscrise egale",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (InscribedAngle, "the inscribed angles on the two arcs"),
            (CongruentSss, "the isosceles triangles from the centre on the two chords are congruent"),
        ],
        cited_by: "Reason::TransferArcChord; TheoremKey::ArcChord",
    },
    Info {
        thm: ParallelChords,
        key: "parallel_chords",
        en: "parallel chords cut equal arcs",
        ro: "coardele paralele determină arce egale",
        kind: Theorem,
        avoidable: true,
        uses: &[(InscribedAngle, "the alternate angles of the parallels are inscribed angles")],
        cited_by: "metric rule_parallel_chords",
    },
    Info {
        thm: PowerOfPoint,
        key: "power_of_point",
        en: "the power of a point theorem",
        ro: "teorema puterii punctului",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (SimilarityAa, "△XAC ∼ △XDB (AA)"),
            (InscribedAngle, "whose equal angles are inscribed angles"),
        ],
        cited_by: "atom PowerOfPoint; ratio prover",
    },
    Info {
        thm: TangentSecant,
        key: "tangent_secant",
        en: "the tangent–secant theorem",
        ro: "teorema tangentei și a secantei",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (SimilarityAa, "△XTA ∼ △XBT (AA)"),
            (TangentChord, "the equal angles come from the tangent–chord angle"),
        ],
        cited_by: "atom TangentSecant",
    },
    Info {
        thm: PowerConverse,
        key: "power_converse",
        en: "the converse of the power of a point theorem",
        ro: "reciproca teoremei puterii punctului",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (SimilaritySas, "△XAC ∼ △XDB (SAS)"),
            (InscribedConverse, "the equal angles make ABCD cyclic"),
        ],
        cited_by: "atom PowerConverse",
    },
    Info {
        thm: EqualTangents,
        key: "equal_tangents",
        en: "the two tangents from a point to a circle are equal",
        ro: "tangentele duse dintr-un punct exterior la un cerc sunt egale",
        kind: Theorem,
        avoidable: true,
        uses: &[(CongruentRhs, "△OPT₁ ≅ △OPT₂ (hypotenuse–leg)")],
        cited_by: "atom EqualTangents",
    },
    Info {
        thm: RadicalAxis,
        key: "radical_axis",
        en: "the radical axis theorem",
        ro: "teorema axei radicale",
        kind: Theorem,
        avoidable: true,
        uses: &[(PowerOfPoint, "a point with equal powers with respect to both circles")],
        cited_by: "engine \"radical axis\"; TheoremKey::RadicalAxis",
    },
    Info {
        thm: Homothety,
        key: "homothety",
        en: "the homothety at a centre of similitude",
        ro: "omotetia cu centrul într-un centru de asemănare",
        kind: Theorem,
        avoidable: true,
        uses: &[(SimilaritySas, "△ZO₁P ∼ △ZO₂Q (SAS)")],
        cited_by: "engine \"homothety at a centre of similitude\"; TheoremKey::Homothety",
    },
    Info {
        thm: Monge,
        key: "monge",
        en: "Monge–d'Alembert's theorem",
        ro: "teorema lui Monge–d'Alembert",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Homothety, "the centres of similitude are homothety centres"),
            (MenelausConverse, "their ratios satisfy Menelaus in the triangle of the centres"),
        ],
        cited_by: "engine \"Monge–d'Alembert\"; TheoremKey::Monge",
    },
    Info {
        thm: Simson,
        key: "simson",
        en: "the Simson line theorem",
        ro: "teorema dreptei lui Simson",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (ThalesConverse, "the feet lie on the circles with diameters PA, PB, PC"),
            (InscribedAngle, "angle chasing in those circles"),
        ],
        cited_by: "atom Simson",
    },
    Info {
        thm: Miquel,
        key: "miquel",
        en: "Miquel's theorem",
        ro: "teorema lui Miquel",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (InscribedAngle, "angles in the two circles"),
            (InscribedConverse, "the third quadrilateral is cyclic"),
        ],
        cited_by: "atom Miquel",
    },
    Info {
        thm: Reim,
        key: "reim",
        en: "Reim's theorem",
        ro: "teorema lui Reim",
        kind: Theorem,
        avoidable: true,
        uses: &[(InscribedAngle, "angles in the two circles")],
        cited_by: "atom Reim",
    },
    Info {
        thm: EulerLine,
        key: "euler_line",
        en: "the Euler line theorem",
        ro: "teorema dreptei lui Euler",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Centroid, "G divides the medians 2 : 1"),
            (Orthocentre, "H is the meet of the altitudes"),
            (Circumcentre, "O is the meet of the perpendicular bisectors"),
            (Homothety, "the homothety at G with ratio −1/2 maps H to O"),
        ],
        cited_by: "statement only (no rule cites it)",
    },
    Info {
        thm: NinePointCircle,
        key: "nine_point_circle",
        en: "the nine-point circle theorem",
        ro: "teorema cercului lui Euler (cercul celor nouă puncte)",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Midline, "the midpoints form a triangle with sides parallel to the sides"),
            (ThalesConverse, "the altitude feet see the midline segments at right angles"),
        ],
        cited_by: "statement only (no rule cites it)",
    },
    Info {
        thm: ExtLawOfSines,
        key: "ext_law_of_sines",
        en: "the extended law of sines (a = 2R·sin A)",
        ro: "teorema sinusurilor extinsă (a = 2R·sin A)",
        kind: Theorem,
        avoidable: true,
        uses: &[
            (Thales, "the diameter through B makes a right angle"),
            (InscribedAngle, "with an inscribed angle equal to A"),
        ],
        cited_by: "atom ExtLawOfSines; ratio prover extended sines",
    },
    Info {
        thm: SinesConverse,
        key: "law_of_sines_converse",
        en: "the converse of the law of sines",
        ro: "reciproca teoremei sinusurilor",
        kind: Theorem,
        avoidable: true,
        uses: &[(LawOfSines, "the ratio sin t / sin(π − p − t) is monotone")],
        cited_by: "engine \"law of sines, converse\"; TheoremKey::SinesConverse",
    },
];

pub const ALL: [Thm; 65] = [
    AngleCalculus,
    SegmentArithmetic,
    CircleDefinition,
    TrigIdentities,
    InscribedAngle,
    SimilarityAa,
    SimilaritySas,
    SimilaritySss,
    SimilaritySsa,
    TriangleEquality,
    LawOfSines,
    CongruentSss,
    CongruentSas,
    CongruentAsa,
    CongruentRhs,
    Isosceles,
    IsoscelesConverse,
    PerpBisector,
    PerpBisectorLocus,
    Circumcentre,
    Orthocentre,
    Incentre,
    Midline,
    MidlineConverse,
    Centroid,
    Intercept,
    InterceptConverse,
    BisectorThm,
    ExtBisectorThm,
    BisectorConverse,
    ExtBisectorConverse,
    Menelaus,
    MenelausConverse,
    Ceva,
    CevaConverse,
    GeometricMean,
    Pythagoras,
    PythagorasConverse,
    Stewart,
    Apollonius,
    ParallelogramLaw,
    BritishFlag,
    PerpFromCentre,
    Thales,
    ThalesConverse,
    MedianHypotenuse,
    CentralAngle,
    TangentChord,
    InscribedConverse,
    ArcChord,
    ParallelChords,
    PowerOfPoint,
    TangentSecant,
    PowerConverse,
    EqualTangents,
    RadicalAxis,
    Homothety,
    Monge,
    Simson,
    Miquel,
    Reim,
    EulerLine,
    NinePointCircle,
    ExtLawOfSines,
    SinesConverse,
];

impl Thm {
    pub fn info(self) -> &'static Info {
        &INFO[self as usize]
    }
    pub fn key(self) -> &'static str {
        self.info().key
    }
    pub fn name_en(self) -> &'static str {
        self.info().en
    }
    pub fn name_ro(self) -> &'static str {
        self.info().ro
    }
    pub fn avoidable(self) -> bool {
        self.info().avoidable
    }
    pub fn by_key(key: &str) -> Option<Thm> {
        ALL.into_iter().find(|t| t.key() == key)
    }
    fn bit(self) -> u128 {
        1u128 << (self as u32)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Forbid(u128);

impl Forbid {
    pub const NONE: Forbid = Forbid(0);

    pub fn of(thms: &[Thm]) -> Forbid {
        let mut f = Forbid::NONE;
        for &t in thms {
            f = f.with(t);
        }
        f
    }

    pub fn closure_of(thms: &[Thm]) -> Forbid {
        let mut f = Forbid::of(thms);
        loop {
            let mut next = f;
            for t in ALL {
                if t.info().uses.iter().any(|(u, _)| f.has(*u)) {
                    next = next.with(t);
                }
            }
            if next == f {
                return f;
            }
            f = next;
        }
    }

    pub fn with(self, t: Thm) -> Forbid {
        Forbid(self.0 | t.bit())
    }
    pub fn has(self, t: Thm) -> bool {
        self.0 & t.bit() != 0
    }
    pub fn any(self, ts: &[Thm]) -> bool {
        ts.iter().any(|&t| self.has(t))
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn union(self, o: Forbid) -> Forbid {
        Forbid(self.0 | o.0)
    }
    pub fn iter(self) -> impl Iterator<Item = Thm> {
        ALL.into_iter().filter(move |t| self.has(*t))
    }
    pub fn atom(self, key: AtomKey) -> bool {
        self.any(of_atom(key))
    }
    pub fn keys(self) -> Vec<&'static str> {
        self.iter().map(Thm::key).collect()
    }
    pub fn unavoidable(self) -> Option<Thm> {
        self.iter().find(|t| !t.avoidable())
    }
}

pub fn descendants(t: Thm) -> Vec<Thm> {
    Forbid::closure_of(&[t])
        .iter()
        .filter(|&u| u != t)
        .collect()
}

pub fn of_atom(key: AtomKey) -> &'static [Thm] {
    use AtomKey as K;
    match key {
        K::Inscribed | K::CyclicOpposite | K::CyclicExterior => &[InscribedAngle],
        K::Thales => &[Thales],
        K::TangentChord => &[TangentChord],
        K::PerpBisector | K::Reflection => &[PerpBisector],
        K::Parallel
        | K::AngleAddition
        | K::AngleSum
        | K::ExteriorAngle
        | K::Supplementary
        | K::VerticalAngles
        | K::CommonAngle
        | K::AlternateAngles
        | K::CorrespondingAngles => &[AngleCalculus],
        K::Radii => &[CircleDefinition],
        K::Isosceles => &[Isosceles],
        K::CentralAngle => &[CentralAngle],
        K::RadiusChord => &[CentralAngle, Isosceles],
        K::PowerOfPoint => &[PowerOfPoint],
        K::Midline => &[Midline],
        K::Orthocentre => &[Orthocentre],
        K::EqualTangents => &[EqualTangents],
        K::TangentSecant => &[TangentSecant],
        K::PowerConverse => &[PowerConverse],
        K::Intercept => &[Intercept],
        K::InterceptConverse => &[InterceptConverse],
        K::BisectorRatio => &[BisectorThm],
        K::ExtBisectorRatio => &[ExtBisectorThm],
        K::BisectorConverse => &[BisectorConverse],
        K::ExtBisectorConverse => &[ExtBisectorConverse],
        K::Menelaus => &[Menelaus],
        K::MenelausConverse => &[MenelausConverse],
        K::Ceva => &[Ceva],
        K::CevaConverse => &[CevaConverse],
        K::MidlineConverse => &[MidlineConverse],
        K::Centroid => &[Centroid],
        K::Pythagoras => &[Pythagoras],
        K::PythagorasConverse => &[PythagorasConverse],
        K::IsoscelesConverse => &[IsoscelesConverse],
        K::PerpBisectorLocus => &[PerpBisectorLocus],
        K::MedianHypotenuse => &[MedianHypotenuse],
        K::Simson => &[Simson],
        K::Miquel => &[Miquel],
        K::Reim => &[Reim],
        K::LawOfSines => &[LawOfSines],
        K::ExtLawOfSines => &[ExtLawOfSines],
        K::CongruentSss => &[CongruentSss],
        K::CongruentSas => &[CongruentSas],
        K::CongruentAsa => &[CongruentAsa],
        K::CongruentRhs => &[CongruentRhs],
        K::SineSupplement | K::SineComplement => &[TrigIdentities],
        K::SameSide | K::OppositeSides | K::OnSegment | K::CommonSide => &[],
    }
}

pub fn of_theorem_key(key: TheoremKey) -> &'static [Thm] {
    use TheoremKey as T;
    match key {
        T::RadicalAxis => &[RadicalAxis],
        T::ArcChord => &[ArcChord],
        T::AngleBisectorThm => &[BisectorThm],
        T::AngleBisectorThmConverse => &[BisectorConverse],
        T::ExtAngleBisectorThm => &[ExtBisectorThm],
        T::ExtAngleBisectorThmConverse => &[ExtBisectorConverse],
        T::Intercept => &[Intercept],
        T::Homothety => &[Homothety],
        T::Monge => &[Monge],
        T::Menelaus => &[Menelaus],
        T::MenelausConverse => &[MenelausConverse],
        T::CevaConverse => &[CevaConverse],
        T::BisectorConcurrency => &[Incentre],
        T::TriangleEquality => &[TriangleEquality],
        T::Pythagoras => &[Pythagoras],
        T::PerpFromSquares => &[PythagorasConverse],
        T::SquaresOfRatio | T::LengthsFromSquares => &[SegmentArithmetic],
        T::Stewart => &[Stewart],
        T::LawOfSines => &[LawOfSines],
        T::EqualSines | T::DoubleAngle | T::TripleAngle | T::SineConst => &[TrigIdentities],
        T::SinesConverse => &[SinesConverse],
        T::PointMerge | T::TangentMerge | T::CircleMerge => &[CircleDefinition],
        T::Congruence => &[CongruentSss, CongruentSas, CongruentAsa, CongruentRhs],
        T::Similarity => &[SimilarityAa, SimilaritySas, SimilaritySss, SimilaritySsa],
        T::Collinear => &[AngleCalculus],
        T::Concyclic
        | T::InscribedConverse
        | T::OppositeAnglesConverse
        | T::ExteriorAngleConverse => &[InscribedConverse],
        T::ThalesConverse => &[ThalesConverse],
        T::Other => &[],
    }
}

pub const ENGINE_NAMES: &[(&str, &[Thm])] = &[
    ("radical axis", &[RadicalAxis]),
    ("Monge–d'Alembert", &[Monge]),
    ("homothety at a centre of similitude", &[Homothety]),
    ("angle bisector theorem", &[BisectorThm, ExtBisectorThm]),
    ("angle bisector theorem (converse)", &[BisectorConverse, ExtBisectorConverse]),
    ("intercept theorem (parallel rungs)", &[Intercept]),
    ("Menelaus' theorem", &[Menelaus]),
    ("Menelaus' theorem (converse)", &[MenelausConverse]),
    ("Ceva's theorem (converse)", &[CevaConverse]),
    ("angle bisectors concur (incentre/excentre)", &[Incentre]),
    ("equality case of the triangle inequality", &[TriangleEquality]),
    ("perpendicular ⇒ squared lengths (Pythagoras)", &[Pythagoras]),
    ("perpendicular from squared lengths", &[PythagorasConverse]),
    ("squares of proportional lengths", &[SegmentArithmetic]),
    ("lengths from squared lengths", &[SegmentArithmetic]),
    ("Stewart's theorem", &[Stewart]),
    ("law of sines", &[LawOfSines]),
    ("law of sines, converse", &[SinesConverse]),
    ("equal or supplementary angles have equal sines", &[TrigIdentities]),
    ("double-angle formula", &[TrigIdentities]),
    ("triple-angle formula", &[TrigIdentities]),
    ("sine of 30°", &[TrigIdentities]),
    ("sine of 45°", &[TrigIdentities]),
    ("sine of 60°", &[TrigIdentities]),
    ("sine of 90°", &[TrigIdentities]),
];

pub fn of_name(name: &str) -> Option<&'static [Thm]> {
    ENGINE_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
}

const SIMILARITY: &[Thm] = &[SimilarityAa, SimilaritySas, SimilaritySss, SimilaritySsa];
const CONGRUENCE: &[Thm] = &[CongruentSss, CongruentSas, CongruentAsa, CongruentRhs];
const ISOSCELES: &[Thm] = &[Isosceles, IsoscelesConverse];

pub fn of_reason(r: &Reason) -> Vec<Thm> {
    match r {
        Reason::Assumption(_) | Reason::Construction(_) => Vec::new(),
        Reason::SimilarTriangles(a, b) => {
            let mut s1 = [a.0, a.1, a.2];
            let mut s2 = [b.0, b.1, b.2];
            s1.sort_unstable();
            s2.sort_unstable();
            if s1 == s2 {
                ISOSCELES.to_vec()
            } else {
                let mut v = SIMILARITY.to_vec();
                v.extend_from_slice(CONGRUENCE);
                v
            }
        }
        Reason::Concyclic(_) => vec![InscribedAngle],
        Reason::Collinear(_) => vec![AngleCalculus],
        Reason::EqualRadius(..) | Reason::PointMerge(..) | Reason::TangentMerge(..) => {
            vec![CircleDefinition]
        }
        Reason::TransferAddMul(..) => vec![SegmentArithmetic],
        Reason::TransferArcChord(..) => vec![ArcChord],
        Reason::Theorem(name, _) | Reason::Formula(name, _, _) => {
            of_name(name).map(<[Thm]>::to_vec).unwrap_or_default()
        }
    }
}

pub fn of_fact(f: &Fact) -> Vec<Thm> {
    match &f.thm {
        Some(t) => {
            let mut v = vec![*t];
            if matches!(f.reason, Reason::Concyclic(_)) && *t != InscribedAngle {
                v.push(InscribedAngle);
            }
            v
        }
        None => of_reason(&f.reason),
    }
}

pub fn detectable(t: Thm) -> bool {
    t.info().kind == Kind::Theorem && !CONGRUENCE.contains(&t) || t == InscribedAngle
}

pub fn is_avoidable_all(ts: &[Thm]) -> bool {
    ts.iter().all(|t| t.avoidable())
}

pub fn render(f: Forbid) -> String {
    f.keys().join(", ")
}

pub fn graph_text() -> String {
    let mut out = String::new();
    out.push_str("Theorem identities and what each is derived from (`ddar --theory`).\n");
    out.push_str("A problem that states theorem T is proved without T and without every theorem below that uses T.\n\n");
    for t in ALL {
        let i = t.info();
        let kind = match i.kind {
            Kind::Calculus => "calculus",
            Kind::Axiom => "axiom",
            Kind::Theorem => "theorem",
        };
        out.push_str(&format!(
            "{:<28} [{kind}{}] {}\n",
            i.key,
            if i.avoidable { "" } else { ", built into the closure" },
            i.en
        ));
        for (u, why) in i.uses {
            out.push_str(&format!("    uses {:<24} {why}\n", u.key()));
        }
        let d = descendants(t);
        if !d.is_empty() {
            out.push_str(&format!(
                "    used by (transitively): {}\n",
                d.iter().map(|x| x.key()).collect::<Vec<_>>().join(", ")
            ));
        }
        out.push_str(&format!("    cited by: {}\n", i.cited_by));
    }
    out
}

#[cfg(test)]
mod tests;
