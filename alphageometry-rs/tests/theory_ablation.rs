use ddar::corpus::{parse_problem, read_corpus, translate};
use ddar::human::{self, Opts};
use ddar::theory::{audit, detect, Forbid, Kind, Thm, ALL};
use std::time::{Duration, Instant};

const JGEX: &str = include_str!("../../corpus/jgex_ag_231.txt");
const IMO: &str = include_str!("../../corpus/imo_ag_30.txt");

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<T: Copy>(&mut self, v: &[T]) -> T {
        v[(self.next() % v.len() as u64) as usize]
    }
}

fn banned_proof_is_clean(name: &str, text: &str, forbid: Forbid) -> Option<bool> {
    let t = parse_problem(name, text)
        .and_then(|p| translate(&p, 1, 200))
        .ok()?;
    let mut problem = t.problem;
    problem.forbid = forbid;
    let (_, mut trace, deps) = ddar::runner::solve_problem_with_trace(&problem).ok()??;
    let raw = audit::raw(&trace.facts, &deps, forbid);
    assert!(raw.is_empty(), "{name}: raw proof cites {raw:?}");
    let goal = problem.goal.clone().unwrap();
    let opts = Opts {
        deadline: Some(Instant::now() + Duration::from_secs(20)),
        strict: false,
    };
    let hp = human::write(&trace, &goal, &deps, &[], &opts);
    if hp.available {
        let v = human::verify(&trace, &goal, &deps, &[], &hp);
        assert!(v.is_empty(), "{name}: checker violations {v:?}");
        trace.forbid = forbid;
        let structural = audit::human(&trace, &hp);
        assert!(
            structural.is_empty(),
            "{name}: human proof cites {structural:?}"
        );
        let text = human::render_en(&trace, &hp, &[]);
        let hits = audit::text(forbid, &text);
        assert!(
            hits.is_empty(),
            "{name}: human proof names {hits:?}\n{text}"
        );
    }
    Some(true)
}

#[test]
fn a_random_ban_never_reaches_the_raw_or_the_human_proof() {
    let problems = read_corpus(JGEX).unwrap();
    let bannable: Vec<Thm> = ALL
        .iter()
        .copied()
        .filter(|t| t.avoidable() && t.info().kind != Kind::Calculus)
        .collect();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut proved = 0;
    for _ in 0..48 {
        let (name, text) = &problems[(rng.next() % problems.len() as u64) as usize];
        let thm = rng.pick(&bannable);
        if banned_proof_is_clean(name, text, Forbid::closure_of(&[thm])).is_some() {
            proved += 1;
        }
    }
    assert!(
        proved >= 12,
        "only {proved} of 48 banned runs proved anything"
    );
}

#[test]
fn banning_every_named_theorem_used_by_a_proof_still_leaves_a_clean_proof_or_none() {
    let problems = read_corpus(JGEX).unwrap();
    for (name, text) in problems.iter().take(30) {
        let Ok(t) = parse_problem(name, text).and_then(|p| translate(&p, 1, 200)) else {
            continue;
        };
        let Ok(Some((_, trace, deps))) = ddar::runner::solve_problem_with_trace(&t.problem) else {
            continue;
        };
        let mut used: Vec<Thm> = Vec::new();
        for &f in &deps {
            if let Some(fact) = trace.facts.get(f as usize) {
                for th in ddar::theory::of_fact(fact) {
                    if th.avoidable() && th.info().kind == Kind::Theorem && !used.contains(&th) {
                        used.push(th);
                    }
                }
            }
        }
        for th in used {
            banned_proof_is_clean(name, text, Forbid::closure_of(&[th]));
        }
    }
}

#[test]
fn no_imo_problem_states_a_library_theorem() {
    let deadline_each = Duration::from_millis(400);
    for (name, text) in read_corpus(IMO).unwrap() {
        let t = parse_problem(&name, &text)
            .and_then(|p| translate(&p, 1, 200))
            .unwrap();
        let started = Instant::now();
        let g = detect::guard(&t.problem);
        assert!(!g.fired(), "{name}: detected {:?}", g.detected);
        assert!(
            started.elapsed() < deadline_each * 3,
            "{name}: detection took {:?}",
            started.elapsed()
        );
    }
}
