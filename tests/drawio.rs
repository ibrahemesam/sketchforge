//! draw.io import, export and the project/page integration around them.

mod common;

use serde_json::json;
use sketch_rs::editor::Editor;
use sketch_rs::geom::Rect;
use sketch_rs::io::drawio;
use sketch_rs::io::export::{self, Format, Scope, Spec};
use sketch_rs::io::import;
use sketch_rs::model::{Color, DefaultStyle, Document, Node, NodeId, NodeKind, Style, TextStyle};
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("sketch-rs-drawio-{tag}-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drawio/two-pages.drawio")
}

fn metadata_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/drawio/metadata.drawio")
}

#[test]
fn import_reads_pages_layers_and_reports_fidelity() {
    let imported = import::import_detailed(&fixture()).unwrap();
    assert_eq!(imported.pages.len(), 2);
    assert_eq!(imported.name, "two-pages");
    let report = imported.report.expect("draw.io report");
    assert_eq!(report.pages, 2);
    assert!(report.shapes >= 6);
    assert!(report.connectors >= 1);
    assert_eq!(report.layers, 1);
    assert!(report.dropped.is_empty());
    let flow = &imported.pages[0];
    assert_eq!(flow.name, "Flow");
    assert_eq!(flow.doc.layers.len(), 2);
    assert_eq!(flow.doc.layers[1].name, "Notes");
    assert!(
        flow.doc
            .nodes
            .values()
            .any(|n| n.label == "Styled" && n.style.fill() == Some(Color::rgb(0xda, 0xe8, 0xfc)))
    );
}

#[test]
fn export_writes_every_page_and_reimports_the_document() {
    let imported = import::import_detailed(&fixture()).unwrap();
    let refs: Vec<(&str, &Document)> = imported
        .pages
        .iter()
        .map(|p| (p.name.as_str(), &p.doc))
        .collect();
    let items: Vec<export::Item> = refs
        .iter()
        .map(|(name, doc)| export::Item {
            name: name.to_string(),
            doc,
            roots: doc.layered_root(),
        })
        .collect();
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    assert!(spec.single_file(items.len()));
    let dir = tmp("export");
    let out = dir.join("board.drawio");
    let written = export::write(&spec, &items, &out, None).unwrap();
    assert_eq!(written, vec![out.clone()]);

    let again = import::import_detailed(&out).unwrap();
    assert_eq!(again.pages.len(), 2);
    assert_eq!(again.pages[0].name, "Flow");
    assert_eq!(again.pages[1].name, "Detail");
    assert_eq!(again.pages[0].doc.layers.len(), 2);
    assert_eq!(again.pages[0].doc.layers[1].name, "Notes");
    let before: Vec<(String, String, String)> = imported.pages[0]
        .doc
        .paint_order()
        .into_iter()
        .filter_map(|id| imported.pages[0].doc.get(id))
        .map(|n| (n.name.clone(), n.label.clone(), n.kind.type_name().into()))
        .collect();
    let after: Vec<(String, String, String)> = again.pages[0]
        .doc
        .paint_order()
        .into_iter()
        .filter_map(|id| again.pages[0].doc.get(id))
        .map(|n| (n.name.clone(), n.label.clone(), n.kind.type_name().into()))
        .collect();
    assert_eq!(before, after);
    let edge = again.pages[0]
        .doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .expect("edge survives round trip");
    assert_eq!(edge.label, "Yes");
    assert!(!edge.labels.is_empty());
}

#[test]
fn export_and_reimport_keep_the_source_connector_default() {
    let mut doc = Document {
        name: "Source defaults".into(),
        ..Document::default()
    };
    let node = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0));
    let source_default = DefaultStyle::new(
        Style {
            stroke: Some(Color::rgb(0x11, 0x22, 0x33)),
            stroke_width: 4.0,
            ..Style::default()
        },
        TextStyle::default(),
    );
    let mut node = node;
    node.connector_default = Some(source_default.clone());
    doc.nodes.insert(node.id, node);
    doc.root.push(NodeId(1));

    let items = vec![export::Item {
        name: doc.name.clone(),
        doc: &doc,
        roots: doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let dir = tmp("connector-default-export");
    let out = dir.join("defaults.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<UserObject"), "{text}");
    assert!(text.contains("__sketchConnectorDefault"), "{text}");

    let again = import::import_detailed(&out).unwrap();
    let back = again.pages[0].doc.get(NodeId(1)).expect("shape survives");
    assert_eq!(back.connector_default, Some(source_default));
    assert!(
        back.metadata.is_empty(),
        "sidecar key stays out of metadata"
    );
}

#[test]
fn export_and_reimport_keep_an_upright_label_flag() {
    let mut doc = Document {
        name: "Upright labels".into(),
        ..Document::default()
    };
    let mut node = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0));
    node.label = "Turned".into();
    node.style.rotation = 30.0;
    node.style.label_upright = true;
    doc.nodes.insert(node.id, node);
    doc.root.push(NodeId(1));

    let items = vec![export::Item {
        name: doc.name.clone(),
        doc: &doc,
        roots: doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let dir = tmp("upright-label-export");
    let out = dir.join("upright.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("rotation=30;"), "{text}");
    assert!(text.contains("__sketchLabelUpright"), "{text}");

    let again = import::import_detailed(&out).unwrap();
    let back = again.pages[0].doc.get(NodeId(1)).expect("shape survives");
    assert!((back.style.rotation - 30.0).abs() < 1e-9);
    assert!(back.style.label_upright, "upright flag was dropped");
    assert!(
        back.metadata.is_empty(),
        "sidecar key stays out of metadata"
    );
}

#[test]
fn import_reads_user_object_metadata_links_tooltips_and_tags() {
    let imported = import::import_detailed(&metadata_fixture()).unwrap();
    let page = &imported.pages[0];
    let portal = page
        .doc
        .nodes
        .values()
        .find(|n| n.label == "Portal")
        .expect("portal shape");
    assert_eq!(portal.link, "https://example.com/docs");
    assert_eq!(portal.tooltip, "Open the portal docs");
    assert_eq!(portal.tags, vec!["web".to_string(), "external".to_string()]);
    assert_eq!(portal.metadata["owner"], "Ada");
    assert_eq!(portal.metadata["space key"], "value with space");
    assert_eq!(portal.metadata["2fa"], "enabled");

    let details = page
        .doc
        .nodes
        .values()
        .find(|n| n.label == "Details")
        .expect("plain cell");
    assert_eq!(details.link, "data:page/id,detail");
    assert_eq!(details.tooltip, "Go to Detail");
    assert_eq!(details.tags, vec!["nav".to_string()]);

    let edge = page
        .doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .expect("edge");
    assert_eq!(edge.link, "page:Data");
    assert_eq!(edge.tooltip, "Internal jump");
    assert_eq!(edge.tags, vec!["flow".to_string()]);
    assert_eq!(edge.metadata["kind"], "primary");
}

#[test]
fn export_keeps_user_object_data_and_sidecar_keys() {
    let imported = import::import_detailed(&metadata_fixture()).unwrap();
    let page = &imported.pages[0];
    let items = vec![export::Item {
        name: page.name.clone(),
        doc: &page.doc,
        roots: page.doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let dir = tmp("metadata-export");
    let out = dir.join("data.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("<UserObject"), "{text}");
    assert!(text.contains("sketch-data="), "{text}");
    assert!(text.contains("link=\"https://example.com/docs\""), "{text}");

    let again = import::import_detailed(&out).unwrap();
    let doc = &again.pages[0].doc;
    let portal = doc
        .nodes
        .values()
        .find(|n| n.label == "Portal")
        .expect("portal survives");
    assert_eq!(portal.link, "https://example.com/docs");
    assert_eq!(portal.tooltip, "Open the portal docs");
    assert_eq!(portal.tags, vec!["web".to_string(), "external".to_string()]);
    assert_eq!(portal.metadata["owner"], "Ada");
    assert_eq!(portal.metadata["space key"], "value with space");
    assert_eq!(portal.metadata["2fa"], "enabled");

    let edge = doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .expect("edge survives");
    assert_eq!(edge.link, "page:Data");
    assert_eq!(edge.tooltip, "Internal jump");
    assert_eq!(edge.tags, vec!["flow".to_string()]);
    assert_eq!(edge.metadata["kind"], "primary");
}

#[test]
fn append_pages_keeps_layers_and_switches_to_the_first_new_page() {
    let imported = import::import_detailed(&fixture()).unwrap();
    let mut ed = Editor::default();
    let before = ed.pages.len();
    let at = ed.append_pages(imported.pages);
    assert_eq!(at, before);
    assert_eq!(ed.pages.len(), before + 2);
    assert_eq!(ed.active_page, at);
    assert_eq!(ed.page_name(), "Flow");
    assert_eq!(ed.doc.layers.len(), 2);
    assert_eq!(ed.doc.layers[1].name, "Notes");
    ed.switch_page(at + 1);
    assert_eq!(ed.page_name(), "Detail");
    assert!(
        ed.doc
            .nodes
            .values()
            .any(|n| n.name == "Frame" || n.is_frame()),
        "swimlane imports as a frame"
    );
    assert_ne!(ed.pages[at].id, ed.pages[at + 1].id);
}

#[test]
fn encrypted_files_are_refused_with_a_clear_message() {
    let dir = tmp("encrypted");
    let p = dir.join("locked.drawio");
    let body = sketch_rs::assets::base64(b"Salted__not-a-real-drawio-stream");
    std::fs::write(
        &p,
        format!(r#"<mxfile><diagram id="d" name="Locked">{body}</diagram></mxfile>"#),
    )
    .unwrap();
    let err = import::import_detailed(&p).unwrap_err();
    assert!(err.to_lowercase().contains("encrypted"), "{err}");
    assert!(!err.contains('\u{2014}') && !err.contains('\u{2013}'));
}

#[test]
fn compressed_payloads_from_real_drawio_files_import() {
    // draw.io percent-encodes then raw-deflates a diagram body and base64s it.
    let inner = r#"<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="Packed" vertex="1" parent="1"><mxGeometry x="5" y="6" width="70" height="30" as="geometry"/></mxCell></root></mxGraphModel>"#;
    let mut encoded = String::new();
    for b in inner.bytes() {
        if b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    let compressed = miniz_oxide::deflate::compress_to_vec(encoded.as_bytes(), 6);
    let xml = format!(
        r#"<mxfile><diagram id="d" name="Packed">{}</diagram></mxfile>"#,
        sketch_rs::assets::base64(&compressed)
    );
    let (pages, _) = drawio::import_str(&xml).unwrap();
    assert_eq!(pages[0].name, "Packed");
    let n = pages[0].doc.nodes.values().next().unwrap();
    assert_eq!(n.label, "Packed");
}

#[test]
fn adaptive_colors_round_trip_through_drawio() {
    let id = NodeId(1);
    let mut n = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 80.0));
    n.label = "Adaptive".into();
    n.style.fills = vec![sketch_rs::model::paint::Paint::solid(Color::rgb(
        0xda, 0xe8, 0xfc,
    ))];
    n.style.stroke = Some(Color::rgb(0x6c, 0x8e, 0xbf));
    n.label_color = Some(Color::rgb(0x1c, 0x1c, 0x1e));
    let mut doc = Document {
        name: "Adaptive colors".into(),
        ..Document::default()
    };
    doc.nodes.insert(id, n);
    doc.root.push(id);

    let mut value = serde_json::to_value(&doc).unwrap();
    value["settings"]["adaptive_colors"] = json!("auto");
    value["settings"]["background_color_dark"] = json!("#101010");
    value["nodes"]["1"]["style"]["fills"][0]["dark"] = json!("#123456");
    value["nodes"]["1"]["style"]["stroke_dark"] = json!("#ffcc00");
    value["nodes"]["1"]["label_color_dark"] = json!("#ffffff");
    let doc: Document = serde_json::from_value(value).unwrap();

    let items = vec![export::Item {
        name: doc.name.clone(),
        doc: &doc,
        roots: doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let dir = tmp("adaptive-colors");
    let out = dir.join("adaptive.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("adaptiveColors=\"auto\""), "{text}");
    assert!(text.contains("light-dark(#dae8fc,#123456)"), "{text}");
    assert!(text.contains("light-dark(#6c8ebf,#ffcc00)"), "{text}");
    assert!(text.contains("light-dark(#1c1c1e,#ffffff)"), "{text}");

    let again = import::import_detailed(&out).unwrap();
    let back = &again.pages[0].doc;
    let settings = serde_json::to_value(&back.settings).unwrap();
    assert_eq!(settings["adaptive_colors"], json!("auto"));
    assert_eq!(settings["background_color_dark"], json!("#101010"));
    let node = back.get(id).expect("shape survives");
    assert_eq!(node.style.stroke_dark, Some(Color::rgb(0xff, 0xcc, 0x00)));
    assert_eq!(node.label_color_dark, Some(Color::WHITE));
    let fills = serde_json::to_value(&node.style.fills).unwrap();
    assert_eq!(fills[0]["dark"], json!("#123456"));
}

#[test]
fn scheme_name_round_trips_through_drawio() {
    let mut ed = Editor::default();
    let id = ed.add_node(
        Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 80.0)),
        None,
    );
    let scheme = sketch_rs::model::scheme::SCHEMES
        .iter()
        .find(|s| s.name == "Teal")
        .unwrap();
    let report = ed.apply_scheme_scoped(scheme, sketch_rs::model::SchemeScope::Page);
    assert_eq!(report.changed, 1);
    assert_eq!(ed.doc.settings.scheme.as_deref(), Some("Teal"));
    assert!(ed.doc.get(id).unwrap().style.stroke == Some(scheme.line));

    let items = vec![export::Item {
        name: ed.doc.name.clone(),
        doc: &ed.doc,
        roots: ed.doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let out = tmp("scheme").join("scheme.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("sketchScheme=\"Teal\""), "{text}");

    let again = import::import_detailed(&out).unwrap();
    assert_eq!(again.pages[0].doc.settings.scheme.as_deref(), Some("Teal"));
}
