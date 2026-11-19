//! On-disk formats for projects and pages, with versioned migrations.
//!
//! Every file carries `format` and `version`. Loading parses into `serde_json::Value`, runs the
//! migration chain from the file's version up to the current one, then deserializes. Each step
//! is a pure function `Value -> Value`, so old files keep loading forever and every step is
//! unit-testable on its own.

use crate::geom::Camera;
use crate::model::{DiagramSettings, Document, Node, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

pub const PROJECT_FORMAT: &str = "sketch-rs-project";
pub const PAGE_FORMAT: &str = "sketch-rs-page";
/// JSON bundle embedded in SVG and PNG exports so they can be reopened as native pages.
pub const BUNDLE_FORMAT: &str = "sketch-rs-bundle";
/// Recovered bundle: project name, pages, and the active page id.
pub type PageBundle = (String, Vec<PageData>, Option<String>);
/// The single-file format of Sketch 0.1 (`.sketch-rs`), version 1. It migrates to a page.
pub const LEGACY_FORMAT: &str = "sketch-rs";
/// Tags from before the app was renamed from paper-rs to Sketch. Files carrying them still
/// load, and are saved back under the current tags.
pub const OLD_PROJECT_FORMAT: &str = "paper-rs-project";
pub const OLD_PAGE_FORMAT: &str = "paper-rs-page";
pub const OLD_LEGACY_FORMAT: &str = "paper-rs";

pub const PROJECT_VERSION: u32 = 1;
pub const BUNDLE_VERSION: u32 = 1;
/// v1: legacy single file. v2: page file inside a project. v3: fill layers (`fills`).
/// v4: page-scoped diagram settings. v5: page-scoped layers.
pub const PAGE_VERSION: u32 = 5;

#[derive(Debug)]
pub enum FormatError {
    Io(std::io::Error),
    Json(serde_json::Error),
    WrongFormat(String),
    TooNew { version: u32, supported: u32 },
    Invalid(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Io(e) => write!(f, "I/O error: {e}"),
            FormatError::Json(e) => write!(f, "malformed file: {e}"),
            FormatError::WrongFormat(s) => write!(f, "not a Sketch file (format {s:?})"),
            FormatError::TooNew { version, supported } => write!(
                f,
                "file version {version} is newer than this app supports ({supported})"
            ),
            FormatError::Invalid(s) => write!(f, "invalid document: {s}"),
        }
    }
}

impl std::error::Error for FormatError {}

impl From<std::io::Error> for FormatError {
    fn from(e: std::io::Error) -> Self {
        FormatError::Io(e)
    }
}

// ---------------------------------------------------------------------------------- project

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    /// Unix milliseconds.
    pub created: u64,
    pub modified: u64,
    pub pages: Vec<PageRef>,
    #[serde(default)]
    pub active_page: Option<String>,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        let now = now_ms();
        let page = PageRef {
            id: new_id(),
            name: "Page 1".into(),
        };
        Manifest {
            format: PROJECT_FORMAT.into(),
            version: PROJECT_VERSION,
            id: new_id(),
            name: name.into(),
            created: now,
            modified: now,
            active_page: Some(page.id.clone()),
            pages: vec![page],
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("manifest serializes")
    }

    pub fn from_json(s: &str) -> Result<Manifest, FormatError> {
        let v: Value = serde_json::from_str(s).map_err(FormatError::Json)?;
        let v = migrate(
            v,
            PROJECT_FORMAT,
            PROJECT_VERSION,
            &[OLD_PROJECT_FORMAT],
            &[],
        )?;
        serde_json::from_value(v).map_err(FormatError::Json)
    }
}

// ---------------------------------------------------------------------------------- page

/// One page: an infinite canvas with its own nodes and camera.
#[derive(Debug, Clone, PartialEq)]
pub struct PageData {
    pub id: String,
    pub name: String,
    pub doc: Document,
    pub camera: Option<Camera>,
}

#[derive(Serialize, Deserialize)]
struct PageFile {
    format: String,
    version: u32,
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    camera: Option<Camera>,
    /// Defaults keep v3 and older pages loading unchanged.
    #[serde(default)]
    settings: DiagramSettings,
    /// Defaults keep v4 and older pages loading unchanged.
    #[serde(default)]
    layers: Vec<crate::model::Layer>,
    #[serde(default)]
    next_layer: u64,
    next_id: u64,
    root: Vec<NodeId>,
    nodes: Vec<Node>,
}

impl PageFile {
    fn from_page(p: &PageData) -> Self {
        let mut nodes: Vec<Node> = p.doc.nodes.values().cloned().collect();
        nodes.sort_by_key(|n| n.id);
        PageFile {
            format: PAGE_FORMAT.into(),
            version: PAGE_VERSION,
            id: p.id.clone(),
            name: p.name.clone(),
            camera: p.camera,
            settings: p.doc.settings.clone(),
            layers: p.doc.layers.clone(),
            next_layer: p.doc.next_layer,
            next_id: p.doc.next_id,
            root: p.doc.root.clone(),
            nodes,
        }
    }

    fn into_page(self) -> Result<PageData, FormatError> {
        let max_id = self.nodes.iter().map(|n| n.id.0).max().unwrap_or(0);
        let max_layer = self
            .nodes
            .iter()
            .filter_map(|n| n.layer)
            .map(|l| l.0)
            .max()
            .unwrap_or(0);
        let nodes: HashMap<NodeId, Node> = self.nodes.into_iter().map(|n| (n.id, n)).collect();
        let doc = Document {
            name: self.name.clone(),
            nodes,
            root: self.root,
            next_id: self.next_id.max(max_id + 1),
            layers: self.layers,
            next_layer: self.next_layer.max(max_layer + 1).max(2),
            settings: self.settings,
            context: Default::default(),
        };
        doc.validate().map_err(FormatError::Invalid)?;
        Ok(PageData {
            id: self.id,
            name: self.name,
            doc,
            camera: self.camera,
        })
    }
}

/// Original image bytes, so image fills survive a trip through SVG or PNG.
#[derive(Serialize, Deserialize)]
struct BundleAsset {
    ext: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct BundleFile {
    format: String,
    version: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active_page: Option<String>,
    pages: Vec<PageFile>,
    #[serde(default)]
    assets: BTreeMap<String, BundleAsset>,
}

/// Page migrations, indexed by source version: `PAGE_STEPS[0]` turns v1 into v2.
const PAGE_STEPS: &[fn(Value) -> Value] =
    &[page_v1_to_v2, page_v2_to_v3, page_v3_to_v4, page_v4_to_v5];
/// Formats accepted as older versions of a page.
const PAGE_ALIASES: &[&str] = &[LEGACY_FORMAT, OLD_LEGACY_FORMAT, OLD_PAGE_FORMAT];

/// v1 (`.sketch-rs` single file) → v2 page: new format tag and a page id.
fn page_v1_to_v2(mut v: Value) -> Value {
    v["format"] = json!(PAGE_FORMAT);
    if v.get("id").is_none() {
        v["id"] = json!(new_id());
    }
    if v.get("name").is_none() {
        v["name"] = json!("Page 1");
    }
    v
}

/// v2 → v3: `style.fill: "#rrggbb"` becomes `style.fills: [{type: solid, color}]`.
fn page_v2_to_v3(mut v: Value) -> Value {
    if let Some(nodes) = v.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for n in nodes {
            let Some(style) = n.get_mut("style").and_then(|s| s.as_object_mut()) else {
                continue;
            };
            if let Some(fill) = style.remove("fill")
                && let Some(c) = fill.as_str()
            {
                style.insert("fills".into(), json!([{ "type": "solid", "color": c }]));
            }
        }
    }
    v
}

/// v3 → v4: page settings appear. Missing settings deserialize to the defaults,
/// so the change only needs to record the version.
fn page_v3_to_v4(v: Value) -> Value {
    v
}

/// v4 → v5: page layers appear. Every page starts with the Background layer
/// and its content rides on it through the node-layer default, matching
/// draw.io's "everything starts on one layer".
fn page_v4_to_v5(mut v: Value) -> Value {
    if v.get("layers").is_none() {
        v["layers"] = json!([{ "id": 1, "name": "Background" }]);
    }
    if v.get("next_layer").is_none() {
        v["next_layer"] = json!(2);
    }
    v
}

pub fn page_to_json(p: &PageData) -> String {
    serde_json::to_string_pretty(&PageFile::from_page(p)).expect("page serializes")
}

/// Parse a page file (or a legacy `.sketch-rs` document) of any supported version.
pub fn page_from_json(s: &str) -> Result<PageData, FormatError> {
    let v: Value = serde_json::from_str(s).map_err(FormatError::Json)?;
    let v = migrate(v, PAGE_FORMAT, PAGE_VERSION, PAGE_ALIASES, PAGE_STEPS)?;
    let f: PageFile = serde_json::from_value(v).map_err(FormatError::Json)?;
    f.into_page()
}

/// Serialize every page, plus the image assets they reference, for embedding in an export.
pub fn pages_to_json(name: &str, pages: &[PageData], active: Option<&str>) -> String {
    let mut ids: Vec<String> = pages
        .iter()
        .flat_map(|p| crate::assets::used_by(&p.doc))
        .collect();
    ids.sort();
    ids.dedup();
    let mut assets = BTreeMap::new();
    for id in ids {
        if let Some(a) = crate::assets::get(&id) {
            assets.insert(
                id,
                BundleAsset {
                    ext: a.ext.to_string(),
                    data: crate::assets::base64(&a.bytes),
                },
            );
        }
    }
    let f = BundleFile {
        format: BUNDLE_FORMAT.into(),
        version: BUNDLE_VERSION,
        name: name.to_string(),
        active_page: active.map(str::to_string),
        pages: pages.iter().map(PageFile::from_page).collect(),
        assets,
    };
    serde_json::to_string_pretty(&f).expect("page bundle serializes")
}

/// Recover the project name, pages and active page from an embedded bundle.
pub fn pages_from_json(s: &str) -> Result<PageBundle, FormatError> {
    let v: Value = serde_json::from_str(s).map_err(FormatError::Json)?;
    let v = migrate(v, BUNDLE_FORMAT, BUNDLE_VERSION, &[], &[])?;
    let f: BundleFile = serde_json::from_value(v).map_err(FormatError::Json)?;
    for (id, a) in &f.assets {
        let bytes = crate::assets::base64_decode(&a.data)
            .ok_or_else(|| FormatError::Invalid(format!("bad image data for {id}")))?;
        crate::assets::register(&bytes).map_err(FormatError::Invalid)?;
    }
    let pages = f
        .pages
        .into_iter()
        .map(PageFile::into_page)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((f.name, pages, f.active_page))
}

/// Check the format tag and run the migration chain up to `current`.
fn migrate(
    mut v: Value,
    format: &str,
    current: u32,
    aliases: &[&str],
    steps: &[fn(Value) -> Value],
) -> Result<Value, FormatError> {
    let tag = v
        .get("format")
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string();
    if tag != format && !aliases.contains(&tag.as_str()) {
        return Err(FormatError::WrongFormat(tag));
    }
    v["format"] = json!(format);
    let mut version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
    if version > current {
        return Err(FormatError::TooNew {
            version,
            supported: current,
        });
    }
    while version < current {
        let step = steps
            .get(version as usize - 1)
            .ok_or_else(|| FormatError::Invalid(format!("no migration from v{version}")))?;
        v = step(v);
        version += 1;
        v["version"] = json!(version);
    }
    Ok(v)
}

// ---------------------------------------------------------------------------------- helpers

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Random 16-hex-digit identifier.
pub fn new_id() -> String {
    format!("{:016x}", fastrand::u64(..))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn page_roundtrip_is_deterministic() {
        let ed = demo::showcase();
        let p = PageData {
            id: "abc".into(),
            name: "Plan".into(),
            doc: Document {
                name: "Plan".into(),
                ..ed.doc.clone()
            },
            camera: Some(Camera::default()),
        };
        let s = page_to_json(&p);
        let back = page_from_json(&s).unwrap();
        assert_eq!(back, p);
        assert_eq!(page_to_json(&back), s);
    }

    #[test]
    fn legacy_v1_file_migrates_to_page() {
        let ed = demo::showcase();
        let legacy = crate::io::file::to_string(&ed.doc, None);
        assert!(legacy.contains("\"version\": 1"));
        let p = page_from_json(&legacy).unwrap();
        assert_eq!(p.doc.nodes, ed.doc.nodes);
        assert_eq!(p.name, ed.doc.name);
        assert_eq!(p.id.len(), 16);
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(matches!(
            page_from_json(r#"{"format":"x","version":1}"#),
            Err(FormatError::WrongFormat(_))
        ));
        assert!(matches!(
            page_from_json(r#"{"format":"sketch-rs-page","version":99}"#),
            Err(FormatError::TooNew { version: 99, .. })
        ));
        assert!(matches!(
            Manifest::from_json(r#"{"format":"sketch-rs-page","version":1}"#),
            Err(FormatError::WrongFormat(_))
        ));
        let broken = r#"{"format":"sketch-rs-page","version":2,"id":"a","name":"x","next_id":1,"root":[5],"nodes":[]}"#;
        assert!(matches!(
            page_from_json(broken),
            Err(FormatError::Invalid(_))
        ));
    }

    #[test]
    fn v2_fill_migrates_to_fill_layers() {
        let v2 = r##"{"format":"sketch-rs-page","version":2,"id":"a","name":"P","next_id":3,
            "root":[1,2],"nodes":[
            {"id":1,"name":"R","type":"rect","rect":{"x":0,"y":0,"w":5,"h":5},
             "style":{"fill":"#ff0000","stroke":null,"stroke_width":1,"corner_radius":2,"opacity":1}},
            {"id":2,"name":"E","type":"ellipse","rect":{"x":0,"y":0,"w":5,"h":5},
             "style":{"fill":null,"stroke":"#000000","stroke_width":1,"corner_radius":0,"opacity":1}}]}"##;
        let p = page_from_json(v2).unwrap();
        let r = p.doc.get(NodeId(1)).unwrap();
        assert_eq!(r.style.fill(), Some(crate::model::Color::rgb(255, 0, 0)));
        assert_eq!(r.style.corner_radius, 2.0);
        assert!(p.doc.get(NodeId(2)).unwrap().style.fills.is_empty());
        assert!(page_to_json(&p).contains("\"version\": 5"));
    }

    #[test]
    fn page_roundtrip_preserves_diagram_settings() {
        use crate::model::{DefaultStyle, ManualGuide, Orientation, PaperSize, Style, TextScript};
        let ed = demo::showcase();
        let mut doc = ed.doc.clone();
        doc.settings.grid = false;
        doc.settings.grid_size = 22.0;
        doc.settings.grid_color = Some(crate::model::Color::rgb(0x33, 0x66, 0x99));
        doc.settings.rulers = true;
        doc.settings.manual_guides =
            vec![ManualGuide::vertical(120.0), ManualGuide::horizontal(45.5)];
        doc.settings.background_enabled = true;
        doc.settings.background_color = crate::model::Color::rgb(0xf0, 0xe8, 0xd8);
        doc.settings.shadow = true;
        doc.settings.sketch = true;
        doc.settings.rounded = true;
        doc.settings.scheme = Some("Sage".into());
        doc.settings.connection_arrows = false;
        doc.settings.autosave = false;
        doc.settings.paper = PaperSize::A4;
        doc.settings.orientation = Orientation::Portrait;
        doc.settings
            .metadata
            .insert("owner".into(), "design".into());
        doc.settings.shape_default = Some(DefaultStyle::new(
            Style::with_fill(crate::model::Color::rgb(1, 2, 3)),
            crate::model::TextStyle::default(),
        ));
        let tid = doc
            .root
            .iter()
            .copied()
            .find(|id| doc.get(*id).is_some_and(|n| n.kind.text().is_some()))
            .expect("demo has a text node");
        let tn = doc.nodes.get_mut(&tid).unwrap();
        tn.text_style.script = TextScript::Super;
        tn.style.rotation = 90.0;
        tn.style.label_upright = true;
        tn.connector_default = Some(DefaultStyle::new(
            Style {
                stroke: Some(crate::model::Color::rgb(0x11, 0x22, 0x33)),
                ..Style::default()
            },
            crate::model::TextStyle::default(),
        ));
        let len = tn.kind.text().unwrap().chars().count();
        tn.text_style
            .apply_range(0, len.min(1), |s| s.script = Some(TextScript::Sub));
        let p = PageData {
            id: "abc".into(),
            name: doc.name.clone(),
            doc,
            camera: None,
        };
        let s = page_to_json(&p);
        let back = page_from_json(&s).unwrap();
        assert_eq!(back, p);
        let back_t = back.doc.get(tid).unwrap();
        assert_eq!(back_t.text_style.script, TextScript::Super);
        assert_eq!(back_t.style.rotation, 90.0);
        assert!(back_t.style.label_upright);
        assert_eq!(
            back_t.text_style.spans.first().and_then(|s| s.script),
            Some(TextScript::Sub)
        );
        assert_eq!(
            back_t.connector_default,
            p.doc.get(tid).unwrap().connector_default
        );
        assert_eq!(back.doc.settings.grid_size, 22.0);
        assert!(back.doc.settings.rulers);
        assert_eq!(
            back.doc.settings.manual_guides,
            vec![ManualGuide::vertical(120.0), ManualGuide::horizontal(45.5)]
        );
        assert_eq!(back.doc.settings.metadata.get("owner").unwrap(), "design");
        assert_eq!(
            back.doc.settings.shape_default,
            p.doc.settings.shape_default
        );
    }

    #[test]
    fn page_view_is_a_session_only_view_option() {
        let mut doc = demo::showcase().doc;
        doc.settings.page_view = true;
        let p = PageData {
            id: "abc".into(),
            name: doc.name.clone(),
            doc,
            camera: None,
        };
        let s = page_to_json(&p);
        assert!(
            !s.contains("page_view"),
            "page view must not reach the file"
        );
        assert!(!page_from_json(&s).unwrap().doc.settings.page_view);
    }

    #[test]
    fn v3_pages_load_with_default_settings() {
        let ed = demo::showcase();
        let p = PageData {
            id: "abc".into(),
            name: "Plan".into(),
            doc: ed.doc.clone(),
            camera: None,
        };
        let mut v: serde_json::Value = serde_json::from_str(&page_to_json(&p)).unwrap();
        v["version"] = json!(3);
        v.as_object_mut().unwrap().remove("settings");
        let back = page_from_json(&v.to_string()).unwrap();
        assert_eq!(back.doc.settings, crate::model::DiagramSettings::default());
        assert_eq!(back.doc.nodes, ed.doc.nodes);
        assert!(page_to_json(&back).contains("\"version\": 5"));
    }

    #[test]
    fn manifest_roundtrip() {
        let m = Manifest::new("Hello");
        assert_eq!(Manifest::from_json(&m.to_json()).unwrap(), m);
    }

    #[test]
    fn files_from_before_the_rename_load_under_the_new_tags() {
        let ed = demo::showcase();
        let legacy = crate::io::file::to_string(&ed.doc, None).replace(
            &format!("\"{LEGACY_FORMAT}\""),
            &format!("\"{OLD_LEGACY_FORMAT}\""),
        );
        assert_eq!(page_from_json(&legacy).unwrap().doc.nodes, ed.doc.nodes);

        let p = PageData {
            id: "abc".into(),
            name: "Plan".into(),
            doc: ed.doc.clone(),
            camera: None,
        };
        let page = page_to_json(&p);
        let old_page = page.replace(PAGE_FORMAT, OLD_PAGE_FORMAT);
        assert_ne!(old_page, page);
        assert_eq!(page_to_json(&page_from_json(&old_page).unwrap()), page);

        let m = Manifest::new("Hello");
        let old_manifest = m.to_json().replace(PROJECT_FORMAT, OLD_PROJECT_FORMAT);
        assert_eq!(Manifest::from_json(&old_manifest).unwrap(), m);
    }

    #[test]
    fn embedded_bundle_roundtrips_pages_and_image_assets() {
        let first = PageData {
            id: "p1".into(),
            name: "First".into(),
            doc: Document {
                name: "First".into(),
                ..demo::showcase().doc
            },
            camera: Some(Camera::default()),
        };
        let img = image::RgbaImage::from_fn(4, 3, |x, y| {
            image::Rgba([(x * 60) as u8, (y * 80) as u8, 20, 255])
        });
        let asset = crate::assets::register_rgba(&img);
        let mut doc = Document::default();
        let mut node = Node::new(
            NodeId(1),
            crate::model::NodeKind::Rect,
            crate::geom::Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        node.style.fills = vec![crate::model::Paint::Image {
            asset: asset.clone(),
            mode: crate::model::ImageMode::Fill,
            opacity: 1.0,
        }];
        doc.nodes.insert(node.id, node);
        doc.root.push(NodeId(1));
        doc.next_id = 2;
        let second = PageData {
            id: "p2".into(),
            name: "Second".into(),
            doc: Document {
                name: "Second".into(),
                ..doc
            },
            camera: None,
        };
        let pages = vec![first, second];
        let json = pages_to_json("Board", &pages, Some("p2"));
        assert!(json.contains(BUNDLE_FORMAT));
        let (name, back, active) = pages_from_json(&json).unwrap();
        assert_eq!(name, "Board");
        assert_eq!(active.as_deref(), Some("p2"));
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], pages[0]);
        assert_eq!(back[1].name, "Second");
        assert_eq!(back[1].doc.nodes.len(), 1);
        let Some(crate::model::Paint::Image {
            asset: restored, ..
        }) = back[1].doc.nodes[&NodeId(1)].style.fills.first()
        else {
            panic!("image fill was not restored");
        };
        assert_eq!(restored, &asset);
        assert!(crate::assets::get(restored).is_some());
    }
}
