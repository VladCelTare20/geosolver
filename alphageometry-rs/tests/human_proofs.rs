use ddar::geo::compile;
use ddar::human::{self, AuxInfo, BlockKind, EngineTrace, Expr, HumanProof, Opts, Reason, Sentence, Stmt};
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
    Case { name, problem: c.problem, aux: Vec::new() }
}

fn imo_2004_p1() -> Case {
    let text = "a b c = triangle a b c; o = midpoint o b c; m = on_circle m o b, on_line m a b; n = on_circle n o b, on_line n a c; r = angle_bisector r b a c, angle_bisector r m o n; o1 = circle o1 b m r; o2 = circle o2 c n r; p = on_circle p o1 r, on_circle p o2 r; f = foot f b a r ? coll p b c";
    let problem = ddar::corpus::translate_text("imo_2004_p1", text, 1).expect("translates").problem;
    let first = problem.points.len() - 1;
    let aux = human::aux_infos(&problem, first, &["f = foot(b -> ar)".to_string()]);
    Case { name: "imo_2004_p1", problem, aux }
}

fn cases() -> Vec<Case> {
    vec![
        geo_case("orthocenter_reflection", "examples/orthocenter_reflection.geo"),
        geo_case("euler_line", "examples/euler_line.geo"),
        geo_case("orthocenter_vertex_distance", "examples/named/orthocenter_vertex_distance.geo"),
        imo_2004_p1(),
        geo_case("imo_2023_p2", "tests/golden/human/imo_2023_p2.geo"),
    ]
}

fn solve(c: &Case) -> Solved {
    let (_, trace, deps) = ddar::runner::solve_problem_with_trace(&c.problem).expect("solves").unwrap_or_else(|| panic!("{} is not proved directly", c.name));
    Solved { trace, goal: c.problem.goal.clone().expect("goal"), deps, aux: c.aux.clone() }
}

fn strict() -> Opts {
    Opts { deadline: Some(Instant::now() + Duration::from_secs(30)), strict: true }
}

fn write(s: &Solved, opts: &Opts) -> HumanProof {
    human::write(&s.trace, &s.goal, &s.deps, &s.aux, opts)
}

fn verify(s: &Solved, hp: &HumanProof) -> Vec<human::check::Violation> {
    human::verify(&s.trace, &s.goal, &s.deps, &s.aux, hp)
}

fn golden_path(name: &str) -> PathBuf {
    root().join("tests/golden/human").join(format!("{name}.en.txt"))
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
            diffs.push(format!("--- {} (expected {})\n{}\n--- got\n{}", c.name, path.display(), want, text));
        }
    }
    assert!(diffs.is_empty(), "golden text changed (HP_BLESS=1 to accept):\n{}", diffs.join("\n"));
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
    let claims = |hp: &HumanProof| hp.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Claim(_))).count();

    let (_, hp, t) = get("orthocenter_reflection");
    assert_eq!(claims(hp), 0);
    assert!(t.contains("so A, B, C, K are concyclic. ∎"), "{t}");
    assert!(t.contains("BC is the perpendicular bisector of HK"), "{t}");

    let (_, hp, t) = get("euler_line");
    assert_eq!(claims(hp), 2);
    assert!(t.contains("Let M be the midpoint of BC."), "{t}");
    assert!(t.contains("Claim 1. △AHB ∼ △MON."), "{t}");
    assert!(t.contains("so O, G, H are collinear. ∎"), "{t}");

    let (_, hp, t) = get("orthocenter_vertex_distance");
    assert!(hp.blocks.last().unwrap().body.iter().any(|s| matches!(s, Sentence::Computation { .. })));
    assert!(t.contains("R = OA = OB = OC for its circumradius."), "{t}");
    assert!(t.contains("law of sines in △"), "{t}");
    assert!(t.contains("AH = 2·MₐO (AH = 2R·cos A; MₐO = R·cos A). ∎"), "{t}");
    assert!(!t.contains("|sin"), "{t}");
    assert!(!hp.blocks.last().unwrap().engine_facts.is_empty());

    let (_, hp, t) = get("imo_2004_p1");
    assert!((3..=7).contains(&claims(hp)), "{t}");
    assert!(t.contains("Let ω₂ be the circle centred at O₂ through C, N, R, P."), "{t}");
    assert!(t.contains("Let F be the foot of the perpendicular from B to AR."), "{t}");
    assert!(t.contains("R, N, F, O are concyclic"), "{t}");
    assert!(t.contains("∠MCN = ½·∠MON = ∠RON"), "{t}");
    assert!(t.contains("R, A, F, P are collinear"), "{t}");
    assert!(t.contains("so P, B, C are collinear. ∎"), "{t}");
    assert!(!t.contains("Angle chasing with"), "{t}");

    let (_, hp, t) = get("imo_2023_p2");
    assert!((3..=7).contains(&claims(hp)), "{t}");
    assert!(t.contains("Let Ω be the circumcircle of triangle ABC, with centre O"), "{t}");
    assert!(!t.contains("Hence NB = NC."), "{t}");
    assert!(t.contains("T, E, S, O are concyclic"), "{t}");
    assert!(t.contains("Claim 4. XA = XP."), "{t}");
    assert!(t.contains("Hence △OAX ≅ △OPX (OA = OP), so XA = XP."), "{t}");
    assert!(t.contains("so ∡BAX = ∡XAC. ∎"), "{t}");
    assert!(!t.contains("Angle chasing with"), "{t}");
    assert!(!t.contains("OA : OX = OP : OX"), "{t}");
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
                assert!(!b.engine_facts.is_empty(), "{}: block {} re-presents no engine fact", c.name, b.id);
            }
            for f in &b.engine_facts {
                assert!(closure.contains(f), "{}: block {} cites fact {f} outside the proof", c.name, b.id);
            }
        }
        assert_eq!(hp.blocks.last().unwrap().kind, BlockKind::Conclusion, "{}", c.name);
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
                terms[1] = Expr::Const { degrees: Rat::from_int(37) };
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
        if let Reason::Hyp { stmt, .. } = r {
            *stmt = Stmt::Coll { pts: vec![0, 1, 2] };
        }
    }
    assert!(!verify(&s, &m).is_empty(), "altered reasons accepted");

    let mut m = base.clone();
    let last = m.blocks.last_mut().unwrap();
    last.stmt = Stmt::Coll { pts: vec![0, 1, 2] };
    assert!(!verify(&s, &m).is_empty(), "wrong conclusion accepted");

    let mut m = base.clone();
    m.blocks.swap(0, 1);
    assert!(!verify(&s, &m).is_empty(), "reordered blocks accepted");

    let mut m = base.clone();
    m.blocks.retain(|b| b.kind != BlockKind::Conclusion);
    assert!(!verify(&s, &m).is_empty(), "proof without a conclusion accepted");
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
    assert!(hp.available, "a non-final corruption must not lose the proof");
    assert!(hp.blocks.iter().any(|b| b.kind == BlockKind::Raw), "the bad block is shown as raw steps");
    assert!(hp.blocks.iter().filter(|b| b.kind == BlockKind::Raw).all(|b| b.body.iter().all(|x| matches!(x, Sentence::Raw { .. }))));
    assert!(verify(&s, &hp).is_empty());

    let mut hp = write(&s, &strict());
    let last = hp.blocks.last_mut().unwrap();
    last.stmt = Stmt::Coll { pts: vec![0, 1, 2] };
    human::repair(&s.trace, &s.goal, &s.deps, &s.aux, &mut hp);
    assert!(!hp.available, "a bad conclusion makes the human proof unavailable, never wrong");
    assert!(human::render_en(&s.trace, &hp, &s.aux).contains("no human proof"));
}

#[test]
fn output_is_deterministic() {
    for c in cases() {
        let s = solve(&c);
        let closure = s.trace.closure(&s.deps);
        let a = human::view::engine_json(&s.trace, &write(&s, &strict()), &closure).to_string();
        let again = solve(&c);
        let b = human::view::engine_json(&again.trace, &write(&again, &strict()), &closure).to_string();
        assert_eq!(a, b, "{}: JSON differs between runs", c.name);
        let par: Vec<String> = std::thread::scope(|sc| {
            let hs: Vec<_> = (0..3).map(|_| sc.spawn(|| human::view::engine_json(&s.trace, &write(&s, &strict()), &closure).to_string())).collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(par.iter().all(|x| *x == a), "{}: JSON differs across threads", c.name);
    }
}

#[test]
fn an_expired_deadline_degrades_but_never_lies() {
    for c in cases() {
        let s = solve(&c);
        let start = Instant::now();
        let hp = write(&s, &Opts { deadline: Some(Instant::now()), strict: false });
        assert!(start.elapsed() < Duration::from_secs(5), "{}: writer ignored its deadline", c.name);
        if hp.available {
            assert!(verify(&s, &hp).is_empty(), "{}: timed-out output fails the checker", c.name);
        }
        let start = Instant::now();
        let hp = write(&s, &Opts { deadline: Some(Instant::now() + Duration::from_millis(1)), strict: false });
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
        let Ok(tr) = ddar::corpus::translate_text(&name, &body, 1) else { continue };
        let problem = tr.problem;
        let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(&problem) else { continue };
        let goal = problem.goal.clone().unwrap();
        proved += 1;
        let hp = human::write(&trace, &goal, &deps, &[], &Opts::default());
        if !hp.available {
            continue;
        }
        available += 1;
        let v = human::verify(&trace, &goal, &deps, &[], &hp);
        assert!(v.is_empty(), "{name}: {v:?}");
        let claims = hp.blocks.iter().filter(|b| matches!(b.kind, BlockKind::Claim(_))).count();
        assert!(claims <= 7, "{name}: {claims} claims");
        raw_blocks += hp.blocks.iter().filter(|b| b.kind == BlockKind::Raw).count();
    }
    assert!(proved >= 200, "only {proved} JGEX problems proved directly");
    assert!(available * 100 >= proved * 97, "human proofs for {available} of {proved}");
    eprintln!("jgex direct: proved {proved}, human {available}, raw blocks {raw_blocks}");
}

#[test]
fn checker_rejects_false_atom_instances() {
    use ddar::human::AtomKey;
    let c = imo_2004_p1();
    let s = solve(&c);
    let base = write(&s, &strict());
    let id = |n: &str| c.problem.points.iter().position(|p| p.name == n).unwrap() as ddar::predicate::PointId;
    let (a, b, cc, o, m, n, r) = (id("a"), id("b"), id("c"), id("o"), id("m"), id("n"), id("r"));
    let with = |key: AtomKey, args: Vec<ddar::predicate::PointId>| -> HumanProof {
        let mut hp = base.clone();
        let l = first_chain_link(&mut hp, false).expect("a chain link");
        l.reasons.push(Reason::Atom { key, stmt: Stmt::Coll { pts: vec![a, b, cc] }, args, from: Vec::new() });
        hp
    };
    let controls = [(AtomKey::Thales, vec![b, cc, n, o]), (AtomKey::PowerOfPoint, vec![a, b, m, cc, n])];
    for (key, args) in controls {
        assert!(verify(&s, &with(key, args.clone())).is_empty(), "true {key:?} {args:?} rejected");
    }
    let false_instances = [
        (AtomKey::Thales, vec![b, cc, n, r]),
        (AtomKey::TangentChord, vec![b, a, cc, n, o]),
        (AtomKey::PowerOfPoint, vec![a, b, m, cc, r]),
        (AtomKey::Midline, vec![o, m, b, a, cc]),
        (AtomKey::PerpBisector, vec![o, r, b, cc]),
        (AtomKey::Radii, vec![r, b, cc]),
    ];
    for (key, args) in false_instances {
        assert!(!verify(&s, &with(key, args.clone())).is_empty(), "false {key:?} {args:?} accepted");
    }
}
