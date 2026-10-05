use super::model::*;
use super::text::{deg, expr as expr_text, FnNames, Names};
use super::trace::EngineTrace;
use crate::predicate::PointId;
use crate::proof::FactId;
use serde_json::{json, Value};

pub struct Namer<'a> {
    pub name: &'a dyn Fn(PointId) -> String,
    pub step: &'a dyn Fn(FactId) -> Option<usize>,
}

fn names_of(nm: &Namer, ps: &[PointId]) -> Vec<String> {
    ps.iter().map(|&p| (nm.name)(p)).collect()
}

fn seg(nm: &Namer, l: (PointId, PointId)) -> String {
    format!("{}{}", (nm.name)(l.0), (nm.name)(l.1))
}

pub fn term(nm: &Namer, e: &Expr) -> String {
    expr_text(&FnNames(nm.name), e)
}

pub fn stmt(nm: &Namer, s: &Stmt) -> Value {
    let (kind, args, points): (&str, Vec<String>, Vec<PointId>) = match s {
        Stmt::Coll { pts } => ("coll", names_of(nm, pts), pts.clone()),
        Stmt::Cyclic { pts } => ("cyclic", names_of(nm, pts), pts.clone()),
        Stmt::Perp { l1, l2 } => ("perp", vec![seg(nm, *l1), seg(nm, *l2)], vec![l1.0, l1.1, l2.0, l2.1]),
        Stmt::Para { l1, l2 } => ("para", vec![seg(nm, *l1), seg(nm, *l2)], vec![l1.0, l1.1, l2.0, l2.1]),
        Stmt::EqAngle { lhs, rhs } => ("eqangle", vec![term(nm, lhs), term(nm, rhs)], expr_points(lhs).into_iter().chain(expr_points(rhs)).collect()),
        Stmt::AngleConst { angle, degrees } => ("aconst", vec![term(nm, angle), degrees.to_string()], expr_points(angle)),
        Stmt::Cong { s1, s2 } => ("cong", vec![seg(nm, *s1), seg(nm, *s2)], vec![s1.0, s1.1, s2.0, s2.1]),
        Stmt::EqRatio { segs } => ("eqratio", segs.iter().map(|x| seg(nm, *x)).collect(), segs.iter().flat_map(|x| [x.0, x.1]).collect()),
        Stmt::RatioConst { s1, s2, value } => ("rconst", vec![seg(nm, *s1), seg(nm, *s2), value.to_string()], vec![s1.0, s1.1, s2.0, s2.1]),
        Stmt::Sim { t1, t2, .. } => (
            "simtri",
            vec![names_of(nm, &[t1.0, t1.1, t1.2]).join(""), names_of(nm, &[t2.0, t2.1, t2.2]).join("")],
            vec![t1.0, t1.1, t1.2, t2.0, t2.1, t2.2],
        ),
        Stmt::Congruent { t1, t2, .. } => (
            "contri",
            vec![names_of(nm, &[t1.0, t1.1, t1.2]).join(""), names_of(nm, &[t2.0, t2.1, t2.2]).join("")],
            vec![t1.0, t1.1, t1.2, t2.0, t2.1, t2.2],
        ),
        Stmt::OnCircle { p, circle } => ("oncircle", vec![(nm.name)(*p), names_of(nm, circle).join("")], std::iter::once(*p).chain(circle.iter().copied()).collect()),
        Stmt::Tangent { p, line, circle } => ("tangent", vec![seg(nm, *line), (nm.name)(*p), names_of(nm, circle).join("")], vec![*p, line.0, line.1]),
        Stmt::RadicalAxis { x, u, v } => ("radical_axis", names_of(nm, &[*x, *u, *v]), vec![*x, *u, *v]),
        Stmt::Coincide { a, b } => ("coincide", names_of(nm, &[*a, *b]), vec![*a, *b]),
        Stmt::Formula { text, pts } => {
            let mut out = text.clone();
            for (i, &p) in pts.iter().enumerate().rev() {
                out = out.replace(&format!("{{{i}}}"), &(nm.name)(p));
            }
            ("formula", vec![out], pts.clone())
        }
        Stmt::Eq { lhs, rhs } => ("eq", vec![term(nm, lhs), term(nm, rhs)], expr_points(lhs).into_iter().chain(expr_points(rhs)).collect()),
    };
    let mut pts = points;
    pts.sort_unstable();
    pts.dedup();
    json!({"kind": kind, "args": args, "points": names_of(nm, &pts)})
}

pub fn expr_points(e: &Expr) -> Vec<PointId> {
    match e {
        Expr::Angle { a, b, c, .. } => vec![*a, *b, *c],
        Expr::LineAngle { l1, l2, .. } => vec![l1.0, l1.1, l2.0, l2.1],
        Expr::Lin { terms } => terms.iter().flat_map(|(_, x)| expr_points(x)).collect(),
        Expr::Seg { a, b } => vec![*a, *b],
        Expr::Prod { factors } => factors.iter().flat_map(|(x, _)| expr_points(x)).collect(),
        Expr::Sin { angle } | Expr::Cos { angle } => expr_points(angle),
        Expr::Const { .. } | Expr::Num { .. } => Vec::new(),
    }
}

pub fn reason(nm: &Namer, r: &Reason) -> Value {
    match r {
        Reason::Hyp { stmt: s, fact } => json!({"kind": "hyp", "stmt": stmt(nm, s), "step": (nm.step)(*fact)}),
        Reason::Claim { n, block, fact } => json!({"kind": "claim", "n": n, "block": block, "step": (nm.step)(*fact)}),
        Reason::Atom { key, stmt: s, args, from } => json!({"kind": "atom", "key": key.as_str(), "args": names_of(nm, args), "stmt": stmt(nm, s), "from": from}),
        Reason::Fact { stmt: s, fact, block, because } => {
            let mut v = stmt(nm, s);
            v["step"] = json!((nm.step)(*fact));
            v["block"] = json!(block);
            v["because"] = Value::Array(because.iter().map(|b| reason(nm, b)).collect());
            v
        }
        Reason::Engine { fact } => json!({"kind": "engine", "step": (nm.step)(*fact)}),
    }
}

fn combination(c: &[Term]) -> Value {
    Value::Array(c.iter().map(|t| json!({"reason": t.reason, "row": t.row, "coef": t.coef.to_string()})).collect())
}

fn link(nm: &Namer, l: &Link) -> Value {
    json!({"reasons": l.reasons.iter().map(|r| reason(nm, r)).collect::<Vec<_>>(), "combination": combination(&l.combination)})
}

pub fn sentence(nm: &Namer, s: &Sentence) -> Value {
    match s {
        Sentence::Chain { terms, links, then, directed } => json!({
            "kind": "chain",
            "directed": directed,
            "terms": terms.iter().map(|e| term(nm, e)).collect::<Vec<_>>(),
            "links": links.iter().map(|l| link(nm, l)).collect::<Vec<_>>(),
            "then": then.as_ref().map(|t| stmt(nm, t)),
        }),
        Sentence::Because { stmt: s, reasons, combination: c } => json!({
            "kind": "because",
            "stmt": stmt(nm, s),
            "reasons": reasons.iter().map(|r| reason(nm, r)).collect::<Vec<_>>(),
            "combination": combination(c),
        }),
        Sentence::Pooled { stmt: s, reasons, combination: c } => json!({
            "kind": "pooled",
            "stmt": stmt(nm, s),
            "reasons": reasons.iter().map(|r| reason(nm, r)).collect::<Vec<_>>(),
            "combination": combination(c),
        }),
        Sentence::Theorem { key, stmt: s, reasons } => json!({
            "kind": "theorem",
            "key": key.as_str(),
            "stmt": stmt(nm, s),
            "reasons": reasons.iter().map(|r| reason(nm, r)).collect::<Vec<_>>(),
        }),
        Sentence::Computation { comp, terms, links } => json!({
            "kind": "computation",
            "comp": comp,
            "terms": terms.iter().map(|e| term(nm, e)).collect::<Vec<_>>(),
            "links": links.iter().map(|l| link(nm, l)).collect::<Vec<_>>(),
        }),
        Sentence::Raw { engine_fact, cites } => json!({
            "kind": "raw",
            "step": (nm.step)(*engine_fact),
            "cites": cites.iter().filter_map(|c| (nm.step)(*c)).collect::<Vec<_>>(),
        }),
    }
}

pub fn setup(nm: &Namer, s: &SetupLine) -> Value {
    match s {
        SetupLine::DirectedAngles => json!({"kind": "directed_angles"}),
        SetupLine::Circle { name, through, centre, diameter } => json!({
            "kind": "circle",
            "name": name,
            "through": names_of(nm, through),
            "centre": centre.map(|c| (nm.name)(c)),
            "diameter": diameter.map(|d| vec![(nm.name)(d.0), (nm.name)(d.1)]),
        }),
        SetupLine::Aux { point, aux_index } => json!({"kind": "aux", "point": (nm.name)(*point), "aux_index": aux_index}),
        SetupLine::Helper { point, meaning } => {
            let m = match meaning {
                HelperMeaning::Midpoint { of } => json!({"kind": "midpoint", "of": [(nm.name)(of.0), (nm.name)(of.1)]}),
                HelperMeaning::Reflection { of, line } => json!({"kind": "reflection", "of": (nm.name)(*of), "line": [(nm.name)(line.0), (nm.name)(line.1)]}),
                HelperMeaning::Point => json!({"kind": "point"}),
            };
            json!({"kind": "helper", "point": (nm.name)(*point), "meaning": m})
        }
    }
}

pub fn block(nm: &Namer, b: &Block) -> Value {
    let kind = match b.kind {
        BlockKind::Claim(n) => json!({"kind": "claim", "n": n}),
        BlockKind::Step => json!({"kind": "step"}),
        BlockKind::Conclusion => json!({"kind": "conclusion"}),
        BlockKind::Raw => json!({"kind": "raw"}),
    };
    let mut steps: Vec<usize> = b.engine_facts.iter().filter_map(|&f| (nm.step)(f)).collect();
    steps.sort_unstable();
    steps.dedup();
    json!({
        "id": b.id,
        "kind": kind["kind"],
        "n": kind.get("n"),
        "stmt": stmt(nm, &b.stmt),
        "body": b.body.iter().map(|s| sentence(nm, s)).collect::<Vec<_>>(),
        "engine_steps": steps,
        "points": names_of(nm, &b.points),
        "objects": b.objects.iter().map(|o| match o {
            ObjRef::Circle { through } => json!({"circle": names_of(nm, through)}),
            ObjRef::Line { through } => json!({"line": names_of(nm, through)}),
        }).collect::<Vec<_>>(),
    })
}

pub fn to_json(hp: &HumanProof, nm: &Namer) -> Value {
    if !hp.available {
        return Value::Null;
    }
    json!({
        "version": hp.version,
        "as_drawn": hp.as_drawn,
        "setup": hp.setup.iter().map(|s| setup(nm, s)).collect::<Vec<_>>(),
        "blocks": hp.blocks.iter().map(|b| block(nm, b)).collect::<Vec<_>>(),
        "metrics": serde_json::to_value(&hp.metrics).unwrap_or(Value::Null),
    })
}

pub fn engine_json(t: &EngineTrace, hp: &HumanProof, closure: &[FactId]) -> Value {
    let names = Names::new(t, &hp.setup);
    let name = |p: PointId| names.get(p);
    let step = |f: FactId| closure.iter().position(|&x| x == f).map(|i| i + 1);
    let nm = Namer { name: &name, step: &step };
    let _ = deg;
    to_json(hp, &nm)
}
