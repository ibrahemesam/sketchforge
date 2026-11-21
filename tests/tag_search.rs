//! DIA-05: find tagged nodes through MCP and select every match.

use drawio_rs::geom::Rect;
use drawio_rs::mcp::{Out, exec};
use drawio_rs::model::{Node, NodeId, NodeKind, TextAlign};
use drawio_rs::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(
        std::env::temp_dir().join(format!("sketch-tag-search-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Tag test").unwrap();
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

fn key(h: &exec::SessionHost, id: NodeId) -> String {
    exec::node_key(&h.ed, h.ed.active_page, id)
}

fn tagged(h: &mut exec::SessionHost, name: &str, tags: &[&str]) -> NodeId {
    let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 80.0, 40.0));
    n.name = name.into();
    n.tags = tags.iter().map(|t| t.to_string()).collect();
    h.ed.add_node(n, None)
}

fn tagged_text(h: &mut exec::SessionHost, name: &str, text: &str, tags: &[&str]) -> NodeId {
    let mut n = Node::new(
        NodeId(0),
        NodeKind::Text {
            text: text.into(),
            font_size: 18.0,
            auto_width: true,
            align: TextAlign::Left,
        },
        Rect::new(0.0, 0.0, 120.0, 30.0),
    );
    n.name = name.into();
    n.tags = tags.iter().map(|t| t.to_string()).collect();
    h.ed.add_node(n, None)
}

#[test]
fn mcp_find_nodes_matches_tags_case_insensitively_with_wildcards() {
    let mut h = host();
    let a = tagged(&mut h, "A", &["web", "external"]);
    let b = tagged(&mut h, "B", &["api"]);
    let c = tagged(&mut h, "C", &["Web"]);
    tagged(&mut h, "D", &[]);

    let found = call(&mut h, "find_nodes", json!({"tag": "web"}));
    assert_eq!(found["count"], json!(2), "{found}");
    let ids = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ids.contains(&key(&h, a).as_str()), "{ids:?}");
    assert!(ids.contains(&key(&h, c).as_str()), "{ids:?}");
    assert!(
        found["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["matched"].as_array().unwrap().iter().any(|m| {
                m["tag"]
                    .as_str()
                    .is_some_and(|tag| tag.eq_ignore_ascii_case("web"))
            })),
        "{found}"
    );

    let wildcard = call(&mut h, "find_nodes", json!({"tag": "*API*"}));
    assert_eq!(wildcard["count"], json!(1), "{wildcard}");
    assert_eq!(
        wildcard["results"][0]["id"],
        json!(key(&h, b)),
        "{wildcard}"
    );

    let prefix = call(&mut h, "find_nodes", json!({"tag": "we*"}));
    assert_eq!(prefix["count"], json!(2), "{prefix}");
}

#[test]
fn mcp_find_nodes_tag_and_text_are_combined() {
    let mut h = host();
    let yes = tagged_text(&mut h, "Yes", "Deploy to web", &["web"]);
    tagged_text(&mut h, "No text", "Something else", &["web"]);
    tagged_text(&mut h, "No tag", "Deploy to web", &[]);

    let found = call(
        &mut h,
        "find_nodes",
        json!({"tag": "web", "textValue": "Deploy to *"}),
    );
    assert_eq!(found["count"], json!(1), "{found}");
    assert_eq!(found["results"][0]["id"], json!(key(&h, yes)), "{found}");
    let matched = found["results"][0]["matched"].as_array().unwrap();
    assert!(matched.contains(&json!({"tag": "web"})), "{matched:?}");
    assert!(
        matched.contains(&json!({"textValue": "Deploy to web"})),
        "{matched:?}"
    );
}

#[test]
fn mcp_find_nodes_tag_search_respects_subtree_scope() {
    let mut h = host();
    let frame = tagged(&mut h, "Frame", &["web"]);
    // A frame is a container; give it a tagged child and leave a tagged node
    // outside the subtree so scoping has something to exclude.
    let child = h.ed.add_node(
        {
            let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(10.0, 10.0, 40.0, 20.0));
            n.name = "Child".into();
            n.tags = vec!["web".into()];
            n
        },
        Some(frame),
    );
    tagged(&mut h, "Outside", &["web"]);

    let frame_key = key(&h, frame);
    let found = call(
        &mut h,
        "find_nodes",
        json!({"nodeId": frame_key, "tag": "web"}),
    );
    assert_eq!(found["count"], json!(2), "{found}");
    let ids = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(ids.contains(&key(&h, frame).as_str()), "{ids:?}");
    assert!(ids.contains(&key(&h, child).as_str()), "{ids:?}");
}

#[test]
fn mcp_tag_search_results_select_all_matches() {
    let mut h = host();
    let a = tagged(&mut h, "A", &["review"]);
    let b = tagged(&mut h, "B", &["review", "urgent"]);
    tagged(&mut h, "C", &["done"]);

    let found = call(&mut h, "find_nodes", json!({"tag": "review"}));
    assert_eq!(found["count"], json!(2), "{found}");
    let ids = found["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();

    let selected = call(&mut h, "select_nodes", json!({"nodeIds": ids}));
    assert_eq!(selected["selected"], json!(2), "{selected}");
    assert_eq!(h.ed.selection, vec![a, b]);
}
