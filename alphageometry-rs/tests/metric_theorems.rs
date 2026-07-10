//! The classical-theorem showcase in `examples/metric/*.geo`.
//!
//! Each file is a coordinate-free construction plus a `prove <equation>` goal.
//! Every one must yield a genuine **Euclidean** proof — a numbered synthetic
//! deduction citing named theorems (additive engine) or built from similar
//! triangles (ratio engine) — never the numerical-certificate fallback. This
//! guards that the famous ratio results (power of a point, geometric mean,
//! Menelaus, Ceva, the angle-bisector and intercept theorems, …) stay derivable
//! purely, and that a named theorem is never used to prove itself.

use ddar::metric;
use std::fs;
use std::path::Path;

/// Split a `.geo` metric program into (construction, goal).
fn split(src: &str) -> (String, String) {
    let mut cons = Vec::new();
    let mut goal = None;
    for raw in src.lines() {
        let line = raw.split('#').next().unwrap_or("");
        for stmt in line.split(';') {
            let s = stmt.trim();
            if s.is_empty() {
                continue;
            }
            match s
                .strip_prefix("prove ")
                .or_else(|| s.strip_prefix("goal:"))
                .or_else(|| s.strip_prefix("?"))
            {
                Some(g) => goal = Some(g.trim().to_string()),
                None => cons.push(s.to_string()),
            }
        }
    }
    (cons.join("\n"), goal.expect("a `prove` line"))
}

#[test]
fn every_metric_example_gets_a_euclidean_proof() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("metric");
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("examples/metric")
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension().and_then(|x| x.to_str()) == Some("geo")).then_some(p)
        })
        .collect();
    files.sort();
    assert!(files.len() >= 10, "expected the full theorem showcase");

    for path in files {
        let src = fs::read_to_string(&path).unwrap();
        let (cons, goal) = split(&src);
        let report = metric::solve(&cons, &goal, 48)
            .unwrap_or_else(|e| panic!("{}: solve error: {e}", path.display()));
        assert!(
            report.contains("EUCLIDEAN PROOF"),
            "{}: expected a Euclidean proof, got:\n{report}",
            path.display()
        );
        assert!(
            !report.contains("numerical certificate"),
            "{}: fell back to a numerical certificate:\n{report}",
            path.display()
        );
    }
}

/// A compound goal that is *not itself* a named theorem should **use** the
/// relevant theorem as a cited lemma (the shortcut), whereas a goal that *is* the
/// theorem must be derived from first principles (never self-cited).
#[test]
fn theorems_are_used_as_lemmas_but_never_to_prove_themselves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("metric");

    // Compound goal → cites the geometric-mean relation as a lemma.
    let compound = fs::read_to_string(dir.join("altitude_relation.geo")).unwrap();
    let (c1, g1) = split(&compound);
    let p1 = metric::solve(&c1, &g1, 48).unwrap();
    assert!(
        p1.contains("By the geometric-mean"),
        "compound proof should cite the geometric-mean theorem:\n{p1}"
    );

    // The theorem itself as a goal → must derive from similar triangles, not cite.
    let bare = fs::read_to_string(dir.join("geometric_mean.geo")).unwrap();
    let (c2, g2) = split(&bare);
    let p2 = metric::solve(&c2, &g2, 48).unwrap();
    assert!(
        !p2.contains("By the geometric-mean"),
        "a theorem must not be used to prove itself:\n{p2}"
    );
    assert!(
        p2.contains("similar (AA)"),
        "the bare theorem should be derived from similar triangles:\n{p2}"
    );
}
