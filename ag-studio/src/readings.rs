use ddar::human::{AtomKey, Expr, HumanProof, Reason, Sentence, Stmt};
use ddar::Problem;

use crate::engine::Solution;

pub const ALTERNATIVES: usize = 12;

fn xy(fig: &Problem, names: &[String], id: u32) -> Option<(f64, f64)> {
    let n = names.get(id as usize)?;
    fig.points
        .iter()
        .find(|p| &p.name == n)
        .map(|p| (p.value.x, p.value.y))
}

fn dir(p: (f64, f64), q: (f64, f64)) -> f64 {
    (q.1 - p.1).atan2(q.0 - p.0).to_degrees()
}

fn lcm(a: i64, b: i64) -> i64 {
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    a / x * b
}

pub fn eval(e: &Expr, at: &dyn Fn(u32) -> Option<(f64, f64)>) -> Option<(f64, i64)> {
    match e {
        Expr::Angle { a, b, c, directed } => {
            let (pa, pb, pc) = (at(*a)?, at(*b)?, at(*c)?);
            if *directed {
                Some(((dir(pb, pc) - dir(pb, pa)).rem_euclid(180.0), 1))
            } else {
                let (u, w) = ((pa.0 - pb.0, pa.1 - pb.1), (pc.0 - pb.0, pc.1 - pb.1));
                Some((
                    (u.0 * w.1 - u.1 * w.0)
                        .abs()
                        .atan2(u.0 * w.0 + u.1 * w.1)
                        .to_degrees(),
                    0,
                ))
            }
        }
        Expr::LineAngle { l1, l2, .. } => Some((
            (dir(at(l2.0)?, at(l2.1)?) - dir(at(l1.0)?, at(l1.1)?)).rem_euclid(180.0),
            1,
        )),
        Expr::Const { degrees } => Some((degrees.to_f64(), 0)),
        Expr::Lin { terms } => {
            let mut v = 0.0;
            let mut div = 0;
            for (k, x) in terms {
                let (xv, xd) = eval(x, at)?;
                v += k.to_f64() * xv;
                if xd > 0 {
                    let d = xd * k.denom_i64().unwrap_or(1).abs();
                    div = if div == 0 { d } else { lcm(div, d) };
                }
            }
            Some((v, div))
        }
        _ => None,
    }
}

pub const ANGLE_TOLERANCE_DEG: f64 = 5e-5;

pub fn holds(l: (f64, i64), r: (f64, i64)) -> bool {
    let div = match (l.1, r.1) {
        (0, 0) => return (l.0 - r.0).abs() < ANGLE_TOLERANCE_DEG,
        (0, d) | (d, 0) => d,
        (a, b) => lcm(a, b),
    };
    let m = 180.0 / div as f64;
    let d = (l.0 - r.0).rem_euclid(m);
    d < ANGLE_TOLERANCE_DEG || d > m - ANGLE_TOLERANCE_DEG
}

pub fn stmts_of_reason<'a>(r: &'a Reason, out: &mut Vec<&'a Stmt>) {
    match r {
        Reason::Hyp { stmt, .. } | Reason::Atom { stmt, .. } | Reason::Lemma { stmt, .. } => {
            out.push(stmt)
        }
        Reason::Fact { stmt, because, .. } => {
            out.push(stmt);
            for b in because {
                stmts_of_reason(b, out);
            }
        }
        Reason::Claim { .. } | Reason::Engine { .. } => {}
    }
}

fn reasons_of(s: &Sentence) -> Vec<&Reason> {
    match s {
        Sentence::Chain { links, .. } | Sentence::Computation { links, .. } => {
            links.iter().flat_map(|l| &l.reasons).collect()
        }
        Sentence::Because { reasons, .. }
        | Sentence::Pooled { reasons, .. }
        | Sentence::Theorem { reasons, .. } => reasons.iter().collect(),
        Sentence::Raw { .. } => Vec::new(),
    }
}

fn side(fig: &Problem, names: &[String], args: &[u32]) -> Option<bool> {
    let [p, q, x, y] = args else { return None };
    let (pp, pq, px, py) = (
        xy(fig, names, *p)?,
        xy(fig, names, *q)?,
        xy(fig, names, *x)?,
        xy(fig, names, *y)?,
    );
    let o = |a: (f64, f64)| (py.0 - px.0) * (a.1 - px.1) - (py.1 - px.1) * (a.0 - px.0);
    let (a, b) = (o(pp), o(pq));
    let scale = ((py.0 - px.0).powi(2) + (py.1 - px.1).powi(2)).sqrt();
    (a.abs() > 1e-9 * scale && b.abs() > 1e-9 * scale).then_some(a.signum() == b.signum())
}

fn on_segment(fig: &Problem, names: &[String], v: u32, inner: u32, outer: u32) -> Option<bool> {
    let (pv, pi, po) = (
        xy(fig, names, v)?,
        xy(fig, names, inner)?,
        xy(fig, names, outer)?,
    );
    let (a, b) = ((pi.0 - pv.0, pi.1 - pv.1), (po.0 - pv.0, po.1 - pv.1));
    Some(a.0 * b.0 + a.1 * b.1 > 0.0 && a.0.hypot(a.1) < b.0.hypot(b.1))
}

pub fn violations(hp: &HumanProof, names: &[String], fig: &Problem) -> Vec<String> {
    let at = |id: u32| xy(fig, names, id);
    let show = |e: &Expr| format!("{e:?}");
    let mut bad = Vec::new();
    let mut check = |l: &Expr, r: &Expr, what: &str| {
        if let (Some(a), Some(b)) = (eval(l, &at), eval(r, &at)) {
            if !holds(a, b) {
                bad.push(format!(
                    "{what}: {} = {} reads {:.6}° vs {:.6}°",
                    show(l),
                    show(r),
                    a.0,
                    b.0
                ));
            }
        }
    };
    let mut stmts: Vec<&Stmt> = Vec::new();
    let mut chains: Vec<&Vec<Expr>> = Vec::new();
    let mut labels: Vec<(AtomKey, &Vec<u32>)> = Vec::new();
    for b in &hp.blocks {
        stmts.push(&b.stmt);
        for s in &b.body {
            match s {
                Sentence::Chain { terms, then, .. } => {
                    chains.push(terms);
                    stmts.extend(then.iter());
                }
                Sentence::Computation { terms, .. } => chains.push(terms),
                Sentence::Because { stmt, .. }
                | Sentence::Pooled { stmt, .. }
                | Sentence::Theorem { stmt, .. } => stmts.push(stmt),
                Sentence::Raw { .. } => {}
            }
            for r in reasons_of(s) {
                stmts_of_reason(r, &mut stmts);
                if let Reason::Atom { key, args, .. } = r {
                    labels.push((*key, args));
                }
            }
        }
    }
    for terms in chains {
        for w in terms.windows(2) {
            check(&w[0], &w[1], "chain link");
        }
    }
    for s in stmts {
        match s {
            Stmt::EqAngle { lhs, rhs } | Stmt::Eq { lhs, rhs } => check(lhs, rhs, "statement"),
            Stmt::AngleConst { angle, degrees } => check(
                angle,
                &Expr::Const {
                    degrees: degrees.clone(),
                },
                "statement",
            ),
            _ => {}
        }
    }
    for (key, args) in labels {
        let ok = match key {
            AtomKey::SameSide => side(fig, names, args),
            AtomKey::OppositeSides => side(fig, names, args).map(|s| !s),
            AtomKey::OnSegment if args.len() == 3 => {
                on_segment(fig, names, args[0], args[1], args[2])
            }
            _ => None,
        };
        if ok == Some(false) {
            bad.push(format!("{key:?} {args:?} does not hold in the figure"));
        }
    }
    bad
}

fn reads_angles(hp: &HumanProof) -> bool {
    let undirected = |e: &Expr| match e {
        Expr::Angle { directed, .. } | Expr::LineAngle { directed, .. } => !directed,
        Expr::Lin { terms } => terms.iter().any(|(_, x)| {
            matches!(
                x,
                Expr::Angle {
                    directed: false,
                    ..
                }
            )
        }),
        _ => false,
    };
    hp.blocks.iter().flat_map(|b| &b.body).any(|s| match s {
        Sentence::Chain { terms, .. } => terms.iter().any(undirected),
        _ => reasons_of(s).iter().any(|r| {
            matches!(r, Reason::Atom { key, .. } if matches!(key, AtomKey::SameSide | AtomKey::OppositeSides | AtomKey::OnSegment))
        }),
    })
}

pub fn depends_on_configuration(sol: &Solution) -> bool {
    let (Some(h), Some(fig)) = (&sol.human, &sol.figure) else {
        return false;
    };
    if !sol.proved || !h.proof.available || !reads_angles(&h.proof) {
        return false;
    }
    let others = crate::spread::alternatives(&fig.problem, fig.aux_from, &sol.input, ALTERNATIVES);
    if others.is_empty() {
        return true;
    }
    others
        .iter()
        .any(|alt| !violations(&h.proof, &h.names, alt).is_empty())
}
