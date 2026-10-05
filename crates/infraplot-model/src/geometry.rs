//! Plane geometry shared by the editors: element boxes, edge routes and bends.
//!
//! Mirrors `web/src/model/geometry.ts`, so a document lays out the same in the browser and
//! in the desktop app.

use crate::catalog::{GRID, NODE_SIZE};
use crate::{Diagram, Edge, Node, Note, Route, Zone};

pub type Point = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    #[must_use]
    pub fn center(&self) -> Point {
        [self.x + self.w / 2.0, self.y + self.h / 2.0]
    }

    #[must_use]
    pub fn contains(&self, inner: &Rect) -> bool {
        inner.x >= self.x
            && inner.y >= self.y
            && inner.x + inner.w <= self.x + self.w
            && inner.y + inner.h <= self.y + self.h
    }

    #[must_use]
    pub fn contains_point(&self, [x, y]: Point) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.w && y <= self.y + self.h
    }

    #[must_use]
    pub fn inflate(&self, by: f64) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + by * 2.0,
            h: self.h + by * 2.0,
        }
    }

    #[must_use]
    pub fn union(&self, o: &Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect {
            x,
            y,
            w: (self.x + self.w).max(o.x + o.w) - x,
            h: (self.y + self.h).max(o.y + o.h) - y,
        }
    }

    /// The rectangle spanned by two corners, in any order.
    #[must_use]
    pub fn from_corners(a: Point, b: Point) -> Rect {
        Rect {
            x: a[0].min(b[0]),
            y: a[1].min(b[1]),
            w: (b[0] - a[0]).abs(),
            h: (b[1] - a[1]).abs(),
        }
    }
}

#[must_use]
pub fn snap(v: f64, enabled: bool) -> f64 {
    if enabled {
        (v / GRID).round() * GRID
    } else {
        v
    }
}

#[must_use]
pub fn snap_point(p: Point, enabled: bool) -> Point {
    [snap(p[0], enabled), snap(p[1], enabled)]
}

/// Room below a node's icon taken by its label.
pub const NODE_LABEL_HEIGHT: f64 = 28.0;

#[must_use]
pub fn node_box(n: &Node) -> Rect {
    Rect {
        x: n.x - NODE_SIZE / 2.0,
        y: n.y - NODE_SIZE / 2.0,
        w: NODE_SIZE,
        h: NODE_SIZE,
    }
}

#[must_use]
pub fn zone_box(z: &Zone) -> Rect {
    Rect {
        x: z.x,
        y: z.y,
        w: z.w,
        h: z.h,
    }
}

/// Approximate box of a note's text (the web editor uses the same estimate).
#[must_use]
pub fn note_box(n: &Note) -> Rect {
    let size = n.size.px();
    let lines: Vec<&str> = n.text.split('\n').collect();
    let longest = lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);
    #[allow(clippy::cast_precision_loss)] // line lengths are tiny
    Rect {
        x: n.x,
        y: n.y,
        w: longest as f64 * size * 0.55,
        h: lines.len() as f64 * size * 1.25,
    }
}

#[must_use]
pub fn target_box(doc: &Diagram, id: &str) -> Option<Rect> {
    if let Some(n) = doc.nodes.iter().find(|n| n.id == id) {
        return Some(node_box(n));
    }
    doc.zones.iter().find(|z| z.id == id).map(zone_box)
}

const EDGE_PAD: f64 = 6.0;

/// Where the ray from the centre of `b` towards `p` leaves `b` (grown by `pad`).
#[must_use]
pub fn clip_to_box(b: &Rect, p: Point, pad: f64) -> Point {
    let [cx, cy] = b.center();
    let dx = p[0] - cx;
    let dy = p[1] - cy;
    if dx == 0.0 && dy == 0.0 {
        return [cx, cy];
    }
    let hw = b.w / 2.0 + pad;
    let hh = b.h / 2.0 + pad;
    let tx = if dx == 0.0 {
        f64::INFINITY
    } else {
        hw / dx.abs()
    };
    let ty = if dy == 0.0 {
        f64::INFINITY
    } else {
        hh / dy.abs()
    };
    let t = tx.min(ty);
    if t >= 1.0 {
        return [cx, cy];
    }
    [cx + dx * t, cy + dy * t]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// How an orthogonal edge is laid out: the main axis it travels along and the span
/// (`lo` → `hi`, in that axis' coordinate) over which `bend` slides the middle segment. When
/// the boxes are apart along the main axis the span is the gap between their facing sides;
/// otherwise it runs between the centres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrthoLayout {
    pub axis: Axis,
    pub lo: f64,
    pub hi: f64,
    /// Centres of the `from` and `to` boxes.
    pub a: Point,
    pub b: Point,
    /// Whether the boxes are apart along the axis (`lo`/`hi` are then on their borders).
    pub apart: bool,
}

#[must_use]
pub fn ortho_layout(from: &Rect, to: &Rect) -> OrthoLayout {
    let a = from.center();
    let b = to.center();
    let gap_x = (to.x - (from.x + from.w)).max(from.x - (to.x + to.w));
    let gap_y = (to.y - (from.y + from.h)).max(from.y - (to.y + to.h));
    let axis = if gap_x > 0.0 && gap_y <= 0.0 {
        Axis::X
    } else if gap_y > 0.0 && gap_x <= 0.0 {
        Axis::Y
    } else if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() {
        Axis::X
    } else {
        Axis::Y
    };
    let (i, gap, half_a, half_b) = match axis {
        Axis::X => (0, gap_x, from.w / 2.0, to.w / 2.0),
        Axis::Y => (1, gap_y, from.h / 2.0, to.h / 2.0),
    };
    if gap > EDGE_PAD * 2.0 {
        let d = b[i] - a[i];
        let dir = if d == 0.0 { 1.0 } else { d.signum() };
        return OrthoLayout {
            axis,
            lo: a[i] + dir * (half_a + EDGE_PAD),
            hi: b[i] - dir * (half_b + EDGE_PAD),
            a,
            b,
            apart: true,
        };
    }
    OrthoLayout {
        axis,
        lo: a[i],
        hi: b[i],
        a,
        b,
        apart: false,
    }
}

/// The edge's bend clamped to 0..1, defaulting to the middle.
#[must_use]
pub fn bend_of(e: &Edge) -> f64 {
    match e.bend {
        Some(v) if v.is_finite() => v.clamp(0.0, 1.0),
        _ => 0.5,
    }
}

/// The bend fraction that puts the middle segment at `coord` (along the layout's axis).
#[must_use]
pub fn bend_at(layout: &OrthoLayout, coord: f64) -> f64 {
    let span = layout.hi - layout.lo;
    if span == 0.0 {
        return 0.5;
    }
    let t = ((coord - layout.lo) / span).clamp(0.0, 1.0);
    (t * 10000.0).round() / 10000.0
}

fn along(axis: Axis, m: f64, other: f64) -> Point {
    match axis {
        Axis::X => [m, other],
        Axis::Y => [other, m],
    }
}

/// The polyline an edge follows, already clipped to its endpoints' borders.
#[must_use]
#[allow(clippy::many_single_char_names)] // same names as geometry.ts
pub fn edge_path(doc: &Diagram, e: &Edge) -> Option<Vec<Point>> {
    let a = target_box(doc, &e.from)?;
    let b = target_box(doc, &e.to)?;
    let ca = a.center();
    let cb = b.center();
    if e.route != Route::Orthogonal {
        return Some(vec![
            clip_to_box(&a, cb, EDGE_PAD),
            clip_to_box(&b, ca, EDGE_PAD),
        ]);
    }
    let l = ortho_layout(&a, &b);
    let m = l.lo + (l.hi - l.lo) * bend_of(e);
    let j = match l.axis {
        Axis::X => 1,
        Axis::Y => 0,
    };
    let m0 = along(l.axis, m, ca[j]);
    let m1 = along(l.axis, m, cb[j]);
    if l.apart {
        let start = along(l.axis, l.lo, ca[j]);
        let end = along(l.axis, l.hi, cb[j]);
        return Some(simplify(&[start, m0, m1, end]));
    }
    Some(simplify(&[
        clip_to_box(&a, m0, EDGE_PAD),
        m0,
        m1,
        clip_to_box(&b, m1, EDGE_PAD),
    ]))
}

/// The middle (bendable) segment of an orthogonal edge, if it has one.
#[must_use]
#[allow(clippy::many_single_char_names)] // same names as geometry.ts
pub fn bend_segment(doc: &Diagram, e: &Edge) -> Option<(Point, Point, OrthoLayout)> {
    if e.route != Route::Orthogonal {
        return None;
    }
    let a = target_box(doc, &e.from)?;
    let b = target_box(doc, &e.to)?;
    let layout = ortho_layout(&a, &b);
    let m = layout.lo + (layout.hi - layout.lo) * bend_of(e);
    let j = match layout.axis {
        Axis::X => 1,
        Axis::Y => 0,
    };
    Some((
        along(layout.axis, m, layout.a[j]),
        along(layout.axis, m, layout.b[j]),
        layout,
    ))
}

/// Drops repeated points and middle points of straight runs.
#[must_use]
pub fn simplify(pts: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(pts.len());
    for &p in pts {
        if let Some(last) = out.last()
            && (last[0] - p[0]).abs() < 1e-6
            && (last[1] - p[1]).abs() < 1e-6
        {
            continue;
        }
        if out.len() >= 2 {
            let prev = out[out.len() - 2];
            let last = out[out.len() - 1];
            let cross =
                (last[0] - prev[0]) * (p[1] - prev[1]) - (last[1] - prev[1]) * (p[0] - prev[0]);
            let dot =
                (last[0] - prev[0]) * (p[0] - last[0]) + (last[1] - prev[1]) * (p[1] - last[1]);
            if cross.abs() < 1e-6 && dot >= 0.0 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentAxis {
    Horizontal,
    Vertical,
}

/// Whether segment p→q is horizontal, vertical or neither.
#[must_use]
#[allow(clippy::float_cmp)] // exact: points come from the grid or from the user
pub fn segment_axis(p: Point, q: Point) -> Option<SegmentAxis> {
    if p == q {
        None
    } else if p[1] == q[1] {
        Some(SegmentAxis::Horizontal)
    } else if p[0] == q[0] {
        Some(SegmentAxis::Vertical)
    } else {
        None
    }
}

/// `p` pulled onto the horizontal or vertical through `from`, whichever is closer.
#[must_use]
pub fn constrain_ortho(from: Point, p: Point) -> Point {
    if (p[0] - from[0]).abs() >= (p[1] - from[1]).abs() {
        [p[0], from[1]]
    } else {
        [from[0], p[1]]
    }
}

#[must_use]
pub fn midpoint(p: Point, q: Point) -> Point {
    [f64::midpoint(p[0], q[0]), f64::midpoint(p[1], q[1])]
}

#[must_use]
pub fn distance(p: Point, q: Point) -> f64 {
    (q[0] - p[0]).hypot(q[1] - p[1])
}

#[must_use]
pub fn path_length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| distance(w[0], w[1])).sum()
}

/// Point at fraction `t` (0..1) of the polyline's length.
#[must_use]
pub fn point_along(pts: &[Point], t: f64) -> Point {
    let mut remaining = path_length(pts) * t;
    for (i, w) in pts.windows(2).enumerate() {
        let seg = distance(w[0], w[1]);
        if remaining <= seg || i == pts.len() - 2 {
            let k = if seg == 0.0 {
                0.0
            } else {
                (remaining / seg).min(1.0)
            };
            return [
                w[0][0] + (w[1][0] - w[0][0]) * k,
                w[0][1] + (w[1][1] - w[0][1]) * k,
            ];
        }
        remaining -= seg;
    }
    pts.first().copied().unwrap_or([0.0, 0.0])
}

/// Distance from `p` to segment a→b.
#[must_use]
pub fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return distance(p, a);
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0);
    distance(p, [a[0] + dx * t, a[1] + dy * t])
}

/// Distance from `p` to the nearest segment of a polyline.
#[must_use]
pub fn distance_to_polyline(p: Point, pts: &[Point]) -> f64 {
    pts.windows(2)
        .map(|w| distance_to_segment(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

fn line_box(points: &[Point]) -> Option<Rect> {
    let first = points.first()?;
    let mut r = Rect {
        x: first[0],
        y: first[1],
        w: 0.0,
        h: 0.0,
    };
    for p in points {
        r = r.union(&Rect {
            x: p[0],
            y: p[1],
            w: 0.0,
            h: 0.0,
        });
    }
    Some(r)
}

/// Bounding box of everything in the document (node labels included).
#[must_use]
pub fn doc_bounds(doc: &Diagram) -> Option<Rect> {
    let boxes = doc
        .zones
        .iter()
        .map(zone_box)
        .chain(doc.nodes.iter().map(|n| {
            let b = node_box(n);
            Rect { h: b.h + 30.0, ..b }
        }))
        .chain(doc.notes.iter().map(note_box))
        .chain(doc.lines.iter().filter_map(|l| line_box(&l.points)));
    boxes.reduce(|a, b| a.union(&b))
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)] // exact grid values
mod tests {
    use super::*;
    use crate::{Arrow, StrokeStyle};

    fn doc() -> Diagram {
        Diagram::from_toml(
            r#"
version = 1
title = "t"

[[nodes]]
id = "a"
kind = "service"
x = 0
y = 0

[[nodes]]
id = "b"
kind = "database"
x = 300
y = 200
"#,
        )
        .unwrap()
    }

    fn edge(route: Route, bend: Option<f64>) -> Edge {
        Edge {
            id: "e".into(),
            from: "a".into(),
            to: "b".into(),
            label: String::new(),
            style: StrokeStyle::Solid,
            arrow: Arrow::End,
            route,
            bend,
            color: None,
        }
    }

    #[test]
    fn straight_edges_are_clipped_to_the_boxes() {
        let d = doc();
        let pts = edge_path(&d, &edge(Route::Straight, None)).unwrap();
        assert_eq!(pts.len(), 2);
        // Leaves `a` through its right side (36 + 6 of padding).
        assert!((pts[0][0] - 42.0).abs() < 1e-9);
        assert!((pts[1][0] - (300.0 - 42.0)).abs() < 1e-9);
    }

    #[test]
    fn orthogonal_edges_bend_in_the_gap() {
        let d = doc();
        // Apart along x (gap 228) and y (gap 128): the larger centre delta (x) wins.
        let pts = edge_path(&d, &edge(Route::Orthogonal, None)).unwrap();
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], [42.0, 0.0]);
        assert_eq!(pts[1], [150.0, 0.0]);
        assert_eq!(pts[2], [150.0, 200.0]);
        assert_eq!(pts[3], [258.0, 200.0]);
        let pts = edge_path(&d, &edge(Route::Orthogonal, Some(0.0))).unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!(pts[1], [42.0, 200.0]);
    }

    #[test]
    fn bend_round_trips() {
        let d = doc();
        let e = edge(Route::Orthogonal, Some(0.25));
        let (p, _, layout) = bend_segment(&d, &e).unwrap();
        assert!((bend_at(&layout, p[0]) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn simplify_drops_collinear_points() {
        let pts = simplify(&[
            [0.0, 0.0],
            [5.0, 0.0],
            [10.0, 0.0],
            [10.0, 0.0],
            [10.0, 5.0],
        ]);
        assert_eq!(pts, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]]);
    }

    #[test]
    fn point_along_walks_the_path() {
        let pts = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]];
        assert_eq!(point_along(&pts, 0.5), [10.0, 0.0]);
        assert_eq!(point_along(&pts, 0.75), [10.0, 5.0]);
        assert!((distance_to_polyline([5.0, 3.0], &pts) - 3.0).abs() < 1e-9);
    }
}
