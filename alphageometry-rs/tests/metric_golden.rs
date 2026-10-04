//! Byte-identical proof text for every metric example that was proved before
//! the trigonometric layer existed (`tests/golden/metric/<dir>_<name>.txt`).
//! New prover stages run only after the old ones fail, so a proof that already
//! existed must not change. `UPDATE_GOLDEN=1` rewrites the files.

use ddar::metric;
use std::fs;
use std::path::Path;

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
            match s.strip_prefix("prove ") {
                Some(g) => goal = Some(g.trim().to_string()),
                None => cons.push(s.to_string()),
            }
        }
    }
    (cons.join("\n"), goal.expect("a `prove` line"))
}

#[test]
fn previously_proved_metric_proofs_are_unchanged() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let golden = root.join("tests/golden/metric");
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let mut files: Vec<_> = fs::read_dir(&golden)
        .expect("tests/golden/metric")
        .filter_map(|e| {
            let p = e.unwrap().path();
            (p.extension().and_then(|x| x.to_str()) == Some("txt")).then_some(p)
        })
        .collect();
    files.sort();
    assert!(files.len() >= 25, "expected the 25 baseline proofs, found {}", files.len());
    let mut changed = Vec::new();
    for path in files {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let (dir, name) = stem.split_once('_').expect("<dir>_<name>.txt");
        let src = fs::read_to_string(root.join("examples").join(dir).join(format!("{name}.geo")))
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
        let (cons, goal) = split(&src);
        let proof = metric::solve(&cons, &goal, metric::DEFAULT_SAMPLES)
            .unwrap_or_else(|e| panic!("{stem}: no longer proved: {e}"));
        let want = fs::read_to_string(&path).unwrap();
        if proof != want {
            if update {
                fs::write(&path, &proof).unwrap();
            }
            changed.push(stem);
        }
    }
    assert!(
        update || changed.is_empty(),
        "proof text changed for: {changed:?} (UPDATE_GOLDEN=1 to accept)"
    );
}
