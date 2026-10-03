//! The AlphaGeometry problem/predicate language and its parser.
//!
//! Faithful port of `parse.py`, with one deliberate change for performance:
//! points are resolved from names to dense integer ids up front, so the engine
//! never hashes point names or objects.

use crate::numerics::Vec2;
use crate::rational::Rat;

/// Dense identifier of a point.
pub type PointId = u32;

/// A named point with concrete coordinates.
#[derive(Clone, Debug)]
pub struct Point {
    pub name: String,
    pub value: Vec2,
}

/// A geometric predicate over points and rational constants.
#[derive(Clone, Debug)]
pub struct Predicate {
    pub name: String,
    pub points: Vec<PointId>,
    /// Constants. Angle degree/`pi` constants are stored pre-scaled exactly as
    /// the reference parser does (a `Xpi/Y` token becomes `X*180/Y`).
    pub constants: Vec<Rat>,
}

/// A fully parsed low-level geometry problem.
#[derive(Clone, Debug)]
pub struct Problem {
    pub points: Vec<Point>,
    pub preds: Vec<Predicate>,
    pub goal: Option<Predicate>,
}

/// A predicate whose point arguments are still names (pre-resolution).
#[derive(Clone, Debug)]
struct RawPredicate {
    name: String,
    point_names: Vec<String>,
    constants: Vec<Rat>,
}

fn parse_constant(tok: &str) -> Result<Rat, String> {
    if let Ok(n) = tok.parse::<i64>() {
        return Ok(Rat::from_int(n));
    }
    if let Some((num, den)) = tok.split_once("pi/") {
        let num: i64 = num
            .parse()
            .map_err(|_| format!("bad pi numerator in '{tok}'"))?;
        let den: i64 = den
            .parse()
            .map_err(|_| format!("bad pi denominator in '{tok}'"))?;
        if den == 0 {
            return Err(format!("zero denominator in '{tok}'"));
        }
        return Ok(&Rat::new(num, den) * &Rat::from_int(180));
    }
    if let Some((num, den)) = tok.split_once('/') {
        let num: i64 = num
            .parse()
            .map_err(|_| format!("bad numerator in '{tok}'"))?;
        let den: i64 = den
            .parse()
            .map_err(|_| format!("bad denominator in '{tok}'"))?;
        if den == 0 {
            return Err(format!("zero denominator in '{tok}'"));
        }
        return Ok(Rat::new(num, den));
    }
    Err(format!("unrecognized constant '{tok}'"))
}

impl RawPredicate {
    fn parse(line: &str) -> Result<RawPredicate, String> {
        let mut it = line.split_whitespace();
        let name = it.next().ok_or("empty predicate")?.to_string();
        let mut point_names = Vec::new();
        let mut constants = Vec::new();
        for arg in it {
            let first = arg.chars().next().unwrap();
            if first.is_ascii_digit() || first == '-' {
                constants.push(parse_constant(arg)?);
            } else {
                point_names.push(arg.to_string());
            }
        }
        Ok(RawPredicate {
            name,
            point_names,
            constants,
        })
    }
}

impl Predicate {
    /// Parse a single predicate given a name resolver.
    pub fn parse(
        line: &str,
        resolve: &dyn Fn(&str) -> Option<PointId>,
    ) -> Result<Predicate, String> {
        let raw = RawPredicate::parse(line)?;
        raw.resolve(resolve)
    }
}

impl RawPredicate {
    fn resolve(self, resolve: &dyn Fn(&str) -> Option<PointId>) -> Result<Predicate, String> {
        let mut points = Vec::with_capacity(self.point_names.len());
        for n in &self.point_names {
            points.push(resolve(n).ok_or_else(|| format!("unknown point '{n}'"))?);
        }
        Ok(Predicate {
            name: self.name,
            points,
            constants: self.constants,
        })
    }
}

impl Problem {
    /// Parse a problem in the AlphaGeometry single-line format.
    pub fn parse(line: &str) -> Result<Problem, String> {
        let (steps, goal_str) = match line.split_once('?') {
            Some((s, g)) => (s, Some(g)),
            None => (line, None),
        };

        let mut points: Vec<Point> = Vec::new();
        let mut name_to_id: std::collections::HashMap<String, PointId> =
            std::collections::HashMap::new();
        let mut raw_preds: Vec<RawPredicate> = Vec::new();

        for step in steps.split(';') {
            let step = step.trim();
            if step.is_empty() {
                continue;
            }
            let (points_part, constraints_part) = step
                .split_once('=')
                .ok_or_else(|| format!("step without '=': '{step}'"))?;

            for point_tok in points_part.split_whitespace() {
                let (name, value) = point_tok
                    .split_once('@')
                    .ok_or_else(|| format!("point without value: '{point_tok}'"))?;
                let (xs, ys) = value
                    .split_once('_')
                    .ok_or_else(|| format!("bad point value: '{value}'"))?;
                let x: f64 = xs.parse().map_err(|_| format!("bad x in '{point_tok}'"))?;
                let y: f64 = ys.parse().map_err(|_| format!("bad y in '{point_tok}'"))?;
                if !name_to_id.contains_key(name) {
                    name_to_id.insert(name.to_string(), points.len() as PointId);
                    points.push(Point {
                        name: name.to_string(),
                        value: Vec2::new(x, y),
                    });
                }
            }

            for constraint in constraints_part.split(',') {
                let constraint = constraint.trim();
                if constraint.is_empty() {
                    continue;
                }
                raw_preds.push(RawPredicate::parse(constraint)?);
            }
        }

        let resolve = |n: &str| name_to_id.get(n).copied();
        let preds = raw_preds
            .into_iter()
            .map(|r| r.resolve(&resolve))
            .collect::<Result<Vec<_>, _>>()?;
        let goal = match goal_str {
            Some(g) if !g.trim().is_empty() => Some(Predicate::parse(g.trim(), &resolve)?),
            _ => None,
        };

        Ok(Problem {
            points,
            preds,
            goal,
        })
    }

    /// Name of a point id (for diagnostics).
    pub fn point_name(&self, id: PointId) -> &str {
        &self.points[id as usize].name
    }

    fn pred_to_string(&self, p: &Predicate) -> String {
        let mut tokens = vec![p.name.clone()];
        tokens.extend(p.points.iter().map(|&id| self.point_name(id).to_string()));
        for c in &p.constants {
            tokens.push(match (c.numer_i64(), c.denom_i64()) {
                (Some(n), Some(1)) => n.to_string(),
                (Some(n), Some(d)) => format!("{n}/{d}"),
                _ => format!("{c:?}"),
            });
        }
        tokens.join(" ")
    }

    /// Serialize back to the low-level AlphaGeometry single-line format, so a
    /// compiled problem can be printed and re-parsed. Full-precision coordinates
    /// are emitted so the round trip is exact.
    pub fn to_ag_string(&self) -> String {
        let pts: Vec<String> = self
            .points
            .iter()
            .map(|p| format!("{}@{}_{}", p.name, p.value.x, p.value.y))
            .collect();
        let preds: Vec<String> = self.preds.iter().map(|p| self.pred_to_string(p)).collect();
        let mut s = format!("{} = {}", pts.join(" "), preds.join(", "));
        if let Some(goal) = &self.goal {
            s.push_str(" ? ");
            s.push_str(&self.pred_to_string(goal));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_problem() {
        let p = Problem::parse("a@0.0_0.0 = ; b@1.0_0.0 = ; c@0.0_1.0 = coll a b c ? cong a b a c")
            .unwrap();
        assert_eq!(p.points.len(), 3);
        assert_eq!(p.points[0].name, "a");
        assert_eq!(p.preds.len(), 1);
        assert_eq!(p.preds[0].name, "coll");
        assert_eq!(p.preds[0].points, vec![0, 1, 2]);
        let g = p.goal.unwrap();
        assert_eq!(g.name, "cong");
        assert_eq!(g.points, vec![0, 1, 0, 2]);
    }

    #[test]
    fn parse_constants() {
        // degree constant, ratio, pi ratio, negative
        let p = Problem::parse(
            "a@0.0_0.0 = ; b@1.0_0.0 = aconst a b a b 60, rconst a b a b 3/2 ? coll a b",
        )
        .unwrap();
        assert_eq!(p.preds[0].constants, vec![Rat::from_int(60)]);
        assert_eq!(p.preds[1].constants, vec![Rat::new(3, 2)]);
    }

    #[test]
    fn parse_pi_constant() {
        let raw = RawPredicate::parse("s_angle a b c d 1pi/3").unwrap();
        // 1pi/3 -> 1*180/3 = 60
        assert_eq!(raw.constants, vec![Rat::from_int(60)]);
    }

    #[test]
    fn parse_negative_coords_and_primes() {
        let p = Problem::parse("x'@-0.5_-0.25 = ; y@1.0_2.0 = coll x' y ? coll x' y").unwrap();
        assert_eq!(p.points[0].name, "x'");
        assert_eq!(p.points[0].value, Vec2::new(-0.5, -0.25));
    }
}
