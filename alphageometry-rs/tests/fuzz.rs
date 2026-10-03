use std::collections::HashMap;
use std::time::Duration;

use ddar::bench::Outcome;
use ddar::corpus::{self, parse_problem, read_corpus, render_problem, sample_figure};
use ddar::fuzz::{generate, solve_case, verdict, Case, Config, Kind};

const JGEX: &str = include_str!("../../corpus/jgex_ag_231.txt");

fn first_problems(n: usize) -> Vec<(String, String)> {
    read_corpus(JGEX).unwrap().into_iter().take(n).collect()
}

#[test]
fn fuzz_smoke_no_false_proof() {
    let problems = first_problems(6);
    let gen = generate(
        &problems,
        &Config {
            per: 4,
            seed: 20261003,
            samples: 4,
        },
    );
    assert!(gen.skipped.is_empty(), "{:?}", gen.skipped);
    assert!(gen.cases.len() >= 36, "only {} cases", gen.cases.len());
    for k in Kind::ALL {
        assert!(gen.cases.iter().any(|c| c.kind == k), "no {} case", k.tag());
    }
    for case in &gen.cases {
        let aux = case.kind == Kind::HypSpecial;
        let budget = Duration::from_millis(if aux { 500 } else { 20_000 });
        let o = solve_case(&case.name, &case.text, budget, aux);
        assert!(o.parsed, "{} did not translate: {}", case.name, o.detail);
        assert_eq!(
            verdict(case, &o),
            "ok",
            "{} [{}] {}\n{}\n{:?}",
            case.name,
            case.kind.tag(),
            case.mutation,
            case.text,
            o.proof
        );
        if case.kind == Kind::Goal || case.kind == Kind::HypGeneric {
            assert_eq!(o.goal_numeric, Some(false), "{} must be false on its figure", case.name);
        }
        if case.kind == Kind::HypSpecial {
            assert_eq!(o.goal_numeric, Some(true), "{} must hold on the original figure", case.name);
        }
    }
}

#[test]
fn fuzz_generation_is_deterministic() {
    let problems = first_problems(3);
    let cfg = Config {
        per: 3,
        seed: 7,
        samples: 3,
    };
    let a: Vec<String> = generate(&problems, &cfg).cases.into_iter().map(|c| c.text).collect();
    let b: Vec<String> = generate(&problems, &cfg).cases.into_iter().map(|c| c.text).collect();
    assert_eq!(a, b);
}

#[test]
fn pinned_figure_reproduces_the_problem() {
    let (name, text) = &first_problems(1)[0];
    let prob = parse_problem(name, text).unwrap();
    let fig = (1..200)
        .find_map(|s| sample_figure(&prob, s).unwrap().filter(|f| f.goal_holds))
        .unwrap();
    let pinned = render_problem(&prob, &fig.coords);
    let o = solve_case(name, &pinned, Duration::from_secs(20), false);
    assert_eq!(o.status, "proved", "{pinned}\n{}", o.detail);
    let tr = corpus::translate(&parse_problem(name, &pinned).unwrap(), 1, 1).unwrap();
    for p in &tr.problem.points {
        assert_eq!(p.value, fig.coords[&p.name], "{} moved", p.name);
    }
    let none: HashMap<String, ddar::numerics::Vec2> = HashMap::new();
    assert_eq!(render_problem(&prob, &none).replace(' ', ""), text.replace(' ', ""));
}

#[test]
fn any_proof_of_a_false_variant_is_unsound() {
    let case = |kind| Case {
        name: "x".into(),
        origin: "x".into(),
        kind,
        mutation: String::new(),
        text: String::new(),
    };
    let proved = Outcome {
        proved: true,
        status: "proved".into(),
        goal_numeric: Some(true),
        ..Outcome::default()
    };
    for k in [Kind::Goal, Kind::HypGeneric, Kind::HypSpecial] {
        assert_eq!(verdict(&case(k), &proved), "UNSOUND");
    }
    assert_eq!(verdict(&case(Kind::Degenerate), &proved), "ok");
    let flagged = Outcome {
        status: "UNSOUND".into(),
        ..proved.clone()
    };
    assert_eq!(verdict(&case(Kind::Degenerate), &flagged), "UNSOUND");
}
