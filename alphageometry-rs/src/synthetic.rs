//! Classical **Euclidean** proofs of metric (length) goals — a numbered,
//! synthetic deduction that cites *named theorems* (the Pythagorean theorem, the
//! perpendicular from the centre of a circle to a chord, Apollonius's median
//! theorem, Thales' angle in a semicircle, …), exactly the way a human writes a
//! geometry proof. This is deliberately **not** a coordinate computation.
//!
//! Method. Each unordered pair of points `{A,B}` has a *squared length*
//! `AB² = |AB|²`. Every applicable classical theorem contributes one exact
//! linear equation among these squared lengths — e.g. a right angle at `C`
//! contributes the Pythagorean equation `AB² = CA² + CB²`. The engine collects
//! the theorem equations that hold in the figure (the figure's own hypotheses
//! decide *which* theorems apply — verified symbolically, never by coordinate
//! coincidence), then eliminates unknown squared lengths by exact rational
//! linear combination until the goal is reached. The chain of theorems used is
//! printed as the proof.
//!
//! Crucially, a theorem that *is* the problem is never cited to "prove" it: when
//! the goal is itself a named result (e.g. the perpendicular-chords relation
//! `AC² + BD² = 4R²`), the engine **derives** it from more elementary theorems,
//! introducing the classical auxiliary points it needs — here the antipode of a
//! circle point, from which Thales, "two perpendiculars to a line are parallel",
//! "parallel chords cut equal arcs", and Pythagoras finish the job.
//!
//! The squared-length ("additive") theorems handled — Pythagoras, the
//! perpendicular-from-centre lemma, Thales, Apollonius's median law, midpoint
//! bisection, and the two circle lemmas above — cover the classic length and
//! sum-of-squares problems. Products/ratios of *unsquared* lengths (Ptolemy,
//! Stewart, Menelaus, Ceva, power of a point) live outside a linear system and
//! are catalogued for a future ratio layer.

use std::collections::{BTreeMap, BTreeSet};

use crate::geo::AlgFigure;
use crate::metric::MExpr;
use crate::numerics::Vec2;
use crate::predicate::PointId;
use crate::rational::Rat;

/// A squared-length unknown `|ab|²`, keyed by the unordered pair.
type Atom = (PointId, PointId);

fn atom(a: PointId, b: PointId) -> Atom {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// A linear equation over squared lengths: `Σ coeff·atom + constant = 0`.
#[derive(Clone, Debug)]
struct Eq {
    terms: BTreeMap<Atom, Rat>,
    constant: Rat,
}

impl Default for Eq {
    fn default() -> Eq {
        Eq {
            terms: BTreeMap::new(),
            constant: Rat::zero(),
        }
    }
}

impl Eq {
    fn is_trivial(&self) -> bool {
        self.terms.values().all(|c| c.is_zero())
    }
    fn add_term(&mut self, a: Atom, c: Rat) {
        let cur = self.terms.get(&a).cloned().unwrap_or_else(Rat::zero);
        let s = &cur + &c;
        if s.is_zero() {
            self.terms.remove(&a);
        } else {
            self.terms.insert(a, s);
        }
    }
    /// `self := self − factor·other`.
    fn sub_scaled(&mut self, other: &Eq, factor: &Rat) {
        for (a, c) in &other.terms {
            self.add_term(*a, -(&(c * factor)));
        }
        self.constant = &self.constant - &(&other.constant * factor);
    }
    fn first_atom(&self) -> Option<Atom> {
        self.terms.keys().next().copied()
    }
}

// ===========================================================================
// Deduction steps (facts, with provenance)
// ===========================================================================

/// One line of the proof: a stated fact, the theorem justifying it, the earlier
/// steps it rests on, and (for metric steps) its squared-length equation.
struct Step {
    text: String,
    eq: Option<Eq>,
    premises: Vec<usize>,
    /// A *compound* named result (parallelogram law, British flag, Stewart, power
    /// of a point, …). A proof that consists of a single such citation equal to
    /// the goal is circular — restating the theorem — and is rejected. Elementary
    /// steps (given data, midpoint, Pythagoras, Thales) are never headlines.
    headline: bool,
}

/// A named entry of the theorem library (for the catalogue / `--theorems`).
pub struct Theorem {
    pub category: &'static str,
    pub name: &'static str,
    pub statement: &'static str,
}

/// The library of classical Euclidean theorems the prover knows about.
/// `--theorems` prints it, grouped by `category`. Each statement is tagged with
/// its status:
///
/// * `[additive]` — operational: fires as a rule in the squared-length engine
///   (`synthetic`) and appears in `--metric` proofs of length / sum-of-squares
///   goals.
/// * `[ratio]` — operational: fires in the multiplicative log-length engine
///   (`ratio`), where the famous results are *derived* from similar triangles.
/// * `[derived]` — a compound result the engine builds from elementary steps
///   (introducing auxiliary points) instead of citing, so it never proves itself.
/// * `[DDAR]` — provable by the deductive-database closure (`engine`) as an
///   angle/ratio/collinearity goal, with a numbered `--proof`.
/// * `[catalogued]` — a recognised classical theorem, statement provided, whose
///   automatic detector is not yet wired.
pub const THEOREMS: &[Theorem] = &[
    // ── Lines & angles ────────────────────────────────────────────────────────
    Theorem { category: "Lines & angles", name: "vertical angles", statement: "vertically opposite angles are equal  [DDAR]" },
    Theorem { category: "Lines & angles", name: "linear pair", statement: "adjacent angles on a line sum to 180°  [DDAR]" },
    Theorem { category: "Lines & angles", name: "alternate angles", statement: "a transversal of parallels makes equal alternate angles  [DDAR]" },
    Theorem { category: "Lines & angles", name: "corresponding angles", statement: "a transversal of parallels makes equal corresponding angles  [DDAR]" },
    Theorem { category: "Lines & angles", name: "co-interior angles", statement: "a transversal of parallels makes co-interior angles sum to 180°  [DDAR]" },
    Theorem { category: "Lines & angles", name: "perpendicular-bisector locus", statement: "a point is equidistant from A, B iff it lies on the perpendicular bisector of AB  [DDAR]" },
    Theorem { category: "Lines & angles", name: "angle-bisector locus", statement: "a point is equidistant from two lines iff it lies on a bisector of their angle  [catalogued]" },
    Theorem { category: "Lines & angles", name: "three parallels intercept", statement: "three parallel lines cut proportional segments on two transversals  [ratio]" },
    Theorem { category: "Lines & angles", name: "perpendicular distance", statement: "the perpendicular is the shortest segment from a point to a line  [catalogued]" },
    Theorem { category: "Lines & angles", name: "Playfair's axiom", statement: "through a point not on a line there passes exactly one parallel to it  [catalogued]" },
    Theorem { category: "Lines & angles", name: "Proclus' axiom", statement: "a line that meets one of two parallel lines meets the other  [catalogued]" },
    Theorem { category: "Lines & angles", name: "equidistance of parallels", statement: "two parallel lines are everywhere the same distance apart  [catalogued]" },
    Theorem { category: "Lines & angles", name: "bisectors of two lines", statement: "the two bisectors of the angles between two intersecting lines are perpendicular  [catalogued]" },

    // ── Triangles: basic ──────────────────────────────────────────────────────
    Theorem { category: "Triangles", name: "angle sum", statement: "the interior angles of a triangle sum to 180°  [DDAR]" },
    Theorem { category: "Triangles", name: "exterior angle", statement: "an exterior angle equals the sum of the two remote interior angles  [DDAR]" },
    Theorem { category: "Triangles", name: "isosceles base angles", statement: "equal sides ⇒ equal base angles (pons asinorum), and conversely  [DDAR]" },
    Theorem { category: "Triangles", name: "equilateral triangle", statement: "all sides equal ⇔ all angles 60°  [DDAR]" },
    Theorem { category: "Triangles", name: "triangle inequality", statement: "each side is shorter than the sum of the other two  [catalogued]" },
    Theorem { category: "Triangles", name: "hinge theorem", statement: "larger included angle ⇒ longer opposite side (SAS inequality)  [catalogued]" },
    Theorem { category: "Triangles", name: "side–angle correspondence", statement: "the larger angle lies opposite the longer side  [catalogued]" },
    Theorem { category: "Triangles", name: "midsegment (midline) theorem", statement: "the segment joining two midpoints is parallel to the third side and half its length  [DDAR]" },
    Theorem { category: "Triangles", name: "Pythagorean theorem", statement: "in a right triangle, hypotenuse² = leg² + leg²  [additive]" },
    Theorem { category: "Triangles", name: "converse Pythagorean theorem", statement: "if c² = a² + b² then the angle opposite c is right  [catalogued]" },
    Theorem { category: "Triangles", name: "45-45-90 triangle", statement: "the sides are 1 : 1 : √2  [additive]" },
    Theorem { category: "Triangles", name: "30-60-90 triangle", statement: "the sides are 1 : √3 : 2, so the squares are 1 : 3 : 4  [catalogued]" },
    Theorem { category: "Triangles", name: "exterior angle inequality (Euclid I.16)", statement: "an exterior angle of a triangle is greater than either remote interior angle  [catalogued]" },
    Theorem { category: "Triangles", name: "median to the hypotenuse", statement: "in a right triangle the median to the hypotenuse equals half the hypotenuse  [DDAR]" },
    Theorem { category: "Triangles", name: "right-triangle inradius", statement: "in a right triangle, r = (a + b − c)/2 with c the hypotenuse  [catalogued]" },
    Theorem { category: "Triangles", name: "acute/obtuse criterion", statement: "c² is less than, equal to, or greater than a² + b² as the angle opposite c is acute, right, or obtuse  [catalogued]" },
    Theorem { category: "Triangles", name: "Pompeiu's theorem", statement: "for P not on the circumcircle of an equilateral triangle ABC, the lengths PA, PB, PC form a triangle  [catalogued]" },
    Theorem { category: "Triangles", name: "generalised Pythagoras (Euclid VI.31)", statement: "for similar figures on the sides of a right triangle, the one on the hypotenuse equals the sum of the other two  [catalogued]" },
    Theorem { category: "Triangles", name: "Leibniz's formula", statement: "PA² + PB² + PC² = GA² + GB² + GC² + 3·PG² for the centroid G and any point P  [catalogued]" },
    Theorem { category: "Triangles", name: "SSA (ambiguous case)", statement: "two sides and a non-included angle determine at most two triangles, uniquely when the given angle faces the longer given side  [catalogued]" },

    // ── Congruence ────────────────────────────────────────────────────────────
    Theorem { category: "Congruence", name: "SSS", statement: "three equal sides ⇒ congruent triangles  [DDAR]" },
    Theorem { category: "Congruence", name: "SAS", statement: "two sides and the included angle equal ⇒ congruent  [DDAR]" },
    Theorem { category: "Congruence", name: "ASA", statement: "two angles and the included side equal ⇒ congruent  [DDAR]" },
    Theorem { category: "Congruence", name: "AAS", statement: "two angles and a non-included side equal ⇒ congruent  [DDAR]" },
    Theorem { category: "Congruence", name: "RHS / hypotenuse–leg", statement: "right angle, hypotenuse and a leg equal ⇒ congruent  [DDAR]" },
    Theorem { category: "Congruence", name: "three medians determine a triangle", statement: "triangles with three equal corresponding medians are congruent  [catalogued]" },

    // ── Similarity & proportion ───────────────────────────────────────────────
    Theorem { category: "Similarity", name: "AA similarity", statement: "two equal angles ⇒ similar triangles (the ratio engine's elementary rule)  [ratio]" },
    Theorem { category: "Similarity", name: "SAS similarity", statement: "an equal angle between proportional sides ⇒ similar  [ratio]" },
    Theorem { category: "Similarity", name: "SSS similarity", statement: "three proportional sides ⇒ similar  [ratio]" },
    Theorem { category: "Similarity", name: "basic proportionality (Thales' intercept)", statement: "DE ∥ BC ⇒ AD:AB = AE:AC = DE:BC — DERIVED from similar triangles  [ratio, derived]" },
    Theorem { category: "Similarity", name: "converse basic proportionality", statement: "a line cutting two sides proportionally is parallel to the third  [catalogued]" },
    Theorem { category: "Similarity", name: "geometric mean (altitude on hypotenuse)", statement: "h² = pq and leg² = hypotenuse × projection — DERIVED from similar right triangles  [ratio, derived]" },
    Theorem { category: "Similarity", name: "ratio of areas of similar triangles", statement: "areas are in the ratio of the squares of corresponding sides  [catalogued]" },
    Theorem { category: "Similarity", name: "ratio of perimeters of similar triangles", statement: "perimeters are in the ratio of corresponding sides  [catalogued]" },
    Theorem { category: "Similarity", name: "altitude-on-hypotenuse similarity", statement: "the altitude to the hypotenuse splits a right triangle into two triangles similar to the whole  [DDAR]" },

    // ── Cevians & triangle centres ────────────────────────────────────────────
    Theorem { category: "Cevians & centres", name: "median concurrency (centroid)", statement: "the three medians meet at the centroid, dividing each 2:1  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "Apollonius's median theorem", statement: "AB² + AC² = 2·AM² + 2·BM² for the median AM to BC  [additive]" },
    Theorem { category: "Cevians & centres", name: "length of a median", statement: "4·mₐ² = 2b² + 2c² − a²  [additive]" },
    Theorem { category: "Cevians & centres", name: "Stewart's theorem", statement: "AD² = (1−t)·AB² + t·AC² − t(1−t)·BC² for a cevian AD with BD:DC = t:(1−t)  [additive]" },
    Theorem { category: "Cevians & centres", name: "angle-bisector theorem", statement: "AD bisects ∠A ⇒ BD:DC = AB:AC — DERIVED (perpendiculars to the bisector)  [ratio, derived]" },
    Theorem { category: "Cevians & centres", name: "external angle-bisector theorem", statement: "the external bisector at A divides BC externally in the ratio AB:AC  [ratio]" },
    Theorem { category: "Cevians & centres", name: "length of an angle bisector", statement: "tₐ² = AB·AC − BD·DC  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Ceva's theorem", statement: "concurrent cevians ⇒ BD·CE·AF = DC·EA·FB — DERIVED from Menelaus on the sub-triangles  [ratio, derived]" },
    Theorem { category: "Cevians & centres", name: "Menelaus's theorem", statement: "for a transversal of ABC, BD·CE·AF = DC·EA·FB — DERIVED (perpendiculars to the transversal)  [ratio, derived]" },
    Theorem { category: "Cevians & centres", name: "van Aubel's theorem", statement: "for concurrent cevians, AP:PD = AF:FB + AE:EC  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "circumcentre", statement: "the perpendicular bisectors of the sides concur, equidistant from the vertices  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "incentre", statement: "the internal angle bisectors concur, equidistant from the sides  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "excentre", statement: "two external and one internal bisector concur at an excentre  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "orthocentre", statement: "the three altitudes of a triangle are concurrent  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "incentre–excentre lemma (Fact 5)", statement: "the arc midpoint is equidistant from two vertices, the incentre and an excentre  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "Euler line", statement: "circumcentre, centroid and orthocentre are collinear, with OG:GH = 1:2  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "nine-point circle", statement: "the midpoints of the sides, feet of the altitudes and midpoints of AH, BH, CH lie on one circle  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "Feuerbach's theorem", statement: "the nine-point circle is tangent to the incircle and the three excircles  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Steiner–Lehmus theorem", statement: "a triangle with two equal angle bisectors is isosceles  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "orthocentre reflection (side)", statement: "the reflection of the orthocentre in a side lies on the circumcircle  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "orthocentre reflection (midpoint)", statement: "the reflection of the orthocentre in the midpoint of a side is the antipode of the opposite vertex  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "angle at the orthocentre", statement: "∠BHC = 180° − ∠A at the orthocentre H  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "vertex–orthocentre distance", statement: "AH = 2·OM: the distance from a vertex to the orthocentre is twice that from the circumcentre to the opposite side  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "reflected circumcircle", statement: "the circumcircle of BHC is the reflection of the circumcircle of ABC in line BC, so it has the same radius  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "orthic incentre", statement: "in an acute triangle, the orthocentre is the incentre of the orthic triangle  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "excentral triangle", statement: "ABC is the orthic triangle of its excentral triangle, whose orthocentre is the incentre I  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "nine-point centre", statement: "the nine-point centre is the midpoint of OH on the Euler line  [DDAR]" },
    Theorem { category: "Cevians & centres", name: "nine-point radius", statement: "the nine-point circle has radius R/2  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Euler's theorem (OI distance)", statement: "OI² = R(R − 2r)  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "circumcentre–orthocentre distance", statement: "OH² = 9R² − (a² + b² + c²)  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Nagel line", statement: "the incentre, centroid and Nagel point are collinear, with IG : GN = 1 : 2  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Sylvester's relation", statement: "as vectors from the circumcentre, OH = OA + OB + OC  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "trigonometric Ceva", statement: "cevians AD, BE, CF concur iff the product of the ratios sin∠BAD/sin∠DAC cyclically equals 1  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "cevian nest theorem", statement: "if DEF is a cevian triangle of ABC and XYZ a cevian triangle of DEF, then AX, BY, CZ concur  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "isogonal conjugate", statement: "reflecting three concurrent cevians in the corresponding angle bisectors gives three concurrent cevians  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "isotomic conjugate", statement: "reflecting the feet of concurrent cevians in the midpoints of the sides gives concurrent cevians  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "O–H isogonal conjugacy", statement: "the circumcentre and the orthocentre are isogonal conjugates  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Carnot's theorem (distances)", statement: "the signed distances from the circumcentre to the three sides sum to R + r  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Carnot's perpendicularity criterion", statement: "perpendiculars at P₁, P₂, P₃ to BC, CA, AB concur iff BP₁² − P₁C² + CP₂² − P₂A² + AP₃² − P₃B² = 0  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "medial triangle", statement: "the midpoints of the sides form a triangle similar to ABC with ratio ½ whose circumcircle is the nine-point circle  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Gergonne–Nagel conjugacy", statement: "the Gergonne and Nagel points are isotomic conjugates  [catalogued]" },
    Theorem { category: "Cevians & centres", name: "Blanchet's theorem", statement: "if cevians through any point of the altitude AD meet AC, AB at E, F, then DA bisects ∠EDF  [catalogued]" },

    // ── Circles: angles ───────────────────────────────────────────────────────
    Theorem { category: "Circles: angles", name: "inscribed-angle theorem", statement: "an inscribed angle is half the central angle on the same arc  [DDAR]" },
    Theorem { category: "Circles: angles", name: "angles in the same segment", statement: "inscribed angles subtending the same arc are equal  [DDAR]" },
    Theorem { category: "Circles: angles", name: "Thales' theorem (semicircle)", statement: "an angle inscribed in a semicircle is a right angle  [additive]" },
    Theorem { category: "Circles: angles", name: "cyclic quadrilateral (opposite angles)", statement: "opposite angles of a cyclic quadrilateral sum to 180°  [DDAR]" },
    Theorem { category: "Circles: angles", name: "cyclic quadrilateral (exterior angle)", statement: "an exterior angle equals the opposite interior angle  [DDAR]" },
    Theorem { category: "Circles: angles", name: "converse of cyclic quadrilateral", statement: "opposite angles summing to 180° ⇒ the quadrilateral is cyclic  [DDAR]" },
    Theorem { category: "Circles: angles", name: "tangent ⟂ radius", statement: "a tangent is perpendicular to the radius at the point of contact  [DDAR]" },
    Theorem { category: "Circles: angles", name: "tangent–chord (alternate segment)", statement: "the tangent–chord angle equals the inscribed angle in the alternate segment  [DDAR]" },
    Theorem { category: "Circles: angles", name: "perpendicular from centre bisects chord", statement: "the perpendicular from the centre to a chord bisects it (and conversely)  [additive]" },
    Theorem { category: "Circles: angles", name: "equal chords subtend equal angles", statement: "equal chords subtend equal angles at the centre  [DDAR]" },
    Theorem { category: "Circles: angles", name: "converse of Thales", statement: "a right angle inscribed in a circle subtends a diameter  [DDAR]" },
    Theorem { category: "Circles: angles", name: "interior angle (two chords)", statement: "the angle between two chords meeting inside a circle is half the sum of the two intercepted arcs  [catalogued]" },
    Theorem { category: "Circles: angles", name: "exterior angle (two secants)", statement: "the angle between two secants (or tangents) from an outside point is half the difference of the intercepted arcs  [catalogued]" },
    Theorem { category: "Circles: angles", name: "equal arcs, equal chords (Euclid III.26–29)", statement: "in equal circles, equal angles stand on equal arcs and equal arcs are subtended by equal chords, and conversely  [catalogued]" },

    // ── Circles: lengths & power ──────────────────────────────────────────────
    Theorem { category: "Circles: lengths", name: "two tangents from a point", statement: "the two tangent segments from an external point are equal  [ratio]" },
    Theorem { category: "Circles: lengths", name: "intersecting chords", statement: "chords AB, CD meeting at P give PA·PB = PC·PD — DERIVED (power of a point)  [ratio, derived]" },
    Theorem { category: "Circles: lengths", name: "intersecting secants", statement: "secants from P give PA·PB = PC·PD — DERIVED (power of a point)  [ratio, derived]" },
    Theorem { category: "Circles: lengths", name: "tangent–secant power", statement: "PT² = PA·PB for a tangent PT and secant PAB — DERIVED (tangent–chord = inscribed angle)  [ratio, derived]" },
    Theorem { category: "Circles: lengths", name: "power of a point", statement: "the product PA·PB along any line through P equals |PO² − R²|  [ratio, derived]" },
    Theorem { category: "Circles: lengths", name: "perpendicular chords", statement: "AC² + BD² = 4R² for perpendicular chords — DERIVED from the antipode + Thales + Pythagoras  [derived]" },
    Theorem { category: "Circles: lengths", name: "extended law of sines", statement: "a / sin A = 2R  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "Ptolemy's theorem", statement: "for a cyclic quadrilateral, AC·BD = AB·CD + AD·BC — DERIVED by the general auxiliary-point search (a point on a side + similar triangles), no per-theorem code  [ratio, derived, aux]" },
    Theorem { category: "Circles: lengths", name: "Ptolemy's inequality", statement: "AC·BD ≤ AB·CD + AD·BC, with equality iff ABCD is cyclic  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "radical axis", statement: "the locus of equal power to two circles is a line ⟂ their centre line  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "radical centre", statement: "the three radical axes of three circles are concurrent  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "Casey's theorem", statement: "a generalisation of Ptolemy to four circles tangent to a fifth  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "Descartes circle theorem", statement: "curvatures of four mutually tangent circles satisfy 2Σk² = (Σk)²  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "Archimedes' broken chord", statement: "if M is the midpoint of arc ABC, the foot of the perpendicular from M to the longer chord halves the broken chord AB + BC  [catalogued]" },
    Theorem { category: "Circles: lengths", name: "common tangent lengths", statement: "for circles with centre distance d, the external and internal common tangents have lengths √(d² − (r₁−r₂)²) and √(d² − (r₁+r₂)²)  [catalogued]" },

    // ── Quadrilaterals & polygons ─────────────────────────────────────────────
    Theorem { category: "Quadrilaterals", name: "parallelogram (opposite sides)", statement: "opposite sides of a parallelogram are equal, and conversely  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "parallelogram (diagonals)", statement: "the diagonals of a parallelogram bisect each other  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "parallelogram law", statement: "AC² + BD² = 2AB² + 2BC²  [additive]" },
    Theorem { category: "Quadrilaterals", name: "rectangle diagonals", statement: "the diagonals of a rectangle are equal  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "British flag theorem", statement: "for a point P and rectangle ABCD, PA² + PC² = PB² + PD²  [additive]" },
    Theorem { category: "Quadrilaterals", name: "rhombus diagonals", statement: "the diagonals of a rhombus are perpendicular bisectors of each other  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "trapezoid midsegment", statement: "the midsegment of a trapezoid is parallel to the bases and half their sum  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Varignon's theorem", statement: "the midpoints of any quadrilateral's sides form a parallelogram  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "Newton's line", statement: "the midpoints of the two diagonals and the centroid of a quadrilateral are collinear  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Pitot's theorem", statement: "a tangential quadrilateral has AB + CD = BC + DA  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Bretschneider's formula", statement: "area of a general quadrilateral from its sides and two opposite angles  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Brahmagupta's formula", statement: "area of a cyclic quadrilateral = √((s−a)(s−b)(s−c)(s−d))  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "kite diagonals", statement: "the diagonals of a kite are perpendicular  [DDAR]" },
    Theorem { category: "Quadrilaterals", name: "isosceles trapezoid", statement: "a trapezoid is cyclic iff it is isosceles  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "orthodiagonal criterion", statement: "a quadrilateral has perpendicular diagonals iff AB² + CD² = BC² + DA²  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Euler's quadrilateral theorem", statement: "AB² + BC² + CD² + DA² = AC² + BD² + 4·MN² for the midpoints M, N of the diagonals  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Japanese theorem (quadrilateral)", statement: "the incentres of the four triangles cut by the diagonals of a cyclic quadrilateral form a rectangle  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Japanese theorem (polygon)", statement: "the sum of the inradii in a triangulation of a cyclic polygon is independent of the triangulation  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Anne's theorem", statement: "the points P with [PAB] + [PCD] = [PBC] + [PDA] in a quadrilateral form its Newton line  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Newton's theorem (tangential)", statement: "the incentre of a tangential quadrilateral lies on its Newton line  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "converse of Pitot", statement: "AB + CD = BC + DA in a convex quadrilateral ⇒ it has an inscribed circle  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "van Aubel's theorem (squares)", statement: "the segments joining the centres of opposite squares erected on the sides of a quadrilateral are equal and perpendicular  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Thébault–Yaglom theorem", statement: "the centres of squares erected on the sides of a parallelogram form a square (Thébault's problem I)  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Finsler–Hadwiger theorem", statement: "two squares sharing a vertex: the two centres and the midpoints of the two segments joining free corners form a square  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Wittenbauer's parallelogram", statement: "the lines through adjacent trisection points of a quadrilateral's sides bound a parallelogram of 8/9 its area  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "equidiagonal criterion", statement: "a quadrilateral has equal diagonals iff its Varignon parallelogram is a rhombus  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Brahmagupta's theorem", statement: "in a cyclic orthodiagonal quadrilateral, the perpendicular to a side from the diagonals' intersection bisects the opposite side  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "circumcentre–side distance", statement: "in a cyclic orthodiagonal quadrilateral, the distance from the circumcentre to a side is half the opposite side  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "anticentre (maltitudes)", statement: "the four maltitudes of a cyclic quadrilateral are concurrent at the anticentre  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "harmonic quadrilateral", statement: "in a cyclic quadrilateral with AB·CD = BC·DA, the tangents at A and C meet on line BD  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Ptolemy's second theorem", statement: "AC/BD = (AB·AD + CB·CD)/(BA·BC + DA·DC) for a cyclic quadrilateral  [catalogued]" },
    Theorem { category: "Quadrilaterals", name: "Fuss' theorem", statement: "a bicentric quadrilateral satisfies 1/(R+d)² + 1/(R−d)² = 1/r², d the distance between the centres  [catalogued]" },

    // ── Areas ─────────────────────────────────────────────────────────────────
    Theorem { category: "Areas", name: "triangle area (base × height)", statement: "area = ½ · base · height  [catalogued]" },
    Theorem { category: "Areas", name: "Heron's formula", statement: "area = √(s(s−a)(s−b)(s−c))  [catalogued]" },
    Theorem { category: "Areas", name: "area = rs", statement: "area = inradius × semiperimeter  [catalogued]" },
    Theorem { category: "Areas", name: "area = abc/4R", statement: "area = product of the sides over four times the circumradius  [catalogued]" },
    Theorem { category: "Areas", name: "shared-base area ratio", statement: "triangles with the same base have areas in the ratio of their heights  [catalogued]" },
    Theorem { category: "Areas", name: "shoelace formula", statement: "area from the coordinates of the vertices (a signed cross-product sum)  [catalogued]" },
    Theorem { category: "Areas", name: "Routh's theorem", statement: "the area ratio of the triangle bounded by three cevians  [catalogued]" },
    Theorem { category: "Areas", name: "Viviani's theorem", statement: "in an equilateral triangle, the sum of distances from an interior point to the sides is constant  [catalogued]" },
    Theorem { category: "Areas", name: "parallelograms on the same base (Euclid I.35)", statement: "parallelograms on the same base and between the same parallels are equal in area  [catalogued]" },
    Theorem { category: "Areas", name: "triangles on the same base (Euclid I.37)", statement: "triangles on the same base and between the same parallels are equal in area  [catalogued]" },
    Theorem { category: "Areas", name: "Pappus' area theorem", statement: "parallelograms on two sides of any triangle determine an equal-area parallelogram on the third, generalising Pythagoras  [catalogued]" },
    Theorem { category: "Areas", name: "Pappus' centroid theorem", statement: "the area (volume) of a surface (solid) of revolution is the generating length (area) times the distance travelled by its centroid  [catalogued]" },
    Theorem { category: "Areas", name: "Pick's theorem", statement: "the area of a lattice polygon is I + B/2 − 1, counting interior and boundary lattice points  [catalogued]" },
    Theorem { category: "Areas", name: "Bolyai–Gerwien theorem", statement: "two polygons of equal area can be dissected into pairwise congruent pieces  [catalogued]" },
    Theorem { category: "Areas", name: "lunes of Alhazen", statement: "the two lunes on the legs of a right triangle, cut by the semicircle on the hypotenuse, together equal the triangle's area  [catalogued]" },
    Theorem { category: "Areas", name: "one-seventh area triangle", statement: "cevians to the one-third points of the sides bound a central triangle of one seventh the area  [catalogued]" },
    Theorem { category: "Areas", name: "Napoleon area relation", statement: "the outer and inner Napoleon triangles differ in area by exactly the area of the original triangle  [catalogued]" },
    Theorem { category: "Areas", name: "area from the diagonals", statement: "a quadrilateral's area is ½·d₁·d₂·sin θ, for the diagonals and the angle between them  [catalogued]" },
    Theorem { category: "Areas", name: "six equal triangles", statement: "the three medians divide a triangle into six triangles of equal area  [catalogued]" },

    // ── Trigonometric relations ───────────────────────────────────────────────
    Theorem { category: "Trigonometry", name: "law of sines", statement: "a/sin A = b/sin B = c/sin C = 2R  [catalogued]" },
    Theorem { category: "Trigonometry", name: "law of cosines", statement: "c² = a² + b² − 2ab·cos C  [catalogued]" },
    Theorem { category: "Trigonometry", name: "law of tangents", statement: "(a−b)/(a+b) = tan(½(A−B)) / tan(½(A+B))  [catalogued]" },
    Theorem { category: "Trigonometry", name: "projection formula", statement: "a = b·cos C + c·cos B  [catalogued]" },
    Theorem { category: "Trigonometry", name: "Mollweide's formula", statement: "(a+b)/c = cos(½(A−B)) / sin(½C)  [catalogued]" },
    Theorem { category: "Trigonometry", name: "Stewart via cosines", statement: "Stewart's theorem follows from the law of cosines on the two sub-triangles  [additive]" },
    Theorem { category: "Trigonometry", name: "sine area formula", statement: "area of a triangle = ½·ab·sin C  [catalogued]" },
    Theorem { category: "Trigonometry", name: "law of cotangents", statement: "cot(A/2) = (s − a)/r, uniformly over the three angles  [catalogued]" },
    Theorem { category: "Trigonometry", name: "tangent identity", statement: "in any triangle, tan A + tan B + tan C = tan A · tan B · tan C  [catalogued]" },
    Theorem { category: "Trigonometry", name: "cosine sum identity", statement: "cos A + cos B + cos C = 1 + r/R  [catalogued]" },
    Theorem { category: "Trigonometry", name: "half-angle product", statement: "sin(A/2)·sin(B/2)·sin(C/2) = r/(4R)  [catalogued]" },
    Theorem { category: "Trigonometry", name: "Regiomontanus' problem", statement: "the angle a segment subtends from a line is maximised where the circle through its endpoints is tangent to the line  [catalogued]" },
    Theorem { category: "Trigonometry", name: "Snellius–Pothenot resection", statement: "a point is determined by the two angles it subtends at three known points, as the intersection of two circular loci  [catalogued]" },

    // ── Projective & advanced configurations ──────────────────────────────────
    Theorem { category: "Advanced", name: "Simson line", statement: "the feet of the perpendiculars from a point on the circumcircle to the sides are collinear  [catalogued]" },
    Theorem { category: "Advanced", name: "Steiner line", statement: "the reflections of a circumcircle point in the sides are collinear (through the orthocentre)  [catalogued]" },
    Theorem { category: "Advanced", name: "Miquel's theorem", statement: "the circles on the three side-points of a triangle share a common point  [catalogued]" },
    Theorem { category: "Advanced", name: "Napoleon's theorem", statement: "the centres of equilateral triangles erected on the sides form an equilateral triangle  [catalogued]" },
    Theorem { category: "Advanced", name: "Morley's trisector theorem", statement: "the adjacent angle trisectors meet in an equilateral triangle  [catalogued]" },
    Theorem { category: "Advanced", name: "Fermat point", statement: "the point minimising total distance to the vertices; sees each side at 120°  [catalogued]" },
    Theorem { category: "Advanced", name: "Gergonne point", statement: "the cevians to the incircle touch-points are concurrent  [DDAR]" },
    Theorem { category: "Advanced", name: "Nagel point", statement: "the cevians to the excircle touch-points are concurrent  [catalogued]" },
    Theorem { category: "Advanced", name: "Desargues' theorem", statement: "two triangles perspective from a point are perspective from a line  [catalogued]" },
    Theorem { category: "Advanced", name: "Pappus's hexagon theorem", statement: "the three intersection points of a hexagon on two lines are collinear  [catalogued]" },
    Theorem { category: "Advanced", name: "Pascal's theorem", statement: "the opposite sides of a hexagon inscribed in a conic meet in three collinear points  [catalogued]" },
    Theorem { category: "Advanced", name: "Brianchon's theorem", statement: "the diagonals of a hexagon circumscribed about a conic are concurrent  [catalogued]" },
    Theorem { category: "Advanced", name: "Newton–Gauss line", statement: "the midpoints of the three diagonals of a complete quadrilateral are collinear  [catalogued]" },
    Theorem { category: "Advanced", name: "Monge's theorem", statement: "the three external centres of similitude of three circles are collinear  [catalogued]" },
    Theorem { category: "Advanced", name: "butterfly theorem", statement: "the midpoint of a chord bisects the segment cut by two chords through it  [catalogued]" },
    Theorem { category: "Advanced", name: "pole and polar", statement: "the polar of a point w.r.t. a circle; reciprocity of pole–polar incidence  [catalogued]" },
    Theorem { category: "Advanced", name: "harmonic conjugates / cross-ratio", statement: "the cross-ratio of four collinear or concyclic points is projectively invariant  [catalogued]" },

    // ── Triangle centers (Kimberling / ETC classics) ──────────────────────────
    Theorem { category: "Triangle centers", name: "Lemoine (symmedian) point", statement: "the three symmedians — medians reflected in the angle bisectors — are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "symmedian–tangent lemma", statement: "the tangents to the circumcircle at B and C meet on the symmedian from A  [DDAR]" },
    Theorem { category: "Triangle centers", name: "Brocard points", statement: "there is a unique point Ω with ∠ΩAB = ∠ΩBC = ∠ΩCA = ω, and a second isogonal twin  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Brocard angle", statement: "cot ω = cot A + cot B + cot C, and ω ≤ 30°  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Spieker centre", statement: "the incentre of the medial triangle is the centroid of the triangle's perimeter  [catalogued]" },
    Theorem { category: "Triangle centers", name: "mittenpunkt", statement: "the lines from each excentre through the midpoint of the corresponding side are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "de Longchamps point", statement: "the reflection of the orthocentre in the circumcentre is the orthocentre of the anticomplementary triangle  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Bevan point", statement: "the circumcentre of the excentral triangle is the reflection of the incentre in the circumcentre  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Schiffler point", statement: "the Euler lines of ABC, BCI, CAI and ABI are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Napoleon points", statement: "the lines from the vertices to the centres of the opposite Napoleon equilateral triangles are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Vecten points", statement: "the lines from the vertices to the centres of squares erected on the opposite sides are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Steiner point", statement: "the Steiner circumellipse meets the circumcircle at a fourth point, the Steiner point  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Kosnita's theorem", statement: "the lines joining each vertex to the circumcentre of the triangle on the other two vertices and O are concurrent  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Lester's theorem", statement: "the two Fermat points, the circumcentre and the nine-point centre are concyclic  [catalogued]" },
    Theorem { category: "Triangle centers", name: "Kariya's theorem", statement: "marking equal distances from the incentre along the perpendiculars to the three sides gives three concurrent cevians  [catalogued]" },

    // ── Named lines & conics ──────────────────────────────────────────────────
    Theorem { category: "Named lines & conics", name: "Brocard axis", statement: "the two Brocard points lie on the circle with diameter OK; its diameter line OK is the Brocard axis  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Lemoine axis", statement: "the tangents to the circumcircle at the vertices meet the opposite sides in three collinear points  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "orthic axis", statement: "the sides of the orthic triangle meet the corresponding sides in three collinear points, on a line perpendicular to the Euler line  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Soddy line", statement: "the incentre, the Gergonne point and the two Soddy circle centres are collinear  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Steiner inellipse", statement: "the unique inscribed ellipse tangent to the sides at their midpoints, centred at the centroid  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Marden's theorem", statement: "the foci of the Steiner inellipse are the critical points of the cubic polynomial with roots at the vertices  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Steiner circumellipse", statement: "the unique circumscribed ellipse centred at the centroid; it has the least area of all circumellipses  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Kiepert hyperbola", statement: "the apexes of Kiepert's construction give perspectors tracing the rectangular hyperbola through A, B, C, G and H  [catalogued]" },
    Theorem { category: "Named lines & conics", name: "Steiner deltoid", statement: "the envelope of all Simson lines of a triangle is a tricuspid deltoid centred at the nine-point centre  [catalogued]" },

    // ── Circles & tangency ────────────────────────────────────────────────────
    Theorem { category: "Circles & tangency", name: "tangent circles", statement: "the point of tangency of two tangent circles lies on the line of centres  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "common chord", statement: "the common chord of two intersecting circles is perpendicular to the line of centres  [DDAR]" },
    Theorem { category: "Circles & tangency", name: "chord-midpoint locus", statement: "the midpoints of all chords through a fixed interior point P form a circle with diameter OP  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Reim's theorem", statement: "two circles meet at P, Q; lines through P and Q cut them again at A, C and B, D — then AB ∥ CD  [DDAR]" },
    Theorem { category: "Circles & tangency", name: "eyeball theorem", statement: "the chords cut on each of two circles by the tangent lines drawn from the other's centre are equal  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Apollonius circle (locus)", statement: "the locus of points with PA : PB = k ≠ 1 is a circle centred on line AB  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Archimedes' twin circles", statement: "in an arbelos, the two circles inscribed on either side of the perpendicular at the inner tangency point are congruent  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Bankoff circle", statement: "the circle through the inner tangency point and the touch points of the arbelos' inscribed circle is congruent to Archimedes' twins  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "arbelos area (Archimedes)", statement: "the arbelos equals in area the circle whose diameter is the inner perpendicular chord to the outer semicircle  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Pappus chain", statement: "in an arbelos, the centre of the n-th circle of the Pappus chain lies at height n times its diameter  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Johnson circles", statement: "three congruent circles through a common point meet pairwise again in a triangle whose circumcircle is congruent to them  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Steiner's porism", statement: "if one closed Steiner chain exists between two circles, every starting circle yields a closed chain  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "six circles theorem (Money-Coutts)", statement: "a chain of circles each tangent to two sides of a triangle and to the previous circle closes after six  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "seven circles theorem", statement: "for a closed chain of six circles tangent to a seventh, the three lines joining opposite tangency points are concurrent  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "five circles theorem", statement: "the circumcircles of the five outer triangles of a pentagram meet pairwise again in five concyclic points  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "first Lemoine circle", statement: "the parallels to the sides through the Lemoine point cut the sides in six concyclic points  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "cosine circle (second Lemoine)", statement: "the antiparallels to the sides through the Lemoine point cut the sides in six concyclic points centred at K  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Tucker circles", statement: "closed alternating parallel–antiparallel hexagons have their six vertices on a circle centred on line OK  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Taylor circle", statement: "the projections of each altitude foot onto the other two sides are six concyclic points  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Conway circle", statement: "extending each side beyond each vertex by the opposite side's length gives six concyclic points centred at the incentre  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Fuhrmann circle", statement: "the reflections of the arc midpoints in the corresponding sides lie on the circle with diameter from the orthocentre to the Nagel point  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "incircle diameter lemma", statement: "the antipode of the incircle's touch point on BC lies on the line from A to the touch point of the A-excircle  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "mixtilinear incircle", statement: "the circle tangent to AB, AC and internally to the circumcircle touches the sides at two points whose midpoint is the incentre  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Sawayama–Thébault lemma", statement: "a circle tangent to a cevian, to the base and internally to the circumcircle has its touch chord through the incentre  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Thébault's theorem (three circles)", statement: "the centres of the two circles tangent to cevian AD, side BC and the circumcircle are collinear with the incentre  [catalogued]" },
    Theorem { category: "Circles & tangency", name: "Malfatti circles", statement: "three circles, each tangent to the other two and to two sides of a triangle, exist and are unique  [catalogued]" },

    // ── Concurrency & collinearity ────────────────────────────────────────────
    Theorem { category: "Concurrency & collinearity", name: "Sylvester–Gallai theorem", statement: "finitely many points, not all collinear, determine a line through exactly two of them  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Kiepert's theorem", statement: "similar isosceles triangles erected on the sides give concurrent vertex-to-apex lines  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Jacobi's theorem", statement: "for points X, Y, Z making pairwise equal base angles at the triangle's vertices, AX, BY, CZ are concurrent  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Droz-Farny theorem", statement: "two perpendicular lines through the orthocentre cut the three sides in segments whose midpoints are collinear  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Miquel point (complete quadrilateral)", statement: "the circumcircles of the four triangles of a complete quadrilateral pass through one point  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Gauss–Bodenmiller theorem", statement: "the circles on the three diagonals of a complete quadrilateral as diameters are coaxal; the four orthocentres lie on their radical axis  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "orthopole", statement: "the perpendiculars to the sides from the projections of the opposite vertices onto any line are concurrent  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "converse of Simson", statement: "if the feet of the perpendiculars from P to the sides are collinear, then P lies on the circumcircle  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Simson line bisection", statement: "the Simson line of P bisects the segment PH to the orthocentre, at a point of the nine-point circle  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "angle of two Simson lines", statement: "the Simson lines of two circumcircle points meet at half the angular measure of their arc  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "converse of Menelaus", statement: "if the product of the three signed section ratios equals −1, the three points on the sides are collinear  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "converse of Ceva", statement: "if the product of the three signed section ratios equals +1, the three cevians are concurrent  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Clifford's circle theorem", statement: "for four circles through a common point, the circles through each triple's remaining intersections meet in one point  [catalogued]" },
    Theorem { category: "Concurrency & collinearity", name: "Terquem's theorem", statement: "the circle through the feet of three concurrent cevians cuts the sides again in the feet of another concurrent triple  [catalogued]" },

    // ── Projective ────────────────────────────────────────────────────────────
    Theorem { category: "Projective", name: "Poncelet's closure theorem", statement: "a polygon inscribed in one conic and circumscribed about another closes for every starting point if it closes once  [catalogued]" },
    Theorem { category: "Projective", name: "Poncelet's triangle porism", statement: "circles with OI² = R² − 2Rr admit a one-parameter family of triangles inscribed in one and circumscribing the other  [catalogued]" },
    Theorem { category: "Projective", name: "Braikenridge–Maclaurin theorem", statement: "converse of Pascal: if the opposite sides of a hexagon meet in three collinear points, its vertices lie on a conic  [catalogued]" },
    Theorem { category: "Projective", name: "fundamental theorem of projective geometry", statement: "a projectivity of a line is determined by the images of three points  [catalogued]" },
    Theorem { category: "Projective", name: "Steiner's conic construction", statement: "the intersections of corresponding lines of two projectively related pencils trace a conic  [catalogued]" },
    Theorem { category: "Projective", name: "complete quadrangle", statement: "on each diagonal of a complete quadrangle, the two diagonal points and the two quadrangle points form a harmonic range  [catalogued]" },
    Theorem { category: "Projective", name: "duality principle", statement: "every theorem of projective plane geometry remains true when points and lines are interchanged  [catalogued]" },
    Theorem { category: "Projective", name: "Brokard's theorem", statement: "the diagonal triangle of a cyclic quadrilateral is self-polar, and the circle's centre is its orthocentre  [catalogued]" },
    Theorem { category: "Projective", name: "Carnot's theorem (conics)", statement: "six points, two on each side of a triangle, lie on a conic iff the product of the six signed side ratios equals 1  [catalogued]" },
    Theorem { category: "Projective", name: "Chasles' theorem (conics)", statement: "four points of a conic subtend the same cross-ratio at every fifth point of the conic  [catalogued]" },

    // ── Transformations ───────────────────────────────────────────────────────
    Theorem { category: "Transformations", name: "two reflections (intersecting axes)", statement: "reflections in two intersecting lines compose to a rotation about the intersection by twice the angle between them  [catalogued]" },
    Theorem { category: "Transformations", name: "two reflections (parallel axes)", statement: "reflections in two parallel lines compose to a translation by twice the distance between them  [catalogued]" },
    Theorem { category: "Transformations", name: "three reflections theorem", statement: "every plane isometry is the composition of at most three reflections  [catalogued]" },
    Theorem { category: "Transformations", name: "Chasles' theorem (isometries)", statement: "every orientation-preserving plane isometry is a translation or a rotation  [catalogued]" },
    Theorem { category: "Transformations", name: "glide reflection classification", statement: "every orientation-reversing plane isometry is a reflection or a glide reflection  [catalogued]" },
    Theorem { category: "Transformations", name: "Hjelmslev's theorem", statement: "when an isometry maps one line onto another, the midpoints of corresponding-point segments are collinear or coincide  [catalogued]" },
    Theorem { category: "Transformations", name: "Heron's shortest path", statement: "the shortest broken path from A to B via a point of a line makes equal angles with it (reflection principle)  [catalogued]" },
    Theorem { category: "Transformations", name: "unique spiral similarity", statement: "for segments AB and CD not related by a translation, exactly one spiral similarity sends A→C and B→D  [catalogued]" },
    Theorem { category: "Transformations", name: "spiral centre construction", statement: "if lines AB, CD meet at X, the centre of the spiral similarity AB→CD is the second point of circles (XAC) and (XBD)  [catalogued]" },
    Theorem { category: "Transformations", name: "spiral similarity pairing", statement: "the spiral similarity taking AB to CD is also the one taking AC to BD  [catalogued]" },
    Theorem { category: "Transformations", name: "similitude centres of circles", statement: "any two circles are homothetic; their external and internal similitude centres divide the centre line in the ratio of the radii  [catalogued]" },
    Theorem { category: "Transformations", name: "inversion maps lines and circles", statement: "inversion maps lines and circles to lines and circles, fixing lines through the centre  [catalogued]" },
    Theorem { category: "Transformations", name: "inversion is conformal", statement: "inversion preserves the magnitude of angles between curves  [catalogued]" },
    Theorem { category: "Transformations", name: "inversion distance formula", statement: "A'B' = r²·AB/(OA·OB) under inversion with centre O and radius r  [catalogued]" },

    // ── Polygons ──────────────────────────────────────────────────────────────
    Theorem { category: "Polygons", name: "interior angle sum", statement: "the interior angles of a convex n-gon sum to (n − 2)·180°  [catalogued]" },
    Theorem { category: "Polygons", name: "exterior angle sum", statement: "the exterior angles of a convex polygon sum to 360°  [catalogued]" },
    Theorem { category: "Polygons", name: "regular pentagon diagonal", statement: "the diagonal of a regular pentagon is φ = (1 + √5)/2 times its side  [catalogued]" },
    Theorem { category: "Polygons", name: "Petr–Douglas–Neumann theorem", statement: "n − 2 successive apex constructions of isosceles triangles on the sides turn any n-gon into a regular n-gon  [catalogued]" },
    Theorem { category: "Polygons", name: "Viviani for regular polygons", statement: "in a regular (or any equiangular) polygon, the sum of distances from an interior point to the sides is constant  [catalogued]" },
    Theorem { category: "Polygons", name: "Erdős–Nagy theorem", statement: "repeatedly flipping concave pockets across their hulls makes any simple polygon convex in finitely many steps  [catalogued]" },
    Theorem { category: "Polygons", name: "cyclic polygon maximises area", statement: "among all polygons with given ordered side lengths, the cyclic one has the greatest area  [catalogued]" },

    // ── Inequalities & extrema ────────────────────────────────────────────────
    Theorem { category: "Inequalities & extrema", name: "Euler's inequality", statement: "R ≥ 2r, with equality iff the triangle is equilateral  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Erdős–Mordell inequality", statement: "PA + PB + PC is at least twice the sum of the distances from an interior point P to the sides  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Barrow's inequality", statement: "PA + PB + PC ≥ 2(PU + PV + PW) for the chords PU, PV, PW bisecting the angles at P — sharpening Erdős–Mordell  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Weitzenböck's inequality", statement: "a² + b² + c² ≥ 4√3·Area  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Hadwiger–Finsler inequality", statement: "a² + b² + c² ≥ (a−b)² + (b−c)² + (c−a)² + 4√3·Area  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Padoa's inequality", statement: "abc ≥ (a + b − c)(b + c − a)(c + a − b)  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Blundon's inequality", statement: "s ≤ 2R + (3√3 − 4)r, the best linear bound on the semiperimeter in R and r  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "centroid minimises squares", statement: "PA² + PB² + PC² is minimised exactly at the centroid  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "isoperimetric inequality", statement: "among all closed plane curves of given length, the circle encloses the greatest area  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "isoperimetric triangle", statement: "among triangles of given perimeter, the equilateral one has the greatest area  [catalogued]" },
    Theorem { category: "Inequalities & extrema", name: "Fagnano's problem", statement: "the orthic triangle has the least perimeter among all triangles inscribed in an acute triangle  [catalogued]" },

    // ── Constructions & impossibility ─────────────────────────────────────────
    Theorem { category: "Constructions & impossibility", name: "Mohr–Mascheroni theorem", statement: "every compass-and-straightedge point construction can be carried out with compass alone  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "Poncelet–Steiner theorem", statement: "a straightedge plus one drawn circle with its centre suffices for every compass construction  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "Napoleon's problem", statement: "the centre of a given circle can be found with compass alone  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "Gauss–Wantzel theorem", statement: "a regular n-gon is constructible iff n is a power of 2 times distinct Fermat primes  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "regular 17-gon (Gauss)", statement: "the regular heptadecagon is constructible with compass and straightedge  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "angle trisection (Wantzel)", statement: "a general angle cannot be trisected with compass and straightedge  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "doubling the cube", statement: "∛2 is not constructible: the Delian problem is unsolvable with compass and straightedge  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "squaring the circle (Lindemann)", statement: "π is transcendental, so no compass-and-straightedge square equals a given circle in area  [catalogued]" },
    Theorem { category: "Constructions & impossibility", name: "Apollonius' problem", statement: "circles tangent to three given circles exist, generically eight of them  [catalogued]" },
];

// ===========================================================================
// Figure facts (gathered symbolically from the construction)
// ===========================================================================

/// A circle recognised in the figure: its centre and the points on it.
struct Circle {
    centre: PointId,
    on: BTreeSet<PointId>,
}

struct Figure {
    /// Point names and coordinates — owned so *auxiliary* points (e.g. antipodes)
    /// can be appended beyond the base construction.
    names: Vec<String>,
    coords: Vec<Vec2>,
    /// A *second*, independently re-sampled instance of the same construction,
    /// aligned by index. Used to gate numeric detections (division ratios, equal
    /// products, similarity) so a rule fires only on a quantity the construction
    /// *fixes* — not a coincidence of one instance. `None` if unavailable.
    coords2: Option<Vec<Vec2>>,
    /// `ab ⟂ cd` right-angle facts, each with the step that introduced it
    /// (`usize::MAX` = a given hypothesis; otherwise the deriving step).
    perps: Vec<([PointId; 4], usize)>,
    /// `ab ∥ cd` facts, each with the introducing step.
    paras: Vec<([PointId; 4], usize)>,
    congs: Vec<[PointId; 4]>,
    colls: Vec<Vec<PointId>>,
    midpoints: Vec<(PointId, PointId, PointId)>, // (m, a, b): m is the midpoint of ab
    circles: Vec<Circle>,
    abs_len2: Vec<(PointId, PointId, Rat)>, // |ab|² = value
    /// For an auxiliary point, the step that introduced it (so proofs that use
    /// it cite the construction).
    aux_intro: BTreeMap<PointId, usize>,
    steps: Vec<Step>,
}

pub(crate) fn rat_of(v: f64) -> Option<Rat> {
    // Beyond 2^53 an f64 is no longer an exact integer, and `as i64` would
    // saturate distinct values onto i64::MAX.
    if !v.is_finite() || v.abs() >= 9.0e15 {
        return None;
    }
    if (v - v.round()).abs() < 1e-9 {
        return Some(Rat::from_int(v.round() as i64));
    }
    for den in 2..=5040i64 {
        let n = v * den as f64;
        if (n - n.round()).abs() < 1e-9 {
            return Some(Rat::new(n.round() as i64, den));
        }
    }
    None
}

impl Figure {
    fn coord(&self, p: PointId) -> Vec2 {
        self.coords[p as usize]
    }
    fn nm(&self, p: PointId) -> String {
        self.names[p as usize].clone()
    }
    fn seg(&self, a: PointId, b: PointId) -> String {
        format!("{}{}", self.nm(a), self.nm(b))
    }
    /// Concatenated point names, e.g. `ABCD` for a polygon.
    fn poly(&self, pts: &[PointId]) -> String {
        pts.iter().map(|&p| self.nm(p)).collect()
    }

    /// Numeric squared distance (oracle used only to check preconditions such as
    /// "these three points are equidistant from O").
    fn d2(&self, a: PointId, b: PointId) -> f64 {
        let (u, v) = (self.coord(a), self.coord(b));
        (u - v).dot(u - v)
    }

    /// The signed ratio `t = BD/BC` in which `d` divides the segment `bc`, but
    /// only if the construction *fixes* it — i.e. the same rational value appears
    /// in the second instance too. Returns `None` for a free point (ratio drifts
    /// between instances) or when it does not rationalise. This is the soundness
    /// gate that keeps a numerically-read ratio from becoming an invented fact.
    fn fixed_ratio(&self, b: PointId, d: PointId, c: PointId) -> Option<Rat> {
        let bc2 = self.d2(b, c);
        if bc2 < 1e-12 {
            return None;
        }
        let t = (self.coord(d) - self.coord(b)).dot(self.coord(c) - self.coord(b)) / bc2;
        let r = rat_of(t)?;
        // Require the *same* ratio in the independent instance.
        if let Some(alt) = self.coords2.as_ref() {
            let (vb, vd, vc) = (
                *alt.get(b as usize)?,
                *alt.get(d as usize)?,
                *alt.get(c as usize)?,
            );
            let bc2b = (vc - vb).dot(vc - vb);
            if bc2b < 1e-12 {
                return None;
            }
            let t2 = (vd - vb).dot(vc - vb) / bc2b;
            if (t2 - r.to_f64()).abs() > 1e-6 {
                return None;
            }
        }
        Some(r)
    }

    fn push_step(&mut self, text: String, eq: Option<Eq>, premises: Vec<usize>) -> usize {
        self.steps.push(Step {
            text,
            eq,
            premises,
            headline: false,
        });
        self.steps.len() - 1
    }

    /// Like [`push_step`] but marks the step a *compound named theorem*, so a
    /// lone citation of it cannot stand as a proof of an identical goal.
    fn push_headline(&mut self, text: String, eq: Option<Eq>, premises: Vec<usize>) -> usize {
        let i = self.push_step(text, eq, premises);
        self.steps[i].headline = true;
        i
    }

    /// Gather the symbolic hypotheses of the construction as facts. `coords2`, if
    /// present, is a second independent instance (aligned by index) used to gate
    /// numeric detections against instance-specific coincidences.
    fn gather(fig: &AlgFigure, coords2: Option<Vec<Vec2>>) -> Figure {
        let mut f = Figure {
            names: fig.names.clone(),
            coords: fig.coords.clone(),
            coords2,
            perps: Vec::new(),
            paras: Vec::new(),
            congs: Vec::new(),
            colls: Vec::new(),
            midpoints: Vec::new(),
            circles: Vec::new(),
            abs_len2: Vec::new(),
            aux_intro: BTreeMap::new(),
            steps: Vec::new(),
        };

        // Predicates.
        for p in &fig.preds {
            let pts = &p.points;
            match p.name.as_str() {
                "perp" if pts.len() == 4 => {
                    f.perps.push(([pts[0], pts[1], pts[2], pts[3]], usize::MAX))
                }
                "para" if pts.len() == 4 => {
                    f.paras.push(([pts[0], pts[1], pts[2], pts[3]], usize::MAX))
                }
                "cong" if pts.len() == 4 => {
                    // Skip the trivial |ab|=|ab| placeholder emitted for DistConst.
                    if !(pts[0] == pts[2] && pts[1] == pts[3]) {
                        f.congs.push([pts[0], pts[1], pts[2], pts[3]]);
                    }
                }
                "coll" if pts.len() >= 3 => f.colls.push(pts.clone()),
                _ => {}
            }
        }

        // Absolute squared lengths from the scale constraints.
        for &(a, b, v) in &fig.scale {
            if let Some(r) = rat_of(v) {
                f.abs_len2.push((a, b, &r * &r));
            }
        }

        f.detect_circles();
        f.detect_midpoints();
        f
    }

    /// A circle = a centre `o` with ≥3 points at equal distance, evidenced by
    /// `cong(o,·,o,·)` predicates or shared-centre absolute lengths.
    fn detect_circles(&mut self) {
        let n = self.coords.len();
        // union of "equidistant from o" evidence
        let mut on: Vec<BTreeSet<PointId>> = vec![BTreeSet::new(); n];
        for c in &self.congs {
            // cong(o,a,o,b): o repeated in positions 0 and 2 → o centre, a,b on it.
            if c[0] == c[2] && c[1] != c[3] {
                on[c[0] as usize].insert(c[1]);
                on[c[0] as usize].insert(c[3]);
            }
            if c[1] == c[3] && c[0] != c[2] {
                on[c[1] as usize].insert(c[0]);
                on[c[1] as usize].insert(c[2]);
            }
        }
        for &(a, b, _) in &self.abs_len2 {
            // shared-centre absolute radii mark a circle (o = the repeated first pt)
            on[a as usize].insert(b);
        }
        for o in 0..n as PointId {
            let set = &on[o as usize];
            if set.len() < 3 {
                continue;
            }
            // Verify numerically that they really are equidistant from o.
            let pts: Vec<PointId> = set.iter().copied().collect();
            let r2 = self.d2(o, pts[0]);
            if pts
                .iter()
                .all(|&p| (self.d2(o, p) - r2).abs() < 1e-6 * (1.0 + r2))
            {
                self.circles.push(Circle {
                    centre: o,
                    on: set.clone(),
                });
            }
        }
    }

    /// A midpoint = a point `m` with `|ma| = |mb|` (a `cong` in any argument
    /// order) and `coll(a,b,m)`, with `m` between `a,b`. This is exactly what
    /// the `midpoint`/`reflect` constructions emit.
    fn detect_midpoints(&mut self) {
        let colls = self.colls.clone();
        let congs = self.congs.clone();
        for c in &congs {
            // |c0 c1| = |c2 c3|. Find the point m shared by both pairs; then the
            // other endpoints a, b satisfy |ma| = |mb|.
            let pair1 = [c[0], c[1]];
            let pair2 = [c[2], c[3]];
            for &m in &pair1 {
                if !pair2.contains(&m) {
                    continue;
                }
                let a = if pair1[0] == m { pair1[1] } else { pair1[0] };
                let b = if pair2[0] == m { pair2[1] } else { pair2[0] };
                if a == b || a == m || b == m {
                    continue;
                }
                let collinear = colls
                    .iter()
                    .any(|s| s.contains(&a) && s.contains(&b) && s.contains(&m));
                if !collinear {
                    continue;
                }
                let (va, vb, vm) = (self.coord(a), self.coord(b), self.coord(m));
                if (vm - va).dot(vb - vm) > 0.0 && !self.midpoints.contains(&(m, a, b)) {
                    self.midpoints.push((m, a, b));
                }
            }
        }
    }

    // -- pretty helpers -----------------------------------------------------

    fn sq(&self, a: PointId, b: PointId) -> String {
        format!("{}²", self.seg(a, b))
    }
}

// ===========================================================================
// Theorem rules — each emits squared-length equations with a cited step
// ===========================================================================

fn one() -> Rat {
    Rat::one()
}
fn ri(n: i64) -> Rat {
    Rat::from_int(n)
}

/// Numerator of a ratio `t = p/(p+q)`, for the `p:q` display in Stewart's step.
fn numer_str(t: &Rat) -> String {
    match t.numer_i64() {
        Some(p) => p.to_string(),
        None => format!("{t}"),
    }
}

/// Complement `q` of a ratio `t = p/(p+q)` (i.e. `denominator − numerator`).
fn ratio_complement_str(t: &Rat) -> String {
    match (t.numer_i64(), t.denom_i64()) {
        (Some(p), Some(s)) => (s - p).to_string(),
        _ => format!("(1−{t})"),
    }
}

impl Figure {
    /// Run every applicable theorem, appending steps. Ordered so derived
    /// qualitative facts (perpendiculars, parallels) exist before the metric
    /// theorems (Pythagoras, parallel-chords) that consume them.
    fn apply_theorems(&mut self) {
        self.rule_absolute_length();
        self.rule_congruent();
        self.rule_midpoint();
        self.rule_perpendicular_from_centre();
        self.rule_thales();
        self.rule_parallel_from_perp();
        self.rule_pythagoras();
        self.rule_parallel_chords();
        self.rule_median();
        self.rule_stewart();
        self.rule_parallelogram();
        self.rule_british_flag();
    }

    fn rule_absolute_length(&mut self) {
        for (a, b, v2) in self.abs_len2.clone() {
            let mut eq = Eq::default();
            eq.add_term(atom(a, b), one());
            eq.constant = -(&v2);
            let val = v2.to_f64().sqrt();
            let text = format!(
                "|{}| = {} (given), so {} = {}.",
                self.seg(a, b),
                pretty(val),
                self.sq(a, b),
                v2
            );
            self.push_step(text, Some(eq), vec![]);
        }
    }

    fn rule_congruent(&mut self) {
        for c in self.congs.clone() {
            let (a, b, x, y) = (c[0], c[1], c[2], c[3]);
            if atom(a, b) == atom(x, y) {
                continue;
            }
            let mut eq = Eq::default();
            eq.add_term(atom(a, b), one());
            eq.add_term(atom(x, y), ri(-1));
            if eq.is_trivial() {
                continue;
            }
            let text = format!(
                "|{}| = |{}| (given), so {} = {}.",
                self.seg(a, b),
                self.seg(x, y),
                self.sq(a, b),
                self.sq(x, y)
            );
            self.push_step(text, Some(eq), vec![]);
        }
    }

    /// Intro steps of any auxiliary points among `pts` (so a fact that uses them
    /// cites the construction that introduced them).
    fn aux_prem(&self, pts: &[PointId]) -> Vec<usize> {
        pts.iter()
            .filter_map(|p| self.aux_intro.get(p).copied())
            .collect()
    }

    fn rule_midpoint(&mut self) {
        for (m, a, b) in self.midpoints.clone() {
            // AM² = ¼·AB²
            let mut eq = Eq::default();
            eq.add_term(atom(a, m), one());
            eq.add_term(atom(a, b), Rat::new(-1, 4));
            let prem = self.aux_prem(&[m, a, b]);
            let text = format!(
                "{} is the midpoint of {}, so {} = ¼·{}.",
                self.nm(m),
                self.seg(a, b),
                self.sq(a, m),
                self.sq(a, b)
            );
            self.push_step(text, Some(eq), prem);
        }
    }

    /// The perpendicular from the centre O to a chord AB passes through its
    /// midpoint M, so OM ⟂ AB — a derived right angle at M.
    fn rule_perpendicular_from_centre(&mut self) {
        let circles: Vec<(PointId, BTreeSet<PointId>)> = self
            .circles
            .iter()
            .map(|c| (c.centre, c.on.clone()))
            .collect();
        for (m, a, b) in self.midpoints.clone() {
            for (o, on) in &circles {
                if *o != m && on.contains(&a) && on.contains(&b) {
                    let text = format!(
                        "{0} lies on the chord {1} of the circle centred at {2} and is its \
                         midpoint, so {2}{0} ⟂ {1} (the perpendicular from the centre of a \
                         circle to a chord bisects it).",
                        self.nm(m),
                        self.seg(a, b),
                        self.nm(*o)
                    );
                    let s = self.push_step(text, None, vec![]);
                    // right angle at M between O and A (and O and B)
                    self.perps.push(([*o, m, a, b], s));
                }
            }
        }
    }

    /// Thales: a point C on a circle sees a diameter PQ at a right angle.
    fn rule_thales(&mut self) {
        // A diameter is a chord whose midpoint is the centre.
        let diameters: Vec<(PointId, PointId, PointId, BTreeSet<PointId>)> = self
            .midpoints
            .iter()
            .filter_map(|&(o, p, q)| {
                self.circles
                    .iter()
                    .find(|c| c.centre == o && c.on.contains(&p) && c.on.contains(&q))
                    .map(|c| (o, p, q, c.on.clone()))
            })
            .collect();
        for (o, p, q, on) in diameters {
            for &cc in &on {
                if cc != p && cc != q && cc != o {
                    let prem = self.aux_prem(&[p, q]);
                    let text = format!(
                        "{0} is a diameter of the circle, so ∠{1}{2}{3} = 90° \
                         (Thales' theorem — an angle in a semicircle is right).",
                        self.seg(p, q),
                        self.nm(p),
                        self.nm(cc),
                        self.nm(q)
                    );
                    let s = self.push_step(text, None, prem);
                    self.perps.push(([cc, p, cc, q], s));
                }
            }
        }
    }

    /// Pythagoras on every right angle at a shared vertex.
    fn rule_pythagoras(&mut self) {
        for ([a, b, c, d], src) in self.perps.clone() {
            // find the shared vertex of segments ab and cd
            let Some((v, x, y)) = shared_vertex(a, b, c, d) else {
                continue;
            };
            if x == y {
                continue;
            }
            // hypotenuse xy, legs vx, vy
            let mut eq = Eq::default();
            eq.add_term(atom(x, y), one());
            eq.add_term(atom(v, x), ri(-1));
            eq.add_term(atom(v, y), ri(-1));
            let prem = if src == usize::MAX { vec![] } else { vec![src] };
            let via = if src == usize::MAX {
                format!(" (the right angle at {} is given)", self.nm(v))
            } else {
                String::new()
            };
            let text = format!(
                "In right triangle {}{}{} (right angle at {}), by the Pythagorean theorem \
                 {} = {} + {}{}.",
                self.nm(x),
                self.nm(v),
                self.nm(y),
                self.nm(v),
                self.sq(x, y),
                self.sq(v, x),
                self.sq(v, y),
                via
            );
            self.push_step(text, Some(eq), prem);
        }
    }

    /// Apollonius: for the median AM to side BC, AB² + AC² = 2AM² + 2BM².
    fn rule_median(&mut self) {
        let n = self.coords.len() as PointId;
        for (m, b, c) in self.midpoints.clone() {
            for a in 0..n {
                if a == m || a == b || a == c {
                    continue;
                }
                let mut eq = Eq::default();
                eq.add_term(atom(a, b), one());
                eq.add_term(atom(a, c), one());
                eq.add_term(atom(a, m), ri(-2));
                eq.add_term(atom(b, m), ri(-2));
                let text = format!(
                    "By Apollonius's median theorem in triangle {}{}{} with median {}: \
                     {} + {} = 2·{} + 2·{}.",
                    self.nm(a),
                    self.nm(b),
                    self.nm(c),
                    self.seg(a, m),
                    self.sq(a, b),
                    self.sq(a, c),
                    self.sq(a, m),
                    self.sq(b, m)
                );
                self.push_step(text, Some(eq), vec![]);
            }
        }
    }

    /// Is the triple numerically collinear (a filter, never a source of facts)?
    fn numerically_collinear(&self, a: PointId, b: PointId, c: PointId) -> bool {
        let (u, v) = (self.coord(b) - self.coord(a), self.coord(c) - self.coord(a));
        let cross = u.x * v.y - u.y * v.x;
        cross.abs() < 1e-9 * (1.0 + u.norm() * v.norm())
    }

    /// Is `{wx} ∥ {yz}` an asserted parallel fact (either order of the two
    /// segments, either endpoint order within each)?
    fn has_para(&self, w: PointId, x: PointId, y: PointId, z: PointId) -> bool {
        let (s1, s2) = (atom(w, x), atom(y, z));
        self.paras
            .iter()
            .any(|(p, _)| (atom(p[0], p[1]), atom(p[2], p[3])) == (s1, s2) || (atom(p[0], p[1]), atom(p[2], p[3])) == (s2, s1))
    }

    /// Is `{wx} ⟂ {yz}` an asserted right-angle fact?
    fn has_perp(&self, w: PointId, x: PointId, y: PointId, z: PointId) -> bool {
        let (s1, s2) = (atom(w, x), atom(y, z));
        self.perps
            .iter()
            .any(|(p, _)| (atom(p[0], p[1]), atom(p[2], p[3])) == (s1, s2) || (atom(p[0], p[1]), atom(p[2], p[3])) == (s2, s1))
    }

    /// Parallelograms `ABCD` (vertices in cyclic order, diagonals `AC`, `BD`)
    /// evidenced by both pairs of opposite sides being asserted parallel.
    /// Deduplicated by the unordered pair of diagonals.
    fn find_parallelograms(&self) -> Vec<[PointId; 4]> {
        let n = self.coords.len() as PointId;
        let mut seen: BTreeSet<(Atom, Atom)> = BTreeSet::new();
        let mut out = Vec::new();
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    for d in 0..n {
                        if a == b || a == c || a == d || b == c || b == d || c == d {
                            continue;
                        }
                        // AB ∥ DC and AD ∥ BC (opposite sides of quad ABCD).
                        if !(self.has_para(a, b, d, c) && self.has_para(a, d, b, c)) {
                            continue;
                        }
                        // Segments of one line are trivially "parallel"; four
                        // collinear points are no parallelogram.
                        if self.numerically_collinear(a, b, c) || self.numerically_collinear(a, b, d) {
                            continue;
                        }
                        let key = {
                            let (x, y) = (atom(a, c), atom(b, d));
                            if x <= y { (x, y) } else { (y, x) }
                        };
                        if seen.insert(key) {
                            out.push([a, b, c, d]);
                        }
                    }
                }
            }
        }
        out
    }

    /// Rectangles `ABCD` (cyclic order): right angles at two adjacent vertices
    /// plus the remaining pair of opposite sides parallel (or a third right
    /// angle) — a characterisation that fires for the `square` construction and
    /// for figures with three asserted right angles. Deduplicated by the
    /// diagonal pair.
    fn find_rectangles(&self) -> Vec<[PointId; 4]> {
        let n = self.coords.len() as PointId;
        let mut seen: BTreeSet<(Atom, Atom)> = BTreeSet::new();
        let mut out = Vec::new();
        for a in 0..n {
            for b in 0..n {
                for c in 0..n {
                    for d in 0..n {
                        if a == b || a == c || a == d || b == c || b == d || c == d {
                            continue;
                        }
                        let right_b = self.has_perp(a, b, b, c);
                        let right_c = self.has_perp(b, c, c, d);
                        if !(right_b && right_c) {
                            continue;
                        }
                        // The right angles at B and C already force AB ∥ DC, so
                        // that parallel says nothing more (a right trapezoid has
                        // it). Closing needs the *other* pair of sides parallel
                        // or a third right angle.
                        let closes = self.has_para(a, d, b, c)
                            || self.has_perp(c, d, d, a)
                            || self.has_perp(d, a, a, b);
                        if !closes {
                            continue;
                        }
                        let key = {
                            let (x, y) = (atom(a, c), atom(b, d));
                            if x <= y { (x, y) } else { (y, x) }
                        };
                        if seen.insert(key) {
                            out.push([a, b, c, d]);
                        }
                    }
                }
            }
        }
        out
    }

    /// Stewart's theorem. For a point `D` dividing side `BC` in the fixed ratio
    /// `BD:DC = p:q` and *any* apex `A`, the section identity holds exactly:
    /// `AD² = (1−t)·AB² + t·AC² − t(1−t)·BC²` with `t = BD/BC`. This generalises
    /// Apollonius's median theorem (the `t = ½` case, handled by `rule_median`)
    /// to an arbitrary cevian. The ratio is read numerically but only used when
    /// the construction *fixes* it (`fixed_ratio` cross-checks a second instance),
    /// so it never becomes an invented fact.
    fn rule_stewart(&mut self) {
        let n = self.coords.len() as PointId;
        // Interior division points (b, d, c) with d strictly between b and c,
        // taken from the collinear sets (symbolic evidence that d ∈ line bc).
        let mut triples: Vec<(PointId, PointId, PointId, Rat)> = Vec::new();
        for set in self.colls.clone() {
            for i in 0..set.len() {
                for j in 0..set.len() {
                    for k in 0..set.len() {
                        if i == j || j == k || i == k {
                            continue;
                        }
                        let (b, d, c) = (set[i], set[j], set[k]);
                        if b >= c {
                            continue; // {b,c} unordered — avoid duplicates
                        }
                        let (vb, vd, vc) = (self.coord(b), self.coord(d), self.coord(c));
                        if (vd - vb).dot(vc - vd) <= 0.0 {
                            continue; // d not strictly between b and c
                        }
                        let Some(t) = self.fixed_ratio(b, d, c) else {
                            continue;
                        };
                        // The midpoint case is Apollonius — leave it to rule_median.
                        if (t.to_f64() - 0.5).abs() < 1e-9 {
                            continue;
                        }
                        triples.push((b, d, c, t));
                    }
                }
            }
        }
        for (b, d, c, t) in triples {
            let one_minus_t = &one() - &t;
            for a in 0..n {
                if a == b || a == c || a == d {
                    continue;
                }
                // A genuine apex (not on line bc).
                if self.numerically_collinear(a, b, c) {
                    continue;
                }
                let mut eq = Eq::default();
                eq.add_term(atom(a, d), one()); // AD²
                eq.add_term(atom(a, b), -(&one_minus_t)); // −(1−t)·AB²
                eq.add_term(atom(a, c), -(&t)); // −t·AC²
                eq.add_term(atom(b, c), &t * &one_minus_t); // +t(1−t)·BC²
                let prem = self.aux_prem(&[a, b, c, d]);
                let abc = format!("{}{}{}", self.nm(a), self.nm(b), self.nm(c));
                let text = format!(
                    "By Stewart's theorem for the cevian {ad} in triangle {abc} \
                     (D divides {bc} with {bd}:{dc} = {p}:{q}): \
                     {ad2} = {om}·{ab2} + {tt}·{ac2} − {tomt}·{bc2}.",
                    ad = self.seg(a, d),
                    bc = self.seg(b, c),
                    bd = self.seg(b, d),
                    dc = self.seg(d, c),
                    p = numer_str(&t),
                    q = ratio_complement_str(&t),
                    ad2 = self.sq(a, d),
                    om = one_minus_t,
                    ab2 = self.sq(a, b),
                    tt = t,
                    ac2 = self.sq(a, c),
                    tomt = &t * &one_minus_t,
                    bc2 = self.sq(b, c),
                );
                self.push_headline(text, Some(eq), prem);
            }
        }
    }

    /// Parallelogram law. If `ABCD` is a parallelogram (both pairs of opposite
    /// sides parallel), then `AC² + BD² = 2·AB² + 2·BC²`.
    fn rule_parallelogram(&mut self) {
        for [a, b, c, d] in self.find_parallelograms() {
            let mut eq = Eq::default();
            eq.add_term(atom(a, c), one()); // AC²
            eq.add_term(atom(b, d), one()); // BD²
            eq.add_term(atom(a, b), ri(-2)); // −2·AB²
            eq.add_term(atom(b, c), ri(-2)); // −2·BC²
            let abcd = self.poly(&[a, b, c, d]);
            let text = format!(
                "{abcd} is a parallelogram, so by the parallelogram law \
                 {ac2} + {bd2} = 2·{ab2} + 2·{bc2}.",
                ac2 = self.sq(a, c),
                bd2 = self.sq(b, d),
                ab2 = self.sq(a, b),
                bc2 = self.sq(b, c),
            );
            self.push_headline(text, Some(eq), vec![]);
        }
    }

    /// The British flag theorem. For a rectangle `ABCD` and *any* point `P`,
    /// `PA² + PC² = PB² + PD²` (the two sums over opposite corners are equal).
    fn rule_british_flag(&mut self) {
        let n = self.coords.len() as PointId;
        for [a, b, c, d] in self.find_rectangles() {
            for p in 0..n {
                if [a, b, c, d].contains(&p) {
                    continue;
                }
                let mut eq = Eq::default();
                eq.add_term(atom(p, a), one());
                eq.add_term(atom(p, c), one());
                eq.add_term(atom(p, b), ri(-1));
                eq.add_term(atom(p, d), ri(-1));
                if eq.is_trivial() {
                    continue;
                }
                let abcd = self.poly(&[a, b, c, d]);
                let text = format!(
                    "{abcd} is a rectangle, so by the British flag theorem \
                     {pa2} + {pc2} = {pb2} + {pd2}.",
                    pa2 = self.sq(p, a),
                    pc2 = self.sq(p, c),
                    pb2 = self.sq(p, b),
                    pd2 = self.sq(p, d),
                );
                self.push_headline(text, Some(eq), vec![]);
            }
        }
    }

    /// Two segments perpendicular to the *same* line are parallel. Derives the
    /// `∥` facts needed for the parallel-chords step (the heart of deriving the
    /// perpendicular-chords result from first principles, rather than citing it).
    fn rule_parallel_from_perp(&mut self) {
        let perps = self.perps.clone();
        let set = |p: PointId, q: PointId| [p.min(q), p.max(q)];
        let mut jobs: Vec<([PointId; 4], Vec<usize>, [PointId; 2])> = Vec::new();
        for i in 0..perps.len() {
            for j in (i + 1)..perps.len() {
                let ([a0, a1, a2, a3], si) = perps[i];
                let ([b0, b1, b2, b3], sj) = perps[j];
                let pl = [(set(a0, a1), set(a2, a3)), (set(a2, a3), set(a0, a1))];
                let ql = [(set(b0, b1), set(b2, b3)), (set(b2, b3), set(b0, b1))];
                // Find a common line; the remaining lines of each are parallel.
                'outer: for (la_other, la_common) in pl {
                    for (lb_other, lb_common) in ql {
                        if la_common == lb_common && la_other != lb_other {
                            let prem: Vec<usize> = [si, sj]
                                .iter()
                                .filter(|&&s| s != usize::MAX)
                                .copied()
                                .collect();
                            jobs.push((
                                [la_other[0], la_other[1], lb_other[0], lb_other[1]],
                                prem,
                                la_common,
                            ));
                            break 'outer;
                        }
                    }
                }
            }
        }
        for ([w, x, y, z], prem, common) in jobs {
            // avoid duplicates
            if self.paras.iter().any(|(p, _)| {
                (set(p[0], p[1]) == set(w, x) && set(p[2], p[3]) == set(y, z))
                    || (set(p[0], p[1]) == set(y, z) && set(p[2], p[3]) == set(w, x))
            }) {
                continue;
            }
            let text = format!(
                "{} ⟂ {} and {} ⟂ {}, so {} ∥ {}  (two lines perpendicular to the same line are parallel).",
                self.seg(w, x),
                self.seg(common[0], common[1]),
                self.seg(y, z),
                self.seg(common[0], common[1]),
                self.seg(w, x),
                self.seg(y, z),
            );
            let s = self.push_step(text, None, prem);
            self.paras.push(([w, x, y, z], s));
        }
    }

    /// Two parallel chords of a circle cut equal arcs, so the chords joining
    /// corresponding endpoints are equal: `AB ∥ ED` (both chords) ⇒ `AE = BD`
    /// (the pairing that holds is chosen from the figure). This is the
    /// elementary step behind the perpendicular-chords result.
    fn rule_parallel_chords(&mut self) {
        let paras = self.paras.clone();
        let mut jobs: Vec<(Atom, Atom, usize, [PointId; 4])> = Vec::new();
        for ([w, x, y, z], src) in paras {
            let on_circle = self
                .circles
                .iter()
                .any(|c| [w, x, y, z].iter().all(|p| c.on.contains(p)));
            if !on_circle {
                continue;
            }
            // Corresponding endpoints: pick the pairing whose cross-chords are
            // numerically equal (the true one for parallel chords).
            let d = |p: PointId, q: PointId| (self.coord(p) - self.coord(q)).norm();
            let (wy, xz) = (d(w, y), d(x, z));
            let (wz, xy) = (d(w, z), d(x, y));
            let (e1, e2) = if (wy - xz).abs() < 1e-6 * (1.0 + wy) {
                (atom(w, y), atom(x, z))
            } else if (wz - xy).abs() < 1e-6 * (1.0 + wz) {
                (atom(w, z), atom(x, y))
            } else {
                continue;
            };
            if e1 == e2 {
                continue;
            }
            jobs.push((e1, e2, src, [w, x, y, z]));
        }
        for (e1, e2, src, [w, x, y, z]) in jobs {
            let mut eq = Eq::default();
            eq.add_term(e1, one());
            eq.add_term(e2, ri(-1));
            let text = format!(
                "{} ∥ {} are parallel chords of the circle, so they cut equal arcs and \
                 {} = {} (equal chords): {} = {}.",
                self.seg(w, x),
                self.seg(y, z),
                self.seg(e1.0, e1.1),
                self.seg(e2.0, e2.1),
                self.sq(e1.0, e1.1),
                self.sq(e2.0, e2.1),
            );
            let prem = if src == usize::MAX { vec![] } else { vec![src] };
            self.push_step(text, Some(eq), prem);
        }
    }

    /// Introduce the **antipode** of each relevant circle point (the far end of
    /// the diameter through it) as an auxiliary point. This is the classical
    /// construction that lets Thales + Pythagoras derive circle length relations
    /// from first principles. `relevant` is the set of points worth reflecting
    /// (goal points and perpendicular endpoints).
    fn add_antipodes(&mut self, relevant: &BTreeSet<PointId>) {
        // (point on circle, its centre, squared radius) for relevant points.
        let mut jobs: Vec<(PointId, PointId)> = Vec::new();
        for c in &self.circles {
            for &p in &c.on {
                if relevant.contains(&p) {
                    jobs.push((p, c.centre));
                }
            }
        }
        for (p, o) in jobs {
            let coord = self.coord(o) * 2.0 - self.coord(p);
            // Skip if it coincides with an existing point.
            if self.coords.iter().any(|q| (*q - coord).norm() < 1e-6) {
                continue;
            }
            let id = self.coords.len() as PointId;
            let name = format!("{}'", self.nm(p));
            self.names.push(name.clone());
            self.coords.push(coord);
            let text = format!(
                "Let {name} be the point diametrically opposite {} on the circle (so {}{name} is a \
                 diameter through the centre {}).",
                self.nm(p),
                self.nm(p),
                self.nm(o)
            );
            let s = self.push_step(text, None, vec![]);
            self.aux_intro.insert(id, s);
            // Register its facts: on the circle, and O is the midpoint of P·P'.
            if let Some(circ) = self.circles.iter_mut().find(|c| c.centre == o) {
                circ.on.insert(id);
            }
            self.midpoints.push((o, p, id));
        }
    }
}

/// The shared endpoint `v` of segments `ab` and `cd`, with the two far ends
/// `x` (on `ab`) and `y` (on `cd`). `None` if they share no endpoint.
fn shared_vertex(
    a: PointId,
    b: PointId,
    c: PointId,
    d: PointId,
) -> Option<(PointId, PointId, PointId)> {
    for (v, x) in [(a, b), (b, a)] {
        for (w, y) in [(c, d), (d, c)] {
            if v == w {
                return Some((v, x, y));
            }
        }
    }
    None
}

// ===========================================================================
// Solving: reduce the goal by exact rational linear combination
// ===========================================================================

/// An echelon row: an equation with a chosen pivot atom and the set of source
/// step indices it was combined from.
struct Row {
    eq: Eq,
    pivot: Atom,
    deps: BTreeSet<usize>,
}

/// Reduce `eq` by the echelon `rows`, accumulating dependencies.
fn reduce(eq: &mut Eq, rows: &[Row], deps: &mut BTreeSet<usize>) {
    for r in rows {
        if let Some(c) = eq.terms.get(&r.pivot).cloned() {
            let factor = &c / eq_pivot_coeff(r);
            eq.sub_scaled(&r.eq, &factor);
            deps.extend(r.deps.iter().copied());
        }
    }
}

fn eq_pivot_coeff(r: &Row) -> &Rat {
    r.eq.terms.get(&r.pivot).unwrap()
}

impl Figure {
    /// Try to prove `goal` using only the metric steps whose index is in
    /// `allowed`. Returns the used step indices (a subset of `allowed`).
    fn prove_with(&self, goal: &Eq, allowed: &BTreeSet<usize>) -> Option<BTreeSet<usize>> {
        let mut rows: Vec<Row> = Vec::new();
        for (i, step) in self.steps.iter().enumerate() {
            if !allowed.contains(&i) {
                continue;
            }
            let Some(eq0) = &step.eq else { continue };
            let mut eq = eq0.clone();
            let mut deps: BTreeSet<usize> = BTreeSet::new();
            deps.insert(i);
            reduce(&mut eq, &rows, &mut deps);
            if let Some(pivot) = eq.first_atom() {
                rows.push(Row { eq, pivot, deps });
            }
        }
        let mut g = goal.clone();
        let mut deps: BTreeSet<usize> = BTreeSet::new();
        reduce(&mut g, &rows, &mut deps);
        (g.terms.is_empty() && g.constant.is_zero()).then_some(deps)
    }

    /// Prove `goal`, then greedily drop unnecessary theorem citations so the
    /// proof is minimal (a human proof cites only what it uses).
    fn prove(&self, goal: &Eq) -> Option<BTreeSet<usize>> {
        let all: BTreeSet<usize> = (0..self.steps.len())
            .filter(|&i| self.steps[i].eq.is_some())
            .collect();
        let mut used = self.prove_with(goal, &all)?;
        // Greedily remove one citation at a time while the goal still follows.
        loop {
            let mut removed = false;
            for &s in used.clone().iter() {
                let mut trial = used.clone();
                trial.remove(&s);
                if let Some(smaller) = self.prove_with(goal, &trial) {
                    used = smaller;
                    removed = true;
                    break;
                }
            }
            if !removed {
                break;
            }
        }
        // Anti-circularity: a proof that is a lone citation of a compound named
        // theorem merely restates it — reject, so such a goal is instead derived
        // from elementary steps (or falls through to the numeric certificate).
        if used.len() == 1 {
            let only = *used.iter().next().unwrap();
            if self.steps[only].headline {
                return None;
            }
        }
        Some(used)
    }

    /// Render the Euclidean proof from the used steps (with premise closure).
    fn render(&self, used: &BTreeSet<usize>, goal_text: &str, goal_val: f64) -> String {
        // Transitive closure over premises.
        let mut keep: BTreeSet<usize> = BTreeSet::new();
        let mut stack: Vec<usize> = used.iter().copied().collect();
        while let Some(i) = stack.pop() {
            if keep.insert(i) {
                stack.extend(self.steps[i].premises.iter().copied());
            }
        }
        let order: Vec<usize> = keep.iter().copied().collect(); // step order == creation order
        let number: BTreeMap<usize, usize> =
            order.iter().enumerate().map(|(n, &i)| (i, n + 1)).collect();

        let mut out = String::new();
        out.push_str("EUCLIDEAN PROOF\n");
        out.push_str(&format!("  Goal:  {goal_text}\n\n"));
        for (n, &i) in order.iter().enumerate() {
            let step = &self.steps[i];
            let cites: Vec<String> = step
                .premises
                .iter()
                .filter_map(|p| number.get(p).map(|k| k.to_string()))
                .collect();
            let refs = if cites.is_empty() {
                String::new()
            } else {
                format!("  [from {}]", cites.join(", "))
            };
            out.push_str(&format!("  {}. {}{}\n", n + 1, step.text, refs));
        }
        out.push_str(&format!(
            "\n  Combining the equations above gives {goal_text}  (= {}). ∎\n",
            pretty(goal_val)
        ));
        out
    }
}

// ===========================================================================
// Goal lowering (metric expression → linear equation over squared lengths)
// ===========================================================================

/// Lower one side of the goal to a linear form over squared lengths, or `None`
/// if it is not linear in squared lengths (a bare/odd length, an area, a
/// product, …) — those need the ratio layer and fall through to numerics.
fn lower(e: &MExpr, fig: &Figure) -> Option<Eq> {
    match e {
        MExpr::Num(v) => {
            let r = rat_of(*v)?;
            Some(Eq {
                terms: BTreeMap::new(),
                constant: r,
            })
        }
        MExpr::Pow(base, p) if (*p - 2.0).abs() < 1e-9 => {
            if let MExpr::Dist(a, b) = base.as_ref() {
                let (a, b) = (fig.pt(a)?, fig.pt(b)?);
                let mut eq = Eq::default();
                eq.add_term(atom(a, b), one());
                Some(eq)
            } else {
                None
            }
        }
        MExpr::Neg(x) => {
            let mut e = lower(x, fig)?;
            for c in e.terms.values_mut() {
                *c = -(&*c);
            }
            e.constant = -(&e.constant);
            Some(e)
        }
        MExpr::Add(x, y) => {
            let mut a = lower(x, fig)?;
            let b = lower(y, fig)?;
            a.sub_scaled(&b, &ri(-1));
            Some(a)
        }
        MExpr::Sub(x, y) => {
            let mut a = lower(x, fig)?;
            let b = lower(y, fig)?;
            a.sub_scaled(&b, &one());
            Some(a)
        }
        MExpr::Mul(x, y) => {
            // Only constant × linear is representable.
            let a = lower(x, fig)?;
            let b = lower(y, fig)?;
            match (a.terms.is_empty(), b.terms.is_empty()) {
                (true, _) => Some(scale_eq(&b, &a.constant)),
                (_, true) => Some(scale_eq(&a, &b.constant)),
                _ => None,
            }
        }
        MExpr::Div(x, y) => {
            // Only division by a nonzero constant stays linear.
            let a = lower(x, fig)?;
            let b = lower(y, fig)?;
            if !b.terms.is_empty() || b.constant.is_zero() {
                return None;
            }
            Some(scale_eq(&a, &b.constant.recip()))
        }
        _ => None,
    }
}

fn scale_eq(e: &Eq, k: &Rat) -> Eq {
    Eq {
        terms: e.terms.iter().map(|(a, c)| (*a, c * k)).collect(),
        constant: &e.constant * k,
    }
}

impl Figure {
    fn pt(&self, name: &str) -> Option<PointId> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| i as PointId)
    }
}

// ===========================================================================
// Driver
// ===========================================================================

/// Outcome of an attempted Euclidean proof.
pub enum Outcome {
    Proved(String),
    Unhandled(String),
}

/// Collect the point names appearing in a metric expression.
fn collect_points(e: &MExpr, out: &mut BTreeSet<String>) {
    match e {
        MExpr::Dist(a, b) => {
            out.insert(a.clone());
            out.insert(b.clone());
        }
        MExpr::Angle(a, b, c) | MExpr::Area(a, b, c) => {
            out.insert(a.clone());
            out.insert(b.clone());
            out.insert(c.clone());
        }
        MExpr::Neg(x)
        | MExpr::Sqrt(x)
        | MExpr::Cos(x)
        | MExpr::Sin(x)
        | MExpr::Tan(x)
        | MExpr::Pow(x, _) => collect_points(x, out),
        MExpr::Add(x, y) | MExpr::Sub(x, y) | MExpr::Mul(x, y) | MExpr::Div(x, y) => {
            collect_points(x, out);
            collect_points(y, out);
        }
        MExpr::Num(_) => {}
    }
}

/// Build a second, independent instance of `cons_src` and return its coordinates
/// aligned to `names` (by point name). Used only to gate numeric detections —
/// `None` when a second valid instance is unavailable (rules then stay
/// conservative and skip ratio/product detections).
pub(crate) fn second_instance(cons_src: &str, names: &[String]) -> Option<Vec<Vec2>> {
    let instances = crate::geo::build_instances(cons_src, 4).ok()?;
    if instances.len() < 2 {
        return None;
    }
    // The last instance uses the highest seed, hence is the most independent of
    // the first (which `build_algebraic` produced).
    let inst = instances.last()?;
    let map: std::collections::HashMap<&str, Vec2> =
        inst.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    names
        .iter()
        .map(|n| map.get(n.as_str()).copied())
        .collect()
}

/// Attempt a classical Euclidean proof of the metric equation `goal` about the
/// coordinate-free construction `cons_src`. Tries the base figure first, then —
/// if that is not enough — introduces classical auxiliary points (the antipodes
/// of the relevant circle points) and derives the result from elementary
/// theorems rather than citing a high-level theorem that would *be* the goal.
pub fn prove_euclidean(cons_src: &str, goal: &str) -> Result<Outcome, String> {
    let (lhs, rhs) = crate::metric::parse_equation(goal)?;
    let algfig = crate::geo::build_algebraic(cons_src)?;

    // A second, independently re-sampled instance (aligned to `algfig` by name)
    // lets the theorem rules gate any numerically-read quantity — a division
    // ratio, an equal product — on the construction actually fixing it.
    let coords2 = second_instance(cons_src, &algfig.names);

    let mut names: BTreeSet<String> = BTreeSet::new();
    collect_points(&lhs, &mut names);
    collect_points(&rhs, &mut names);

    for use_aux in [false, true] {
        let mut fig = Figure::gather(&algfig, coords2.clone());
        if use_aux {
            let mut relevant: BTreeSet<PointId> = names.iter().filter_map(|n| fig.pt(n)).collect();
            for p in &algfig.preds {
                if p.name == "perp" {
                    relevant.extend(p.points.iter().copied());
                }
            }
            fig.add_antipodes(&relevant);
        }
        fig.apply_theorems();

        // General metric hypotheses imposed by `point:` become given equations
        // (squared-length ones — the ratio prover handles the multiplicative ones).
        let mut hyp_steps: Vec<(String, Eq)> = Vec::new();
        for (hl, hr) in &algfig.metric_hyps {
            if let (Some(le), Some(re)) = (lower(hl, &fig), lower(hr, &fig)) {
                let mut eq = le;
                eq.sub_scaled(&re, &one());
                if !eq.is_trivial() {
                    hyp_steps.push((
                        format!("{} = {} (given).", hl.to_display(), hr.to_display()),
                        eq,
                    ));
                }
            }
        }
        for (text, eq) in hyp_steps {
            fig.push_step(text, Some(eq), vec![]);
        }

        let (Some(l), Some(r)) = (lower(&lhs, &fig), lower(&rhs, &fig)) else {
            return Ok(Outcome::Unhandled(
                "the goal is not a linear relation among squared lengths (needs the ratio layer)"
                    .to_string(),
            ));
        };
        let mut goal_eq = l.clone();
        goal_eq.sub_scaled(&r, &one());
        let goal_val = eval_numeric(&lhs, &fig).unwrap_or(f64::NAN);

        if let Some(used) = fig.prove(&goal_eq) {
            if !used.is_empty() {
                return Ok(Outcome::Proved(fig.render(&used, goal, goal_val)));
            }
        }
    }
    Ok(Outcome::Unhandled(
        "no chain of the known theorems reaches the goal".to_string(),
    ))
}

/// Numerically evaluate a (linear-in-squared-length) expression for display.
fn eval_numeric(e: &MExpr, fig: &Figure) -> Option<f64> {
    Some(match e {
        MExpr::Num(v) => *v,
        MExpr::Pow(b, p) if (*p - 2.0).abs() < 1e-9 => {
            if let MExpr::Dist(x, y) = b.as_ref() {
                fig.d2(fig.pt(x)?, fig.pt(y)?)
            } else {
                return None;
            }
        }
        MExpr::Neg(x) => -eval_numeric(x, fig)?,
        MExpr::Add(x, y) => eval_numeric(x, fig)? + eval_numeric(y, fig)?,
        MExpr::Sub(x, y) => eval_numeric(x, fig)? - eval_numeric(y, fig)?,
        MExpr::Mul(x, y) => eval_numeric(x, fig)? * eval_numeric(y, fig)?,
        MExpr::Div(x, y) => eval_numeric(x, fig)? / eval_numeric(y, fig)?,
        _ => return None,
    })
}

/// Render a value as a tidy closed form (integer, fraction, or `a√b`). Shared
/// with the ratio prover.
pub(crate) fn pretty_len(v: f64) -> String {
    pretty(v)
}

/// Render a value as a tidy closed form (integer, fraction, or `a√b`).
fn pretty(v: f64) -> String {
    if v.abs() < 1e-9 {
        return "0".into();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let a = v.abs();
    if (a - a.round()).abs() < 1e-6 {
        return format!("{sign}{}", a.round() as i64);
    }
    for q in 2..=12i64 {
        let p = a * q as f64;
        if (p - p.round()).abs() < 1e-6 {
            return format!("{sign}{}/{}", p.round() as i64, q);
        }
    }
    let sq = a * a;
    for q in 1..=12i64 {
        let m = sq * q as f64;
        if (m - m.round()).abs() < 1e-5 {
            let m = m.round() as i64;
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
    format!("{sign}{a:.4}")
}

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

    fn proved(cons: &str, goal: &str) -> bool {
        match prove_euclidean(cons, goal) {
            Ok(Outcome::Proved(p)) => {
                assert!(p.contains("EUCLIDEAN PROOF"));
                true
            }
            Ok(Outcome::Unhandled(e)) => {
                eprintln!("unhandled: {e}");
                false
            }
            Err(e) => {
                eprintln!("error: {e}");
                false
            }
        }
    }

    #[test]
    fn pythagoras_direct() {
        assert!(proved(
            "A = free\nB = free\nC = point: perp(C,A,C,B)",
            "dist(A,B)^2 = dist(C,A)^2 + dist(C,B)^2"
        ));
    }

    #[test]
    fn romanian_part_a_am() {
        // Circle r=6, M midpoint of chord AB, OM=3 ⇒ AM² = 27 (AM = 3√3).
        assert!(proved(
            "O = free\nM = point: dist(O,M)=3\nA = point: dist(O,A)=6, perp(A,M,O,M)\n\
             B = point: dist(O,B)=6, coll(A,M,B)",
            "dist(A,M)^2 = 27"
        ));
    }

    #[test]
    fn romanian_part_b_perpendicular_chords() {
        assert!(proved(
            "O = free\nA = point: dist(O,A)=6\nB = point: dist(O,B)=6\n\
             C = point: dist(O,C)=6\nD = point: dist(O,D)=6, perp(A,B,C,D)",
            "dist(A,C)^2 + dist(B,D)^2 = 144"
        ));
    }

    #[test]
    fn stewart_concrete_cevian() {
        // BC = 6, D on BC with BD = 2 (so BD:DC = 1:2), AB = 5, AC = 4
        // ⇒ AD² = 14 by Stewart's theorem.
        assert!(proved(
            "B = free\nC = point: dist(B,C)=6\nD = point: coll(B,D,C), dist(B,D)=2\n\
             A = point: dist(A,B)=5, dist(A,C)=4",
            "dist(A,D)^2 = 14"
        ));
    }

    #[test]
    fn parallelogram_law_is_not_circularly_proved() {
        // The parallelogram law as a *goal* must not be "proved" by citing the
        // parallelogram law — it falls through to another route (here, none).
        assert!(!proved(
            "A = free\nB = free\nC = free\nD = parallelogram(A, B, C)",
            "dist(A,C)^2 + dist(B,D)^2 = 2*dist(A,B)^2 + 2*dist(B,C)^2"
        ));
    }

    #[test]
    fn rejects_false() {
        assert!(!proved(
            "A = free\nB = free\nC = free",
            "dist(A,B)^2 = dist(A,C)^2 + dist(B,C)^2"
        ));
    }
}
