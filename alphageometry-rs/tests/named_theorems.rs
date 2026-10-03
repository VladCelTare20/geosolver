//! The named-theorem corpus: every program in `examples/named/` (classical
//! results from Euclid, Johnson, Altshiller-Court, Coxeter & Greitzer, EGMO,
//! cut-the-knot, …) must compile and hold numerically, and gets an honest
//! status. Relational goals must be proved by DDAR (directly or with a small
//! auxiliary-point search). Metric goals must be proved by a classical
//! Euclidean proof — except the ones pinned in [`NUMERIC_ONLY`], which hold in
//! every sampled figure but have no Euclidean proof yet and so must come back
//! as `NoProof`, never as proved.
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

/// Metric goals with no Euclidean proof yet: true in every sampled figure,
/// reported as unproved. Moving one out of this list needs a real proof.
const NUMERIC_ONLY: &[&str] = &[
    "euler_formula_oi.geo",
    "symmedian_ratio.geo",
];

#[test]
fn named_theorem_corpus_proves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/named");
    let mut proven = 0usize;
    let mut numeric_only: Vec<String> = Vec::new();
    for path in geo_files(&dir) {
        let src = fs::read_to_string(&path).unwrap();
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let compiled = match compile(&src) {
            Ok(c) => c,
            // A metric goal has no DDAR predicate (it used to compile to a
            // trivially-true placeholder and "prove" vacuously); only the
            // classical Euclidean provers may prove it.
            Err(e) if e.is_metric_goal() => {
                let (cons, goal) = split_goal(&src);
                match metric::solve(&cons, &goal, metric::DEFAULT_SAMPLES) {
                    Ok(proof) => {
                        assert!(
                            proof.starts_with("EUCLIDEAN PROOF"),
                            "{file}: an Ok result must be a Euclidean proof:\n{proof}"
                        );
                        assert!(
                            !NUMERIC_ONLY.contains(&file.as_str()),
                            "{file} is now proved — remove it from NUMERIC_ONLY:\n{proof}"
                        );
                        proven += 1;
                    }
                    Err(e @ metric::MetricError::NoProof { .. }) => {
                        assert_eq!(e.numerically_holds(), Some(true), "{file}");
                        assert!(
                            e.evidence().is_some_and(|ev| ev.samples >= 8),
                            "{file}: too few sampled figures: {e:?}"
                        );
                        numeric_only.push(file);
                    }
                    Err(e) => panic!("{file}: metric prover: {e}"),
                }
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
    let expected: Vec<String> = NUMERIC_ONLY.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        numeric_only, expected,
        "the set of numeric-only (unproved) metric theorems changed"
    );
    eprintln!(
        "proved {proven} named-theorem corpus programs; {} hold numerically without a \
         Euclidean proof",
        numeric_only.len()
    );
    assert!(proven >= 93, "expected at least 93 proved corpus programs, found {proven}");
}
