use ddar::geo::compile;
use ddar::human::{
    self, AuxInfo, BlockKind, EngineTrace, Expr, HumanProof, Opts, Reason, Sentence, Stmt,
};
use ddar::predicate::{Predicate, Problem};
use ddar::proof::FactId;
use ddar::rational::Rat;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Case {
    name: &'static str,
    problem: Problem,
    aux: Vec<AuxInfo>,
}

struct Solved {
    trace: EngineTrace,
    goal: Predicate,
    deps: Vec<FactId>,
    aux: Vec<AuxInfo>,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn geo_case(name: &'static str, rel: &str) -> Case {
    let src = std::fs::read_to_string(root().join(rel)).expect("geo file");
    let c = compile(&src).expect("compiles");
    Case {
        name,
        problem: c.problem,
        aux: Vec::new(),
    }
}

fn imo_2004_p1() -> Case {
    let text = "a b c = triangle a b c; o = midpoint o b c; m = on_circle m o b, on_line m a b; n = on_circle n o b, on_line n a c; r = angle_bisector r b a c, angle_bisector r m o n; o1 = circle o1 b m r; o2 = circle o2 c n r; p = on_circle p o1 r, on_circle p o2 r; f = foot f b a r ? coll p b c";
    let problem = ddar::corpus::translate_text("imo_2004_p1", text, 1)
        .expect("translates")
        .problem;
    let first = problem.points.len() - 1;
    let aux = human::aux_infos(&problem, first, &["f = foot(b -> ar)".to_string()]);
    Case {
        name: "imo_2004_p1",
        problem,
        aux,
    }
}

fn cases() -> Vec<Case> {
    vec![
        geo_case(
            "orthocenter_reflection",
            "examples/orthocenter_reflection.geo",
        ),
        geo_case("euler_line", "examples/euler_line.geo"),
        geo_case(
            "orthocenter_vertex_distance",
            "examples/named/orthocenter_vertex_distance.geo",
        ),
        imo_2004_p1(),
        geo_case("imo_2023_p2", "tests/golden/human/imo_2023_p2.geo"),
    ]
}

fn solve(c: &Case) -> Solved {
    let (_, trace, deps) = ddar::runner::solve_problem_with_trace(&c.problem)
        .expect("solves")
        .unwrap_or_else(|| panic!("{} is not proved directly", c.name));
    Solved {
        trace,
        goal: c.problem.goal.clone().expect("goal"),
        deps,
        aux: c.aux.clone(),
    }
}

fn strict() -> Opts {
    Opts {
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        strict: true,
    }
}

fn write(s: &Solved, opts: &Opts) -> HumanProof {
    human::write(&s.trace, &s.goal, &s.deps, &s.aux, opts)
}

fn verify(s: &Solved, hp: &HumanProof) -> Vec<human::check::Violation> {
    human::verify(&s.trace, &s.goal, &s.deps, &s.aux, hp)
}

fn golden_path(name: &str) -> PathBuf {
    root()
        .join("tests/golden/human")
        .join(format!("{name}.en.txt"))
}

#[test]
fn golden_examples_render_as_recorded() {
    let bless = std::env::var_os("HP_BLESS").is_some();
    let mut diffs = Vec::new();
    for c in cases() {
        let s = solve(&c);
        let hp = write(&s, &strict());
        assert!(hp.available, "{}: writer gave up", c.name);
        assert!(verify(&s, &hp).is_empty(), "{}: checker violations", c.name);
        let text = human::render_en(&s.trace, &hp, &s.aux);
        let path = golden_path(c.name);
        if bless {
            std::fs::write(&path, &text).unwrap();
            continue;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        if want != text {
            diffs.push(format!(
                "--- {} (expected {})\n{}\n--- got\n{}",
                c.name,
                path.display(),
                want,
                text
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "golden text changed (HP_BLESS=1 to accept):\n{}",
        diffs.join("\n")
    );
}

#[test]
fn golden_examples_have_the_planned_shape() {
    let by: Vec<(Case, HumanProof, String)> = cases()
        .into_iter()
        .map(|c| {
            let s = solve(&c);
            let hp = write(&s, &strict());
            let t = human::render_en(&s.trace, &hp, &s.aux);
            (c, hp, t)
        })
        .collect();
    let get = |n: &str| by.iter().find(|x| x.0.name == n).unwrap();
    let numbered = |hp: &HumanProof, t: &str| {
        for (i, b) in hp.blocks.iter().enumerate() {
            assert_eq!(b.step as usize, i + 1, "steps are numbered in order");
            assert!(
                t.contains(&format!("{}. ", i + 1)),
                "step {} is not printed:\n{t}",
                i + 1
            );
        }
        for b in &hp.blocks {
            for s in &b.body {
                if let Sentence::Chain { links, .. } | Sentence::Computation { links, .. } = s {
                    assert!(
                        links
                            .iter()
                            .filter(|l| !human::barem::identity_only(l))
                            .count()
                            <= 8
                            && links.len() <= 12,
                        "a chain longer than eight links:\n{t}"
                    );
                }
            }
        }
        assert!(
            !t.contains("Combining") && !t.contains("Angle chasing") && !t.contains("Claim "),
            "{t}"
        );
        assert!(t.trim_end().ends_with(", as required. \u{220e}"), "{t}");
    };

    let (_, hp, t) = get("orthocenter_reflection");
    numbered(hp, t);
    assert!(t.contains("BC is the perpendicular bisector of HK"), "{t}");
    assert!(t.contains("so A, H, K are collinear. (2)"), "{t}");
    assert!(!t.contains("read from the figure"), "{t}");
    assert!(
        t.contains("K is the reflection of H in BC, so BC is the perpendicular bisector of HK"),
        "{t}"
    );
    assert!(t.contains("H lies on the segment KA"), "{t}");
    assert!(
        !t.contains("counter-clockwise"),
        "only figure angles are used:\n{t}"
    );
    assert!(t.contains("C, K lie on the same side of AB, so by the converse of the inscribed angle theorem, A, B, C, K lie on one circle, as required. \u{220e}"), "{t}");

    let (_, hp, t) = get("euler_line");
    numbered(hp, t);
    assert!(t.contains("Let M be the midpoint of BC."), "{t}");
    assert!(t.contains("OM is the perpendicular bisector of BC"), "{t}");
    assert!(t.contains("MN is a midline of"), "{t}");
    assert!(
        t.contains("Hence \u{25b3}AHB \u{223c} \u{25b3}MON (AA). (5)"),
        "{t}"
    );
    assert!(
        t.contains("Hence \u{25b3}AHG \u{223c} \u{25b3}MOG (SAS). (6)"),
        "{t}"
    );
    assert!(
        t.contains("G is the centroid of \u{25b3}ABC, so AG = 2\u{b7}GM."),
        "{t}"
    );
    assert!(t.contains("= 1 / 2      [AG = 2\u{b7}GM, by (1)]"), "{t}");
    assert!(!t.contains("\u{25b3}ABG \u{223c} \u{25b3}MNG"), "{t}");
    assert!(t.contains("\u{25b3}AHG \u{223c} \u{25b3}MOG, by (6) and A, G, M are collinear, so \u{2220}AGH = \u{2220}MGO. \u{2220}AGH = \u{2220}MGO and H, O lie on opposite sides of AM, so by the converse of the vertical angles theorem, O, G, H lie on one line, as required. \u{220e}"), "{t}");
    assert!(!t.contains("counter-clockwise"), "no directed angles:\n{t}");

    let (_, hp, t) = get("orthocenter_vertex_distance");
    numbered(hp, t);
    assert!(t.contains("R = OA = OB = OC for its circumradius."), "{t}");
    assert!(
        t.contains(
            "Plan: it suffices to show that AH = 2R\u{b7}cos A and M\u{2090}O = R\u{b7}cos A."
        ),
        "{t}"
    );
    assert!(t.contains("We show that AH = 2R\u{b7}cos A:"), "{t}");
    assert!(
        t.contains("so AB = 2R\u{b7}sin C."),
        "a one-line computation is a sentence:\n{t}"
    );
    assert!(t.to_lowercase().contains("extended law of sines"), "{t}");
    assert!(t.contains("law of sines in \u{25b3}BHA"), "{t}");
    assert!(
        !t.contains("sin\u{2220}COA")
            && !t.contains("sin\u{2220}BOA")
            && !t.contains("sin\u{2220}COB"),
        "{t}"
    );
    assert!(
        t.lines()
            .filter(|l| l.trim_start().starts_with('='))
            .count()
            <= 5,
        "{t}"
    );
    assert!(
        t.contains("so AH = 2\u{b7}M\u{2090}O, as required. \u{220e}"),
        "{t}"
    );
    assert!(
        t.contains("central angle \u{2220}BOC = 2\u{2220}BAC"),
        "the angle at O on chord BC justifies \u{2220}OBM\u{2090} = 90\u{b0} \u{2212} A:\n{t}"
    );
    assert!(
        !t.contains("|sin")
            && !t.contains("R\u{b7}R")
            && !t.contains("R\u{b2}\u{b7}sin\u{2220}OCB / R"),
        "{t}"
    );
    let laws = |b: &ddar::human::Block| {
        t.lines().count() > 0
            && b.body.iter().all(|s| match s {
                Sentence::Computation { links, .. } => {
                    let mut tris: Vec<String> = Vec::new();
                    for l in links {
                        for r in &l.reasons {
                            if let ddar::human::Reason::Fact {
                                stmt: ddar::human::Stmt::Formula { text, pts },
                                ..
                            } = r
                            {
                                if text.starts_with("law of sines") {
                                    let mut p = pts.clone();
                                    p.sort_unstable();
                                    tris.push(format!("{p:?}"));
                                }
                            }
                        }
                    }
                    tris.sort();
                    tris.dedup();
                    tris.len() <= 3
                }
                _ => true,
            })
    };
    assert!(
        hp.blocks.iter().all(laws),
        "a trigonometric step applies the law of sines in more than three triangles:\n{t}"
    );
    assert!(!hp.blocks.last().unwrap().engine_facts.is_empty());

    let (_, hp, t) = get("imo_2004_p1");
    numbered(hp, t);
    assert!(
        t.contains("Let \u{3c9}\u{2082} be the circle through C, N, R, P."),
        "{t}"
    );
    assert!(
        t.contains("Let F be the foot of the perpendicular from B to AR."),
        "{t}"
    );
    assert!(t.contains("OF is the perpendicular bisector of BN"), "{t}");
    assert!(
        t.contains("so by the converse of the inscribed angle theorem, R, N, F, O are concyclic."),
        "{t}"
    );
    assert!(t.contains("\u{2220}BFA = 90\u{b0} and \u{2220}BNA = 90\u{b0}, so by the converse of Thales' theorem, F, N, B, A are concyclic."), "two right angles on BA need no side condition:\n{t}");
    assert!(
        t.contains("= \u{bd}\u{b7}\u{2220}NOM    [central angle"),
        "{t}"
    );
    assert!(
        t.contains("NC \u{27c2} NB, by (1) and OF \u{27c2} NB, by (4), so NC \u{2225} OF. (5)"),
        "{t}"
    );
    assert!(
        t.contains("= \u{2220}FON    [NC \u{2225} OF, by (5); alternate angles]"),
        "{t}"
    );
    assert!(t.contains("power of the point A"), "{t}");
    assert!(
        t.contains("so A lies on their radical axis, the line PR; hence R, A, F, P are collinear."),
        "{t}"
    );
    assert!(
        t.contains("Hence P lies on the line BC, as required. \u{220e}") && !t.contains("\u{2221}"),
        "{t}"
    );

    let (_, hp, t) = get("imo_2023_p2");
    numbered(hp, t);
    assert!(
        t.contains("Let \u{3a9} be the circumcircle of triangle ABC, with centre O"),
        "{t}"
    );
    assert!(
        t.contains("2. We show that NB = NC:") && t.contains("Hence NB = NC. (2)"),
        "NB = NC is used twice, so it is its own step:\n{t}"
    );
    assert!(!t.contains("NB = NC (inscribed angles"), "{t}");
    assert!(t.contains("T, E lie on the same side of SO, so by the converse of the inscribed angle theorem, T, E, S, O are concyclic. (5)"), "{t}");
    assert!(
        t.contains("T, P lie on the same side of ED, so by the converse of the inscribed angle theorem, T, P, E, D are concyclic. (8)"),
        "the concyclicity that used directed angles now reads with ordinary angles:\n{t}"
    );
    assert!(
        !t.lines().any(|l| l
            .split('[')
            .next()
            .unwrap_or("")
            .trim_end()
            .ends_with("= 0\u{b0}"))
            && t.lines()
                .filter(|l| l.contains("\u{2221}") && !l.starts_with("\u{2221}(a, b)"))
                .all(|l| l.contains("\u{2261}") || l.trim_start().starts_with('[')),
        "0\u{b0} and 180\u{b0} are never identified by =; directed rows are congruences:\n{t}"
    );
    assert!(
        t.contains("[exterior angle of the cyclic quadrilateral"),
        "{t}"
    );
    assert!(
        !t.contains("\nSo T, P, X, O are concyclic")
            && !t.contains("\nSo T, P, E, D are concyclic"),
        "every concyclicity names its converse:\n{t}"
    );
    assert!(t.contains("We show that XA = XP:"), "{t}");
    assert!(t.contains("\u{25b3}OAX \u{2245} \u{25b3}OPX"), "{t}");
    assert!(
        t.contains("so \u{2221}BAX \u{2261} \u{2221}XAC (mod 180\u{b0}), as required. \u{220e}"),
        "X is on the bisector's line beyond A in this figure, so AX does not bisect \u{2220}BAC as a ray:\n{t}"
    );
    assert!(!t.contains("OA : OX = OP : OX"), "{t}");
    assert!(!t.contains("2\u{2221}"), "no doubled directed angles:\n{t}");
    assert!(
        t.contains("We show that A, I, X are collinear:") && t.contains("[angle addition]"),
        "{t}"
    );
    assert!(!hp.blocks.last().unwrap().engine_facts.is_empty());
}

#[test]
fn every_claim_and_link_is_reverified() {
    for c in cases() {
        let s = solve(&c);
        let hp = write(&s, &strict());
        let closure = s.trace.closure(&s.deps);
        for b in &hp.blocks {
            if !matches!(b.kind, BlockKind::Raw | BlockKind::Conclusion) {
                assert!(
                    !b.engine_facts.is_empty(),
                    "{}: block {} re-presents no engine fact",
                    c.name,
                    b.id
                );
            }
            for f in &b.engine_facts {
                assert!(
                    closure.contains(f),
                    "{}: block {} cites fact {f} outside the proof",
                    c.name,
                    b.id
                );
            }
        }
        assert_eq!(
            hp.blocks.last().unwrap().kind,
            BlockKind::Conclusion,
            "{}",
            c.name
        );
        assert!(verify(&s, &hp).is_empty(), "{}", c.name);
    }
}

fn first_chain_link(hp: &mut HumanProof, skip_conclusion: bool) -> Option<&mut ddar::human::Link> {
    for b in hp.blocks.iter_mut() {
        if skip_conclusion && b.kind == BlockKind::Conclusion {
            continue;
        }
        for s in b.body.iter_mut() {
            if let Sentence::Chain { links, .. } = s {
                if let Some(l) = links.iter_mut().find(|l| !l.combination.is_empty()) {
                    return Some(l);
                }
            }
        }
    }
    None
}

#[test]
fn checker_rejects_corrupted_proofs() {
    let c = geo_case("euler_line", "examples/euler_line.geo");
    let s = solve(&c);
    let base = write(&s, &strict());
    assert!(verify(&s, &base).is_empty());

    let mut m = base.clone();
    let l = first_chain_link(&mut m, false).expect("a chain link");
    l.combination[0].coef = &l.combination[0].coef * &Rat::from_int(2);
    assert!(!verify(&s, &m).is_empty(), "doubled coefficient accepted");

    let mut m = base.clone();
    let l = first_chain_link(&mut m, false).unwrap();
    l.combination.clear();
    l.reasons.clear();
    assert!(!verify(&s, &m).is_empty(), "link without reasons accepted");

    let mut m = base.clone();
    'outer: for b in m.blocks.iter_mut() {
        for s in b.body.iter_mut() {
            if let Sentence::Chain { terms, .. } = s {
                terms[1] = Expr::Const {
                    degrees: Rat::from_int(37),
                };
                break 'outer;
            }
        }
    }
    assert!(!verify(&s, &m).is_empty(), "wrong chain term accepted");

    let mut m = base.clone();
    let l = first_chain_link(&mut m, false).unwrap();
    for r in l.reasons.iter_mut() {
        if let Reason::Atom { args, .. } = r {
            args.reverse();
        }
        if let Reason::Hyp { stmt, .. } | Reason::Lemma { stmt, .. } = r {
            *stmt = Stmt::Coll { pts: vec![0, 1, 2] };
        }
    }
    assert!(!verify(&s, &m).is_empty(), "altered reasons accepted");

    let mut m = base.clone();
    let tagged = m.blocks.iter().position(|b| b.tag).expect("a cited step");
    m.blocks[tagged].tag = false;
    assert!(
        !verify(&s, &m).is_empty(),
        "a cited step without its relation tag accepted"
    );

    let mut m = base.clone();
    m.blocks[0].step = 7;
    assert!(!verify(&s, &m).is_empty(), "misnumbered step accepted");

    let mut m = base.clone();
    m.goal = Some(ddar::human::GoalWords::Concyclic {
        pts: vec![0, 1, 2, 3],
    });
    assert!(
        !verify(&s, &m).is_empty(),
        "a conclusion worded as another goal accepted"
    );

    let mut m = base.clone();
    let last = m.blocks.last_mut().unwrap();
    last.stmt = Stmt::Coll { pts: vec![0, 1, 2] };
    assert!(!verify(&s, &m).is_empty(), "wrong conclusion accepted");

    let mut m = base.clone();
    m.blocks.swap(0, 1);
    assert!(!verify(&s, &m).is_empty(), "reordered blocks accepted");

    let mut m = base.clone();
    m.blocks.retain(|b| b.kind != BlockKind::Conclusion);
    assert!(
        !verify(&s, &m).is_empty(),
        "proof without a conclusion accepted"
    );
}

fn identity_link(hp: &mut HumanProof) -> Option<(&mut Vec<Expr>, usize)> {
    for b in hp.blocks.iter_mut() {
        for s in b.body.iter_mut() {
            if let Sentence::Chain { terms, links, .. } = s {
                if let Some(k) = links.iter().position(|l| l.combination.is_empty() && matches!(l.reasons.as_slice(), [Reason::Atom { key, .. }] if key.is_identity_label())) {
                    return Some((terms, k));
                }
            }
        }
    }
    None
}

#[test]
fn identity_links_are_checked_and_honestly_labelled() {
    let c = geo_case("imo_2023_p2", "tests/golden/human/imo_2023_p2.geo");
    let s = solve(&c);
    let base = write(&s, &strict());
    assert!(verify(&s, &base).is_empty());
    let mut probe = base.clone();
    assert!(
        identity_link(&mut probe).is_some(),
        "the golden proof exposes at least one angle addition"
    );

    let mut m = base.clone();
    for b in m.blocks.iter_mut() {
        for st in b.body.iter_mut() {
            if let Sentence::Chain { links, .. } = st {
                for l in links.iter_mut() {
                    if let [Reason::Atom { key, args, .. }] = l.reasons.as_mut_slice() {
                        if key.is_identity_label() && l.combination.is_empty() {
                            *key = if *key == human::AtomKey::AngleSum {
                                human::AtomKey::ExteriorAngle
                            } else {
                                human::AtomKey::AngleSum
                            };
                            *args = vec![0, 1, 2];
                        }
                    }
                }
            }
        }
    }
    assert!(
        !verify(&s, &m).is_empty(),
        "a mislabelled angle addition accepted"
    );

    let mut m = base.clone();
    let (terms, k) = identity_link(&mut m).unwrap();
    terms[k + 1] = Expr::Lin {
        terms: vec![
            (Rat::one(), terms[k + 1].clone()),
            (
                Rat::one(),
                Expr::Const {
                    degrees: Rat::from_int(30),
                },
            ),
        ],
    };
    assert!(
        !verify(&s, &m).is_empty(),
        "an angle addition that is not an identity accepted"
    );

    let mut m = base.clone();
    let l = first_chain_link(&mut m, false).unwrap();
    l.combination.clear();
    l.reasons = vec![Reason::Atom {
        key: human::AtomKey::AngleAddition,
        stmt: Stmt::Formula {
            text: String::new(),
            pts: vec![],
        },
        args: vec![],
        from: vec![],
    }];
    assert!(
        !verify(&s, &m).is_empty(),
        "a fact link relabelled as angle addition accepted"
    );
}

fn converse(hp: &mut HumanProof) -> Option<(&mut human::TheoremKey, &mut Stmt, &mut Vec<Reason>)> {
    for b in hp.blocks.iter_mut() {
        for s in b.body.iter_mut() {
            if let Sentence::Theorem { key, stmt, reasons } = s {
                if key.is_converse() && reasons.len() == 2 {
                    return Some((key, stmt, reasons));
                }
            }
        }
    }
    None
}

#[test]
fn converse_steps_and_cyclic_labels_are_checked() {
    let c = geo_case("imo_2023_p2", "tests/golden/human/imo_2023_p2.geo");
    let s = solve(&c);
    let base = write(&s, &strict());
    assert!(verify(&s, &base).is_empty());
    let mut probe = base.clone();
    assert!(
        converse(&mut probe).is_some(),
        "the golden proof names a converse of the inscribed angle theorem"
    );

    let mut m = base.clone();
    let (_, _, reasons) = converse(&mut m).unwrap();
    if let Reason::Atom { key, .. } = &mut reasons[1] {
        *key = if *key == human::AtomKey::SameSide {
            human::AtomKey::OppositeSides
        } else {
            human::AtomKey::SameSide
        };
    }
    assert!(
        !verify(&s, &m).is_empty(),
        "a false same-side condition accepted"
    );

    let mut m = base.clone();
    let (_, _, reasons) = converse(&mut m).unwrap();
    reasons.truncate(1);
    assert!(
        !verify(&s, &m).is_empty(),
        "a figure-angle converse without its side condition accepted"
    );

    let mut m = base.clone();
    let (_, stmt, _) = converse(&mut m).unwrap();
    if let Stmt::Cyclic { pts } = stmt {
        pts[0] = (0..20).find(|p| !pts.contains(p)).unwrap();
    }
    assert!(
        !verify(&s, &m).is_empty(),
        "a converse concluding other points accepted"
    );

    let mut m = base.clone();
    let (key, _, _) = converse(&mut m).unwrap();
    *key = human::TheoremKey::OppositeAnglesConverse;
    assert!(
        !verify(&s, &m).is_empty(),
        "an equal-angle converse relabelled as opposite angles accepted"
    );

    let mut m = base.clone();
    let mut swapped = false;
    for b in m.blocks.iter_mut() {
        for st in b.body.iter_mut() {
            if let Sentence::Chain { links, .. } = st {
                for l in links.iter_mut() {
                    for r in l.reasons.iter_mut() {
                        if let Reason::Atom { key, .. } = r {
                            if key.is_cyclic_quad() && !swapped {
                                *key = if *key == human::AtomKey::CyclicExterior {
                                    human::AtomKey::CyclicOpposite
                                } else {
                                    human::AtomKey::CyclicExterior
                                };
                                swapped = true;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(swapped, "the golden proof labels a cyclic quadrilateral");
    assert!(
        !verify(&s, &m).is_empty(),
        "an exterior angle relabelled as opposite angles accepted"
    );
}

#[test]
fn a_circle_merge_needs_three_shared_points() {
    let a = Stmt::Cyclic {
        pts: vec![0, 1, 2, 3],
    };
    let b = Stmt::Cyclic {
        pts: vec![0, 1, 2, 4],
    };
    let far = Stmt::Cyclic {
        pts: vec![0, 1, 4, 5],
    };
    let lemma = |st: &Stmt| Reason::Lemma {
        stmt: st.clone(),
        block: 1,
        sentence: 0,
    };
    assert!(human::check::merge_ok(
        &Stmt::Cyclic {
            pts: vec![0, 1, 2, 3, 4]
        },
        &[lemma(&a), lemma(&b)]
    )
    .is_ok());
    assert!(
        human::check::merge_ok(
            &Stmt::Cyclic {
                pts: vec![0, 1, 2, 3, 5]
            },
            &[lemma(&a), lemma(&far)]
        )
        .is_err(),
        "two circles sharing two points merged"
    );
    assert!(
        human::check::merge_ok(
            &Stmt::Cyclic {
                pts: vec![0, 1, 2, 3, 4, 5]
            },
            &[lemma(&a), lemma(&b)]
        )
        .is_err(),
        "a merge naming a point no circle contains"
    );
}

#[test]
fn a_setup_line_never_states_the_goal() {
    let c = geo_case(
        "orthocenter_reflection",
        "examples/orthocenter_reflection.geo",
    );
    let s = solve(&c);
    let base = write(&s, &strict());
    assert!(verify(&s, &base).is_empty());
    let ddar::predicate::Predicate { points, .. } = s.goal.clone();
    let mut m = base.clone();
    m.setup.push(human::SetupLine::Circle {
        name: "\u{3b3}".into(),
        through: points.clone(),
        centre: None,
        diameter: None,
    });
    assert!(
        !verify(&s, &m).is_empty(),
        "a setup circle through the goal's four points accepted:\n{}",
        human::render_en(&s.trace, &m, &s.aux)
    );
}

#[test]
fn a_corrupted_block_falls_back_to_the_raw_steps() {
    let c = geo_case("euler_line", "examples/euler_line.geo");
    let s = solve(&c);
    let mut hp = write(&s, &strict());
    let l = first_chain_link(&mut hp, true).expect("a non-final chain");
    l.combination[0].coef = &l.combination[0].coef * &Rat::from_int(3);
    let found = human::repair(&s.trace, &s.goal, &s.deps, &s.aux, &mut hp);
    assert!(found > 0);
    assert!(
        hp.available,
        "a non-final corruption must not lose the proof"
    );
    assert!(
        hp.blocks.iter().any(|b| b.kind == BlockKind::Raw),
        "the bad block is shown as raw steps"
    );
    assert!(hp
        .blocks
        .iter()
        .filter(|b| b.kind == BlockKind::Raw)
        .all(|b| b.body.iter().all(|x| matches!(x, Sentence::Raw { .. }))));
    assert!(verify(&s, &hp).is_empty());

    let mut hp = write(&s, &strict());
    let last = hp.blocks.last_mut().unwrap();
    last.stmt = Stmt::Coll { pts: vec![0, 1, 2] };
    human::repair(&s.trace, &s.goal, &s.deps, &s.aux, &mut hp);
    assert!(
        !hp.available,
        "a bad conclusion makes the human proof unavailable, never wrong"
    );
    assert!(human::render_en(&s.trace, &hp, &s.aux).contains("no human proof"));
}

#[test]
fn output_is_deterministic() {
    for c in cases() {
        let s = solve(&c);
        let closure = s.trace.closure(&s.deps);
        let a = human::view::engine_json(&s.trace, &write(&s, &strict()), &closure).to_string();
        let again = solve(&c);
        let b =
            human::view::engine_json(&again.trace, &write(&again, &strict()), &closure).to_string();
        assert_eq!(a, b, "{}: JSON differs between runs", c.name);
        let par: Vec<String> = std::thread::scope(|sc| {
            let hs: Vec<_> = (0..3)
                .map(|_| {
                    sc.spawn(|| {
                        human::view::engine_json(&s.trace, &write(&s, &strict()), &closure)
                            .to_string()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(
            par.iter().all(|x| *x == a),
            "{}: JSON differs across threads",
            c.name
        );
    }
}

#[test]
fn an_expired_deadline_degrades_but_never_lies() {
    for c in cases() {
        let s = solve(&c);
        let start = Instant::now();
        let hp = write(
            &s,
            &Opts {
                deadline: Some(Instant::now()),
                strict: false,
            },
        );
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{}: writer ignored its deadline",
            c.name
        );
        if hp.available {
            assert!(
                verify(&s, &hp).is_empty(),
                "{}: timed-out output fails the checker",
                c.name
            );
        }
        let start = Instant::now();
        let hp = write(
            &s,
            &Opts {
                deadline: Some(Instant::now() + Duration::from_millis(1)),
                strict: false,
            },
        );
        assert!(start.elapsed() < Duration::from_secs(5), "{}", c.name);
        if hp.available {
            assert!(verify(&s, &hp).is_empty(), "{}", c.name);
        }
    }
}

#[test]
fn the_jgex_corpus_never_shows_a_failed_check() {
    let text = std::fs::read_to_string(root().join("../corpus/jgex_ag_231.txt")).expect("corpus");
    let (mut proved, mut available, mut raw_blocks) = (0usize, 0usize, 0usize);
    for (name, body) in ddar::corpus::read_corpus(&text).expect("corpus parses") {
        let Ok(tr) = ddar::corpus::translate_text(&name, &body, 1) else {
            continue;
        };
        let problem = tr.problem;
        let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(&problem) else {
            continue;
        };
        let goal = problem.goal.clone().unwrap();
        proved += 1;
        let hp = human::write(&trace, &goal, &deps, &[], &Opts::default());
        if !hp.available {
            continue;
        }
        available += 1;
        let v = human::verify(&trace, &goal, &deps, &[], &hp);
        assert!(v.is_empty(), "{name}: {v:?}");
        let claims = hp
            .blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Claim(_)))
            .count();
        assert!(claims <= 7, "{name}: {claims} claims");
        raw_blocks += hp
            .blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Raw)
            .count();
    }
    assert!(proved >= 200, "only {proved} JGEX problems proved directly");
    assert!(
        available * 100 >= proved * 97,
        "human proofs for {available} of {proved}"
    );
    eprintln!("jgex direct: proved {proved}, human {available}, raw blocks {raw_blocks}");
}

#[test]
fn checker_rejects_false_atom_instances() {
    use ddar::human::AtomKey;
    let c = imo_2004_p1();
    let s = solve(&c);
    let base = write(&s, &strict());
    let id = |n: &str| {
        c.problem.points.iter().position(|p| p.name == n).unwrap() as ddar::predicate::PointId
    };
    let (a, b, cc, o, m, n, r) = (
        id("a"),
        id("b"),
        id("c"),
        id("o"),
        id("m"),
        id("n"),
        id("r"),
    );
    let with = |key: AtomKey, args: Vec<ddar::predicate::PointId>| -> HumanProof {
        let mut hp = base.clone();
        let l = first_chain_link(&mut hp, false).expect("a chain link");
        l.reasons.push(Reason::Atom {
            key,
            stmt: Stmt::Coll {
                pts: vec![a, b, cc],
            },
            args,
            from: Vec::new(),
        });
        hp
    };
    let controls = [
        (AtomKey::Thales, vec![b, cc, n, o]),
        (AtomKey::PowerOfPoint, vec![a, b, m, cc, n]),
        (AtomKey::CentralAngle, vec![o, n, cc, m, m]),
        (AtomKey::CentralAngle, vec![o, n, cc, b]),
    ];
    for (key, args) in controls {
        assert!(
            verify(&s, &with(key, args.clone())).is_empty(),
            "true {key:?} {args:?} rejected"
        );
    }
    let false_instances = [
        (AtomKey::Thales, vec![b, cc, n, r]),
        (AtomKey::TangentChord, vec![b, a, cc, n, o]),
        (AtomKey::PowerOfPoint, vec![a, b, m, cc, r]),
        (AtomKey::Midline, vec![o, m, b, a, cc]),
        (AtomKey::PerpBisector, vec![o, r, b, cc]),
        (AtomKey::Radii, vec![r, b, cc]),
        (AtomKey::CentralAngle, vec![o, n, cc, b, b]),
        (AtomKey::CentralAngle, vec![o, n, cc, m, b]),
        (AtomKey::CentralAngle, vec![r, n, cc, m, m]),
    ];
    for (key, args) in false_instances {
        assert!(
            !verify(&s, &with(key, args.clone())).is_empty(),
            "false {key:?} {args:?} accepted"
        );
    }
}

fn within_open_segment(
    trace: &EngineTrace,
    x: ddar::predicate::PointId,
    b: ddar::predicate::PointId,
    c: ddar::predicate::PointId,
) -> bool {
    let (px, pb, pc) = (trace.coord(x), trace.coord(b), trace.coord(c));
    let len2 = (pc.0 - pb.0).powi(2) + (pc.1 - pb.1).powi(2);
    let along = ((px.0 - pb.0) * (pc.0 - pb.0) + (px.1 - pb.1) * (pc.1 - pb.1)) / len2;
    along > 1e-9 && along < 1.0 - 1e-9
}

fn collinear(
    trace: &EngineTrace,
    a: ddar::predicate::PointId,
    b: ddar::predicate::PointId,
    c: ddar::predicate::PointId,
) -> bool {
    let (pa, pb, pc) = (trace.coord(a), trace.coord(b), trace.coord(c));
    let cross = (pb.0 - pa.0) * (pc.1 - pa.1) - (pb.1 - pa.1) * (pc.0 - pa.0);
    let scale = [(pa, pb), (pb, pc), (pa, pc)]
        .iter()
        .map(|(p, q)| (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2))
        .fold(0.0f64, f64::max);
    cross.abs() <= 1e-7 * scale
}

#[test]
fn theorem_names_and_triangles_in_the_jgex_proofs_match_the_figure() {
    use ddar::human::{AtomKey, TheoremKey};
    let text = std::fs::read_to_string(root().join("../corpus/jgex_ag_231.txt")).expect("corpus");
    let (mut internal, mut external, mut midlines) = (0usize, 0usize, 0usize);
    for (name, body) in ddar::corpus::read_corpus(&text).expect("corpus parses") {
        let Ok(tr) = ddar::corpus::translate_text(&name, &body, 1) else {
            continue;
        };
        let problem = tr.problem;
        let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(&problem) else {
            continue;
        };
        let goal = problem.goal.clone().unwrap();
        let hp = human::write(&trace, &goal, &deps, &[], &Opts::default());
        for block in &hp.blocks {
            for sentence in &block.body {
                if let Sentence::Theorem {
                    key,
                    stmt: Stmt::EqRatio { segs },
                    ..
                } = sentence
                {
                    if matches!(
                        key,
                        TheoremKey::AngleBisectorThm | TheoremKey::ExtAngleBisectorThm
                    ) {
                        let [(x, b), (x2, c), ..] = segs[..] else {
                            continue;
                        };
                        assert_eq!(x, x2, "{name}");
                        let between = within_open_segment(&trace, x, b, c);
                        assert_eq!(*key == TheoremKey::AngleBisectorThm, between, "{name}");
                        if between {
                            internal += 1;
                        } else {
                            external += 1;
                        }
                    }
                }
                human::barem::visit(sentence, &mut |r| {
                    if let Reason::Atom {
                        key: AtomKey::Midline,
                        args,
                        ..
                    } = r
                    {
                        midlines += 1;
                        assert!(
                            !collinear(&trace, args[2], args[3], args[4]),
                            "{name}: a midline of a degenerate triangle"
                        );
                    }
                });
            }
        }
    }
    assert!(
        internal >= 1 && external >= 1 && midlines >= 1,
        "{internal} {external} {midlines}"
    );
}

fn restating_blocks(hp: &HumanProof) -> Vec<u16> {
    let sorted = |v: &[ddar::predicate::PointId]| {
        let mut v = v.to_vec();
        v.sort_unstable();
        v.dedup();
        v
    };
    let mut out = Vec::new();
    for b in &hp.blocks {
        let cites: Vec<u16> = human::barem::cited_blocks(b).into_iter().collect();
        let [k] = cites[..] else { continue };
        let Some(prev) = hp.blocks.iter().find(|x| x.id == k) else {
            continue;
        };
        let same = match (&prev.stmt, &b.stmt) {
            (Stmt::Coll { pts: x }, Stmt::Coll { pts: y })
            | (Stmt::Cyclic { pts: x }, Stmt::Cyclic { pts: y }) => sorted(x) == sorted(y),
            _ => false,
        };
        if same {
            out.push(b.id);
        }
    }
    out
}

#[test]
fn no_proof_shows_a_raw_predicate_name_or_a_block_that_restates_its_premise() {
    let mut checked = 0usize;
    let mut remaining: Vec<String> = Vec::new();
    let mut sources: Vec<(String, Problem)> = Vec::new();
    let named = root().join("examples/named");
    for entry in std::fs::read_dir(&named).expect("named examples") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "geo") {
            let src = std::fs::read_to_string(&path).unwrap();
            if let Ok(c) = compile(&src) {
                sources.push((path.display().to_string(), c.problem));
            }
        }
    }
    let text = std::fs::read_to_string(root().join("../corpus/jgex_ag_231.txt")).expect("corpus");
    for (name, body) in ddar::corpus::read_corpus(&text).expect("corpus parses") {
        if let Ok(tr) = ddar::corpus::translate_text(&name, &body, 1) {
            sources.push((name, tr.problem));
        }
    }
    for (name, problem) in sources {
        let Some(goal) = problem.goal.clone() else {
            continue;
        };
        let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(&problem) else {
            continue;
        };
        let hp = human::write(&trace, &goal, &deps, &[], &Opts::default());
        if !hp.available {
            continue;
        }
        checked += 1;
        let rendered = human::render_en(&trace, &hp, &[]);
        assert!(!rendered.contains("distmeq"), "{name}: {rendered}");
        let restating = restating_blocks(&hp);
        if !restating.is_empty() {
            eprintln!("restating: {name} {restating:?}");
            remaining.push(name);
        }
    }
    assert!(checked >= 200, "only {checked} proofs checked");
    assert!(
        remaining
            .iter()
            .all(|n| !n.contains("examples/named/monge")),
        "{remaining:?}"
    );
    assert!(remaining.len() <= 2, "{remaining:?}");
}
