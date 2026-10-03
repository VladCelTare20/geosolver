//! The verified named-theorem corpus: every program in `examples/named/`
//! (classical results from Euclid, Johnson, Altshiller-Court, Coxeter &
//! Greitzer, EGMO, cut-the-knot, …) must compile, hold numerically, and be
//! proved by the engine — directly or with a small auxiliary-point search.
//!
//! `examples/named/unproven/` holds correct statements the engine cannot yet
//! prove within a small budget; they are documentation, not tested here.

use ddar::aux_search::solve_with_aux;
use ddar::geo::compile;
use ddar::metric;
use ddar::runner::solve_problem;
use std::fs;
use std::path::{Path, PathBuf};

fn geo_files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .expect("dir")
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension().and_then(|e| e.to_str()) == Some("geo")).then_some(p)
        })
        .collect();
    v.sort();
    v
}

/// Split a program into (construction, goal) at its `prove` line.
fn split_goal(src: &str) -> (String, String) {
    let mut cons = Vec::new();
    let mut goal = None;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            match s.strip_prefix("prove ") {
                Some(g) => goal = Some(g.trim().to_string()),
                None if !s.is_empty() => cons.push(s.to_string()),
                None => {}
            }
        }
    }
    (cons.join("\n"), goal.expect("a `prove` line"))
}

#[test]
fn named_theorem_corpus_proves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/named");
    let mut proven = 0usize;
    for path in geo_files(&dir) {
        let src = fs::read_to_string(&path).unwrap();
        let compiled = match compile(&src) {
            Ok(c) => c,
            // A metric goal has no DDAR predicate (it used to compile to a
            // trivially-true placeholder and "prove" vacuously); it must
            // instead be proved or numerically verified by the metric prover.
            Err(e) if e.is_metric_goal() => {
                let (cons, goal) = split_goal(&src);
                metric::solve(&cons, &goal, 48)
                    .unwrap_or_else(|e| panic!("{}: metric prover: {e}", path.display()));
                proven += 1;
                continue;
            }
            Err(e) => panic!("{}: compile error: {e}", path.display()),
        };
        assert_ne!(
            compiled.goal_numerically_holds,
            Some(false),
            "{}: goal is numerically false",
            path.display()
        );
        let direct = solve_problem(&compiled.problem).unwrap_or(false);
        let ok = direct || {
            // Mirror the small verification budget the corpus was curated with.
            let (res, _) = solve_with_aux(&compiled.problem, 2, 50_000, false);
            res.is_some()
        };
        assert!(ok, "{}: the engine did not prove the goal", path.display());
        proven += 1;
    }
    eprintln!("proved all {proven} named-theorem corpus programs");
    assert!(proven >= 90, "expected at least 90 corpus programs, found {proven}");
}
