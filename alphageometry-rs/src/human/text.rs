use super::model::*;
use super::trace::EngineTrace;
use crate::predicate::PointId;
use crate::proof::{FactId, Reason as ER};
use crate::rational::Rat;
use std::collections::BTreeMap;

pub struct Names {
    map: BTreeMap<PointId, String>,
}

fn subscript(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => char::from_u32(0x2080 + c.to_digit(10).unwrap()).unwrap(),
            c => c,
        })
        .collect()
}

pub fn disp(raw: &str) -> String {
    if raw.is_empty() {
        return "?".into();
    }
    let lead: String = raw.chars().take_while(|c| c.is_alphabetic()).collect();
    let rest = &raw[lead.len()..];
    let mut head = String::new();
    let mut chars = lead.chars();
    if let Some(c) = chars.next() {
        head.extend(c.to_uppercase());
    }
    let tail: String = chars.collect();
    if lead.chars().all(|c| c.is_lowercase()) {
        head.push_str(&tail);
    } else {
        head.push_str(&tail);
    }
    if rest.chars().all(|c| c.is_ascii_digit()) {
        format!("{head}{}", subscript(rest))
    } else {
        format!("{head}{rest}")
    }
}

pub trait PointNames {
    fn get(&self, p: PointId) -> String;
    fn pts(&self, ps: &[PointId]) -> String {
        ps.iter().map(|&p| self.get(p)).collect::<Vec<_>>().join("")
    }
    fn list(&self, ps: &[PointId]) -> String {
        ps.iter().map(|&p| self.get(p)).collect::<Vec<_>>().join(", ")
    }
}

pub struct FnNames<'a>(pub &'a dyn Fn(PointId) -> String);

impl PointNames for FnNames<'_> {
    fn get(&self, p: PointId) -> String {
        (self.0)(p)
    }
}

impl PointNames for Names {
    fn get(&self, p: PointId) -> String {
        Names::get(self, p)
    }
}

impl Names {
    pub fn new(t: &EngineTrace, setup: &[SetupLine]) -> Names {
        let mut map = BTreeMap::new();
        let mut used: Vec<String> = Vec::new();
        for (i, n) in t.names.iter().enumerate() {
            if !n.starts_with('_') {
                let d = disp(n);
                used.push(d.clone());
                map.insert(i as PointId, d);
            }
        }
        let mut generic = 0;
        for s in setup {
            if let SetupLine::Helper { point, meaning } = s {
                let cand: Vec<String> = match meaning {
                    HelperMeaning::Midpoint { .. } => ["M", "N", "K", "L"].iter().map(|x| x.to_string()).collect(),
                    HelperMeaning::Reflection { of, .. } => vec![format!("{}′", map.get(of).cloned().unwrap_or_else(|| disp(t.name(*of))))],
                    HelperMeaning::Point => Vec::new(),
                };
                let name = cand.into_iter().find(|c| !used.contains(c)).unwrap_or_else(|| loop {
                    generic += 1;
                    let c = format!("P{}", subscript(&generic.to_string()));
                    if !used.contains(&c) {
                        break c;
                    }
                });
                used.push(name.clone());
                map.insert(*point, name);
            }
        }
        for (i, n) in t.names.iter().enumerate() {
            map.entry(i as PointId).or_insert_with(|| {
                if n.starts_with('_') {
                    loop {
                        generic += 1;
                        let c = format!("P{}", subscript(&generic.to_string()));
                        if !used.contains(&c) {
                            used.push(c.clone());
                            break c;
                        }
                    }
                } else {
                    disp(n)
                }
            });
        }
        Names { map }
    }

    pub fn get(&self, p: PointId) -> String {
        self.map.get(&p).cloned().unwrap_or_else(|| format!("P{p}"))
    }

}

pub fn deg(r: &Rat) -> String {
    if r.is_integer() {
        format!("{}°", r)
    } else {
        format!("{:.1}°", r.to_f64())
    }
}

fn rat_text(r: &Rat) -> String {
    if r.is_integer() {
        r.to_string()
    } else if *r == Rat::new(1, 2) {
        "½".into()
    } else {
        r.to_string()
    }
}

pub fn expr(n: &dyn PointNames, e: &Expr) -> String {
    match e {
        Expr::Angle { a, b, c, directed } => format!("{}{}", if *directed { "∡" } else { "∠" }, n.pts(&[*a, *b, *c])),
        Expr::LineAngle { l1, l2, directed } => format!("{}({}, {})", if *directed { "∡" } else { "∠" }, n.pts(&[l1.0, l1.1]), n.pts(&[l2.0, l2.1])),
        Expr::Const { degrees } => deg(degrees),
        Expr::Lin { terms } => {
            let mut out = String::new();
            for (i, (k, x)) in terms.iter().enumerate() {
                let (neg, body) = match x {
                    Expr::Const { degrees } => {
                        let v = degrees * k;
                        (v.is_negative(), deg(&v.abs()))
                    }
                    _ => {
                        let mag = k.abs();
                        (
                            k.is_negative(),
                            if mag.is_one() {
                                expr(n, x)
                            } else {
                                if mag.is_integer() {
                                    format!("{}{}", rat_text(&mag), expr(n, x))
                                } else {
                                    format!("{}·{}", rat_text(&mag), expr(n, x))
                                }
                            },
                        )
                    }
                };
                if i == 0 {
                    out.push_str(if neg { "−" } else { "" });
                } else {
                    out.push_str(if neg { " − " } else { " + " });
                }
                out.push_str(&body);
            }
            out
        }
        Expr::Seg { a, b } => n.pts(&[*a, *b]),
        Expr::Sq { a, b } => format!("{}²", n.pts(&[*a, *b])),
        Expr::Prod { factors } => {
            let num: Vec<String> = factors.iter().filter(|(_, k)| *k > 0).map(|(x, k)| pow(n, x, *k)).collect();
            let den: Vec<String> = factors.iter().filter(|(_, k)| *k < 0).map(|(x, k)| pow(n, x, -*k)).collect();
            let num_s = if num.is_empty() { "1".to_string() } else { num.join("·") };
            if den.is_empty() {
                num_s
            } else if den.len() == 1 {
                format!("{num_s} / {}", den[0])
            } else {
                format!("{num_s} / ({})", den.join("·"))
            }
        }
        Expr::Sin { angle } => format!("sin{}", expr(n, angle)),
        Expr::Cos { angle } => format!("cos{}", expr(n, angle)),
        Expr::Num { value } => value.to_string(),
    }
}

fn pow(n: &dyn PointNames, x: &Expr, k: i32) -> String {
    let base = expr(n, x);
    match k {
        1 => base,
        2 => format!("{base}²"),
        3 => format!("{base}³"),
        k => format!("{base}^{k}"),
    }
}

pub fn stmt(n: &dyn PointNames, s: &Stmt) -> String {
    match s {
        Stmt::Coll { pts } => format!("{} are collinear", n.list(pts)),
        Stmt::Cyclic { pts } => format!("{} are concyclic", n.list(pts)),
        Stmt::Perp { l1, l2 } => format!("{} ⟂ {}", n.pts(&[l1.0, l1.1]), n.pts(&[l2.0, l2.1])),
        Stmt::Para { l1, l2 } => format!("{} ∥ {}", n.pts(&[l1.0, l1.1]), n.pts(&[l2.0, l2.1])),
        Stmt::EqAngle { lhs, rhs } => format!("{} = {}", expr(n, lhs), expr(n, rhs)),
        Stmt::AngleConst { angle, degrees } => format!("{} = {}", expr(n, angle), deg(degrees)),
        Stmt::Cong { s1, s2 } => format!("{} = {}", n.pts(&[s1.0, s1.1]), n.pts(&[s2.0, s2.1])),
        Stmt::EqRatio { segs } => segs.chunks(2).map(|c| format!("{} : {}", n.pts(&[c[0].0, c[0].1]), c.get(1).map(|x| n.pts(&[x.0, x.1])).unwrap_or_default())).collect::<Vec<_>>().join(" = "),
        Stmt::RatioConst { s1, s2, value } => format!("{} = {}·{}", n.pts(&[s1.0, s1.1]), value, n.pts(&[s2.0, s2.1])),
        Stmt::Sim { t1, t2, .. } => format!("△{} ∼ △{}", n.pts(&[t1.0, t1.1, t1.2]), n.pts(&[t2.0, t2.1, t2.2])),
        Stmt::Congruent { t1, t2, .. } => format!("△{} ≅ △{}", n.pts(&[t1.0, t1.1, t1.2]), n.pts(&[t2.0, t2.1, t2.2])),
        Stmt::OnCircle { p, circle } => format!("{} lies on ({})", n.get(*p), n.pts(circle)),
        Stmt::Tangent { p, line, circle } => format!("{} is tangent at {} to the circle centred at {}", n.pts(&[line.0, line.1]), n.get(*p), n.pts(circle)),
        Stmt::RadicalAxis { x, u, v } => format!("{} lies on the radical axis {}", n.get(*x), n.pts(&[*u, *v])),
        Stmt::Coincide { a, b } => format!("{} coincides with {}", n.get(*a), n.get(*b)),
        Stmt::Formula { text, pts } => {
            let mut out = text.clone();
            for (i, &p) in pts.iter().enumerate().rev() {
                out = out.replace(&format!("{{{i}}}"), &n.get(p));
            }
            out
        }
        Stmt::Eq { lhs, rhs } => format!("{} = {}", expr(n, lhs), expr(n, rhs)),
    }
}

pub fn hyp_text(n: &dyn PointNames, s: &Stmt) -> String {
    if let Stmt::EqAngle { lhs: Expr::Angle { a, b, c, .. }, rhs: Expr::Angle { a: a2, b: b2, c: c2, .. } } = s {
        if b == b2 && c == a2 {
            return format!("{} bisects ∠{}", n.pts(&[*b, *c]), n.pts(&[*a, *b, *c2]));
        }
    }
    stmt(n, s)
}

pub fn reason(n: &dyn PointNames, r: &Reason, claims: &BTreeMap<u16, u16>) -> String {
    let _ = claims;
    match r {
        Reason::Hyp { stmt: s, .. } => hyp_text(n, s),
        Reason::Claim { n: k, .. } => format!("Claim {k}"),
        Reason::Atom { key, args, stmt: s, from } => {
            let base = atom_text(n, *key, args, s);
            let f: Vec<String> = from.iter().filter_map(|b| claims.get(b).map(|k| format!("Claim {k}"))).collect();
            if f.is_empty() {
                base
            } else {
                format!("{base} ({})", f.join(", "))
            }
        }
        Reason::Fact { stmt: s, block, because, .. } => {
            let base = match s {
                Stmt::Coll { pts } if pts.len() >= 3 && block.is_some() => format!("{} on {}", n.get(pts[pts.len() - 1]), n.pts(&pts[..2])),
                _ => stmt(n, s),
            };
            let mut inner: Vec<String> = Vec::new();
            for b in because {
                let t = match b {
                    Reason::Atom { stmt: s2, from, .. } if same_stmt(s2, s) => {
                        let f: Vec<String> = from.iter().filter_map(|x| claims.get(x).map(|k| format!("Claim {k}"))).collect();
                        if f.is_empty() {
                            continue;
                        }
                        f.join(", ")
                    }
                    Reason::Fact { stmt: s2, because: bb, .. } if same_stmt(s2, s) && bb.is_empty() => continue,
                    _ => reason(n, b, claims),
                };
                if !inner.contains(&t) {
                    inner.push(t);
                }
            }
            if inner.is_empty() {
                base
            } else {
                format!("{base} ({})", inner.join(", "))
            }
        }
        Reason::Engine { fact } => format!("step {}", fact + 1),
    }
}

pub fn atom_text(n: &dyn PointNames, key: AtomKey, a: &[PointId], s: &Stmt) -> String {
    match key {
        AtomKey::Inscribed => {
            if a.len() > 4 {
                format!("the inscribed angles in ({})", n.pts(a))
            } else {
                format!("{} cyclic", n.pts(a))
            }
        }
        AtomKey::Thales => format!("{} is a diameter", n.pts(&[a[0], a[1]])),
        AtomKey::TangentChord => format!("{} is tangent to the circle centred at {}", n.pts(&[a[0], a[1]]), n.get(a[4])),
        AtomKey::PerpBisector => format!("{} is the perpendicular bisector of {}", n.pts(&[a[0], a[1]]), n.pts(&[a[2], a[3]])),
        AtomKey::Parallel => format!("{} ∥ {}, both ⟂ {}", n.pts(&[a[0], a[1]]), n.pts(&[a[2], a[3]]), n.pts(&[a[4], a[5]])),
        AtomKey::Radii | AtomKey::Isosceles => stmt(n, s),
        AtomKey::CentralAngle => format!("central angle ∠{} = 2∠{}", n.pts(&[a[1], a[0], a[2]]), n.pts(&[a[1], a[3], a[2]])),
        AtomKey::PowerOfPoint => {
            let _ = s;
            format!("power of {} with respect to circle ({})", n.get(a[0]), n.pts(&a[1..]))
        }
        AtomKey::Midline => format!("midline {} ∥ {}", n.pts(&[a[0], a[1]]), n.pts(&[a[2], a[3]])),
        AtomKey::Orthocentre => format!("{} ⟂ {}, the altitudes concur", n.pts(&[a[3], a[0]]), n.pts(&[a[1], a[2]])),
    }
}

pub fn theorem_name(k: TheoremKey) -> &'static str {
    match k {
        TheoremKey::RadicalAxis => "the radical axis theorem",
        TheoremKey::ArcChord => "equal arcs and chords",
        TheoremKey::AngleBisectorThm => "the angle bisector theorem",
        TheoremKey::AngleBisectorThmConverse => "the converse of the angle bisector theorem",
        TheoremKey::Intercept => "the intercept theorem",
        TheoremKey::Homothety => "the homothety at a centre of similitude",
        TheoremKey::Monge => "Monge–d'Alembert",
        TheoremKey::Menelaus => "Menelaus' theorem",
        TheoremKey::MenelausConverse => "the converse of Menelaus' theorem",
        TheoremKey::CevaConverse => "the converse of Ceva's theorem",
        TheoremKey::BisectorConcurrency => "the concurrency of angle bisectors",
        TheoremKey::TriangleEquality => "the equality case of the triangle inequality",
        TheoremKey::Pythagoras => "Pythagoras",
        TheoremKey::PerpFromSquares => "squared lengths",
        TheoremKey::SquaresOfRatio => "squares of proportional lengths",
        TheoremKey::Stewart => "Stewart's theorem",
        TheoremKey::LengthsFromSquares => "lengths from squared lengths",
        TheoremKey::LawOfSines => "the law of sines",
        TheoremKey::EqualSines => "equal sines",
        TheoremKey::DoubleAngle => "the double-angle formula",
        TheoremKey::TripleAngle => "the triple-angle formula",
        TheoremKey::SineConst => "a known sine",
        TheoremKey::SinesConverse => "the converse of the law of sines",
        TheoremKey::PointMerge => "uniqueness of the intersection",
        TheoremKey::TangentMerge => "tangency",
        TheoremKey::Congruence => "congruence",
        TheoremKey::Similarity => "similarity",
        TheoremKey::Collinear => "collinearity",
        TheoremKey::Concyclic => "concyclicity",
        TheoremKey::Other => "a classical rule",
    }
}

pub fn aux_text(n: &dyn PointNames, t: &EngineTrace, point: PointId, desc: &str) -> String {
    let me = n.get(point);
    let open = desc.find('(');
    let (kind, args) = match open {
        Some(i) => (&desc[..i], desc[i + 1..].trim_end_matches(')')),
        None => (desc, ""),
    };
    let tok = |s: &str| -> String {
        let s = s.trim();
        if let Some(id) = t.point_id(s) {
            return n.get(id);
        }
        let mut out = String::new();
        let mut rest = s;
        while !rest.is_empty() {
            let mut matched = false;
            let mut cands: Vec<(usize, PointId)> = (0..t.n as PointId).filter(|&p| rest.starts_with(t.name(p)) && !t.name(p).is_empty()).map(|p| (t.name(p).len(), p)).collect();
            cands.sort_by(|a, b| b.cmp(a));
            if let Some((len, p)) = cands.first() {
                out.push_str(&n.get(*p));
                rest = &rest[*len..];
                matched = true;
            }
            if !matched {
                out.push_str(rest);
                break;
            }
        }
        out
    };
    match kind {
        "foot" => {
            let parts: Vec<&str> = args.split("->").collect();
            if parts.len() == 2 {
                return format!("Let {me} be the foot of the perpendicular from {} to {}.", tok(parts[0]), tok(parts[1]));
            }
        }
        "midpoint" => {
            let parts: Vec<&str> = args.split(',').collect();
            if parts.len() == 2 {
                return format!("Let {me} be the midpoint of {}{}.", tok(parts[0]), tok(parts[1]));
            }
        }
        "intersect" => {
            let parts: Vec<&str> = split_top(args);
            if parts.len() == 2 {
                return format!("Let {me} be the intersection of {} and {}.", shape(&tok, parts[0]), shape(&tok, parts[1]));
            }
        }
        "parallelogram" => {
            let parts: Vec<&str> = args.split(',').collect();
            if parts.len() == 3 {
                return format!("Let {me} complete the parallelogram {}{}{}{me}.", tok(parts[0]), tok(parts[1]), tok(parts[2]));
            }
        }
        _ => {}
    }
    format!("Let {me} = {desc}.")
}

fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

fn shape(tok: &dyn Fn(&str) -> String, s: &str) -> String {
    if let Some(rest) = s.strip_prefix("circumcircle(") {
        let pts: Vec<String> = rest.trim_end_matches(')').split(',').map(tok).collect();
        return format!("circle ({})", pts.join(""));
    }
    if let Some(rest) = s.strip_prefix("perp(") {
        let parts = split_top(rest.trim_end_matches(')'));
        if parts.len() == 2 {
            return format!("the perpendicular from {} to {}", tok(parts[0]), tok(parts[1]));
        }
    }
    format!("line {}", tok(s))
}

pub struct Rendered {
    pub lines: Vec<String>,
}

pub fn render(t: &EngineTrace, hp: &HumanProof, aux_desc: &[(PointId, String)], raw_line: &dyn Fn(FactId) -> String) -> Rendered {
    let n = Names::new(t, &hp.setup);
    let mut lines: Vec<String> = Vec::new();
    let claims: BTreeMap<u16, u16> = hp.blocks.iter().filter_map(|b| if let BlockKind::Claim(k) = b.kind { Some((b.id, k)) } else { None }).collect();
    for s in &hp.setup {
        match s {
            SetupLine::DirectedAngles => lines.push("∡ denotes directed angles modulo 180°.".into()),
            SetupLine::Circle { name, through, centre, diameter } => {
                let nm = if name.is_empty() { format!("({})", n.pts(through)) } else { name.clone() };
                let mut l = match diameter {
                    Some(d) => format!("Let {nm} be the circle with diameter {}", n.pts(&[d.0, d.1])),
                    None => format!("Let {nm} be the circle through {}", n.list(through)),
                };
                if let Some(c) = centre {
                    l.push_str(&format!("; its centre is {}", n.get(*c)));
                }
                l.push('.');
                lines.push(l);
            }
            SetupLine::Aux { point, aux_index } => {
                let desc = aux_desc.get(*aux_index).map(|x| x.1.clone()).unwrap_or_default();
                lines.push(aux_text(&n, t, *point, &desc));
            }
            SetupLine::Helper { point, meaning } => match meaning {
                HelperMeaning::Midpoint { of } => lines.push(format!("Let {} be the midpoint of {}.", n.get(*point), n.pts(&[of.0, of.1]))),
                HelperMeaning::Reflection { of, line } => lines.push(format!("Let {} be the reflection of {} in {}.", n.get(*point), n.get(*of), n.pts(&[line.0, line.1]))),
                HelperMeaning::Point => {}
            },
        }
    }
    if !lines.is_empty() {
        lines.push(String::new());
    }
    for b in &hp.blocks {
        let mut body = String::new();
        for s in &b.body {
            if !body.is_empty() {
                body.push(' ');
            }
            body.push_str(&sentence(&n, s, &claims, raw_line));
        }
        match b.kind {
            BlockKind::Claim(k) => {
                lines.push(format!("Claim {k}. {}.", cap(&stmt(&n, &b.stmt))));
                lines.push(format!("Proof. {body}"));
            }
            BlockKind::Conclusion => lines.push(format!("{body} ∎")),
            _ => lines.push(body),
        }
    }
    Rendered { lines }
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn sentence(n: &dyn PointNames, s: &Sentence, claims: &BTreeMap<u16, u16>, raw_line: &dyn Fn(FactId) -> String) -> String {
    match s {
        Sentence::Chain { terms, links, then, directed } => {
            let t: Vec<String> = terms.iter().map(|e| expr(n, e)).collect();
            let rs: Vec<String> = links.iter().map(|l| reasons_text(n, &l.reasons, claims).join(", ")).collect();
            let mut out = format!("{}{} ({})", if *directed { "" } else { "As drawn, " }, t.join(" = "), rs.join("; "));
            match then {
                Some(st) => out.push_str(&format!(", so {}.", stmt(n, st))),
                None => out.push('.'),
            }
            out
        }
        Sentence::Because { stmt: st, reasons, .. } => {
            if reasons.is_empty() {
                format!("Hence {}.", stmt(n, st))
            } else {
                let inner = match reasons.as_slice() {
                    [Reason::Fact { stmt: s2, because, .. }] if same_stmt(s2, st) && !because.is_empty() => because.as_slice(),
                    _ => reasons.as_slice(),
                };
                let rs: Vec<String> = reasons_text(n, inner, claims);
                let own = stmt(n, st);
                let restates = match inner {
                    [Reason::Fact { stmt: s2, .. }] => same_stmt(s2, st),
                    [Reason::Atom { stmt: s2, key: AtomKey::Radii, from, .. }] => !from.is_empty() && same_stmt(s2, st),
                    _ => false,
                };
                let from_claims: Vec<String> = match inner {
                    [Reason::Atom { from, .. }] => from.iter().filter_map(|b| claims.get(b).map(|k| format!("Claim {k}"))).collect(),
                    _ => Vec::new(),
                };
                if restates && !from_claims.is_empty() {
                    format!("{} ({}).", cap(&own), from_claims.join(", "))
                } else if restates || (rs.len() == 1 && rs[0] == own) {
                    format!("{} (shown above).", cap(&own))
                } else {
                    format!("{} ({}).", cap(&own), rs.join("; "))
                }
            }
        }
        Sentence::Pooled { stmt: st, reasons, .. } => {
            let rs: Vec<String> = reasons_text(n, reasons, claims);
            let angular = matches!(st, Stmt::Coll { .. } | Stmt::Cyclic { .. } | Stmt::Perp { .. } | Stmt::Para { .. } | Stmt::EqAngle { .. } | Stmt::AngleConst { .. });
            if angular {
                format!("Angle chasing with {} gives {}.", join_and(&rs), stmt(n, st))
            } else {
                format!("From {}, {}.", join_and(&rs), stmt(n, st))
            }
        }
        Sentence::Theorem { key, stmt: st, reasons } => {
            let rs: Vec<String> = reasons_text(n, reasons, claims);
            match (st, rs.is_empty()) {
                (Stmt::Formula { .. }, true) => format!("Apply {}.", theorem_name(*key)),
                (Stmt::Formula { .. }, false) => format!("Apply {} ({}).", theorem_name(*key), rs.join("; ")),
                (_, true) => format!("By {}, {}.", theorem_name(*key), stmt(n, st)),
                (_, false) => format!("By {} ({}), {}.", theorem_name(*key), rs.join("; "), stmt(n, st)),
            }
        }
        Sentence::Computation { terms, links, .. } => {
            let mut out = String::new();
            for (i, t) in terms.iter().enumerate() {
                if i == 0 {
                    out.push_str(&format!("\n    {}", expr(n, t)));
                } else {
                    let rs: Vec<String> = links[i - 1].reasons.iter().map(|r| reason(n, r, claims)).collect();
                    out.push_str(&format!("\n  = {}    [{}]", expr(n, t), rs.join("; ")));
                }
            }
            out.push('\n');
            out
        }
        Sentence::Raw { engine_fact, cites } => {
            let c: Vec<String> = cites.iter().map(|x| format!("{}", x + 1)).collect();
            format!("{} [facts {}].", raw_line(*engine_fact), c.join(", "))
        }
    }
}

pub fn reasons_text(n: &dyn PointNames, rs: &[Reason], claims: &BTreeMap<u16, u16>) -> Vec<String> {
    let fact_stmts: Vec<&Stmt> = rs.iter().filter_map(|r| if let Reason::Fact { stmt, .. } = r { Some(stmt) } else { None }).collect();
    let mut out: Vec<String> = Vec::new();
    for r in rs {
        if let Reason::Atom { stmt, .. } = r {
            if fact_stmts.iter().any(|s| same_stmt(s, stmt)) {
                continue;
            }
        }
        let t = reason(n, r, claims);
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

fn same_stmt(a: &Stmt, b: &Stmt) -> bool {
    match (a, b) {
        (Stmt::Cong { s1, s2 }, Stmt::Cong { s1: t1, s2: t2 }) => {
            let n = |x: (PointId, PointId)| (x.0.min(x.1), x.0.max(x.1));
            let (a1, a2, b1, b2) = (n(*s1), n(*s2), n(*t1), n(*t2));
            (a1 == b1 && a2 == b2) || (a1 == b2 && a2 == b1)
        }
        _ => a == b,
    }
}

fn join_and(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        _ => format!("{} and {}", v[..v.len() - 1].join(", "), v[v.len() - 1]),
    }
}

pub fn engine_line(t: &EngineTrace, f: FactId) -> String {
    let names: Vec<String> = t.names.iter().map(|x| disp(x)).collect();
    let fact = &t.facts[f as usize];
    let log = crate::proof::ProofLog { facts: vec![fact.clone()] };
    let line = log.step_lines(&[0], &names).into_iter().next().unwrap_or_default();
    let line = line.split_once(". ").map(|x| x.1.to_string()).unwrap_or(line);
    let _ = ER::Assumption(String::new());
    line
}
