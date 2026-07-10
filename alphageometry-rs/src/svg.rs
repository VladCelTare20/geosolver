//! SVG rendering of problem figures — a faithful, dependency-free replica of the
//! original AlphaGeometry figure view (`numericals.py::draw`), improved.
//!
//! The original AlphaGeometry drew, after solving, a matplotlib figure on a dark
//! canvas: white points with labels placed in the roomiest direction, white
//! lines through each collinear set, cyan circles, and the goal highlighted in
//! red (with angle wedges for angular goals). This module reproduces that look
//! as a single self-contained SVG string — no matplotlib, no external assets —
//! and adds, next to the drawing, a neatly typeset panel listing the
//! **constructions** (the hypotheses in natural notation) and the **goal**, so
//! the reader sees the figure and how it was built side by side.
//!
//! Beyond the original it also draws standard figure conventions that make the
//! diagram self-explaining: right-angle squares for `perp`, congruence tick
//! marks for equal segments, parallel chevrons for `para`, and colored angle
//! wedges for `eqangle`.
//!
//! Entry points: [`render`] (dark theme, with panel) and [`render_with`].

use crate::numerics::{distance, intersect_ll, NumCircle, NumLine, Vec2};
use crate::predicate::{PointId, Predicate, Problem};
use rustc_hash::{FxHashMap, FxHashSet};
use std::f64::consts::PI;
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// Layout constants
// ---------------------------------------------------------------------------

const FIG: f64 = 560.0; // side of the (square) drawing area
const PAD: f64 = 40.0; // outer margin around the drawing
const PANEL_W: f64 = 340.0; // width of the construction panel
const PANEL_LH: f64 = 23.0; // panel line height
const MARGIN: f64 = 0.055; // fraction of extra space around the figure (just enough for edge labels)
const MIN_SIDE: f64 = 0.45; // shortest card side, as a fraction of FIG (aspect-fitted cards)

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

/// Which color scheme to draw in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    /// AlphaGeometry's original look: near-black canvas, white points/lines.
    Dark,
    /// A print-friendly white canvas.
    Light,
}

struct Palette {
    bg: &'static str,
    fig_bg: &'static str,
    point: &'static str,
    label: &'static str,
    line: &'static str,
    seg: &'static str,
    circle: &'static str,
    goal: &'static str,
    /// Auxiliary constructions found by the search (points, dashed lines/circles).
    aux: &'static str,
    mark: &'static str,
    panel_bg: &'static str,
    panel_rule: &'static str,
    text: &'static str,
    heading: &'static str,
    muted: &'static str,
    /// Cycling colors for per-hypothesis angle highlights (never red).
    accents: &'static [&'static str],
}

const DARK: Palette = Palette {
    bg: "#0b0e13",
    fig_bg: "#0b0e13",
    point: "#ffffff",
    label: "#9be29b",
    line: "#e8ecf1",
    seg: "#5c6b7a",
    circle: "#37c8d6",
    goal: "#ff5a5f",
    aux: "#ffc24b",
    mark: "#c4ccd6",
    panel_bg: "#12161d",
    panel_rule: "#26303b",
    text: "#d7dee7",
    heading: "#8aa0b5",
    muted: "#6d7c8c",
    accents: &[
        "#4c9be8", "#57c26a", "#e8973a", "#b07be0", "#c98a5e", "#e07bc0", "#26c2cf", "#c9c94a",
    ],
};

const LIGHT: Palette = Palette {
    bg: "#ffffff",
    fig_bg: "#ffffff",
    point: "#16191d",
    label: "#12813a",
    line: "#465059",
    seg: "#aab6c1",
    circle: "#2a6fb0",
    goal: "#d81f26",
    aux: "#b45309",
    mark: "#5a6672",
    panel_bg: "#f6f8fa",
    panel_rule: "#dde3e9",
    text: "#2a2f36",
    heading: "#5b6570",
    muted: "#8a949e",
    accents: &[
        "#1f77b4", "#2ca02c", "#e07a1a", "#8a55c0", "#8c564b", "#c85ba8", "#1899a6", "#9a9a1e",
    ],
};

fn palette(t: Theme) -> Palette {
    match t {
        Theme::Dark => DARK,
        Theme::Light => LIGHT,
    }
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// How to render a figure.
pub struct FigureOptions {
    pub theme: Theme,
    /// Optional title shown atop the panel (e.g. the problem name).
    pub title: Option<String>,
    /// Draw the construction/goal side panel.
    pub panel: bool,
    /// If solving succeeded, a short status line (e.g. "Proven").
    pub status: Option<String>,
    /// Index of the first *auxiliary* point (one appended by the aux search).
    /// Points `aux_from..` — and every segment, line, and circle built from
    /// them — are drawn in the auxiliary style (amber, dashed) so the found
    /// constructions stand apart from the problem's givens.
    pub aux_from: Option<usize>,
}

impl Default for FigureOptions {
    fn default() -> FigureOptions {
        FigureOptions {
            theme: Theme::Dark,
            title: None,
            panel: true,
            status: None,
            aux_from: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Coordinate mapping
// ---------------------------------------------------------------------------

/// Maps problem coordinates into a screen rectangle (y grows downward), scaling
/// uniformly and centering, with a small margin like AG1's `plt.margins`.
struct Mapper {
    scale: f64,
    ox: f64,
    oy: f64,
}

impl Mapper {
    fn new(pts: &[Vec2], rx: f64, ry: f64, rw: f64, rh: f64) -> Mapper {
        // Fold only finite points into the bounding box; a stray inf/NaN
        // coordinate (accepted by the parser) must not corrupt the scale.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        let mut any = false;
        for p in pts {
            if p.x.is_finite() && p.y.is_finite() {
                any = true;
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
        }
        if !any {
            (x0, y0, x1, y1) = (0.0, 0.0, 1.0, 1.0);
        }
        let (dx, dy) = (x1 - x0, y1 - y0);
        // Fit each axis into its side of the rect (with margin) and take the
        // tighter scale — for a square rect this reduces to the old
        // max-dimension fit, for an aspect-matched rect both axes fill it.
        let sx = dx.max(1e-9) * (1.0 + 2.0 * MARGIN);
        let sy = dy.max(1e-9) * (1.0 + 2.0 * MARGIN);
        let scale = (rw / sx).min(rh / sy);
        // Center the data bbox inside the rect.
        let cx = (x0 + x1) / 2.0;
        let cy = (y0 + y1) / 2.0;
        let ox = rx + rw / 2.0 - cx * scale;
        let oy = ry + rh / 2.0 + cy * scale;
        Mapper { scale, ox, oy }
    }
    fn map(&self, p: Vec2) -> (f64, f64) {
        (self.ox + p.x * self.scale, self.oy - p.y * self.scale)
    }
    fn r(&self, r: f64) -> f64 {
        r * self.scale
    }
}

// ---------------------------------------------------------------------------
// Name display: uppercase with subscripts (a1 -> A₁, x' -> X′), like AG1.
// ---------------------------------------------------------------------------

fn disp(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else if let Some(d) = ch.to_digit(10) {
            // Unicode subscript digits ₀..₉
            out.push(char::from_u32(0x2080 + d).unwrap());
        } else if ch == '\'' {
            out.push('\u{2032}'); // prime
        } else {
            out.extend(ch.to_uppercase());
        }
    }
    out
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// ---------------------------------------------------------------------------
// cong "center + point" detection (unchanged contract from the old renderer)
// ---------------------------------------------------------------------------

/// Interpret a 4-point `cong` as "center + point on circle": if one point is
/// shared between the two segments, it is the center. Problems write all four
/// orders (`cong o a o b`, `cong a o b o`, `cong a o o d`, `cong o a d o`).
pub(crate) fn cong_center(p: &[PointId]) -> Option<(PointId, PointId)> {
    for (c1, t1, c2, t2) in [(0, 1, 2, 3), (0, 1, 3, 2), (1, 0, 2, 3), (1, 0, 3, 2)] {
        if p[c1] == p[c2] && p[t1] != p[t2] {
            return Some((p[c1], p[t1]));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Geometry extraction
// ---------------------------------------------------------------------------

/// Consecutive point pairs a predicate names (its "segments").
fn pred_pairs(pred: &Predicate) -> Vec<(PointId, PointId)> {
    match pred.name.as_str() {
        "cong" | "perp" | "para" | "eqangle" | "eqratio" | "distmeq" | "distseq" | "s_angle"
        | "aconst" | "angeq" | "rconst" | "acompute" => {
            pred.points.chunks_exact(2).map(|c| (c[0], c[1])).collect()
        }
        "coll" => {
            let p = &pred.points;
            (1..p.len()).map(|i| (p[0], p[i])).collect()
        }
        _ => Vec::new(),
    }
}

/// All circles to draw: `cyclic` sets and `cong`-center patterns.
fn detect_circles(problem: &Problem, aux_from: usize) -> Vec<(Vec2, f64, bool)> {
    let coord = |i: PointId| problem.points[i as usize].value;
    let pred_aux = |p: &Predicate| p.points.iter().any(|&i| (i as usize) >= aux_from);
    let max_r = max_circle_radius(problem);
    // Gather every candidate circle with its aux flag first, then dedup with
    // the non-aux ones ranked ahead: a circle asserted by both a given and an
    // auxiliary hypothesis is a *given* circle and must not be drawn dashed.
    let mut raw: Vec<(Vec2, f64, bool)> = Vec::new();
    let mut center_radii: FxHashMap<PointId, Vec<(f64, bool)>> = FxHashMap::default();
    for pred in &problem.preds {
        if pred.name == "cong" && pred.points.len() == 4 {
            if let Some((o, a)) = cong_center(&pred.points) {
                // An auxiliary equal-distance pair around a *given* centre
                // (e.g. a reflection's BP = BP′) reads better as ticked
                // segments than as a huge dashed circle — draw_cong_ticks
                // picks those up instead. A circle centred at an aux point
                // (a constructed circumcenter) stays a circle.
                if pred_aux(pred) && (o as usize) < aux_from {
                    continue;
                }
                center_radii
                    .entry(o)
                    .or_default()
                    .push((distance(coord(o), coord(a)), pred_aux(pred)));
            }
        }
        if pred.name == "cyclic" && pred.points.len() >= 3 {
            let (a, b, c) = (pred.points[0], pred.points[1], pred.points[2]);
            if let Some(circ) = NumCircle::through(coord(a), coord(b), coord(c)) {
                raw.push((circ.center, circ.r, pred_aux(pred)));
            }
        }
    }
    for (o, radii) in center_radii {
        for (r, aux) in radii {
            raw.push((coord(o), r, aux));
        }
    }
    raw.sort_by_key(|&(_, _, aux)| aux); // stable: given circles win the dedup
    let mut out: Vec<(Vec2, f64, bool)> = Vec::new();
    let mut seen = FxHashSet::default();
    for (c, r, aux) in raw {
        let key = (
            (c.x * 1e4).round() as i64,
            (c.y * 1e4).round() as i64,
            (r * 1e4).round() as i64,
        );
        if r.is_finite() && r > 1e-9 && r <= max_r && seen.insert(key) {
            out.push((c, r, aux));
        }
    }
    out
}

/// Extreme endpoints of a collinear set: the two farthest-apart points. This is
/// robust to repeated/coincident points (taking a direction from two identical
/// points would be NaN and silently drop the rest of the set). Returns `None`
/// if every point coincides.
fn line_extremes(pts: &[Vec2]) -> Option<(Vec2, Vec2)> {
    let mut best: Option<(f64, Vec2, Vec2)> = None;
    for i in 0..pts.len() {
        for j in (i + 1)..pts.len() {
            let d = distance(pts[i], pts[j]);
            if best.map_or(true, |(bd, _, _)| d > bd) {
                best = Some((d, pts[i], pts[j]));
            }
        }
    }
    best.filter(|&(d, _, _)| d > 1e-9).map(|(_, a, b)| (a, b))
}

/// A generous upper bound on a sensible circle radius for a figure: circles far
/// larger than the point cloud (e.g. the circumcircle of three near-collinear
/// points) are near-degenerate artifacts and are dropped rather than drawn.
fn max_circle_radius(problem: &Problem) -> f64 {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in &problem.points {
        if p.value.x.is_finite() && p.value.y.is_finite() {
            x0 = x0.min(p.value.x);
            y0 = y0.min(p.value.y);
            x1 = x1.max(p.value.x);
            y1 = y1.max(p.value.y);
        }
    }
    if !x0.is_finite() {
        return f64::INFINITY;
    }
    (x1 - x0).max(y1 - y0).max(1e-9) * 12.0
}

// ---------------------------------------------------------------------------
// SVG primitives
// ---------------------------------------------------------------------------

/// Whether a screen point is safe to emit (finite coordinates).
fn ok(p: (f64, f64)) -> bool {
    p.0.is_finite() && p.1.is_finite()
}

/// Whether a screen point lies within (a small margin around) the figure area —
/// used to suppress angle marks whose apex falls far off the drawing.
fn on_fig(p: (f64, f64)) -> bool {
    let (lo, hi) = (PAD - 60.0, PAD + FIG + 60.0);
    p.0 > lo && p.0 < hi && p.1 > lo && p.1 < hi
}

fn line(s: &mut String, a: (f64, f64), b: (f64, f64), color: &str, w: f64, extra: &str) {
    if !ok(a) || !ok(b) {
        return;
    }
    let _ = writeln!(
        s,
        r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{color}" stroke-width="{w}" stroke-linecap="round"{extra}/>"#,
        a.0, a.1, b.0, b.1
    );
}

fn circle(s: &mut String, c: (f64, f64), r: f64, stroke: &str, w: f64, extra: &str) {
    if !ok(c) || !r.is_finite() {
        return;
    }
    let _ = writeln!(
        s,
        r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="none" stroke="{stroke}" stroke-width="{w}"{extra}/>"#,
        c.0, c.1, r
    );
}

fn dot(s: &mut String, c: (f64, f64), r: f64, fill: &str, extra: &str) {
    if !ok(c) || !r.is_finite() {
        return;
    }
    let _ = writeln!(
        s,
        r#"<circle cx="{:.1}" cy="{:.1}" r="{:.1}" fill="{fill}"{extra}/>"#,
        c.0, c.1, r
    );
}

/// A filled angle wedge at screen point `h`, spanning the *minor* arc between the
/// directions to `p1` and `p2`, radius `r` px. Built as a fan polygon so there is
/// no SVG arc sweep-flag ambiguity in the y-down coordinate system.
fn wedge(s: &mut String, h: (f64, f64), p1: (f64, f64), p2: (f64, f64), r: f64, color: &str) {
    if !ok(h) || !ok(p1) || !ok(p2) {
        return;
    }
    let a1 = (p1.1 - h.1).atan2(p1.0 - h.0);
    let a2 = (p2.1 - h.1).atan2(p2.0 - h.0);
    let mut delta = a2 - a1;
    while delta > PI {
        delta -= 2.0 * PI;
    }
    while delta < -PI {
        delta += 2.0 * PI;
    }
    let k = 20;
    let mut d = format!("M{:.1},{:.1}", h.0, h.1);
    for i in 0..=k {
        let a = a1 + delta * (i as f64) / (k as f64);
        let _ = write!(d, " L{:.1},{:.1}", h.0 + r * a.cos(), h.1 + r * a.sin());
    }
    d.push('Z');
    let _ = writeln!(
        s,
        r#"<path d="{d}" fill="{color}" fill-opacity="0.30" stroke="{color}" stroke-width="1.2" stroke-opacity="0.9"/>"#
    );
}

/// A right-angle square in the corner at `h`, aligned to unit screen directions
/// `u`, `v` (each pointing along one of the perpendicular arms), size `t` px.
fn right_angle(s: &mut String, h: (f64, f64), u: (f64, f64), v: (f64, f64), t: f64, color: &str) {
    if !ok(h) || !ok(u) || !ok(v) {
        return;
    }
    let p1 = (h.0 + u.0 * t, h.1 + u.1 * t);
    let p2 = (h.0 + (u.0 + v.0) * t, h.1 + (u.1 + v.1) * t);
    let p3 = (h.0 + v.0 * t, h.1 + v.1 * t);
    let _ = writeln!(
        s,
        r#"<polyline points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="none" stroke="{color}" stroke-width="1.3"/>"#,
        p1.0, p1.1, p2.0, p2.1, p3.0, p3.1
    );
}

/// `n` short tick marks straddling the midpoint of screen segment a–b, drawn
/// perpendicular to it (the classic "equal length" convention).
fn seg_ticks(s: &mut String, a: (f64, f64), b: (f64, f64), n: u32, color: &str) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (ux, uy) = (dx / len, dy / len); // along
    let (px, py) = (-uy, ux); // perpendicular
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let gap = 4.0;
    let half = 5.0;
    let start = -(gap * (n as f64 - 1.0)) / 2.0;
    for i in 0..n {
        let off = start + gap * i as f64;
        let c = (mid.0 + ux * off, mid.1 + uy * off);
        line(
            s,
            (c.0 - px * half, c.1 - py * half),
            (c.0 + px * half, c.1 + py * half),
            color,
            1.4,
            "",
        );
    }
}

/// `n` chevrons (">") at the midpoint of screen segment a–b, pointing along it —
/// the "parallel" convention.
fn seg_chevrons(s: &mut String, a: (f64, f64), b: (f64, f64), n: u32, color: &str) {
    if !ok(a) || !ok(b) {
        return;
    }
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    let (ux, uy) = (dx / len, dy / len);
    let (px, py) = (-uy, ux);
    let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let size = 4.5;
    let gap = 4.5;
    let start = -(gap * (n as f64 - 1.0)) / 2.0;
    for i in 0..n {
        let off = start + gap * i as f64;
        let tip = (mid.0 + ux * (off + size), mid.1 + uy * (off + size));
        let l = (mid.0 + ux * off - px * size, mid.1 + uy * off - py * size);
        let r = (mid.0 + ux * off + px * size, mid.1 + uy * off + py * size);
        let _ = writeln!(
            s,
            r#"<polyline points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="none" stroke="{color}" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/>"#,
            l.0, l.1, tip.0, tip.1, r.0, r.1
        );
    }
}

// ---------------------------------------------------------------------------
// Smart label placement (port of AG1 numericals.naming_position)
// ---------------------------------------------------------------------------

/// Choose a screen-space offset direction for a point's label: the middle of the
/// widest angular gap among the directions "occupied" by incident lines and
/// circles, so the label lands in open space rather than on a stroke.
fn label_dir(
    p: Vec2,
    idx: PointId,
    m: &Mapper,
    lines: &[(Vec2, Vec2)],
    segs: &FxHashSet<(PointId, PointId)>,
    problem: &Problem,
    circles: &[(Vec2, f64)],
) -> (f64, f64) {
    let ps = m.map(p);
    let mut occupied: Vec<f64> = Vec::new();
    let push_dir = |occ: &mut Vec<f64>, to: (f64, f64)| {
        let a = (to.1 - ps.1).atan2(to.0 - ps.0);
        if a.is_finite() {
            occ.push(a);
        }
    };

    // Directions to neighbors sharing a drawn segment.
    for &(a, b) in segs {
        let other = if a == idx {
            Some(b)
        } else if b == idx {
            Some(a)
        } else {
            None
        };
        if let Some(o) = other {
            push_dir(&mut occupied, m.map(problem.points[o as usize].value));
        }
    }
    // Along any collinear line the point lies on: both directions.
    for &(lo, hi) in lines {
        let l = NumLine::through(lo, hi);
        if l.distance(p) < 1e-6 {
            let d = (hi - lo).normalize();
            push_dir(&mut occupied, m.map(p + d));
            push_dir(&mut occupied, m.map(p - d));
        }
    }
    // Tangent directions of circles through the point.
    for &(c, r) in circles {
        if (distance(c, p) - r).abs() < 1e-4 {
            let radial = (p - c).normalize();
            let tan = radial.perp_rot();
            push_dir(&mut occupied, m.map(p + tan));
            push_dir(&mut occupied, m.map(p - tan));
        }
    }

    if occupied.is_empty() {
        // up-right default (unit length, matching every structure-derived dir)
        return (
            std::f64::consts::FRAC_1_SQRT_2,
            -std::f64::consts::FRAC_1_SQRT_2,
        );
    }
    occupied.sort_by(|a, b| a.partial_cmp(b).unwrap());
    occupied.push(occupied[0] + 2.0 * PI);
    let mut best_gap = -1.0;
    let mut best_mid = occupied[0];
    for w in occupied.windows(2) {
        let gap = w[1] - w[0];
        if gap > best_gap {
            best_gap = gap;
            best_mid = w[0] + gap / 2.0;
        }
    }
    (best_mid.cos(), best_mid.sin())
}

/// Rough screen width of a displayed label (px) at font size `fs`. Uppercase
/// glyphs are ~0.62 em; subscript digits and primes are much narrower.
fn label_width(text: &str, fs: f64) -> f64 {
    let mut w = 0.0;
    for ch in text.chars() {
        let f = match ch {
            '\u{2080}'..='\u{2089}' => 0.42, // subscript digit
            '\u{2032}' => 0.34,              // prime
            _ => 0.62,
        };
        w += fs * f;
    }
    w.max(fs * 0.5)
}

/// Place point labels and emit them, running a light collision-relaxation pass
/// so no two names overlap and no name sits on a dot. Each label starts in the
/// roomiest direction ([`label_dir`]) and is then nudged: sprung toward that
/// ideal spot, repelled from overlapping labels and dots, kept clear of its own
/// point, and clamped onto the figure card. A short leader line is drawn for any
/// label a dense cluster forced far from its dot.
#[allow(clippy::too_many_arguments)]
fn place_labels(
    s: &mut String,
    problem: &Problem,
    m: &Mapper,
    coll_lines: &[(Vec2, Vec2)],
    seg_set: &FxHashSet<(PointId, PointId)>,
    circles: &[(Vec2, f64)],
    dots: &[(f64, f64)],
    pal: &Palette,
    aux_from: usize,
    fig_w: f64,
    fig_h: f64,
) {
    const FS: f64 = 19.0;
    const OFF: f64 = 17.0;

    struct Lab {
        text: String,
        anchor: (f64, f64), // the point's dot
        ideal: (f64, f64),  // preferred label centre
        pos: (f64, f64),    // current label centre
        hw: f64,            // half width  (box)
        hh: f64,            // half height (box)
        aux: bool,          // auxiliary point → label in the aux color
    }

    let mut labs: Vec<Lab> = Vec::new();
    for (i, p) in problem.points.iter().enumerate() {
        if p.name.starts_with('_') {
            continue;
        }
        let ps = m.map(p.value);
        if !ok(ps) {
            continue;
        }
        let (dx, dy) = label_dir(p.value, i as PointId, m, coll_lines, seg_set, problem, circles);
        let shown = disp(&p.name);
        let hw = label_width(&shown, FS) / 2.0 + 1.5;
        let ideal = (ps.0 + dx * OFF, ps.1 + dy * OFF);
        labs.push(Lab {
            text: xml_escape(&shown),
            anchor: ps,
            ideal,
            pos: ideal,
            hw,
            hh: FS * 0.5,
            aux: i >= aux_from,
        });
    }

    let n = labs.len();
    let (lo_x, hi_x) = (PAD, PAD + fig_w);
    let (lo_y, hi_y) = (PAD, PAD + fig_h);
    for _ in 0..90 {
        let mut push = vec![(0.0f64, 0.0f64); n];
        for i in 0..n {
            // spring toward the ideal offset position
            push[i].0 += (labs[i].ideal.0 - labs[i].pos.0) * 0.08;
            push[i].1 += (labs[i].ideal.1 - labs[i].pos.1) * 0.08;
            // keep clearance from the label's own dot
            let (ax, ay) = labs[i].anchor;
            let (mut ux, mut uy) = (labs[i].pos.0 - ax, labs[i].pos.1 - ay);
            let d = (ux * ux + uy * uy).sqrt();
            let want = OFF * 0.7;
            if d < want {
                if d < 1e-6 {
                    ux = 0.0;
                    uy = -1.0;
                } else {
                    ux /= d;
                    uy /= d;
                }
                push[i].0 += ux * (want - d) * 0.5;
                push[i].1 += uy * (want - d) * 0.5;
            }
        }
        // label ↔ label repulsion (axis-aligned box separation)
        for i in 0..n {
            for j in (i + 1)..n {
                let mut dx = labs[i].pos.0 - labs[j].pos.0;
                let mut dy = labs[i].pos.1 - labs[j].pos.1;
                let ox = (labs[i].hw + labs[j].hw) - dx.abs();
                let oy = (labs[i].hh + labs[j].hh) - dy.abs();
                if ox > 0.0 && oy > 0.0 {
                    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
                        dx = if i % 2 == 0 { 1.0 } else { -1.0 };
                        dy = 0.5;
                    }
                    if ox < oy {
                        let p = ox * 0.5 * if dx >= 0.0 { 1.0 } else { -1.0 };
                        push[i].0 += p;
                        push[j].0 -= p;
                    } else {
                        let p = oy * 0.5 * if dy >= 0.0 { 1.0 } else { -1.0 };
                        push[i].1 += p;
                        push[j].1 -= p;
                    }
                }
            }
        }
        // label ↔ dot repulsion (dots are fixed)
        for i in 0..n {
            for &(dxc, dyc) in dots {
                let dx = labs[i].pos.0 - dxc;
                let dy = labs[i].pos.1 - dyc;
                let ox = (labs[i].hw + 5.0) - dx.abs();
                let oy = (labs[i].hh + 5.0) - dy.abs();
                if ox > 0.0 && oy > 0.0 {
                    if ox < oy {
                        push[i].0 += ox * 0.5 * if dx >= 0.0 { 1.0 } else { -1.0 };
                    } else {
                        push[i].1 += oy * 0.5 * if dy >= 0.0 { 1.0 } else { -1.0 };
                    }
                }
            }
        }
        for i in 0..n {
            labs[i].pos.0 = (labs[i].pos.0 + push[i].0 * 0.7)
                .clamp(lo_x + labs[i].hw + 2.0, hi_x - labs[i].hw - 2.0);
            labs[i].pos.1 = (labs[i].pos.1 + push[i].1 * 0.7)
                .clamp(lo_y + labs[i].hh + 2.0, hi_y - labs[i].hh - 2.0);
        }
    }

    for lab in &labs {
        let (ax, ay) = lab.anchor;
        let dist = ((lab.pos.0 - ax).powi(2) + (lab.pos.1 - ay).powi(2)).sqrt();
        if dist > OFF * 1.9 {
            // leader stops at the label-box edge, not its centre
            let t = ((dist - (lab.hw + 2.0)) / dist).max(0.0);
            let _ = writeln!(
                s,
                r#"<line x1="{ax:.1}" y1="{ay:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="0.8" opacity="0.5"/>"#,
                ax + (lab.pos.0 - ax) * t,
                ay + (lab.pos.1 - ay) * t,
                pal.mark
            );
        }
        // A halo in the figure's background colour keeps the name legible where it
        // crosses a line or circle (paint-order draws the stroke first).
        let _ = writeln!(
            s,
            r#"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{FS}" font-weight="600" font-style="italic" paint-order="stroke" stroke="{}" stroke-width="4" stroke-linejoin="round" fill="{}">{}</text>"#,
            lab.pos.0,
            lab.pos.1 + FS * 0.34, // baseline from box centre
            pal.fig_bg,
            if lab.aux { pal.aux } else { pal.label },
            lab.text
        );
    }
}

// ---------------------------------------------------------------------------
// Goal highlight (port of AG1 numericals.highlight, red)
// ---------------------------------------------------------------------------

/// Draw the goal predicate emphasized in `color`, with angle wedges for angular
/// goals — mirroring the original figure view's red highlight.
fn highlight_goal(s: &mut String, problem: &Problem, m: &Mapper, pal: &Palette) {
    let Some(goal) = &problem.goal else { return };
    let coord = |i: PointId| problem.points[i as usize].value;
    let mp = |i: PointId| m.map(coord(i));
    let g = pal.goal;
    let p = &goal.points;
    let thick = 2.4;

    let seg = |s: &mut String, a: PointId, b: PointId| {
        line(s, mp(a), mp(b), g, thick, "");
    };
    // Wedge of the angle between segment ab and segment cd, drawn at their
    // intersection, oriented toward the far ends (as AG1 does).
    let ang = |s: &mut String, a: PointId, b: PointId, c: PointId, d: PointId| {
        let (la, lb, lc, ld) = (coord(a), coord(b), coord(c), coord(d));
        let x = intersect_ll(&NumLine::through(la, lb), &NumLine::through(lc, ld));
        let Some(x) = x else {
            seg(s, a, b);
            seg(s, c, d);
            return;
        };
        let xs = m.map(x);
        // If the two lines meet far outside the drawing (a common inscribed-angle
        // configuration), drawing rays from that point would slash giant strokes
        // across the figure — just emphasize the four segments instead.
        if !on_fig(xs) {
            seg(s, a, b);
            seg(s, c, d);
            return;
        }
        // pick, on each line, the endpoint farther from x as the ray direction
        let far = |x: Vec2, u: Vec2, v: Vec2| {
            if distance(x, u) >= distance(x, v) {
                u
            } else {
                v
            }
        };
        let e1 = far(x, la, lb);
        let e2 = far(x, lc, ld);
        line(s, xs, m.map(e1), g, thick, "");
        line(s, xs, m.map(e2), g, thick, "");
        wedge(s, xs, m.map(e1), m.map(e2), 26.0, g);
    };

    match goal.name.as_str() {
        "cyclic" if p.len() >= 3 => {
            if let Some(c) = NumCircle::through(coord(p[0]), coord(p[1]), coord(p[2])) {
                if c.r <= max_circle_radius(problem) {
                    circle(
                        s,
                        m.map(c.center),
                        m.r(c.r),
                        g,
                        2.2,
                        r#" stroke-dasharray="7 5""#,
                    );
                }
            }
        }
        "coll" => {
            let pts: Vec<Vec2> = p.iter().map(|&i| coord(i)).collect();
            if let Some((lo, hi)) = line_extremes(&pts) {
                line(s, m.map(lo), m.map(hi), g, thick, "");
            }
        }
        "perp" if p.len() == 4 => {
            seg(s, p[0], p[1]);
            seg(s, p[2], p[3]);
            draw_right_angle(s, coord(p[0]), coord(p[1]), coord(p[2]), coord(p[3]), m, g);
        }
        "para" | "cong" | "eqratio" | "rconst" if p.len() >= 4 => {
            for c in p.chunks_exact(2) {
                seg(s, c[0], c[1]);
            }
        }
        "midp" if p.len() == 3 => {
            seg(s, p[1], p[0]);
            seg(s, p[2], p[0]);
        }
        "eqangle" if p.len() == 8 => {
            ang(s, p[0], p[1], p[2], p[3]);
            ang(s, p[4], p[5], p[6], p[7]);
        }
        "aconst" | "acompute" | "s_angle" if p.len() >= 4 => {
            ang(s, p[0], p[1], p[2], p[3]);
        }
        _ => {
            for (a, b) in pred_pairs(goal) {
                if a != b {
                    seg(s, a, b);
                }
            }
        }
    }
}

/// Right-angle square at the intersection of lines ab and cd (if finite).
fn draw_right_angle(s: &mut String, a: Vec2, b: Vec2, c: Vec2, d: Vec2, m: &Mapper, color: &str) {
    let l1 = NumLine::through(a, b);
    let l2 = NumLine::through(c, d);
    let Some(x) = intersect_ll(&l1, &l2) else {
        return;
    };
    let xs = m.map(x);
    // Screen-space unit directions along each arm; sign chosen toward the
    // farther defining endpoint so the square sits inside the figure.
    let dir_screen = |x: Vec2, e1: Vec2, e2: Vec2| -> (f64, f64) {
        let e = if distance(x, e1) >= distance(x, e2) {
            e1
        } else {
            e2
        };
        let es = m.map(e);
        let (dx, dy) = (es.0 - xs.0, es.1 - xs.1);
        let n = (dx * dx + dy * dy).sqrt().max(1e-6);
        (dx / n, dy / n)
    };
    let u = dir_screen(x, a, b);
    let v = dir_screen(x, c, d);
    right_angle(s, xs, u, v, 11.0, color);
}

// ---------------------------------------------------------------------------
// Pretty-printing predicates for the panel (port of AG1 pretty.pretty_nl)
// ---------------------------------------------------------------------------

fn pretty_angle(a: &str, b: &str, c: &str, d: &str) -> String {
    let (mut a, mut b, mut c, mut d) = (a, b, c, d);
    if b == c || b == d {
        std::mem::swap(&mut a, &mut b);
    }
    if a == d {
        std::mem::swap(&mut c, &mut d);
    }
    if a == c {
        format!("\u{2220}{}{}{}", disp(b), disp(a), disp(d))
    } else {
        format!("\u{2220}({}{}, {}{})", disp(a), disp(b), disp(c), disp(d))
    }
}

fn const_str(pred: &Predicate) -> String {
    pred.constants
        .first()
        .map(|c| match (c.numer_i64(), c.denom_i64()) {
            (Some(n), Some(1)) => n.to_string(),
            (Some(n), Some(d)) => format!("{n}/{d}"),
            _ => format!("{c:?}"),
        })
        .unwrap_or_default()
}

/// Natural-notation form of a hypothesis/goal predicate.
/// The figure legend as plain text: the pretty-printed hypotheses and goal,
/// exactly as the SVG side panel typesets them — for callers (like the web UI)
/// that lay the legend out themselves instead of baking it into the drawing.
pub fn legend_lines(problem: &Problem) -> (Vec<String>, Option<String>) {
    let cons = problem.preds.iter().map(|p| pretty(problem, p)).collect();
    let goal = problem.goal.as_ref().map(|g| pretty(problem, g));
    (cons, goal)
}

fn pretty(problem: &Problem, pred: &Predicate) -> String {
    let n = |i: usize| disp(problem.point_name(pred.points[i]));
    let nm = pred.points.len();
    match pred.name.as_str() {
        "coll" if nm >= 2 => format!(
            "{} collinear",
            pred.points
                .iter()
                .map(|&i| disp(problem.point_name(i)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "cyclic" if nm >= 3 => format!(
            "{} concyclic",
            pred.points
                .iter()
                .map(|&i| disp(problem.point_name(i)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "midp" if nm == 3 => format!("{} = midpoint {}{}", n(0), n(1), n(2)),
        "cong" if nm == 4 => format!("{}{} = {}{}", n(0), n(1), n(2), n(3)),
        "perp" if nm == 4 => format!("{}{} \u{27c2} {}{}", n(0), n(1), n(2), n(3)),
        "para" if nm == 4 => format!("{}{} \u{2225} {}{}", n(0), n(1), n(2), n(3)),
        "eqangle" if nm == 8 => {
            let a = pred_names(problem, pred);
            format!(
                "{} = {}",
                pretty_angle(&a[0], &a[1], &a[2], &a[3]),
                pretty_angle(&a[4], &a[5], &a[6], &a[7])
            )
        }
        "eqratio" if nm == 8 => format!(
            "{}{} : {}{} = {}{} : {}{}",
            n(0),
            n(1),
            n(2),
            n(3),
            n(4),
            n(5),
            n(6),
            n(7)
        ),
        "aconst" | "s_angle" if nm == 4 => {
            let a = pred_names(problem, pred);
            format!(
                "{} = {}\u{00b0}",
                pretty_angle(&a[0], &a[1], &a[2], &a[3]),
                const_str(pred)
            )
        }
        "acompute" if nm == 4 => {
            let a = pred_names(problem, pred);
            pretty_angle(&a[0], &a[1], &a[2], &a[3])
        }
        "rconst" if nm == 4 => format!("{}{} : {}{} = {}", n(0), n(1), n(2), n(3), const_str(pred)),
        "simtri" | "simtri2" | "simtri*" if nm == 6 => format!(
            "\u{25b3}{}{}{} \u{223c} \u{25b3}{}{}{}",
            n(0),
            n(1),
            n(2),
            n(3),
            n(4),
            n(5)
        ),
        "contri" | "contri2" | "contri*" if nm == 6 => format!(
            "\u{25b3}{}{}{} \u{2245} \u{25b3}{}{}{}",
            n(0),
            n(1),
            n(2),
            n(3),
            n(4),
            n(5)
        ),
        "circle" if nm == 4 => {
            format!("{} = circumcenter {}{}{}", n(0), n(1), n(2), n(3))
        }
        _ => {
            // Fallback: name + display point names + constants.
            let mut parts = vec![pred.name.clone()];
            parts.extend(pred.points.iter().map(|&i| disp(problem.point_name(i))));
            let c = const_str(pred);
            if !c.is_empty() {
                parts.push(c);
            }
            parts.join(" ")
        }
    }
}

fn pred_names(problem: &Problem, pred: &Predicate) -> Vec<String> {
    pred.points
        .iter()
        .map(|&i| problem.point_name(i).to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Main render
// ---------------------------------------------------------------------------

/// Render a problem to a standalone SVG document with the default look
/// (dark theme, construction panel).
pub fn render(problem: &Problem) -> String {
    render_with(problem, &FigureOptions::default())
}

/// Render a problem with explicit [`FigureOptions`].
pub fn render_with(problem: &Problem, opts: &FigureOptions) -> String {
    let pal = palette(opts.theme);
    let coord = |i: PointId| problem.points[i as usize].value;
    let aux_from = opts.aux_from.unwrap_or(usize::MAX);
    let is_aux = |i: PointId| (i as usize) >= aux_from;

    // Figure area sits at (PAD, PAD), FIG on its long side.
    let circles = detect_circles(problem, aux_from);
    // Frame the view to the *points* — the content the reader actually cares
    // about — and to nothing else. Circles and extended lines are allowed to
    // spill past the figure edge, where the clip path below trims them cleanly
    // at the rounded border: an arc with no point on it carries no information,
    // so it must never cost zoom. The card itself adapts to the point cloud's
    // aspect ratio (long side FIG, short side proportional, floored at
    // MIN_SIDE·FIG) so an elongated figure isn't padded with dead bands.
    let finite_pts: Vec<Vec2> = problem
        .points
        .iter()
        .map(|p| p.value)
        .filter(|v| v.x.is_finite() && v.y.is_finite())
        .collect();
    let (fig_w, fig_h) = {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for v in &finite_pts {
            x0 = x0.min(v.x);
            y0 = y0.min(v.y);
            x1 = x1.max(v.x);
            y1 = y1.max(v.y);
        }
        let (dx, dy) = (x1 - x0, y1 - y0);
        if !x0.is_finite() || !(dx > 0.0) && !(dy > 0.0) {
            (FIG, FIG)
        } else if dx >= dy {
            (FIG, (FIG * (dy / dx).max(MIN_SIDE)).round())
        } else {
            ((FIG * (dx / dy).max(MIN_SIDE)).round(), FIG)
        }
    };
    let m = Mapper::new(&finite_pts, PAD, PAD, fig_w, fig_h);
    let coll_lines_aux: Vec<(Vec2, Vec2, bool)> = problem
        .preds
        .iter()
        .filter(|p| p.name == "coll")
        .filter_map(|p| {
            line_extremes(&p.points.iter().map(|&i| coord(i)).collect::<Vec<_>>())
                .map(|(lo, hi)| (lo, hi, p.points.iter().any(|&i| is_aux(i))))
        })
        .collect();
    let coll_lines: Vec<(Vec2, Vec2)> =
        coll_lines_aux.iter().map(|&(lo, hi, _)| (lo, hi)).collect();
    // Plain (center, r) view of the circles for label placement.
    let circles_xy: Vec<(Vec2, f64)> = circles.iter().map(|&(c, r, _)| (c, r)).collect();

    // All named segments (for skeleton + label placement), minus those already
    // covered by a full coll line.
    let mut seg_set: FxHashSet<(PointId, PointId)> = FxHashSet::default();
    for pred in &problem.preds {
        for (a, b) in pred_pairs(pred) {
            if a != b {
                seg_set.insert(if a < b { (a, b) } else { (b, a) });
            }
        }
    }

    // Canvas dimensions. The panel height mirrors draw_panel's exact vertical
    // advances so its content never overflows the card (which happens once the
    // panel is taller than the figure — many hypotheses plus title and status).
    let panel = opts.panel;
    let panel_x = PAD + fig_w + PAD;
    let n_cons = problem.preds.len();
    let mut panel_content = PAD + 34.0;
    if opts.title.is_some() {
        panel_content += 30.0;
    }
    if opts.status.is_some() {
        panel_content += 26.0;
    }
    panel_content += 24.0 + PANEL_LH * n_cons as f64; // CONSTRUCTION heading + rows
    if problem.goal.is_some() {
        panel_content += 12.0 + 24.0; // gap + GOAL heading + goal text baseline
    }
    panel_content += 8.0; // font descent
    let panel_h = panel_content + PAD;
    let width = if panel {
        panel_x + PANEL_W + PAD
    } else {
        PAD + fig_w + PAD
    };
    let height = (2.0 * PAD + fig_h).max(if panel { panel_h } else { 0.0 });

    let mut s = String::new();
    let _ = writeln!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="Helvetica, Arial, sans-serif">"#
    );
    let _ = writeln!(
        s,
        r#"<rect width="{width:.0}" height="{height:.0}" fill="{}"/>"#,
        pal.bg
    );
    // Figure background panel (subtle rounded card).
    let _ = writeln!(
        s,
        r#"<rect x="{:.0}" y="{:.0}" width="{fig_w:.0}" height="{fig_h:.0}" rx="10" fill="{}"/>"#,
        PAD, PAD, pal.fig_bg
    );

    // Clip all mapped geometry to the figure card, so a circle (or extended
    // line) trimmed by the tighter framing ends neatly at the rounded border
    // instead of bleeding across the background or the panel. Labels are drawn
    // afterwards, outside this clip, so a name near the edge is never cut.
    let _ = writeln!(
        s,
        r#"<defs><clipPath id="figclip"><rect x="{:.0}" y="{:.0}" width="{fig_w:.0}" height="{fig_h:.0}" rx="10"/></clipPath></defs>"#,
        PAD, PAD
    );
    let _ = writeln!(s, r#"<g clip-path="url(#figclip)">"#);

    // 1. Skeleton segments (thin) — the construction lines.
    for &(a, b) in &seg_set {
        // Skip if a full collinear line already covers this pair's direction and
        // both endpoints lie on it (avoids double-drawing).
        let (ca, cb) = (coord(a), coord(b));
        let covered = coll_lines.iter().any(|&(lo, hi)| {
            let l = NumLine::through(lo, hi);
            l.distance(ca) < 1e-6 && l.distance(cb) < 1e-6
        });
        if covered {
            continue;
        }
        if is_aux(a) || is_aux(b) {
            line(&mut s, m.map(ca), m.map(cb), pal.aux, 1.4, r#" stroke-dasharray="6 5""#);
        } else {
            line(&mut s, m.map(ca), m.map(cb), pal.seg, 1.3, "");
        }
    }

    // 2. Full lines through collinear sets (slightly extended).
    for &(lo, hi, aux) in &coll_lines_aux {
        let ext = (hi - lo) * 0.06;
        if aux {
            line(&mut s, m.map(lo - ext), m.map(hi + ext), pal.aux, 1.6, r#" stroke-dasharray="7 6""#);
        } else {
            line(&mut s, m.map(lo - ext), m.map(hi + ext), pal.line, 1.6, "");
        }
    }

    // 3. Circles.
    for &(c, r, aux) in &circles {
        if aux {
            circle(
                &mut s,
                m.map(c),
                m.r(r),
                pal.aux,
                1.6,
                r#" opacity="0.9" stroke-dasharray="7 6""#,
            );
        } else {
            circle(
                &mut s,
                m.map(c),
                m.r(r),
                pal.circle,
                1.6,
                r#" opacity="0.85""#,
            );
        }
    }

    // 4. Convention marks: right-angle squares, congruence ticks, parallels,
    //    and colored eqangle wedges (hypotheses).
    draw_perp_marks(&mut s, problem, &m, &pal);
    draw_cong_ticks(&mut s, problem, &m, &pal, aux_from);
    draw_para_marks(&mut s, problem, &m, &pal);
    draw_eqangle_marks(&mut s, problem, &m, &pal);

    // 5. Goal highlight (red, on top of structure but under points).
    highlight_goal(&mut s, problem, &m, &pal);

    let _ = writeln!(&mut s, "</g>"); // end figure clip — labels follow, unclipped

    // 6. Points (dots first, so every label sits on top of every dot). Dot
    //    centres double as fixed obstacles the labels are relaxed away from.
    let mut dots: Vec<(f64, f64)> = Vec::new();
    for (i, p) in problem.points.iter().enumerate() {
        let ps = m.map(p.value);
        if !ok(ps) {
            continue; // non-finite coordinate: skip dot and label entirely
        }
        let anon = p.name.starts_with('_');
        dot(
            &mut s,
            ps,
            if anon { 2.0 } else { 4.2 },
            if is_aux(i as PointId) { pal.aux } else { pal.point },
            r##" stroke="#00000022" stroke-width="0.5""##,
        );
        dots.push(ps);
    }

    // 7. Point labels, collision-relaxed so names never overlap each other or a
    //    dot — vital when a problem samples several points almost on top of one
    //    another (coincident cevian feet, a point and its foot, …).
    place_labels(
        &mut s, problem, &m, &coll_lines, &seg_set, &circles_xy, &dots, &pal, aux_from, fig_w,
        fig_h,
    );

    // 8. Panel.
    if panel {
        draw_panel(&mut s, problem, opts, &pal, panel_x, panel_content);
    }

    s.push_str("</svg>\n");
    s
}

// ---------------------------------------------------------------------------
// Convention marks driven by hypotheses
// ---------------------------------------------------------------------------

fn draw_perp_marks(s: &mut String, problem: &Problem, m: &Mapper, pal: &Palette) {
    let coord = |i: PointId| problem.points[i as usize].value;
    for pred in problem.preds.iter().filter(|p| p.name == "perp") {
        if pred.points.len() == 4 {
            let p = &pred.points;
            draw_right_angle(
                s,
                coord(p[0]),
                coord(p[1]),
                coord(p[2]),
                coord(p[3]),
                m,
                pal.mark,
            );
        }
    }
}

/// Group the segments of `name` predicates (`cong`/`para`) into equivalence
/// classes by the *asserted* relation: the two segments named by one predicate
/// are unioned (transitively), so only genuinely asserted-equal segments share a
/// mark — never two segments that merely happen to have equal length/direction
/// in the sampled figure. Classes are returned deterministically ordered by
/// their smallest segment, with singletons dropped.
fn relation_classes(
    problem: &Problem,
    name: &str,
    skip_cong_center: bool,
    aux_from: usize,
) -> Vec<Vec<(PointId, PointId)>> {
    let mut index: FxHashMap<(PointId, PointId), usize> = FxHashMap::default();
    let mut parent: Vec<usize> = Vec::new();
    let mut id_of = |k: (PointId, PointId), parent: &mut Vec<usize>| -> usize {
        *index.entry(k).or_insert_with(|| {
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    let ord = |a: PointId, b: PointId| if a < b { (a, b) } else { (b, a) };
    for pred in problem.preds.iter().filter(|p| p.name == name) {
        if pred.points.len() != 4 {
            continue;
        }
        if skip_cong_center {
            if let Some((o, _)) = cong_center(&pred.points) {
                // Centred pairs normally become circles, not ticks — except an
                // auxiliary pair around a given centre, which detect_circles
                // deliberately leaves to us (see the comment there).
                let aux = pred.points.iter().any(|&i| (i as usize) >= aux_from);
                if !(aux && (o as usize) < aux_from) {
                    continue;
                }
            }
        }
        let p = &pred.points;
        if p[0] == p[1] || p[2] == p[3] {
            continue;
        }
        let s1 = id_of(ord(p[0], p[1]), &mut parent);
        let s2 = id_of(ord(p[2], p[3]), &mut parent);
        let (r1, r2) = (find(&mut parent, s1), find(&mut parent, s2));
        parent[r1] = r2;
    }
    // Collect members per root.
    let mut by_root: FxHashMap<usize, Vec<(PointId, PointId)>> = FxHashMap::default();
    let pairs: Vec<((PointId, PointId), usize)> = index.into_iter().collect();
    for (seg, node) in pairs {
        let root = find(&mut parent, node);
        by_root.entry(root).or_default().push(seg);
    }
    let mut classes: Vec<Vec<(PointId, PointId)>> =
        by_root.into_values().filter(|v| v.len() >= 2).collect();
    for c in &mut classes {
        c.sort();
    }
    classes.sort_by_key(|c| c[0]);
    classes
}

fn draw_cong_ticks(s: &mut String, problem: &Problem, m: &Mapper, pal: &Palette, aux_from: usize) {
    let coord = |i: PointId| problem.points[i as usize].value;
    // At most three distinct tick counts (1/2/3); further classes are left
    // unmarked rather than reusing a count and implying a false congruence.
    for (idx, segs) in relation_classes(problem, "cong", true, aux_from)
        .iter()
        .take(3)
        .enumerate()
    {
        let n = idx as u32 + 1;
        for &(a, b) in segs {
            let aux = (a as usize) >= aux_from || (b as usize) >= aux_from;
            let color = if aux { pal.aux } else { pal.mark };
            seg_ticks(s, m.map(coord(a)), m.map(coord(b)), n, color);
        }
    }
}

fn draw_para_marks(s: &mut String, problem: &Problem, m: &Mapper, pal: &Palette) {
    let coord = |i: PointId| problem.points[i as usize].value;
    for (idx, segs) in relation_classes(problem, "para", false, usize::MAX)
        .iter()
        .take(3)
        .enumerate()
    {
        let n = idx as u32 + 1;
        for &(a, b) in segs {
            seg_chevrons(s, m.map(coord(a)), m.map(coord(b)), n, pal.mark);
        }
    }
}

fn draw_eqangle_marks(s: &mut String, problem: &Problem, m: &Mapper, pal: &Palette) {
    let coord = |i: PointId| problem.points[i as usize].value;
    let eqs: Vec<&Predicate> = problem
        .preds
        .iter()
        .filter(|p| p.name == "eqangle" && p.points.len() == 8)
        .collect();
    // Avoid clutter: only annotate when there are a handful.
    if eqs.len() > 4 {
        return;
    }
    for (i, pred) in eqs.iter().enumerate() {
        let color = pal.accents[i % pal.accents.len()];
        let p = &pred.points;
        for chunk in [(p[0], p[1], p[2], p[3]), (p[4], p[5], p[6], p[7])] {
            let (a, b, c, d) = chunk;
            let x = intersect_ll(
                &NumLine::through(coord(a), coord(b)),
                &NumLine::through(coord(c), coord(d)),
            );
            let Some(x) = x else { continue };
            let xs = m.map(x);
            // Skip when the angle's apex (the two lines' meeting point) is off the
            // drawing — a detached wedge floating in the margin only confuses.
            if !on_fig(xs) {
                continue;
            }
            let far = |u: Vec2, v: Vec2| {
                if distance(x, u) >= distance(x, v) {
                    u
                } else {
                    v
                }
            };
            let e1 = far(coord(a), coord(b));
            let e2 = far(coord(c), coord(d));
            wedge(s, xs, m.map(e1), m.map(e2), 20.0, color);
        }
    }
}

// ---------------------------------------------------------------------------
// Construction panel
// ---------------------------------------------------------------------------

fn draw_panel(
    s: &mut String,
    problem: &Problem,
    opts: &FigureOptions,
    pal: &Palette,
    x: f64,
    content_h: f64,
) {
    let w = PANEL_W;
    // The box hugs its content (top-aligned beside the figure) instead of
    // stretching to the figure's full height and leaving dead space.
    let _ = writeln!(
        s,
        r#"<rect x="{x:.0}" y="{PAD:.0}" width="{w:.0}" height="{:.0}" rx="10" fill="{}" stroke="{}" stroke-width="1"/>"#,
        content_h - PAD,
        pal.panel_bg,
        pal.panel_rule
    );
    let tx = x + 22.0;
    let mut y = PAD + 34.0;

    // Title.
    if let Some(t) = &opts.title {
        let _ = writeln!(
            s,
            r#"<text x="{tx:.1}" y="{y:.1}" font-size="18" font-weight="700" fill="{}">{}</text>"#,
            pal.text,
            xml_escape(t)
        );
        y += 30.0;
    }
    if let Some(st) = &opts.status {
        let _ = writeln!(
            s,
            r#"<text x="{tx:.1}" y="{y:.1}" font-size="13" fill="{}">{} ✓</text>"#,
            pal.label,
            xml_escape(st)
        );
        y += 26.0;
    }

    // Constructions heading.
    let heading = |s: &mut String, y: f64, txt: &str| {
        let _ = writeln!(
            s,
            r#"<text x="{tx:.1}" y="{y:.1}" font-size="12" font-weight="700" letter-spacing="1.2" fill="{}">{}</text>"#,
            pal.heading, txt
        );
        let _ = writeln!(
            s,
            r#"<line x1="{tx:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1"/>"#,
            y + 8.0,
            x + w - 22.0,
            y + 8.0,
            pal.panel_rule
        );
    };

    heading(s, y, "CONSTRUCTION");
    y += 24.0;
    let aux_from = opts.aux_from.unwrap_or(usize::MAX);
    for pred in &problem.preds {
        // Hypotheses introduced by an auxiliary construction take the aux
        // color, matching the dashed amber elements in the drawing.
        let aux = pred.points.iter().any(|&i| (i as usize) >= aux_from);
        let _ = writeln!(
            s,
            r#"<text x="{tx:.1}" y="{y:.1}" font-size="14" fill="{}"><tspan fill="{}">{} </tspan>{}</text>"#,
            if aux { pal.aux } else { pal.text },
            if aux { pal.aux } else { pal.muted },
            if aux { "+" } else { "•" },
            xml_escape(&pretty(problem, pred))
        );
        y += PANEL_LH;
    }

    // Goal.
    if let Some(goal) = &problem.goal {
        y += 12.0;
        heading(s, y, "GOAL");
        y += 24.0;
        let _ = writeln!(
            s,
            r#"<text x="{tx:.1}" y="{y:.1}" font-size="15" font-weight="600" fill="{}">{}</text>"#,
            pal.goal,
            xml_escape(&pretty(problem, goal))
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Problem;

    fn sample() -> Problem {
        Problem::parse(
            "a@0.0_0.0 = ; b@2.0_0.0 = ; m@1.0_0.0 = ; o@1.0_0.5 = \
             coll a b m, cong o a o b ? cong m a m b",
        )
        .unwrap()
    }

    #[test]
    fn renders_points_lines_circles_goal() {
        let svg = render(&sample());
        assert!(svg.starts_with("<svg"));
        assert!(svg.trim_end().ends_with("</svg>"));
        for name in ["A", "B", "M", "O"] {
            assert!(svg.contains(&format!(">{name}</text>")), "missing {name}");
        }
        // dark theme line/circle/goal colors present
        assert!(svg.contains(DARK.line));
        assert!(svg.contains(DARK.circle));
        assert!(svg.contains(DARK.goal));
        // panel headings
        assert!(svg.contains("CONSTRUCTION"));
        assert!(svg.contains("GOAL"));
    }

    #[test]
    fn light_theme_switches_palette() {
        let opts = FigureOptions {
            theme: Theme::Light,
            ..Default::default()
        };
        let svg = render_with(&sample(), &opts);
        assert!(svg.contains(LIGHT.bg));
        assert!(svg.contains(LIGHT.circle));
    }

    #[test]
    fn no_panel_is_narrower() {
        let with = render(&sample());
        let opts = FigureOptions {
            panel: false,
            ..Default::default()
        };
        let without = render_with(&sample(), &opts);
        assert!(without.len() < with.len());
        assert!(!without.contains("CONSTRUCTION"));
    }

    #[test]
    fn pretty_forms() {
        let p = sample();
        // midp-style and cong come out in natural notation
        let cong = p.preds.iter().find(|x| x.name == "cong").unwrap();
        assert!(pretty(&p, cong).contains('=') || pretty(&p, cong).contains('O'));
        let goal = p.goal.as_ref().unwrap();
        assert_eq!(pretty(&p, goal), "MA = MB");
    }

    #[test]
    fn xml_escaping_names() {
        let p = Problem::parse("a<@0_0 = ; b@1_0 = coll a< b ? coll a< b").unwrap();
        let svg = render(&p);
        assert!(!svg.contains(">a<<"), "unescaped name leaked");
    }

    #[test]
    fn non_finite_coords_never_emit_nan() {
        // The parser accepts `inf`/`NaN` coordinate tokens; the figure must
        // still be valid SVG with no `NaN`/`inf` in any attribute.
        let p = Problem::parse("p@inf_0 = ; q@1_1 = ; r@2_0 = coll p q r ? coll p q r").unwrap();
        let svg = render(&p);
        assert!(!svg.contains("NaN"), "NaN leaked into SVG");
        assert!(!svg.contains("\"inf"), "inf leaked into SVG");
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn duplicate_collinear_point_keeps_the_line() {
        // Two coincident leading points must not drop the genuinely distinct
        // third point (line_extremes robustness).
        let pts = [
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(5.0, 0.0),
        ];
        let (a, b) = line_extremes(&pts).expect("a real line survives");
        assert!(distance(a, b) > 4.0, "extremes span the full segment");
    }

    #[test]
    fn cong_ticks_do_not_conflate_independent_congruences() {
        // Two independent unit segments (never asserted congruent to each other)
        // must not receive the same tick count. AB=CD and EF=GH with all four
        // the same length: two classes, tick counts 1 and 2 — never all-equal.
        let p = Problem::parse(
            "a@0_0 = ; b@1_0 = ; c@0_1 = ; d@1_1 = ; e@0_2 = ; f@1_2 = ; g@0_3 = ; h@1_3 = \
             cong a b c d, cong e f g h ? coll a b",
        )
        .unwrap();
        let classes = relation_classes(&p, "cong", true, usize::MAX);
        assert_eq!(classes.len(), 2, "two independent congruence classes");
        assert!(classes.iter().all(|c| c.len() == 2));
    }

    #[test]
    fn transitive_congruence_is_one_class() {
        // AB=CD and CD=EF ⇒ all three share a class (one tick count).
        let p = Problem::parse(
            "a@0_0 = ; b@1_0 = ; c@0_1 = ; d@1_1 = ; e@0_2 = ; f@1_2 = \
             cong a b c d, cong c d e f ? coll a b",
        )
        .unwrap();
        let classes = relation_classes(&p, "cong", true, usize::MAX);
        assert_eq!(classes.len(), 1);
        assert_eq!(classes[0].len(), 3);
    }
}
