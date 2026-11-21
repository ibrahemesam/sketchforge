//! CON-18: reverse a connector from the editor and through MCP.

use drawio_rs::geom::{Rect, v2};
use drawio_rs::mcp::{Out, exec};
use drawio_rs::model::{Binding, Head, LineJump, Node, NodeId, NodeKind, Route};
use drawio_rs::project::Store;
use serde_json::{Value, json};

fn host() -> exec::SessionHost {
    let store = Store::open(std::env::temp_dir().join(format!(
        "sketch-connector-reverse-{:016x}",
        fastrand::u64(..)
    )))
    .unwrap();
    let mut h = exec::SessionHost::new(store);
    h.session.create(&mut h.ed, "Reverse test").unwrap();
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

fn bound_connector(h: &mut exec::SessionHost, a: NodeId, b: NodeId) -> NodeId {
    let n = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(50.0, 40.0), v2(300.0, 200.0)],
            start: Some(Binding {
                node: a,
                fixed: v2(0.5, 0.5),
                inside: false,
                floating: false,
            }),
            end: Some(Binding {
                node: b,
                fixed: v2(0.25, 0.75),
                inside: false,
                floating: false,
            }),
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::Arrow,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 300.0, 200.0),
    );
    h.ed.add_node(n, None)
}

#[test]
fn mcp_reverse_connector_swaps_bindings_and_points() {
    let mut h = host();
    let a = h.ed.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = h.ed.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let id = bound_connector(&mut h, a, b);
    let before = match &h.ed.doc.get(id).unwrap().kind {
        NodeKind::Connector { points, .. } => points.clone(),
        _ => panic!("still a connector"),
    };
    let mut expected = before.clone();
    expected.reverse();
    let node_id = key(&h, id);
    let result = call(
        &mut h,
        "reverse_connectors",
        json!({"nodeIds": [node_id.clone()]}),
    );

    assert_eq!(result["reversed"], json!([key(&h, id)]), "{result}");
    let n = h.ed.doc.get(id).unwrap();
    let NodeKind::Connector {
        points,
        start,
        end,
        start_head,
        end_head,
        ..
    } = &n.kind
    else {
        panic!("still a connector")
    };
    assert_eq!(points, &expected);
    assert_eq!(start.as_ref().unwrap().node, b);
    assert_eq!(start.as_ref().unwrap().fixed, v2(0.25, 0.75));
    assert_eq!(end.as_ref().unwrap().node, a);
    assert_eq!(end.as_ref().unwrap().fixed, v2(0.5, 0.5));
    assert_eq!(*start_head, Head::None);
    assert_eq!(*end_head, Head::Arrow);

    call(&mut h, "undo", json!({}));
    let n = h.ed.doc.get(id).unwrap();
    let NodeKind::Connector {
        points, start, end, ..
    } = &n.kind
    else {
        panic!("still a connector")
    };
    assert_eq!(points, &before);
    assert_eq!(start.as_ref().unwrap().node, a);
    assert_eq!(end.as_ref().unwrap().node, b);
}

#[test]
fn mcp_reverse_connectors_skips_shapes_and_is_one_undo_step() {
    let mut h = host();
    let a = h.ed.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = h.ed.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let p = bound_connector(&mut h, a, b);
    let q = bound_connector(&mut h, b, a);
    let shape = h.ed.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Ellipse,
            Rect::new(10.0, 10.0, 30.0, 30.0),
        ),
        None,
    );

    let points_before = |h: &exec::SessionHost, id| match &h.ed.doc.get(id).unwrap().kind {
        NodeKind::Connector { points, .. } => points.clone(),
        _ => panic!("still a connector"),
    };
    let bp = points_before(&h, p);
    let bq = points_before(&h, q);
    let (pk, qk, sk) = (key(&h, p), key(&h, q), key(&h, shape));
    let result = call(
        &mut h,
        "reverse_connectors",
        json!({"nodeIds": [pk, qk, sk.clone()]}),
    );
    assert_eq!(result["count"], json!(2), "{result}");
    assert_eq!(result["notConnectors"], json!([sk]), "{result}");
    for (id, before) in [(p, &bp), (q, &bq)] {
        let n = h.ed.doc.get(id).unwrap();
        let NodeKind::Connector { points, .. } = &n.kind else {
            panic!("still a connector")
        };
        let mut expected = before.clone();
        expected.reverse();
        assert_eq!(points, &expected, "connector {id:?}");
    }

    call(&mut h, "undo", json!({}));
    for (id, before) in [(p, &bp), (q, &bq)] {
        let n = h.ed.doc.get(id).unwrap();
        let NodeKind::Connector { points, .. } = &n.kind else {
            panic!("still a connector")
        };
        assert_eq!(points, before, "connector {id:?} undone");
    }
}
