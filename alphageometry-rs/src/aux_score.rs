//! Coincidence score of an auxiliary-point candidate: how many figure lines,
//! circles and equal-distance classes pass through it beyond the ones that
//! define it. Ranking only; nothing here is asserted.

use crate::numerics::{distance, NumCircle, NumLine, Vec2};
use crate::predicate::{PointId, Predicate};

#[derive(Clone, Copy, Debug)]
pub(crate) enum DefObj {
    Line(Vec2),
    Circle(Vec2, f64),
    EqDist(PointId, PointId),
}

struct FigLine {
    line: NumLine,
    dir: Vec2,
    goal: bool,
}

struct FigCircle {
    circle: NumCircle,
    goal: bool,
}

pub(crate) struct Scorer {
    pts: Vec<Vec2>,
    goal: Vec<bool>,
    scale: f64,
    lines: Vec<FigLine>,
    circles: Vec<FigCircle>,
}

impl Scorer {
    pub(crate) fn new(pts: &[Vec2], goal_pts: &[PointId]) -> Scorer {
        let n = pts.len();
        let mut goal = vec![false; n];
        for &g in goal_pts {
            if (g as usize) < n {
                goal[g as usize] = true;
            }
        }
        let scale = (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
            .map(|(i, j)| distance(pts[i], pts[j]))
            .fold(0.0f64, f64::max)
            .max(1e-9);
        let tol = 1e-9 * scale;

        let mut lines: Vec<FigLine> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                if distance(pts[i], pts[j]) < tol {
                    continue;
                }
                let l = NumLine::through(pts[i], pts[j]);
                let dir = (pts[j] - pts[i]).normalize();
                if lines
                    .iter()
                    .any(|f| cross(f.dir, dir).abs() < 1e-9 && f.line.distance(pts[i]) < tol)
                {
                    continue;
                }
                let g = (0..n).any(|k| goal[k] && l.distance(pts[k]) < tol);
                lines.push(FigLine {
                    line: l,
                    dir,
                    goal: g,
                });
            }
        }

        let mut circles: Vec<FigCircle> = Vec::new();
        let mut keys: Vec<(f64, f64, f64)> = Vec::new();
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    if let Some(c) = NumCircle::through(pts[i], pts[j], pts[k]) {
                        if c.r.is_finite() && c.r < 1e3 * scale {
                            keys.push((c.center.x, c.center.y, c.r));
                        }
                    }
                }
            }
        }
        keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let ctol = 1e-8 * scale;
        let mut taken = vec![false; keys.len()];
        for a in 0..keys.len() {
            if taken[a] {
                continue;
            }
            let (x, y, r) = keys[a];
            for b in a + 1..keys.len() {
                if keys[b].0 - x > ctol {
                    break;
                }
                if (keys[b].1 - y).abs() < ctol && (keys[b].2 - r).abs() < ctol {
                    taken[b] = true;
                }
            }
            let circle = NumCircle {
                center: Vec2::new(x, y),
                r,
            };
            let g = (0..n).any(|k| goal[k] && circle.distance(pts[k]) < ctol);
            circles.push(FigCircle { circle, goal: g });
        }

        Scorer {
            pts: pts.to_vec(),
            goal,
            scale,
            lines,
            circles,
        }
    }

    pub(crate) fn scale(&self) -> f64 {
        self.scale
    }

    /// Extra incidences of `x` (each figure line or circle through it, and each
    /// equal-distance class it centres, not implied by `defs`), plus half a
    /// point for every such object that also holds a goal point.
    pub(crate) fn score(&self, x: Vec2, defs: &[DefObj]) -> f64 {
        let tol = 1e-8 * self.scale;
        let mut s = 0.0;
        for l in &self.lines {
            if l.line.distance(x) >= tol {
                continue;
            }
            if defs
                .iter()
                .any(|d| matches!(d, DefObj::Line(u) if cross(*u, l.dir).abs() < 1e-7))
            {
                continue;
            }
            s += if l.goal { 1.5 } else { 1.0 };
        }
        for c in &self.circles {
            if c.circle.distance(x) >= tol * (1.0 + c.circle.r / self.scale) {
                continue;
            }
            if defs.iter().any(|d| {
                matches!(d, DefObj::Circle(o, r)
                    if distance(*o, c.circle.center) < 1e-6 * self.scale
                        && (r - c.circle.r).abs() < 1e-6 * self.scale)
            }) {
                continue;
            }
            s += if c.goal { 1.5 } else { 1.0 };
        }
        s + self.centre_score(x, defs)
    }

    fn centre_score(&self, x: Vec2, defs: &[DefObj]) -> f64 {
        let n = self.pts.len();
        let tol = 1e-9 * self.scale;
        let mut d: Vec<(f64, usize)> = (0..n).map(|i| (distance(x, self.pts[i]), i)).collect();
        d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut s = 0.0;
        let mut a = 0;
        while a < n {
            let mut b = a + 1;
            while b < n && d[b].0 - d[b - 1].0 < tol * (1.0 + d[b].0 / self.scale) {
                b += 1;
            }
            if b - a >= 2 {
                let members: Vec<usize> = d[a..b].iter().map(|e| e.1).collect();
                let mut parent: Vec<usize> = (0..members.len()).collect();
                fn find(p: &mut [usize], mut x: usize) -> usize {
                    while p[x] != x {
                        p[x] = p[p[x]];
                        x = p[x];
                    }
                    x
                }
                for def in defs {
                    if let DefObj::EqDist(u, v) = *def {
                        let iu = members.iter().position(|&m| m == u as usize);
                        let iv = members.iter().position(|&m| m == v as usize);
                        if let (Some(iu), Some(iv)) = (iu, iv) {
                            let (ru, rv) = (find(&mut parent, iu), find(&mut parent, iv));
                            parent[ru] = rv;
                        }
                    }
                }
                let comps = (0..members.len())
                    .filter(|&i| find(&mut parent, i) == i)
                    .count();
                let goal = members.iter().any(|&m| self.goal[m]);
                s += (comps - 1) as f64 * if goal { 1.5 } else { 1.0 };
            }
            a = b;
        }
        s
    }
}

fn cross(u: Vec2, v: Vec2) -> f64 {
    u.x * v.y - u.y * v.x
}

/// The loci a construction's own predicates put `me` on, read numerically:
/// lines through it, circles through it, and pairs it is equidistant from.
pub(crate) fn defs_of(
    preds: &[Predicate],
    me: PointId,
    coord: impl Fn(PointId) -> Vec2,
) -> Vec<DefObj> {
    let x = coord(me);
    let mut out = Vec::new();
    let line_to = |other: PointId, out: &mut Vec<DefObj>| {
        let v = coord(other) - x;
        if v.norm() > 0.0 {
            out.push(DefObj::Line(v.normalize()));
        }
    };
    for p in preds {
        let pts = &p.points;
        match p.name.as_str() {
            "coll" => {
                for &q in pts {
                    if q != me {
                        line_to(q, &mut out);
                    }
                }
            }
            "para" | "perp" | "eqangle" => {
                for ch in pts.chunks_exact(2) {
                    if ch[0] == me && ch[1] != me {
                        line_to(ch[1], &mut out);
                    } else if ch[1] == me && ch[0] != me {
                        line_to(ch[0], &mut out);
                    }
                }
            }
            "cyclic" if pts.len() >= 4 && pts.contains(&me) => {
                let others: Vec<PointId> = pts.iter().copied().filter(|&q| q != me).collect();
                if let Some(c) =
                    NumCircle::through(coord(others[0]), coord(others[1]), coord(others[2]))
                {
                    out.push(DefObj::Circle(c.center, c.r));
                }
            }
            "cong" if pts.len() == 4 => {
                let other = |a: PointId, b: PointId| {
                    if a == me && b != me {
                        Some(b)
                    } else if b == me && a != me {
                        Some(a)
                    } else {
                        None
                    }
                };
                match (other(pts[0], pts[1]), other(pts[2], pts[3])) {
                    (Some(u), Some(v)) => out.push(DefObj::EqDist(u, v)),
                    (Some(o), None) => out.push(DefObj::Circle(
                        coord(o),
                        distance(coord(pts[2]), coord(pts[3])),
                    )),
                    (None, Some(o)) => out.push(DefObj::Circle(
                        coord(o),
                        distance(coord(pts[0]), coord(pts[1])),
                    )),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_of_hypotenuse_scores_as_a_circumcentre() {
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(0.0, 2.0),
        ];
        let s = Scorer::new(&pts, &[]);
        let m = Vec2::new(1.0, 1.0);
        let defs = [
            DefObj::Line(Vec2::new(1.0, -1.0).normalize()),
            DefObj::EqDist(1, 2),
        ];
        assert_eq!(s.score(m, &defs), 1.0);
        let generic = Vec2::new(0.3, 0.7);
        assert_eq!(s.score(generic, &[]), 0.0);
    }
}
