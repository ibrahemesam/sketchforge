//! The document arena: nodes keyed by id plus ordered child lists.

use super::layer::{Layer, LayerId};
use super::node::{Node, NodeId, Style};
use super::paint::{Paint, Stop, TextSpan};
use super::settings::{ColorMode, DiagramSettings};
use crate::geom::{Rect, V2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Values a page learns from the project around it: its position in the page
/// list, the page count and the project/file name. They feed predefined
/// placeholders (`%pagenumber%`, `%page%`, `%filename%`) and are not saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageContext {
    pub page_number: usize,
    pub page_count: usize,
    pub page_name: String,
    pub file_name: String,
}

impl Default for PageContext {
    fn default() -> Self {
        PageContext {
            page_number: 1,
            page_count: 1,
            page_name: "Page 1".into(),
            file_name: "Untitled".into(),
        }
    }
}

impl PageContext {
    /// A context for a page, with a fallback name when the page is unnamed.
    pub fn new(page_number: usize, page_count: usize, page_name: &str, file_name: &str) -> Self {
        PageContext {
            page_number: page_number.max(1),
            page_count: page_count.max(1),
            page_name: if page_name.trim().is_empty() {
                "Page 1".into()
            } else {
                page_name.to_string()
            },
            file_name: if file_name.trim().is_empty() {
                "Untitled".into()
            } else {
                file_name.to_string()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub name: String,
    pub nodes: HashMap<NodeId, Node>,
    /// Top-level nodes in paint order, bottom to top. With layers on the page,
    /// the paint order buckets this list by layer first (see `layered_root`).
    pub root: Vec<NodeId>,
    pub next_id: u64,
    /// Page-scoped layer records, bottom to top. `layers[0]` is the default
    /// Background layer. The single root list stays the z-order storage;
    /// layer order only re-buckets it (ADR-009).
    #[serde(default = "default_layers")]
    pub layers: Vec<Layer>,
    /// Next unused layer id across the page's history (never reused).
    #[serde(default = "default_next_layer")]
    pub next_layer: u64,
    /// Page-scoped editor and diagram settings (draw.io's Diagram tab).
    #[serde(default)]
    pub settings: DiagramSettings,
    /// Project position and file name for predefined placeholders. Refreshed
    /// by the editor and never written to disk (each page file only stores
    /// the document).
    #[serde(skip)]
    pub context: PageContext,
}

/// Old projects and `Document::default()` start with the one Background layer.
fn default_layers() -> Vec<Layer> {
    vec![Layer::background()]
}

fn default_next_layer() -> u64 {
    2
}

fn resolved_stop(s: &Stop, mode: ColorMode) -> Stop {
    Stop {
        pos: s.pos,
        color: mode.resolve(s.color, s.dark),
        dark: None,
    }
}

fn resolved_paint(p: &Paint, mode: ColorMode) -> Paint {
    match p {
        Paint::Solid { color, dark } => Paint::Solid {
            color: mode.resolve(*color, *dark),
            dark: None,
        },
        Paint::Linear { stops, from, to } => Paint::Linear {
            stops: stops.iter().map(|s| resolved_stop(s, mode)).collect(),
            from: *from,
            to: *to,
        },
        Paint::Radial {
            stops,
            center,
            radius,
        } => Paint::Radial {
            stops: stops.iter().map(|s| resolved_stop(s, mode)).collect(),
            center: *center,
            radius: *radius,
        },
        Paint::Image { .. } => p.clone(),
    }
}

fn resolved_style(style: &mut Style, mode: ColorMode) {
    for p in &mut style.fills {
        *p = resolved_paint(p, mode);
    }
    if let Some(p) = &mut style.saved_gradient {
        *p = resolved_paint(p, mode);
    }
    if let Some(c) = style.stroke {
        style.stroke = Some(mode.resolve(c, style.stroke_dark));
    }
    style.stroke_dark = None;
}

fn resolved_span(span: &mut TextSpan, mode: ColorMode) {
    if let Some(c) = span.color {
        span.color = Some(mode.resolve(c, span.color_dark));
    }
    span.color_dark = None;
}

impl Default for Document {
    fn default() -> Self {
        Document {
            name: "Untitled".into(),
            nodes: HashMap::new(),
            root: Vec::new(),
            next_id: 1,
            layers: default_layers(),
            next_layer: default_next_layer(),
            settings: DiagramSettings::default(),
            context: PageContext::default(),
        }
    }
}

impl Document {
    /// A render/export copy with dark colors resolved into the light fields.
    /// The stored document is never modified, so switching appearance cannot
    /// rewrite the authored palette.
    pub fn with_appearance(&self, mode: ColorMode) -> Document {
        let mut out = self.clone();
        out.settings.background_color = mode.resolve(
            self.settings.background_color,
            self.settings.background_color_dark,
        );
        out.settings.background_color_dark = None;
        if let Some(d) = &mut out.settings.shape_default {
            resolved_style(&mut d.style, mode);
        }
        if let Some(d) = &mut out.settings.connector_default {
            resolved_style(&mut d.style, mode);
        }
        let auto_fill = self.settings.auto_fill_with(mode);
        for node in out.nodes.values_mut() {
            if node.style.fill_auto
                && let Some(c) = auto_fill
            {
                node.style.fills = vec![Paint::solid(c)];
                node.style.fill_auto = false;
            }
            resolved_style(&mut node.style, mode);
            if let Some(c) = node.label_color {
                node.label_color = Some(mode.resolve(c, node.label_color_dark));
            }
            node.label_color_dark = None;
            if let Some(c) = node.text_bg {
                node.text_bg = Some(mode.resolve(c, node.text_bg_dark));
            }
            node.text_bg_dark = None;
            if let Some(c) = node.text_border {
                node.text_border = Some(mode.resolve(c, node.text_border_dark));
            }
            node.text_border_dark = None;
            for span in &mut node.text_style.spans {
                resolved_span(span, mode);
            }
        }
        out
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn children(&self, parent: Option<NodeId>) -> &[NodeId] {
        match parent {
            None => &self.root,
            Some(p) => self
                .nodes
                .get(&p)
                .map(|n| n.children.as_slice())
                .unwrap_or(&[]),
        }
    }

    pub fn children_mut(&mut self, parent: Option<NodeId>) -> Option<&mut Vec<NodeId>> {
        match parent {
            None => Some(&mut self.root),
            Some(p) => self.nodes.get_mut(&p).map(|n| &mut n.children),
        }
    }

    /// World-space position of a node's local coordinate origin's parent space.
    pub fn parent_origin(&self, parent: Option<NodeId>) -> V2 {
        let mut o = V2::ZERO;
        let mut cur = parent;
        while let Some(id) = cur {
            let Some(n) = self.nodes.get(&id) else { break };
            o = o + n.rect.min();
            cur = n.parent;
        }
        o
    }

    pub fn world_rect(&self, id: NodeId) -> Option<Rect> {
        let n = self.nodes.get(&id)?;
        Some(n.rect.translate(self.parent_origin(n.parent)))
    }

    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.nodes.get(&id).and_then(|n| n.parent);
        while let Some(p) = cur {
            out.push(p);
            cur = self.nodes.get(&p).and_then(|n| n.parent);
        }
        out
    }

    pub fn is_ancestor(&self, ancestor: NodeId, of: NodeId) -> bool {
        self.ancestors(of).contains(&ancestor)
    }

    /// The node and all its descendants, parent first.
    pub fn subtree(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            if let Some(n) = self.nodes.get(&out[i]) {
                out.extend(n.children.iter().copied());
            }
            i += 1;
        }
        out
    }

    /// The layer id in charge of `id`: the nearest ancestor chain member's own
    /// layer, or the Background layer when nothing on the chain names one.
    pub fn effective_layer(&self, id: NodeId) -> LayerId {
        let mut cur = Some(id);
        while let Some(c) = cur {
            match self.nodes.get(&c) {
                Some(n) => {
                    if let Some(l) = n.layer {
                        return l;
                    }
                    cur = n.parent;
                }
                None => break,
            }
        }
        LayerId(1)
    }

    /// The layer record `id` paints on. Old files can name a layer that no
    /// longer exists (clamped edits prevent this), so unknown ids fall back
    /// to Background rather than dropping the node.
    pub fn layer_of(&self, id: NodeId) -> &Layer {
        let eff = self.effective_layer(id);
        self.layers
            .iter()
            .find(|l| l.id == eff)
            .unwrap_or_else(|| self.layers.first().expect("page always has a layer"))
    }

    pub fn layer_by_id(&self, id: LayerId) -> Option<&Layer> {
        self.layers.iter().find(|l| l.id == id)
    }

    /// Top-level ids in paint order: layer order first, then the root list
    /// order within each layer. Content naming a layer the page no longer
    /// has rides at the very top of the Background bucket so nothing ever
    /// disappears after a bad migration.
    pub fn layered_root(&self) -> Vec<NodeId> {
        let mut out = Vec::with_capacity(self.root.len());
        for l in &self.layers {
            out.extend(self.root.iter().copied().filter(|id| {
                self.nodes
                    .get(id)
                    .is_some_and(|n| self.effective_layer(n.id) == l.id)
            }));
        }
        out.extend(self.root.iter().copied().filter(|id| {
            let eff = self
                .nodes
                .get(id)
                .map(|n| self.effective_layer(n.id))
                .unwrap_or(LayerId(1));
            self.layer_by_id(eff).is_none()
        }));
        out
    }

    /// All nodes in paint order (layer buckets first, then depth-first with
    /// parents before children inside each bucket).
    pub fn paint_order(&self) -> Vec<NodeId> {
        fn walk(doc: &Document, ids: &[NodeId], out: &mut Vec<NodeId>) {
            for id in ids {
                out.push(*id);
                if let Some(n) = doc.nodes.get(id) {
                    walk(doc, &n.children, out);
                }
            }
        }
        let mut out = Vec::with_capacity(self.nodes.len());
        walk(self, &self.layered_root(), &mut out);
        out
    }

    /// Whether a node and all its ancestors are visible, composed with the
    /// page layer the node paints on (layer visibility is inherited with the
    /// membership, so hiding one record hides the whole subtree).
    pub fn is_effectively_visible(&self, id: NodeId) -> bool {
        self.layer_of(id).visible && {
            let mut cur = Some(id);
            while let Some(c) = cur {
                match self.nodes.get(&c) {
                    Some(n) if n.visible => cur = n.parent,
                    _ => return false,
                }
            }
            true
        }
    }

    /// Whether a node or any ancestor is locked, including its layer's lock.
    pub fn is_effectively_locked(&self, id: NodeId) -> bool {
        self.layer_of(id).locked || {
            let mut cur = Some(id);
            while let Some(c) = cur {
                match self.nodes.get(&c) {
                    Some(n) if n.locked => return true,
                    Some(n) => cur = n.parent,
                    None => return false,
                }
            }
            false
        }
    }

    /// A fresh layer id for this page.
    pub fn alloc_layer_id(&mut self) -> LayerId {
        let id = LayerId(self.next_layer);
        self.next_layer += 1;
        id
    }

    /// Bounds of all visible content in world space.
    pub fn content_bounds(&self) -> Option<Rect> {
        let rects: Vec<Rect> = self
            .root
            .iter()
            .filter(|id| self.nodes.get(id).is_some_and(|n| n.visible))
            .filter(|id| self.layer_of(**id).visible)
            .filter_map(|id| self.world_rect(*id))
            .collect();
        Rect::union_all(&rects)
    }

    /// A child may only name its parent's effective layer for inheritance to
    /// stay unambiguous; root nodes may name any layer.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        let mut check = |ids: &[NodeId], parent: Option<NodeId>| -> Result<(), String> {
            for id in ids {
                let n = self.nodes.get(id).ok_or(format!("missing node {id}"))?;
                if n.parent != parent {
                    return Err(format!(
                        "node {id} has parent {:?}, expected {:?}",
                        n.parent, parent
                    ));
                }
                if !seen.insert(*id) {
                    return Err(format!("node {id} appears twice"));
                }
            }
            Ok(())
        };
        check(&self.root, None)?;
        for n in self.nodes.values() {
            check(&n.children, Some(n.id))?;
            if n.id.0 >= self.next_id {
                return Err(format!("node {} >= next_id {}", n.id, self.next_id));
            }
        }
        if seen.len() != self.nodes.len() {
            return Err(format!(
                "{} nodes unreachable",
                self.nodes.len() - seen.len()
            ));
        }
        // A subtree lives on exactly one layer: a child may only name its
        // parent's effective layer, so inheritance stays unambiguous.
        for n in self.nodes.values() {
            if let (Some(l), Some(p)) = (n.layer, n.parent)
                && self
                    .nodes
                    .get(&p)
                    .is_some_and(|pn| self.effective_layer(pn.id) != l)
            {
                return Err(format!(
                    "node {} on layer {l} under a parent painting on a different layer",
                    n.id
                ));
            }
        }
        Ok(())
    }
}
