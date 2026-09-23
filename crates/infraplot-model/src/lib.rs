//! The infra-plot document model.
//!
//! A [`Diagram`] is the single source of truth for everything the client renders,
//! in both the 2D sketch view and the 3D isometric view. It can be stored as JSON
//! or TOML; both encodings are lossless and equivalent.
//!
//! Coordinates live on a flat "floor" plane measured in world units (1 unit = 1 px
//! in the 2D view at 100% zoom). `x` grows to the right and `y` grows downwards.

use std::collections::{BTreeMap, HashMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;

/// An infrastructure diagram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Diagram")]
pub struct Diagram {
    /// Format version. Must be `1`.
    pub version: u32,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Rectangular areas that group nodes (networks, VPCs, clusters...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<Zone>,
    /// Infrastructure components.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<Node>,
    /// Connections between nodes and/or zones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<Edge>,
    /// Free-form polylines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
    /// Free-floating text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Zone {
    pub id: String,
    #[serde(default)]
    pub kind: ZoneKind,
    #[serde(default)]
    pub label: String,
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Hex colour (`#rgb` or `#rrggbb`). Defaults to the colour of `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Border style. Defaults to the style of `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StrokeStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ZoneKind {
    #[default]
    Generic,
    Region,
    Vpc,
    Subnet,
    Dmz,
    K8sCluster,
    Namespace,
    OnPrem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub label: String,
    /// Centre x.
    pub x: f64,
    /// Centre y.
    pub y: f64,
    /// Hex colour (`#rgb` or `#rrggbb`). Defaults to the colour of `kind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Arbitrary key/value details (IPs, ports, versions...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub meta: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum NodeKind {
    Service,
    Server,
    Vm,
    Container,
    Pod,
    K8s,
    Proxy,
    LoadBalancer,
    ApiGateway,
    Database,
    Cache,
    Queue,
    Storage,
    Function,
    Firewall,
    Cdn,
    Dns,
    User,
    Internet,
    Monitoring,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub id: String,
    /// Id of a node or zone.
    pub from: String,
    /// Id of a node or zone.
    pub to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default)]
    pub style: StrokeStyle,
    #[serde(default = "Arrow::end")]
    pub arrow: Arrow,
    #[serde(default)]
    pub route: Route,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Line {
    pub id: String,
    /// At least two `[x, y]` points.
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub style: StrokeStyle,
    #[serde(default)]
    pub arrow: Arrow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub text: String,
    #[serde(default)]
    pub size: TextSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StrokeStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Arrow {
    #[default]
    None,
    End,
    Start,
    Both,
}

impl Arrow {
    fn end() -> Self {
        Self::End
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Route {
    #[default]
    Straight,
    Orthogonal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TextSize {
    S,
    #[default]
    M,
    L,
    Xl,
}

/// A single problem found while validating a diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Issue {
    /// Location of the problem, e.g. `edges[2].to`.
    pub path: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid TOML: {0}")]
    Toml(#[from] toml::de::Error),
}

impl Diagram {
    pub fn from_json(src: &str) -> Result<Self, ParseError> {
        Ok(serde_json::from_str(src)?)
    }

    pub fn from_toml(src: &str) -> Result<Self, ParseError> {
        Ok(toml::from_str(src)?)
    }

    /// # Panics
    ///
    /// Never: every field serialises to JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("diagram is always JSON-serialisable")
    }

    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }

    /// Semantic checks that the JSON Schema cannot express.
    #[must_use]
    pub fn validate(&self) -> Vec<Issue> {
        let mut v = Validator::default();
        if self.version != FORMAT_VERSION {
            v.push(
                "version",
                format!(
                    "unsupported version {}, expected {FORMAT_VERSION}",
                    self.version
                ),
            );
        }

        let mut targets: HashSet<&str> = HashSet::new();
        for (i, z) in self.zones.iter().enumerate() {
            let p = format!("zones[{i}]");
            v.id(&p, &z.id);
            v.finite(&p, &[z.x, z.y, z.w, z.h]);
            if z.w <= 0.0 || z.h <= 0.0 {
                v.push(&p, "zone width and height must be positive");
            }
            v.color(&p, z.color.as_deref());
            targets.insert(&z.id);
        }
        for (i, n) in self.nodes.iter().enumerate() {
            let p = format!("nodes[{i}]");
            v.id(&p, &n.id);
            v.finite(&p, &[n.x, n.y]);
            v.color(&p, n.color.as_deref());
            targets.insert(&n.id);
        }
        for (i, e) in self.edges.iter().enumerate() {
            let p = format!("edges[{i}]");
            v.id(&p, &e.id);
            v.color(&p, e.color.as_deref());
            for (field, target) in [("from", &e.from), ("to", &e.to)] {
                if !targets.contains(target.as_str()) {
                    v.push(
                        format!("{p}.{field}"),
                        format!("unknown node or zone `{target}`"),
                    );
                }
            }
            if e.from == e.to {
                v.push(&p, "an edge cannot connect an element to itself");
            }
        }
        for (i, l) in self.lines.iter().enumerate() {
            let p = format!("lines[{i}]");
            v.id(&p, &l.id);
            v.color(&p, l.color.as_deref());
            if l.points.len() < 2 {
                v.push(format!("{p}.points"), "a line needs at least two points");
            }
            v.finite(&p, &l.points.iter().flatten().copied().collect::<Vec<_>>());
        }
        for (i, n) in self.notes.iter().enumerate() {
            let p = format!("notes[{i}]");
            v.id(&p, &n.id);
            v.finite(&p, &[n.x, n.y]);
            v.color(&p, n.color.as_deref());
        }
        v.issues
    }
}

#[derive(Default)]
struct Validator {
    issues: Vec<Issue>,
    seen: HashMap<String, String>,
}

impl Validator {
    fn push(&mut self, path: impl Into<String>, message: impl Into<String>) {
        self.issues.push(Issue {
            path: path.into(),
            message: message.into(),
        });
    }

    fn id(&mut self, path: &str, id: &str) {
        if id.is_empty() || id.chars().any(char::is_whitespace) {
            self.push(
                format!("{path}.id"),
                "ids must be non-empty and contain no whitespace",
            );
        } else if let Some(prev) = self.seen.insert(id.to_owned(), path.to_owned()) {
            self.push(
                format!("{path}.id"),
                format!("duplicate id `{id}` (also used by {prev})"),
            );
        }
    }

    fn finite(&mut self, path: &str, values: &[f64]) {
        if values.iter().any(|n| !n.is_finite()) {
            self.push(path, "coordinates must be finite numbers");
        }
    }

    fn color(&mut self, path: &str, color: Option<&str>) {
        let Some(c) = color else { return };
        let ok = c.strip_prefix('#').is_some_and(|hex| {
            matches!(hex.len(), 3 | 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit())
        });
        if !ok {
            self.push(
                format!("{path}.color"),
                format!("`{c}` is not a #rgb or #rrggbb colour"),
            );
        }
    }
}

/// JSON Schema (draft 2020-12) for [`Diagram`] documents.
#[must_use]
pub fn json_schema() -> serde_json::Value {
    let mut schema = schemars::schema_for!(Diagram).to_value();
    if let Some(obj) = schema.as_object_mut() {
        obj.insert(
            "$id".into(),
            "https://infra-plot.dev/schema/diagram.schema.json".into(),
        );
    }
    schema
}
