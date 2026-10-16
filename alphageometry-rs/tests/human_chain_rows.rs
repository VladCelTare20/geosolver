use ddar::geo::compile;
use ddar::human::{
    self, AuxInfo, BlockKind, EngineTrace, Expr, HumanProof, Link, Opts, Reason, Sentence, Term,
};
use ddar::predicate::{Predicate, Problem};
use ddar::proof::FactId;
use ddar::rational::Rat;
use std::path::PathBuf;

const MUST_LINTS: [&str; 5] = ["L1", "L2", "L3", "L5", "L7"];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Solved {
    hp: HumanProof,
    trace: EngineTrace,
    goal: Predicate,
    deps: Vec<FactId>,
    aux: Vec<AuxInfo>,
}

fn simson() -> Solved {
    let src = std::fs::read_to_string(root().join("tests/golden/human/simson.geo")).expect("geo");
    let problem = compile(&src).expect("compiles").problem;
    let (hp, trace, deps, _) =
        human::for_problem(&problem, &[], &Opts::default()).expect("Simson is proved");
    Solved {
        hp,
        trace,
        goal: problem.goal.clone().expect("goal"),
        deps,
        aux: Vec::new(),
    }
}

fn lint_rules(s: &Solved, hp: &HumanProof) -> Vec<&'static str> {
    human::lints(&s.trace, &s.goal, &s.deps, &s.aux, hp)
        .into_iter()
        .map(|v| v.rule)
        .collect()
}

fn angle(a: u16, b: u16, c: u16, directed: bool) -> Expr {
    Expr::Angle {
        a: a as _,
        b: b as _,
        c: c as _,
        directed,
    }
}

fn lin(parts: Vec<(i64, Expr)>) -> Expr {
    Expr::Lin {
        terms: parts
            .into_iter()
            .map(|(k, e)| (Rat::from_int(k), e))
            .collect(),
    }
}

fn deg(d: i64) -> Expr {
    Expr::Const {
        degrees: Rat::from_int(d),
    }
}

fn fact_link(reasons: &[FactId], used: &[usize]) -> Link {
    Link {
        reasons: reasons
            .iter()
            .map(|&f| Reason::Engine { fact: f })
            .collect(),
        combination: used
            .iter()
            .map(|&r| Term {
                reason: r as u16,
                row: 0,
                coef: Rat::one(),
            })
            .collect(),
    }
}

fn with_chain(s: &Solved, terms: Vec<Expr>, links: Vec<Link>, directed: bool) -> HumanProof {
    let mut hp = s.hp.clone();
    let b = hp
        .blocks
        .iter_mut()
        .find(|b| b.kind != BlockKind::Raw)
        .expect("a written block");
    b.body.push(Sentence::Chain {
        terms,
        links,
        then: None,
        directed,
    });
    hp
}

#[test]
fn every_chain_lint_fires_on_its_own_bad_row() {
    let s = simson();
    let base = lint_rules(&s, &s.hp);
    for rule in MUST_LINTS {
        assert!(
            !base.contains(&rule),
            "the Simson proof itself has {rule}: {base:?}"
        );
    }
    let one = || fact_link(&[0], &[0]);
    let cases: Vec<(&str, Vec<Expr>, Vec<Link>, bool)> = vec![
        (
            "L1",
            vec![
                angle(0, 1, 2, false),
                lin(vec![
                    (1, angle(0, 1, 2, false)),
                    (1, angle(1, 2, 3, false)),
                    (-1, angle(2, 3, 4, false)),
                ]),
            ],
            vec![one()],
            false,
        ),
        (
            "L2",
            vec![angle(0, 1, 2, false), angle(0, 1, 2, false)],
            vec![one()],
            false,
        ),
        (
            "L3",
            vec![
                angle(0, 1, 2, true),
                lin(vec![
                    (1, angle(1, 2, 3, true)),
                    (1, angle(2, 3, 4, true)),
                    (1, deg(90)),
                ]),
                lin(vec![
                    (1, angle(0, 3, 4, true)),
                    (1, angle(1, 2, 4, true)),
                    (1, deg(90)),
                ]),
                lin(vec![
                    (1, angle(0, 1, 3, true)),
                    (1, angle(0, 2, 4, true)),
                    (1, deg(90)),
                ]),
                angle(0, 1, 4, true),
            ],
            vec![one(), one(), one(), one()],
            true,
        ),
        (
            "L4",
            vec![
                angle(0, 1, 2, false),
                lin(vec![
                    (1, angle(3, 4, 5, false)),
                    (1, angle(1, 2, 3, false)),
                    (-1, angle(0, 2, 4, false)),
                ]),
            ],
            vec![one()],
            false,
        ),
        (
            "L5",
            vec![
                angle(0, 1, 2, false),
                lin(vec![(1, angle(1, 2, 3, false)), (1, angle(2, 3, 4, false))]),
            ],
            vec![Link {
                reasons: Vec::new(),
                combination: Vec::new(),
            }],
            false,
        ),
        (
            "L6",
            vec![angle(0, 1, 2, false), angle(1, 2, 3, false)],
            vec![fact_link(&[0, 1], &[0, 1])],
            false,
        ),
        (
            "L7",
            vec![
                lin(vec![
                    (1, angle(0, 1, 2, false)),
                    (-1, angle(1, 2, 3, false)),
                ]),
                deg(0),
            ],
            vec![one()],
            false,
        ),
        (
            "L8",
            vec![angle(0, 1, 2, false), angle(1, 2, 3, false)],
            vec![fact_link(&[0, 1], &[0])],
            false,
        ),
    ];
    for (rule, terms, links, directed) in cases {
        let hp = with_chain(&s, terms, links, directed);
        let got = lint_rules(&s, &hp);
        assert!(
            got.contains(&rule),
            "{rule} not reported for its bad row: {got:?}"
        );
    }
    let rules: Vec<&str> = human::check::LINT_RULES.iter().map(|(r, _)| *r).collect();
    assert_eq!(rules, ["L1", "L2", "L3", "L4", "L5", "L6", "L7", "L8"]);
}

#[test]
fn a_directed_congruence_to_zero_is_not_an_angle_sum_equated_to_zero() {
    let s = simson();
    let hp = with_chain(
        &s,
        vec![
            lin(vec![(1, angle(0, 1, 2, true)), (1, angle(1, 2, 3, true))]),
            deg(0),
        ],
        vec![fact_link(&[0], &[0])],
        true,
    );
    assert!(!lint_rules(&s, &hp).contains(&"L7"));
}

#[test]
fn a_constant_the_chain_ends_with_is_not_carried() {
    let s = simson();
    let hp = with_chain(
        &s,
        vec![
            angle(0, 1, 2, true),
            lin(vec![
                (1, angle(1, 2, 3, true)),
                (1, angle(2, 3, 4, true)),
                (1, deg(90)),
            ]),
            lin(vec![
                (1, angle(0, 3, 4, true)),
                (1, angle(1, 2, 4, true)),
                (1, deg(90)),
            ]),
            lin(vec![
                (1, angle(0, 1, 3, true)),
                (1, angle(0, 2, 4, true)),
                (1, deg(90)),
            ]),
            lin(vec![(1, angle(0, 1, 4, true)), (1, deg(90))]),
        ],
        vec![fact_link(&[0], &[0]); 4],
        true,
    );
    assert!(!lint_rules(&s, &hp).contains(&"L3"));
}

#[test]
fn simson_with_p_off_the_circumcircle_stays_unproved() {
    for p_line in ["P = free", "D = free\nP = on_circum(A, B, D)"] {
        let src = format!(
            "A B C = triangle\n{p_line}\nX = foot(P, line(B, C))\nY = foot(P, line(C, A))\nZ = foot(P, line(A, B))\nprove coll(X, Y, Z)\n"
        );
        let problem = compile(&src).expect("compiles").problem;
        assert!(
            human::for_problem(&problem, &[], &Opts::default()).is_none(),
            "a false Simson statement was proved:\n{src}"
        );
    }
}

fn same_angle(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (
            Expr::Angle {
                a: a1,
                b: b1,
                c: c1,
                directed: d1,
            },
            Expr::Angle {
                a: a2,
                b: b2,
                c: c2,
                directed: d2,
            },
        ) => d1 == d2 && b1 == b2 && ((a1, c1) == (a2, c2) || (!*d1 && (a1, c1) == (c2, a2))),
        _ => a == b,
    }
}

fn reopens_its_start(terms: &[Expr]) -> bool {
    let [start, next, ..] = terms else {
        return false;
    };
    match next {
        Expr::Lin { terms: parts } => parts.iter().any(|(_, e)| same_angle(e, start)),
        _ => false,
    }
}

#[test]
fn corpus_proofs_keep_every_chain_row_clean() {
    let text = std::fs::read_to_string(root().join("tests/golden/human/corpus_solved.tsv"))
        .expect("solved corpus fixture");
    let (mut proofs, mut problems) = (0usize, std::collections::BTreeSet::new());
    let mut bad: Vec<String> = Vec::new();
    for line in text.lines().filter(|l| !l.is_empty()) {
        let cols: Vec<&str> = line.splitn(5, '\t').collect();
        let [name, k, first, aux, low] = cols[..] else {
            panic!("malformed fixture line");
        };
        let p = Problem::parse(low).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let aux: Vec<String> = aux
            .split('\u{1f}')
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let infos = human::aux_infos(&p, first.parse().unwrap(), &aux);
        let Some((hp, trace, deps, _)) = human::for_problem(&p, &infos, &Opts::default()) else {
            bad.push(format!("{name} {k}: not proved from its solved fixture"));
            continue;
        };
        problems.insert(name.to_string());
        if !hp.available {
            continue;
        }
        proofs += 1;
        let goal = p.goal.clone().unwrap();
        let v = human::verify(&trace, &goal, &deps, &infos, &hp);
        if !v.is_empty() {
            bad.push(format!("{name} {k}: checker {v:?}"));
        }
        for l in human::lints(&trace, &goal, &deps, &infos, &hp) {
            if MUST_LINTS.contains(&l.rule) {
                bad.push(format!(
                    "{name} {k}: {} block {} {}",
                    l.rule, l.block, l.detail
                ));
            }
        }
        for b in &hp.blocks {
            for s in &b.body {
                if let Sentence::Chain { terms, .. } = s {
                    if reopens_its_start(terms) {
                        bad.push(format!("{name} {k}: block {} opens on its own start", b.id));
                    }
                }
            }
        }
        let shown = human::render_en(&trace, &hp, &infos);
        for row in shown.lines() {
            let head = row.split('[').next().unwrap_or("");
            if head.contains('\u{2221}') && head.trim_start().starts_with('=') {
                bad.push(format!("{name} {k}: directed row with '=': {row}"));
            }
        }
    }
    assert_eq!(
        problems.len(),
        261,
        "the fixture covers the 30 IMO and 231 JGEX problems"
    );
    assert!(proofs >= 260, "only {proofs} human proofs");
    assert!(
        bad.is_empty(),
        "{} chain problems:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
