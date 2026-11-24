//! DIA-12: custom links that show, hide, toggle or select layers.

use drawio_rs::editor::{Button, Editor, InputEvent, Modifiers};
use drawio_rs::geom::{Camera, Rect, v2};
use drawio_rs::io::drawio;
use drawio_rs::model::{Node, NodeId, NodeKind};
use serde_json::{Value, json};

fn action_link(action: &str, selector: &str, ids: &[String]) -> String {
    let ids = serde_json::to_string(ids).unwrap();
    format!("data:action/json,{{\"actions\":[{{\"{action}\":{{\"{selector}\":{ids}}}}}]}}")
}

fn rect(id: u64) -> Node {
    Node::new(
        NodeId(id),
        NodeKind::Rect,
        Rect::new((id - 1) as f64 * 300.0, 0.0, 80.0, 40.0),
    )
}

fn seeded() -> (Editor, drawio_rs::model::LayerId, NodeId, NodeId) {
    let mut e = Editor::default();
    let layer = e.layer_add();
    let button = e.add_node(rect(1), None);
    let target = e.add_node(rect(2), None);
    e.selection = vec![target];
    e.move_selection_to_layer(&[target], layer);
    e.selection.clear();
    (e, layer, button, target)
}

#[test]
fn custom_link_toggles_a_layer() {
    let (mut e, layer, button, target) = seeded();
    let link = action_link("toggle", "layers", &[layer.0.to_string()]);
    e.doc.nodes.get_mut(&button).unwrap().link = link.clone();

    assert!(e.doc.layer_by_id(layer).unwrap().visible);
    e.activate_link(&link);
    assert!(!e.doc.layer_by_id(layer).unwrap().visible);
    e.activate_link(&link);
    assert!(e.doc.layer_by_id(layer).unwrap().visible);
    assert!(e.doc.get(target).is_some());
}

#[test]
fn custom_link_hides_and_shows_a_layer() {
    let (mut e, layer, button, _) = seeded();
    let hide = action_link("hide", "layers", &[layer.0.to_string()]);
    let show = action_link("show", "layers", &[layer.0.to_string()]);

    e.activate_link(&hide);
    assert!(!e.doc.layer_by_id(layer).unwrap().visible, "{hide}");
    e.activate_link(&show);
    assert!(e.doc.layer_by_id(layer).unwrap().visible, "{show}");

    // The button itself stays outside the toggled layer.
    assert!(e.doc.get(button).is_some());
}

#[test]
fn custom_link_selects_a_layer() {
    let (mut e, layer, _, target) = seeded();
    let link = action_link("select", "layers", &[layer.0.to_string()]);

    e.activate_link(&link);
    assert_eq!(e.selection, vec![target], "{link}");
}

#[test]
fn custom_link_empty_tag_selector_shows_every_tagged_cell() {
    let mut e = Editor::default();
    let tagged = e.add_node(rect(1), None);
    let untagged = e.add_node(rect(2), None);
    e.doc.nodes.get_mut(&tagged).unwrap().tags = vec!["urgent".into()];
    e.doc.nodes.get_mut(&tagged).unwrap().visible = false;
    e.doc.nodes.get_mut(&untagged).unwrap().visible = false;
    let link = action_link("show", "tags", &[]);

    e.activate_link(&link);
    assert!(e.doc.get(tagged).unwrap().visible, "{link}");
    assert!(!e.doc.get(untagged).unwrap().visible, "{link}");
}

#[test]
fn command_click_on_linked_shape_runs_its_actions() {
    let (mut e, layer, button, _) = seeded();
    let link = action_link("hide", "layers", &[layer.0.to_string()]);
    e.doc.nodes.get_mut(&button).unwrap().link = link.clone();
    e.viewport = v2(1000.0, 800.0);
    e.camera = Camera {
        pan: v2(0.0, 0.0),
        zoom: 1.0,
    };

    e.handle(InputEvent::Modifiers(Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    }));
    e.handle(InputEvent::PointerDown {
        pos: v2(40.0, 20.0),
        button: Button::Primary,
        pressure: None,
        time_ms: 0.0,
    });
    e.handle(InputEvent::PointerUp {
        pos: v2(40.0, 20.0),
        button: Button::Primary,
        time_ms: 10.0,
    });
    e.handle(InputEvent::Modifiers(Modifiers::NONE));

    assert!(!e.doc.layer_by_id(layer).unwrap().visible, "{link}");
}

#[test]
fn unsupported_action_links_are_blocked() {
    let (mut e, _, _, _) = seeded();
    for link in [
        "data:action/json,{}",
        "data:action/json,{\"actions\":[]}",
        "data:action/json,{\"actions\":[{\"highlight\":{\"cells\":[\"1\"]}}]}",
        "data:action/json,{not json}",
    ] {
        e.activate_link(link);
        assert!(e.status.contains("Link blocked"), "{link}: {}", e.status);
    }
}

#[test]
fn action_link_layer_ids_survive_drawio_export_and_import() {
    let (e, layer, button, _) = seeded();
    let link = action_link("toggle", "layers", &[layer.0.to_string()]);
    assert_eq!(e.doc.get(button).unwrap().link, "");
    let mut e = e;
    e.doc.nodes.get_mut(&button).unwrap().link = link.clone();

    let xml = drawio::export(&[("Page", &e.doc)]);
    assert!(
        xml.contains("&quot;layers&quot;:[&quot;l2&quot;]"),
        "export did not rewrite layer ids: {xml}"
    );

    let (pages, report) = drawio::import_str(&xml).unwrap();
    assert!(report.layers >= 1);
    let reloaded = pages[0]
        .doc
        .nodes
        .values()
        .find(|n| n.link.starts_with("data:action/json,"))
        .expect("action link was dropped");
    let json = reloaded.link.strip_prefix("data:action/json,").unwrap();
    let v: Value = serde_json::from_str(json).unwrap();
    assert_eq!(v["actions"][0]["toggle"]["layers"][0], json!("2"), "{json}");
}
