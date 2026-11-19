//! Text to diagram generation (M8, ADR-011).
//!
//! Four local parsers turn Mermaid, CSV, SQL, and JSON graph text into native
//! nodes with deterministic layouts. Nothing here is a renderer: the output is
//! an ordinary `Document`, so inserted content selects, styles, connects,
//! exports, and undoes like hand-drawn work. No parser makes a network request,
//! and AI generation stays out of scope.

pub mod csv;
pub mod json_graph;
pub mod mermaid;
pub mod sql;

use crate::geom::{Rect, V2, v2};
use crate::model::{
    Binding, Color, Document, Head, LineJump, Node, NodeId, NodeKind, Route, TextAlign,
};
use std::collections::HashSet;

/// Where the text came from. Each value maps to one parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Mermaid,
    Csv,
    Sql,
    Json,
}

impl Source {
    pub const ALL: [Source; 4] = [Source::Mermaid, Source::Csv, Source::Sql, Source::Json];

    pub fn id(self) -> &'static str {
        match self {
            Source::Mermaid => "mermaid",
            Source::Csv => "csv",
            Source::Sql => "sql",
            Source::Json => "json",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::Mermaid => "Mermaid",
            Source::Csv => "CSV",
            Source::Sql => "SQL schema",
            Source::Json => "JSON graph",
        }
    }

    pub fn parse_id(s: &str) -> Option<Source> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mermaid" | "flowchart" | "sequence" => Some(Source::Mermaid),
            "csv" => Some(Source::Csv),
            "sql" => Some(Source::Sql),
            "json" | "graph" => Some(Source::Json),
            _ => None,
        }
    }

    /// Short guidance shown above the text area.
    pub fn hint(self) -> &'static str {
        match self {
            Source::Mermaid => {
                "Paste a flowchart (graph TD, A[Start] --> B{Ready?}) or a sequence diagram \
                 (sequenceDiagram, participant A, A->>B: hello)."
            }
            Source::Csv => {
                "Paste comma, semicolon, or tab separated rows. The first row is a header when \
                 the box below is checked. Tree layouts treat the first cell as the id and the \
                 second as its parent."
            }
            Source::Sql => {
                "Paste CREATE TABLE statements. Columns become rows; primary keys are marked PK, \
                 and REFERENCES or FOREIGN KEY clauses become bound connectors."
            }
            Source::Json => {
                "Paste {\"nodes\":[{\"id\":\"a\",\"label\":\"Start\"}],\"edges\":[{\"source\":\"a\",\
                 \"target\":\"b\"}]}. Optional x, y, width, and height values are kept when every \
                 node has them."
            }
        }
    }
}

/// Deterministic placement for generated nodes. `Auto` keeps JSON coordinates
/// when every node provides them and falls back to a vertical flow otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    Auto,
    Grid,
    #[default]
    VerticalFlow,
    HorizontalFlow,
    VerticalTree,
    HorizontalTree,
}

impl Layout {
    pub const ALL: [Layout; 6] = [
        Layout::Auto,
        Layout::Grid,
        Layout::VerticalFlow,
        Layout::HorizontalFlow,
        Layout::VerticalTree,
        Layout::HorizontalTree,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Layout::Auto => "auto",
            Layout::Grid => "grid",
            Layout::VerticalFlow => "vertical",
            Layout::HorizontalFlow => "horizontal",
            Layout::VerticalTree => "vertical-tree",
            Layout::HorizontalTree => "horizontal-tree",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Layout::Auto => "Auto",
            Layout::Grid => "Grid",
            Layout::VerticalFlow => "Vertical flow",
            Layout::HorizontalFlow => "Horizontal flow",
            Layout::VerticalTree => "Vertical tree",
            Layout::HorizontalTree => "Horizontal tree",
        }
    }

    pub fn parse_id(s: &str) -> Option<Layout> {
        Layout::ALL
            .into_iter()
            .find(|l| l.id() == s.trim().to_ascii_lowercase())
    }

    pub fn is_tree(self) -> bool {
        matches!(self, Layout::VerticalTree | Layout::HorizontalTree)
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Layout::HorizontalFlow | Layout::HorizontalTree)
    }
}

/// Options shared by the parsers that need user choices.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    pub layout: Layout,
    /// CSV: skip the first row and use it for labels.
    pub header: bool,
    /// CSV: 0-based column used as the node label; None joins the non-empty cells.
    pub label_column: Option<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            layout: Layout::VerticalFlow,
            header: false,
            label_column: None,
        }
    }
}

/// Non-fatal compromises the parser made. Empty means a complete import.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Report {
    pub warnings: Vec<String>,
}

impl Report {
    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    pub fn summary(&self) -> String {
        match self.warnings.len() {
            0 => String::new(),
            1 => self.warnings[0].clone(),
            n => format!("{n} approximations: {}", self.warnings.join("; ")),
        }
    }
}

/// A parsed diagram plus the suggested page name and any approximations.
#[derive(Debug, Clone)]
pub struct Generated {
    pub name: String,
    pub doc: Document,
    pub report: Report,
}

/// Parse `text` with the parser for `source`. Errors are user-facing sentences
/// with a line number where the parser knows one.
pub fn generate(source: Source, text: &str, opts: &Options) -> Result<Generated, String> {
    if text.trim().is_empty() {
        return Err("paste some text first".into());
    }
    match source {
        Source::Mermaid => mermaid::parse(text),
        Source::Csv => csv::parse(text, opts),
        Source::Sql => sql::parse(text),
        Source::Json => json_graph::parse(text, opts.layout),
    }
}

// ----------------------------------------------------------------------------- shared builder

pub(crate) const INK: Color = Color::INK;
pub(crate) const WHITE: Color = Color::WHITE;
pub(crate) const BLUE: Color = Color::rgb(0xda, 0xe8, 0xfc);
pub(crate) const GREEN: Color = Color::rgb(0xd5, 0xe8, 0xd4);
pub(crate) const YELLOW: Color = Color::rgb(0xff, 0xf2, 0xcc);
pub(crate) const PURPLE: Color = Color::rgb(0xe1, 0xd5, 0xe7);
pub(crate) const MUTED: Color = Color::rgb(0x6b, 0x6b, 0x75);

/// A small document builder shared by the parsers. It mirrors the template pack
/// builder: nodes are ordinary native kinds and connectors bind by node id.
pub(crate) struct Builder {
    pub doc: Document,
}

impl Builder {
    pub fn new(name: &str) -> Builder {
        Builder {
            doc: Document {
                name: name.into(),
                ..Default::default()
            },
        }
    }

    pub fn add(&mut self, mut node: Node, parent: Option<NodeId>) -> NodeId {
        let id = self.doc.alloc_id();
        node.id = id;
        node.parent = parent;
        if let Some(p) = parent {
            if let Some(parent) = self.doc.nodes.get_mut(&p) {
                parent.children.push(id);
            }
        } else {
            self.doc.root.push(id);
        }
        self.doc.nodes.insert(id, node);
        id
    }

    pub fn rect(
        &mut self,
        rect: Rect,
        label: &str,
        fill: Option<Color>,
        radius: f64,
        parent: Option<NodeId>,
    ) -> NodeId {
        let kind = NodeKind::Rect;
        let mut node = Node::new(NodeId(0), kind, rect);
        node.style.set_fill(fill);
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        node.style.corner_radius = radius;
        node.label = label.into();
        node.name = first_line(label, "Rectangle");
        self.add(node, parent)
    }

    pub fn ellipse(&mut self, rect: Rect, label: &str, fill: Option<Color>) -> NodeId {
        let mut node = Node::new(NodeId(0), NodeKind::Ellipse, rect);
        node.style.set_fill(fill);
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        node.label = label.into();
        node.name = first_line(label, "Ellipse");
        self.add(node, None)
    }

    pub fn polygon(&mut self, rect: Rect, label: &str, sides: u32, fill: Option<Color>) -> NodeId {
        let mut node = Node::new(NodeId(0), NodeKind::Polygon { sides }, rect);
        node.style.set_fill(fill);
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        node.label = label.into();
        node.name = first_line(label, "Polygon");
        self.add(node, None)
    }

    pub fn text(
        &mut self,
        at: V2,
        text: &str,
        size: f64,
        color: Color,
        parent: Option<NodeId>,
    ) -> NodeId {
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: text.into(),
                font_size: size,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(at.x, at.y, 1.0, 1.0),
        );
        node.name = first_line(text, "Text");
        node.style.set_fill(Some(color));
        self.add(node, parent)
    }

    pub fn frame(&mut self, rect: Rect, name: &str) -> NodeId {
        let mut node = Node::new(NodeId(0), NodeKind::Frame { clip: false }, rect);
        node.style.set_fill(Some(WHITE));
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        node.style.corner_radius = 6.0;
        node.name = name.into();
        self.add(node, None)
    }

    pub fn world_rect(&self, id: NodeId) -> Option<Rect> {
        self.doc.world_rect(id)
    }

    /// A plain line node, used for lifelines and table rules.
    pub fn line(
        &mut self,
        a: V2,
        b: V2,
        color: Color,
        width: f64,
        parent: Option<NodeId>,
    ) -> NodeId {
        let rect = Rect::from_points(a, b);
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Line {
                a: a - rect.min(),
                b: b - rect.min(),
            },
            rect,
        );
        node.style.stroke = Some(color);
        node.style.stroke_width = width;
        node.name = "Line".into();
        self.add(node, parent)
    }

    fn anchor(&self, id: NodeId, fixed: V2) -> V2 {
        let rect = self.doc.world_rect(id).unwrap_or_default();
        v2(rect.x + rect.w * fixed.x, rect.y + rect.h * fixed.y)
    }

    /// A bound connector between two nodes. `fixed` is the normalized point on
    /// each node (0.5, 0.5 is the center).
    #[allow(clippy::too_many_arguments)]
    pub fn link(
        &mut self,
        from: (NodeId, V2),
        to: (NodeId, V2),
        label: &str,
        dashed: bool,
        heads: (Head, Head),
    ) -> NodeId {
        let (from_id, from_fixed) = from;
        let (to_id, to_fixed) = to;
        let a = self.anchor(from_id, from_fixed);
        let b = self.anchor(to_id, to_fixed);
        self.connector(
            vec![a, b],
            Some((from_id, from_fixed)),
            Some((to_id, to_fixed)),
            label,
            dashed,
            heads,
        )
    }

    /// An unbound connector through world points.
    pub fn connector(
        &mut self,
        points: Vec<V2>,
        start: Option<(NodeId, V2)>,
        end: Option<(NodeId, V2)>,
        label: &str,
        dashed: bool,
        heads: (Head, Head),
    ) -> NodeId {
        let bounds = Rect::union_all(
            &points
                .iter()
                .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        let local: Vec<V2> = points.iter().map(|p| *p - bounds.min()).collect();
        let bind = |(id, fixed): (NodeId, V2)| Binding {
            node: id,
            fixed,
            inside: false,
            floating: false,
        };
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: local,
                start: start.map(bind),
                end: end.map(bind),
                route: Route::Straight,
                start_head: heads.0,
                end_head: heads.1,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            bounds,
        );
        node.style.stroke = Some(INK);
        node.style.stroke_width = 1.6;
        if dashed {
            node.style.dash = vec![6.0, 4.0];
        }
        node.label = label.into();
        node.name = if label.is_empty() {
            "Connector".into()
        } else {
            first_line(label, "Connector")
        };
        self.add(node, None)
    }

    pub fn set_dash(&mut self, id: NodeId, pattern: &[f64]) {
        if let Some(n) = self.doc.nodes.get_mut(&id) {
            n.style.dash = pattern.to_vec();
        }
    }

    pub fn set_style(&mut self, id: NodeId, fill: Option<Color>, stroke: Option<Color>) {
        if let Some(n) = self.doc.nodes.get_mut(&id) {
            n.style.set_fill(fill);
            n.style.stroke = stroke;
        }
    }

    /// Resolve every connector once, then validate. Resolving keeps the
    /// preview and the inserted content aligned; bindings stay in place so the
    /// connectors follow nodes after insertion.
    pub fn finish(mut self, name: &str, report: Report) -> Result<Generated, String> {
        let ids: Vec<NodeId> = self
            .doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Connector { .. }))
            .map(|n| n.id)
            .collect();
        for id in ids {
            if let Some(points) = crate::connector::resolve(&self.doc, id) {
                set_world_points(&mut self.doc, id, &points);
            }
        }
        self.doc
            .validate()
            .map_err(|e| format!("generated diagram is invalid: {e}"))?;
        Ok(Generated {
            name: name.into(),
            doc: self.doc,
            report,
        })
    }
}

fn first_line(text: &str, fallback: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        fallback.into()
    } else {
        line.chars().take(48).collect()
    }
}

fn set_world_points(doc: &mut Document, id: NodeId, pts: &[V2]) {
    let Some(node) = doc.nodes.get(&id) else {
        return;
    };
    let origin = doc.parent_origin(node.parent);
    let Some(bounds) = Rect::union_all(
        &pts.iter()
            .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
            .collect::<Vec<_>>(),
    ) else {
        return;
    };
    let local: Vec<V2> = pts.iter().map(|p| *p - bounds.min()).collect();
    let node = doc.nodes.get_mut(&id).unwrap();
    node.rect = bounds.translate(V2::ZERO - origin);
    if let NodeKind::Connector { points, .. } = &mut node.kind {
        *points = local;
    }
}

// ----------------------------------------------------------------------------------- layouts

/// Sizes used by the layout helpers.
pub(crate) fn label_size(label: &str, base_w: f64, base_h: f64) -> (f64, f64) {
    let lines: Vec<&str> = label.lines().collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let w = (longest as f64 * 8.0 + 32.0).clamp(base_w, 280.0);
    let h = base_h + (lines.len().saturating_sub(1)) as f64 * 16.0;
    (w, h)
}

/// Place nodes in near-square grid cells. Deterministic for a fixed order.
pub(crate) fn grid_positions(sizes: &[(f64, f64)]) -> Vec<V2> {
    if sizes.is_empty() {
        return Vec::new();
    }
    let cols = (sizes.len() as f64).sqrt().ceil() as usize;
    let cols = cols.max(1);
    let cell_w = sizes.iter().map(|s| s.0).fold(0.0, f64::max) + 36.0;
    let cell_h = sizes.iter().map(|s| s.1).fold(0.0, f64::max) + 36.0;
    sizes
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let col = i % cols;
            let row = i / cols;
            v2(col as f64 * cell_w, row as f64 * cell_h)
        })
        .collect()
}

/// Direction of a layered layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    Down,
    Up,
    Right,
    Left,
}

/// Longest-path ranks. Cycles leave the remaining nodes in one trailing rank
/// instead of failing; the caller reports the cycle.
fn ranks(n: usize, edges: &[(usize, usize)]) -> (Vec<usize>, bool) {
    let mut seen = HashSet::new();
    let mut out = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for &(a, b) in edges {
        if a >= n || b >= n || a == b || !seen.insert((a, b)) {
            continue;
        }
        out[a].push(b);
        indeg[b] += 1;
    }
    let mut rank = vec![0usize; n];
    let mut queue: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut head = 0;
    let mut processed = 0;
    while head < queue.len() {
        let a = queue[head];
        head += 1;
        processed += 1;
        for &b in &out[a] {
            rank[b] = rank[b].max(rank[a] + 1);
            indeg[b] -= 1;
            if indeg[b] == 0 {
                queue.push(b);
            }
        }
    }
    let cycle = processed < n;
    if cycle {
        let max = rank.iter().copied().max().unwrap_or(0);
        for (i, r) in rank.iter_mut().enumerate() {
            if indeg[i] > 0 {
                *r = max + 1;
            }
        }
    }
    (rank, cycle)
}

/// Ranked layout: nodes flow along the main axis, one rank per column or row.
/// Returns the top-left position of every node, shifted so the bounding box
/// starts at the origin, plus a warning when a cycle was broken.
pub(crate) fn layered_positions(
    sizes: &[(f64, f64)],
    edges: &[(usize, usize)],
    dir: Dir,
) -> (Vec<V2>, Option<String>) {
    let n = sizes.len();
    if n == 0 {
        return (Vec::new(), None);
    }
    let (rank, cycle) = ranks(n, edges);
    let max_rank = rank.iter().copied().max().unwrap_or(0);
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (i, &r) in rank.iter().enumerate() {
        groups[r].push(i);
    }
    let horizontal = matches!(dir, Dir::Left | Dir::Right);
    let reverse = matches!(dir, Dir::Up | Dir::Left);
    let main_size = |i: usize| if horizontal { sizes[i].0 } else { sizes[i].1 };
    let cross_size = |i: usize| if horizontal { sizes[i].1 } else { sizes[i].0 };

    let main_gap = 76.0;
    let cross_gap = 36.0;
    let mut main_at = vec![0.0f64; groups.len()];
    let mut cursor = 0.0f64;
    for (r, group) in groups.iter().enumerate() {
        main_at[r] = cursor;
        cursor += group.iter().map(|&i| main_size(i)).fold(0.0, f64::max) + main_gap;
    }
    let main_total = (cursor - main_gap).max(0.0);

    let mut pos = vec![V2::ZERO; n];
    for (r, group) in groups.iter().enumerate() {
        let total: f64 = group.iter().map(|&i| cross_size(i)).sum::<f64>()
            + cross_gap * (group.len().saturating_sub(1)) as f64;
        let mut across = -total / 2.0;
        for &i in group {
            let (main, cross) = if reverse {
                (main_total - main_at[r] - main_size(i), across)
            } else {
                (main_at[r], across)
            };
            pos[i] = if horizontal {
                v2(main, cross)
            } else {
                v2(cross, main)
            };
            across += cross_size(i) + cross_gap;
        }
    }

    let min_x = pos.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let min_y = pos.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let shift = v2(-min_x, -min_y);
    for p in &mut pos {
        *p = *p + shift;
    }
    let warning = cycle.then(|| "a cycle was broken to lay the graph out".to_string());
    (pos, warning)
}

/// Add a chain between consecutive nodes so a "flow" with no edges still reads
/// as a sequence.
pub(crate) fn chain_edges(n: usize) -> Vec<(usize, usize)> {
    (0..n.saturating_sub(1)).map(|i| (i, i + 1)).collect()
}

/// Apply a computed top-left position list to a size list.
pub(crate) fn rect_at(pos: V2, size: (f64, f64)) -> Rect {
    Rect::new(pos.x, pos.y, size.0, size.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_and_layout_ids_round_trip() {
        for s in Source::ALL {
            assert_eq!(Source::parse_id(s.id()), Some(s));
        }
        for l in Layout::ALL {
            assert_eq!(Layout::parse_id(l.id()), Some(l));
        }
        assert_eq!(Source::parse_id("nope"), None);
        assert_eq!(Layout::parse_id("nope"), None);
    }

    #[test]
    fn empty_text_is_refused_before_a_parser_runs() {
        let err = generate(Source::Mermaid, "   \n", &Options::default()).unwrap_err();
        assert!(err.contains("paste some text"), "{err}");
    }

    #[test]
    fn layered_layout_is_deterministic_and_flows_along_the_main_axis() {
        let sizes = [(100.0, 50.0), (100.0, 50.0), (100.0, 50.0)];
        let edges = [(0, 1), (1, 2)];
        let (a, w) = layered_positions(&sizes, &edges, Dir::Down);
        let (b, _) = layered_positions(&sizes, &edges, Dir::Down);
        assert_eq!(a, b);
        assert!(w.is_none());
        assert!(a[0].y < a[1].y && a[1].y < a[2].y);
        let (h, _) = layered_positions(&sizes, &edges, Dir::Right);
        assert!(h[0].x < h[1].x && h[1].x < h[2].x);
    }

    #[test]
    fn a_cycle_is_reported_and_still_lays_out() {
        let sizes = [(100.0, 50.0); 3];
        let (pos, warning) = layered_positions(&sizes, &[(0, 1), (1, 2), (2, 0)], Dir::Down);
        assert_eq!(pos.len(), 3);
        assert!(warning.is_some());
    }
}
