//! Editor state and pointer interaction, independent of gpui so it can be unit-tested.
//!
//! Follows the web editor (`web/src/state/store.ts` and the input half of
//! `web/src/render2d/canvas2d.ts`): same tools, drags, snapping and undo semantics.

use std::collections::HashSet;
use std::path::PathBuf;

use infraplot_model::catalog::{GRID, NODE_SIZE};
use infraplot_model::geometry::{
    Axis, NODE_LABEL_HEIGHT, Point, Rect, SegmentAxis, bend_segment, constrain_ortho, distance,
    distance_to_polyline, doc_bounds, edge_path, midpoint, node_box, note_box, segment_axis, snap,
    snap_point, zone_box,
};
use infraplot_model::ops::ElementType;
use infraplot_model::{Diagram, NodeKind, ZoneKind};

const HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Hand,
    Zone(ZoneKind),
    Connect,
    Line,
    Note,
    Node(NodeKind),
}

impl Tool {
    pub fn hint(self) -> &'static str {
        match self {
            Self::Select => "Click to select · drag to move · Space/middle button to pan",
            Self::Hand => "Drag to pan",
            Self::Zone(_) => "Drag to draw a zone (a click makes a 320×220 one)",
            Self::Connect => "Drag from a node or zone to another",
            Self::Line => {
                "Drag for a segment, or click points and finish with Enter / double-click"
            }
            Self::Note => "Click to place a note",
            Self::Node(_) => "Click to place the node",
        }
    }
}

/// Screen = world × zoom + (x, y), relative to the canvas' top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

pub const MIN_ZOOM: f64 = 0.1;
pub const MAX_ZOOM: f64 = 4.0;

impl Camera {
    pub fn to_world(self, p: Point) -> Point {
        [(p[0] - self.x) / self.zoom, (p[1] - self.y) / self.zoom]
    }

    pub fn to_screen(self, p: Point) -> Point {
        [p[0] * self.zoom + self.x, p[1] * self.zoom + self.y]
    }

    /// Zooms by `factor` keeping the world point under screen point `at` in place.
    pub fn zoom_at(&mut self, factor: f64, at: Point) {
        let zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let k = zoom / self.zoom;
        self.x = at[0] - (at[0] - self.x) * k;
        self.y = at[1] - (at[1] - self.y) * k;
        self.zoom = zoom;
    }

    /// Centres `bounds` in a `viewport`-sized canvas, at most at 200 %.
    pub fn fit(bounds: Option<Rect>, viewport: Point) -> Self {
        let Some(b) = bounds.filter(|_| viewport[0] > 0.0) else {
            return Self {
                x: viewport[0] / 2.0,
                y: viewport[1] / 2.0,
                zoom: 1.0,
            };
        };
        let pad = 80.0;
        let zoom = ((viewport[0] - pad * 2.0) / b.w.max(1.0))
            .min((viewport[1] - pad * 2.0) / b.h.max(1.0))
            .clamp(MIN_ZOOM, 2.0);
        let c = b.center();
        Self {
            x: viewport[0] / 2.0 - c[0] * zoom,
            y: viewport[1] / 2.0 - c[1] * zoom,
            zoom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    Nw,
    Ne,
    Sw,
    Se,
}

impl Corner {
    pub const ALL: [Self; 4] = [Self::Nw, Self::Ne, Self::Sw, Self::Se];

    pub fn of(self, r: &Rect) -> Point {
        match self {
            Self::Nw => [r.x, r.y],
            Self::Ne => [r.x + r.w, r.y],
            Self::Sw => [r.x, r.y + r.h],
            Self::Se => [r.x + r.w, r.y + r.h],
        }
    }

    fn west(self) -> bool {
        matches!(self, Self::Nw | Self::Sw)
    }

    fn north(self) -> bool {
        matches!(self, Self::Nw | Self::Ne)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    Corner(Corner),
    Bend,
    Vertex(usize),
    Segment(usize),
}

/// A draggable handle on the current selection; `w`/`h` are in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handle {
    pub kind: HandleKind,
    pub at: Point,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Handle(HandleKind),
    Element(String, ElementType),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Pan {
        start: Point,
        camera: Camera,
    },
    Move {
        id: String,
        last: Point,
        moved: bool,
        contents: HashSet<String>,
    },
    Resize {
        id: String,
        corner: Corner,
        orig: Rect,
        start: Point,
        moved: bool,
    },
    Bend {
        id: String,
        moved: bool,
    },
    Vertex {
        id: String,
        index: usize,
        moved: bool,
    },
    Segment {
        id: String,
        index: usize,
        orig: Vec<Point>,
        start: Point,
        moved: bool,
    },
    Zone {
        start: Point,
        cur: Point,
    },
    Connect {
        from: String,
        cur: Point,
    },
    Line {
        start: Point,
        cur: Point,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub shift: bool,
    /// Disables grid snapping.
    pub alt: bool,
}

/// What the view should do after a pointer event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Move keyboard focus to the label field of the selection.
    pub focus_label: bool,
}

pub struct Editor {
    pub doc: Diagram,
    /// File the document was opened from / saved to.
    pub path: Option<PathBuf>,
    pub dirty: bool,
    undo: Vec<Diagram>,
    redo: Vec<Diagram>,
    pub selection: Option<String>,
    pub tool: Tool,
    pub camera: Camera,
    pub drag: Option<Drag>,
    /// Points of a polyline being drawn click by click.
    pub line_draft: Option<Vec<Point>>,
    /// Snapped pointer position, in world units.
    pub cursor: Point,
    /// Space held: drags pan.
    pub space: bool,
    /// Canvas size in pixels, from the last paint.
    pub viewport: Point,
}

impl Editor {
    pub fn new(doc: Diagram, path: Option<PathBuf>) -> Self {
        Self {
            doc,
            path,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            selection: None,
            tool: Tool::Select,
            camera: Camera::default(),
            drag: None,
            line_draft: None,
            cursor: [0.0, 0.0],
            space: false,
            viewport: [0.0, 0.0],
        }
    }

    // ------------------------------------------------------------ history

    /// Records the current doc so the next mutation can be undone.
    pub fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Mutates the doc without touching history (e.g. mid-drag).
    pub fn mutate<R>(&mut self, f: impl FnOnce(&mut Diagram) -> R) -> R {
        self.dirty = true;
        f(&mut self.doc)
    }

    /// A single undoable change.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut Diagram) -> R) -> R {
        self.checkpoint();
        self.mutate(f)
    }

    /// Replaces the whole document (open / new).
    pub fn load(&mut self, doc: Diagram, path: Option<PathBuf>) {
        self.doc = doc;
        self.path = path;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.selection = None;
        self.tool = Tool::Select;
        self.drag = None;
        self.line_draft = None;
        self.fit();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(doc) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, doc));
            self.after_restore();
        }
    }

    pub fn redo(&mut self) {
        if let Some(doc) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, doc));
            self.after_restore();
        }
    }

    fn after_restore(&mut self) {
        self.dirty = true;
        if let Some(sel) = &self.selection
            && self.doc.element_type(sel).is_none()
        {
            self.selection = None;
        }
    }

    // ------------------------------------------------------------ commands

    pub fn selected_type(&self) -> Option<ElementType> {
        self.selection
            .as_deref()
            .and_then(|id| self.doc.element_type(id))
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.line_draft = None;
        self.drag = None;
    }

    pub fn delete_selection(&mut self) {
        if let Some(id) = self.selection.take() {
            self.edit(|d| d.delete_element(&id));
        }
    }

    pub fn duplicate_selection(&mut self) {
        let Some(id) = self.selection.clone() else {
            return;
        };
        self.checkpoint();
        match self.mutate(|d| d.duplicate_element(&id)) {
            Some(copy) => self.selection = Some(copy),
            // Edges can't be duplicated: drop the checkpoint we just took.
            None => {
                self.undo.pop();
            }
        }
    }

    /// Esc: cancels a draft, then the tool, then the selection.
    pub fn cancel(&mut self) {
        if self.line_draft.is_some() || self.drag.is_some() {
            self.line_draft = None;
            self.drag = None;
        } else if self.tool != Tool::Select {
            self.tool = Tool::Select;
        } else {
            self.selection = None;
        }
    }

    pub fn fit(&mut self) {
        self.camera = Camera::fit(doc_bounds(&self.doc), self.viewport);
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.camera
            .zoom_at(factor, [self.viewport[0] / 2.0, self.viewport[1] / 2.0]);
    }

    /// Ends the polyline being drawn click by click.
    pub fn finish_line(&mut self, commit: bool) {
        let mut pts = self.line_draft.take().unwrap_or_default();
        // A double-click adds the same point twice.
        pts.dedup();
        if commit && pts.len() >= 2 {
            self.commit_line(pts);
        }
    }

    fn commit_line(&mut self, pts: Vec<Point>) {
        let id = self.edit(|d| d.add_line(pts));
        self.selection = Some(id);
        self.tool = Tool::Select;
    }

    // ------------------------------------------------------------ hit testing

    /// Handles of the current selection, top-most last.
    pub fn handles(&self) -> Vec<Handle> {
        let mut out = Vec::new();
        let Some(id) = self.selection.as_deref() else {
            return out;
        };
        match self.doc.element_type(id) {
            Some(ElementType::Zone) => {
                if let Some(z) = self.doc.zones.iter().find(|z| z.id == id) {
                    let b = zone_box(z);
                    for c in Corner::ALL {
                        out.push(Handle {
                            kind: HandleKind::Corner(c),
                            at: c.of(&b),
                            w: 10.0,
                            h: 10.0,
                        });
                    }
                }
            }
            Some(ElementType::Edge) => {
                if let Some(e) = self.doc.edges.iter().find(|e| e.id == id)
                    && let Some((p, q, layout)) = bend_segment(&self.doc, e)
                    && distance(p, q) > 1.0
                {
                    // The middle segment runs across the main axis and slides along it.
                    let across = layout.axis == Axis::X;
                    out.push(Handle {
                        kind: HandleKind::Bend,
                        at: midpoint(p, q),
                        w: if across { 8.0 } else { 22.0 },
                        h: if across { 22.0 } else { 8.0 },
                    });
                }
            }
            Some(ElementType::Line) => {
                if let Some(l) = self.doc.lines.iter().find(|l| l.id == id) {
                    for (i, w) in l.points.windows(2).enumerate() {
                        let Some(axis) = segment_axis(w[0], w[1]) else {
                            continue;
                        };
                        // Leave short segments to the vertex handles.
                        if distance(w[0], w[1]) * self.camera.zoom < 36.0 {
                            continue;
                        }
                        let horizontal = axis == SegmentAxis::Horizontal;
                        out.push(Handle {
                            kind: HandleKind::Segment(i),
                            at: midpoint(w[0], w[1]),
                            w: if horizontal { 18.0 } else { 8.0 },
                            h: if horizontal { 8.0 } else { 18.0 },
                        });
                    }
                    for (i, &p) in l.points.iter().enumerate() {
                        out.push(Handle {
                            kind: HandleKind::Vertex(i),
                            at: p,
                            w: 10.0,
                            h: 10.0,
                        });
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// What is under world point `p`, top-most first: handles, notes, nodes, edges, lines,
    /// zones.
    pub fn hit(&self, p: Point) -> Option<Hit> {
        let z = self.camera.zoom;
        // A few extra pixels make the small handles easier to grab.
        let slack = 3.0 / z;
        for h in self.handles().iter().rev() {
            if (p[0] - h.at[0]).abs() <= h.w / z / 2.0 + slack
                && (p[1] - h.at[1]).abs() <= h.h / z / 2.0 + slack
            {
                return Some(Hit::Handle(h.kind));
            }
        }
        let el = |id: &str, t| Some(Hit::Element(id.to_owned(), t));
        for n in self.doc.notes.iter().rev() {
            if note_box(n).contains_point(p) {
                return el(&n.id, ElementType::Note);
            }
        }
        for n in self.doc.nodes.iter().rev() {
            let b = node_box(n);
            let b = Rect {
                h: b.h + NODE_LABEL_HEIGHT,
                ..b
            };
            if b.contains_point(p) {
                return el(&n.id, ElementType::Node);
            }
        }
        let tolerance = 8.0 / z;
        for e in self.doc.edges.iter().rev() {
            if let Some(pts) = edge_path(&self.doc, e)
                && distance_to_polyline(p, &pts) <= tolerance
            {
                return el(&e.id, ElementType::Edge);
            }
        }
        for l in self.doc.lines.iter().rev() {
            if distance_to_polyline(p, &l.points) <= tolerance {
                return el(&l.id, ElementType::Line);
            }
        }
        for zone in self.doc.zones.iter().rev() {
            if zone_box(zone).contains_point(p) {
                return el(&zone.id, ElementType::Zone);
            }
        }
        None
    }

    // ------------------------------------------------------------ pointer

    pub fn pointer_down(
        &mut self,
        screen: Point,
        button: Button,
        mods: Mods,
        click_count: usize,
    ) -> Outcome {
        let world = self.camera.to_world(screen);
        let snapped = snap_point(world, !mods.alt);
        let mut outcome = Outcome::default();

        if button == Button::Middle || self.space || self.tool == Tool::Hand {
            self.drag = Some(Drag::Pan {
                start: screen,
                camera: self.camera,
            });
            return outcome;
        }
        if button != Button::Left {
            return outcome;
        }
        if click_count >= 2 {
            return self.double_click(world);
        }
        let hit = self.hit(world);

        match self.tool {
            Tool::Select => match hit {
                Some(Hit::Handle(kind)) => self.grab_handle(kind, world),
                Some(Hit::Element(id, _)) => {
                    self.selection = Some(id.clone());
                    let contents = self.doc.zone_contents(&id);
                    self.drag = Some(Drag::Move {
                        id,
                        last: snapped,
                        moved: false,
                        contents,
                    });
                }
                None => self.selection = None,
            },
            Tool::Node(kind) => {
                let id = self.edit(|d| d.add_node(kind, snapped));
                self.selection = Some(id);
                self.tool = Tool::Select;
            }
            Tool::Zone(_) => {
                self.drag = Some(Drag::Zone {
                    start: snapped,
                    cur: snapped,
                });
            }
            Tool::Connect => {
                if let Some(Hit::Element(id, ElementType::Node | ElementType::Zone)) = hit {
                    self.drag = Some(Drag::Connect {
                        from: id,
                        cur: world,
                    });
                }
            }
            Tool::Line => match &mut self.line_draft {
                Some(draft) => {
                    let p = match draft.last() {
                        Some(&last) if mods.shift => constrain_ortho(last, snapped),
                        _ => snapped,
                    };
                    draft.push(p);
                }
                None => {
                    self.drag = Some(Drag::Line {
                        start: snapped,
                        cur: snapped,
                    });
                }
            },
            Tool::Note => {
                let id = self.edit(|d| d.add_note(snapped, "Text"));
                self.selection = Some(id);
                self.tool = Tool::Select;
                outcome.focus_label = true;
            }
            Tool::Hand => {}
        }
        outcome
    }

    fn double_click(&mut self, world: Point) -> Outcome {
        self.drag = None;
        if self.line_draft.is_some() {
            self.finish_line(true);
            return Outcome::default();
        }
        if let Some(Hit::Element(id, _)) = self.hit(world) {
            self.selection = Some(id);
            return Outcome { focus_label: true };
        }
        Outcome::default()
    }

    fn grab_handle(&mut self, kind: HandleKind, world: Point) {
        let Some(id) = self.selection.clone() else {
            return;
        };
        self.drag = match (kind, self.doc.element_type(&id)) {
            (HandleKind::Corner(corner), Some(ElementType::Zone)) => {
                let orig = self.doc.zones.iter().find(|z| z.id == id).map(zone_box);
                orig.map(|orig| Drag::Resize {
                    id,
                    corner,
                    orig,
                    start: world,
                    moved: false,
                })
            }
            (HandleKind::Bend, Some(ElementType::Edge)) => Some(Drag::Bend { id, moved: false }),
            (HandleKind::Vertex(index), Some(ElementType::Line)) => Some(Drag::Vertex {
                id,
                index,
                moved: false,
            }),
            (HandleKind::Segment(index), Some(ElementType::Line)) => {
                let orig = self
                    .doc
                    .lines
                    .iter()
                    .find(|l| l.id == id)
                    .map(|l| l.points.clone());
                orig.map(|orig| Drag::Segment {
                    id,
                    index,
                    orig,
                    start: world,
                    moved: false,
                })
            }
            _ => None,
        };
    }

    /// Takes a checkpoint the first time a drag changes the document.
    fn first_move(&mut self, moved: bool) {
        if !moved {
            self.checkpoint();
        }
        self.dirty = true;
    }

    /// Returns whether anything visible changed.
    #[allow(clippy::too_many_lines, clippy::float_cmp)] // one arm per drag kind; exact grid values
    pub fn pointer_move(&mut self, screen: Point, mods: Mods) -> bool {
        let world = self.camera.to_world(screen);
        let snapped = snap_point(world, !mods.alt);
        let draft_end = self.line_draft.as_ref().and_then(|d| d.last().copied());
        // Shift keeps the segment being drawn horizontal or vertical.
        self.cursor = match draft_end {
            Some(end) if mods.shift => constrain_ortho(end, snapped),
            _ => snapped,
        };
        let Some(mut drag) = self.drag.take() else {
            return self.line_draft.is_some();
        };
        match &mut drag {
            Drag::Pan { start, camera } => {
                self.camera = Camera {
                    x: camera.x + screen[0] - start[0],
                    y: camera.y + screen[1] - start[1],
                    ..*camera
                };
            }
            Drag::Move {
                id,
                last,
                moved,
                contents,
            } => {
                let (dx, dy) = (snapped[0] - last[0], snapped[1] - last[1]);
                if dx != 0.0 || dy != 0.0 {
                    self.first_move(*moved);
                    *moved = true;
                    *last = snapped;
                    // Shift moves a zone on its own, leaving its contents behind.
                    let empty = HashSet::new();
                    let contents = if mods.shift { &empty } else { &*contents };
                    self.doc.move_element(id, dx, dy, contents);
                }
            }
            Drag::Resize {
                id,
                corner,
                orig,
                start,
                moved,
            } => {
                let dx = snap(world[0] - start[0], !mods.alt);
                let dy = snap(world[1] - start[1], !mods.alt);
                let mut b = *orig;
                if corner.west() {
                    b.x = orig.x + dx.min(orig.w - GRID);
                    b.w = orig.w - (b.x - orig.x);
                } else {
                    b.w = (orig.w + dx).max(GRID);
                }
                if corner.north() {
                    b.y = orig.y + dy.min(orig.h - GRID);
                    b.h = orig.h - (b.y - orig.y);
                } else {
                    b.h = (orig.h + dy).max(GRID);
                }
                self.first_move(*moved);
                *moved = true;
                self.doc.resize_zone(id, b);
            }
            Drag::Bend { id, moved } => {
                if self.doc.bend_target(id, world, !mods.alt).is_some() {
                    self.first_move(*moved);
                    *moved = true;
                    self.doc.bend_edge_to(id, world, !mods.alt);
                }
            }
            Drag::Vertex { id, index, moved } => {
                let line = self.doc.lines.iter().find(|l| l.id == *id);
                if let Some(cur) = line.and_then(|l| l.points.get(*index).copied()) {
                    let points = &line.map(|l| l.points.clone()).unwrap_or_default();
                    let neighbour = index
                        .checked_sub(1)
                        .and_then(|i| points.get(i))
                        .or_else(|| points.get(*index + 1))
                        .copied();
                    let p = match neighbour {
                        Some(n) if mods.shift => constrain_ortho(n, snapped),
                        _ => snapped,
                    };
                    if p != cur {
                        self.first_move(*moved);
                        *moved = true;
                        self.doc.move_line_vertex(id, *index, p);
                    }
                }
            }
            Drag::Segment {
                id,
                index,
                orig,
                start,
                moved,
            } => {
                if let (Some(&p), Some(&q)) = (orig.get(*index), orig.get(*index + 1)) {
                    let axis = segment_axis(p, q);
                    let dx = if axis == Some(SegmentAxis::Horizontal) {
                        0.0
                    } else {
                        snap(p[0] + world[0] - start[0], !mods.alt) - p[0]
                    };
                    let dy = if axis == Some(SegmentAxis::Vertical) {
                        0.0
                    } else {
                        snap(p[1] + world[1] - start[1], !mods.alt) - p[1]
                    };
                    self.first_move(*moved);
                    *moved = true;
                    self.doc.move_line_segment(id, *index, orig, dx, dy);
                }
            }
            Drag::Zone { cur, .. } => *cur = self.cursor,
            Drag::Line { start, cur } => {
                *cur = if mods.shift {
                    constrain_ortho(*start, snapped)
                } else {
                    snapped
                };
            }
            Drag::Connect { cur, .. } => *cur = world,
        }
        self.drag = Some(drag);
        true
    }

    pub fn pointer_up(&mut self, screen: Point) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        let world = self.camera.to_world(screen);
        match drag {
            Drag::Zone { start, cur } => {
                let mut r = Rect::from_corners(start, cur);
                if r.w < GRID * 2.0 || r.h < GRID * 2.0 {
                    r = Rect {
                        x: start[0],
                        y: start[1],
                        w: 320.0,
                        h: 220.0,
                    };
                }
                let kind = match self.tool {
                    Tool::Zone(k) => k,
                    _ => ZoneKind::Generic,
                };
                let id = self.edit(|d| d.add_zone(kind, r));
                self.selection = Some(id);
                self.tool = Tool::Select;
            }
            Drag::Connect { from, .. } => {
                if let Some(Hit::Element(to, ElementType::Node | ElementType::Zone)) =
                    self.hit(world)
                    && to != from
                {
                    let id = self.edit(|d| d.add_edge(&from, &to));
                    self.selection = id;
                }
            }
            Drag::Line { start, cur } => {
                if distance(start, cur) >= GRID {
                    self.commit_line(vec![start, cur]);
                } else {
                    // A click: start a multi-point polyline, finished with Enter / double-click.
                    self.line_draft = Some(vec![start]);
                }
            }
            _ => {}
        }
    }

    /// Wheel / touchpad scroll: pans, or zooms around the pointer with Ctrl.
    pub fn wheel(&mut self, screen: Point, delta: Point, zoom: bool) {
        if zoom {
            self.camera.zoom_at((delta[1] * 0.01).exp(), screen);
        } else {
            self.camera.x += delta[0];
            self.camera.y += delta[1];
        }
    }

    /// Where the dragged connection starts, for the preview.
    pub fn connect_origin(&self, from: &str) -> Option<Point> {
        if let Some(n) = self.doc.nodes.iter().find(|n| n.id == from) {
            return Some([n.x, n.y]);
        }
        self.doc
            .zones
            .iter()
            .find(|z| z.id == from)
            .map(|z| zone_box(z).center())
    }
}

/// Selection outline of an element, in world units.
pub fn selection_box(doc: &Diagram, id: &str) -> Option<Rect> {
    match doc.element_type(id)? {
        ElementType::Node => doc.nodes.iter().find(|n| n.id == id).map(|n| {
            let b = node_box(n);
            Rect {
                h: NODE_SIZE + NODE_LABEL_HEIGHT,
                ..b
            }
        }),
        ElementType::Zone => doc.zones.iter().find(|z| z.id == id).map(zone_box),
        ElementType::Note => doc.notes.iter().find(|n| n.id == id).map(note_box),
        ElementType::Edge | ElementType::Line => None,
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)] // exact grid values
mod tests {
    use super::*;

    fn editor() -> Editor {
        let mut e = Editor::new(Diagram::new("t"), None);
        e.viewport = [800.0, 600.0];
        e
    }

    const NO: Mods = Mods {
        shift: false,
        alt: false,
    };

    #[test]
    fn placing_and_dragging_a_node() {
        let mut e = editor();
        e.set_tool(Tool::Node(NodeKind::Database));
        e.pointer_down([105.0, 98.0], Button::Left, NO, 1);
        assert_eq!(e.tool, Tool::Select);
        assert_eq!(e.doc.nodes[0].x, 100.0);
        assert_eq!(e.selection.as_deref(), Some("database-1"));

        e.pointer_down([100.0, 100.0], Button::Left, NO, 1);
        e.pointer_move([161.0, 139.0], NO);
        e.pointer_up([161.0, 139.0]);
        assert_eq!((e.doc.nodes[0].x, e.doc.nodes[0].y), (160.0, 140.0));
        e.undo();
        assert_eq!((e.doc.nodes[0].x, e.doc.nodes[0].y), (100.0, 100.0));
        e.undo();
        assert!(e.doc.nodes.is_empty());
        assert_eq!(e.selection, None);
        e.redo();
        assert_eq!(e.doc.nodes.len(), 1);
    }

    #[test]
    fn connecting_two_nodes() {
        let mut e = editor();
        e.edit(|d| {
            d.add_node(NodeKind::Service, [0.0, 0.0]);
            d.add_node(NodeKind::Database, [300.0, 0.0]);
        });
        e.set_tool(Tool::Connect);
        e.pointer_down([0.0, 0.0], Button::Left, NO, 1);
        e.pointer_move([300.0, 10.0], NO);
        e.pointer_up([300.0, 10.0]);
        assert_eq!(e.doc.edges.len(), 1);
        assert_eq!(e.doc.edges[0].from, "service-1");
        assert_eq!(e.doc.edges[0].to, "database-1");
        // The edge itself is clickable halfway.
        assert_eq!(
            e.hit([150.0, 2.0]),
            Some(Hit::Element("edge-1".into(), ElementType::Edge))
        );
    }

    #[test]
    fn zones_by_drag_or_click_and_resizing() {
        let mut e = editor();
        e.set_tool(Tool::Zone(ZoneKind::Vpc));
        e.pointer_down([0.0, 0.0], Button::Left, NO, 1);
        e.pointer_move([200.0, 120.0], NO);
        e.pointer_up([200.0, 120.0]);
        assert_eq!((e.doc.zones[0].w, e.doc.zones[0].h), (200.0, 120.0));
        // Grab the south-east handle.
        e.pointer_down([201.0, 119.0], Button::Left, NO, 1);
        assert!(matches!(e.drag, Some(Drag::Resize { .. })));
        e.pointer_move([260.0, 160.0], NO);
        e.pointer_up([260.0, 160.0]);
        assert_eq!((e.doc.zones[0].w, e.doc.zones[0].h), (260.0, 160.0));

        e.set_tool(Tool::Zone(ZoneKind::Subnet));
        e.pointer_down([400.0, 400.0], Button::Left, NO, 1);
        e.pointer_up([400.0, 400.0]);
        // Bigger zones go first, so smaller ones are drawn (and hit) on top.
        assert_eq!(e.doc.zones[0].id, "subnet-1");
        assert_eq!((e.doc.zones[0].w, e.doc.zones[0].h), (320.0, 220.0));
    }

    #[test]
    fn polylines_click_by_click() {
        let mut e = editor();
        e.set_tool(Tool::Line);
        e.pointer_down([0.0, 0.0], Button::Left, NO, 1);
        e.pointer_up([0.0, 0.0]);
        assert!(e.line_draft.is_some());
        e.pointer_down([100.0, 0.0], Button::Left, NO, 1);
        e.pointer_up([100.0, 0.0]);
        let shift = Mods {
            shift: true,
            alt: false,
        };
        e.pointer_down([100.0, 87.0], Button::Left, shift, 1);
        e.pointer_down([100.0, 87.0], Button::Left, shift, 2);
        assert_eq!(e.doc.lines.len(), 1);
        assert_eq!(
            e.doc.lines[0].points,
            vec![[0.0, 0.0], [100.0, 0.0], [100.0, 80.0]]
        );
    }

    #[test]
    fn panning_and_zooming() {
        let mut e = editor();
        e.space = true;
        e.pointer_down([10.0, 10.0], Button::Left, NO, 1);
        e.pointer_move([30.0, 50.0], NO);
        e.pointer_up([30.0, 50.0]);
        assert_eq!((e.camera.x, e.camera.y), (20.0, 40.0));
        let before = e.camera.to_world([400.0, 300.0]);
        e.wheel([400.0, 300.0], [0.0, 50.0], true);
        assert!(e.camera.zoom > 1.0);
        let after = e.camera.to_world([400.0, 300.0]);
        assert!(distance(before, after) < 1e-9);
    }
}
