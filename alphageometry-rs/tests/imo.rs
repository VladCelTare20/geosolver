//! Integration test: every bundled IMO problem must be proved, matching the
//! reference Python DDAR (26/26).

use ddar::runner::{parse_dataset, solve};

const DATASET: &str = include_str!("../problems.tsv");

#[test]
fn all_bundled_problems_are_proved() {
    let entries = parse_dataset(DATASET);
    assert_eq!(entries.len(), 26, "expected 26 bundled problems");

    let mut failures = Vec::new();
    for e in &entries {
        match solve(e.problem) {
            Ok(true) => {}
            Ok(false) => failures.push(format!("{} ({}): not proven", e.name, e.group)),
            Err(err) => failures.push(format!("{} ({}): error: {err}", e.name, e.group)),
        }
    }

    assert!(
        failures.is_empty(),
        "some problems were not proved:\n{}",
        failures.join("\n")
    );
}
