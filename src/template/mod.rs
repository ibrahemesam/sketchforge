//! Native templates and diagram packs (M7, ADR-010).
//!
//! A template file is JSON: a named library of templates, each with a category,
//! optional description and keywords, an optional SVG thumbnail, and one or
//! more pages. A page stores ordinary nodes, the root order, diagram settings
//! and layers, so insertion produces editable native content. Bundled packs
//! are built by pure builders in [`packs`] from registry shapes and the same
//! node kinds a user can draw; no template carries a vendor asset.
//!
//! A node may record the registry shape it came from. If the current build no
//! longer has that shape, insertion reports the name and leaves that node out
//! while the rest of the template still arrives.

use crate::geom::Rect;
use crate::library::LibItem;
use crate::model::settings::DiagramSettings;
use crate::model::{Document, Layer, LayerId, Node, NodeId, NodeKind};
use crate::project::{PageData, new_id};
use crate::registry;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub mod packs;

pub const TEMPLATE_FORMAT: &str = "drawio-rs-template-library";
pub const TEMPLATE_VERSION: u32 = 1;
/// Files written by the app end with this suffix.
pub const TEMPLATE_SUFFIX: &str = ".sketch-templates.json";

/// One node inside a template page, plus where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateNode {
    /// Registry shape id or name, when the node was built from one. Insertion
    /// checks it and reports a missing shape instead of guessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    pub node: Node,
}

/// One page of a template: nodes, root order, settings and layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplatePage {
    pub name: String,
    #[serde(default, skip_serializing_if = "DiagramSettings::is_default")]
    pub settings: DiagramSettings,
    #[serde(default = "default_layers", skip_serializing_if = "is_default_layers")]
    pub layers: Vec<Layer>,
    #[serde(
        default = "default_next_layer",
        skip_serializing_if = "is_default_next"
    )]
    pub next_layer: u64,
    #[serde(default)]
    pub nodes: Vec<TemplateNode>,
    #[serde(default)]
    pub root: Vec<NodeId>,
}

fn default_layers() -> Vec<Layer> {
    vec![Layer::background()]
}

fn is_default_layers(l: &[Layer]) -> bool {
    l.len() == 1 && l[0] == Layer::background()
}

fn default_next_layer() -> u64 {
    2
}

fn is_default_next(n: &u64) -> bool {
    *n <= 2
}

impl TemplatePage {
    pub fn from_doc(name: &str, doc: &Document, shapes: &HashMap<NodeId, String>) -> TemplatePage {
        // Root order first, then each subtree, so the file is deterministic and
        // pages read top to bottom.
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        for root in &doc.root {
            for id in doc.subtree(*root) {
                if seen.insert(id) && doc.nodes.contains_key(&id) {
                    order.push(id);
                }
            }
        }
        for id in doc.nodes.keys() {
            if seen.insert(*id) {
                order.push(*id);
            }
        }
        TemplatePage {
            name: name.into(),
            settings: doc.settings.clone(),
            layers: doc.layers.clone(),
            next_layer: doc.next_layer,
            nodes: order
                .into_iter()
                .filter_map(|id| {
                    doc.nodes.get(&id).cloned().map(|node| TemplateNode {
                        shape: shapes.get(&id).cloned(),
                        node,
                    })
                })
                .collect(),
            root: doc.root.clone(),
        }
    }

    /// Rebuild the page as a native document.
    pub fn to_doc(&self) -> Document {
        let mut doc = Document {
            name: self.name.clone(),
            settings: self.settings.clone(),
            layers: if self.layers.is_empty() {
                default_layers()
            } else {
                self.layers.clone()
            },
            next_layer: self.next_layer.max(2),
            ..Default::default()
        };
        for item in &self.nodes {
            let mut node = item.node.clone();
            node.id = item.node.id;
            doc.next_id = doc.next_id.max(node.id.0 + 1);
            doc.nodes.insert(node.id, node);
        }
        for root in &self.root {
            if doc.nodes.contains_key(root) && !doc.root.contains(root) {
                doc.root.push(*root);
            }
        }
        doc
    }

    /// Remove nodes whose registry shape no longer exists and repair every
    /// reference to a removed node. Returns the missing shape names, sorted.
    pub fn sanitize(&self) -> (TemplatePage, Vec<String>) {
        let mut missing = Vec::new();
        let mut dropped = HashSet::new();
        for item in &self.nodes {
            if let Some(shape) = &item.shape
                && !shape.trim().is_empty()
                && registry::by_id(shape).is_none()
            {
                missing.push(shape.trim().to_string());
                dropped.insert(item.node.id);
            }
        }
        missing.sort();
        missing.dedup();
        if dropped.is_empty() && self.references_are_sound() {
            return (self.clone(), missing);
        }
        let mut out = self.clone();
        out.nodes.retain(|n| !dropped.contains(&n.node.id));
        let live: HashSet<NodeId> = out.nodes.iter().map(|n| n.node.id).collect();
        out.root.retain(|id| live.contains(id));
        for item in &mut out.nodes {
            item.node.children.retain(|id| live.contains(id));
            if let NodeKind::Connector { start, end, .. } = &mut item.node.kind {
                if start.as_ref().is_some_and(|b| !live.contains(&b.node)) {
                    *start = None;
                }
                if end.as_ref().is_some_and(|b| !live.contains(&b.node)) {
                    *end = None;
                }
            }
            if item
                .node
                .instance
                .as_ref()
                .is_some_and(|l| !live.contains(&l.main))
            {
                item.node.instance = None;
            }
        }
        (out, missing)
    }

    fn references_are_sound(&self) -> bool {
        let live: HashSet<NodeId> = self.nodes.iter().map(|n| n.node.id).collect();
        self.root.iter().all(|id| live.contains(id))
            && self.nodes.iter().all(|item| {
                item.node.children.iter().all(|id| live.contains(id))
                    && match &item.node.kind {
                        NodeKind::Connector { start, end, .. } => {
                            start.as_ref().is_none_or(|b| live.contains(&b.node))
                                && end.as_ref().is_none_or(|b| live.contains(&b.node))
                        }
                        _ => true,
                    }
                    && item
                        .node
                        .instance
                        .as_ref()
                        .is_none_or(|l| live.contains(&l.main))
            })
    }

    /// A library item for previews and insertion. `layers` maps template layer
    /// ids to the current page's layers; an absent or `None` mapping puts the
    /// node on the page's background.
    pub fn to_item(&self, name: &str, layers: &HashMap<LayerId, Option<LayerId>>) -> LibItem {
        let mut nodes: Vec<Node> = self.nodes.iter().map(|n| n.node.clone()).collect();
        if !layers.is_empty() {
            for node in &mut nodes {
                node.layer = node.layer.and_then(|id| layers.get(&id).copied().flatten());
            }
        }
        LibItem {
            id: format!("template-{}", name.to_lowercase().replace(' ', "-")),
            name: name.into(),
            nodes,
            roots: self.root.clone(),
        }
    }

    /// Build a page for the project, with fresh page and node ids.
    pub fn to_page_data(&self, name: &str) -> PageData {
        let doc = self.to_doc();
        PageData {
            id: new_id(),
            name: name.into(),
            doc: Document {
                name: name.into(),
                ..doc
            },
            camera: None,
        }
    }

    /// Bounds of the page content.
    pub fn bounds(&self) -> Option<Rect> {
        let doc = self.to_doc();
        crate::io::svg::export_bounds(&doc, &doc.root)
    }
}

/// One ready starting point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,
    /// Optional SVG preview, so a template can be shared without the app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    pub pages: Vec<TemplatePage>,
}

impl Template {
    pub fn matches(&self, query: &str) -> bool {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        let hay = format!(
            "{} {} {} {}",
            self.name,
            self.category,
            self.description,
            self.keywords.join(" ")
        )
        .to_lowercase();
        q.split_whitespace().all(|w| hay.contains(w))
    }

    /// The sanitized page plus the missing shape names.
    pub fn page(&self, index: usize) -> Option<(TemplatePage, Vec<String>)> {
        self.pages.get(index).map(TemplatePage::sanitize)
    }

    /// Preview item: the first page, or the selected one.
    pub fn preview_item(&self, index: usize) -> Option<LibItem> {
        let (page, _) = self.page(index)?;
        Some(page.to_item(&self.name, &HashMap::new()))
    }

    /// Insert the selected page into the current page. Shape references are
    /// checked; missing shapes are reported and left out.
    pub fn insert_item(
        &self,
        index: usize,
        layers: &HashMap<LayerId, Option<LayerId>>,
    ) -> Option<(LibItem, Vec<String>)> {
        let (page, missing) = self.page(index)?;
        Some((page.to_item(&self.name, layers), missing))
    }

    /// Every page as project data, with one report of missing shape names.
    pub fn page_data(&self) -> (Vec<PageData>, Vec<String>) {
        let mut missing = Vec::new();
        let mut pages = Vec::new();
        for (i, page) in self.pages.iter().enumerate() {
            let (page, mut lost) = page.sanitize();
            missing.append(&mut lost);
            let name = if page.name.trim().is_empty() {
                format!("{} {}", self.name, i + 1)
            } else {
                format!("{} - {}", self.name, page.name)
            };
            pages.push(page.to_page_data(&name));
        }
        missing.sort();
        missing.dedup();
        (pages, missing)
    }

    /// Render the first (or selected) page as an SVG thumbnail.
    pub fn render_thumbnail(&self, index: usize) -> Option<String> {
        let (page, _) = self.page(index)?;
        let doc = page.to_doc();
        crate::io::svg::export(&doc, Some(&doc.root), Some(crate::model::Color::WHITE))
    }

    /// Fill an empty thumbnail from the first page.
    pub fn fill_thumbnail(&mut self) {
        if self.thumbnail.is_none() {
            self.thumbnail = self.render_thumbnail(0);
        }
    }
}

/// A named collection of templates. Bundled packs form one library; every file
/// in the custom folder is another.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateLibrary {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub templates: Vec<Template>,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Default for TemplateLibrary {
    fn default() -> Self {
        TemplateLibrary {
            format: TEMPLATE_FORMAT.into(),
            version: TEMPLATE_VERSION,
            name: "Templates".into(),
            templates: Vec::new(),
            path: None,
        }
    }
}

impl TemplateLibrary {
    pub fn bundled() -> TemplateLibrary {
        TemplateLibrary {
            name: "Built-in".into(),
            templates: packs::all(),
            ..TemplateLibrary::default()
        }
    }

    pub fn from_json(text: &str) -> Result<TemplateLibrary, String> {
        let mut lib: TemplateLibrary =
            serde_json::from_str(text).map_err(|e| format!("malformed template file: {e}"))?;
        if lib.format != TEMPLATE_FORMAT {
            return Err(format!(
                "not a Sketch template library (format {:?})",
                lib.format
            ));
        }
        if lib.version > TEMPLATE_VERSION {
            return Err(format!(
                "template library version {} is newer than this app supports ({TEMPLATE_VERSION})",
                lib.version
            ));
        }
        lib.format = TEMPLATE_FORMAT.into();
        lib.version = TEMPLATE_VERSION;
        lib.validate()?;
        Ok(lib)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("template library serializes")
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("template library has no name".into());
        }
        if self.templates.is_empty() {
            return Err("template library has no templates".into());
        }
        let mut ids = HashSet::new();
        for t in &self.templates {
            if t.id.trim().is_empty() {
                return Err("a template has no id".into());
            }
            if !ids.insert(t.id.clone()) {
                return Err(format!("duplicate template id {:?}", t.id));
            }
            if t.name.trim().is_empty() {
                return Err(format!("template {:?} has no name", t.id));
            }
            if t.category.trim().is_empty() {
                return Err(format!("template {:?} has no category", t.id));
            }
            if t.pages.is_empty() {
                return Err(format!("template {:?} has no pages", t.id));
            }
            for (i, page) in t.pages.iter().enumerate() {
                let mut node_ids = HashSet::new();
                for item in &page.nodes {
                    if !node_ids.insert(item.node.id) {
                        return Err(format!(
                            "template {:?} page {} repeats node {:?}",
                            t.id,
                            i + 1,
                            item.node.id
                        ));
                    }
                }
                for root in &page.root {
                    if !node_ids.contains(root) {
                        return Err(format!(
                            "template {:?} page {} has a missing root {:?}",
                            t.id,
                            i + 1,
                            root
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn load_file(path: &Path) -> Result<TemplateLibrary, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?;
        let mut lib =
            TemplateLibrary::from_json(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        lib.path = Some(path.to_path_buf());
        Ok(lib)
    }

    /// Load every `.json` file in a folder. Files that are not template
    /// libraries are skipped, and their errors are returned for the browser.
    pub fn load_folder(dir: &Path) -> (Vec<TemplateLibrary>, Vec<String>) {
        let mut libs = Vec::new();
        let mut errors = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (
                libs,
                vec![format!("could not read template folder {}", dir.display())],
            );
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.to_ascii_lowercase().ends_with(".json"))
            })
            .collect();
        paths.sort();
        for path in paths {
            match TemplateLibrary::load_file(&path) {
                Ok(lib) => libs.push(lib),
                Err(e) => errors.push(e),
            }
        }
        (libs, errors)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        crate::io::atomic::write(path, self.to_json().as_bytes())
    }

    /// Save one template as its own library file, with a thumbnail if possible.
    pub fn save_template(template: &Template, dir: &Path) -> Result<PathBuf, String> {
        let mut template = template.clone();
        template.fill_thumbnail();
        let lib = TemplateLibrary {
            name: template.name.clone(),
            templates: vec![template.clone()],
            ..TemplateLibrary::default()
        };
        lib.validate()?;
        let stem: String = template
            .id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let stem = if stem.trim_matches('-').is_empty() {
            "template".into()
        } else {
            stem
        };
        let path = dir.join(format!("{stem}{TEMPLATE_SUFFIX}"));
        lib.save(&path)
            .map_err(|e| format!("could not save {}: {e}", path.display()))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests;
