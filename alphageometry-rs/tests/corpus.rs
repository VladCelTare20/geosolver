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
        (JGEX, "examples/complete2/012/complete_004_6_GDD_FULL_81-109_101.gex", "ddar"),
        (JGEX, "examples/complete2/012/complete_002_6_GDD_FULL_41-60_59.gex", "ddar"),
        (JGEX, "examples/complete2/012/complete_004_6_GDD_FULL_81-109_90.gex", "ddar"),
    ];
    for (corpus, name, method) in cases {
        let o = solve_one(name, &statement(corpus, name), Duration::from_secs(60));
        assert!(o.proved, "{name}: {} {}", o.status, o.detail);
        assert_eq!(o.status, "proved");
        assert_eq!(o.method, method, "{name}");
        let proof = o.proof.as_deref().expect("a proved result carries a proof");
        assert!(proof.contains("Proof of") && proof.contains('∎'), "{name}: {proof}");
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
            "a b c = triangle a b c; o = circle o a b c; m = midpoint m b c ? perp o m a b".to_string(),
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
