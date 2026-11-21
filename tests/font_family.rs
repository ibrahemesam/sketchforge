//! TXT-06: font family picker across the MCP surface, draw.io and native files.

use drawio_rs::geom::Rect;
use drawio_rs::io::drawio;
use drawio_rs::mcp::{Out, exec};
use drawio_rs::model::{Document, Node, NodeId, NodeKind, TextAlign};
use drawio_rs::project::Store;
use serde_json::{Value, json};
use std::time::{Duration, Instant};

fn host() -> exec::SessionHost {
    let store = Store::open(
        std::env::temp_dir().join(format!("sketch-font-family-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Font family test").unwrap();
    h
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match exec::call(h, name, &args) {
        Ok(outs) => match outs.into_iter().next() {
            Some(Out::Text(t)) => serde_json::from_str(&t).unwrap_or(Value::String(t)),
            Some(Out::Image { .. }) => json!("image"),
            None => Value::Null,
        },
        Err(e) => panic!("{name} failed: {e}"),
    }
}

/// The family list waits on a background font scan; poll until it lands.
fn families_after_scan() -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let families = drawio_rs::text::font_families();
        if drawio_rs::text::library_epoch() > 0 || Instant::now() >= deadline {
            return families;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn mcp_font_info_reports_installed_families() {
    let families = families_after_scan();
    let family = families
        .iter()
        .find(|f| !f.eq_ignore_ascii_case("Inter"))
        .expect("this machine has a non-Inter system family");
    let mut h = host();
    let info = call(
        &mut h,
        "get_font_family_info",
        json!({"familyNames": [family, "Inter", "No Such Font 4E7"]}),
    );
    let rows = info["families"].as_array().expect("families array");
    assert_eq!(rows.len(), 3);

    assert_eq!(rows[0]["family"], *family);
    assert_eq!(
        rows[0]["available"], true,
        "installed family: {:?}",
        rows[0]
    );
    assert!(
        rows[0]["styles"].as_array().is_some_and(|s| !s.is_empty()),
        "installed family should report styles: {:?}",
        rows[0]
    );
    assert_eq!(rows[0]["note"], "", "{:?}", rows[0]);

    assert_eq!(rows[1]["available"], true);
    assert_eq!(rows[1]["styles"].as_array().unwrap().len(), 4);

    assert_eq!(rows[2]["available"], false, "{:?}", rows[2]);
    assert_eq!(rows[2]["styles"].as_array().unwrap().len(), 0);
    assert!(
        rows[2]["note"]
            .as_str()
            .is_some_and(|n| n.contains("not available"))
    );
}

#[test]
fn mcp_basic_info_lists_installed_families() {
    let families = families_after_scan();
    let mut h = host();
    let info = call(&mut h, "get_basic_info", json!({}));
    let listed = info["fontFamilies"].as_array().expect("fontFamilies array");
    let names: Vec<&str> = listed.iter().filter_map(Value::as_str).collect();
    assert!(
        names.iter().any(|n| n.eq_ignore_ascii_case("Inter")),
        "{names:?}"
    );
    assert!(
        families.iter().any(|f| names.contains(&f.as_str())),
        "basic info should list a system family: {names:?}"
    );
}

#[test]
fn mcp_font_family_is_written_and_reported() {
    let families = families_after_scan();
    let family = families
        .iter()
        .find(|f| !f.eq_ignore_ascii_case("Inter"))
        .expect("this machine has a non-Inter system family");
    let mut h = host();
    let artboard = call(
        &mut h,
        "create_artboard",
        json!({"name": "Type", "styles": {"width": 400, "height": 200}}),
    );
    let html =
        format!("<p style=\"font-family: {family}; font-size: 24px; color: #111827\">Hello</p>");
    call(
        &mut h,
        "write_html",
        json!({"targetNodeId": artboard["id"], "mode": "insert-children", "html": html}),
    );
    let found = call(&mut h, "find_nodes", json!({"textValue": "Hello"}));
    let id = found["results"][0]["id"].clone();
    let key = id.as_str().unwrap();

    let styles = call(
        &mut h,
        "get_computed_styles",
        json!({"nodeIds": [id.clone()]}),
    );
    assert_eq!(styles[key]["fontFamily"], *family, "{styles}");

    call(
        &mut h,
        "update_styles",
        json!({"updates": [{"nodeIds": [id.clone()], "styles": {"fontFamily": "Inter"}}]}),
    );
    let styles = call(&mut h, "get_computed_styles", json!({"nodeIds": [id]}));
    assert_eq!(styles[key]["fontFamily"], "Inter", "{styles}");
}

fn text_node(id: NodeId, family: &str) -> Node {
    let mut n = Node::new(
        id,
        NodeKind::Text {
            text: "Hello".into(),
            font_size: 18.0,
            auto_width: true,
            align: TextAlign::Left,
        },
        Rect::new(0.0, 0.0, 120.0, 30.0),
    );
    n.text_style.family = Some(family.into());
    n
}

#[test]
fn drawio_round_trips_font_family_on_text_and_labels() {
    const FAMILY: &str = "DejaVu Sans";
    let mut doc = Document::default();
    let mut label = Node::new(NodeId(2), NodeKind::Rect, Rect::new(0.0, 50.0, 120.0, 40.0));
    label.label = "Label".into();
    label.text_style.family = Some(FAMILY.into());
    for n in [text_node(NodeId(1), FAMILY), label] {
        doc.nodes.insert(n.id, n);
    }
    doc.root = vec![NodeId(1), NodeId(2)];

    let xml = drawio::export(&[("Page", &doc)]);
    assert!(
        xml.contains(&format!("fontFamily={FAMILY};")),
        "font family missing from draw.io export: {xml}"
    );
    let (pages, _) = drawio::import_str(&xml).expect("reimport draw.io export");
    let families: Vec<Option<&str>> = pages[0]
        .doc
        .nodes
        .values()
        .map(|n| n.text_style.family.as_deref())
        .collect();
    assert_eq!(families.len(), 2, "{families:?}");
    assert!(
        families.iter().all(|f| *f == Some(FAMILY)),
        "font family lost on import: {families:?}"
    );
}

#[test]
fn font_family_survives_project_reload() {
    const FAMILY: &str = "DejaVu Sans";
    let dir = std::env::temp_dir().join(format!(
        "sketch-font-family-reload-{:016x}",
        fastrand::u64(..)
    ));
    let store = Store::open(dir).unwrap();
    let (project, mut page) = store.create("Type").unwrap();
    let node = text_node(NodeId(1), FAMILY);
    page.doc.nodes.insert(node.id, node);
    page.doc.root.push(NodeId(1));
    project.save_page(&page).unwrap();
    let back = project.load_page(&page.id).unwrap();
    let families: Vec<Option<&str>> = back
        .doc
        .nodes
        .values()
        .map(|n| n.text_style.family.as_deref())
        .collect();
    assert_eq!(families, vec![Some(FAMILY)], "{families:?}");
}
