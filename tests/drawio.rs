//! draw.io import, export and the project/page integration around them.

mod common;

use drawio_rs::editor::Editor;
use drawio_rs::geom::Rect;
use drawio_rs::io::drawio;
use drawio_rs::io::export::{self, Format, Scope, Spec};
use drawio_rs::io::import;
use drawio_rs::model::{
    Color, DefaultStyle, Document, Head, Node, NodeId, NodeKind, Style, TextStyle,
};
use serde_json::json;
use std::path::{Path, PathBuf};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("drawio-rs-drawio-{tag}-{:016x}", fastrand::u64(..)));
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
    let body = drawio_rs::assets::base64(b"Salted__not-a-real-drawio-stream");
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
        drawio_rs::assets::base64(&compressed)
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
    n.style.fills = vec![drawio_rs::model::paint::Paint::solid(Color::rgb(
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
fn life_drawio_keeps_its_explicit_shape_colors() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("Life.drawio");
    let xml = std::fs::read_to_string(&source).unwrap();
    let (pages, _) = drawio::import_str(&xml).unwrap();
    let fills: Vec<Color> = pages
        .iter()
        .flat_map(|page| page.doc.nodes.values())
        .filter_map(|n| n.style.fill())
        .collect();
    let strokes: Vec<Color> = pages
        .iter()
        .flat_map(|page| page.doc.nodes.values())
        .filter_map(|n| n.style.stroke)
        .collect();
    assert!(
        fills.contains(&Color::rgb(0x60, 0xa9, 0x17)),
        "green fill was lost: {fills:?}"
    );
    assert!(
        fills.contains(&Color::rgb(0xd8, 0x00, 0x73)),
        "magenta fill was lost: {fills:?}"
    );
    assert!(
        strokes.contains(&Color::rgb(0x2d, 0x76, 0x00)),
        "green stroke was lost: {strokes:?}"
    );
    assert!(
        strokes.contains(&Color::rgb(0xa5, 0x00, 0x40)),
        "magenta stroke was lost: {strokes:?}"
    );
}

#[test]
fn exporting_a_gradient_marks_it_for_drawio() {
    let id = NodeId(1);
    let mut n = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 80.0));
    n.label = "Gradient".into();
    n.style.fills = vec![drawio_rs::model::paint::Paint::linear(
        Color::rgb(1, 2, 3),
        Color::rgb(4, 5, 6),
    )];
    n.style.stroke = Some(Color::BLACK);
    let doc = Document {
        name: "Gradient export".into(),
        nodes: [(id, n)].into_iter().collect(),
        root: vec![id],
        ..Document::default()
    };
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
    let out = tmp("gradient-export").join("gradient.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("gradient=1;"), "{text}");
    assert!(text.contains("gradientColor=#040506"), "{text}");

    let again = import::import_detailed(&out).unwrap();
    let back = again.pages[0].doc.get(id).expect("gradient survives");
    assert!(
        matches!(
            back.style.fills.first(),
            Some(drawio_rs::model::paint::Paint::Linear { .. })
        ),
        "{:?}",
        back.style.fills
    );
}

#[test]
fn scheme_name_round_trips_through_drawio() {
    let mut ed = Editor::default();
    let id = ed.add_node(
        Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 80.0)),
        None,
    );
    let scheme = drawio_rs::model::scheme::SCHEMES
        .iter()
        .find(|s| s.name == "Teal")
        .unwrap();
    let report = ed.apply_scheme_scoped(scheme, drawio_rs::model::SchemeScope::Page);
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

#[test]
fn bpmn_drawio_shapes_import_as_native_shapes() {
    let cells = [
        (
            "start",
            "shape=mxgraph.bpmn.event;outline=standard;symbol=general;html=1;",
            "Start",
        ),
        (
            "wait",
            "shape=mxgraph.bpmn.event;outline=catching;symbol=message;html=1;",
            "Wait",
        ),
        (
            "done",
            "shape=mxgraph.bpmn.event;outline=end;symbol=terminate2;html=1;",
            "Done",
        ),
        (
            "work",
            "shape=mxgraph.bpmn.task2;html=1;whiteSpace=wrap;container=1;expand=0;collapsible=0;",
            "Work",
        ),
        (
            "branch",
            "shape=mxgraph.bpmn.gateway2;outline=none;symbol=none;gwType=exclusive;html=1;",
            "Branch",
        ),
    ];
    let mut body = String::new();
    for (id, style, label) in cells {
        body.push_str(&format!(
            r#"<mxCell id="{id}" value="{label}" style="{style}" vertex="1" parent="1"><mxGeometry x="0" y="0" width="50" height="50" as="geometry"/></mxCell>"#
        ));
    }
    let xml = format!(
        r#"<mxfile><diagram id="d" name="BPMN"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{body}</root></mxGraphModel></diagram></mxfile>"#
    );
    let (pages, _) = drawio::import_str(&xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Start Event",
        "Intermediate Event",
        "End Event",
        "Task",
        "Gateway",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_bpmn_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "bpmn-start", "Start"),
        (2, "bpmn-intermediate", "Wait"),
        (3, "bpmn-end", "Done"),
        (4, "bpmn-task", "Work"),
        (5, "bpmn-gateway", "Branch"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry BPMN shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "BPMN export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("bpmn-export").join("bpmn.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=mxgraph.bpmn.event;outline=standard",
        "shape=mxgraph.bpmn.event;outline=catching",
        "shape=mxgraph.bpmn.event;outline=end",
        "shape=mxgraph.bpmn.task2",
        "shape=mxgraph.bpmn.gateway2",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Start Event",
        "Intermediate Event",
        "End Event",
        "Task",
        "Gateway",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn c4_drawio_shapes_import_as_native_shapes() {
    let cells = [
        (
            "person",
            "Person",
            "Ada",
            "shape=mxgraph.c4.person2;html=1;",
        ),
        ("system", "Software System", "Orders", "rounded=1;html=1;"),
        ("container", "Container", "API", "rounded=1;html=1;"),
        ("component", "Component", "Cart", "rounded=1;html=1;"),
        (
            "boundary",
            "SystemScopeBoundary",
            "Scope",
            "rounded=1;dashed=1;html=1;",
        ),
    ];
    let mut body = String::new();
    for (id, c4_type, label, style) in cells {
        body.push_str(&format!(
            r#"<UserObject id="{id}" label="{label}" c4Type="{c4_type}" placeholders="1"><mxCell id="cell-{id}" style="{style}" vertex="1" parent="1"><mxGeometry x="0" y="0" width="100" height="60" as="geometry"/></mxCell></UserObject>"#
        ));
    }
    let xml = format!(
        r#"<mxfile><diagram id="d" name="C4"><mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>{body}</root></mxGraphModel></diagram></mxfile>"#
    );
    let (pages, _) = drawio::import_str(&xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "C4 Person",
        "C4 Software System",
        "C4 Container",
        "C4 Component",
        "C4 Boundary",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_c4_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "c4-person", "Person"),
        (2, "c4-system", "System"),
        (3, "c4-container", "Container"),
        (4, "c4-component", "Component"),
        (5, "c4-boundary", "Boundary"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry C4 shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "C4 export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("c4-export").join("c4.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=mxgraph.c4.person2",
        "c4Type=\"Person\"",
        "c4Type=\"Software System\"",
        "c4Type=\"Container\"",
        "c4Type=\"Component\"",
        "c4Type=\"SystemScopeBoundary\"",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "C4 Person",
        "C4 Software System",
        "C4 Container",
        "C4 Component",
        "C4 Boundary",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn sysml_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="SysML"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="block" vertex="1" parent="1" style="fontStyle=1;align=center;verticalAlign=middle;childLayout=stackLayout;horizontal=1;horizontalStack=0;resizeParent=1;resizeParentMax=0;resizeLast=0;marginBottom=0;">
          <mxGeometry x="0" y="0" width="180" height="100" as="geometry"/>
          <mxCell id="block-stereotype" value="&lt;&lt;block&gt;&gt;" vertex="1" connectable="0" parent="block" style="html=1;align=center;strokeColor=none;fillColor=none;">
            <mxGeometry x="0" y="0" width="140" height="20" as="geometry"/>
          </mxCell>
          <mxCell id="block-name" value="Block1" vertex="1" connectable="0" parent="block" style="html=1;align=center;strokeColor=none;fillColor=none;fontStyle=1;">
            <mxGeometry x="0" y="20" width="140" height="14" as="geometry"/>
          </mxCell>
        </mxCell>
        <mxCell id="requirement" vertex="1" parent="1" style="fontStyle=1;align=center;verticalAlign=middle;childLayout=stackLayout;horizontal=1;horizontalStack=0;resizeParent=1;resizeParentMax=0;resizeLast=0;marginBottom=0;">
          <mxGeometry x="220" y="0" width="190" height="90" as="geometry"/>
          <mxCell id="requirement-stereotype" value="&lt;&lt;requirement&gt;&gt;" vertex="1" connectable="0" parent="requirement" style="html=1;align=center;strokeColor=none;fillColor=none;">
            <mxGeometry x="0" y="0" width="190" height="16" as="geometry"/>
          </mxCell>
        </mxCell>
        <mxCell id="port" value="Transmission" vertex="1" parent="1" style="html=1;shape=mxgraph.sysml.port1;fontStyle=1;whiteSpace=wrap;align=center;points=[[0,0.5,0],[1,0.5,0]];">
          <mxGeometry x="0" y="140" width="160" height="60" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["SysML Block", "SysML Requirement", "SysML Port"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_sysml_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "sysml-block", "Block"),
        (2, "sysml-requirement", "Requirement"),
        (3, "sysml-port", "Port"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry SysML shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "SysML export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("sysml-export").join("sysml.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "childLayout=stackLayout",
        "&lt;&lt;block&gt;&gt;",
        "&lt;&lt;requirement&gt;&gt;",
        "shape=mxgraph.sysml.port1",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["SysML Block", "SysML Requirement", "SysML Port"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn flowchart_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="Flowchart"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="doc" value="Doc" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.document2;">
          <mxGeometry x="0" y="0" width="160" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="manual-input" value="Input" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.manual_input;">
          <mxGeometry x="180" y="0" width="160" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="manual-operation" value="Operation" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.manual_operation;">
          <mxGeometry x="360" y="0" width="160" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="on-page" value="Reference" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.on-page_reference;perimeter=ellipsePerimeter;">
          <mxGeometry x="540" y="0" width="60" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="sequential" value="Data" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.sequential_data;">
          <mxGeometry x="0" y="120" width="160" height="80" as="geometry"/>
        </mxCell>
        <mxCell id="stored" value="Stored" vertex="1" parent="1" style="html=1;whiteSpace=wrap;shape=mxgraph.flowchart.stored_data;">
          <mxGeometry x="180" y="120" width="160" height="80" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Document",
        "Manual Input",
        "Manual Operation",
        "On-Page Connector",
        "Data",
        "Database",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_flowchart_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "document", "Doc"),
        (2, "manual-input", "Input"),
        (3, "manual-operation", "Operation"),
        (4, "on-page-connector", "Reference"),
        (5, "data", "Data"),
        (6, "database", "Stored"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry flowchart shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "Flowchart export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("flowchart-export").join("flowchart.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=document;",
        "shape=manualInput;",
        "shape=trapezoid;",
        "shape=mxgraph.flowchart.on-page_reference;",
        "shape=parallelogram;",
        "shape=cylinder3;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Document",
        "Manual Input",
        "Manual Operation",
        "On-Page Connector",
        "Data",
        "Database",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn er_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="ER"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="weak" value="Weak" vertex="1" parent="1" style="shape=ext;double=1;margin=3;whiteSpace=wrap;html=1;align=center;">
          <mxGeometry x="0" y="0" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="key" value="Key" vertex="1" parent="1" style="ellipse;whiteSpace=wrap;html=1;align=center;fontStyle=4;">
          <mxGeometry x="120" y="0" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="weak-key" value="Weak Key" vertex="1" parent="1" style="ellipse;whiteSpace=wrap;html=1;align=center;fontStyle=20;">
          <mxGeometry x="240" y="0" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="derived" value="Derived" vertex="1" parent="1" style="ellipse;whiteSpace=wrap;html=1;align=center;dashed=1;">
          <mxGeometry x="0" y="80" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="identifying" value="Identifies" vertex="1" parent="1" style="shape=rhombus;double=1;perimeter=rhombusPerimeter;whiteSpace=wrap;html=1;align=center;">
          <mxGeometry x="120" y="80" width="120" height="60" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Weak Entity",
        "Key Attribute",
        "Derived Attribute",
        "Relationship",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_er_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "weak-entity", "Weak"),
        (2, "key-attribute", "Key"),
        (3, "derived-attribute", "Derived"),
        (4, "relationship", "Identifies"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry ER shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "ER export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("er-export").join("er.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=ext;double=1;",
        "fontStyle=4;",
        "dashed=1;",
        "rhombus;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in [
        "Weak Entity",
        "Key Attribute",
        "Derived Attribute",
        "Relationship",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn uml_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="UML"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="note" value="Comment" vertex="1" parent="1" style="shape=note2;boundedLbl=1;whiteSpace=wrap;html=1;size=25;verticalAlign=top;align=center;">
          <mxGeometry x="0" y="0" width="120" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="package" value="Package" vertex="1" parent="1" style="shape=folder;fontStyle=1;tabWidth=80;tabHeight=30;tabPosition=left;html=1;boundedLbl=1;container=1;collapsible=0;whiteSpace=wrap;">
          <mxGeometry x="160" y="0" width="150" height="80" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Note", "Package"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_uml_shapes_export_with_drawio_styles() {
    let defs = [(1, "note", "Comment"), (2, "package", "Package")];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry UML shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "UML export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("uml-export").join("uml.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in ["shape=note2;", "shape=folder;", "tabWidth=80;"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Note", "Package"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn floorplan_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="Floorplan"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="door" value="Left door" vertex="1" parent="1" style="html=1;shape=mxgraph.floorplan.doorLeft;aspect=fixed;">
          <mxGeometry x="0" y="0" width="80" height="85" as="geometry"/>
        </mxCell>
        <mxCell id="window" value="Sliding window" vertex="1" parent="1" style="html=1;shape=mxgraph.floorplan.windowGlider;dx=0.25;">
          <mxGeometry x="120" y="0" width="100" height="10" as="geometry"/>
        </mxCell>
        <mxCell id="stairs" value="Landing" vertex="1" parent="1" style="html=1;shape=mxgraph.floorplan.stairsRest;">
          <mxGeometry x="0" y="120" width="300" height="200" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Door", "Window", "Stairs"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_floorplan_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "door", "Door"),
        (2, "window", "Window"),
        (3, "stairs", "Stairs"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry floorplan shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "Floorplan export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("floorplan-export").join("floorplan.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=mxgraph.floorplan.doorLeft;",
        "shape=mxgraph.floorplan.window;",
        "shape=mxgraph.floorplan.stairs;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Door", "Window", "Stairs"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn network_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="Network"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="server" value="Mail relay" vertex="1" parent="1" style="html=1;shape=mxgraph.networks.mail_server;">
          <mxGeometry x="0" y="0" width="105" height="105" as="geometry"/>
        </mxCell>
        <mxCell id="host" value="Workstation" vertex="1" parent="1" style="html=1;shape=mxgraph.networks.pc;">
          <mxGeometry x="140" y="0" width="100" height="70" as="geometry"/>
        </mxCell>
        <mxCell id="storage" value="Archive" vertex="1" parent="1" style="html=1;shape=mxgraph.networks.storage;">
          <mxGeometry x="280" y="0" width="100" height="100" as="geometry"/>
        </mxCell>
        <mxCell id="firewall" value="Edge filter" vertex="1" parent="1" style="html=1;shape=mxgraph.networks.firewall;">
          <mxGeometry x="0" y="140" width="100" height="100" as="geometry"/>
        </mxCell>
        <mxCell id="cloud" value="Private cloud" vertex="1" parent="1" style="html=1;shape=mxgraph.networks.cloud;">
          <mxGeometry x="140" y="140" width="90" height="50" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Server", "Host", "Database", "Firewall", "Cloud"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_network_shapes_export_with_drawio_styles() {
    let defs = [
        (1, "server", "Mail relay"),
        (2, "host", "Workstation"),
        (3, "database", "Archive"),
        (4, "firewall", "Edge filter"),
        (5, "cloud", "Private cloud"),
    ];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry network shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "Network export".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("network-export").join("network.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=mxgraph.networks.server;",
        "shape=mxgraph.networks.pc;",
        "shape=mxgraph.networks.firewall;",
        "shape=mxgraph.networks.cloud;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Server", "Host", "Firewall", "Cloud"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn person_drawio_shapes_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="Org"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="mermaid" value="Project lead" vertex="1" parent="1" style="shape=mermaidPerson;perimeter=mermaidPersonPerimeter;whiteSpace=wrap;html=1;">
          <mxGeometry x="0" y="0" width="100" height="100" as="geometry"/>
        </mxCell>
        <mxCell id="person" value="Reviewer" vertex="1" parent="1" style="shape=person;whiteSpace=wrap;html=1;">
          <mxGeometry x="140" y="0" width="100" height="100" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        names.iter().filter(|name| **name == "Person").count(),
        2,
        "{names:?}"
    );
}

#[test]
fn native_person_shape_exports_with_drawio_style() {
    let def = drawio_rs::registry::by_id("person").expect("registry Person shape");
    let mut node =
        drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
    node.id = NodeId(1);
    node.label = "Project lead".into();
    let doc = Document {
        name: "Person export".into(),
        nodes: [(node.id, node)].into_iter().collect(),
        root: vec![NodeId(1)],
        ..Document::default()
    };
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
    let out = tmp("person-export").join("person.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in ["shape=mermaidPerson;", "perimeter=mermaidPersonPerimeter;"] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    assert!(names.contains(&"Person"), "{names:?}");
}

#[test]
fn uml_actor_and_class_import_as_native_shapes() {
    let xml = r#"<mxfile><diagram id="d" name="UML"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="actor" value="Customer" vertex="1" parent="1" style="shape=umlActor;verticalLabelPosition=bottom;verticalAlign=top;html=1;">
          <mxGeometry x="0" y="0" width="30" height="60" as="geometry"/>
        </mxCell>
        <mxCell id="class" value="Account" vertex="1" parent="1" style="swimlane;fontStyle=0;align=center;verticalAlign=top;childLayout=stackLayout;horizontal=1;startSize=55;horizontalStack=0;resizeParent=1;resizeParentMax=0;resizeLast=0;collapsible=0;marginBottom=0;html=1;whiteSpace=wrap;">
          <mxGeometry x="60" y="0" width="140" height="183" as="geometry"/>
          <mxCell id="attributes" value="attributes" vertex="1" connectable="0" parent="class" style="text;html=1;strokeColor=none;fillColor=none;">
            <mxGeometry x="0" y="0" width="140" height="20" as="geometry"/>
          </mxCell>
          <mxCell id="operations" value="operations" vertex="1" connectable="0" parent="class" style="text;html=1;strokeColor=none;fillColor=none;">
            <mxGeometry x="0" y="93" width="140" height="20" as="geometry"/>
          </mxCell>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Actor", "Class"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn native_uml_actor_and_class_export_with_drawio_styles() {
    let defs = [(1, "actor", "Customer"), (2, "class", "Account")];
    let mut nodes = Vec::new();
    let mut root = Vec::new();
    for (id, def_id, label) in defs {
        let def = drawio_rs::registry::by_id(def_id).expect("registry UML shape");
        let mut node =
            drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
        node.id = NodeId(id);
        node.label = label.into();
        root.push(node.id);
        nodes.push((node.id, node));
    }
    let doc = Document {
        name: "UML actor and class".into(),
        nodes: nodes.into_iter().collect(),
        root,
        ..Document::default()
    };
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
    let out = tmp("uml-actor-class").join("uml.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=umlActor;",
        "verticalLabelPosition=bottom;",
        "childLayout=stackLayout;",
        "startSize=55;",
        "value=\"attributes\"",
        "value=\"operations\"",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, _) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    for expected in ["Actor", "Class"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn pie_drawio_shape_imports_as_native_slice() {
    let xml = r#"<mxfile><diagram id="d" name="Charts"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="pie" value="Quarter share" vertex="1" parent="1" style="html=1;shape=mxgraph.basic.pie;startAngle=0.2;endAngle=0.9;">
          <mxGeometry x="0" y="0" width="120" height="120" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, report) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    assert!(names.contains(&"Pie Slice"), "{names:?}");
    assert!(
        report
            .approximated
            .iter()
            .any(|d| d.reason.contains("pie angles")),
        "{report:?}"
    );
}

#[test]
fn native_pie_slice_exports_with_drawio_style() {
    let def = drawio_rs::registry::by_id("pie-slice").expect("registry Pie Slice shape");
    let mut node =
        drawio_rs::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), None);
    node.id = NodeId(1);
    node.label = "Quarter share".into();
    let doc = Document {
        name: "Pie export".into(),
        nodes: [(node.id, node)].into_iter().collect(),
        root: vec![NodeId(1)],
        ..Document::default()
    };
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
    let out = tmp("pie-export").join("pie.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "shape=mxgraph.basic.pie;",
        "startAngle=0;",
        "endAngle=0.25;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }

    let (pages, report) = drawio::import_str(&text).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    assert!(names.contains(&"Pie Slice"), "{names:?}");
    assert!(
        !report
            .approximated
            .iter()
            .any(|d| d.reason.contains("pie angles")),
        "{report:?}"
    );
}

#[test]
fn mermaid_use_case_actor_drawio_shapes_import_as_native_actors() {
    let xml = r#"<mxfile><diagram id="d" name="Use cases"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="normal" value="Normal user" vertex="1" parent="1" style="shape=mermaidUsecaseActor;actorType=normal;verticalLabelPosition=bottom;verticalAlign=top;html=1;outlineConnect=0;">
          <mxGeometry x="0" y="0" width="56" height="72" as="geometry"/>
        </mxCell>
        <mxCell id="hollow" value="Guest" vertex="1" parent="1" style="shape=mermaidUsecaseActor;actorType=hollow;verticalLabelPosition=bottom;verticalAlign=top;html=1;outlineConnect=0;">
          <mxGeometry x="80" y="0" width="56" height="72" as="geometry"/>
        </mxCell>
        <mxCell id="awesome" value="Admin" vertex="1" parent="1" style="shape=mermaidUsecaseActor;actorType=awesome;verticalLabelPosition=bottom;verticalAlign=top;html=1;outlineConnect=0;">
          <mxGeometry x="160" y="0" width="56" height="72" as="geometry"/>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, report) = drawio::import_str(xml).unwrap();
    let names: Vec<&str> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.name.as_str())
        .collect();
    assert_eq!(
        names.iter().filter(|name| **name == "Actor").count(),
        3,
        "{names:?}"
    );
    assert_eq!(
        report
            .approximated
            .iter()
            .filter(|d| d.reason.contains("use case actor type"))
            .count(),
        2,
        "{report:?}"
    );
}

#[test]
fn er_cardinality_arrows_survive_drawio_round_trip() {
    let xml = r#"<mxfile><diagram id="d" name="ER"><mxGraphModel><root>
        <mxCell id="0"/><mxCell id="1" parent="0"/>
        <mxCell id="a" value="Customer" vertex="1" parent="1" style="whiteSpace=wrap;html=1;">
          <mxGeometry x="0" y="0" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="b" value="Order" vertex="1" parent="1" style="whiteSpace=wrap;html=1;">
          <mxGeometry x="200" y="0" width="100" height="40" as="geometry"/>
        </mxCell>
        <mxCell id="e" value="places" edge="1" parent="1" source="a" target="b" style="edgeStyle=entityRelationEdgeStyle;fontSize=12;html=1;startArrow=ERmandOne;endArrow=ERoneToMany;">
          <mxGeometry relative="1" as="geometry">
            <mxPoint x="100" y="20" as="sourcePoint"/>
            <mxPoint x="200" y="20" as="targetPoint"/>
          </mxGeometry>
        </mxCell>
      </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, report) = drawio::import_str(xml).unwrap();
    let edge = pages[0]
        .doc
        .nodes
        .values()
        .find(|n| n.label == "places")
        .expect("ER edge");
    assert!(edge.is_connector());
    assert_eq!(edge.metadata["drawioEdgeStyle"], "entityRelationEdgeStyle");
    assert_eq!(edge.metadata["drawioStartArrow"], "ERmandOne");
    assert_eq!(edge.metadata["drawioEndArrow"], "ERoneToMany");
    assert!(
        matches!(
            edge.kind,
            NodeKind::Connector {
                start_head: Head::Bar,
                end_head: Head::Arrow,
                ..
            }
        ),
        "{:?}",
        edge.kind
    );
    assert!(
        report
            .approximated
            .iter()
            .any(|d| d.reason.contains("ER cardinality")),
        "{report:?}"
    );

    let doc = &pages[0].doc;
    let items = vec![export::Item {
        name: doc.name.clone(),
        doc,
        roots: doc.layered_root(),
    }];
    let spec = Spec {
        format: Format::Drawio,
        scope: Scope::AllPages,
        ..Spec::default()
    };
    let out = tmp("er-cardinality").join("er.drawio");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    for needle in [
        "edgeStyle=entityRelationEdgeStyle;",
        "startArrow=ERmandOne;",
        "endArrow=ERoneToMany;",
    ] {
        assert!(text.contains(needle), "{needle} missing from {text}");
    }
    assert!(!text.contains("edgeStyle=orthogonalEdgeStyle;"), "{text}");

    let (again, _) = drawio::import_str(&text).unwrap();
    let reimported = again[0]
        .doc
        .nodes
        .values()
        .find(|n| n.label == "places")
        .expect("reimported ER edge");
    assert_eq!(
        reimported.metadata["drawioStartArrow"], "ERmandOne",
        "{text}"
    );
    assert_eq!(
        reimported.metadata["drawioEndArrow"], "ERoneToMany",
        "{text}"
    );
}
