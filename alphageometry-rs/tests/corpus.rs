use std::time::Duration;

use ddar::bench::{run_corpus, solve_one, RunConfig};
use ddar::corpus::{parse_problem, read_corpus, translate};

const IMO: &str = include_str!("../../corpus/imo_ag_30.txt");
const JGEX: &str = include_str!("../../corpus/jgex_ag_231.txt");

fn statement(corpus: &str, name: &str) -> String {
    read_corpus(corpus)
        .unwrap()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no problem {name}"))
        .1
}

#[test]
fn every_corpus_problem_translates_with_a_numerically_true_goal() {
    for (corpus, expected) in [(IMO, 30), (JGEX, 231)] {
        let problems = read_corpus(corpus).unwrap();
        assert_eq!(problems.len(), expected);
        for (name, text) in problems {
            let t = parse_problem(&name, &text)
                .and_then(|p| translate(&p, 1, 200))
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(t.goal_holds, "{name}: goal false on the sampled figure");
        }
    }
}

#[test]
fn quick_corpus_problems_prove_with_engine_proofs() {
    let cases = [
        (IMO, "translated_imo_2000_p1", "ddar"),
        (IMO, "translated_imo_2002_p2a", "ddar"),
        (IMO, "translated_imo_2010_p4", "ddar"),
        (IMO, "translated_imo_2015_p3", "aux"),
        (
            JGEX,
            "examples/complete2/012/complete_004_6_GDD_FULL_81-109_101.gex",
            "ddar",
        ),
        (
            JGEX,
            "examples/complete2/012/complete_002_6_GDD_FULL_41-60_59.gex",
            "ddar",
        ),
        (
            JGEX,
            "examples/complete2/012/complete_004_6_GDD_FULL_81-109_90.gex",
            "ddar",
        ),
    ];
    for (corpus, name, method) in cases {
        let o = solve_one(name, &statement(corpus, name), Duration::from_secs(60));
        assert!(o.proved, "{name}: {} {}", o.status, o.detail);
        assert_eq!(o.status, "proved");
        assert_eq!(o.method, method, "{name}");
        let proof = o.proof.as_deref().expect("a proved result carries a proof");
        assert!(
            proof.contains("Proof of") && proof.contains('∎'),
            "{name}: {proof}"
        );
        assert!(o.steps.unwrap_or(0) > 0, "{name}");
        assert_eq!(o.aux.is_empty(), method == "ddar", "{name}");
    }
}

#[test]
fn false_corpus_goals_are_never_proved() {
    let cases = [
        (
            statement(IMO, "translated_imo_2000_p1").replace("? cong e p e q", "? cong e p p q"),
            "cong e p p q",
        ),
        (
            statement(IMO, "translated_imo_2004_p5").replace("? cong a p c p", "? cong a p a c"),
            "cong a p a c",
        ),
        (
            "a b c = triangle a b c; o = circle o a b c; m = midpoint m b c ? perp o m a b"
                .to_string(),
            "perp o m a b",
        ),
    ];
    for (text, goal) in cases {
        assert!(text.ends_with(goal), "mutation did not apply: {text}");
        let o = solve_one("false", &text, Duration::from_secs(10));
        assert!(!o.proved, "a false goal `{goal}` was proved");
        assert_eq!(o.status, "goal-false", "{goal}: {}", o.detail);
        assert_eq!(o.goal_numeric, Some(false));
    }
}

#[test]
fn unknown_constructions_are_reported_not_skipped() {
    let o = solve_one(
        "bad",
        "a b c = triangle a b c; d = frobnicate d a b ? coll a b d",
        Duration::from_secs(5),
    );
    assert!(!o.proved);
    assert_eq!(o.status, "parse-error");
    assert!(o.detail.contains("frobnicate"), "{}", o.detail);
}

#[cfg(unix)]
#[test]
fn an_overrunning_child_is_killed_at_the_deadline() {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("ddar-kill-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("hang.sh");
    std::fs::write(&script, "#!/bin/sh\nexec sleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cfg = RunConfig {
        exe: script,
        corpus: dir.join("none.txt"),
        budget: Duration::from_millis(200),
        jobs: 1,
        threads: 1,
        mem_mb: 1024,
        grace: Duration::from_millis(300),
        proofs_dir: None,
        child_flag: "--corpus-one",
        human_stats: false,
    };
    let start = std::time::Instant::now();
    let out = run_corpus(&cfg, &[("p".into(), "unused".into())], &|_, _, _| {});
    let took = start.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].status, "killed-timeout");
    assert!(!out[0].proved);
    assert!(took < Duration::from_secs(5), "kill took {took:?}");
}

fn shown_angle_degrees(problem: &ddar::Problem, label: &str) -> Option<f64> {
    let shown = |n: &str| {
        let mut out = String::new();
        for (i, ch) in n.chars().enumerate() {
            match ch.to_digit(10) {
                Some(d) if i > 0 => out.push(char::from_u32(0x2080 + d).unwrap()),
                _ if ch == '\'' => out.push('\u{2032}'),
                _ => out.extend(ch.to_uppercase()),
            }
        }
        out
    };
    let mut names: Vec<(String, usize)> = problem
        .points
        .iter()
        .enumerate()
        .map(|(i, p)| (shown(&p.name), i))
        .collect();
    names.sort_by_key(|(n, _)| std::cmp::Reverse(n.chars().count()));
    let mut rest = label;
    let mut ids = Vec::new();
    while !rest.is_empty() {
        let (n, i) = names.iter().find(|(n, _)| rest.starts_with(n.as_str()))?;
        ids.push(*i);
        rest = &rest[n.len()..];
    }
    let [x, v, y] = ids[..] else { return None };
    let at = |i: usize| problem.points[i].value;
    let (u, w) = (at(x) - at(v), at(y) - at(v));
    Some(
        (u.dot(w) / (u.norm() * w.norm()))
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees(),
    )
}

#[test]
fn every_displayed_undirected_angle_goal_is_true_in_its_figure() {
    let mut checked = 0;
    let mut directed = 0;
    for (name, text) in read_corpus(JGEX).unwrap() {
        if !text.contains("? eqangle") {
            continue;
        }
        let t = parse_problem(&name, &text)
            .and_then(|p| translate(&p, 1, 200))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut problem = t.problem.clone();
        problem.goal = t.goals.first().cloned();
        let (_, goal) = ddar::svg::legend_lines(&problem);
        let goal = goal.expect("a goal");
        if goal.contains('\u{2221}') {
            directed += 1;
            continue;
        }
        let labels: Vec<&str> = goal
            .split('\u{2220}')
            .skip(1)
            .map(|s| s.split([' ', '=']).next().unwrap_or(""))
            .filter(|s| !s.starts_with('('))
            .collect();
        if labels.len() != 2 {
            continue;
        }
        let a =
            shown_angle_degrees(&problem, labels[0]).unwrap_or_else(|| panic!("{name}: {goal}"));
        let b =
            shown_angle_degrees(&problem, labels[1]).unwrap_or_else(|| panic!("{name}: {goal}"));
        assert!(
            (a - b).abs() < 1e-6,
            "{name}: shows {goal} but the figure has {a:.4}° and {b:.4}°"
        );
        checked += 1;
    }
    assert!(checked >= 20, "only {checked} goals were checked");
    assert!(
        directed >= 1,
        "the readings false in the figure are shown as directed angles"
    );
}
