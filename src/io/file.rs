//! The `.sketch-rs` document format: versioned, pretty-printed JSON.

use crate::geom::Camera;
use crate::model::{DiagramSettings, Document, Node, NodeId};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FORMAT: &str = "sketch-rs";
pub const VERSION: u32 = 1;
pub const EXTENSION: &str = "sketch-rs";
/// Documents saved before the rename from paper-rs. They open like `.sketch-rs` files.
pub const OLD_EXTENSION: &str = "paper-rs";

/// Whether `path` has a document extension, current or pre-rename.
pub fn is_document(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e == EXTENSION || e == OLD_EXTENSION)
}

#[derive(Serialize, Deserialize)]
struct FileV1 {
    format: String,
    version: u32,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    camera: Option<Camera>,
    next_id: u64,
    root: Vec<NodeId>,
    nodes: Vec<Node>,
    /// Page/diagram settings added after v1 shipped. Older readers ignore
    /// unknown fields, and missing settings load as defaults.
    #[serde(default, skip_serializing_if = "DiagramSettings::is_default")]
    settings: DiagramSettings,
}

#[derive(Debug)]
pub enum FileError {
    Io(std::io::Error),
    Json(serde_json::Error),
    NotSketchRs,
    UnsupportedVersion(u32),
    Invalid(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileError::Io(e) => write!(f, "I/O error: {e}"),
            FileError::Json(e) => write!(f, "malformed file: {e}"),
            FileError::NotSketchRs => write!(f, "not a Sketch document"),
            FileError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "file version {v} is newer than this app supports ({VERSION})"
                )
            }
            FileError::Invalid(s) => write!(f, "invalid document: {s}"),
        }
    }
}

impl std::error::Error for FileError {}

pub fn to_string(doc: &Document, camera: Option<&Camera>) -> String {
    let mut nodes: Vec<Node> = doc.nodes.values().cloned().collect();
    nodes.sort_by_key(|n| n.id);
    let f = FileV1 {
        format: FORMAT.into(),
        version: VERSION,
        name: doc.name.clone(),
        camera: camera.copied(),
        next_id: doc.next_id,
        root: doc.root.clone(),
        nodes,
        settings: doc.settings.clone(),
    };
    serde_json::to_string_pretty(&f).expect("document serializes")
}

pub fn from_str(s: &str) -> Result<(Document, Option<Camera>), FileError> {
    use crate::project::format::{FormatError, page_from_json};
    match page_from_json(s) {
        Ok(p) => Ok((p.doc, p.camera)),
        Err(FormatError::WrongFormat(_)) => Err(FileError::NotSketchRs),
        Err(FormatError::TooNew { version, .. }) => Err(FileError::UnsupportedVersion(version)),
        Err(FormatError::Json(e)) => Err(FileError::Json(e)),
        Err(FormatError::Invalid(e)) => Err(FileError::Invalid(e)),
        Err(FormatError::Io(e)) => Err(FileError::Io(e)),
    }
}

/// Write atomically (temp file + rename) so a crash never leaves a truncated document.
pub fn save(path: &Path, doc: &Document, camera: Option<&Camera>) -> Result<(), FileError> {
    let tmp = path.with_extension("sketch-rs.tmp");
    std::fs::write(&tmp, to_string(doc, camera)).map_err(FileError::Io)?;
    std::fs::rename(&tmp, path).map_err(FileError::Io)
}

pub fn load(path: &Path) -> Result<(Document, Option<Camera>), FileError> {
    let s = std::fs::read_to_string(path).map_err(FileError::Io)?;
    from_str(&s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demo;

    #[test]
    fn roundtrip_demo_document() {
        let mut ed = demo::showcase();
        ed.doc.settings.grid = false;
        ed.doc.settings.grid_size = 12.5;
        ed.doc.settings.rulers = true;
        ed.doc.settings.manual_guides = vec![
            crate::model::ManualGuide::vertical(33.0),
            crate::model::ManualGuide::horizontal(77.5),
        ];
        ed.doc
            .settings
            .metadata
            .insert("owner".into(), "Micky".into());
        let cam = Camera {
            pan: crate::geom::v2(3.0, 4.0),
            zoom: 0.5,
        };
        let s = to_string(&ed.doc, Some(&cam));
        let (doc, cam2) = from_str(&s).unwrap();
        assert_eq!(doc, ed.doc);
        assert_eq!(cam2, Some(cam));
        // Serialization is deterministic.
        assert_eq!(to_string(&doc, Some(&cam)), s);
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(matches!(
            from_str("{\"hello\":1}"),
            Err(FileError::NotSketchRs)
        ));
        assert!(matches!(
            from_str("{\"format\":\"x\",\"version\":1}"),
            Err(FileError::NotSketchRs)
        ));
        assert!(matches!(
            from_str("{\"format\":\"sketch-rs\",\"version\":99}"),
            Err(FileError::UnsupportedVersion(99))
        ));
        let broken =
            r#"{"format":"sketch-rs","version":1,"name":"x","next_id":1,"root":[5],"nodes":[]}"#;
        assert!(matches!(from_str(broken), Err(FileError::Invalid(_))));
    }

    #[test]
    fn connector_labels_round_trip() {
        use crate::geom::{Rect, v2};
        use crate::model::{Color, Head, LineJump, NodeKind, Route, TextAlign};

        let mut ed = demo::showcase();
        let id = ed.add_node(
            Node::new(
                NodeId(900),
                NodeKind::Connector {
                    points: vec![v2(0.0, 0.0), v2(120.0, 40.0)],
                    start: None,
                    end: None,
                    route: Route::Straight,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(0.0, 0.0, 120.0, 40.0),
            ),
            None,
        );
        ed.edit("label", |tx| {
            let n = tx.node_mut(id).unwrap();
            n.label = "yes".into();
            n.label_size = Some(14.0);
            n.label_color = Some(Color::rgb(0x11, 0x22, 0x33));
            n.label_wrap = false;
            n.label_opacity = 0.8;
            n.label_pos = Some(0.25);
            n.label_offset = Some(v2(12.0, -8.0));
            n.text_bg = Some(Color::WHITE);
            n.text_border = Some(Color::INK);
            n.text_style.align = Some(TextAlign::Right);
        });
        let s = to_string(&ed.doc, None);
        let (doc, _) = from_str(&s).unwrap();
        let n = doc.get(id).unwrap();
        assert_eq!(n.label, "yes");
        assert_eq!(n.label_size, Some(14.0));
        assert_eq!(n.label_color, Some(Color::rgb(0x11, 0x22, 0x33)));
        assert!(!n.label_wrap);
        assert_eq!(n.label_opacity, 0.8);
        assert_eq!(n.label_pos, Some(0.25));
        assert_eq!(n.label_offset, Some(v2(12.0, -8.0)));
        assert_eq!(n.text_bg, Some(Color::WHITE));
        assert_eq!(n.text_border, Some(Color::INK));
        assert_eq!(n.text_style.align, Some(TextAlign::Right));
    }

    #[test]
    fn extra_connector_labels_round_trip() {
        use crate::geom::{Rect, v2};
        use crate::model::{ExtraLabel, Head, LineJump, NodeKind, Route};

        let mut ed = demo::showcase();
        let id = ed.add_node(
            Node::new(
                NodeId(901),
                NodeKind::Connector {
                    points: vec![v2(0.0, 0.0), v2(120.0, 40.0)],
                    start: None,
                    end: None,
                    route: Route::Straight,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(0.0, 0.0, 120.0, 40.0),
            ),
            None,
        );
        ed.edit("labels", |tx| {
            let n = tx.node_mut(id).unwrap();
            n.label = "main".into();
            n.labels.push(ExtraLabel {
                text: "second".into(),
                pos: Some(0.75),
                offset: Some(v2(0.0, 20.0)),
            });
            n.labels.push(ExtraLabel::new("third"));
        });
        let s = to_string(&ed.doc, None);
        let (doc, _) = from_str(&s).unwrap();
        let n = doc.get(id).unwrap();
        assert_eq!(n.label_count(), 3);
        assert_eq!(n.label_text(0), Some("main"));
        assert_eq!(n.label_text(1), Some("second"));
        assert_eq!(n.label_text(2), Some("third"));
        assert_eq!(n.labels[0].pos, Some(0.75));
        assert_eq!(n.labels[0].offset, Some(v2(0.0, 20.0)));
        assert_eq!(n.labels[1].pos, None);
        assert_eq!(n.labels[1].offset, None);
        // A document saved before extras keeps exactly one label.
        let s = to_string(&ed.doc, None);
        assert!(from_str(&s).is_ok());
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("sketch-rs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("doc.sketch-rs");
        let ed = demo::showcase();
        save(&path, &ed.doc, None).unwrap();
        let (doc, cam) = load(&path).unwrap();
        assert_eq!(doc, ed.doc);
        assert!(cam.is_none());
        std::fs::remove_dir_all(dir).ok();
    }
}
