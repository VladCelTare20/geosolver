use super::{of_atom, of_fact, of_theorem_key, Forbid, Thm};
use crate::human::model::{AtomKey, Block, HumanProof, Reason, Sentence, Stmt};
use crate::human::trace::EngineTrace;
use crate::proof::{Fact, FactId, ProofLog};

pub fn raw(facts: &[Fact], steps: &[FactId], forbid: Forbid) -> Vec<(FactId, Thm)> {
    if forbid.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for &f in steps {
        let Some(fact) = facts.get(f as usize) else {
            continue;
        };
        for t in of_fact(fact) {
            if forbid.has(t) {
                out.push((f, t));
            }
        }
    }
    out
}

pub fn raw_log(log: &ProofLog, deps: &[FactId], forbid: Forbid) -> Vec<(FactId, Thm)> {
    if forbid.is_empty() {
        return Vec::new();
    }
    raw(&log.facts, &log.closure(deps), forbid)
}

fn reason_thms(t: &EngineTrace, r: &Reason, out: &mut Vec<(Thm, String)>) {
    let fact_thms = |f: FactId, out: &mut Vec<(Thm, String)>| {
        if let Some(fact) = t.facts.get(f as usize) {
            for th in of_fact(fact) {
                out.push((th, format!("engine fact {f}")));
            }
        }
    };
    match r {
        Reason::Hyp { .. } | Reason::Lemma { .. } => {}
        Reason::Atom { key, stmt, .. } => {
            for &th in of_atom(*key) {
                out.push((th, format!("atom {}", key.as_str())));
            }
            if inscribed(*key) && matches!(stmt, Stmt::Cyclic { .. }) {
                out.push((
                    Thm::InscribedConverse,
                    format!("atom {} concluding a circle", key.as_str()),
                ));
            }
        }
        Reason::Claim { fact, .. } | Reason::Engine { fact } => fact_thms(*fact, out),
        Reason::Fact { fact, because, .. } => {
            fact_thms(*fact, out);
            for b in because {
                reason_thms(t, b, out);
            }
        }
    }
}

fn inscribed(key: AtomKey) -> bool {
    matches!(
        key,
        AtomKey::Inscribed | AtomKey::CyclicOpposite | AtomKey::CyclicExterior
    )
}

fn sentence_thms(t: &EngineTrace, s: &Sentence, out: &mut Vec<(Thm, String)>) {
    match s {
        Sentence::Chain {
            then: Some(Stmt::Cyclic { .. }),
            ..
        } => {
            out.push((
                Thm::InscribedConverse,
                "an angle chain concluding a circle".into(),
            ));
        }
        Sentence::Because {
            stmt: Stmt::Cyclic { .. },
            reasons,
            ..
        } if reasons
            .iter()
            .any(|r| matches!(r, Reason::Atom { key, .. } if inscribed(*key))) =>
        {
            out.push((
                Thm::InscribedConverse,
                "inscribed angles concluding a circle".into(),
            ));
        }
        _ => {}
    }
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => {
            for l in links {
                for r in &l.reasons {
                    reason_thms(t, r, out);
                }
            }
        }
        Sentence::Because { reasons, .. } | Sentence::Pooled { reasons, .. } => {
            for r in reasons {
                reason_thms(t, r, out);
            }
        }
        Sentence::Theorem { key, reasons, .. } => {
            for &th in of_theorem_key(*key) {
                out.push((th, format!("theorem {}", key.as_str())));
            }
            for r in reasons {
                reason_thms(t, r, out);
            }
        }
        Sentence::Raw { engine_fact, .. } => {
            if let Some(fact) = t.facts.get(*engine_fact as usize) {
                for th in of_fact(fact) {
                    out.push((th, format!("engine step {engine_fact}")));
                }
            }
        }
    }
}

pub fn block(t: &EngineTrace, b: &Block, forbid: Forbid) -> Vec<(Thm, String)> {
    let mut all = Vec::new();
    for &f in &b.engine_facts {
        if let Some(fact) = t.facts.get(f as usize) {
            for th in of_fact(fact) {
                all.push((th, format!("engine fact {f}")));
            }
        }
    }
    for s in &b.body {
        sentence_thms(t, s, &mut all);
    }
    all.retain(|(th, _)| forbid.has(*th));
    all
}

pub fn human(t: &EngineTrace, hp: &HumanProof) -> Vec<(u16, Thm, String)> {
    let forbid = t.forbid;
    if forbid.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for b in &hp.blocks {
        for (th, what) in block(t, b, forbid) {
            out.push((b.id, th, what));
        }
    }
    out
}

const MARKERS: &[(Thm, &[&str], &[&str])] = &[
    (Thm::Midline, &["midline", "midsegment"], &[]),
    (Thm::Pythagoras, &["pythagoras", "pythagorean"], &[]),
    (Thm::Menelaus, &["menelaus"], &[]),
    (Thm::Ceva, &["ceva"], &[]),
    (Thm::Simson, &["simson"], &[]),
    (Thm::Miquel, &["miquel"], &[]),
    (Thm::Reim, &["reim's", "reim "], &[]),
    (Thm::Stewart, &["stewart"], &[]),
    (
        Thm::Apollonius,
        &["apollonius", "median theorem", "median law"],
        &[],
    ),
    (Thm::RadicalAxis, &["radical axis", "radical centre"], &[]),
    (Thm::Monge, &["monge"], &[]),
    (Thm::Homothety, &["homothety", "centre of similitude"], &[]),
    (Thm::EqualTangents, &["equal tangents"], &[]),
    (Thm::MedianHypotenuse, &["median to the hypotenuse"], &[]),
    (
        Thm::LawOfSines,
        &["law of sines"],
        &["extended law of sines"],
    ),
    (Thm::LawOfCosines, &["law of cosines"], &[]),
    (Thm::ExtLawOfSines, &["extended law of sines"], &[]),
    (Thm::Thales, &["thales"], &["converse of thales"]),
    (
        Thm::Intercept,
        &["intercept theorem", "basic proportionality"],
        &["converse of the intercept theorem"],
    ),
    (
        Thm::BisectorThm,
        &["angle bisector theorem", "angle-bisector theorem"],
        &[
            "converse of the angle bisector theorem",
            "external angle bisector theorem",
            "external angle-bisector theorem",
        ],
    ),
    (
        Thm::ExtBisectorThm,
        &[
            "external angle bisector theorem",
            "external angle-bisector theorem",
        ],
        &["converse of the external angle bisector theorem"],
    ),
    (
        Thm::Incentre,
        &["bisectors concur", "angle bisectors concur"],
        &[],
    ),
    (Thm::TriangleEquality, &["triangle inequality"], &[]),
    (Thm::BritishFlag, &["british flag", "british-flag"], &[]),
    (Thm::ParallelogramLaw, &["parallelogram law"], &[]),
    (
        Thm::TangentSecant,
        &["tangent–secant", "tangent-secant"],
        &[],
    ),
    (
        Thm::PowerOfPoint,
        &["power of a point", "power of the point"],
        &["converse of the power of a point"],
    ),
    (Thm::GeometricMean, &["leg theorem", "geometric mean"], &[]),
];

fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

pub fn text(forbid: Forbid, text: &str) -> Vec<Thm> {
    if forbid.is_empty() {
        return Vec::new();
    }
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    for (th, needles, minus) in MARKERS {
        if !forbid.has(*th) {
            continue;
        }
        let plus: usize = needles.iter().map(|n| count(&lower, n)).sum();
        let less: usize = minus.iter().map(|n| count(&lower, n)).sum();
        if plus > less {
            out.push(*th);
        }
    }
    out
}
