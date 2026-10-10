//! Solve-rate benchmark over an AG1-format corpus.
//!
//! [`solve_one`] runs the full Euclidean pipeline on one problem — DDAR, then
//! the auxiliary-point search — under a wall-clock budget. A problem counts as
//! proved only when the engine produced a numbered proof for every goal
//! conjunct (on the figure augmented with any auxiliary points); a numeric
//! check never stands in for a proof.
//!
//! [`run_corpus`] runs many problems in parallel, each in its own child
//! process, so the deadline is enforced by killing the child: a deduction
//! closure that overruns its budget cannot hold the benchmark hostage.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::aux_search::{apply_constructions, solve_max_until};
use crate::corpus::{parse_problem, translate};
use crate::quiet_panic::quiet;
use crate::runner::solve_problem_with_proof;
use crate::Problem;

/// The result of one benchmark problem.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub name: String,
    /// The statement parsed and a figure satisfying every premise was built.
    pub parsed: bool,
    /// Every goal conjunct holds on the sampled figure (`None` if no figure).
    pub goal_numeric: Option<bool>,
    pub proved: bool,
    /// `ddar`, `aux`, or `-`.
    pub method: String,
    /// Auxiliary constructions used, `name = description`.
    pub aux: Vec<String>,
    /// Numbered proof steps (summed over goal conjuncts).
    pub steps: Option<usize>,
    pub secs: f64,
    /// `proved`, `unproved`, `timeout`, `goal-false`, `parse-error`,
    /// `translate-error`, `engine-panic`, `proof-missing`, `UNSOUND`,
    /// `killed-timeout`, `killed-memory`, `crashed`.
    pub status: String,
    pub detail: String,
    pub proof: Option<String>,
    pub proved_problems: Vec<(Problem, Vec<String>, usize)>,
    pub human: Vec<String>,
}

/// Column header of the results TSV.
pub const TSV_HEADER: &str =
    "name\tparsed\tgoal_numeric\tproved\tmethod\taux_count\taux\tsteps\tsecs\tstatus\tdetail";

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

impl Outcome {
    pub fn to_tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.3}\t{}\t{}",
            clean(&self.name),
            if self.parsed { "yes" } else { "no" },
            match self.goal_numeric {
                Some(true) => "yes",
                Some(false) => "no",
                None => "-",
            },
            if self.proved { "yes" } else { "no" },
            if self.method.is_empty() { "-" } else { &self.method },
            self.aux.len(),
            if self.aux.is_empty() {
                "-".to_string()
            } else {
                clean(&self.aux.join("; "))
            },
            self.steps.map_or("-".to_string(), |s| s.to_string()),
            self.secs,
            self.status,
            if self.detail.is_empty() {
                "-".to_string()
            } else {
                clean(&self.detail)
            },
        )
    }

    pub fn from_tsv(line: &str) -> Option<Outcome> {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 11 {
            return None;
        }
        let dash = |s: &str| if s == "-" { String::new() } else { s.to_string() };
        Some(Outcome {
            name: f[0].to_string(),
            parsed: f[1] == "yes",
            goal_numeric: match f[2] {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            },
            proved: f[3] == "yes",
            method: f[4].to_string(),
            aux: if f[6] == "-" {
                vec![]
            } else {
                f[6].split("; ").map(str::to_string).collect()
            },
            steps: f[7].parse().ok(),
            secs: f[8].parse().unwrap_or(0.0),
            status: f[9].to_string(),
            detail: dash(f[10]),
            proof: None,
            proved_problems: Vec::new(),
            human: Vec::new(),
        })
    }
}

pub(crate) fn proof_steps(proof: &str) -> usize {
    let header = proof.lines().find(|l| l.starts_with("Proof of")).unwrap_or("");
    header
        .rfind('(')
        .and_then(|i| header[i + 1..].split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| {
            proof
                .lines()
                .filter(|l| {
                    let t = l.trim_start();
                    let d = t.chars().take_while(char::is_ascii_digit).count();
                    d > 0 && t[d..].starts_with('.')
                })
                .count()
        })
}

pub(crate) enum Attempt {
    Proved {
        proof: String,
        aux: Vec<String>,
        problem: Problem,
    },
    NotProved {
        status: &'static str,
        detail: String,
    },
    Panicked,
}

pub(crate) fn prove_goal(problem: &Problem, deadline: Instant) -> Attempt {
    let direct = catch_unwind(AssertUnwindSafe(|| quiet(|| solve_problem_with_proof(problem))));
    match direct {
        Err(_) => return Attempt::Panicked,
        Ok(Err(e)) => {
            return Attempt::NotProved {
                status: "engine-panic",
                detail: e,
            }
        }
        Ok(Ok(Some(proof))) => return Attempt::Proved { proof, aux: vec![], problem: problem.clone() },
        Ok(Ok(None)) => {}
    }
    if Instant::now() >= deadline {
        return Attempt::NotProved {
            status: "timeout",
            detail: "budget spent in the base DDAR closure".into(),
        };
    }
    let (found, stats) = solve_max_until(problem, false, Some(deadline));
    let Some(found) = found else {
        return Attempt::NotProved {
            status: if Instant::now() >= deadline {
                "timeout"
            } else {
                "unproved"
            },
            detail: format!("{} DDAR runs in the aux search", stats.runs),
        };
    };
    let aug = apply_constructions(problem, &found.constructions);
    let aux: Vec<String> = found
        .constructions
        .iter()
        .map(|c| format!("{} = {}", c.name, c.desc))
        .collect();
    match catch_unwind(AssertUnwindSafe(|| quiet(|| solve_problem_with_proof(&aug)))) {
        Ok(Ok(Some(proof))) => Attempt::Proved { proof, aux, problem: aug },
        _ => Attempt::NotProved {
            status: "proof-missing",
            detail: format!(
                "aux search reported success with [{}] but proof extraction failed",
                aux.join("; ")
            ),
        },
    }
}

/// Run the full pipeline on one corpus statement within `budget`.
pub fn solve_one(name: &str, text: &str, budget: Duration) -> Outcome {
    let start = Instant::now();
    let deadline = start + budget;
    let mut out = Outcome {
        name: name.to_string(),
        method: "-".into(),
        ..Outcome::default()
    };
    let finish = |mut o: Outcome| {
        o.secs = start.elapsed().as_secs_f64();
        o
    };
    let prob = match parse_problem(name, text) {
        Ok(p) => p,
        Err(e) => {
            out.status = "parse-error".into();
            out.detail = e;
            return finish(out);
        }
    };
    let mut seed = 1u64;
    for _ in 0..4 {
        let tr = match translate(&prob, seed, 200) {
            Ok(t) => t,
            Err(e) => {
                out.status = "translate-error".into();
                out.detail = e;
                return finish(out);
            }
        };
        seed = tr.seed + 1;
        out.parsed = true;
        out.goal_numeric = Some(tr.goal_holds);
        let mut proofs: Vec<String> = Vec::new();
        let mut aux: Vec<String> = Vec::new();
        let mut proved_problems: Vec<(Problem, Vec<String>, usize)> = Vec::new();
        let mut panicked = false;
        let mut failure: Option<(&'static str, String)> = None;
        for g in &tr.goals {
            let mut p = tr.problem.clone();
            p.goal = Some(g.clone());
            let attempt = if tr.goal_holds {
                prove_goal(&p, deadline)
            } else {
                match catch_unwind(AssertUnwindSafe(|| quiet(|| solve_problem_with_proof(&p)))) {
                    Ok(Ok(Some(proof))) => Attempt::Proved { proof, aux: vec![], problem: p.clone() },
                    Err(_) => Attempt::Panicked,
                    _ => Attempt::NotProved {
                        status: "goal-false",
                        detail: "goal does not hold numerically on any sampled figure".into(),
                    },
                }
            };
            match attempt {
                Attempt::Proved { proof, aux: a, problem } => {
                    proofs.push(proof);
                    proved_problems.push((problem, a.clone(), tr.problem.points.len()));
                    aux.extend(a);
                }
                Attempt::NotProved { status, detail } => {
                    failure = Some((status, detail));
                    break;
                }
                Attempt::Panicked => {
                    panicked = true;
                    break;
                }
            }
        }
        if panicked {
            out.status = "engine-panic".into();
            out.detail = format!("DDAR panicked on the figure from seed {}", tr.seed);
            if Instant::now() < deadline {
                continue;
            }
            return finish(out);
        }
        if let Some((status, detail)) = failure {
            out.status = status.into();
            out.detail = detail;
            return finish(out);
        }
        out.steps = Some(proofs.iter().map(|p| proof_steps(p)).sum());
        out.method = if aux.is_empty() { "ddar" } else { "aux" }.into();
        out.aux = aux;
        out.proof = Some(proofs.join("\n"));
        out.proved_problems = proved_problems;
        if !tr.goal_holds {
            out.status = "UNSOUND".into();
            out.detail = "proved a goal that is numerically false".into();
            return finish(out);
        }
        out.proved = true;
        out.status = "proved".into();
        let o = finish(out);
        if o.secs > budget.as_secs_f64() {
            let mut o = o;
            o.proved = false;
            o.status = "timeout".into();
            o.detail = "proof found after the budget expired".into();
            return o;
        }
        return o;
    }
    finish(out)
}

/// Configuration of a parallel, process-isolated corpus run.
pub struct RunConfig {
    /// The executable to spawn per problem (normally `ddar` itself).
    pub exe: PathBuf,
    pub corpus: PathBuf,
    pub budget: Duration,
    /// Problems solved concurrently (child processes).
    pub jobs: usize,
    /// Rayon threads per child.
    pub threads: usize,
    /// Kill a child whose resident memory exceeds this many MiB.
    pub mem_mb: u64,
    /// How long past the budget a child may run before it is killed.
    pub grace: Duration,
    pub proofs_dir: Option<PathBuf>,
    /// The child mode flag: `--corpus-one` (benchmark) or `--fuzz-one`.
    pub child_flag: &'static str,
    pub human_stats: bool,
}

fn rss_mb(pid: u32) -> Option<u64> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4096 / (1024 * 1024))
}

fn run_child(cfg: &RunConfig, name: &str) -> Outcome {
    let start = Instant::now();
    let mut cmd = Command::new(&cfg.exe);
    cmd.arg(cfg.child_flag)
        .arg(&cfg.corpus)
        .arg(name)
        .arg("--budget")
        .arg(format!("{}", cfg.budget.as_secs_f64()))
        .env("RAYON_NUM_THREADS", cfg.threads.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = &cfg.proofs_dir {
        cmd.arg("--proofs").arg(d);
    }
    if cfg.human_stats {
        cmd.arg("--human-stats").arg("-");
    }
    let failed = |status: &str, detail: String| Outcome {
        name: name.to_string(),
        method: "-".into(),
        status: status.into(),
        detail,
        secs: start.elapsed().as_secs_f64(),
        ..Outcome::default()
    };
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return failed("crashed", format!("spawn failed: {e}")),
    };
    let pid = child.id();
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = BufReader::new(&mut stderr).read_to_string(&mut s);
        s
    });
    let grace = cfg.grace;
    let hard = cfg.budget + grace;
    let mut killed: Option<(&str, String)> = None;
    let exit = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {}
            Err(_) => break None,
        }
        if start.elapsed() >= hard {
            killed = Some((
                "killed-timeout",
                format!("no result {:.0}s past the {:.0}s budget", grace.as_secs_f64(), cfg.budget.as_secs_f64()),
            ));
        } else if rss_mb(pid).is_some_and(|m| m > cfg.mem_mb) {
            killed = Some(("killed-memory", format!("resident memory above {} MiB", cfg.mem_mb)));
        }
        if killed.is_some() {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();
    if let Some((status, detail)) = killed {
        return failed(status, detail);
    }
    let result = stdout
        .lines()
        .find_map(|l| l.strip_prefix("RESULT\t").and_then(Outcome::from_tsv));
    match result {
        Some(mut o) => {
            o.secs = o.secs.max(0.0);
            o.human = stdout.lines().filter(|l| l.starts_with("HUMAN\t")).map(str::to_string).collect();
            o
        }
        None => {
            let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            failed(
                "crashed",
                format!("exit {:?}, no result line; stderr: {tail}", exit.and_then(|e| e.code())),
            )
        }
    }
}

/// Run every listed problem of a corpus, `cfg.jobs` at a time, each in its own
/// process with a hard deadline. `progress` is called as each one finishes.
pub fn run_corpus(
    cfg: &RunConfig,
    problems: &[(String, String)],
    progress: &(dyn Fn(usize, usize, &Outcome) + Sync),
) -> Vec<Outcome> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<HashMap<usize, Outcome>> = Mutex::new(HashMap::new());
    std::thread::scope(|s| {
        for _ in 0..cfg.jobs.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= problems.len() {
                    break;
                }
                let o = run_child(cfg, &problems[i].0);
                let k = done.fetch_add(1, Ordering::SeqCst) + 1;
                progress(k, problems.len(), &o);
                results.lock().unwrap().insert(i, o);
            });
        }
    });
    let mut map = results.into_inner().unwrap();
    (0..problems.len()).filter_map(|i| map.remove(&i)).collect()
}

/// Child side of [`run_corpus`]: solve one named problem and print its
/// result line (and write its proof when `proofs_dir` is given).
pub fn child_main(corpus: &Path, name: &str, budget: Duration, proofs_dir: Option<&Path>) -> Outcome {
    let text = std::fs::read_to_string(corpus).unwrap_or_default();
    let problems = crate::corpus::read_corpus(&text).unwrap_or_default();
    let o = match problems.iter().find(|(n, _)| n == name) {
        Some((n, t)) => solve_one(n, t, budget),
        None => Outcome {
            name: name.to_string(),
            method: "-".into(),
            status: "parse-error".into(),
            detail: format!("no problem named `{name}` in {}", corpus.display()),
            ..Outcome::default()
        },
    };
    if let Some(dir) = proofs_dir {
        write_proof(dir, &o);
    }
    o
}

/// Write an outcome's proof (if any) to `dir/<sanitised name>.txt`.
pub fn write_proof(dir: &Path, o: &Outcome) {
    let Some(proof) = &o.proof else {
        return;
    };
    let _ = std::fs::create_dir_all(dir);
    let mut body = format!("{}\n", o.name);
    for a in &o.aux {
        body.push_str(&format!("aux: {a}\n"));
    }
    body.push('\n');
    body.push_str(proof);
    body.push('\n');
    let _ = std::fs::write(proof_path(dir, &o.name), body);
}

/// Where [`write_proof`] puts the proof of problem `name`.
pub fn proof_path(dir: &Path, name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    dir.join(format!("{safe}.txt"))
}

/// Read a results TSV written by the benchmark.
pub fn read_results(path: &Path) -> std::io::Result<Vec<Outcome>> {
    let f = std::fs::File::open(path)?;
    Ok(BufReader::new(f)
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.starts_with("name\t"))
        .filter_map(|l| Outcome::from_tsv(&l))
        .collect())
}
