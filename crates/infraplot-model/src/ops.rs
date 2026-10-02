//! Editing operations shared by the editors: add, move, delete, duplicate, bend.
//!
//! Mirrors `web/src/state/ops.ts` and the id helpers of `web/src/model/doc.ts`.

use std::collections::HashSet;

use crate::catalog::GRID;
use crate::geometry::{
    Point, Rect, SegmentAxis, bend_at, bend_of, bend_segment, node_box, note_box, segment_axis,
    snap, zone_box,
};
use crate::{
    Arrow, Diagram, Edge, Line, Node, NodeKind, Note, Route, StrokeStyle, TextSize, Zone, ZoneKind,
};

/// What kind of element an id refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementType {
    Zone,
    Node,
    Edge,
    Line,
    Note,
}

impl ElementType {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Zone => "Zone",
            Self::Node => "Node",
            Self::Edge => "Edge",
            Self::Line => "Line",
            Self::Note => "Note",
        }
    }
}

impl Diagram {
    /// An empty document.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            version: crate::FORMAT_VERSION,
            title: title.into(),
            description: String::new(),
            look: crate::Look::default(),
            zones: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            lines: Vec::new(),
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub fn element_type(&self, id: &str) -> Option<ElementType> {
        if self.zones.iter().any(|z| z.id == id) {
            Some(ElementType::Zone)
        } else if self.nodes.iter().any(|n| n.id == id) {
            Some(ElementType::Node)
        } else if self.edges.iter().any(|e| e.id == id) {
            Some(ElementType::Edge)
        } else if self.lines.iter().any(|l| l.id == id) {
            Some(ElementType::Line)
        } else if self.notes.iter().any(|n| n.id == id) {
            Some(ElementType::Note)
        } else {
            None
        }
    }

    pub fn zone_mut(&mut self, id: &str) -> Option<&mut Zone> {
        self.zones.iter_mut().find(|z| z.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn edge_mut(&mut self, id: &str) -> Option<&mut Edge> {
        self.edges.iter_mut().find(|e| e.id == id)
    }

    pub fn line_mut(&mut self, id: &str) -> Option<&mut Line> {
        self.lines.iter_mut().find(|l| l.id == id)
    }

    pub fn note_mut(&mut self, id: &str) -> Option<&mut Note> {
        self.notes.iter_mut().find(|n| n.id == id)
    }

    fn ids(&self) -> HashSet<&str> {
        self.zones
            .iter()
            .map(|z| z.id.as_str())
            .chain(self.nodes.iter().map(|n| n.id.as_str()))
            .chain(self.edges.iter().map(|e| e.id.as_str()))
            .chain(self.lines.iter().map(|l| l.id.as_str()))
            .chain(self.notes.iter().map(|n| n.id.as_str()))
            .collect()
    }

    /// `<prefix>-<n>` with the first `n` not taken.
    #[must_use]
    pub fn unique_id(&self, prefix: &str) -> String {
        let ids = self.ids();
        let mut base = String::new();
        for c in prefix.to_lowercase().chars() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                base.push(c);
            } else if !base.ends_with('-') {
                base.push('-');
            }
        }
        if base.is_empty() {
            base.push_str("el");
        }
        let mut n = 1u64;
        loop {
            let id = format!("{base}-{n}");
            if !ids.contains(id.as_str()) {
                return id;
            }
            n += 1;
        }
    }

    /// Adds a node centred on `p` (snapped to the grid) and returns its id.
    pub fn add_node(&mut self, kind: NodeKind, p: Point) -> String {
        let id = self.unique_id(kind.slug());
        self.nodes.push(Node {
            id: id.clone(),
            kind,
            label: kind.info().label.to_owned(),
            x: snap(p[0], true),
            y: snap(p[1], true),
            color: None,
            meta: std::collections::BTreeMap::new(),
        });
        id
    }

    /// Adds a zone; bigger zones go first so smaller ones render on top of them.
    pub fn add_zone(&mut self, kind: ZoneKind, r: Rect) -> String {
        let id = self.unique_id(if kind == ZoneKind::Generic {
            "zone"
        } else {
            kind.slug()
        });
        let zone = Zone {
            id: id.clone(),
            kind,
            label: kind.info().label.to_owned(),
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            color: None,
            style: None,
        };
        let area = r.w * r.h;
        match self.zones.iter().position(|z| z.w * z.h < area) {
            Some(i) => self.zones.insert(i, zone),
            None => self.zones.push(zone),
        }
        id
    }

    /// Connects two nodes/zones. Returns the existing edge if they are already connected.
    pub fn add_edge(&mut self, from: &str, to: &str) -> Option<String> {
        if from == to {
            return None;
        }
        if let Some(e) = self.edges.iter().find(|e| e.from == from && e.to == to) {
            return Some(e.id.clone());
        }
        let id = self.unique_id("edge");
        self.edges.push(Edge {
            id: id.clone(),
            from: from.to_owned(),
            to: to.to_owned(),
            label: String::new(),
            style: StrokeStyle::Solid,
            arrow: Arrow::End,
            route: Route::Straight,
            bend: None,
            color: None,
        });
        Some(id)
    }

    pub fn add_line(&mut self, points: Vec<Point>) -> String {
        let id = self.unique_id("line");
        self.lines.push(Line {
            id: id.clone(),
            points,
            style: StrokeStyle::Solid,
            arrow: Arrow::End,
            color: None,
        });
        id
    }

    pub fn add_note(&mut self, p: Point, text: impl Into<String>) -> String {
        let id = self.unique_id("note");
        self.notes.push(Note {
            id: id.clone(),
            x: p[0],
            y: p[1],
            text: text.into(),
            size: TextSize::M,
            color: None,
        });
        id
    }

    /// Ids of everything fully inside zone `id`: what moving the zone drags along.
    #[must_use]
    pub fn zone_contents(&self, id: &str) -> HashSet<String> {
        let Some(zone) = self.zones.iter().find(|z| z.id == id) else {
            return HashSet::new();
        };
        let outer = zone_box(zone);
        let mut inside = HashSet::new();
        for z in &self.zones {
            if z.id != id && outer.contains(&zone_box(z)) {
                inside.insert(z.id.clone());
            }
        }
        for n in &self.nodes {
            if outer.contains(&node_box(n)) {
                inside.insert(n.id.clone());
            }
        }
        for n in &self.notes {
            if outer.contains(&note_box(n)) {
                inside.insert(n.id.clone());
            }
        }
        for l in &self.lines {
            if l.points.iter().all(|&p| outer.contains_point(p)) {
                inside.insert(l.id.clone());
            }
        }
        inside
    }

    /// Moves an element by (dx, dy). Moving a zone drags along `contents` (normally taken
    /// with [`Diagram::zone_contents`] when the drag starts, so the zone doesn't sweep up
    /// what it passes over).
    pub fn move_element(&mut self, id: &str, dx: f64, dy: f64, contents: &HashSet<String>) {
        let shift = |pts: &mut Vec<Point>| {
            for p in pts {
                p[0] += dx;
                p[1] += dy;
            }
        };
        match self.element_type(id) {
            Some(ElementType::Node) => {
                if let Some(n) = self.node_mut(id) {
                    n.x += dx;
                    n.y += dy;
                }
            }
            Some(ElementType::Note) => {
                if let Some(n) = self.note_mut(id) {
                    n.x += dx;
                    n.y += dy;
                }
            }
            Some(ElementType::Line) => {
                if let Some(l) = self.line_mut(id) {
                    shift(&mut l.points);
                }
            }
            Some(ElementType::Zone) => {
                for z in &mut self.zones {
                    if z.id == id || contents.contains(&z.id) {
                        z.x += dx;
                        z.y += dy;
                    }
                }
                for n in &mut self.nodes {
                    if contents.contains(&n.id) {
                        n.x += dx;
                        n.y += dy;
                    }
                }
                for n in &mut self.notes {
                    if contents.contains(&n.id) {
                        n.x += dx;
                        n.y += dy;
                    }
                }
                for l in &mut self.lines {
                    if contents.contains(&l.id) {
                        shift(&mut l.points);
                    }
                }
            }
            Some(ElementType::Edge) | None => {}
        }
    }

    /// Deletes an element and every edge attached to it.
    pub fn delete_element(&mut self, id: &str) {
        self.zones.retain(|z| z.id != id);
        self.nodes.retain(|n| n.id != id);
        self.lines.retain(|l| l.id != id);
        self.notes.retain(|n| n.id != id);
        self.edges
            .retain(|e| e.id != id && e.from != id && e.to != id);
    }

    /// Copies an element next to the original and returns the copy's id.
    pub fn duplicate_element(&mut self, id: &str) -> Option<String> {
        const OFFSET: f64 = 40.0;
        match self.element_type(id)? {
            ElementType::Edge => None,
            ElementType::Node => {
                let mut copy = self.nodes.iter().find(|n| n.id == id)?.clone();
                copy.id = self.unique_id(copy.kind.slug());
                copy.x += OFFSET;
                copy.y += OFFSET;
                let new_id = copy.id.clone();
                self.nodes.push(copy);
                Some(new_id)
            }
            ElementType::Zone => {
                let mut copy = self.zones.iter().find(|z| z.id == id)?.clone();
                copy.id = self.unique_id("zone");
                copy.x += OFFSET;
                copy.y += OFFSET;
                let new_id = copy.id.clone();
                self.zones.push(copy);
                Some(new_id)
            }
            ElementType::Note => {
                let mut copy = self.notes.iter().find(|n| n.id == id)?.clone();
                copy.id = self.unique_id("note");
                copy.x += OFFSET;
                copy.y += OFFSET;
                let new_id = copy.id.clone();
                self.notes.push(copy);
                Some(new_id)
            }
            ElementType::Line => {
                let mut copy = self.lines.iter().find(|l| l.id == id)?.clone();
                copy.id = self.unique_id("line");
                for p in &mut copy.points {
                    p[0] += OFFSET;
                    p[1] += OFFSET;
                }
                let new_id = copy.id.clone();
                self.lines.push(copy);
                Some(new_id)
            }
        }
    }

    /// The bend that would put the middle segment of orthogonal edge `id` through `p`, or
    /// `None` if it wouldn't change anything.
    #[must_use]
    #[allow(clippy::float_cmp)] // both sides are rounded the same way
    pub fn bend_target(&self, id: &str, p: Point, snap_to_grid: bool) -> Option<f64> {
        let e = self.edges.iter().find(|e| e.id == id)?;
        let (_, _, layout) = bend_segment(self, e)?;
        let i = match layout.axis {
            crate::geometry::Axis::X => 0,
            crate::geometry::Axis::Y => 1,
        };
        let bend = bend_at(&layout, snap(p[i], snap_to_grid));
        (bend != bend_of(e)).then_some(bend)
    }

    /// Slides the middle segment of orthogonal edge `id` through `p`. Returns the new bend.
    pub fn bend_edge_to(&mut self, id: &str, p: Point, snap_to_grid: bool) -> Option<f64> {
        let bend = self.bend_target(id, p, snap_to_grid)?;
        self.edge_mut(id)?.bend = Some(bend);
        Some(bend)
    }

    /// Moves point `index` of line `id` to `p`.
    pub fn move_line_vertex(&mut self, id: &str, index: usize, p: Point) {
        if let Some(pt) = self.line_mut(id).and_then(|l| l.points.get_mut(index)) {
            *pt = p;
        }
    }

    /// Moves segment `index` (points `index` and `index + 1`) of line `id` perpendicular to
    /// itself, so right angles stay right. Diagonal segments move freely. `orig` are the
    /// line's points when the drag started.
    pub fn move_line_segment(&mut self, id: &str, index: usize, orig: &[Point], dx: f64, dy: f64) {
        let (Some(&p), Some(&q)) = (orig.get(index), orig.get(index + 1)) else {
            return;
        };
        let axis = segment_axis(p, q);
        let mx = if axis == Some(SegmentAxis::Horizontal) {
            0.0
        } else {
            dx
        };
        let my = if axis == Some(SegmentAxis::Vertical) {
            0.0
        } else {
            dy
        };
        if let Some(l) = self.line_mut(id) {
            l.points = orig
                .iter()
                .enumerate()
                .map(|(i, &[x, y])| {
                    if i == index || i == index + 1 {
                        [x + mx, y + my]
                    } else {
                        [x, y]
                    }
                })
                .collect();
        }
    }

    /// Resizes zone `id` to `r`, keeping it at least one grid cell in each direction.
    pub fn resize_zone(&mut self, id: &str, r: Rect) {
        if let Some(z) = self.zone_mut(id) {
            z.x = r.x;
            z.y = r.y;
            z.w = r.w.max(GRID);
            z.h = r.h.max(GRID);
        }
    }

    /// Renames an element and rewires edges pointing at it. Returns `false` if `to` is
    /// taken or not a valid id.
    pub fn rename_id(&mut self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }
        if to.is_empty() || to.chars().any(char::is_whitespace) || self.element_type(to).is_some() {
            return false;
        }
        let set = |id: &mut String| {
            if id == from {
                to.clone_into(id);
            }
        };
        let Some(t) = self.element_type(from) else {
            return false;
        };
        match t {
            ElementType::Zone => self.zones.iter_mut().for_each(|z| set(&mut z.id)),
            ElementType::Node => self.nodes.iter_mut().for_each(|n| set(&mut n.id)),
            ElementType::Edge => self.edges.iter_mut().for_each(|e| set(&mut e.id)),
            ElementType::Line => self.lines.iter_mut().for_each(|l| set(&mut l.id)),
            ElementType::Note => self.notes.iter_mut().for_each(|n| set(&mut n.id)),
        }
        for e in &mut self.edges {
            set(&mut e.from);
            set(&mut e.to);
        }
        true
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)] // exact grid values
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_slugged() {
        let mut d = Diagram::new("t");
        let a = d.add_node(NodeKind::LoadBalancer, [13.0, 27.0]);
        let b = d.add_node(NodeKind::LoadBalancer, [100.0, 0.0]);
        assert_eq!(a, "load-balancer-1");
        assert_eq!(b, "load-balancer-2");
        assert_eq!((d.nodes[0].x, d.nodes[0].y), (20.0, 20.0));
        // Same slugging as `uniqueId` in the web editor.
        assert_eq!(d.unique_id("Hello World!"), "hello-world--1");
        assert!(d.validate().is_empty());
    }

    #[test]
    fn moving_a_zone_drags_its_contents() {
        let mut d = Diagram::new("t");
        let z = d.add_zone(
            ZoneKind::Vpc,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 400.0,
                h: 300.0,
            },
        );
        let inside = d.add_node(NodeKind::Server, [100.0, 100.0]);
        let outside = d.add_node(NodeKind::Server, [600.0, 100.0]);
        let contents = d.zone_contents(&z);
        assert!(contents.contains(&inside) && !contents.contains(&outside));
        d.move_element(&z, 20.0, 40.0, &contents);
        assert_eq!((d.zones[0].x, d.zones[0].y), (20.0, 40.0));
        assert_eq!((d.nodes[0].x, d.nodes[0].y), (120.0, 140.0));
        assert_eq!((d.nodes[1].x, d.nodes[1].y), (600.0, 100.0));
    }

    #[test]
    fn deleting_a_node_drops_its_edges_and_renames_rewire() {
        let mut d = Diagram::new("t");
        let a = d.add_node(NodeKind::Service, [0.0, 0.0]);
        let b = d.add_node(NodeKind::Database, [200.0, 0.0]);
        let e = d.add_edge(&a, &b).unwrap();
        assert_eq!(d.add_edge(&a, &b), Some(e.clone()));
        assert!(d.rename_id(&b, "db"));
        assert_eq!(d.edges[0].to, "db");
        assert!(!d.rename_id(&a, "db"));
        d.delete_element("db");
        assert!(d.edges.is_empty());
    }

    #[test]
    fn segments_move_perpendicular() {
        let mut d = Diagram::new("t");
        let orig = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0]];
        let l = d.add_line(orig.clone());
        d.move_line_segment(&l, 0, &orig, 30.0, 20.0);
        assert_eq!(
            d.lines[0].points,
            vec![[0.0, 20.0], [100.0, 20.0], [100.0, 100.0]]
        );
        let dup = d.duplicate_element(&l).unwrap();
        assert_eq!(d.lines[1].id, dup);
        assert_eq!(d.lines[1].points[0], [40.0, 60.0]);
    }
}
