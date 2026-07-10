//! Small helpers shared by the CLI binary and the integration tests.

use crate::{Ddar, Problem};

/// Parse, assume all hypotheses, close, and check the goal for one problem.
pub fn solve(problem_str: &str) -> Result<bool, String> {
    let problem = Problem::parse(problem_str)?;
    solve_problem(&problem)
}

/// Solve an already-parsed problem: assume every hypothesis, run the deductive
/// closure, and report whether the goal holds.
pub fn solve_problem(problem: &Problem) -> Result<bool, String> {
    let mut ddar = Ddar::new(&problem.points);
    for pred in &problem.preds {
        ddar.force_pred(pred);
    }
    ddar.deduction_closure();
    match &problem.goal {
        Some(goal) => Ok(ddar.check_pred(goal)),
        None => Err("problem has no goal".to_string()),
    }
}

/// Solve with proof provenance; on success returns the rendered numbered proof.
pub fn solve_problem_with_proof(problem: &Problem) -> Result<Option<String>, String> {
    let mut ddar = Ddar::new_tracked(&problem.points);
    for pred in &problem.preds {
        ddar.force_pred(pred);
    }
    ddar.deduction_closure();
    let goal = problem
        .goal
        .as_ref()
        .ok_or_else(|| "problem has no goal".to_string())?;
    Ok(ddar.check_pred_deps(goal).map(|deps| {
        let text = ddar.render_pred(goal);
        ddar.proof_report(&deps, &text)
    }))
}

/// A parsed entry from the bundled `problems.tsv` dataset.
pub struct Entry<'a> {
    pub group: &'a str,
    pub name: &'a str,
    pub problem: &'a str,
}

/// Parse the bundled dataset (`group\tname\tproblem` per line).
pub fn parse_dataset(data: &str) -> Vec<Entry<'_>> {
    data.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let mut it = line.splitn(3, '\t');
            let group = it.next().unwrap();
            let name = it.next().expect("missing name");
            let problem = it.next().expect("missing problem");
            Entry {
                group,
                name,
                problem,
            }
        })
        .collect()
}
