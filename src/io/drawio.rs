//! draw.io (`.drawio`, `.xml`) import and export, per ADR-011.
//!
//! Import parses the `mxfile`/`mxGraphModel` XML into an intermediate cell list, maps
//! the supported subset onto native nodes, layers and connectors, and returns a report
//! of what was imported, approximated or dropped. Compressed payloads are decoded;
//! encrypted ones are refused with a clear message. Export writes the native model as
//! an uncompressed `mxfile` so draw.io can open it directly.

use crate::geom::{Rect, V2, v2};
use crate::model::{
    AdaptiveColors, Binding, Color, DEFAULT_HEAD_SIZE, Document, FlowAnimation, FlowDirection,
    FlowTiming, Head, ImageMode, Layer, LayerId, LineJump, Node, NodeId, NodeKind, PageTransition,
    Paint, Route, TextAlign, VAlign,
};
use roxmltree::Node as XmlNode;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// Native-only source connector default, carried in the UserObject sidecar.
const SKETCH_CONNECTOR_DEFAULT: &str = "__sketchConnectorDefault";
/// Native-only upright-label flag. draw.io has no style key for it, so it
/// rides in the UserObject sidecar and survives a Sketch round trip.
const SKETCH_LABEL_UPRIGHT: &str = "__sketchLabelUpright";
/// Native-only grid color. draw.io has no page setting for this, so the value
/// rides in the mxGraphModel sidecar and survives a Sketch round trip.
const SKETCH_GRID_COLOR: &str = "__sketchGridColor";
/// Native-only page transition. Draw.io has no page attribute for this, so the
/// value rides in the mxGraphModel sidecar and survives a Sketch round trip.
const SKETCH_PAGE_TRANSITION: &str = "__sketchPageTransition";
const SKETCH_PAGE_TRANSITION_MS: &str = "__sketchPageTransitionMs";

// --------------------------------------------------------------------------------- report

/// One cell the adapter could not represent exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub page: String,
    pub cell: String,
    pub item: String,
    pub reason: String,
}

/// What a draw.io import read, approximated and dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub pages: usize,
    pub shapes: usize,
    pub connectors: usize,
    pub layers: usize,
    pub approximated: Vec<Diagnostic>,
    pub dropped: Vec<Diagnostic>,
    pub warnings: Vec<String>,
}

impl ImportReport {
    pub fn total(&self) -> usize {
        self.shapes + self.connectors
    }

    pub fn summary(&self) -> String {
        format!(
            "{} page(s), {} shape(s), {} connector(s), {} layer(s); {} approximated, {} dropped",
            self.pages,
            self.shapes,
            self.connectors,
            self.layers,
            self.approximated.len(),
            self.dropped.len()
        )
    }

    fn approx(&mut self, page: &str, cell: &str, item: &str, reason: impl Into<String>) {
        self.approximated.push(Diagnostic {
            page: page.into(),
            cell: cell.into(),
            item: item.into(),
            reason: reason.into(),
        });
    }

    fn drop_cell(&mut self, page: &str, cell: &str, item: &str, reason: impl Into<String>) {
        self.dropped.push(Diagnostic {
            page: page.into(),
            cell: cell.into(),
            item: item.into(),
            reason: reason.into(),
        });
    }

    /// Record a non-cell warning (unsupported features, compression notes).
    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }
}

// --------------------------------------------------------------------------------- import

/// Parse a draw.io file into native pages plus a fidelity report.
pub fn import_str(xml: &str) -> Result<(Vec<crate::project::PageData>, ImportReport), String> {
    let parsed =
        roxmltree::Document::parse(xml).map_err(|e| format!("not valid draw.io XML: {e}"))?;
    let root = parsed.root_element();
    let mut report = ImportReport::default();
    let mut pages = Vec::new();
    if root.has_tag_name("mxfile") {
        let diagrams: Vec<XmlNode<'_, '_>> = root
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("diagram"))
            .collect();
        if diagrams.is_empty() {
            return Err("the draw.io file has no diagrams".into());
        }
        for (i, d) in diagrams.iter().enumerate() {
            pages.push(import_diagram(*d, i, &mut report)?);
        }
    } else if root.has_tag_name("diagram") {
        pages.push(import_diagram(root, 0, &mut report)?);
    } else if root.has_tag_name("mxGraphModel") {
        let name = "Page 1".to_string();
        pages.push(import_model(root, name, new_page_id(), &mut report)?);
    } else {
        return Err(format!(
            "unsupported XML root <{}>; expected a draw.io <mxfile> or <mxGraphModel>",
            root.tag_name().name()
        ));
    }
    report.pages = pages.len();
    Ok((pages, report))
}

fn new_page_id() -> String {
    crate::project::format::new_id()
}

fn import_diagram(
    d: XmlNode<'_, '_>,
    index: usize,
    report: &mut ImportReport,
) -> Result<crate::project::PageData, String> {
    let name = d
        .attribute("name")
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("Page {}", index + 1));
    // draw.io ids are only unique inside the file and can contain path
    // characters, so native pages get a fresh id. Names and order carry over.
    let id = new_page_id();
    if let Some(model) = d
        .children()
        .find(|n| n.is_element() && n.has_tag_name("mxGraphModel"))
    {
        return import_model(model, name, id, report);
    }
    let text = d.text().unwrap_or("").trim();
    if text.is_empty() {
        return Err(format!("diagram \"{name}\" has no XML content"));
    }
    let decoded = decode_payload(text, &name)?;
    let parsed = roxmltree::Document::parse(&decoded)
        .map_err(|e| format!("diagram \"{name}\" did not decode to valid XML: {e}"))?;
    let model = parsed.root_element();
    if !model.has_tag_name("mxGraphModel") {
        return Err(format!(
            "diagram \"{name}\" decoded to <{}>, not <mxGraphModel>",
            model.tag_name().name()
        ));
    }
    import_model(model, name, id, report)
}

/// Decode a draw.io diagram body: plain XML, percent-encoded XML, or base64 of a raw
/// deflate stream of percent-encoded XML. Encrypted bodies are refused explicitly.
fn decode_payload(text: &str, page: &str) -> Result<String, String> {
    let t = text.trim();
    let plain = percent_decode(t);
    if plain.trim_start().starts_with('<') {
        return Ok(plain);
    }
    let Some(bytes) = crate::assets::base64_decode(t) else {
        return Err(format!(
            "diagram \"{page}\" is encrypted or uses a payload Sketch cannot read. \
             Password-protected draw.io files are not supported."
        ));
    };
    if bytes.starts_with(b"Salted__") {
        return Err(format!(
            "diagram \"{page}\" is encrypted. Password-protected draw.io files are not supported."
        ));
    }
    for v in [
        miniz_oxide::inflate::decompress_to_vec(&bytes),
        miniz_oxide::inflate::decompress_to_vec_zlib(&bytes),
    ]
    .into_iter()
    .flatten()
    {
        let s = percent_decode(&String::from_utf8_lossy(&v));
        if s.trim_start().starts_with('<') {
            return Ok(s);
        }
    }
    if bytes.starts_with(b"<") {
        let s = percent_decode(&String::from_utf8_lossy(&bytes));
        if s.trim_start().starts_with('<') {
            return Ok(s);
        }
    }
    Err(format!(
        "diagram \"{page}\" is encrypted or uses an unsupported compression. \
         Password-protected draw.io files are not supported."
    ))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex_val(b[i + 1]), hex_val(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

// --------------------------------------------------------------------------------- cells

#[derive(Debug, Clone, Default)]
struct Geom {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    points: Vec<V2>,
    source: Option<V2>,
    target: Option<V2>,
    offset: Option<V2>,
    relative: bool,
    has_box: bool,
    has_x: bool,
}

#[derive(Debug, Clone, Default)]
struct RawCell {
    id: String,
    value: String,
    style: String,
    parent: Option<String>,
    vertex: bool,
    edge: bool,
    source: Option<String>,
    target: Option<String>,
    visible: bool,
    locked: bool,
    link: String,
    tooltip: String,
    tags: Vec<String>,
    metadata: std::collections::BTreeMap<String, String>,
    geom: Geom,
    order: usize,
}

impl RawCell {
    fn parsed_style(&self) -> ParsedStyle {
        ParsedStyle::parse(&self.style)
    }
}

fn parse_cell(node: XmlNode<'_, '_>, wrapper: Option<XmlNode<'_, '_>>, order: usize) -> RawCell {
    let wrapper_value = wrapper.and_then(|w| w.attribute("label"));
    let value = node
        .attribute("value")
        .or(wrapper_value)
        .unwrap_or("")
        .to_string();
    let id = node
        .attribute("id")
        .or_else(|| wrapper.and_then(|w| w.attribute("id")))
        .unwrap_or("")
        .to_string();
    let parent = node.attribute("parent").map(String::from);
    let vertex = attr_bool(node, "vertex");
    let edge = attr_bool(node, "edge");
    let mut geom = Geom::default();
    if let Some(g) = node
        .children()
        .find(|n| n.is_element() && n.has_tag_name("mxGeometry"))
    {
        geom.x = num_attr(g, "x").unwrap_or(0.0);
        geom.y = num_attr(g, "y").unwrap_or(0.0);
        geom.w = num_attr(g, "width").unwrap_or(0.0);
        geom.h = num_attr(g, "height").unwrap_or(0.0);
        geom.has_box = g.attribute("width").is_some() || g.attribute("height").is_some();
        geom.has_x = g.attribute("x").is_some();
        geom.relative = g.attribute("relative").is_some_and(|v| v == "1");
        for p in g
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("mxPoint"))
        {
            let point = v2(
                num_attr(p, "x").unwrap_or(0.0),
                num_attr(p, "y").unwrap_or(0.0),
            );
            match p.attribute("as") {
                Some("sourcePoint") => geom.source = Some(point),
                Some("targetPoint") => geom.target = Some(point),
                Some("offset") => geom.offset = Some(point),
                _ => {}
            }
        }
        if let Some(array) = g
            .children()
            .find(|n| n.is_element() && n.has_tag_name("Array"))
        {
            for p in array
                .children()
                .filter(|n| n.is_element() && n.has_tag_name("mxPoint"))
            {
                geom.points.push(v2(
                    num_attr(p, "x").unwrap_or(0.0),
                    num_attr(p, "y").unwrap_or(0.0),
                ));
            }
        }
    }
    let mut metadata = std::collections::BTreeMap::new();
    let link = node
        .attribute("link")
        .or_else(|| wrapper.and_then(|w| w.attribute("link")))
        .unwrap_or("")
        .to_string();
    let tooltip = node
        .attribute("tooltip")
        .or_else(|| wrapper.and_then(|w| w.attribute("tooltip")))
        .unwrap_or("")
        .to_string();
    let tags = node
        .attribute("tags")
        .or_else(|| wrapper.and_then(|w| w.attribute("tags")))
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect();
    if let Some(w) = wrapper {
        for a in w.attributes() {
            let key = a.name();
            if is_reserved_wrapper_attr(key) {
                continue;
            }
            metadata.insert(key.to_string(), a.value().to_string());
        }
        // Keys that are not valid XML names travel in a JSON sidecar so a
        // native round trip keeps them even when the attribute cannot spell them.
        if let Some(json) = w.attribute("sketch-data")
            && let Ok(map) =
                serde_json::from_str::<std::collections::BTreeMap<String, String>>(json)
        {
            for (k, v) in map {
                metadata.entry(k).or_insert(v);
            }
        }
    }
    RawCell {
        id,
        value,
        style: node.attribute("style").unwrap_or("").to_string(),
        parent,
        vertex,
        edge,
        source: node.attribute("source").map(String::from),
        target: node.attribute("target").map(String::from),
        visible: node.attribute("visible").map(|v| v != "0").unwrap_or(true),
        locked: attr_bool(node, "locked"),
        link,
        tooltip,
        tags,
        metadata,
        geom,
        order,
    }
}

fn is_reserved_wrapper_attr(name: &str) -> bool {
    matches!(
        name,
        "id" | "label" | "link" | "tooltip" | "tags" | "placeholders" | "sketch-data"
    ) || name.starts_with("xmlns")
}

/// Copy wrapper data (custom properties, link, tooltip and tags) onto a native node.
fn apply_cell_data(node: &mut Node, cell: &RawCell) {
    let mut metadata = cell.metadata.clone();
    if let Some(json) = metadata.remove(SKETCH_CONNECTOR_DEFAULT)
        && let Ok(ds) = serde_json::from_str::<crate::model::DefaultStyle>(&json)
    {
        node.connector_default = Some(ds);
    }
    if let Some(value) = metadata.remove(SKETCH_LABEL_UPRIGHT) {
        node.style.label_upright = value == "1" || value.eq_ignore_ascii_case("true");
    }
    node.metadata = metadata;
    node.link = cell.link.clone();
    node.tooltip = cell.tooltip.clone();
    node.tags = cell.tags.clone();
}

/// Whether a string can be an XML attribute name (letters, digits, `_`, `-`,
/// `.`, no leading digit). Keys that fail travel in the `sketch-data` sidecar.
fn xml_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn attr_bool(node: XmlNode<'_, '_>, name: &str) -> bool {
    node.attribute(name)
        .is_some_and(|v| v == "1" || v == "true")
}

fn num_attr(node: XmlNode<'_, '_>, name: &str) -> Option<f64> {
    node.attribute(name)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

// --------------------------------------------------------------------------------- model

fn import_model(
    model: XmlNode<'_, '_>,
    name: String,
    id: String,
    report: &mut ImportReport,
) -> Result<crate::project::PageData, String> {
    let mut doc = Document {
        name: name.clone(),
        ..Document::default()
    };
    apply_model_settings(&mut doc, model);

    let root_el = model
        .children()
        .find(|n| n.is_element() && n.has_tag_name("root"))
        .ok_or_else(|| format!("diagram \"{name}\" has no <root>"))?;

    let mut cells: Vec<RawCell> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (order, ch) in root_el.children().filter(|n| n.is_element()).enumerate() {
        let (cell_el, wrapper) = if ch.has_tag_name("mxCell") {
            (ch, None)
        } else if ch.has_tag_name("UserObject") || ch.has_tag_name("object") {
            match ch
                .children()
                .find(|n| n.is_element() && n.has_tag_name("mxCell"))
            {
                Some(c) => (c, Some(ch)),
                None => continue,
            }
        } else {
            continue;
        };
        let cell = parse_cell(cell_el, wrapper, order);
        if cell.id.is_empty() {
            report.drop_cell(&name, "?", "cell", "cell has no id");
            continue;
        }
        if !seen.insert(cell.id.clone()) {
            report.drop_cell(
                &name,
                &cell.id,
                "cell",
                "duplicate cell id; the later cell was ignored",
            );
            continue;
        }
        cells.push(cell);
    }
    if cells.is_empty() {
        return Err(format!("diagram \"{name}\" has no cells"));
    }

    let by_id: HashMap<String, usize> = cells
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), i))
        .collect();

    // ---- layers: cells parented to the root cell with no vertex/edge flag.
    let mut layer_ids: HashMap<String, Option<LayerId>> = HashMap::new();
    layer_ids.insert("0".into(), None);
    let layer_cells: Vec<&RawCell> = cells
        .iter()
        .filter(|c| c.parent.as_deref() == Some("0") && !c.vertex && !c.edge)
        .collect();
    let background_id = layer_cells
        .iter()
        .find(|c| c.id == "1")
        .or_else(|| layer_cells.first())
        .map(|c| c.id.clone());
    if let Some(c) = background_id
        .as_ref()
        .and_then(|bg| layer_cells.iter().find(|c| &c.id == bg))
    {
        let label = plain_text(&c.value);
        if !label.is_empty() {
            doc.layers[0].name = label;
        }
        doc.layers[0].visible = c.visible;
        doc.layers[0].locked = c.locked;
        layer_ids.insert(c.id.clone(), None);
    }
    for c in &layer_cells {
        if Some(&c.id) == background_id.as_ref() {
            continue;
        }
        let lid = LayerId(doc.next_layer);
        doc.next_layer += 1;
        doc.layers.push(Layer {
            id: lid,
            name: {
                let label = plain_text(&c.value);
                if label.is_empty() {
                    "Layer".into()
                } else {
                    label
                }
            },
            visible: c.visible,
            locked: c.locked,
        });
        layer_ids.insert(c.id.clone(), Some(lid));
        report.layers += 1;
    }
    if !layer_ids.contains_key("1") {
        layer_ids.insert("1".into(), None);
    }

    let layer_of = |cell: &RawCell| -> Option<LayerId> {
        let mut cur = cell.parent.as_deref();
        let mut guard = 0;
        while let Some(pid) = cur {
            if let Some(l) = layer_ids.get(pid) {
                return *l;
            }
            guard += 1;
            if guard > 256 {
                return None;
            }
            cur = by_id
                .get(pid)
                .and_then(|i| cells.get(*i))
                .and_then(|c| c.parent.as_deref());
        }
        None
    };

    let is_edge_label = |cell: &RawCell| -> bool {
        cell.vertex
            && cell
                .parent
                .as_ref()
                .and_then(|p| by_id.get(p))
                .and_then(|i| cells.get(*i))
                .is_some_and(|p| p.edge)
    };

    let vertex_parent_cell = |cell: &RawCell| -> Option<String> {
        let p = cell.parent.as_deref()?;
        if p == "0" {
            return None;
        }
        let pc = cells.get(*by_id.get(p)?)?;
        if pc.vertex && !pc.edge {
            Some(p.to_string())
        } else {
            None
        }
    };

    // ---- vertices.
    let mut drawio_to_native: HashMap<String, NodeId> = HashMap::new();
    let mut node_order: Vec<(usize, NodeId, Option<String>, Option<LayerId>)> = Vec::new();
    let mut group_ids: Vec<NodeId> = Vec::new();
    for cell in &cells {
        if !cell.vertex || cell.edge || is_edge_label(cell) {
            continue;
        }
        let st = cell.parsed_style();
        let mut rect = Rect::new(cell.geom.x, cell.geom.y, cell.geom.w, cell.geom.h);
        if !cell.geom.has_box && !is_group_style(&st) && !is_container_style(&st) {
            report.approx(
                &name,
                &cell.id,
                "geometry",
                "missing width/height; used 120 x 60",
            );
            rect.w = 120.0;
            rect.h = 60.0;
        }
        if rect.w < 0.0 || rect.h < 0.0 {
            report.approx(&name, &cell.id, "geometry", "negative size clamped to zero");
            rect.w = rect.w.max(0.0);
            rect.h = rect.h.max(0.0);
        }
        let id = doc.alloc_id();
        let (mut node, approximation) = build_vertex(cell, &st, rect);
        if let Some(reason) = approximation {
            report.approx(&name, &cell.id, "shape", reason);
        }
        if is_group_style(&st) {
            group_ids.push(id);
        }
        let parent_cell = vertex_parent_cell(cell);
        let layer = if parent_cell.is_none() {
            layer_of(cell)
        } else {
            None
        };
        node.id = id;
        node.parent = None;
        node.children.clear();
        node.visible = cell.visible;
        node.locked = cell.locked;
        node.layer = layer;
        apply_cell_data(&mut node, cell);
        drawio_to_native.insert(cell.id.clone(), id);
        node_order.push((cell.order, id, parent_cell, layer));
        doc.nodes.insert(id, node);
        report.shapes += 1;
    }

    // ---- connectors. Vertices are mapped first so bindings can resolve.
    let parent_origin = |cell: &RawCell| -> V2 {
        let Some(p) = cell.parent.as_deref() else {
            return V2::ZERO;
        };
        if layer_ids.contains_key(p) || p == "0" {
            return V2::ZERO;
        }
        world_origin(p, &cells, &by_id, &layer_ids)
    };

    let mut connector_ids: Vec<NodeId> = Vec::new();
    for cell in &cells {
        if !cell.edge {
            continue;
        }
        let st = cell.parsed_style();
        let origin = parent_origin(cell);
        let source_id = cell
            .source
            .as_ref()
            .and_then(|s| drawio_to_native.get(s))
            .copied();
        let target_id = cell
            .target
            .as_ref()
            .and_then(|s| drawio_to_native.get(s))
            .copied();
        let mut world: Vec<V2> = Vec::new();
        if let Some(p) = cell.geom.source {
            world.push(origin + p);
        } else if let Some(p) = cell
            .source
            .as_ref()
            .and_then(|src| cell_center(src, &cells, &by_id, &layer_ids))
        {
            world.push(p);
        }
        for p in &cell.geom.points {
            world.push(origin + *p);
        }
        if let Some(p) = cell.geom.target {
            world.push(origin + p);
        } else if let Some(p) = cell
            .target
            .as_ref()
            .and_then(|tgt| cell_center(tgt, &cells, &by_id, &layer_ids))
        {
            world.push(p);
        }
        if world.len() < 2
            && let (Some(s), Some(t)) = (source_id, target_id)
            && let (Some(sr), Some(tr)) = (doc.world_rect(s), doc.world_rect(t))
        {
            world = vec![sr.center(), tr.center()];
        }
        if world.len() < 2 {
            report.drop_cell(
                &name,
                &cell.id,
                "edge",
                "edge has neither two endpoints nor source/target cells",
            );
            continue;
        }
        let bbox = Rect::union_all(
            &world
                .iter()
                .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
        let native_parent =
            vertex_parent_cell(cell).and_then(|p| drawio_to_native.get(&p).copied());
        let local: Vec<V2> = world.iter().map(|p| *p - bbox.min()).collect();
        let parent_off = doc.parent_origin(native_parent);
        let route = route_for(&st);
        let start_head = head_for(st.get("startArrow")).unwrap_or(Head::None);
        let end_head = head_for(st.get("endArrow")).unwrap_or(Head::Arrow);
        let start_head_size = st.num("startSize").unwrap_or(DEFAULT_HEAD_SIZE);
        let end_head_size = st.num("endSize").unwrap_or(DEFAULT_HEAD_SIZE);
        let start_head_offset = st.num("sourcePerimeterSpacing").unwrap_or(0.0);
        let end_head_offset = st.num("targetPerimeterSpacing").unwrap_or(0.0);
        let exit = st.num("exitX").zip(st.num("exitY"));
        let entry = st.num("entryX").zip(st.num("entryY"));
        let start = source_id.map(|node| Binding {
            node,
            fixed: exit.map_or(v2(0.5, 0.5), |(x, y)| v2(x, y)),
            inside: exit.is_some() && st.get("exitPerimeter") == Some("0"),
            floating: exit.is_none(),
        });
        let end = target_id.map(|node| Binding {
            node,
            fixed: entry.map_or(v2(0.5, 0.5), |(x, y)| v2(x, y)),
            inside: entry.is_some() && st.get("entryPerimeter") == Some("0"),
            floating: entry.is_none(),
        });
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: local,
                start,
                end,
                route,
                start_head,
                end_head,
                jump: jump_for(&st),
                jump_size: st.num("jumpSize").unwrap_or(6.0),
            },
            Rect::new(bbox.x - parent_off.x, bbox.y - parent_off.y, bbox.w, bbox.h),
        );
        node.start_head_size = start_head_size;
        node.end_head_size = end_head_size;
        node.start_head_offset = start_head_offset;
        node.end_head_offset = end_head_offset;
        node.name = if plain_text(&cell.value).is_empty() {
            "Arrow".into()
        } else {
            plain_text(&cell.value)
        };
        node.flow = flow_for(&st);
        apply_style(&mut node, &st, StyleTarget::Edge);
        node.label = plain_text(&cell.value);
        if !node.label.is_empty() {
            node.label_size = Some(st.num("fontSize").unwrap_or(11.0));
            let font_pair = st.color_pair("fontColor").unwrap_or((Color::BLACK, None));
            node.label_color = Some(font_pair.0);
            node.label_color_dark = font_pair.1;
        }
        if cell.geom.has_x {
            node.label_pos = Some(((cell.geom.x.clamp(-1.0, 1.0) + 1.0) * 0.5).clamp(0.0, 1.0));
        }
        node.label_offset = cell.geom.offset;
        apply_label_style(&mut node, &st);
        if route == Route::Elbow && !cell.geom.points.is_empty() {
            report.approx(
                &name,
                &cell.id,
                "edge",
                "orthogonal edge is re-routed; fixed waypoints are not preserved",
            );
        }
        node.parent = None;
        node.children.clear();
        node.visible = cell.visible;
        node.locked = cell.locked;
        node.layer = if native_parent.is_none() {
            layer_of(cell)
        } else {
            None
        };
        apply_cell_data(&mut node, cell);
        let id = doc.alloc_id();
        node.id = id;
        drawio_to_native.insert(cell.id.clone(), id);
        node_order.push((cell.order, id, vertex_parent_cell(cell), node.layer));
        doc.nodes.insert(id, node);
        connector_ids.push(id);
        report.connectors += 1;
    }

    // ---- parent links and paint order.
    node_order.sort_by_key(|(order, _, _, _)| *order);
    for (_, id, parent_cell, _) in &node_order {
        let parent = parent_cell
            .as_ref()
            .and_then(|p| drawio_to_native.get(p))
            .copied();
        if let Some(n) = doc.nodes.get_mut(id) {
            n.parent = parent;
        }
        if let Some(p) = parent {
            if let Some(pn) = doc.nodes.get_mut(&p) {
                pn.children.push(*id);
            } else {
                doc.root.push(*id);
            }
        } else {
            doc.root.push(*id);
        }
    }

    // ---- native groups keep their rect as the union of their children.
    group_ids.sort_by_key(|id| std::cmp::Reverse(group_depth(&doc, *id)));
    for id in group_ids {
        normalize_group(&mut doc, id);
    }

    // ---- connector points are stored node-local and fitted to the routed bounds.
    for id in connector_ids {
        refit_connector(&mut doc, id);
    }

    // ---- extra edge labels: child cells of an edge.
    for cell in &cells {
        if !cell.vertex || !is_edge_label(cell) {
            continue;
        }
        let Some(parent) = cell.parent.as_ref().and_then(|p| drawio_to_native.get(p)) else {
            report.drop_cell(&name, &cell.id, "edge label", "label's edge is missing");
            continue;
        };
        let Some(parent) = doc.nodes.get_mut(parent) else {
            continue;
        };
        if !parent.is_connector() {
            report.drop_cell(
                &name,
                &cell.id,
                "edge label",
                "label's parent is not a connector",
            );
            continue;
        }
        let text = plain_text(&cell.value);
        if text.is_empty() {
            continue;
        }
        let pos = Some(((cell.geom.x.clamp(-1.0, 1.0) + 1.0) * 0.5).clamp(0.0, 1.0));
        if parent.label.is_empty() {
            parent.label = text;
            parent.label_pos = pos;
            parent.label_offset = cell.geom.offset;
        } else {
            parent.labels.push(crate::model::ExtraLabel {
                text,
                pos,
                offset: cell.geom.offset,
            });
        }
    }

    doc.validate()
        .map_err(|e| format!("imported diagram \"{name}\" is invalid: {e}"))?;

    Ok(crate::project::PageData {
        id,
        name,
        doc,
        camera: None,
    })
}

fn world_origin(
    id: &str,
    cells: &[RawCell],
    by_id: &HashMap<String, usize>,
    layers: &HashMap<String, Option<LayerId>>,
) -> V2 {
    let Some(cell) = by_id.get(id).and_then(|i| cells.get(*i)) else {
        return V2::ZERO;
    };
    if layers.contains_key(id) || id == "0" {
        return V2::ZERO;
    }
    let base = cell
        .parent
        .as_deref()
        .map(|p| world_origin(p, cells, by_id, layers))
        .unwrap_or(V2::ZERO);
    base + v2(cell.geom.x, cell.geom.y)
}

fn cell_center(
    id: &str,
    cells: &[RawCell],
    by_id: &HashMap<String, usize>,
    layers: &HashMap<String, Option<LayerId>>,
) -> Option<V2> {
    let cell = by_id.get(id).and_then(|i| cells.get(*i))?;
    Some(world_origin(id, cells, by_id, layers) + v2(cell.geom.w * 0.5, cell.geom.h * 0.5))
}

fn group_depth(doc: &Document, id: NodeId) -> usize {
    doc.ancestors(id).len()
}

fn normalize_group(doc: &mut Document, id: NodeId) {
    let Some(group) = doc.nodes.get(&id) else {
        return;
    };
    let children = group.children.clone();
    let union = Rect::union_all(
        &children
            .iter()
            .filter_map(|c| doc.nodes.get(c))
            .map(|c| c.rect)
            .collect::<Vec<_>>(),
    );
    let Some(u) = union else { return };
    let Some(group) = doc.nodes.get_mut(&id) else {
        return;
    };
    let base = group.rect.min();
    group.rect = Rect::new(base.x + u.x, base.y + u.y, u.w, u.h);
    for c in children {
        if let Some(n) = doc.nodes.get_mut(&c) {
            n.rect.x -= u.x;
            n.rect.y -= u.y;
        }
    }
}

fn refit_connector(doc: &mut Document, id: NodeId) {
    let Some(points) = crate::connector::resolve(doc, id) else {
        return;
    };
    if points.len() < 2 {
        return;
    }
    let Some(bbox) = Rect::union_all(
        &points
            .iter()
            .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
            .collect::<Vec<_>>(),
    ) else {
        return;
    };
    let Some(n) = doc.nodes.get(&id) else { return };
    let parent_off = doc.parent_origin(n.parent);
    let local: Vec<V2> = points.iter().map(|p| *p - bbox.min()).collect();
    let rect = Rect::new(bbox.x - parent_off.x, bbox.y - parent_off.y, bbox.w, bbox.h);
    let Some(n) = doc.nodes.get_mut(&id) else {
        return;
    };
    n.rect = rect;
    if let NodeKind::Connector { points, .. } = &mut n.kind {
        *points = local;
    }
}

// --------------------------------------------------------------------------------- styles

struct ParsedStyle {
    map: HashMap<String, String>,
}

impl ParsedStyle {
    fn parse(s: &str) -> ParsedStyle {
        let mut map = HashMap::new();
        for part in s.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match part.split_once('=') {
                Some((k, v)) => {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
                None => {
                    map.insert(part.to_string(), String::new());
                }
            }
        }
        ParsedStyle { map }
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    fn has(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    fn num(&self, key: &str) -> Option<f64> {
        self.get(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }

    fn flag(&self, key: &str) -> bool {
        self.get(key).is_some_and(|v| v == "1" || v == "true")
    }

    fn color_pair(&self, key: &str) -> Option<(Color, Option<Color>)> {
        let v = self.get(key)?;
        if v.eq_ignore_ascii_case("none") {
            return None;
        }
        parse_color_pair(v)
    }
}

/// A `light-dark(light,dark)` value, or a fixed light color with no dark variant.
fn parse_color_pair(s: &str) -> Option<(Color, Option<Color>)> {
    let s = s.trim();
    let inner = s
        .get(..11)
        .filter(|p| p.eq_ignore_ascii_case("light-dark("))
        .and_then(|_| s.get(11..))
        .and_then(|rest| rest.strip_suffix(')'));
    let Some(inner) = inner else {
        return parse_color(s).map(|c| (c, None));
    };
    let mut depth = 0usize;
    let mut split = None;
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                split = Some(i);
                break;
            }
            _ => {}
        }
    }
    let (light, dark) = split.map_or((inner, ""), |i| (&inner[..i], &inner[i + 1..]));
    let light = if light.trim().eq_ignore_ascii_case("none") {
        None
    } else {
        Some(parse_color(light)?)
    };
    let dark = if dark.trim().is_empty() || dark.trim().eq_ignore_ascii_case("none") {
        None
    } else {
        Some(parse_color(dark)?)
    };
    light.map(|c| (c, dark))
}

fn color_value(light: Color, dark: Option<Color>) -> String {
    match dark {
        Some(d) => format!("light-dark({},{})", light.to_hex(), d.to_hex()),
        None => light.to_hex(),
    }
}

fn solid_pair((light, dark): (Color, Option<Color>)) -> Paint {
    match dark {
        Some(d) => Paint::solid_pair(light, d),
        None => Paint::solid(light),
    }
}

fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let p = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        return match hex.len() {
            6 => Some(Color::rgb(p(0)?, p(2)?, p(4)?)),
            8 => Some(Color::rgba(p(0)?, p(2)?, p(4)?, p(6)?)),
            3 => {
                let d = |i: usize| {
                    u8::from_str_radix(hex.get(i..i + 1)?, 16)
                        .ok()
                        .map(|v| v * 17)
                };
                Some(Color::rgb(d(0)?, d(1)?, d(2)?))
            }
            _ => None,
        };
    }
    let lower = s.to_ascii_lowercase();
    let (r, g, b) = match lower.as_str() {
        "white" => (255, 255, 255),
        "black" => (0, 0, 0),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "gray" | "grey" => (128, 128, 128),
        "lightgray" | "lightgrey" => (211, 211, 211),
        "darkgray" | "darkgrey" => (169, 169, 169),
        _ => return None,
    };
    Some(Color::rgb(r, g, b))
}

#[derive(Clone, Copy)]
enum StyleTarget {
    Shape,
    Registry,
    Text,
    Edge,
    Group,
    Frame,
}

fn is_group_style(st: &ParsedStyle) -> bool {
    st.has("group") && !st.has("swimlane") && !st.flag("container")
}

fn is_container_style(st: &ParsedStyle) -> bool {
    st.has("swimlane") || st.flag("container") || st.has("table") || st.has("tableRow")
}

fn is_text_style(st: &ParsedStyle) -> bool {
    st.has("text") && !st.flag("container") && !st.has("swimlane")
}

fn shape_name(st: &ParsedStyle) -> Option<&str> {
    if let Some(s) = st.get("shape").filter(|s| !s.is_empty()) {
        return Some(s);
    }
    // draw.io writes its built-in shapes as a bare leading token, for example
    // `ellipse;whiteSpace=wrap;html=1;`. The importer must accept both that
    // shorthand and the explicit `shape=ellipse;` form.
    const SHORTHAND: &[&str] = &[
        "ellipse",
        "doubleellipse",
        "rhombus",
        "triangle",
        "hexagon",
        "star",
        "line",
        "image",
    ];
    SHORTHAND.iter().copied().find(|name| st.has(name))
}

fn leaves(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn registry_for_shape(shape: &str) -> Option<&'static str> {
    let leaf = leaves(shape).to_ascii_lowercase();
    let id = match leaf.as_str() {
        "database" | "cylinder" | "cylinder3" | "datastore" | "internalstorage" => "database",
        "document" => "document",
        "note" => "note",
        "cloud" => "cloud",
        "actor" | "umlactor" => "actor",
        "queue" => "queue",
        "delay" => "delay",
        "parallelogram" | "data" => "data",
        "trapezoid" | "manualoperation" | "manual-operation" => "manual-operation",
        "manualinput" | "manual-input" => "manual-input",
        "preparation" | "step" => "preparation",
        "offpageconnector" | "off-page" => "off-page",
        "or" | "onpageconnector" | "on-page-connector" => "on-page-connector",
        "package" => "package",
        "host" => "host",
        "server" => "server",
        "firewall" => "firewall",
        "door" => "door",
        "window" => "window",
        "stairs" => "stairs",
        "person" => "person",
        "class" => "class",
        "weakentity" | "weak-entity" => "weak-entity",
        "attribute" => "attribute",
        "keyattribute" | "key-attribute" => "key-attribute",
        "derivedattribute" | "derived-attribute" => "derived-attribute",
        "entity" => "entity",
        "relationship" => "relationship",
        "terminator" => "terminator",
        "process" => "process",
        "decision" => "decision",
        _ => return None,
    };
    Some(id)
}

fn build_vertex(cell: &RawCell, st: &ParsedStyle, rect: Rect) -> (Node, Option<String>) {
    let value = plain_text(&cell.value);
    let mut approximation = None;
    let shape = shape_name(st);
    let mut node = if is_group_style(st) {
        let mut n = Node::new(NodeId(0), NodeKind::Group, rect);
        apply_style(&mut n, st, StyleTarget::Group);
        n.name = if value.is_empty() {
            "Group".into()
        } else {
            value.clone()
        };
        n
    } else if is_container_style(st) {
        if st.has("table") || st.has("tableRow") {
            approximation = Some("table cells are imported as a group".into());
            let mut n = Node::new(NodeId(0), NodeKind::Group, rect);
            apply_style(&mut n, st, StyleTarget::Group);
            n.name = if value.is_empty() {
                "Table".into()
            } else {
                value.clone()
            };
            n
        } else {
            approximation = Some("swimlane title band is imported as a frame title".into());
            let mut n = Node::new(NodeId(0), NodeKind::Frame { clip: false }, rect);
            apply_style(&mut n, st, StyleTarget::Frame);
            n.name = if value.is_empty() {
                "Frame".into()
            } else {
                value.clone()
            };
            n
        }
    } else if is_text_style(st) {
        let font_size = st.num("fontSize").unwrap_or(11.0);
        let align = align_for(st.get("align"));
        let auto_width = !st.flag("whiteSpace") && st.get("whiteSpace") != Some("wrap");
        let mut n = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: value.clone(),
                font_size,
                auto_width,
                align,
            },
            rect,
        );
        apply_style(&mut n, st, StyleTarget::Text);
        apply_text_style(&mut n, st);
        n.name = if value.is_empty() {
            "Text".into()
        } else {
            first_line(&value)
        };
        n
    } else if shape == Some("image") || st.get("image").is_some() {
        let mut n = Node::new(NodeId(0), NodeKind::Rect, rect);
        apply_style(&mut n, st, StyleTarget::Shape);
        n.name = if value.is_empty() {
            "Image".into()
        } else {
            first_line(&value)
        };
        if let Some(uri) = st.get("image") {
            if let Some(asset) = register_image(uri) {
                n.style.fills = vec![Paint::Image {
                    asset,
                    mode: ImageMode::Fill,
                    opacity: 1.0,
                }];
                n.style.fill_enabled = true;
                n.style.fill_auto = false;
            } else {
                approximation = Some("image is a remote URL; imported as a plain rectangle".into());
            }
        }
        n
    } else if let Some(reg) = shape.map(leaves).and_then(registry_for_shape) {
        let Some(def) = crate::registry::by_id(reg) else {
            let mut n = Node::new(NodeId(0), NodeKind::Rect, rect);
            apply_style(&mut n, st, StyleTarget::Shape);
            n.name = first_line(&value);
            return (n, approximation);
        };
        let mut n = crate::registry::build(def, rect, None);
        apply_style(&mut n, st, StyleTarget::Registry);
        n.name = def.name.into();
        n.label = value.clone();
        apply_label_style(&mut n, st);
        return (n, approximation);
    } else {
        let kind = match shape.map(leaves).unwrap_or("") {
            "ellipse" => NodeKind::Ellipse,
            "doubleellipse" => {
                approximation = Some("inner ellipse is not represented".into());
                NodeKind::Ellipse
            }
            "rhombus" => NodeKind::Polygon { sides: 4 },
            "triangle" => NodeKind::Polygon { sides: 3 },
            "hexagon" => NodeKind::Polygon { sides: 6 },
            "star" => NodeKind::Star {
                points: 5,
                inner: 0.5,
            },
            "line" => {
                approximation = Some("line shape imported as a rectangle".into());
                NodeKind::Rect
            }
            "" => NodeKind::Rect,
            other => {
                approximation = Some(format!(
                    "unknown draw.io shape \"{other}\" imported as a rectangle"
                ));
                NodeKind::Rect
            }
        };
        let mut n = Node::new(NodeId(0), kind, rect);
        apply_style(&mut n, st, StyleTarget::Shape);
        n.label = value.clone();
        n.name = if value.is_empty() {
            n.kind.type_name().into()
        } else {
            first_line(&value)
        };
        apply_label_style(&mut n, st);
        n
    };
    node.id = NodeId(0);
    (node, approximation)
}

fn register_image(uri: &str) -> Option<String> {
    let data = uri.strip_prefix("data:")?;
    let (meta, body) = data.split_once(',')?;
    if !meta.contains(";base64") {
        return None;
    }
    let bytes = crate::assets::base64_decode(body)?;
    crate::assets::register(&bytes).ok()
}

fn apply_style(n: &mut Node, st: &ParsedStyle, target: StyleTarget) {
    let width = st.num("strokeWidth").unwrap_or(1.0).max(0.0);
    let opacity = st
        .num("opacity")
        .map(|v| (v / 100.0).clamp(0.0, 1.0))
        .unwrap_or(1.0);
    let dash = if let Some(pattern) = st.get("dashPattern") {
        pattern
            .split_whitespace()
            .filter_map(|v| v.parse::<f64>().ok())
            .filter(|v| *v >= 0.0)
            .collect::<Vec<_>>()
    } else if st.flag("dashed") {
        vec![4.0, 4.0]
    } else {
        Vec::new()
    };
    match target {
        StyleTarget::Text => {
            let pair = st.color_pair("fontColor").unwrap_or((Color::BLACK, None));
            n.style.fills = vec![solid_pair(pair)];
            n.style.fill_enabled = true;
            n.style.fill_auto = false;
            n.style.stroke = None;
            n.style.stroke_dark = None;
            n.style.stroke_enabled = false;
            n.style.opacity = opacity;
        }
        StyleTarget::Group => {
            if st.has("fillColor") {
                match st.color_pair("fillColor") {
                    Some(pair) => {
                        n.style.fills = vec![solid_pair(pair)];
                        n.style.fill_enabled = true;
                    }
                    None => {
                        n.style.fills.clear();
                        n.style.fill_enabled = false;
                    }
                }
            }
            if st.has("strokeColor") {
                let pair = st.color_pair("strokeColor");
                n.style.stroke = pair.map(|(c, _)| c);
                n.style.stroke_dark = pair.and_then(|(_, d)| d);
                n.style.stroke_enabled = n.style.stroke.is_some();
                n.style.stroke_width = width;
            }
            n.style.opacity = opacity;
            n.style.dash = dash;
        }
        StyleTarget::Edge => {
            n.style.fills.clear();
            n.style.fill_enabled = false;
            let pair = st.color_pair("strokeColor").unwrap_or((Color::BLACK, None));
            n.style.stroke = Some(pair.0);
            n.style.stroke_dark = pair.1;
            n.style.stroke_enabled = true;
            n.style.stroke_width = width;
            n.style.dash = dash;
            n.style.opacity = opacity;
            n.style.corner_radius = 0.0;
        }
        StyleTarget::Registry => {
            if st.has("fillColor") {
                match st.color_pair("fillColor") {
                    Some(pair) => {
                        n.style.fills = vec![solid_pair(pair)];
                        n.style.fill_enabled = true;
                    }
                    None => {
                        n.style.fills.clear();
                        n.style.fill_enabled = false;
                    }
                }
                n.style.fill_auto = false;
            }
            if st.has("strokeColor") {
                let pair = st.color_pair("strokeColor");
                n.style.stroke = pair.map(|(c, _)| c);
                n.style.stroke_dark = pair.and_then(|(_, d)| d);
                n.style.stroke_enabled = n.style.stroke.is_some();
                n.style.stroke_width = width;
            }
            n.style.dash = dash;
            n.style.opacity = opacity;
        }
        StyleTarget::Shape | StyleTarget::Frame => {
            let fill_pair = if st.has("fillColor") {
                st.color_pair("fillColor")
            } else {
                Some((Color::WHITE, None))
            };
            if let (Some((from, from_dark)), Some((to, to_dark))) =
                (fill_pair, st.color_pair("gradientColor"))
            {
                let (a, b) = match st.get("gradientDirection").unwrap_or("south") {
                    "east" => (v2(0.0, 0.5), v2(1.0, 0.5)),
                    "west" => (v2(1.0, 0.5), v2(0.0, 0.5)),
                    "north" => (v2(0.5, 1.0), v2(0.5, 0.0)),
                    _ => (v2(0.5, 0.0), v2(0.5, 1.0)),
                };
                n.style.fills = vec![Paint::Linear {
                    stops: vec![
                        crate::model::Stop {
                            pos: 0.0,
                            color: from,
                            dark: from_dark,
                        },
                        crate::model::Stop {
                            pos: 1.0,
                            color: to,
                            dark: to_dark,
                        },
                    ],
                    from: a,
                    to: b,
                }];
                n.style.fill_enabled = true;
            } else {
                n.style.fills = fill_pair.map(solid_pair).into_iter().collect();
                n.style.fill_enabled = fill_pair.is_some();
            }
            n.style.fill_auto = false;
            let stroke_pair = st.color_pair("strokeColor").unwrap_or((Color::BLACK, None));
            n.style.stroke = Some(stroke_pair.0);
            n.style.stroke_dark = stroke_pair.1;
            n.style.stroke_enabled = true;
            n.style.stroke_width = width;
            n.style.dash = dash;
            n.style.opacity = opacity;
            if st.flag("rounded") {
                let pct = st.num("arcSize").unwrap_or(15.0).clamp(0.0, 100.0);
                n.style.corner_radius =
                    (n.rect.w.min(n.rect.h) * pct / 100.0).clamp(0.0, n.rect.w.min(n.rect.h) * 0.5);
            }
            if let Some(r) = st.num("rotation") {
                n.style.rotation = r;
            }
            if st.has("shadow") {
                n.style.shadow = Some(st.flag("shadow"));
            }
            if st.has("sketch") {
                n.style.sketch = Some(st.flag("sketch"));
            }
            if st.has("glass") {
                n.style.glass = st.flag("glass");
            }
        }
    }
}

fn apply_font_family(n: &mut Node, st: &ParsedStyle) {
    if let Some(family) = st.get("fontFamily")
        && !family.is_empty()
        && !family.eq_ignore_ascii_case("helvetica")
    {
        n.text_style.family = Some(family.to_string());
    }
}

fn apply_text_style(n: &mut Node, st: &ParsedStyle) {
    if let Some(style) = st.num("fontStyle") {
        let bits = style as i32;
        n.text_style.bold = bits & 1 != 0;
        n.text_style.italic = bits & 2 != 0;
    }
    apply_font_family(n, st);
    if let Some(align) = st.get("align") {
        n.text_style.align = Some(align_for(Some(align)));
    }
    if st.flag("textPath") {
        n.text_style.curve = Some(0.6);
    }
}

fn apply_label_style(n: &mut Node, st: &ParsedStyle) {
    if !n.supports_label() && !n.is_connector() {
        return;
    }
    n.label_size = Some(st.num("fontSize").unwrap_or(11.0));
    let font_pair = st.color_pair("fontColor").unwrap_or((Color::BLACK, None));
    n.label_color = Some(font_pair.0);
    n.label_color_dark = font_pair.1;
    if let Some(align) = st.get("align") {
        n.text_style.align = Some(align_for(Some(align)));
    }
    n.label_valign = match st.get("verticalAlign") {
        Some("top") => Some(VAlign::Top),
        Some("bottom") => Some(VAlign::Bottom),
        Some("middle") | None => Some(VAlign::Middle),
        _ => None,
    };
    n.label_wrap = st.get("whiteSpace") != Some("nowrap");
    if let Some(opacity) = st.num("textOpacity") {
        n.label_opacity = (opacity / 100.0).clamp(0.0, 1.0);
    }
    if let Some(pair) = st.color_pair("labelBackgroundColor") {
        n.text_bg = Some(pair.0);
        n.text_bg_dark = pair.1;
    }
    if let Some(pair) = st.color_pair("labelBorderColor") {
        n.text_border = Some(pair.0);
        n.text_border_dark = pair.1;
    }
    apply_font_family(n, st);
    if st.flag("textShadow") || st.flag("fontShadow") {
        n.text_shadow = true;
    }
    if st.flag("textPath") {
        n.text_style.curve = Some(0.6);
    }
}

fn align_for(value: Option<&str>) -> TextAlign {
    match value {
        Some("left") => TextAlign::Left,
        Some("right") => TextAlign::Right,
        _ => TextAlign::Center,
    }
}

fn route_for(st: &ParsedStyle) -> Route {
    if st.flag("curved") {
        return Route::Curved;
    }
    match st.get("edgeStyle") {
        Some("orthogonalEdgeStyle") | Some("elbowEdgeStyle") | Some("entityRelationEdgeStyle") => {
            Route::Elbow
        }
        _ => Route::Straight,
    }
}

fn head_for(value: Option<&str>) -> Option<Head> {
    match value {
        Some("none") => Some(Head::None),
        Some("classic") | Some("open") | Some("openAsync") | Some("async") | Some("halfCircle") => {
            Some(Head::Arrow)
        }
        Some("block") | Some("box") | Some("triangle") | Some("doubleBlock") => {
            Some(Head::Triangle)
        }
        Some("oval") | Some("circle") | Some("dot") => Some(Head::Dot),
        Some("diamond") | Some("diamondThin") => Some(Head::Diamond),
        Some("bar") | Some("line") => Some(Head::Bar),
        _ => None,
    }
}

fn jump_for(st: &ParsedStyle) -> LineJump {
    match st.get("jumpStyle") {
        Some("arc") => LineJump::Arc,
        Some("gap") => LineJump::Gap,
        Some("sharp") => LineJump::Sharp,
        Some("line") => LineJump::Line,
        _ => LineJump::None,
    }
}

/// draw.io's flow style keys, including the older `flowDuration` spelling.
fn flow_for(st: &ParsedStyle) -> FlowAnimation {
    let duration = st
        .num("flowAnimationDuration")
        .or_else(|| st.num("flowDuration"))
        .filter(|d| *d > 0.0 && d.is_finite())
        .unwrap_or(FlowAnimation::DEFAULT_DURATION_MS);
    FlowAnimation {
        enabled: st.flag("flowAnimation"),
        duration_ms: duration,
        timing: FlowTiming::from_style(st.get("flowAnimationTimingFunction").unwrap_or("linear")),
        direction: FlowDirection::from_style(st.get("flowAnimationDirection").unwrap_or("normal")),
    }
}

/// Convert draw.io's HTML-ish label to plain text: `<br>` becomes a newline, other
/// tags are removed, and common entities are decoded.
pub fn plain_text(value: &str) -> String {
    let mut s = String::with_capacity(value.len());
    let mut it = value.chars().peekable();
    while let Some(c) = it.next() {
        if c == '<' {
            let mut tag = String::new();
            for d in it.by_ref() {
                if d == '>' {
                    break;
                }
                tag.push(d);
            }
            let lower = tag.trim().to_ascii_lowercase();
            let br = lower == "br" || lower == "br/" || lower.starts_with("br ");
            let div = (lower == "/div" || lower.starts_with("div"))
                && !s.is_empty()
                && !s.ends_with('\n');
            if br || div {
                s.push('\n');
            }
            continue;
        }
        if c == '&' {
            let mut ent = String::new();
            let mut found = false;
            while let Some(&d) = it.peek() {
                if d == ';' {
                    it.next();
                    found = true;
                    break;
                }
                if ent.len() > 12 || d.is_whitespace() {
                    break;
                }
                ent.push(d);
                it.next();
            }
            if found {
                let decoded = match ent.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    _ => ent
                        .strip_prefix('#')
                        .and_then(|n| {
                            if let Some(hex) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                                u32::from_str_radix(hex, 16).ok()
                            } else {
                                n.parse::<u32>().ok()
                            }
                        })
                        .and_then(char::from_u32),
                };
                if let Some(ch) = decoded {
                    s.push(ch);
                } else {
                    s.push('&');
                    s.push_str(&ent);
                    s.push(';');
                }
                continue;
            }
            s.push('&');
            s.push_str(&ent);
            continue;
        }
        s.push(c);
    }
    s.replace("<br>", "\n").trim().to_string()
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").to_string()
}

fn apply_model_settings(doc: &mut Document, model: XmlNode<'_, '_>) {
    if let Some(v) = model.attribute("grid") {
        doc.settings.grid = v != "0";
    }
    if let Some(v) = num_attr(model, "gridSize") {
        doc.settings.grid_size = v.clamp(1.0, 1000.0);
    }
    if let Some(v) = model.attribute(SKETCH_GRID_COLOR)
        && let Some(c) = parse_color(v)
    {
        doc.settings.grid_color = Some(c);
    }
    doc.settings.page_transition = match model.attribute(SKETCH_PAGE_TRANSITION) {
        Some("fade") => PageTransition::Fade,
        _ => PageTransition::None,
    };
    if let Some(v) = num_attr(model, SKETCH_PAGE_TRANSITION_MS) {
        doc.settings.page_transition_ms = v.clamp(100.0, 3000.0) as u64;
    }
    if let Some(v) = model.attribute("shadow") {
        doc.settings.shadow = v == "1";
    }
    doc.settings.adaptive_colors = match model.attribute("adaptiveColors") {
        Some("simple") => AdaptiveColors::Simple,
        Some("none") => AdaptiveColors::None,
        _ => AdaptiveColors::Auto,
    };
    if let Some(name) = model.attribute("sketchScheme")
        && crate::model::scheme::SCHEMES.iter().any(|s| s.name == name)
    {
        doc.settings.scheme = Some(name.to_string());
    }
    if let Some((c, dark)) = model.attribute("background").and_then(parse_color_pair) {
        doc.settings.background_enabled = c.a > 0;
        doc.settings.background_color = c;
        doc.settings.background_color_dark = dark;
    }
}

// --------------------------------------------------------------------------------- export

/// Serialize native pages as an uncompressed draw.io `mxfile`. A single `&str` result
/// keeps the existing export pipeline simple (one file, every page inside it).
pub fn export(pages: &[(&str, &Document)]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "<mxfile host=\"sketch\" modified=\"{now}\" agent=\"sketch\" version=\"24.0.0\" type=\"device\">"
    );
    for (i, (name, doc)) in pages.iter().enumerate() {
        let (pw, ph) = doc.settings.paper_size();
        out.push_str("  <diagram id=\"");
        out.push_str(&format!("page-{}", i + 1));
        out.push_str("\" name=\"");
        attr(&mut out, name);
        out.push_str("\">\n    <mxGraphModel dx=\"0\" dy=\"0\" grid=\"");
        out.push_str(if doc.settings.grid { "1" } else { "0" });
        out.push_str("\" gridSize=\"");
        out.push_str(&fmt_num(doc.settings.grid_size));
        out.push_str("\" guides=\"1\" tooltips=\"1\" connect=\"1\" arrows=\"1\" fold=\"1\" page=\"1\" pageScale=\"1\" pageWidth=\"");
        out.push_str(&fmt_num(pw));
        out.push_str("\" pageHeight=\"");
        out.push_str(&fmt_num(ph));
        out.push_str("\" math=\"0\" shadow=\"");
        out.push_str(if doc.settings.shadow { "1" } else { "0" });
        out.push_str("\" adaptiveColors=\"");
        out.push_str(match doc.settings.adaptive_colors {
            AdaptiveColors::Auto => "auto",
            AdaptiveColors::Simple => "simple",
            AdaptiveColors::None => "none",
        });
        out.push('"');
        if doc.settings.background_enabled || doc.settings.background_color_dark.is_some() {
            out.push_str(" background=\"");
            out.push_str(&color_value(
                doc.settings.background_color,
                doc.settings.background_color_dark,
            ));
            out.push('"');
        }
        if let Some(c) = doc.settings.grid_color {
            out.push(' ');
            out.push_str(SKETCH_GRID_COLOR);
            out.push_str("=\"");
            out.push_str(&c.to_hex());
            out.push('"');
        }
        if doc.settings.page_transition == PageTransition::Fade {
            out.push(' ');
            out.push_str(SKETCH_PAGE_TRANSITION);
            out.push_str("=\"fade\" ");
            out.push_str(SKETCH_PAGE_TRANSITION_MS);
            out.push_str("=\"");
            out.push_str(&doc.settings.page_transition_ms.to_string());
            out.push('"');
        }
        if let Some(name) = &doc.settings.scheme {
            out.push_str(" sketchScheme=\"");
            attr(&mut out, name);
            out.push('"');
        }
        out.push_str(">\n      <root>\n");
        out.push_str("        <mxCell id=\"0\" />\n        <mxCell id=\"1\" value=\"");
        attr(&mut out, &doc.layers[0].name);
        out.push_str("\" parent=\"0\"");
        if !doc.layers[0].visible {
            out.push_str(" visible=\"0\"");
        }
        if doc.layers[0].locked {
            out.push_str(" locked=\"1\"");
        }
        out.push_str(" />\n");
        for layer in doc.layers.iter().skip(1) {
            out.push_str("        <mxCell id=\"");
            out.push_str(&layer_cell(layer.id));
            out.push_str("\" value=\"");
            attr(&mut out, &layer.name);
            out.push_str("\" parent=\"0\"");
            if !layer.visible {
                out.push_str(" visible=\"0\"");
            }
            if layer.locked {
                out.push_str(" locked=\"1\"");
            }
            out.push_str(" />\n");
        }
        for id in doc.layered_root() {
            emit_node(&mut out, doc, id, None);
        }
        out.push_str("      </root>\n    </mxGraphModel>\n  </diagram>\n");
    }
    out.push_str("</mxfile>\n");
    out
}

fn emit_node(out: &mut String, doc: &Document, id: NodeId, parent: Option<NodeId>) {
    let Some(n) = doc.get(id) else { return };
    let parent_attr = match parent {
        Some(p) => node_cell(p),
        None => match n.layer {
            Some(l) if doc.layers.first().is_some_and(|b| b.id != l) => layer_cell(l),
            _ => "1".to_string(),
        },
    };
    let mut style = shape_style(n, doc);
    if n.style.opacity < 1.0 {
        let _ = write!(
            style,
            "opacity={};",
            (n.style.opacity * 100.0).round() as i64
        );
    }
    if n.style.rotation != 0.0 {
        let _ = write!(style, "rotation={};", fmt_num(n.style.rotation));
    }
    if let Some(true) = n.style.shadow {
        style.push_str("shadow=1;");
    } else if let Some(false) = n.style.shadow {
        style.push_str("shadow=0;");
    }
    if let Some(true) = n.style.sketch {
        style.push_str("sketch=1;");
    } else if let Some(false) = n.style.sketch {
        style.push_str("sketch=0;");
    }
    if n.style.glass {
        style.push_str("glass=1;");
    }
    style_options(out, &mut style, n);
    let value = match &n.kind {
        NodeKind::Text { text, .. } | NodeKind::Sticky { text, .. } => text.clone(),
        NodeKind::Frame { .. } => n.name.clone(),
        NodeKind::Group => String::new(),
        NodeKind::Connector { .. } => n.label.clone(),
        _ => n.label.clone(),
    };
    let has_data = !n.metadata.is_empty()
        || !n.link.is_empty()
        || !n.tooltip.is_empty()
        || !n.tags.is_empty()
        || n.connector_default.is_some()
        || n.style.label_upright;
    if has_data {
        out.push_str("        <UserObject id=\"");
        out.push_str(&node_cell(id));
        out.push_str("\" label=\"");
        attr(out, &value);
        out.push('"');
        if !n.link.is_empty() {
            out.push_str(" link=\"");
            attr(out, &n.link);
            out.push('"');
        }
        if !n.tooltip.is_empty() {
            out.push_str(" tooltip=\"");
            attr(out, &n.tooltip);
            out.push('"');
        }
        if !n.tags.is_empty() {
            out.push_str(" tags=\"");
            attr(out, &n.tags.join(","));
            out.push('"');
        }
        let mut sidecar = std::collections::BTreeMap::new();
        for (key, value) in &n.metadata {
            if xml_name_ok(key) && !is_reserved_wrapper_attr(key) {
                out.push(' ');
                out.push_str(key);
                out.push_str("=\"");
                attr(out, value);
                out.push('"');
            } else {
                sidecar.insert(key.clone(), value.clone());
            }
        }
        if let Some(ds) = &n.connector_default
            && let Ok(json) = serde_json::to_string(ds)
        {
            sidecar.insert(SKETCH_CONNECTOR_DEFAULT.to_string(), json);
        }
        if n.style.label_upright {
            sidecar.insert(SKETCH_LABEL_UPRIGHT.to_string(), "1".into());
        }
        if !sidecar.is_empty() {
            out.push_str(" sketch-data=\"");
            attr(out, &serde_json::to_string(&sidecar).unwrap_or_default());
            out.push('"');
        }
        out.push_str(">\n          <mxCell style=\"");
        attr(out, &style);
        out.push('"');
    } else {
        out.push_str("        <mxCell id=\"");
        out.push_str(&node_cell(id));
        out.push_str("\" value=\"");
        attr(out, &value);
        out.push_str("\" style=\"");
        attr(out, &style);
        out.push('"');
    }
    if matches!(n.kind, NodeKind::Connector { .. }) {
        out.push_str(" edge=\"1\"");
        if let NodeKind::Connector { start, end, .. } = &n.kind {
            if let Some(b) = start {
                let _ = write!(out, " source=\"{}\"", node_cell(b.node));
            }
            if let Some(b) = end {
                let _ = write!(out, " target=\"{}\"", node_cell(b.node));
            }
        }
    } else {
        out.push_str(" vertex=\"1\"");
        if n.is_group() {
            out.push_str(" connectable=\"0\"");
        }
    }
    out.push_str(" parent=\"");
    out.push_str(&parent_attr);
    out.push_str("\">\n");
    emit_geometry(out, doc, n, parent);
    out.push_str("        </mxCell>\n");
    if has_data {
        out.push_str("        </UserObject>\n");
    }

    if let NodeKind::Connector { .. } = &n.kind {
        for (i, extra) in n.labels.iter().enumerate() {
            out.push_str("        <mxCell id=\"");
            out.push_str(&format!("e{}-label{}", id.0, i + 1));
            out.push_str("\" value=\"");
            attr(out, &extra.text);
            out.push_str("\" style=\"edgeLabel;html=1;align=center;verticalAlign=middle;resizable=0;points=[];\" vertex=\"1\" connectable=\"0\" parent=\"");
            out.push_str(&node_cell(id));
            out.push_str("\">\n          <mxGeometry x=\"");
            out.push_str(&fmt_num(extra.pos.map(|p| p * 2.0 - 1.0).unwrap_or(0.0)));
            out.push_str("\" relative=\"1\" as=\"geometry\">\n");
            if let Some(off) = extra.offset {
                let _ = writeln!(
                    out,
                    "            <mxPoint x=\"{}\" y=\"{}\" as=\"offset\" />",
                    fmt_num(off.x),
                    fmt_num(off.y)
                );
            }
            out.push_str("          </mxGeometry>\n        </mxCell>\n");
        }
    }
    for child in &n.children {
        emit_node(out, doc, *child, Some(id));
    }
}

fn emit_geometry(out: &mut String, doc: &Document, n: &Node, parent: Option<NodeId>) {
    if let NodeKind::Connector { points, .. } = &n.kind {
        let origin = doc.parent_origin(parent);
        let world: Vec<V2> = if let Some(wr) = doc.world_rect(n.id) {
            points.iter().map(|p| wr.min() + *p).collect()
        } else {
            points.iter().map(|p| origin + *p).collect()
        };
        out.push_str("          <mxGeometry relative=\"1\"");
        if let Some(pos) = n.label_pos {
            let _ = write!(out, " x=\"{}\"", fmt_num(pos * 2.0 - 1.0));
        }
        out.push_str(" as=\"geometry\">\n");
        let bound_start = matches!(&n.kind, NodeKind::Connector { start: Some(_), .. });
        let bound_end = matches!(&n.kind, NodeKind::Connector { end: Some(_), .. });
        if !bound_start && let Some(p) = world.first() {
            let _ = writeln!(
                out,
                "            <mxPoint x=\"{}\" y=\"{}\" as=\"sourcePoint\" />",
                fmt_num(p.x - origin.x),
                fmt_num(p.y - origin.y)
            );
        }
        let inner: &[V2] = if world.len() > 2 {
            &world[1..world.len() - 1]
        } else {
            &[]
        };
        if !inner.is_empty() {
            out.push_str("            <Array as=\"points\">\n");
            for p in inner {
                let _ = writeln!(
                    out,
                    "              <mxPoint x=\"{}\" y=\"{}\" />",
                    fmt_num(p.x - origin.x),
                    fmt_num(p.y - origin.y)
                );
            }
            out.push_str("            </Array>\n");
        }
        if !bound_end && let Some(p) = world.last() {
            let _ = writeln!(
                out,
                "            <mxPoint x=\"{}\" y=\"{}\" as=\"targetPoint\" />",
                fmt_num(p.x - origin.x),
                fmt_num(p.y - origin.y)
            );
        }
        if let Some(off) = n.label_offset {
            let _ = writeln!(
                out,
                "            <mxPoint x=\"{}\" y=\"{}\" as=\"offset\" />",
                fmt_num(off.x),
                fmt_num(off.y)
            );
        }
        out.push_str("          </mxGeometry>\n");
        return;
    }
    let _ = writeln!(
        out,
        "          <mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\" />",
        fmt_num(n.rect.x),
        fmt_num(n.rect.y),
        fmt_num(n.rect.w),
        fmt_num(n.rect.h)
    );
}

fn shape_style(n: &Node, _doc: &Document) -> String {
    let mut style = String::new();
    match &n.kind {
        NodeKind::Group => style.push_str("group;"),
        NodeKind::Frame { .. } => style.push_str("swimlane;startSize=26;html=1;"),
        NodeKind::Text { .. } => style.push_str("text;html=1;"),
        NodeKind::Sticky { .. } => style.push_str("shape=note;whiteSpace=wrap;html=1;"),
        NodeKind::Ellipse => style.push_str("ellipse;whiteSpace=wrap;html=1;"),
        NodeKind::Polygon { sides: 4, .. } => style.push_str("rhombus;whiteSpace=wrap;html=1;"),
        NodeKind::Polygon { sides: 3, .. } => style.push_str("triangle;whiteSpace=wrap;html=1;"),
        NodeKind::Polygon { sides: 6, .. } => style.push_str("hexagon;whiteSpace=wrap;html=1;"),
        NodeKind::Polygon { .. } => style.push_str("rhombus;whiteSpace=wrap;html=1;"),
        NodeKind::Star { .. } => style.push_str("shape=star;whiteSpace=wrap;html=1;"),
        NodeKind::Path { .. } => {
            if let Some(def) = crate::registry::all().iter().find(|d| d.name == n.name) {
                style.push_str(drawio_shape_for_registry(def.id).unwrap_or("rounded=0;"));
            } else {
                style.push_str("rounded=0;whiteSpace=wrap;html=1;");
            }
        }
        NodeKind::Line { .. } => style.push_str("line;html=1;"),
        NodeKind::Connector { .. } => {
            style.push_str("html=1;");
        }
        NodeKind::Rect | NodeKind::Stroke { .. } => {
            style.push_str("rounded=0;whiteSpace=wrap;html=1;")
        }
    }
    if n.style.corner_radius > 0.0
        && matches!(
            n.kind,
            NodeKind::Rect
                | NodeKind::Frame { .. }
                | NodeKind::Path { .. }
                | NodeKind::Ellipse
                | NodeKind::Polygon { .. }
        )
    {
        let min = n.rect.w.min(n.rect.h).max(1.0);
        let pct = (n.style.corner_radius / min * 100.0).clamp(0.0, 100.0);
        style.push_str("rounded=1;");
        let _ = write!(style, "arcSize={};", fmt_num(pct));
    }
    if let Some(img) = n.style.fills.iter().find_map(|p| match p {
        Paint::Image { asset, .. } => Some(asset),
        _ => None,
    }) {
        if let Some(a) = crate::assets::get(img) {
            let _ = write!(
                style,
                "shape=image;image=data:{};base64,{};",
                a.mime(),
                crate::assets::base64(&a.bytes)
            );
        }
    } else if matches!(n.kind, NodeKind::Text { .. }) {
        let (light, dark) = n.style.fill_pair().unwrap_or((Color::BLACK, None));
        let _ = write!(
            style,
            "fillColor=none;strokeColor=none;fontColor={};",
            color_value(light, dark)
        );
    } else if !n.is_group() {
        if !n.style.fill_enabled || n.style.fills.is_empty() {
            style.push_str("fillColor=none;");
        } else if let Some(Paint::Linear { stops, .. }) = n.style.fills.last() {
            if stops.len() >= 2 {
                let _ = write!(
                    style,
                    "fillColor={};gradientColor={};",
                    color_value(stops[0].color, stops[0].dark),
                    color_value(stops[1].color, stops[1].dark)
                );
            }
        } else if let Some((light, dark)) = n.style.fill_pair() {
            let _ = write!(style, "fillColor={};", color_value(light, dark));
        }
    }
    if matches!(n.kind, NodeKind::Connector { .. }) || !n.is_group() {
        if n.style.stroke_enabled
            && let Some(c) = n.style.stroke
        {
            let _ = write!(
                style,
                "strokeColor={};strokeWidth={};",
                color_value(c, n.style.stroke_dark),
                fmt_num(n.style.stroke_width)
            );
        } else if !matches!(n.kind, NodeKind::Text { .. }) {
            style.push_str("strokeColor=none;");
        }
    }
    if !n.style.dash.is_empty() {
        style.push_str("dashed=1;dashPattern=");
        for (i, d) in n.style.dash.iter().enumerate() {
            if i > 0 {
                style.push(' ');
            }
            let _ = write!(style, "{}", fmt_num(*d));
        }
        style.push(';');
    }
    if let NodeKind::Connector {
        route,
        start_head,
        end_head,
        jump,
        jump_size,
        start,
        end,
        ..
    } = &n.kind
    {
        if *route == Route::Elbow {
            style.push_str("edgeStyle=orthogonalEdgeStyle;");
        } else if *route == Route::Curved {
            style.push_str("curved=1;");
        }
        let _ = write!(
            style,
            "startArrow={};endArrow={};",
            head_name(*start_head),
            head_name(*end_head)
        );
        if n.start_head_size != DEFAULT_HEAD_SIZE {
            let _ = write!(style, "startSize={};", fmt_num(n.start_head_size));
        }
        if n.end_head_size != DEFAULT_HEAD_SIZE {
            let _ = write!(style, "endSize={};", fmt_num(n.end_head_size));
        }
        if n.start_head_offset != 0.0 {
            let _ = write!(
                style,
                "sourcePerimeterSpacing={};",
                fmt_num(n.start_head_offset)
            );
        }
        if n.end_head_offset != 0.0 {
            let _ = write!(
                style,
                "targetPerimeterSpacing={};",
                fmt_num(n.end_head_offset)
            );
        }
        if *jump != LineJump::None {
            let _ = write!(
                style,
                "jumpStyle={};jumpSize={};",
                match jump {
                    LineJump::None => "none",
                    LineJump::Arc => "arc",
                    LineJump::Gap => "gap",
                    LineJump::Sharp => "sharp",
                    LineJump::Line => "line",
                },
                fmt_num(*jump_size)
            );
        }
        if n.flow.enabled {
            let _ = write!(
                style,
                "flowAnimation=1;flowAnimationDuration={};flowAnimationTimingFunction={};flowAnimationDirection={};",
                fmt_num(n.flow.duration_ms),
                n.flow.timing.style(),
                n.flow.direction.style()
            );
        }
        if let Some(b) = start
            && !b.floating
        {
            let _ = write!(
                style,
                "exitX={};exitY={};exitPerimeter={};",
                fmt_num(b.fixed.x),
                fmt_num(b.fixed.y),
                if b.inside { 0 } else { 1 }
            );
        }
        if let Some(b) = end
            && !b.floating
        {
            let _ = write!(
                style,
                "entryX={};entryY={};entryPerimeter={};",
                fmt_num(b.fixed.x),
                fmt_num(b.fixed.y),
                if b.inside { 0 } else { 1 }
            );
        }
    }
    style
}

fn style_options(out: &mut String, style: &mut String, n: &Node) {
    let _ = out;
    if n.supports_label() || n.is_connector() {
        if let Some(c) = n.label_color {
            let _ = write!(style, "fontColor={};", color_value(c, n.label_color_dark));
        }
        if let Some(size) = n.label_size {
            let _ = write!(style, "fontSize={};", fmt_num(size));
        }
        if n.label_valign.is_some() {
            let _ = write!(
                style,
                "verticalAlign={};",
                match n.label_valign {
                    Some(VAlign::Top) => "top",
                    Some(VAlign::Bottom) => "bottom",
                    _ => "middle",
                }
            );
        }
        if let Some(align) = n.text_style.align {
            let _ = write!(
                style,
                "align={};",
                match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }
            );
        }
        style.push_str(if n.label_wrap {
            "whiteSpace=wrap;"
        } else {
            "whiteSpace=nowrap;"
        });
        if n.label_opacity < 1.0 {
            let _ = write!(
                style,
                "textOpacity={};",
                (n.label_opacity * 100.0).round() as i64
            );
        }
        if let Some(c) = n.text_bg {
            let _ = write!(
                style,
                "labelBackgroundColor={};",
                color_value(c, n.text_bg_dark)
            );
        }
        if let Some(c) = n.text_border {
            let _ = write!(
                style,
                "labelBorderColor={};",
                color_value(c, n.text_border_dark)
            );
        }
        if n.text_shadow {
            style.push_str("textShadow=1;");
        }
    }
    if let NodeKind::Text {
        font_size, align, ..
    } = &n.kind
    {
        let _ = write!(style, "fontSize={};", fmt_num(*font_size));
        let _ = write!(
            style,
            "align={};",
            match align {
                TextAlign::Left => "left",
                TextAlign::Center => "center",
                TextAlign::Right => "right",
            }
        );
    }
    if n.text_style.bold || n.text_style.italic {
        let bits = (n.text_style.bold as i32) | ((n.text_style.italic as i32) << 1);
        let _ = write!(style, "fontStyle={bits};");
    }
    if let Some(family) = &n.text_style.family {
        let _ = write!(style, "fontFamily={family};");
    }
    if n.text_style.curve.is_some() {
        style.push_str("textPath=1;");
    }
}

fn head_name(h: Head) -> &'static str {
    match h {
        Head::None => "none",
        Head::Arrow => "classic",
        Head::Triangle => "block",
        Head::Dot => "oval",
        Head::Bar => "bar",
        Head::Diamond => "diamond",
    }
}

fn drawio_shape_for_registry(id: &str) -> Option<&'static str> {
    Some(match id {
        "process" | "entity" => "rounded=0;whiteSpace=wrap;html=1;",
        "decision" | "relationship" => "rhombus;whiteSpace=wrap;html=1;",
        "terminator" => "rounded=1;arcSize=50;whiteSpace=wrap;html=1;",
        "attribute" | "derived-attribute" | "key-attribute" | "on-page-connector" => {
            "ellipse;whiteSpace=wrap;html=1;"
        }
        "database" => "shape=cylinder3;whiteSpace=wrap;html=1;",
        "document" => "shape=document;whiteSpace=wrap;html=1;",
        "data" => "shape=parallelogram;whiteSpace=wrap;html=1;",
        "manual-input" => "shape=manualInput;whiteSpace=wrap;html=1;",
        "manual-operation" => "shape=trapezoid;whiteSpace=wrap;html=1;",
        "preparation" => "shape=hexagon;whiteSpace=wrap;html=1;",
        "delay" => "shape=delay;whiteSpace=wrap;html=1;",
        "off-page" => "shape=offPageConnector;whiteSpace=wrap;html=1;",
        "queue" => "shape=queue;whiteSpace=wrap;html=1;",
        "note" => "shape=note;whiteSpace=wrap;html=1;",
        "package" => "shape=package;whiteSpace=wrap;html=1;",
        "cloud" => "shape=cloud;whiteSpace=wrap;html=1;",
        "host" => "shape=host;whiteSpace=wrap;html=1;",
        "server" => "shape=server;whiteSpace=wrap;html=1;",
        "firewall" => "shape=firewall;whiteSpace=wrap;html=1;",
        "actor" => "shape=actor;whiteSpace=wrap;html=1;",
        "class" => "shape=class;whiteSpace=wrap;html=1;",
        "weak-entity" => "shape=weakEntity;whiteSpace=wrap;html=1;",
        "door" => "shape=door;whiteSpace=wrap;html=1;",
        "window" => "shape=window;whiteSpace=wrap;html=1;",
        "stairs" => "shape=stairs;whiteSpace=wrap;html=1;",
        "person" => "shape=person;whiteSpace=wrap;html=1;",
        _ => return None,
    })
}

fn node_cell(id: NodeId) -> String {
    format!("n{}", id.0)
}

fn layer_cell(id: LayerId) -> String {
    format!("l{}", id.0)
}

fn attr(out: &mut String, value: &str) {
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            '\t' => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
}

fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

// --------------------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeKind};

    const TWO_PAGES: &str = include_str!("../../tests/fixtures/drawio/two-pages.drawio");

    #[test]
    fn imports_two_pages_in_order() {
        let (pages, report) = import_str(TWO_PAGES).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].name, "Flow");
        assert_eq!(pages[1].name, "Detail");
        assert_eq!(report.pages, 2);
        assert!(report.shapes >= 6);
        assert!(report.connectors >= 1);
        assert!(report.dropped.is_empty());
    }

    #[test]
    fn imports_layers_groups_geometry_labels_and_styles() {
        let (pages, report) = import_str(TWO_PAGES).unwrap();
        let doc = &pages[0].doc;
        assert_eq!(doc.layers.len(), 2);
        assert_eq!(doc.layers[1].name, "Notes");
        let child = doc
            .nodes
            .values()
            .find(|n| n.label == "Child")
            .expect("group child");
        let root_group = doc
            .get(child.parent.expect("group child has a parent"))
            .expect("group imported");
        assert!(root_group.is_group());
        // Native groups normalize their rect to the children's union, so the
        // child sits at the group's origin and the group covers its world box.
        assert_eq!(child.rect, Rect::new(0.0, 0.0, 80.0, 40.0));
        assert_eq!(
            doc.world_rect(child.id),
            Some(Rect::new(60.0, 80.0, 80.0, 40.0))
        );
        assert_eq!(root_group.rect, Rect::new(60.0, 80.0, 80.0, 40.0));
        let styled = doc
            .nodes
            .values()
            .find(|n| n.label == "Styled")
            .expect("styled cell");
        assert_eq!(styled.style.fill(), Some(Color::rgb(0xda, 0xe8, 0xfc)));
        assert_eq!(styled.style.stroke, Some(Color::rgb(0x6c, 0x8e, 0xbf)));
        assert!(styled.style.corner_radius > 0.0);
        assert!((styled.style.rotation - 15.0).abs() < 1e-9);
        assert!((styled.style.opacity - 0.5).abs() < 1e-9);
        let edge = doc
            .nodes
            .values()
            .find(|n| n.is_connector())
            .expect("edge imported");
        let NodeKind::Connector {
            start, end, route, ..
        } = &edge.kind
        else {
            unreachable!()
        };
        assert!(start.is_some());
        assert!(end.is_some());
        assert_eq!(*route, Route::Elbow);
        assert_eq!(edge.label, "Yes");
        assert!(!edge.labels.is_empty());
        assert!(
            report
                .approximated
                .iter()
                .any(|d| d.reason.contains("waypoints"))
        );
    }

    #[test]
    fn imports_and_exports_fixed_and_floating_endpoints() {
        let xml = r#"<mxGraphModel><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="a" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="b" vertex="1" parent="1"><mxGeometry x="300" y="0" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="fixed" value="Fixed" edge="1" parent="1" source="a" target="b"
              style="exitX=0;exitY=0;exitPerimeter=1;entryX=1;entryY=1;entryPerimeter=1;">
              <mxGeometry relative="1" as="geometry"/>
            </mxCell>
            <mxCell id="floating" value="Floating" edge="1" parent="1" source="a" target="b" style="html=1;">
              <mxGeometry relative="1" as="geometry"/>
            </mxCell>
        </root></mxGraphModel>"#;
        let (pages, _) = import_str(xml).unwrap();
        let doc = &pages[0].doc;
        let fixed_edge = doc
            .nodes
            .values()
            .find(|n| n.label == "Fixed")
            .expect("fixed edge");
        let floating_edge = doc
            .nodes
            .values()
            .find(|n| n.label == "Floating")
            .expect("floating edge");
        let NodeKind::Connector { start, end, .. } = &fixed_edge.kind else {
            unreachable!()
        };
        let start = start.unwrap();
        let end = end.unwrap();
        assert!(!start.floating);
        assert_eq!(start.fixed, v2(0.0, 0.0));
        assert!(!end.floating);
        assert_eq!(end.fixed, v2(1.0, 1.0));
        let NodeKind::Connector { start, end, .. } = &floating_edge.kind else {
            unreachable!()
        };
        assert!(start.unwrap().floating);
        assert!(end.unwrap().floating);

        let exported = export(&[("Page", doc)]);
        assert_eq!(exported.matches("exitX=").count(), 1);
        assert_eq!(exported.matches("entryX=").count(), 1);
        assert!(exported.contains("exitX=0;exitY=0;exitPerimeter=1;"));
        let (again, _) = import_str(&exported).unwrap();
        let restored_fixed = again[0]
            .doc
            .nodes
            .values()
            .find(|n| n.label == "Fixed")
            .expect("fixed edge after export");
        let restored_floating = again[0]
            .doc
            .nodes
            .values()
            .find(|n| n.label == "Floating")
            .expect("floating edge after export");
        let NodeKind::Connector { start, end, .. } = &restored_fixed.kind else {
            unreachable!()
        };
        assert!(!start.unwrap().floating);
        assert!(!end.unwrap().floating);
        let NodeKind::Connector { start, end, .. } = &restored_floating.kind else {
            unreachable!()
        };
        assert!(start.unwrap().floating);
        assert!(end.unwrap().floating);
    }

    #[test]
    fn imports_and_exports_head_size_and_offset() {
        let xml = r#"<mxGraphModel><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="a" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="b" vertex="1" parent="1"><mxGeometry x="300" y="0" width="100" height="60" as="geometry"/></mxCell>
            <mxCell id="sized" value="Sized" edge="1" parent="1" source="a" target="b"
              style="startArrow=classic;endArrow=block;startSize=10;endSize=3;sourcePerimeterSpacing=8;targetPerimeterSpacing=-4;">
              <mxGeometry relative="1" as="geometry"/>
            </mxCell>
        </root></mxGraphModel>"#;
        let (pages, _) = import_str(xml).unwrap();
        let doc = &pages[0].doc;
        let edge = doc
            .nodes
            .values()
            .find(|n| n.label == "Sized")
            .expect("sized edge");
        assert_eq!(edge.start_head_size, 10.0);
        assert_eq!(edge.end_head_size, 3.0);
        assert_eq!(edge.start_head_offset, 8.0);
        assert_eq!(edge.end_head_offset, -4.0);

        let exported = export(&[("Page", doc)]);
        assert!(exported.contains("startSize=10;"), "{exported}");
        assert!(exported.contains("endSize=3;"), "{exported}");
        assert!(exported.contains("sourcePerimeterSpacing=8;"), "{exported}");
        assert!(
            exported.contains("targetPerimeterSpacing=-4;"),
            "{exported}"
        );

        let (again, _) = import_str(&exported).unwrap();
        let restored = again[0]
            .doc
            .nodes
            .values()
            .find(|n| n.label == "Sized")
            .expect("sized edge after export");
        assert_eq!(restored.start_head_size, 10.0);
        assert_eq!(restored.end_head_size, 3.0);
        assert_eq!(restored.start_head_offset, 8.0);
        assert_eq!(restored.end_head_offset, -4.0);
    }

    #[test]
    fn reports_approximations_instead_of_dropping_unknown_shapes() {
        let xml = r#"<mxGraphModel><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="2" value="Mystery" style="shape=made.up.thing;fillColor=#ff0000;" vertex="1" parent="1">
              <mxGeometry x="10" y="20" width="80" height="40" as="geometry"/>
            </mxCell>
        </root></mxGraphModel>"#;
        let (pages, report) = import_str(xml).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(report.shapes, 1);
        assert!(report.dropped.is_empty());
        assert!(
            report
                .approximated
                .iter()
                .any(|d| d.reason.contains("unknown draw.io shape")),
            "{report:?}"
        );
        let n = pages[0].doc.nodes.values().next().unwrap();
        assert!(matches!(n.kind, NodeKind::Rect));
        assert_eq!(n.label, "Mystery");
    }

    #[test]
    fn geometry_round_trips_within_tolerance() {
        let (pages, _) = import_str(TWO_PAGES).unwrap();
        let refs: Vec<(&str, &Document)> =
            pages.iter().map(|p| (p.name.as_str(), &p.doc)).collect();
        let xml = export(&refs);
        let (again, report) = import_str(&xml).unwrap();
        assert_eq!(again.len(), 2);
        assert_eq!(again[0].name, "Flow");
        let before = &pages[0].doc;
        let after = &again[0].doc;
        assert_eq!(before.layers.len(), after.layers.len());
        for (id, n) in &before.nodes {
            let Some(m) = after.nodes.get(id) else {
                panic!("missing node {id}");
            };
            assert!(
                (n.rect.x - m.rect.x).abs() < 0.01
                    && (n.rect.y - m.rect.y).abs() < 0.01
                    && (n.rect.w - m.rect.w).abs() < 0.01
                    && (n.rect.h - m.rect.h).abs() < 0.01,
                "{id}: {:?} vs {:?}",
                n.rect,
                m.rect
            );
            assert_eq!(n.label, m.label);
            assert_eq!(n.kind.type_name(), m.kind.type_name());
        }
        assert!(report.dropped.is_empty());
    }

    #[test]
    fn decodes_compressed_payloads() {
        let inner = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="A &amp; B" vertex="1" parent="1"><mxGeometry x="1" y="2" width="30" height="40" as="geometry"/></mxCell></root></mxGraphModel>"#;
        let encoded = percent_encode(inner);
        let compressed = miniz_oxide::deflate::compress_to_vec(encoded.as_bytes(), 6);
        let b64 = crate::assets::base64(&compressed);
        let xml = format!(r#"<mxfile><diagram id="d" name="Packed">{b64}</diagram></mxfile>"#);
        let (pages, _) = import_str(&xml).unwrap();
        assert_eq!(pages[0].name, "Packed");
        let n = pages[0].doc.nodes.values().next().unwrap();
        assert_eq!(n.label, "A & B");
    }

    #[test]
    fn refuses_encrypted_payloads() {
        let data = crate::assets::base64(b"Salted__not-a-real-drawio-stream");
        let xml = format!(r#"<mxfile><diagram id="d" name="Locked">{data}</diagram></mxfile>"#);
        let err = import_str(&xml).unwrap_err();
        assert!(err.to_lowercase().contains("encrypted"), "{err}");
        assert!(!err.contains('\u{2014}') && !err.contains('\u{2013}'));
    }

    #[test]
    fn exports_escapes_values_and_extra_labels() {
        let mut doc = Document::default();
        let a = doc.alloc_id();
        let mut n = Node::new(a, NodeKind::Rect, Rect::new(1.0, 2.0, 30.0, 40.0));
        n.label = "a & b <c>".into();
        doc.nodes.insert(a, n.clone());
        doc.root.push(a);
        let xml = export(&[("Page & One", &doc)]);
        assert!(xml.contains("Page &amp; One"));
        assert!(xml.contains("a &amp; b &lt;c&gt;"));
        assert!(xml.contains("<mxGraphModel"));
    }

    #[test]
    fn imports_and_exports_page_and_element_shadow_flags() {
        let xml = r#"<mxGraphModel shadow="1"><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="on" value="On" vertex="1" parent="1" style="shadow=1;">
              <mxGeometry x="0" y="0" width="40" height="30" as="geometry"/>
            </mxCell>
            <mxCell id="off" value="Off" vertex="1" parent="1" style="shadow=0;">
              <mxGeometry x="60" y="0" width="40" height="30" as="geometry"/>
            </mxCell>
            <mxCell id="inherit" value="Inherit" vertex="1" parent="1" style="html=1;">
              <mxGeometry x="120" y="0" width="40" height="30" as="geometry"/>
            </mxCell>
        </root></mxGraphModel>"#;
        let (pages, _) = import_str(xml).unwrap();
        let doc = &pages[0].doc;
        assert!(doc.settings.shadow);
        assert_eq!(find(doc, "On").unwrap().style.shadow, Some(true));
        assert_eq!(find(doc, "Off").unwrap().style.shadow, Some(false));
        assert_eq!(find(doc, "Inherit").unwrap().style.shadow, None);

        let exported = export(&[("Page", doc)]);
        assert!(exported.contains("shadow=\"1\""));
        assert!(exported.contains("shadow=1;"));
        assert!(exported.contains("shadow=0;"));
        let (again, _) = import_str(&exported).unwrap();
        let restored = &again[0].doc;
        assert!(restored.settings.shadow);
        assert_eq!(find(restored, "On").unwrap().style.shadow, Some(true));
        assert_eq!(find(restored, "Off").unwrap().style.shadow, Some(false));
        assert_eq!(find(restored, "Inherit").unwrap().style.shadow, None);
    }

    #[test]
    fn imports_and_exports_glass_flag() {
        let xml = r#"<mxGraphModel><root>
            <mxCell id="0"/><mxCell id="1" parent="0"/>
            <mxCell id="on" value="On" vertex="1" parent="1" style="glass=1;">
              <mxGeometry x="0" y="0" width="40" height="30" as="geometry"/>
            </mxCell>
            <mxCell id="off" value="Off" vertex="1" parent="1" style="glass=0;">
              <mxGeometry x="60" y="0" width="40" height="30" as="geometry"/>
            </mxCell>
        </root></mxGraphModel>"#;
        let (pages, _) = import_str(xml).unwrap();
        let doc = &pages[0].doc;
        assert!(find(doc, "On").unwrap().style.glass);
        assert!(!find(doc, "Off").unwrap().style.glass);

        let exported = export(&[("Page", doc)]);
        assert!(exported.contains("glass=1;"));
        assert!(!exported.contains("glass=0;"), "false is the default");
        let (again, _) = import_str(&exported).unwrap();
        let restored = &again[0].doc;
        assert!(find(restored, "On").unwrap().style.glass);
        assert!(!find(restored, "Off").unwrap().style.glass);
    }

    fn percent_encode(s: &str) -> String {
        let mut out = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
                )
            {
                out.push(b as char);
            } else {
                let _ = write!(out, "%{b:02X}");
            }
        }
        out
    }

    #[allow(dead_code)]
    fn find<'a>(doc: &'a Document, label: &str) -> Option<&'a Node> {
        doc.nodes.values().find(|n| n.label == label)
    }
}
