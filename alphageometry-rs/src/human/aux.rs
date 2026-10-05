use super::text::PointNames;
use super::trace::EngineTrace;
use crate::predicate::PointId;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuxArg {
    pub key: &'static str,
    pub pts: Vec<PointId>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuxWording {
    pub key: &'static str,
    pub args: Vec<AuxArg>,
}

pub const KEYS: [&str; 22] = [
    "aux.midpoint",
    "aux.foot",
    "aux.reflect",
    "aux.circumcenter",
    "aux.orthocenter",
    "aux.incenter",
    "aux.excenter",
    "aux.excenter_any",
    "aux.centroid",
    "aux.bisector_foot",
    "aux.antipode_on",
    "aux.parallelogram",
    "aux.incircle_touch",
    "aux.excircle_touch",
    "aux.spiral_center",
    "aux.isogonal",
    "aux.inverse",
    "aux.pole",
    "aux.tangent",
    "aux.arc_midpoint",
    "aux.harmonic",
    "aux.intersect",
];

pub const ARG_KEYS: [&str; 9] = ["point", "line", "segment", "triangle", "aux.line.para", "aux.line.perp", "aux.line.tangent_at", "aux.line.isogonal", "aux.circumcircle"];

fn split_top<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    let b = s.as_bytes();
    let mut i = 0;
    while i < s.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && s[i..].starts_with(sep) {
            out.push(s[start..i].trim());
            i += sep.len();
            start = i;
            continue;
        }
        i += 1;
    }
    out.push(s[start..].trim());
    out
}

pub fn points(t: &EngineTrace, s: &str) -> Option<Vec<PointId>> {
    let s = s.trim();
    if s.is_empty() {
        return Some(Vec::new());
    }
    let mut cands: Vec<(usize, PointId)> = (0..t.n as PointId).filter(|&p| !t.name(p).is_empty() && s.starts_with(t.name(p))).map(|p| (t.name(p).len(), p)).collect();
    cands.sort_by(|a, b| b.cmp(a));
    for (len, p) in cands {
        if let Some(mut rest) = points(t, &s[len..]) {
            rest.insert(0, p);
            return Some(rest);
        }
    }
    None
}

fn call(s: &str) -> Option<(&str, &str)> {
    let s = s.trim();
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    Some((&s[..open], &s[open + 1..s.len() - 1]))
}

fn list(t: &EngineTrace, s: &str) -> Option<Vec<PointId>> {
    let mut out = Vec::new();
    for part in split_top(s, ",") {
        out.extend(points(t, part)?);
    }
    Some(out)
}

pub fn shape(t: &EngineTrace, s: &str) -> Option<AuxArg> {
    let s = s.trim();
    if let Some((head, inner)) = call(s) {
        return match head {
            "circumcircle" => Some(AuxArg { key: "aux.circumcircle", pts: list(t, inner)? }),
            "circle" => {
                let p = list(t, inner)?;
                (p.len() == 2).then(|| AuxArg { key: "aux.circle", pts: p })
            }
            "para" | "perp" => {
                let parts = split_top(inner, ",");
                let mut p = points(t, parts.first()?)?;
                p.extend(points(t, parts.get(1)?)?);
                (p.len() == 3).then(|| AuxArg { key: if head == "para" { "aux.line.para" } else { "aux.line.perp" }, pts: p })
            }
            "tangent_at" => {
                let parts = split_top(inner, ",");
                let mut p = points(t, parts.first()?)?;
                p.extend(points(t, parts.get(1)?.trim_start_matches("centre "))?);
                (p.len() == 2).then(|| AuxArg { key: "aux.line.tangent_at", pts: p })
            }
            "isogonal" => {
                let parts = split_top(inner, " in ");
                let mut p = points(t, parts.first()?)?;
                p.extend(points(t, parts.get(1)?)?);
                (p.len() == 5).then(|| AuxArg { key: "aux.line.isogonal", pts: p })
            }
            _ => None,
        };
    }
    let p = points(t, s)?;
    (p.len() == 2).then(|| AuxArg { key: "line", pts: p })
}

fn on_shape(a: &AuxArg) -> Vec<PointId> {
    match a.key {
        "line" | "aux.circumcircle" => a.pts.clone(),
        "aux.circle" => vec![a.pts[1]],
        "aux.line.para" | "aux.line.perp" => vec![a.pts[0]],
        "aux.line.tangent_at" => vec![a.pts[0]],
        "aux.line.isogonal" => vec![a.pts[0]],
        _ => Vec::new(),
    }
}

fn side(t: &EngineTrace, p: PointId, a: PointId, b: PointId) -> i32 {
    t.orient(a, b, p)
}

pub fn parse(t: &EngineTrace, me: PointId, desc: &str) -> Option<AuxWording> {
    let (kind, inner) = call(desc)?;
    let pt = |s: &str| -> Option<AuxArg> {
        let p = points(t, s)?;
        (p.len() == 1).then(|| AuxArg { key: "point", pts: p })
    };
    let seg = |s: &str| -> Option<AuxArg> {
        let p = points(t, s)?;
        (p.len() == 2).then(|| AuxArg { key: "line", pts: p })
    };
    let tri = |s: &str| -> Option<AuxArg> {
        let p = list(t, s)?;
        (p.len() == 3).then(|| AuxArg { key: "triangle", pts: p })
    };
    let w = |key: &'static str, args: Vec<AuxArg>| Some(AuxWording { key, args });
    match kind {
        "midpoint" => {
            let p = list(t, inner)?;
            (p.len() == 2).then(|| AuxWording { key: "aux.midpoint", args: vec![AuxArg { key: "segment", pts: p }] })
        }
        "foot" => {
            let parts = split_top(inner, "->");
            w("aux.foot", vec![pt(parts.first()?)?, seg(parts.get(1)?)?])
        }
        "reflect" => {
            let parts = split_top(inner, " over ");
            let rhs = parts.get(1)?;
            let target = if points(t, rhs)?.len() == 1 { pt(rhs)? } else { seg(rhs)? };
            w("aux.reflect", vec![pt(parts.first()?)?, target])
        }
        "circumcenter" | "orthocenter" | "incenter" | "centroid" => {
            let key = match kind {
                "circumcenter" => "aux.circumcenter",
                "orthocenter" => "aux.orthocenter",
                "incenter" => "aux.incenter",
                _ => "aux.centroid",
            };
            w(key, vec![tri(inner)?])
        }
        "excenter" => {
            let tr = tri(inner)?;
            let v = &tr.pts;
            let away: Vec<usize> = (0..3).filter(|&k| side(t, v[k], v[(k + 1) % 3], v[(k + 2) % 3]) * side(t, me, v[(k + 1) % 3], v[(k + 2) % 3]) < 0).collect();
            if away.len() == 1 {
                let k = away[0];
                w("aux.excenter", vec![tr.clone(), AuxArg { key: "point", pts: vec![v[k]] }])
            } else {
                w("aux.excenter_any", vec![tr])
            }
        }
        "bisector_foot" => {
            let parts = split_top(inner, " in ");
            let p = points(t, parts.get(1)?)?;
            (p.len() == 3).then_some(())?;
            w("aux.bisector_foot", vec![pt(parts.first()?)?, AuxArg { key: "triangle", pts: p }])
        }
        "antipode" => {
            if let Some(i) = inner.find(" in ") {
                let circ = inner[i + 4..].trim().trim_start_matches('(').trim_end_matches(')');
                return w("aux.antipode_on", vec![pt(&inner[..i])?, AuxArg { key: "aux.circumcircle", pts: list(t, circ)? }]);
            }
            let parts = split_top(inner, " on ");
            w("aux.antipode_on", vec![pt(parts.first()?)?, shape(t, parts.get(1)?)?])
        }
        "parallelogram" => {
            let p = list(t, inner)?;
            (p.len() == 3).then(|| AuxWording { key: "aux.parallelogram", args: vec![AuxArg { key: "triangle", pts: p }] })
        }
        "incircle_touch" | "excircle_touch" => {
            let rest = inner.trim_start_matches("opp ");
            let parts = split_top(rest, " in ");
            let p = points(t, parts.get(1)?)?;
            (p.len() == 3).then_some(())?;
            w(if kind == "incircle_touch" { "aux.incircle_touch" } else { "aux.excircle_touch" }, vec![pt(parts.first()?)?, AuxArg { key: "triangle", pts: p }])
        }
        "spiral_center" => {
            let parts = split_top(inner, "->");
            w("aux.spiral_center", vec![AuxArg { key: "segment", pts: points(t, parts.first()?)? }, AuxArg { key: "segment", pts: points(t, parts.get(1)?)? }])
        }
        "isogonal" => {
            let parts = split_top(inner, " in ");
            let p = points(t, parts.get(1)?)?;
            (p.len() == 3).then_some(())?;
            w("aux.isogonal", vec![pt(parts.first()?)?, AuxArg { key: "triangle", pts: p }])
        }
        "inverse" => {
            let parts = split_top(inner, " in ");
            w("aux.inverse", vec![pt(parts.first()?)?, shape(t, parts.get(1)?)?])
        }
        "pole" => {
            let parts = split_top(inner, " of ");
            w("aux.pole", vec![seg(parts.first()?)?, shape(t, parts.get(1)?)?])
        }
        "tangent" => {
            let parts = split_top(inner, " to ");
            w("aux.tangent", vec![pt(parts.first()?)?, shape(t, parts.get(1)?)?])
        }
        "arc_midpoint" => {
            let parts = split_top(inner, " on ");
            let p = list(t, parts.first()?)?;
            (p.len() == 2).then_some(())?;
            w("aux.arc_midpoint", vec![AuxArg { key: "segment", pts: p }, shape(t, parts.get(1)?)?])
        }
        "harmonic" => {
            let parts = split_top(inner, " wrt ");
            let ab = list(t, parts.get(1)?)?;
            (ab.len() == 2).then_some(())?;
            w("aux.harmonic", vec![pt(parts.first()?)?, AuxArg { key: "point", pts: vec![ab[0]] }, AuxArg { key: "point", pts: vec![ab[1]] }])
        }
        "intersect" => {
            let parts = split_top(inner, ",");
            if parts.len() != 2 {
                return None;
            }
            let (a, b) = (shape(t, parts[0])?, shape(t, parts[1])?);
            let common: Vec<PointId> = on_shape(&a).into_iter().filter(|p| *p != me && on_shape(&b).contains(p)).collect();
            if let Some(&c) = common.first() {
                return w("aux.intersect2", vec![a, b, AuxArg { key: "point", pts: vec![c] }]);
            }
            w("aux.intersect", vec![a, b])
        }
        _ => None,
    }
}

pub fn arg_en(n: &dyn PointNames, a: &AuxArg, circle_name: &dyn Fn(&[PointId]) -> Option<String>) -> String {
    let p = |i: usize| n.get(a.pts[i]);
    match a.key {
        "point" => p(0),
        "segment" => n.pts(&a.pts),
        "triangle" => n.pts(&a.pts),
        "line" => format!("line {}", n.pts(&a.pts)),
        "aux.line.para" => format!("the line through {} parallel to {}", p(0), n.pts(&a.pts[1..])),
        "aux.line.perp" => format!("the line through {} perpendicular to {}", p(0), n.pts(&a.pts[1..])),
        "aux.line.tangent_at" => format!("the tangent at {} to the circle centred at {}", p(0), p(1)),
        "aux.line.isogonal" => format!("the isogonal of {}{} in ∠{}", p(0), p(1), n.pts(&a.pts[2..])),
        "aux.circumcircle" => circle_name(&a.pts).unwrap_or_else(|| format!("circle ({})", n.pts(&a.pts))),
        "aux.circle" => format!("the circle centred at {} through {}", p(0), p(1)),
        _ => n.pts(&a.pts),
    }
}

pub fn en(n: &dyn PointNames, me: &str, w: &AuxWording, circle_name: &dyn Fn(&[PointId]) -> Option<String>) -> String {
    let a = |i: usize| w.args.get(i).map(|x| arg_en(n, x, circle_name)).unwrap_or_default();
    let body = match w.key {
        "aux.midpoint" => format!("the midpoint of {}", a(0)),
        "aux.foot" => format!("the foot of the perpendicular from {} to {}", a(0), a(1).trim_start_matches("line ")),
        "aux.reflect" => format!("the reflection of {} in {}", a(0), a(1)),
        "aux.circumcenter" => format!("the circumcentre of triangle {}", a(0)),
        "aux.orthocenter" => format!("the orthocentre of triangle {}", a(0)),
        "aux.incenter" => format!("the incentre of triangle {}", a(0)),
        "aux.centroid" => format!("the centroid of triangle {}", a(0)),
        "aux.excenter" => format!("the excentre of triangle {} opposite {}", a(0), a(1)),
        "aux.excenter_any" => format!("an excentre of triangle {}", a(0)),
        "aux.bisector_foot" => format!("the foot of the angle bisector from {} in triangle {}", a(0), a(1)),
        "aux.antipode_on" => format!("the antipode of {} on {}", a(0), a(1)),
        "aux.parallelogram" => {
            let p = &w.args[0].pts;
            return format!("Let {me} be the point such that {}{}{}{me} is a parallelogram.", n.get(p[0]), n.get(p[1]), n.get(p[2]));
        }
        "aux.incircle_touch" => format!("the point where the incircle of triangle {} touches the side opposite {}", a(1), a(0)),
        "aux.excircle_touch" => format!("the point where the excircle of triangle {} opposite {} touches the side opposite {}", a(1), a(0), a(0)),
        "aux.spiral_center" => format!("the centre of the spiral similarity taking {} to {}", a(0), a(1)),
        "aux.isogonal" => format!("the isogonal conjugate of {} in triangle {}", a(0), a(1)),
        "aux.inverse" => format!("the inverse of {} in {}", a(0), a(1)),
        "aux.pole" => format!("the pole of {} with respect to {}", a(0).trim_start_matches("line "), a(1)),
        "aux.tangent" => format!("a point of tangency of a tangent from {} to {}", a(0), a(1)),
        "aux.arc_midpoint" => format!("the midpoint of arc {} of {}", a(0), a(1)),
        "aux.harmonic" => format!("the harmonic conjugate of {} with respect to {} and {}", a(0), a(1), a(2)),
        "aux.intersect2" => format!("the second intersection of {} and {}", a(0), a(1)),
        "aux.intersect" => format!("the intersection of {} and {}", a(0), a(1)),
        _ => return format!("Let {me} be a point of the construction."),
    };
    format!("Let {me} be {body}.")
}
