//! The universal MAX solver must solve every problem in `examples/olympiad`
//! from its bare statement — no construction hints, and no per-problem tuning:
//! the exact same maximum-effort path (DDAR, else the iterative-deepening
//! auxiliary search across all cores) is applied to each. Release-only: the aux
//! search is slow under the debug numeric-consistency asserts.
#![cfg(not(debug_assertions))]

use ddar::aux_search::solve_max;
use ddar::geo;
use ddar::runner::solve_problem;

/// Solve a bare-statement problem with the universal MAX solver.
fn solved(src: &str) -> bool {
    let compiled = geo::compile(src).expect("compile");
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let direct = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        solve_problem(&compiled.problem).unwrap_or(false)
    }))
    .unwrap_or(false);
    let ok = direct || solve_max(&compiled.problem, false).0.is_some();
    std::panic::set_hook(prev);
    ok
}

#[test]
fn universal_solver_solves_every_olympiad_problem() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/olympiad");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("read examples/olympiad") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("geo") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        assert!(
            solved(&src),
            "unsolved from bare statement: {}",
            path.display()
        );
        count += 1;
    }
    assert!(
        count >= 6,
        "expected a populated problem set, found only {count}"
    );
}
