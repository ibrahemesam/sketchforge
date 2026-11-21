//! DIA-03: edit custom links through MCP with safe targets and one-step undo.

use drawio_rs::geom::Rect;
use drawio_rs::mcp::{Out, ToolResult, exec};
use drawio_rs::model::{Node, NodeId, NodeKind};
use drawio_rs::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(
        std::env::temp_dir().join(format!("sketch-link-edit-{:016x}", fastrand::u64(..))),
    )
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Link test").unwrap();
    h
}

fn raw(h: &mut exec::SessionHost, name: &str, args: Value) -> ToolResult {
    exec::call(h, name, &args)
}

fn call(h: &mut exec::SessionHost, name: &str, args: Value) -> Value {
    match raw(h, name, args) {
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

fn rect(h: &mut exec::SessionHost, name: &str) -> NodeId {
    let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 80.0, 40.0));
    n.name = name.into();
    h.ed.add_node(n, None)
}

fn stored_link(h: &exec::SessionHost, id: NodeId) -> String {
    h.ed.doc.get(id).unwrap().link.clone()
}

fn set(h: &mut exec::SessionHost, updates: Value) -> Value {
    call(h, "set_link", json!({"updates": updates}))
}

#[test]
fn mcp_set_link_stores_page_and_external_targets() {
    let mut h = host();
    let page = rect(&mut h, "Page link");
    let web = rect(&mut h, "Web link");
    let page_key = key(&h, page);
    let web_key = key(&h, web);

    let out = set(
        &mut h,
        json!([
            {"nodeId": page_key, "link": "page:Details"},
            {"nodeId": web_key, "link": "https://example.com/a?b=1"}
        ]),
    );
    assert_eq!(out["linked"], json!(2), "{out}");
    assert_eq!(stored_link(&h, page), "page:Details");
    assert_eq!(stored_link(&h, web), "https://example.com/a?b=1");

    let page_info = call(&mut h, "get_node_info", json!({"nodeId": page_key}));
    assert_eq!(page_info["link"], json!("page:Details"), "{page_info}");
    let web_info = call(&mut h, "get_node_info", json!({"nodeId": web_key}));
    assert_eq!(
        web_info["link"],
        json!("https://example.com/a?b=1"),
        "{web_info}"
    );
}

#[test]
fn mcp_set_link_empty_clears_and_undo_restores() {
    let mut h = host();
    let id = rect(&mut h, "Link");
    let k = key(&h, id);

    set(
        &mut h,
        json!([{"nodeId": k, "link": "https://example.com"}]),
    );
    assert_eq!(stored_link(&h, id), "https://example.com");

    let cleared = set(&mut h, json!([{"nodeId": k, "link": ""}]));
    assert_eq!(cleared["linked"], json!(1), "{cleared}");
    assert_eq!(stored_link(&h, id), "");

    call(&mut h, "undo", json!({}));
    assert_eq!(stored_link(&h, id), "https://example.com");

    call(&mut h, "redo", json!({}));
    assert_eq!(stored_link(&h, id), "");
}

#[test]
fn mcp_set_link_batch_is_one_undo_step() {
    let mut h = host();
    let a = rect(&mut h, "A");
    let b = rect(&mut h, "B");
    let a_key = key(&h, a);
    let b_key = key(&h, b);

    set(
        &mut h,
        json!([
            {"nodeId": a_key, "link": "https://a.example"},
            {"nodeId": b_key, "link": "https://b.example"}
        ]),
    );
    assert_eq!(stored_link(&h, a), "https://a.example");
    assert_eq!(stored_link(&h, b), "https://b.example");

    call(&mut h, "undo", json!({}));
    assert_eq!(stored_link(&h, a), "", "first node did not undo");
    assert_eq!(
        stored_link(&h, b),
        "",
        "second node did not undo in the same step"
    );

    call(&mut h, "redo", json!({}));
    assert_eq!(stored_link(&h, a), "https://a.example");
    assert_eq!(stored_link(&h, b), "https://b.example");
}

#[test]
fn mcp_set_link_rejects_unsafe_targets() {
    let mut h = host();
    let id = rect(&mut h, "Unsafe");
    let k = key(&h, id);

    for bad in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html,hi",
        "example.com",
        "page:",
    ] {
        let err = raw(
            &mut h,
            "set_link",
            json!({"updates": [{"nodeId": k, "link": bad}]}),
        )
        .expect_err("unsafe link should be refused");
        assert!(err.contains("unsafe link"), "{bad}: {err}");
        assert_eq!(stored_link(&h, id), "", "{bad} reached the document");
    }

    let out = set(
        &mut h,
        json!([{"nodeId": k, "link": " MAILTO:me@example.com "}]),
    );
    assert_eq!(out["linked"], json!(1), "{out}");
    assert_eq!(stored_link(&h, id), "MAILTO:me@example.com");
}
