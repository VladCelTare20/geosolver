use ddar::geo::compile;
use ddar::human::atoms::{build_all, AtomSrc};
use ddar::human::check::Checker;
use ddar::human::ctx::Ctx;
use ddar::human::theorems::{config_holds, Hyp};
use ddar::human::{self, AtomKey, EngineTrace, Opts, Reason, Sentence};
use ddar::predicate::{PointId, Predicate};
use ddar::proof::FactId;
use std::time::{Duration, Instant};

struct Fix {
    trace: EngineTrace,
    goal: Predicate,
    deps: Vec<FactId>,
}

fn fixture(src: &str) -> Fix {
    let c = compile(src).expect("compiles");
    let (_, trace, deps) = ddar::runner::solve_problem_with_trace(&c.problem).expect("solves").expect("proved directly");
    Fix { trace, goal: c.problem.goal.clone().expect("goal"), deps }
}

impl Fix {
    fn id(&self, n: &str) -> PointId {
        self.trace.names.iter().position(|x| x.eq_ignore_ascii_case(n)).unwrap_or_else(|| panic!("no point {n} in {:?}", self.trace.names)) as PointId
    }
    fn ids(&self, ns: &[&str]) -> Vec<PointId> {
        ns.iter().map(|n| self.id(n)).collect()
    }
    fn ctx(&self) -> Ctx<'_> {
        let elsewhere = Predicate { name: "elsewhere".into(), points: Vec::new(), constants: Vec::new() };
        Ctx::new(&self.trace, &elsewhere, &self.deps, &[], None)
    }
    fn generated(&self, key: AtomKey, args: &[PointId]) -> bool {
        let cx = self.ctx();
        build_all(&cx).iter().any(|a| a.src == AtomSrc::Human(key) && a.args == args)
    }
    fn accepted(&self, key: AtomKey, args: &[PointId]) -> bool {
        let cx = self.ctx();
        Checker::new(&cx).template(self.trace.facts.len() as FactId, key, args)
    }
    fn names(&self, v: &[PointId]) -> Vec<String> {
        v.iter().map(|&p| self.trace.name(p).to_string()).collect()
    }
}

struct Case {
    name: &'static str,
    src: &'static str,
    pos: (AtomKey, &'static [&'static str]),
    neg: &'static [(AtomKey, &'static [&'static str])],
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "equal tangents",
            src: "O A = segment\nP = shift(A, O, A)\nT1 T2 = tangent(P, circle(O, A))\nprove cong(P, T1, P, T2)",
            pos: (AtomKey::EqualTangents, &["P", "T1", "T2", "O"]),
            neg: &[(AtomKey::EqualTangents, &["P", "T1", "A", "O"]), (AtomKey::EqualTangents, &["P", "T1", "T1", "O"]), (AtomKey::EqualTangents, &["A", "T1", "T2", "O"])],
        },
        Case {
            name: "tangent and secant",
            src: "O A = segment\nP = shift(A, O, A)\nT1 T2 = tangent(P, circle(O, A))\nB = reflect(A, O)\nprove cong(P, T1, P, T2)",
            pos: (AtomKey::TangentSecant, &["P", "T1", "A", "B", "O"]),
            neg: &[(AtomKey::TangentSecant, &["P", "A", "T1", "B", "O"]), (AtomKey::TangentSecant, &["P", "T1", "A", "A", "O"])],
        },
        Case {
            name: "power of a point, converse",
            src: "A B C = triangle\nO = circumcenter(A, B, C)\nD = point: on(D, circle(O, A))\nP = meet(line(A, B), line(C, D))\nprove dist(P, A) * dist(P, B) = dist(P, C) * dist(P, D)",
            pos: (AtomKey::PowerConverse, &["P", "A", "B", "C", "D"]),
            neg: &[(AtomKey::PowerConverse, &["P", "A", "B", "C", "O"]), (AtomKey::PowerConverse, &["P", "A", "A", "C", "D"])],
        },
        Case {
            name: "intercept theorem",
            src: "A B C = triangle\nD = on_line(A, B)\nE = meet(para_line(D, line(B, C)), line(A, C))\nprove eqratio(A, D, A, B, A, E, A, C)",
            pos: (AtomKey::Intercept, &["A", "B", "D", "C", "E"]),
            neg: &[(AtomKey::Intercept, &["A", "D", "B", "C", "E"]), (AtomKey::Intercept, &["B", "D", "A", "E", "C"])],
        },
        Case {
            name: "intercept theorem, converse",
            src: "A B C = triangle\nD = midpoint(A, B)\nE = midpoint(A, C)\nF = reflect(E, A)\nprove para(D, E, B, C)",
            pos: (AtomKey::InterceptConverse, &["A", "B", "D", "C", "E"]),
            neg: &[(AtomKey::InterceptConverse, &["A", "D", "B", "F", "C"])],
        },
        Case {
            name: "angle bisector theorem",
            src: "A B C = triangle\nX = meet(bisector(B, A, C), line(B, C))\nM = midpoint(B, C)\nprove eqratio(X, B, X, C, A, B, A, C)",
            pos: (AtomKey::BisectorRatio, &["A", "B", "C", "X"]),
            neg: &[(AtomKey::ExtBisectorRatio, &["A", "B", "C", "X"]), (AtomKey::BisectorRatio, &["A", "B", "C", "M"]), (AtomKey::BisectorConverse, &["A", "B", "C", "M"])],
        },
        Case {
            name: "external angle bisector theorem",
            src: "A B C = triangle\nI = incenter(A, B, C)\nX = meet(perp_line(A, line(A, I)), line(B, C))\nprove eqratio(X, B, X, C, A, B, A, C)",
            pos: (AtomKey::ExtBisectorRatio, &["A", "B", "C", "X"]),
            neg: &[(AtomKey::BisectorRatio, &["A", "B", "C", "X"])],
        },
        Case {
            name: "Menelaus",
            src: "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nF = midpoint(A, B)\nG = meet(line(A, D), line(B, E))\nprove coll(C, G, F)",
            pos: (AtomKey::Menelaus, &["A", "C", "D", "B", "G", "E"]),
            neg: &[(AtomKey::Menelaus, &["A", "C", "D", "B", "G", "C"]), (AtomKey::Menelaus, &["A", "B", "C", "D", "E", "F"]), (AtomKey::MenelausConverse, &["A", "B", "C", "D", "E", "F"])],
        },
        Case {
            name: "Ceva",
            src: "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nG = meet(line(A, D), line(B, E))\nF = meet(line(C, G), line(A, B))\nprove cong(F, A, F, B)",
            pos: (AtomKey::Ceva, &["A", "B", "C", "D", "E", "F", "G"]),
            neg: &[(AtomKey::Ceva, &["A", "B", "C", "D", "E", "C", "G"]), (AtomKey::Menelaus, &["A", "B", "C", "D", "E", "F"])],
        },
        Case {
            name: "angle bisector theorem, converse",
            src: "A B C = triangle\nX = meet(bisector(B, A, C), line(B, C))\nM = midpoint(B, C)\nprove eqratio(X, B, X, C, A, B, A, C)",
            pos: (AtomKey::BisectorConverse, &["A", "B", "C", "X"]),
            neg: &[(AtomKey::ExtBisectorConverse, &["A", "B", "C", "X"]), (AtomKey::BisectorConverse, &["A", "B", "C", "M"])],
        },
        Case {
            name: "external angle bisector theorem, converse",
            src: "A B C = triangle\nI = incenter(A, B, C)\nX = meet(perp_line(A, line(A, I)), line(B, C))\nprove eqratio(X, B, X, C, A, B, A, C)",
            pos: (AtomKey::ExtBisectorConverse, &["A", "B", "C", "X"]),
            neg: &[(AtomKey::BisectorConverse, &["A", "B", "C", "X"])],
        },
        Case {
            name: "Menelaus, converse",
            src: "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nF = midpoint(A, B)\nG = meet(line(A, D), line(B, E))\nprove coll(C, G, F)",
            pos: (AtomKey::MenelausConverse, &["A", "C", "D", "B", "G", "E"]),
            neg: &[(AtomKey::MenelausConverse, &["A", "B", "C", "D", "E", "F"])],
        },
        Case {
            name: "Ceva, converse",
            src: "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nF = midpoint(A, B)\nG = meet(line(A, D), line(B, E))\nprove coll(C, G, F)",
            pos: (AtomKey::CevaConverse, &["A", "B", "C", "D", "E", "F", "G"]),
            neg: &[(AtomKey::CevaConverse, &["A", "B", "C", "D", "E", "F", "A"]), (AtomKey::MenelausConverse, &["A", "B", "C", "D", "E", "F"])],
        },
        Case {
            name: "Pythagoras",
            src: "B C = segment\nA = on_dia(B, C)\nM = midpoint(B, C)\nprove cong(M, A, M, B)",
            pos: (AtomKey::Pythagoras, &["B", "A", "C"]),
            neg: &[(AtomKey::Pythagoras, &["A", "B", "C"]), (AtomKey::Pythagoras, &["B", "M", "C"])],
        },
        Case {
            name: "Pythagoras, converse",
            src: "O A = segment\nP = shift(A, O, A)\nT1 T2 = tangent(P, circle(O, A))\nprove cong(P, T1, P, T2)",
            pos: (AtomKey::PythagorasConverse, &["O", "T1", "P"]),
            neg: &[(AtomKey::PythagorasConverse, &["T1", "O", "P"]), (AtomKey::PythagorasConverse, &["O", "A", "P"])],
        },
        Case {
            name: "law of sines",
            src: "A B C = triangle\nO = circumcenter(A, B, C)\nH = orthocenter(A, B, C)\nMa = midpoint(B, C)\nprove dist(A, H) = 2 * dist(O, Ma)",
            pos: (AtomKey::LawOfSines, &["A", "B", "C"]),
            neg: &[(AtomKey::LawOfSines, &["A", "B", "Ma"])],
        },
        Case {
            name: "extended law of sines",
            src: "A B C = triangle\nO = circumcenter(A, B, C)\nH = orthocenter(A, B, C)\nMa = midpoint(B, C)\nprove dist(A, H) = 2 * dist(O, Ma)",
            pos: (AtomKey::ExtLawOfSines, &["A", "B", "C", "O"]),
            neg: &[(AtomKey::ExtLawOfSines, &["A", "B", "C", "H"]), (AtomKey::ExtLawOfSines, &["A", "B", "Ma", "O"])],
        },
        Case {
            name: "congruence (SSS)",
            src: "A B = segment\nC = free\nD = reflect(C, line(A, B))\nE = free\nprove eqangle(A, C, A, B, A, B, A, D)",
            pos: (AtomKey::CongruentSss, &["A", "C", "B", "A", "D", "B"]),
            neg: &[(AtomKey::CongruentSss, &["A", "C", "B", "A", "E", "B"]), (AtomKey::CongruentSss, &["A", "C", "B", "D", "A", "B"])],
        },
        Case {
            name: "congruence (SAS)",
            src: "A B = segment\nC = free\nD = reflect(C, line(A, B))\nE = free\nprove eqangle(A, C, A, B, A, B, A, D)",
            pos: (AtomKey::CongruentSas, &["B", "A", "C", "B", "A", "D"]),
            neg: &[(AtomKey::CongruentSas, &["B", "A", "C", "B", "A", "E"]), (AtomKey::CongruentSas, &["A", "B", "C", "A", "B", "E"])],
        },
        Case {
            name: "congruence (ASA)",
            src: "A B = segment\nC = free\nD = reflect(C, line(A, B))\nE = free\nprove eqangle(A, C, A, B, A, B, A, D)",
            pos: (AtomKey::CongruentAsa, &["B", "A", "C", "B", "A", "D"]),
            neg: &[(AtomKey::CongruentAsa, &["B", "A", "C", "B", "A", "E"])],
        },
        Case {
            name: "congruence (RHS)",
            src: "O A = segment\nP = shift(A, O, A)\nT1 T2 = tangent(P, circle(O, A))\nprove eqangle(O, P, O, T1, O, T2, O, P)",
            pos: (AtomKey::CongruentRhs, &["O", "T1", "P", "O", "T2", "P"]),
            neg: &[(AtomKey::CongruentRhs, &["O", "A", "P", "O", "T2", "P"]), (AtomKey::CongruentRhs, &["T1", "P", "O", "T2", "P", "O"])],
        },
        Case {
            name: "midline converse",
            src: "A B C = triangle\nM = midpoint(A, B)\nN = meet(para_line(M, line(B, C)), line(A, C))\nK = midpoint(A, M)\nprove cong(N, A, N, C)",
            pos: (AtomKey::MidlineConverse, &["M", "N", "A", "B", "C"]),
            neg: &[(AtomKey::MidlineConverse, &["K", "N", "A", "B", "C"])],
        },
        Case {
            name: "centroid",
            src: "A B C = triangle\nD = midpoint(B, C)\nE = midpoint(C, A)\nG = meet(line(A, D), line(B, E))\nprove dist(A, G) = 2 * dist(G, D)",
            pos: (AtomKey::Centroid, &["G", "A", "B", "C", "D", "E"]),
            neg: &[(AtomKey::Centroid, &["G", "A", "B", "C", "D", "D"]), (AtomKey::Centroid, &["A", "G", "B", "C", "D", "E"])],
        },
        Case {
            name: "median to the hypotenuse",
            src: "B C = segment\nA = on_dia(B, C)\nM = midpoint(B, C)\nN = midpoint(A, B)\nprove cong(M, A, M, B)",
            pos: (AtomKey::MedianHypotenuse, &["M", "B", "C", "A"]),
            neg: &[(AtomKey::MedianHypotenuse, &["N", "B", "C", "A"]), (AtomKey::MedianHypotenuse, &["M", "B", "C", "N"])],
        },
        Case {
            name: "perpendicular bisector",
            src: "A B = segment\nM = midpoint(A, B)\nX = on_tline(M, A, B)\nY = free\nprove cong(X, A, X, B)",
            pos: (AtomKey::PerpBisectorLocus, &["X", "A", "B", "M"]),
            neg: &[(AtomKey::PerpBisectorLocus, &["Y", "A", "B", "M"])],
        },
        Case {
            name: "isosceles converse",
            src: "B C = segment\nA = point: dist(A, B) = dist(A, C)\nD = free\nprove eqangle(B, C, B, A, C, A, C, B)",
            pos: (AtomKey::IsoscelesConverse, &["A", "B", "C"]),
            neg: &[(AtomKey::IsoscelesConverse, &["D", "B", "C"])],
        },
        Case {
            name: "Simson line",
            src: "A B C = triangle\nP = on_circum(A, B, C)\nX = foot(P, line(B, C))\nY = foot(P, line(C, A))\nZ = foot(P, line(A, B))\nQ = free\nprove coll(X, Y, Z)",
            pos: (AtomKey::Simson, &["P", "A", "B", "C", "X", "Y", "Z"]),
            neg: &[(AtomKey::Simson, &["Q", "A", "B", "C", "X", "Y", "Z"]), (AtomKey::Simson, &["P", "A", "B", "C", "X", "Y", "Y"])],
        },
        Case {
            name: "Miquel",
            src: "A B C = triangle\nD = on_line(B, C)\nE = on_line(C, A)\nF = on_line(A, B)\nM = meet(circumcircle(A, E, F), circumcircle(B, F, D))\nprove cyclic(C, D, E, M)",
            pos: (AtomKey::Miquel, &["A", "B", "C", "D", "E", "F", "M"]),
            neg: &[(AtomKey::Miquel, &["A", "B", "C", "D", "E", "F", "C"])],
        },
        Case {
            name: "Reim",
            src: "O1 O2 = segment\nP = free\nQ = meet(circle(O1, P), circle(O2, P))\nA = on_circle(O1, P)\nB = meet(line(A, P), circle(O2, P))\nC = on_circle(O1, P)\nD = meet(line(C, Q), circle(O2, P))\nprove para(A, C, B, D)",
            pos: (AtomKey::Reim, &["P", "Q", "A", "C", "B", "D"]),
            neg: &[(AtomKey::Reim, &["P", "Q", "A", "C", "D", "B"])],
        },
    ]
}

#[test]
#[ignore]
fn list_generated_theorems() {
    for c in cases() {
        let f = fixture(c.src);
        let cx = f.ctx();
        let mut seen: Vec<String> = Vec::new();
        for a in build_all(&cx) {
            if let AtomSrc::Human(k) = a.src {
                if ddar::human::theorems::is_theorem(k) {
                    let s = format!("{k:?} {:?}", f.names(&a.args));
                    if !seen.contains(&s) {
                        seen.push(s);
                    }
                }
            }
        }
        eprintln!("{}: {}", c.name, seen.join("; "));
    }
}

#[test]
fn every_library_theorem_fires_on_its_figure_and_not_without_its_hypotheses() {
    let mut failures = Vec::new();
    for c in cases() {
        let f = fixture(c.src);
        let (key, args) = c.pos;
        let a = f.ids(args);
        if !f.accepted(key, &a) {
            failures.push(format!("{}: checker rejects {key:?} {args:?}", c.name));
        }
        if !f.generated(key, &a) {
            failures.push(format!("{}: writer does not generate {key:?} {args:?}", c.name));
        }
        for (k, n) in c.neg {
            let a = f.ids(n);
            if f.accepted(*k, &a) {
                failures.push(format!("{}: checker accepts false instance {k:?} {n:?}", c.name));
            }
            if f.generated(*k, &a) {
                failures.push(format!("{}: writer generates false instance {k:?} {n:?}", c.name));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn configuration_hypotheses_are_read_from_the_figure() {
    let f = fixture("A B C = triangle\nD = midpoint(A, B)\nE = midpoint(A, C)\nF = reflect(E, A)\nprove para(D, E, B, C)");
    let p = |n: &str| f.id(n);
    let t = &f.trace;
    assert!(config_holds(t, &Hyp::SplitAlike(p("A"), p("D"), p("B"), p("E"), p("C"))));
    assert!(!config_holds(t, &Hyp::SplitAlike(p("A"), p("D"), p("B"), p("F"), p("C"))));
    assert!(config_holds(t, &Hyp::Between(p("D"), p("A"), p("B"))));
    assert!(!config_holds(t, &Hyp::Outside(p("D"), p("A"), p("B"))));
    assert!(config_holds(t, &Hyp::Outside(p("F"), p("E"), p("C"))));
    assert!(!config_holds(t, &Hyp::Between(p("A"), p("A"), p("B"))));
    assert!(!config_holds(t, &Hyp::Triangle(p("A"), p("D"), p("B"))));
    assert!(!config_holds(t, &Hyp::Distinct(vec![p("A"), p("D"), p("A")])));
    assert!(!config_holds(t, &Hyp::OutsideParity([(p("D"), p("A"), p("B")), (p("E"), p("A"), p("C")), (p("F"), p("E"), p("C"))], false)));
    assert!(config_holds(t, &Hyp::OutsideParity([(p("D"), p("A"), p("B")), (p("E"), p("A"), p("C")), (p("F"), p("E"), p("C"))], true)));
}

fn imo_2012_p1() -> Fix {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../corpus/imo_ag_30.txt")).expect("corpus");
    let (_, body) = ddar::corpus::read_corpus(&text).expect("parses").into_iter().find(|(n, _)| n == "translated_imo_2012_p1").expect("problem");
    let problem = ddar::corpus::translate_text("translated_imo_2012_p1", &body, 1).expect("translates").problem;
    let (_, trace, deps) = ddar::runner::solve_problem_with_trace(&problem).expect("solves").expect("proved directly");
    Fix { trace, goal: problem.goal.clone().unwrap(), deps }
}

fn strict() -> Opts {
    Opts { deadline: Some(Instant::now() + Duration::from_secs(30)), strict: true }
}

fn theorem_reasons(hp: &human::HumanProof) -> Vec<(usize, usize, usize, usize)> {
    let mut out = Vec::new();
    for (bi, b) in hp.blocks.iter().enumerate() {
        for (si, s) in b.body.iter().enumerate() {
            if let Sentence::Chain { links, .. } = s {
                for (li, l) in links.iter().enumerate() {
                    for (ri, r) in l.reasons.iter().enumerate() {
                        if let Reason::Atom { key, .. } = r {
                            if ddar::human::theorems::is_theorem(*key) && l.combination.iter().any(|c| c.reason as usize == ri) {
                                out.push((bi, si, li, ri));
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[test]
fn a_proof_built_from_theorems_verifies_and_shows_no_similar_triangles() {
    let f = imo_2012_p1();
    let hp = human::write(&f.trace, &f.goal, &f.deps, &[], &strict());
    assert!(hp.available);
    assert!(hp.metrics.library, "the theorem search did not win on IMO 2012 P1");
    assert!(human::verify(&f.trace, &f.goal, &f.deps, &[], &hp).is_empty());
    assert!(hp.metrics.similar_steps <= 2, "{}", human::render_en(&f.trace, &hp, &[]));
    assert!(!hp.blocks.iter().any(|b| matches!(b.stmt, ddar::human::Stmt::Sim { .. } | ddar::human::Stmt::Congruent { .. })), "{}", human::render_en(&f.trace, &hp, &[]));
    assert!(hp.metrics.theorem_steps >= 4, "{}", human::render_en(&f.trace, &hp, &[]));
    assert!(!theorem_reasons(&hp).is_empty());
}

#[test]
fn checker_rejects_a_corrupted_theorem_instance() {
    let f = imo_2012_p1();
    let hp = human::write(&f.trace, &f.goal, &f.deps, &[], &strict());
    let sites = theorem_reasons(&hp);
    assert!(!sites.is_empty());
    let (bi, si, li, ri) = sites[0];
    let n = f.trace.n as PointId;
    let mut rejected = 0;
    let mut tried = 0;
    let Sentence::Chain { links, .. } = &hp.blocks[bi].body[si] else { unreachable!() };
    let Reason::Atom { key, args, .. } = &links[li].reasons[ri] else { unreachable!() };
    let (key, args) = (*key, args.clone());
    let mut variants: Vec<(AtomKey, Vec<PointId>)> = Vec::new();
    for pos in 0..args.len() {
        for p in 0..n {
            if !args.contains(&p) {
                let mut a = args.clone();
                a[pos] = p;
                variants.push((key, a));
                break;
            }
        }
    }
    for other in ddar::human::theorems::LIBRARY {
        if other != key {
            variants.push((other, args.clone()));
        }
    }
    let mut swapped = args.clone();
    swapped.swap(0, 1);
    variants.push((key, swapped));
    for (k, a) in variants {
        let mut bad = hp.clone();
        if let Sentence::Chain { links, .. } = &mut bad.blocks[bi].body[si] {
            if let Reason::Atom { key, args, .. } = &mut links[li].reasons[ri] {
                *key = k;
                *args = a.clone();
            }
        }
        tried += 1;
        if !human::verify(&f.trace, &f.goal, &f.deps, &[], &bad).is_empty() {
            rejected += 1;
        } else {
            panic!("corrupted {k:?} {:?} accepted", f.names(&a));
        }
    }
    assert_eq!(rejected, tried);
}

#[test]
fn the_theorem_search_is_deterministic() {
    let f = imo_2012_p1();
    let a = human::view::engine_json(&f.trace, &human::write(&f.trace, &f.goal, &f.deps, &[], &Opts::default()), &f.trace.closure(&f.deps));
    for _ in 0..2 {
        let b = human::view::engine_json(&f.trace, &human::write(&f.trace, &f.goal, &f.deps, &[], &Opts::default()), &f.trace.closure(&f.deps));
        assert_eq!(a, b);
    }
    let handles: Vec<_> = (0..3)
        .map(|_| {
            std::thread::spawn(move || {
                let f = imo_2012_p1();
                human::view::engine_json(&f.trace, &human::write(&f.trace, &f.goal, &f.deps, &[], &Opts::default()), &f.trace.closure(&f.deps))
            })
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), a);
    }
}

#[test]
fn the_theorem_search_respects_its_deadline_and_never_lies() {
    let f = imo_2012_p1();
    for ms in [0u64, 1, 5, 20] {
        let opts = Opts { deadline: Some(Instant::now() + Duration::from_millis(ms)), strict: false };
        let start = Instant::now();
        let hp = human::write(&f.trace, &f.goal, &f.deps, &[], &opts);
        assert!(start.elapsed() < Duration::from_millis(ms + 1500), "{ms} ms deadline took {:?}", start.elapsed());
        if hp.available {
            assert!(human::verify(&f.trace, &f.goal, &f.deps, &[], &hp).is_empty(), "{ms} ms deadline produced a proof that fails the check");
        }
    }
}

#[test]
fn every_fixture_proof_verifies() {
    for c in cases() {
        let f = fixture(c.src);
        let hp = human::write(&f.trace, &f.goal, &f.deps, &[], &strict());
        if hp.available {
            let v = human::verify(&f.trace, &f.goal, &f.deps, &[], &hp);
            assert!(v.is_empty(), "{}: {v:?}", c.name);
        }
    }
}

#[test]
fn a_theorem_is_never_cited_to_restate_the_problem_itself() {
    for (src, key, word) in [
        ("A B C = triangle\nP = on_circum(A, B, C)\nX = foot(P, line(B, C))\nY = foot(P, line(C, A))\nZ = foot(P, line(A, B))\nprove coll(X, Y, Z)", AtomKey::Simson, "Simson"),
        ("A B C = triangle\nD = on_line(B, C)\nE = on_line(C, A)\nF = on_line(A, B)\nM = meet(circumcircle(A, E, F), circumcircle(B, F, D))\nprove cyclic(C, D, E, M)", AtomKey::Miquel, "Miquel"),
        ("O A = segment\nP = shift(A, O, A)\nT1 T2 = tangent(P, circle(O, A))\nprove cong(P, T1, P, T2)", AtomKey::EqualTangents, "equal tangents"),
    ] {
        let f = fixture(src);
        let _ = key;
        let hp = human::write(&f.trace, &f.goal, &f.deps, &[], &strict());
        let t = human::render_en(&f.trace, &hp, &[]);
        assert!(!t.contains(word), "{t}");
        assert!(human::verify(&f.trace, &f.goal, &f.deps, &[], &hp).is_empty());
    }
}
