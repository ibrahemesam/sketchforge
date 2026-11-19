//! The native shape registry (ADR-010): named, categorized shapes built from
//! the existing node kinds. Entries emit native kinds or editable vector paths,
//! so a registry shape is a regular node: styles, connectors, frames, auto
//! layout, and export apply unchanged, and previews render the same geometry
//! the canvas does. Definitions are pure data plus pure builders, so an entry
//! cannot execute code or fetch a remote asset.

use crate::geom::{Rect, V2, v2};
use crate::model::settings::DefaultStyle;
use crate::model::{Color, FillRule, Node, NodeId, NodeKind, SubPath};
use crate::vector::{VPath, to_subpaths};
use std::f64::consts::PI;

/// Shape categories for the search report and the MCP surface.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Category {
    Flowchart,
    Uml,
    Er,
    Bpmn,
    Network,
    FloorPlan,
    Org,
    C4,
    Sysml,
    Charts,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Flowchart => "Flowchart",
            Category::Uml => "UML",
            Category::Er => "ER",
            Category::Bpmn => "BPMN",
            Category::Network => "Network",
            Category::FloorPlan => "Floor plan",
            Category::Org => "Org chart",
            Category::C4 => "C4",
            Category::Sysml => "SysML",
            Category::Charts => "Charts",
        }
    }
}

/// One registry entry: identity, search terms, preferred size, and geometry.
pub struct ShapeDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub keywords: &'static [&'static str],
    /// Preferred size for a first insert and for the search grid previews.
    pub size: (f64, f64),
    /// True when the shape takes the diagram's default style, like draw.io: the
    /// page's remembered fill and text style apply on insert and in its
    /// preview, so a searched shape matches the current diagram. Stroke-art
    /// shapes (actor, door, window, stairs) keep their own pen look.
    pub plain: bool,
}

/// Build the node for a registry shape. The returned id is 0; the editor
/// allocates the real id at insert time.
pub fn build(def: &ShapeDef, rect: Rect, page_default: Option<&DefaultStyle>) -> Node {
    let mut n = Node::new(NodeId(0), kind_for(def.id, rect.w, rect.h), rect);
    if def.plain
        && let Some(ds) = page_default
    {
        n.style = ds.style.clone();
        n.text_style = ds.text_style.clone();
    }
    customize(def.id, &mut n);
    n.name = def.name.into();
    n
}

/// The NodeKind for an entry: an existing parametric kind, or a built path.
fn kind_for(id: &str, w: f64, h: f64) -> NodeKind {
    match id {
        "process" | "entity" => NodeKind::Rect,
        "decision" | "relationship" | "bpmn-gateway" => NodeKind::Polygon { sides: 4 },
        "attribute" | "derived-attribute" | "key-attribute" | "on-page-connector"
        | "bpmn-start" | "bpmn-end" | "venn-circle" => NodeKind::Ellipse,
        "bpmn-task" | "c4-system" | "c4-container" | "c4-component" | "c4-boundary"
        | "sysml-block" | "sysml-requirement" | "sysml-port" => NodeKind::Rect,
        "terminator" => NodeKind::Rect,
        _ => NodeKind::Path {
            subpaths: paths_for(id, w, h),
            fill_rule: FillRule::default(),
        },
    }
}

/// Style touches that survive the diagram's default style.
fn customize(id: &str, n: &mut Node) {
    match id {
        // Terminator: keep the pill look at any height.
        "terminator" => n.style.corner_radius = n.rect.h * 0.5,
        // Pen shapes: stroke only, no area fill.
        "actor" | "door" | "window" | "stairs" | "c4-person" => {
            n.style.fills.clear();
            n.style.fill_enabled = false;
            n.style.fill_auto = false;
            n.style.stroke = Some(Color::INK);
            n.style.stroke_width = 2.0;
        }
        "bpmn-start" | "bpmn-intermediate" => {
            n.style.set_fill(Some(Color::WHITE));
            n.style.fill_auto = false;
            n.style.stroke = Some(Color::INK);
            n.style.stroke_width = 2.0;
        }
        "bpmn-end" => {
            n.style.set_fill(Some(Color::WHITE));
            n.style.fill_auto = false;
            n.style.stroke = Some(Color::INK);
            n.style.stroke_width = 5.0;
        }
        "bpmn-task" => n.style.corner_radius = 12.0,
        "c4-system" | "c4-container" | "c4-component" => n.style.corner_radius = 6.0,
        "c4-boundary" => {
            n.style.fills.clear();
            n.style.fill_enabled = false;
            n.style.fill_auto = false;
            n.style.stroke = Some(Color::rgb(0x8a, 0x8a, 0x92));
            n.style.stroke_width = 2.0;
            n.style.dash = vec![8.0, 6.0];
        }
        "sysml-port" => {
            n.style.set_fill(Some(Color::WHITE));
            n.style.fill_auto = false;
            n.style.stroke = Some(Color::INK);
            n.style.stroke_width = 2.0;
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------------- defs

pub const DEFS: &[ShapeDef] = &[
    // Flowchart
    ShapeDef {
        id: "process",
        name: "Process",
        category: Category::Flowchart,
        keywords: &["process", "step", "rectangle", "box"],
        size: (160.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "decision",
        name: "Decision",
        category: Category::Flowchart,
        keywords: &["decision", "branch", "question", "diamond"],
        size: (140.0, 100.0),
        plain: true,
    },
    ShapeDef {
        id: "terminator",
        name: "Terminator",
        category: Category::Flowchart,
        keywords: &["terminator", "start", "end", "pill", "rounded"],
        size: (160.0, 60.0),
        plain: true,
    },
    ShapeDef {
        id: "document",
        name: "Document",
        category: Category::Flowchart,
        keywords: &["document", "paper", "report", "printout", "wavy"],
        size: (160.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "data",
        name: "Data",
        category: Category::Flowchart,
        keywords: &["data", "input output", "parallelogram", "io"],
        size: (160.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "manual-input",
        name: "Manual Input",
        category: Category::Flowchart,
        keywords: &["manual input", "form", "trapezoid", "slanted"],
        size: (160.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "manual-operation",
        name: "Manual Operation",
        category: Category::Flowchart,
        keywords: &["manual operation", "inverted trapezoid", "labor"],
        size: (160.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "preparation",
        name: "Preparation",
        category: Category::Flowchart,
        keywords: &["preparation", "initialization", "setup", "hexagon"],
        size: (180.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "delay",
        name: "Delay",
        category: Category::Flowchart,
        keywords: &["delay", "wait", "half circle", "stadium"],
        size: (140.0, 80.0),
        plain: true,
    },
    ShapeDef {
        id: "off-page",
        name: "Off-Page Connector",
        category: Category::Flowchart,
        keywords: &["off page", "connector", "link", "pentagon arrow"],
        size: (140.0, 120.0),
        plain: true,
    },
    ShapeDef {
        id: "on-page-connector",
        name: "On-Page Connector",
        category: Category::Flowchart,
        keywords: &["on page", "connector", "reference", "circle"],
        size: (60.0, 60.0),
        plain: true,
    },
    ShapeDef {
        id: "queue",
        name: "Queue",
        category: Category::Flowchart,
        keywords: &["queue", "channel", "fifo", "pipe"],
        size: (130.0, 90.0),
        plain: true,
    },
    // ER
    ShapeDef {
        id: "entity",
        name: "Entity",
        category: Category::Er,
        keywords: &["entity", "table", "box", "record"],
        size: (120.0, 60.0),
        plain: true,
    },
    ShapeDef {
        id: "weak-entity",
        name: "Weak Entity",
        category: Category::Er,
        keywords: &["weak entity", "double border", "identifying"],
        size: (120.0, 60.0),
        plain: true,
    },
    ShapeDef {
        id: "attribute",
        name: "Attribute",
        category: Category::Er,
        keywords: &["attribute", "field", "ellipse", "column"],
        size: (110.0, 50.0),
        plain: true,
    },
    ShapeDef {
        id: "derived-attribute",
        name: "Derived Attribute",
        category: Category::Er,
        keywords: &["derived attribute", "dashed ellipse", "computed"],
        size: (130.0, 50.0),
        plain: true,
    },
    ShapeDef {
        id: "key-attribute",
        name: "Key Attribute",
        category: Category::Er,
        keywords: &["key attribute", "identifier", "primary key", "underlined"],
        size: (120.0, 50.0),
        plain: true,
    },
    ShapeDef {
        id: "relationship",
        name: "Relationship",
        category: Category::Er,
        keywords: &["relationship", "diamond", "association", "crow"],
        size: (130.0, 90.0),
        plain: true,
    },
    // UML
    ShapeDef {
        id: "actor",
        name: "Actor",
        category: Category::Uml,
        keywords: &["actor", "stick figure", "use case", "user"],
        size: (60.0, 100.0),
        plain: false,
    },
    ShapeDef {
        id: "class",
        name: "Class",
        category: Category::Uml,
        keywords: &["class", "uml", "attributes", "methods", "compartments"],
        size: (160.0, 110.0),
        plain: true,
    },
    ShapeDef {
        id: "note",
        name: "Note",
        category: Category::Uml,
        keywords: &["note", "annotation", "folded corner", "comment"],
        size: (130.0, 90.0),
        plain: true,
    },
    ShapeDef {
        id: "package",
        name: "Package",
        category: Category::Uml,
        keywords: &["package", "folder", "tab", "namespace"],
        size: (150.0, 70.0),
        plain: true,
    },
    // Network
    ShapeDef {
        id: "database",
        name: "Database",
        category: Category::Network,
        keywords: &["database", "cylinder", "db", "storage", "disk"],
        size: (120.0, 110.0),
        plain: true,
    },
    ShapeDef {
        id: "cloud",
        name: "Cloud",
        category: Category::Network,
        keywords: &["cloud", "internet", "network", "bumps"],
        size: (140.0, 90.0),
        plain: true,
    },
    ShapeDef {
        id: "host",
        name: "Host",
        category: Category::Network,
        keywords: &["host", "monitor", "computer", "pc", "screen"],
        size: (120.0, 100.0),
        plain: true,
    },
    ShapeDef {
        id: "server",
        name: "Server",
        category: Category::Network,
        keywords: &["server", "rack", "units", "appliance"],
        size: (110.0, 110.0),
        plain: true,
    },
    ShapeDef {
        id: "firewall",
        name: "Firewall",
        category: Category::Network,
        keywords: &["firewall", "brick wall", "security", "filter"],
        size: (140.0, 90.0),
        plain: true,
    },
    // Floor plan
    ShapeDef {
        id: "door",
        name: "Door",
        category: Category::FloorPlan,
        keywords: &["door", "arc", "hinge", "swing"],
        size: (60.0, 60.0),
        plain: false,
    },
    ShapeDef {
        id: "window",
        name: "Window",
        category: Category::FloorPlan,
        keywords: &["window", "double line", "glass"],
        size: (100.0, 40.0),
        plain: false,
    },
    ShapeDef {
        id: "stairs",
        name: "Stairs",
        category: Category::FloorPlan,
        keywords: &["stairs", "steps", "zigzag"],
        size: (130.0, 90.0),
        plain: false,
    },
    // Org chart
    ShapeDef {
        id: "person",
        name: "Person",
        category: Category::Org,
        keywords: &["person", "people", "head", "shoulders", "org chart"],
        size: (80.0, 100.0),
        plain: true,
    },
    // BPMN 2.0
    ShapeDef {
        id: "bpmn-start",
        name: "Start Event",
        category: Category::Bpmn,
        keywords: &["bpmn", "start", "event", "circle", "trigger"],
        size: (50.0, 50.0),
        plain: false,
    },
    ShapeDef {
        id: "bpmn-intermediate",
        name: "Intermediate Event",
        category: Category::Bpmn,
        keywords: &["bpmn", "intermediate", "event", "double circle", "catch"],
        size: (50.0, 50.0),
        plain: false,
    },
    ShapeDef {
        id: "bpmn-end",
        name: "End Event",
        category: Category::Bpmn,
        keywords: &["bpmn", "end", "event", "thick circle", "finish"],
        size: (50.0, 50.0),
        plain: false,
    },
    ShapeDef {
        id: "bpmn-task",
        name: "Task",
        category: Category::Bpmn,
        keywords: &["bpmn", "task", "activity", "rounded rectangle", "step"],
        size: (150.0, 70.0),
        plain: true,
    },
    ShapeDef {
        id: "bpmn-gateway",
        name: "Gateway",
        category: Category::Bpmn,
        keywords: &["bpmn", "gateway", "decision", "diamond", "branch"],
        size: (60.0, 60.0),
        plain: true,
    },
    // C4 model
    ShapeDef {
        id: "c4-person",
        name: "C4 Person",
        category: Category::C4,
        keywords: &["c4", "person", "actor", "user", "role"],
        size: (90.0, 130.0),
        plain: false,
    },
    ShapeDef {
        id: "c4-system",
        name: "C4 Software System",
        category: Category::C4,
        keywords: &["c4", "system", "software", "boundary", "application"],
        size: (210.0, 120.0),
        plain: true,
    },
    ShapeDef {
        id: "c4-container",
        name: "C4 Container",
        category: Category::C4,
        keywords: &["c4", "container", "service", "app", "deployable"],
        size: (180.0, 100.0),
        plain: true,
    },
    ShapeDef {
        id: "c4-component",
        name: "C4 Component",
        category: Category::C4,
        keywords: &["c4", "component", "module", "library", "inside"],
        size: (150.0, 90.0),
        plain: true,
    },
    ShapeDef {
        id: "c4-boundary",
        name: "C4 Boundary",
        category: Category::C4,
        keywords: &["c4", "boundary", "dashed", "group", "scope"],
        size: (360.0, 240.0),
        plain: false,
    },
    // SysML
    ShapeDef {
        id: "sysml-block",
        name: "SysML Block",
        category: Category::Sysml,
        keywords: &["sysml", "block", "system", "part", "structure"],
        size: (180.0, 100.0),
        plain: true,
    },
    ShapeDef {
        id: "sysml-requirement",
        name: "SysML Requirement",
        category: Category::Sysml,
        keywords: &["sysml", "requirement", "specification", "need", "trace"],
        size: (190.0, 90.0),
        plain: true,
    },
    ShapeDef {
        id: "sysml-port",
        name: "SysML Port",
        category: Category::Sysml,
        keywords: &["sysml", "port", "interface", "connection", "square"],
        size: (26.0, 26.0),
        plain: false,
    },
    // Charts and Venn diagrams
    ShapeDef {
        id: "pie-slice",
        name: "Pie Slice",
        category: Category::Charts,
        keywords: &["chart", "pie", "slice", "wedge", "quarter"],
        size: (120.0, 120.0),
        plain: false,
    },
    ShapeDef {
        id: "venn-circle",
        name: "Venn Circle",
        category: Category::Charts,
        keywords: &["venn", "circle", "overlap", "set", "diagram"],
        size: (180.0, 180.0),
        plain: true,
    },
];

pub fn all() -> &'static [ShapeDef] {
    DEFS
}

pub fn count() -> usize {
    DEFS.len()
}

/// Look an entry up by stable id or exact name (case-insensitive).
pub fn by_id(id: &str) -> Option<&'static ShapeDef> {
    let q = id.trim().to_lowercase();
    DEFS.iter()
        .find(|d| d.id == q || d.name.to_lowercase() == q)
}

/// Fuzzy search over name and keywords, best matches first.
pub fn search(query: &str) -> Vec<&'static ShapeDef> {
    let q: Vec<char> = query.trim().to_lowercase().chars().collect();
    if q.is_empty() {
        return DEFS.iter().collect();
    }
    let mut hit: Vec<(Option<i64>, &'static ShapeDef)> =
        DEFS.iter().map(|d| (score(d, &q), d)).collect();
    hit.retain(|(m, _)| m.is_some());
    hit.sort_by_key(|(m, _)| m.unwrap_or(i64::MAX));
    hit.into_iter().map(|(_, d)| d).collect()
}

/// Subsequence matcher with a bonus for word starts and consecutive runs.
fn fuzzy(q: &[char], hay: &str) -> Option<i64> {
    let hay = hay.to_lowercase();
    let mut score = 0i64;
    let mut run = 0i64;
    let mut last: Option<(usize, char)> = None;
    let mut qi = 0;
    for (i, ch) in hay.char_indices() {
        if qi < q.len() && ch == q[qi] {
            let word_start = match last {
                None => true,
                Some((li, _)) => hay[li..i].contains([' ', '/', '-']),
            };
            score += if word_start { 3 } else { 1 } + run;
            run += 2;
            last = Some((i, ch));
            qi += 1;
        } else if last.is_some() {
            run = 0;
        }
    }
    (qi == q.len()).then_some(score)
}

fn score(d: &ShapeDef, q: &[char]) -> Option<i64> {
    let mut best: Option<i64> = None;
    let consider = |s: &str, penalty: i64, best: &mut Option<i64>| {
        if let Some(m) = fuzzy(q, s) {
            *best = Some(best.map_or(m + penalty, |b| b.max(m + penalty)));
        }
    };
    consider(d.id, 0, &mut best);
    consider(d.name, 0, &mut best);
    for k in d.keywords {
        consider(k, -1, &mut best);
    }
    best
}

// ---------------------------------------------------------------------------------- geometry

fn paths_for(id: &str, w: f64, h: f64) -> Vec<SubPath> {
    let p = match id {
        "document" => document(w, h),
        "data" => poly(&[(w * 0.2, 0.0), (w, 0.0), (w * 0.8, h), (0.0, h)]),
        "manual-input" => poly(&[(0.0, h * 0.25), (w, 0.0), (w, h), (0.0, h)]),
        "manual-operation" => poly(&[(0.0, 0.0), (w, 0.0), (w * 0.8, h), (w * 0.2, h)]),
        "preparation" => {
            let d = w * 0.15;
            poly(&[
                (d, 0.0),
                (w - d, 0.0),
                (w, h * 0.5),
                (w - d, h),
                (d, h),
                (0.0, h * 0.5),
            ])
        }
        "off-page" => {
            let t = h * 0.8;
            poly(&[(0.0, 0.0), (w, 0.0), (w, t), (w * 0.5, h), (0.0, t)])
        }
        "queue" => {
            let t = h * 0.28;
            let g = w * 0.65;
            poly(&[
                (0.0, 0.0),
                (w, 0.0),
                (w, t),
                (g, t),
                (g, h - t),
                (w, h - t),
                (w, h),
                (0.0, h),
            ])
        }
        "delay" => {
            let r = (h * 0.5).min(w * 0.75);
            let mut p = VPath::new();
            p.move_to(v2(0.0, 0.0))
                .line_to(v2(w - r, 0.0))
                .arc_to(w - r, h * 0.5, r, r, deg(-90.0), deg(90.0))
                .line_to(v2(0.0, h))
                .close();
            p
        }
        "database" => cylinder(w, h),
        "cloud" => cloud(w, h),
        "host" => host(w, h),
        "server" => server(w, h),
        "firewall" => firewall(w, h),
        "actor" => actor(w, h),
        "class" => class(w, h),
        "note" => note(w, h),
        "package" => package(w, h),
        "weak-entity" => weak_entity(w, h),
        "door" => door(w, h),
        "window" => window(w, h),
        "stairs" => stairs(w, h),
        "person" | "c4-person" => person(w, h),
        "bpmn-intermediate" => intermediate_event(w, h),
        "pie-slice" => pie_slice(w, h),
        _ => VPath::new(),
    };
    to_subpaths(&p, v2(0.0, 0.0))
}

fn poly(pts: &[(f64, f64)]) -> VPath {
    let q: Vec<V2> = pts.iter().map(|(x, y)| v2(*x, *y)).collect();
    VPath::polyline(&q, true)
}

fn deg(x: f64) -> f64 {
    x * PI / 180.0
}

/// Document flowchart: straight top and sides, a smooth curve along the bottom
/// that dips to the edge.
fn document(w: f64, h: f64) -> VPath {
    let e = h * 0.12;
    let mut p = VPath::new();
    p.move_to(v2(0.0, 0.0))
        .line_to(v2(w, 0.0))
        .line_to(v2(w, h - e))
        .arc_to(w * 0.5, h - e, w * 0.5, e, deg(0.0), deg(180.0))
        .close();
    p
}

/// Database/cylinder: straight sides and elliptical caps, plus the front rim of
/// the top cap as its own subpath so the rim line stays visible.
fn cylinder(w: f64, h: f64) -> VPath {
    let e = (h * 0.2).min(w * 0.3);
    let mut p = VPath::new();
    p.move_to(v2(0.0, e))
        .arc_to(w * 0.5, e, w * 0.5, e, deg(180.0), deg(360.0))
        .line_to(v2(w, h - e))
        .arc_to(w * 0.5, h - e, w * 0.5, e, deg(0.0), deg(180.0))
        .close();
    // Front rim of the top cap.
    p.arc_to(w * 0.5, e, w * 0.5, e, deg(0.0), deg(180.0));
    p
}

/// Cloud: an ellipse with uneven bumps, smoothed closed.
fn cloud(w: f64, h: f64) -> VPath {
    let (cx, cy) = (w * 0.5, h * 0.58);
    let (rx, ry) = (w * 0.5, h * 0.5);
    let n = 24;
    let pts: Vec<V2> = (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * 2.0 * PI;
            // Three uneven bumps around the ellipse, as if circles had merged.
            let bump = 1.0 + 0.13 * (3.0 * a + 1.2).sin();
            let wobble = 1.0 + 0.07 * (5.0 * a - 0.6).sin();
            v2(cx + a.cos() * rx * bump * wobble, cy + a.sin() * ry * bump)
        })
        .collect();
    smooth_closed(&pts)
}

/// Closed Catmull-Rom smoothing: passes through the points with rounded turns.
pub fn smooth_closed(pts: &[V2]) -> VPath {
    let n = pts.len();
    let mut p = VPath::new();
    if n < 3 {
        return p;
    }
    p.move_to(pts[0]);
    for i in 0..n {
        let p0 = pts[(i + n - 1) % n];
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        let p3 = pts[(i + 2) % n];
        p.cubic_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
    }
    p.close();
    p
}

/// Host: monitor, stand, and base.
fn host(w: f64, h: f64) -> VPath {
    let mut p = VPath::new();
    let sh = h * 0.68;
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, sh), (0.0, sh)]));
    p.append(&poly(&[
        (w * 0.42, sh),
        (w * 0.58, sh),
        (w * 0.62, h * 0.8),
        (w * 0.38, h * 0.8),
    ]));
    p.append(&poly(&[
        (w * 0.26, h * 0.8),
        (w * 0.74, h * 0.8),
        (w * 0.74, h * 0.86),
        (w * 0.26, h * 0.86),
    ]));
    p
}

/// Server: a rack of three units with indicator dots.
fn server(w: f64, h: f64) -> VPath {
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]));
    p.move_to(v2(0.0, h / 3.0)).line_to(v2(w, h / 3.0));
    p.move_to(v2(0.0, 2.0 * h / 3.0))
        .line_to(v2(w, 2.0 * h / 3.0));
    let (r, cx) = (w * 0.05, w * 0.85);
    for cy in [h * 0.16, h * 0.5, h * 0.84] {
        p.arc_to(cx, cy, r, r, deg(0.0), deg(360.0));
        p.close();
    }
    p
}

/// Firewall: a wall of bricks with offset joints.
fn firewall(w: f64, h: f64) -> VPath {
    let rows = 4;
    let rh = h / rows as f64;
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]));
    for i in 1..rows {
        let y = rh * i as f64;
        p.move_to(v2(0.0, y)).line_to(v2(w, y));
        let n = 4;
        for j in 0..n {
            let x = w * (j as f64 + if i % 2 == 0 { 0.0 } else { 0.5 }) / n as f64;
            p.move_to(v2(x, y - rh)).line_to(v2(x, y + rh));
        }
    }
    p
}

/// UML actor: head, body, arms, legs.
fn actor(w: f64, h: f64) -> VPath {
    let cx = w * 0.5;
    let hr = w * 0.22;
    let head_y = hr + h * 0.02;
    let mut p = VPath::new();
    p.arc_to(cx, head_y, hr, hr, deg(0.0), deg(360.0));
    p.close();
    let neck = head_y + hr;
    let hip = h * 0.62;
    p.move_to(v2(cx, neck)).line_to(v2(cx, hip)); // body
    let shoulder = neck + (hip - neck) * 0.25;
    p.move_to(v2(w * 0.08, shoulder))
        .line_to(v2(w - w * 0.08, shoulder)); // arms
    p.move_to(v2(cx, hip)).line_to(v2(w * 0.16, h)); // legs
    p.move_to(v2(cx, hip)).line_to(v2(w - w * 0.16, h));
    p
}

/// UML class: box with attribute and method compartments.
fn class(w: f64, h: f64) -> VPath {
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]));
    p.move_to(v2(0.0, h * 0.28)).line_to(v2(w, h * 0.28));
    p.move_to(v2(0.0, h * 0.62)).line_to(v2(w, h * 0.62));
    p
}

/// UML note: rectangle with a folded corner.
fn note(w: f64, h: f64) -> VPath {
    let f = (w * 0.18).min(h * 0.3);
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w - f, 0.0), (w, f), (w, h), (0.0, h)]));
    p.move_to(v2(w - f, 0.0))
        .line_to(v2(w - f, f))
        .line_to(v2(w, f));
    p
}

/// UML package: body with a tab on top.
fn package(w: f64, h: f64) -> VPath {
    let t = h * 0.25;
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, t), (w, t), (w, h), (0.0, h)]));
    p.append(&poly(&[(0.0, 0.0), (w * 0.4, 0.0), (w * 0.4, t), (0.0, t)]));
    p
}

/// ER weak entity: a box with a second border inside.
fn weak_entity(w: f64, h: f64) -> VPath {
    let t = (h * 0.12).min(8.0);
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]));
    p.append(&poly(&[(t, t), (w - t, t), (w - t, h - t), (t, h - t)]));
    p
}

/// Floor-plan door: a leaf from the hinge and its quarter-arc swing.
fn door(w: f64, h: f64) -> VPath {
    let r = w.min(h * 0.9);
    let mut p = VPath::new();
    p.move_to(v2(0.0, h))
        .line_to(v2(0.0, h - r))
        .arc_to(0.0, h, r, r, deg(-90.0), deg(0.0));
    p
}

/// Floor-plan window: two parallel frame lines.
fn window(w: f64, h: f64) -> VPath {
    let mut p = VPath::new();
    p.append(&poly(&[(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]));
    let t = h * 0.3;
    p.append(&poly(&[(0.0, t), (w, t), (w, h - t), (0.0, h - t)]));
    p
}

/// Floor-plan stairs: a closed staircase silhouette.
fn stairs(w: f64, h: f64) -> VPath {
    let n = 5;
    let s = h * 0.8 / n as f64;
    let t = w / n as f64;
    let mut pts = vec![(0.0, h)];
    let mut x = 0.0;
    let mut y = h;
    for _ in 0..n {
        y -= s;
        pts.push((x, y)); // riser
        x += t;
        pts.push((x, y)); // tread
    }
    pts.push((w, h)); // right wall
    poly(&pts)
}

/// Org-chart person: head with shoulders.
fn person(w: f64, h: f64) -> VPath {
    let cx = w * 0.5;
    let hr = w * 0.28;
    let mut p = VPath::new();
    p.arc_to(cx, hr + h * 0.05, hr, hr, deg(0.0), deg(360.0));
    p.close();
    let ry = h * 0.4;
    p.move_to(v2(cx - w * 0.46, h))
        .arc_to(cx, h, w * 0.46, ry, deg(180.0), deg(360.0));
    p
}

/// BPMN intermediate event: an outer circle with a smaller inner ring.
fn intermediate_event(w: f64, h: f64) -> VPath {
    let (cx, cy) = (w * 0.5, h * 0.5);
    let outer = w.min(h) * 0.5;
    let inner = outer * 0.72;
    let mut p = VPath::new();
    p.arc_to(cx, cy, outer, outer, deg(0.0), deg(360.0));
    p.close();
    p.arc_to(cx, cy, inner, inner, deg(0.0), deg(360.0));
    p.close();
    p
}

/// Pie slice: a quarter wedge from the center to the top-right corner. Packs
/// rotate and fill four of these to make a pie chart.
fn pie_slice(w: f64, h: f64) -> VPath {
    let (cx, cy) = (w * 0.5, h * 0.5);
    let r = w.min(h) * 0.5;
    let mut p = VPath::new();
    p.move_to(v2(cx, cy))
        .line_to(v2(cx + r, cy))
        .arc_to(cx, cy, r, r, deg(0.0), deg(-90.0))
        .close();
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_by_name_and_keyword() {
        let hits = search("cylinder");
        assert_eq!(hits[0].id, "database");
        let hits = search("queue");
        assert_eq!(hits[0].id, "queue");
        assert!(by_id("database").is_some());
        assert!(by_id("Database").is_some());
        assert!(by_id("nope").is_none());
    }

    #[test]
    fn every_def_builds_inside_its_rect() {
        for d in all() {
            let rect = Rect::new(100.0, 100.0, d.size.0, d.size.1);
            let n = build(d, rect, None);
            assert_eq!(n.rect, rect, "{}", d.id);
            if let NodeKind::Path { subpaths, .. } = &n.kind {
                assert!(!subpaths.is_empty(), "{}", d.id);
                // The geometry hugs its rect; smoothing and bump outlines
                // may round out by a fraction, never wildly.
                let slack = 0.25 * d.size.0.max(d.size.1);
                for sp in subpaths {
                    for pt in &sp.points {
                        assert!(pt.p.x >= -slack, "{}", d.id);
                        assert!(pt.p.x <= d.size.0 + slack, "{}", d.id);
                        assert!(pt.p.y >= -slack, "{}", d.id);
                        assert!(pt.p.y <= d.size.1 + slack, "{}", d.id);
                    }
                }
            }
        }
    }

    #[test]
    fn arc_to_matches_kappa_at_quarter_sweeps() {
        // The approximation control offset must match the offset the existing
        // ellipse() builder uses, so registry arcs and ellipse() draw alike.
        let mut p = VPath::new();
        p.arc_to(10.0, 10.0, 5.0, 5.0, 0.0, PI / 2.0);
        assert_eq!(p.segs.len(), 2); // move + one cubic
        use crate::vector::Seg;
        match p.segs[1] {
            Seg::C(c1, _, _) => {
                // c1 = p0 + tangent(0°) * (4/3)tan(sweep/4), 4/3 tan(PI/8) = KAPPA.
                assert!((c1.x - 15.0).abs() < 1e-9, "c1 = {c1:?}");
                let expect = 10.0 + 5.0 * 0.552_284_749_830_793_4;
                assert!((c1.y - expect).abs() < 1e-9, "c1 = {c1:?}");
            }
            _ => panic!("expected a cubic"),
        }
    }
}
