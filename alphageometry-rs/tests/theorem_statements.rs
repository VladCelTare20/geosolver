use ddar::geo::compile;
use ddar::human::{self, AuxInfo, EngineTrace, Opts};
use ddar::predicate::Problem;
use ddar::proof::FactId;
use ddar::theory::{audit, detect, Forbid, Thm};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const NOT_DETECTED: &[&str] = &["parallel_chords"];

fn statements() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus/theorem_statements");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("corpus/theorem_statements")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "geo").then(|| {
                let key = p.file_stem().unwrap().to_string_lossy().to_string();
                (key, std::fs::read_to_string(&p).unwrap())
            })
        })
        .collect();
    out.sort();
    out
}

fn split_metric(src: &str) -> (String, String) {
    let mut cons = Vec::new();
    let mut goal = String::new();
    for line in src.lines() {
        let l = line.split('#').next().unwrap_or("").trim();
        if let Some(g) = l.strip_prefix("prove ") {
            goal = g.trim().to_string();
        } else if !l.is_empty() {
            cons.push(l.to_string());
        }
    }
    (cons.join("\n"), goal)
}

enum Verdict {
    Proved(String),
    Circular,
}

struct Solved {
    trace: EngineTrace,
    deps: Vec<FactId>,
    aux: Vec<AuxInfo>,
    problem: Problem,
}

fn solve_ddar(problem: &Problem) -> Option<Solved> {
    if let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(problem) {
        return Some(Solved {
            trace,
            deps,
            aux: Vec::new(),
            problem: problem.clone(),
        });
    }
    let deadline = Some(Instant::now() + Duration::from_secs(60));
    let (found, _) = ddar::aux_search::solve_max_until(problem, false, deadline);
    let cons = found?.constructions;
    let aug = ddar::aux_search::apply_constructions(problem, &cons);
    let (_, trace, deps) = ddar::runner::solve_problem_with_trace(&aug).ok()??;
    let descs: Vec<String> = cons
        .iter()
        .map(|c| format!("{} = {}", c.name, c.desc))
        .collect();
    let aux = human::aux_infos(&aug, problem.points.len(), &descs);
    Some(Solved {
        trace,
        deps,
        aux,
        problem: aug,
    })
}

fn check_ddar(key: &str, thm: Thm, problem: &mut Problem) -> Result<Verdict, String> {
    let forbid = Forbid::closure_of(&[thm]);
    let g = detect::guard(problem);
    if !NOT_DETECTED.contains(&key) && !g.fired() {
        return Err("the statement was not detected".into());
    }
    if let Some(t) = g.unavoidable() {
        return if t == thm {
            Ok(Verdict::Circular)
        } else {
            Err(format!("unavoidable {t:?} reported for {thm:?}"))
        };
    }
    problem.forbid = g.forbid;
    let Some(mut s) = solve_ddar(problem) else {
        return Ok(Verdict::Circular);
    };
    let raw = audit::raw(&s.trace.facts, &s.deps, forbid);
    if !raw.is_empty() {
        return Err(format!("the raw proof cites {raw:?}"));
    }
    let goal = s.problem.goal.clone().unwrap();
    let opts = Opts {
        deadline: Some(Instant::now() + Duration::from_secs(30)),
        strict: false,
    };
    let hp = human::write(&s.trace, &goal, &s.deps, &s.aux, &opts);
    let text = if hp.available {
        let v = human::verify(&s.trace, &goal, &s.deps, &s.aux, &hp);
        if !v.is_empty() {
            return Err(format!("checker violations: {v:?}"));
        }
        s.trace.forbid = forbid;
        let structural = audit::human(&s.trace, &hp);
        if !structural.is_empty() {
            return Err(format!("the human proof cites {structural:?}"));
        }
        human::render_en(&s.trace, &hp, &s.aux)
    } else {
        String::new()
    };
    let hits = audit::text(forbid, &text);
    if !hits.is_empty() {
        return Err(format!("the human proof names {hits:?}:\n{text}"));
    }
    let first = text
        .lines()
        .find(|l| l.trim_start().starts_with("1."))
        .map(str::to_string)
        .unwrap_or_else(|| "(no human proof; raw derivation only)".into());
    Ok(Verdict::Proved(first))
}

fn check_metric(thm: Thm, src: &str) -> Result<Verdict, String> {
    let forbid = Forbid::closure_of(&[thm]);
    let (cons, goal) = split_metric(src);
    let deadline = Some(Instant::now() + Duration::from_secs(60));
    let (res, stated) =
        ddar::metric::solve_until_guarded(&cons, &goal, ddar::metric::DEFAULT_SAMPLES, deadline);
    if !stated.contains(&thm) {
        return Err(format!("detected {stated:?}, not {thm:?}"));
    }
    match res {
        Ok(proof) => {
            let hits = audit::text(forbid, &proof);
            if !hits.is_empty() {
                return Err(format!("the proof names {hits:?}:\n{proof}"));
            }
            let first = proof
                .lines()
                .find(|l| l.trim_start().starts_with("1."))
                .unwrap_or("")
                .trim()
                .to_string();
            Ok(Verdict::Proved(first))
        }
        Err(ddar::metric::MetricError::NoProof { .. }) => Ok(Verdict::Circular),
        Err(e) => Err(format!("metric prover: {}", e.message())),
    }
}

#[test]
fn every_theorem_statement_is_proved_without_itself_or_is_reported_circular() {
    let mut failures = Vec::new();
    let mut table = Vec::new();
    let all = statements();
    assert!(all.len() >= 35, "{} statements", all.len());
    for (key, src) in &all {
        let Some(thm) = Thm::by_key(key) else {
            failures.push(format!("{key}: no theorem has this key"));
            continue;
        };
        let verdict = match compile(src) {
            Ok(mut c) => check_ddar(key, thm, &mut c.problem),
            Err(e) if e.is_metric_goal() => check_metric(thm, src),
            Err(e) => Err(format!("does not compile: {}", e.message())),
        };
        match verdict {
            Ok(Verdict::Proved(first)) => table.push(format!("{key:26} proved   {first}")),
            Ok(Verdict::Circular) => table.push(format!("{key:26} circular")),
            Err(e) => failures.push(format!("{key}: {e}")),
        }
    }
    eprintln!("{}", table.join("\n"));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    let proved = table.iter().filter(|l| l.contains(" proved ")).count();
    assert!(
        proved >= 30,
        "only {proved} statements proved:\n{}",
        table.join("\n")
    );
}

#[test]
fn the_inscribed_angle_theorem_is_reported_circular() {
    let src = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../corpus/theorem_statements/inscribed_angle.geo"),
    )
    .unwrap();
    let c = compile(&src).unwrap();
    let g = detect::guard(&c.problem);
    assert_eq!(g.unavoidable(), Some(Thm::InscribedAngle));
}

const FALSE_DDAR: &[(&str, &str)] = &[
    (
        "midline",
        "A B C = triangle\nD = midpoint(A, B)\nE = on_line(A, C)\nprove para(D, E, B, C)",
    ),
    (
        "orthocentre",
        "A B C = triangle\nH = point: perp(A, H, B, C)\nprove perp(C, H, A, B)",
    ),
    (
        "thales",
        "A B = segment\nO = midpoint(A, B)\nC = free\nprove perp(C, A, C, B)",
    ),
    (
        "menelaus",
        "A B C = triangle\nD = on_line(B, C)\nE = on_line(C, A)\nF = on_line(A, B)\nprove dist(B, D)*dist(C, E)*dist(A, F) = dist(D, C)*dist(E, A)*dist(F, B)",
    ),
    (
        "inscribed_converse",
        "A B C = triangle\nD = free\nprove cyclic(A, B, C, D)",
    ),
];

#[test]
fn false_statements_stay_unproved_with_and_without_the_ban() {
    for (key, src) in FALSE_DDAR {
        let thm = Thm::by_key(key).unwrap();
        let c = match compile(src) {
            Ok(c) => c,
            Err(e) => panic!("{key}: {}", e.message()),
        };
        assert_eq!(
            c.goal_numerically_holds,
            Some(false),
            "{key}: the goal holds"
        );
        assert!(
            !detect::guard(&c.problem).fired(),
            "{key}: a false goal was detected"
        );
        for forbid in [Forbid::NONE, Forbid::closure_of(&[thm])] {
            let mut p = c.problem.clone();
            p.forbid = forbid;
            let proved = ddar::runner::solve_problem(&p).unwrap_or(false);
            assert!(
                !proved,
                "{key}: a false statement was proved (ban {forbid:?})"
            );
        }
    }
}

#[test]
fn false_metric_statements_stay_unproved_under_the_ban() {
    use ddar::synthetic::{prove_euclidean, Outcome};
    let cases: &[(&[Thm], &str, &str)] = &[
        (
            &[Thm::Pythagoras],
            "B C = segment\nA = free",
            "dist(A, B)^2 + dist(A, C)^2 = dist(B, C)^2",
        ),
        (
            &[Thm::Pythagoras],
            "B C = segment\nA = on_dia(B, C)",
            "dist(A, B)^2 + dist(A, C)^2 = 2*dist(B, C)^2",
        ),
        (
            &[Thm::Stewart],
            "B = free\nC = point: dist(B,C)=6\nD = point: coll(B,D,C), dist(B,D)=2\nA = free",
            "9*dist(A,D)^2 = 6*dist(A,B)^2 + 3*dist(A,C)^2 - dist(B,C)^2",
        ),
        (
            &[Thm::Apollonius],
            "A B C = triangle\nM = midpoint(B, C)",
            "4*dist(A,M)^2 = 2*dist(A,B)^2 + 2*dist(A,C)^2 - 2*dist(B,C)^2",
        ),
        (
            &[Thm::Apollonius],
            "A B C = triangle\nM = on_line(B, C)",
            "4*dist(A,M)^2 = 2*dist(A,B)^2 + 2*dist(A,C)^2 - dist(B,C)^2",
        ),
    ];
    for (thms, cons, goal) in cases {
        let forbid = Forbid::closure_of(thms);
        let out = ddar::theory::with_forbid(forbid, || prove_euclidean(cons, goal));
        if let Ok(Outcome::Proved(p)) = out {
            panic!("{goal} was proved under the ban:\n{p}");
        }
        let ratio = ddar::theory::with_forbid(forbid, || ddar::ratio::prove_ratio(cons, goal));
        assert!(
            !matches!(ratio, Ok(Outcome::Proved(_))),
            "{goal}: ratio prover"
        );
        let (res, _) =
            ddar::metric::solve_until_guarded(cons, goal, ddar::metric::DEFAULT_SAMPLES, None);
        assert!(
            res.is_err(),
            "{goal} was proved by the guarded metric solve"
        );
    }
}
