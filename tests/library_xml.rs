//! CUS-05: draw.io custom shape library XML import and export.

mod common;

use sketch_rs::editor::Editor;
use sketch_rs::geom::v2;
use sketch_rs::io::{drawio, library_xml};
use sketch_rs::library::LibItem;
use sketch_rs::model::{Document, Node, NodeId, NodeKind};

const LIBRARY: &str = include_str!("fixtures/drawio/shape-library.xml");

#[test]
fn imports_mxlibrary_entries_into_library_items() {
    let items = library_xml::import_library(LIBRARY).expect("import library");
    assert_eq!(items.len(), 2, "{items:?}");
    let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["Decision", "Terminal"]);
    assert!(items.iter().all(|i| i.roots.len() == 1));
    let kinds: Vec<&str> = items.iter().map(|i| i.nodes[0].kind.type_name()).collect();
    assert_eq!(kinds, ["Polygon", "Ellipse"], "{kinds:?}");
}

#[test]
fn imported_items_insert_as_editable_shapes() {
    let items = library_xml::import_library(LIBRARY).expect("import library");
    let mut ed = Editor::default();
    let ids = ed.insert_library_item(&items[0], v2(100.0, 100.0));
    assert_eq!(ids.len(), 1);
    assert!(ed.doc.get(ids[0]).is_some());
    assert_eq!(ed.doc.get(ids[0]).unwrap().name, "Decision");
    ed.undo();
    assert!(ed.doc.get(ids[0]).is_none());
}

#[test]
fn export_library_writes_mxlibrary_json() {
    let mut doc = Document::default();
    let node = Node::new(
        NodeId(1),
        NodeKind::Rect,
        sketch_rs::geom::Rect::new(0.0, 0.0, 80.0, 50.0),
    );
    doc.nodes.insert(NodeId(1), node);
    doc.root.push(NodeId(1));
    let item = LibItem::from_selection(&doc, &[NodeId(1)], "Box").expect("library item");
    let xml = library_xml::export_library(&[item]);
    assert!(xml.starts_with("<mxlibrary>"), "{xml}");
    assert!(xml.contains("\"title\":\"Box\""), "{xml}");
    assert!(xml.contains("&lt;mxGraphModel "), "{xml}");
    // The exported entry is a real mxGraphModel, not just a label.
    let back = library_xml::import_library(&xml).expect("reimport library");
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].name, "Box");
}

#[test]
fn malformed_library_is_reported() {
    assert!(library_xml::import_library("<mxlibrary>not json</mxlibrary>").is_err());
    assert!(library_xml::import_library("<mxfile></mxfile>").is_err());
}

#[test]
fn drawio_export_still_writes_a_full_mxfile() {
    // Guard the adapter reuse: library export extracts a model from this output.
    let doc = Document::default();
    let xml = drawio::export(&[("Page", &doc)]);
    assert!(xml.contains("<mxGraphModel"), "{xml}");
    assert!(xml.contains("</mxGraphModel>"), "{xml}");
}

#[test]
fn library_tab_file_operations_round_trip() {
    use sketch_rs::ui::library::{LibraryUi, export_xml_file, import_xml_file};

    let mut lib = LibraryUi::default();
    let mut ed = Editor::default();
    let dir = std::env::temp_dir();
    let input = dir.join(format!("sketch-lib-in-{}.xml", std::process::id()));
    let output = dir.join(format!("sketch-lib-out-{}.xml", std::process::id()));
    std::fs::write(&input, LIBRARY).expect("write fixture");

    import_xml_file(&mut lib, &mut ed, &input);
    assert_eq!(lib.library.items.len(), 2, "{}", ed.status);
    assert!(lib.dirty);
    assert!(ed.status.contains("Imported 2 shape(s)"), "{}", ed.status);

    export_xml_file(&lib, &mut ed, &output);
    let xml = std::fs::read_to_string(&output).expect("read export");
    assert!(xml.starts_with("<mxlibrary>"), "{xml}");
    let back = library_xml::import_library(&xml).expect("reimport");
    assert_eq!(back.len(), 2);
    assert!(ed.status.contains("Exported 2 shape(s)"), "{}", ed.status);

    let _ = std::fs::remove_file(&input);
    let _ = std::fs::remove_file(&output);
}
