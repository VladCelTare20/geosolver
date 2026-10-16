use super::*;

#[test]
fn info_table_is_in_enum_order_with_unique_keys() {
    assert_eq!(INFO.len(), ALL.len());
    for (i, t) in ALL.iter().enumerate() {
        assert_eq!(*t as usize, i, "{t:?} is out of order in ALL");
        assert_eq!(INFO[i].thm, *t, "INFO[{i}] is not {t:?}");
        assert_eq!(Thm::by_key(t.key()), Some(*t));
        assert!(!t.name_en().is_empty() && !t.name_ro().is_empty());
        assert!(!INFO[i].cited_by.is_empty());
    }
    let mut keys: Vec<&str> = ALL.iter().map(|t| t.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), ALL.len());
}

#[test]
fn the_dependency_graph_is_acyclic() {
    let mut state = vec![0u8; ALL.len()];
    fn visit(t: Thm, state: &mut Vec<u8>, path: &mut Vec<Thm>) {
        match state[t as usize] {
            2 => return,
            1 => panic!("cycle through {:?}", path),
            _ => {}
        }
        state[t as usize] = 1;
        path.push(t);
        for (u, _) in t.info().uses {
            visit(*u, state, path);
        }
        path.pop();
        state[t as usize] = 2;
    }
    for t in ALL {
        visit(t, &mut state, &mut Vec::new());
    }
    for t in ALL {
        assert!(!descendants(t).contains(&t), "{t:?} is its own descendant");
    }
}

#[test]
fn axioms_have_no_uses_and_theorems_have_one_with_a_reason() {
    for t in ALL {
        let i = t.info();
        match i.kind {
            Kind::Calculus | Kind::Axiom => assert!(i.uses.is_empty(), "{t:?}"),
            Kind::Theorem => {
                assert!(
                    !i.uses.is_empty(),
                    "{t:?} is a theorem derived from nothing"
                );
                for (u, why) in i.uses {
                    assert!(!why.is_empty(), "{t:?} uses {u:?} without a reason");
                }
            }
        }
        if !i.avoidable {
            assert!(
                i.kind != Kind::Theorem,
                "{t:?}: only foundations are built in"
            );
        }
    }
}

#[test]
fn forbidden_sets_contain_the_theorem_and_everything_derived_from_it() {
    let f = Forbid::closure_of(&[Thm::Pythagoras]);
    for t in [
        Thm::Pythagoras,
        Thm::PythagorasConverse,
        Thm::Stewart,
        Thm::Apollonius,
        Thm::ParallelogramLaw,
        Thm::BritishFlag,
    ] {
        assert!(f.has(t), "{t:?}");
    }
    assert!(!f.has(Thm::GeometricMean));
    assert!(!f.has(Thm::SimilarityAa));
    let m = Forbid::closure_of(&[Thm::Menelaus]);
    for t in [
        Thm::Ceva,
        Thm::CevaConverse,
        Thm::MenelausConverse,
        Thm::Monge,
    ] {
        assert!(m.has(t), "{t:?}");
    }
    assert!(!m.has(Thm::Intercept));
    let s = Forbid::closure_of(&[Thm::SimilarityAa]);
    for t in [
        Thm::Intercept,
        Thm::Menelaus,
        Thm::Ceva,
        Thm::BisectorThm,
        Thm::GeometricMean,
        Thm::Pythagoras,
    ] {
        assert!(s.has(t), "{t:?}");
    }
    for t in ALL {
        let c = Forbid::closure_of(&[t]);
        assert!(c.has(t));
        for d in descendants(t) {
            assert!(c.has(d));
            assert!(d.info().uses.iter().any(|(u, _)| c.has(*u)));
        }
        assert_eq!(c.unavoidable().is_some(), !t.avoidable(), "{t:?}");
    }
}

#[test]
fn every_atom_and_theorem_key_has_an_identity_or_is_a_documented_label() {
    use crate::human::model::{AtomKey, TheoremKey};
    for k in AtomKey::ALL {
        let ts = of_atom(k);
        if ts.is_empty() {
            assert!(k.is_config_label(), "{k:?} has no theorem identity");
        }
    }
    for k in TheoremKey::ALL {
        let ts = of_theorem_key(k);
        if ts.is_empty() {
            assert_eq!(k, TheoremKey::Other, "{k:?} has no theorem identity");
        }
    }
}

fn engine_sources() -> Vec<&'static str> {
    vec![
        include_str!("../engine.rs"),
        include_str!("../engine/classics.rs"),
        include_str!("../engine/sqlen.rs"),
        include_str!("../engine/trig.rs"),
    ]
}

fn string_after(part: &str) -> Option<String> {
    let rest = part.trim_start();
    let rest = rest.strip_prefix('"')?;
    rest.split_once('"').map(|(n, _)| n.to_string())
}

#[test]
fn every_named_engine_rule_has_a_theorem_identity() {
    let mut names: Vec<String> = Vec::new();
    for src in engine_sources() {
        for marker in [
            "Reason::Theorem(",
            "Reason::Formula(",
            ".reason(",
            "const EQUAL_SINES: &str =",
        ] {
            for part in src.split(marker).skip(1) {
                if let Some(n) = string_after(part) {
                    names.push(n);
                }
            }
        }
        for part in src.split("\"sine of ").skip(1) {
            if let Some((n, _)) = part.split_once('"') {
                names.push(format!("sine of {n}"));
            }
        }
        for part in src.split("=> (scaled(").skip(1) {
            let _ = part;
        }
        for line in src.lines() {
            let l = line.trim();
            if (l.starts_with("\"double-angle formula\"")
                || l.starts_with("\"triple-angle formula\""))
                && l.ends_with(',')
            {
                names.push(l.trim_end_matches(',').trim_matches('"').to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    assert!(names.len() >= 20, "{names:?}");
    for n in &names {
        assert!(
            of_name(n).is_some(),
            "the engine names {n:?} but theory has no identity for it"
        );
    }
    for (n, ts) in ENGINE_NAMES {
        assert!(!ts.is_empty(), "{n}");
        assert!(
            crate::human::classify::theorem_key(n) != crate::human::model::TheoremKey::Other,
            "{n} is not classified by the writer"
        );
    }
}

#[test]
fn every_engine_reason_variant_maps_to_a_theorem_or_is_a_hypothesis() {
    use crate::proof::Reason;
    let all = [
        Reason::Assumption("x".into()),
        Reason::Construction("x".into()),
        Reason::SimilarTriangles((0, 1, 2), (3, 4, 5)),
        Reason::SimilarTriangles((0, 1, 2), (0, 2, 1)),
        Reason::Concyclic(vec![0, 1, 2, 3]),
        Reason::Collinear(vec![0, 1, 2]),
        Reason::EqualRadius(0, vec![1, 2, 3]),
        Reason::PointMerge(0, 1),
        Reason::TangentMerge(0, 1),
        Reason::TransferAddMul((0, 1), (2, 3)),
        Reason::TransferArcChord((0, 1), (2, 3)),
        Reason::Theorem("Menelaus' theorem", vec![]),
        Reason::Formula("law of sines", String::new(), vec![]),
    ];
    for r in &all {
        let ts = of_reason(r);
        match r {
            Reason::Assumption(_) | Reason::Construction(_) => assert!(ts.is_empty()),
            _ => assert!(!ts.is_empty(), "{r:?}"),
        }
    }
}

#[test]
fn the_graph_text_names_every_identity() {
    let g = graph_text();
    for t in ALL {
        assert!(g.contains(t.key()), "{t:?}");
    }
}
