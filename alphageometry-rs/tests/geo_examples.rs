//! Example validation.
//!
//! * `examples/*.geo` — the theorem library: each must compile *and prove
//!   directly* (no auxiliary points).
//! * `examples/imo/*.geo` — full contest problems: each must compile and its
//!   goal must hold numerically in the sampled figure (i.e. the construction is
//!   correct). Proving some of them needs auxiliary points, which is the job of
//!   the aux search, not this test.

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
fn theorem_library_proves_directly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut proven = Vec::new();
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
        assert!(
            solve_problem(&compiled.problem).unwrap(),
            "{}: DDAR did not prove the goal",
            path.display()
        );
        proven.push(path.file_name().unwrap().to_string_lossy().into_owned());
    }
    eprintln!(
        "proved {} theorem examples: {}",
        proven.len(),
        proven.join(", ")
    );
    assert!(proven.len() >= 12, "expected at least 12 theorem examples");
}

#[test]
fn imo_problems_construct_correctly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/imo");
    if !dir.exists() {
        return;
    }
    let mut checked = Vec::new();
    for path in geo_files(&dir) {
        let src = fs::read_to_string(&path).unwrap();
        let compiled =
            compile(&src).unwrap_or_else(|e| panic!("{}: compile error: {e}", path.display()));
        // The figure must satisfy the stated goal numerically (construction is
        // correct); whether pure DDAR proves it is a separate question.
        assert_ne!(
            compiled.goal_numerically_holds,
            Some(false),
            "{}: goal is numerically false — the construction is wrong",
            path.display()
        );
        let direct = solve_problem(&compiled.problem).unwrap_or(false);
        checked.push(format!(
            "{}{}",
            path.file_name().unwrap().to_string_lossy(),
            if direct {
                " (proves directly!)"
            } else {
                " (needs aux)"
            }
        ));
    }
    eprintln!(
        "constructed {} IMO problems: {}",
        checked.len(),
        checked.join(", ")
    );
    assert!(!checked.is_empty());
}
