//! Numerical (floating-point) Euclidean geometry.
//!
//! Faithful port of the reference `numericals.py`. DDAR uses concrete point
//! coordinates to *guess* which geometric facts hold (collinearity, equal
//! distances, inscribed angles, …); those guesses are then discharged exactly
//! by the algebraic elimination engine. This module is the "oracle".

use std::f64::consts::PI;

/// Numerical tolerance used throughout, matching the reference implementation.
pub const ATOM: f64 = 1e-12;

/// A point / vector in the plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    #[inline]
    pub const fn new(x: f64, y: f64) -> Vec2 {
        Vec2 { x, y }
    }

    #[inline]
    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    #[inline]
    pub fn norm(self) -> f64 {
        self.dot(self).sqrt()
    }

    #[inline]
    pub fn normalize(self) -> Vec2 {
        let n = self.norm();
        Vec2::new(self.x / n, self.y / n)
    }

    /// Rotate by -90 degrees: `[x, y] -> [y, -x]` (matches `perp_rot`).
    #[inline]
    pub fn perp_rot(self) -> Vec2 {
        Vec2::new(self.y, -self.x)
    }

    /// Direction in units of half-turns (pi), i.e. `atan2(y, x) / pi`.
    /// Range `(-1, 1]`.
    #[inline]
    pub fn direction(self) -> f64 {
        self.y.atan2(self.x) / PI
    }
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    #[inline]
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    #[inline]
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl std::ops::Mul<f64> for Vec2 {
    type Output = Vec2;
    #[inline]
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

#[inline]
pub fn distance(a: Vec2, b: Vec2) -> f64 {
    (a - b).norm()
}

/// Free-function form of [`Vec2::direction`].
#[inline]
pub fn direction_of(v: Vec2) -> f64 {
    v.direction()
}

#[inline]
pub fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    (a + b) * 0.5
}

/// Raw signed area determinant `det([b-a; c-a])`.
#[inline]
pub fn det(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    let u = b - a;
    let v = c - a;
    u.x * v.y - u.y * v.x
}

/// Orientation of the triple: `+1` counter-clockwise, `-1` clockwise, `0`
/// collinear (within [`ATOM`]).
#[inline]
pub fn orientation(a: Vec2, b: Vec2, c: Vec2) -> i32 {
    let d = det(a, b, c);
    if d > ATOM {
        1
    } else if d < -ATOM {
        -1
    } else {
        0
    }
}

#[inline]
pub fn collinear(a: Vec2, b: Vec2, c: Vec2) -> bool {
    orientation(a, b, c) == 0
}

/// A line `{ x : x·n = c }` with `n` a unit normal.
#[derive(Clone, Copy, Debug)]
pub struct NumLine {
    pub n: Vec2,
    pub c: f64,
}

impl NumLine {
    /// Line through `a` with normal `n`.
    pub fn through1(n: Vec2, a: Vec2) -> NumLine {
        NumLine { n, c: a.dot(n) }
    }

    /// Line through two points.
    pub fn through(a: Vec2, b: Vec2) -> NumLine {
        NumLine::through1((b - a).normalize().perp_rot(), a)
    }

    /// Direction of the line in half-turns, in `[0, 1)`.
    pub fn direction(&self) -> f64 {
        (self.n.direction() + 0.5).rem_euclid(1.0)
    }

    /// Distance from a point to the line.
    pub fn distance(&self, a: Vec2) -> f64 {
        (self.c - a.dot(self.n)).abs()
    }

    /// A 1-D coordinate of `a` projected along the line.
    pub fn position(&self, a: Vec2) -> f64 {
        -self.n.perp_rot().dot(a)
    }
}

pub fn perp_bisector(a: Vec2, b: Vec2) -> NumLine {
    NumLine::through1((b - a).normalize(), midpoint(a, b))
}

/// Intersection of two lines, or `None` if (near) parallel or degenerate.
pub fn intersect_ll(l1: &NumLine, l2: &NumLine) -> Option<Vec2> {
    // Solve [n1; n2] x = [c1; c2]. A line built from coincident points has a
    // NaN normal; `NaN < ATOM` is false, so the NaN case must be checked
    // explicitly or a NaN "intersection" would be returned.
    let det = l1.n.x * l2.n.y - l1.n.y * l2.n.x;
    if det.is_nan() || det.abs() < ATOM {
        return None;
    }
    let x = (l1.c * l2.n.y - l2.c * l1.n.y) / det;
    let y = (l1.n.x * l2.c - l2.n.x * l1.c) / det;
    Some(Vec2::new(x, y))
}

/// A circle with a center and radius.
#[derive(Clone, Copy, Debug)]
pub struct NumCircle {
    pub center: Vec2,
    pub r: f64,
}

impl NumCircle {
    pub fn through1(center: Vec2, a: Vec2) -> NumCircle {
        NumCircle {
            center,
            r: distance(a, center),
        }
    }

    /// Circumcircle of three points. Returns `None` if the points are collinear.
    pub fn through(a: Vec2, b: Vec2, c: Vec2) -> Option<NumCircle> {
        let center = intersect_ll(&perp_bisector(a, b), &perp_bisector(a, c))?;
        Some(NumCircle::through1(center, a))
    }

    /// Distance from a point to the circle.
    pub fn distance(&self, a: Vec2) -> f64 {
        (distance(self.center, a) - self.r).abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn orientation_basic() {
        assert_eq!(orientation(v(0.0, 0.0), v(1.0, 0.0), v(0.0, 1.0)), 1);
        assert_eq!(orientation(v(0.0, 0.0), v(0.0, 1.0), v(1.0, 0.0)), -1);
        assert_eq!(orientation(v(0.0, 0.0), v(1.0, 0.0), v(2.0, 0.0)), 0);
    }

    #[test]
    fn line_through_points() {
        let l = NumLine::through(v(0.0, 0.0), v(2.0, 0.0));
        assert!(l.distance(v(1.0, 0.0)) < ATOM);
        assert!(l.distance(v(1.0, 1.0)) > 0.5);
        // A horizontal line has direction 0 in half-turns.
        assert!((l.direction() - 0.0).abs() < 1e-9 || (l.direction() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn circumcircle() {
        let c = NumCircle::through(v(1.0, 0.0), v(-1.0, 0.0), v(0.0, 1.0)).unwrap();
        assert!(distance(c.center, v(0.0, 0.0)) < 1e-9);
        assert!((c.r - 1.0).abs() < 1e-9);
        assert!(c.distance(v(0.0, -1.0)) < 1e-9);
    }

    #[test]
    fn intersect() {
        let l1 = NumLine::through(v(0.0, 0.0), v(1.0, 0.0));
        let l2 = NumLine::through(v(0.0, 0.0), v(0.0, 1.0));
        let p = intersect_ll(&l1, &l2).unwrap();
        assert!(distance(p, v(0.0, 0.0)) < 1e-9);
    }

    #[test]
    fn degenerate_circle_is_none_not_nan() {
        // Coincident points give a NaN line normal; the intersection (and
        // therefore the circumcircle) must be None, never Some(NaN).
        let a = v(0.0, 0.0);
        let c = NumCircle::through(a, a, v(1.0, 0.0));
        assert!(c.is_none());
        let collinear = NumCircle::through(v(0.0, 0.0), v(1.0, 0.0), v(2.0, 0.0));
        assert!(collinear.is_none());
    }
}
