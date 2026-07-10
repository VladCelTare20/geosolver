//! The verified named-theorem corpus: every program in `examples/named/`
//! (classical results from Euclid, Johnson, Altshiller-Court, Coxeter &
//! Greitzer, EGMO, cut-the-knot, …) must compile, hold numerically, and be
//! proved by the engine — directly or with a small auxiliary-point search.
//!
//! `examples/named/unproven/` holds correct statements the engine cannot yet
//! prove within a small budget; they are documentation, not tested here.

use ddar::aux_search::solve_with_aux;
use ddar::geo::compile;
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

#[test]
fn named_theorem_corpus_proves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/named");
    let mut proven = 0usize;
    for path in geo_files(&dir) {
        let src = fs::read_to_string(&path).unwrap();
        let compiled =
            compile(&src).unwrap_or_else(|e| panic!("{}: compile error: {e}", path.display()));
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
