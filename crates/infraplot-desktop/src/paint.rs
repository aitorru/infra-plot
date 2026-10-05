//! Paints a diagram onto a gpui canvas in the `clean` look of the web editor
//! (`web/src/render2d/canvas2d.ts`): tinted tiles with line icons, rounded zones, crisp strokes
//! with filled arrowheads.

// Geometry reads best with the short names of the web renderer (p, t, z, r…).
#![allow(clippy::many_single_char_names)]

use gpui::{
    App, BorderStyle, Bounds, ContentMask, Font, FontWeight, Hsla, PathBuilder, Pixels,
    StrokeOptions, TextAlign, TextRun, TransformationMatrix, Window, font, point, px, quad, size,
};
use infraplot_model::catalog::{GRID, NODE_SIZE};
use infraplot_model::geometry::{Point, Rect, edge_path, node_box, point_along, zone_box};
use infraplot_model::{Arrow, Diagram, Edge, Line, Node, Note, StrokeStyle, Zone};
use lyon::tessellation::{LineCap, LineJoin};

use crate::icons::icon;
use crate::state::{Camera, Drag, Handle, selection_box};
use crate::theme::{Theme, color_or, hsla, mix};

pub const FONT_FAMILY: &str = "Inter";

/// Corner radius of orthogonal routes.
const CORNER: f64 = 10.0;

/// Everything the canvas needs for one frame, detached from the editor entity.
pub struct Scene {
    pub doc: Diagram,
    pub camera: Camera,
    pub selection: Option<String>,
    pub handles: Vec<Handle>,
    pub drag: Option<Drag>,
    pub connect_origin: Option<Point>,
    pub line_draft: Option<Vec<Point>>,
    pub cursor: Point,
    pub theme: &'static Theme,
}

struct Painter<'a> {
    origin: gpui::Point<Pixels>,
    camera: Camera,
    theme: &'static Theme,
    window: &'a mut Window,
    cx: &'a mut App,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Anchor {
    Start,
    Middle,
}

#[allow(clippy::cast_possible_truncation)] // screen coordinates fit in f32
fn f(v: f64) -> f32 {
    v as f32
}

impl Painter<'_> {
    fn screen(&self, p: Point) -> gpui::Point<Pixels> {
        let [x, y] = self.camera.to_screen(p);
        point(self.origin.x + px(f(x)), self.origin.y + px(f(y)))
    }

    fn rect(&self, r: &Rect) -> Bounds<Pixels> {
        let z = self.camera.zoom;
        Bounds::new(
            self.screen([r.x, r.y]),
            size(px(f(r.w * z)), px(f(r.h * z))),
        )
    }

    /// World length → pixels.
    fn len(&self, v: f64) -> Pixels {
        px(f(v * self.camera.zoom))
    }

    fn fill_rect(&mut self, r: &Rect, radius: f64, fill: Hsla, border: Option<(Hsla, f64)>) {
        let (bc, bw) = border.unwrap_or((gpui::transparent_black(), 0.0));
        let b = self.rect(r);
        let q = quad(b, self.len(radius), fill, px(f(bw)), bc, BorderStyle::Solid);
        self.window.paint_quad(q);
    }

    /// A rounded rectangle outline with an optional dash pattern (in pixels).
    fn stroke_rounded_rect(
        &mut self,
        r: &Rect,
        radius: f64,
        color: Hsla,
        width: f64,
        dash: &[f64],
    ) {
        let b = self.rect(r);
        let rr = self
            .len(radius)
            .min(b.size.width / 2.)
            .min(b.size.height / 2.);
        let (l, t, rt, bt) = (b.left(), b.top(), b.right(), b.bottom());
        let mut pb = Self::stroke_builder(width, dash);
        let radii = point(rr, rr);
        pb.move_to(point(l + rr, t));
        pb.line_to(point(rt - rr, t));
        pb.arc_to(radii, px(0.), false, true, point(rt, t + rr));
        pb.line_to(point(rt, bt - rr));
        pb.arc_to(radii, px(0.), false, true, point(rt - rr, bt));
        pb.line_to(point(l + rr, bt));
        pb.arc_to(radii, px(0.), false, true, point(l, bt - rr));
        pb.line_to(point(l, t + rr));
        pb.arc_to(radii, px(0.), false, true, point(l + rr, t));
        if let Ok(path) = pb.build() {
            self.window.paint_path(path, color);
        }
    }

    fn stroke_builder(width: f64, dash: &[f64]) -> PathBuilder {
        let opts = StrokeOptions::default()
            .with_line_width(f(width))
            .with_line_cap(LineCap::Round)
            .with_line_join(LineJoin::Round);
        let mut pb = PathBuilder::stroke(px(f(width))).with_style(gpui::PathStyle::Stroke(opts));
        if !dash.is_empty() {
            let d: Vec<Pixels> = dash.iter().map(|&v| px(f(v))).collect();
            pb = pb.dash_array(&d);
        }
        pb
    }

    /// A polyline with its corners rounded by up to `radius` (world units).
    fn polyline(&mut self, pts: &[Point], radius: f64, color: Hsla, width: f64, dash: &[f64]) {
        let Some(&first) = pts.first() else {
            return;
        };
        let mut pb = Self::stroke_builder(width, dash);
        pb.move_to(self.screen(first));
        for i in 1..pts.len() {
            let p = pts[i];
            let prev = pts[i - 1];
            let Some(&next) = pts.get(i + 1) else {
                pb.line_to(self.screen(p));
                break;
            };
            let l1 = (p[0] - prev[0]).hypot(p[1] - prev[1]);
            let l2 = (next[0] - p[0]).hypot(next[1] - p[1]);
            let k = radius.min(l1 / 2.0).min(l2 / 2.0);
            if k < 0.5 || l1 == 0.0 || l2 == 0.0 {
                pb.line_to(self.screen(p));
                continue;
            }
            let a = [
                p[0] + (prev[0] - p[0]) * k / l1,
                p[1] + (prev[1] - p[1]) * k / l1,
            ];
            let b = [
                p[0] + (next[0] - p[0]) * k / l2,
                p[1] + (next[1] - p[1]) * k / l2,
            ];
            pb.line_to(self.screen(a));
            pb.curve_to(self.screen(b), self.screen(p));
        }
        if let Ok(path) = pb.build() {
            self.window.paint_path(path, color);
        }
    }

    fn polygon(&mut self, pts: &[Point], color: Hsla) {
        let mut pb = PathBuilder::fill();
        let screen: Vec<_> = pts.iter().map(|&p| self.screen(p)).collect();
        pb.add_polygon(&screen, true);
        if let Ok(path) = pb.build() {
            self.window.paint_path(path, color);
        }
    }

    /// Text centred vertically on `at`; `size` is in world units.
    fn text(&mut self, s: &str, at: Point, size: f64, color: Hsla, weight: f32, anchor: Anchor) {
        self.text_with_bg(s, at, size, color, weight, anchor, None);
    }

    /// Like [`Self::text`], optionally on a rounded "pill" (fill, border, horizontal padding).
    /// Returns the text width in world units.
    #[allow(clippy::too_many_arguments)]
    fn text_with_bg(
        &mut self,
        s: &str,
        at: Point,
        size: f64,
        color: Hsla,
        weight: f32,
        anchor: Anchor,
        bg: Option<(Hsla, Option<Hsla>, f64, f64)>,
    ) -> f64 {
        if s.is_empty() || size * self.camera.zoom < 2.0 {
            return 0.0;
        }
        let font_size = self.len(size);
        let run = TextRun {
            len: s.len(),
            font: Font {
                weight: FontWeight(weight),
                ..font(FONT_FAMILY)
            },
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line =
            self.window
                .text_system()
                .shape_line(s.to_owned().into(), font_size, &[run], None);
        let width = f64::from(line.width) / self.camera.zoom;
        let x = match anchor {
            Anchor::Start => at[0],
            Anchor::Middle => at[0] - width / 2.0,
        };
        if let Some((fill, border, pad, height)) = bg {
            let r = Rect {
                x: x - pad,
                y: at[1] - height / 2.0,
                w: width + pad * 2.0,
                h: height,
            };
            self.fill_rect(&r, height / 2.0, fill, border.map(|c| (c, 1.0)));
        }
        let line_height = size * 1.25;
        let origin = self.screen([x, at[1] - line_height / 2.0]);
        line.paint(
            origin,
            self.len(line_height),
            TextAlign::Left,
            None,
            self.window,
            self.cx,
        )
        .ok();
        width
    }

    fn svg(&mut self, key: &'static str, data: &'static [u8], r: &Rect, color: Hsla) {
        let b = self.rect(r);
        self.window
            .paint_svg(
                b,
                key.into(),
                Some(data),
                TransformationMatrix::unit(),
                color,
                self.cx,
            )
            .ok();
    }
}

/// Dash pattern (world units) for a stroke `width` wide, used with round caps.
fn dash(style: StrokeStyle, width: f64) -> Vec<f64> {
    match style {
        StrokeStyle::Solid => Vec::new(),
        StrokeStyle::Dashed => vec![width * 4.0, width * 3.5],
        StrokeStyle::Dotted => vec![width * 0.4, width * 3.0],
    }
}

/// Moves the end of a polyline back by `by` (so a stroke doesn't poke through an arrow tip).
fn trim_end(pts: &mut [Point], by: f64) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let (last, prev) = (pts[n - 1], pts[n - 2]);
    let len = (last[0] - prev[0]).hypot(last[1] - prev[1]);
    if len > by {
        pts[n - 1] = [
            last[0] - (last[0] - prev[0]) * by / len,
            last[1] - (last[1] - prev[1]) * by / len,
        ];
    }
}

pub fn paint(scene: &Scene, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let t = scene.theme;
    window.paint_quad(gpui::fill(bounds, hsla(t.paper)));
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut p = Painter {
            origin: bounds.origin,
            camera: scene.camera,
            theme: t,
            window,
            cx,
        };
        grid(&mut p, bounds);
        for z in &scene.doc.zones {
            zone(&mut p, z);
        }
        for l in &scene.doc.lines {
            line(&mut p, l);
        }
        for e in &scene.doc.edges {
            if let Some(pts) = edge_path(&scene.doc, e) {
                edge(&mut p, e, &pts);
            }
        }
        for n in &scene.doc.nodes {
            node(&mut p, n);
        }
        for n in &scene.doc.notes {
            note(&mut p, n);
        }
        overlay(&mut p, scene);
    });
}

fn grid(p: &mut Painter, bounds: Bounds<Pixels>) {
    let z = p.camera.zoom;
    let mut step = GRID;
    while step * z < 10.0 {
        step *= 2.0;
    }
    let w = f64::from(bounds.size.width);
    let h = f64::from(bounds.size.height);
    let [x0, y0] = p.camera.to_world([0.0, 0.0]);
    let [x1, y1] = p.camera.to_world([w, h]);
    let color = hsla(p.theme.grid);
    let dot = px(2.);
    let mut x = (x0 / step).floor() * step;
    while x <= x1 {
        let mut y = (y0 / step).floor() * step;
        while y <= y1 {
            let s = p.screen([x, y]);
            p.window.paint_quad(quad(
                Bounds::new(s, size(dot, dot)),
                px(1.),
                color,
                px(0.),
                gpui::transparent_black(),
                BorderStyle::Solid,
            ));
            y += step;
        }
        x += step;
    }
}

fn zone(p: &mut Painter, z: &Zone) {
    let t = p.theme;
    let info = z.kind.info();
    let color = color_or(z.color.as_deref(), color_or(Some(info.color), 0xe9ecef));
    let style = z.style.unwrap_or(info.style);
    let r = zone_box(z);
    let width = 1.5;
    let border = mix(color, t.zone_stroke, if t.dark { 0.45 } else { 0.6 });
    p.fill_rect(&r, 12.0, hsla(t.surface(color)).opacity(0.45), None);
    let d: Vec<f64> = dash(style, width)
        .into_iter()
        .map(|v| v * p.camera.zoom)
        .collect();
    p.stroke_rounded_rect(&r, 12.0, hsla(border), width * p.camera.zoom.max(0.5), &d);
    p.text(
        &info.label.to_uppercase(),
        [z.x + 14.0, z.y + 17.0],
        10.0,
        hsla(t.muted),
        650.0,
        Anchor::Start,
    );
    p.text(
        &z.label,
        [z.x + 14.0, z.y + 35.0],
        15.0,
        hsla(t.ink),
        600.0,
        Anchor::Start,
    );
}

fn node(p: &mut Painter, n: &Node) {
    let t = p.theme;
    let info = n.kind.info();
    let color = color_or(n.color.as_deref(), color_or(Some(info.color), 0xdee2e6));
    let tile = t.surface(color);
    let s = NODE_SIZE;
    let inset = s * 0.06;
    let side = s - inset * 2.0;
    let r = Rect {
        x: n.x - side / 2.0,
        y: n.y - side / 2.0,
        w: side,
        h: side,
    };
    p.fill_rect(
        &r,
        s * 0.2,
        hsla(tile),
        Some((
            hsla(mix(tile, t.ink, 0.28)),
            1.5 * p.camera.zoom.clamp(0.5, 2.0),
        )),
    );
    if let Some(ic) = icon(n.kind) {
        let g = s * 0.6;
        let gr = Rect {
            x: n.x - g / 2.0,
            y: n.y - g / 2.0,
            w: g,
            h: g,
        };
        p.svg(ic.body_key, ic.body, &gr, hsla(mix(t.panel, color, 0.14)));
        p.svg(
            ic.strokes_key,
            ic.strokes,
            &gr,
            hsla(mix(color, t.ink, 0.72)),
        );
    }
    let b = node_box(n);
    p.text(
        &n.label,
        [n.x, b.y + b.h + 14.0],
        14.0,
        hsla(t.ink),
        500.0,
        Anchor::Middle,
    );
}

fn strokes(p: &mut Painter, pts: &[Point], style: StrokeStyle, arrow: Arrow, color: Hsla) {
    let width = 1.6;
    let head = 10.0;
    let at_end = matches!(arrow, Arrow::End | Arrow::Both);
    let at_start = matches!(arrow, Arrow::Start | Arrow::Both);
    let mut line = pts.to_vec();
    if at_end {
        trim_end(&mut line, head * 0.8);
    }
    if at_start {
        line.reverse();
        trim_end(&mut line, head * 0.8);
        line.reverse();
    }
    let z = p.camera.zoom;
    let d: Vec<f64> = dash(style, width).into_iter().map(|v| v * z).collect();
    // gpui drops level or plumb strokes thinner than about 1.3 px altogether, so straight
    // edges vanished when zoomed out: keep them at 1.5 px at least.
    p.polyline(&line, CORNER, color, (width * z).max(1.5), &d);
    let mut tip = |from: Point, to: Point| {
        let a = (to[1] - from[1]).atan2(to[0] - from[0]);
        let bx = to[0] - a.cos() * head;
        let by = to[1] - a.sin() * head;
        let px_ = a.sin() * head * 0.42;
        let py_ = -a.cos() * head * 0.42;
        p.polygon(&[to, [bx + px_, by + py_], [bx - px_, by - py_]], color);
    };
    let n = pts.len();
    if n >= 2 {
        if at_end {
            tip(pts[n - 2], pts[n - 1]);
        }
        if at_start {
            tip(pts[1], pts[0]);
        }
    }
}

fn edge(p: &mut Painter, e: &Edge, pts: &[Point]) {
    let t = p.theme;
    let color = hsla(color_or(e.color.as_deref(), t.ink));
    strokes(p, pts, e.style, e.arrow, color);
    if !e.label.is_empty() {
        let mid = point_along(pts, 0.5);
        p.text_with_bg(
            &e.label,
            mid,
            12.0,
            color,
            500.0,
            Anchor::Middle,
            Some((hsla(t.panel), Some(hsla(t.line)), 9.0, 22.0)),
        );
    }
}

fn line(p: &mut Painter, l: &Line) {
    let color = hsla(color_or(l.color.as_deref(), p.theme.ink));
    strokes(p, &l.points, l.style, l.arrow, color);
}

fn note(p: &mut Painter, n: &Note) {
    let size = n.size.px();
    let color = hsla(color_or(n.color.as_deref(), p.theme.ink));
    for (i, text) in n.text.split('\n').enumerate() {
        #[allow(clippy::cast_precision_loss)] // a handful of lines
        let y = n.y + size * 0.62 + i as f64 * size * 1.25;
        p.text(text, [n.x, y], size, color, 400.0, Anchor::Start);
    }
}

fn overlay(p: &mut Painter, scene: &Scene) {
    let t = p.theme;
    let accent = hsla(t.accent);
    let z = p.camera.zoom;
    let preview_dash = [6.0, 4.0];
    if let Some(id) = scene.selection.as_deref() {
        if let Some(b) = selection_box(&scene.doc, id) {
            let radius = if scene.doc.zones.iter().any(|zone| zone.id == id) {
                14.0
            } else {
                8.0
            };
            p.stroke_rounded_rect(&b.inflate(6.0), radius, accent, 1.5, &preview_dash);
        }
        let path = scene
            .doc
            .edges
            .iter()
            .find(|e| e.id == id)
            .and_then(|e| edge_path(&scene.doc, e))
            .or_else(|| {
                scene
                    .doc
                    .lines
                    .iter()
                    .find(|l| l.id == id)
                    .map(|l| l.points.clone())
            });
        if let Some(pts) = path {
            p.polyline(&pts, CORNER, accent.opacity(0.3), 8.0, &[]);
        }
        for h in &scene.handles {
            let r = Rect {
                x: h.at[0] - h.w / z / 2.0,
                y: h.at[1] - h.h / z / 2.0,
                w: h.w / z,
                h: h.h / z,
            };
            let radius = h.w.min(h.h) / z / 2.0;
            p.fill_rect(&r, radius, hsla(t.panel), Some((accent, 1.5)));
        }
    }
    match &scene.drag {
        Some(Drag::Zone { start, cur }) => {
            let r = Rect::from_corners(*start, *cur);
            p.stroke_rounded_rect(&r, 0.0, accent, 1.5, &preview_dash);
        }
        Some(Drag::Connect { cur, .. }) => {
            if let Some(from) = scene.connect_origin {
                p.polyline(&[from, *cur], 0.0, accent, 1.5, &preview_dash);
            }
        }
        Some(Drag::Line { start, cur }) => {
            p.polyline(&[*start, *cur], 0.0, accent, 1.5, &preview_dash);
        }
        _ => {}
    }
    if let Some(draft) = &scene.line_draft {
        let mut pts = draft.clone();
        pts.push(scene.cursor);
        p.polyline(&pts, 0.0, accent, 1.5, &preview_dash);
    }
}
