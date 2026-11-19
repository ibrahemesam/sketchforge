//! Connector flow animation: stored settings, draw.io mapping, and rendered motion.

mod common;

use common::{editor, gpu, render_canvas};
use sketch_rs::geom::{Rect, V2, v2};
use sketch_rs::io::drawio;
use sketch_rs::model::{
    Color, Document, FlowDirection, FlowTiming, Head, LineJump, Node, NodeId, NodeKind, Route,
};
use sketch_rs::render::draw;
use sketch_rs::render::scene;
use sketch_rs::render::tess::Tess;
use sketch_rs::ui::inspector;

fn connector() -> Node {
    let mut n = Node::new(
        NodeId(7),
        NodeKind::Connector {
            points: vec![v2(0.0, 40.0), v2(400.0, 40.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(100.0, 160.0, 400.0, 80.0),
    );
    n.style.stroke = Some(Color::INK);
    n.style.stroke_width = 4.0;
    n.style.dash = vec![20.0, 20.0];
    n.flow.enabled = true;
    n.flow.duration_ms = 500.0;
    n
}

fn stroke_vertices(n: &Node, now_ms: f64) -> Vec<[f32; 2]> {
    let drawn = draw::node_draw_with(n, n.rect, 1.0, false, false, None, &[], false, now_ms);
    let pieces = Tess::new(1.0).ops(&drawn.ops);
    pieces
        .into_iter()
        .flat_map(|p| p.mesh.vertices)
        .map(|v| v.pos)
        .collect()
}

#[test]
fn flow_dashes_move_with_time() {
    let n = connector();
    let start = stroke_vertices(&n, 0.0);
    let middle = stroke_vertices(&n, 250.0);
    assert!(!start.is_empty(), "the connector should tessellate");
    assert_ne!(
        start, middle,
        "flow animation should shift the dash pattern halfway through its cycle"
    );
}

#[test]
fn flow_frames_render_different_pixels() {
    let Some(g) = gpu() else { return };
    let mut e = editor(800.0, 400.0);
    let mut n = connector();
    n.style.stroke_width = 7.0;
    n.style.dash = vec![36.0, 24.0];
    n.flow.duration_ms = 900.0;
    let id = e.add_node(n, None);

    e.now_ms = 0.0;
    let first = render_canvas(g, &e);
    e.now_ms = 225.0;
    let quarter = render_canvas(g, &e);

    e.doc.nodes.get_mut(&id).unwrap().flow.enabled = false;
    e.now_ms = 0.0;
    let still = render_canvas(g, &e);
    e.now_ms = 225.0;
    let unchanged = render_canvas(g, &e);

    if let Ok(dir) = std::env::var("SKETCH_RS_FLOW_ARTIFACT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        first.save(dir.join("flow-frame-start.png")).unwrap();
        quarter.save(dir.join("flow-frame-quarter.png")).unwrap();
        still.save(dir.join("static-frame-start.png")).unwrap();
        unchanged
            .save(dir.join("static-frame-quarter.png"))
            .unwrap();
    }

    let changed = first
        .pixels()
        .zip(quarter.pixels())
        .filter(|(a, b)| a.0 != b.0)
        .count();
    assert!(
        changed > 100,
        "flow animation should change visible pixels, found {changed}"
    );
    assert_eq!(
        still.as_raw(),
        unchanged.as_raw(),
        "a disabled flow should stay pixel-identical across time"
    );
}

#[test]
fn flow_dashes_are_stable_when_disabled() {
    let mut n = connector();
    n.flow.enabled = false;
    assert_eq!(stroke_vertices(&n, 0.0), stroke_vertices(&n, 250.0));
}

#[test]
fn flow_starts_at_the_authored_dash_at_export_time() {
    let n = connector();
    assert_eq!(stroke_vertices(&n, 0.0), {
        let mut plain = n.clone();
        plain.flow.enabled = false;
        stroke_vertices(&plain, 0.0)
    });
}

#[test]
fn flow_defaults_keep_old_nodes_unchanged() {
    let mut n = connector();
    n.flow.enabled = false;
    n.flow.duration_ms = 500.0;
    let value = serde_json::to_value(&n).unwrap();
    assert!(
        value.get("flow").is_none(),
        "default flow should not be stored"
    );

    let mut old = serde_json::to_value(&n).unwrap();
    old.as_object_mut().unwrap().remove("flow");
    let restored: Node = serde_json::from_value(old).unwrap();
    assert!(!restored.flow.enabled);
    assert_eq!(restored.flow.duration_ms, 500.0);
}

#[test]
fn flow_timing_and_direction_change_the_dash_offset() {
    use sketch_rs::model::FlowAnimation;
    let pattern = [10.0, 10.0];
    let normal = FlowAnimation {
        enabled: true,
        duration_ms: 1000.0,
        timing: FlowTiming::Linear,
        direction: FlowDirection::Normal,
    };
    assert!((normal.dash_offset(0.0, &pattern) - 0.0).abs() < 1e-9);
    assert!((normal.dash_offset(250.0, &pattern) - 5.0).abs() < 1e-9);
    assert!((normal.dash_offset(1000.0, &pattern) - 0.0).abs() < 1e-9);

    let reverse = FlowAnimation {
        direction: FlowDirection::Reverse,
        ..normal
    };
    assert!((reverse.dash_offset(250.0, &pattern) - 15.0).abs() < 1e-9);

    let alternate = FlowAnimation {
        direction: FlowDirection::Alternate,
        ..normal
    };
    assert!((alternate.dash_offset(250.0, &pattern) - 10.0).abs() < 1e-9);
    assert!((alternate.dash_offset(500.0, &pattern) - 20.0).abs() < 1e-9);
    assert!((alternate.dash_offset(750.0, &pattern) - 10.0).abs() < 1e-9);

    let alternate_reverse = FlowAnimation {
        direction: FlowDirection::AlternateReverse,
        ..normal
    };
    assert!((alternate_reverse.dash_offset(0.0, &pattern) - 20.0).abs() < 1e-9);
    assert!((alternate_reverse.dash_offset(500.0, &pattern) - 0.0).abs() < 1e-9);

    let ease_in = FlowAnimation {
        timing: FlowTiming::EaseIn,
        ..normal
    };
    assert!(ease_in.dash_offset(250.0, &pattern) < normal.dash_offset(250.0, &pattern));
}

#[test]
fn scene_cache_rebuilds_flowing_connectors_each_frame() {
    let mut e = editor(800.0, 600.0);
    let n = connector();
    let id = e.add_node(n, None);
    let mut cache = scene::SceneCache::default();

    e.now_ms = 0.0;
    let first = scene::canvas(&e, &mut cache, V2::ZERO, false, false);
    e.now_ms = 250.0;
    let second = scene::canvas(&e, &mut cache, V2::ZERO, false, false);
    let positions = |f: &scene::CanvasFrame| -> Vec<[f32; 2]> {
        f.items
            .iter()
            .flat_map(|item| item.pieces.iter())
            .flat_map(|piece| piece.mesh.vertices.iter().map(|v| v.pos))
            .collect()
    };
    assert!(!positions(&first).is_empty());
    assert_ne!(positions(&first), positions(&second));

    e.doc.nodes.get_mut(&id).unwrap().flow.enabled = false;
    e.now_ms = 0.0;
    let still = scene::canvas(&e, &mut cache, V2::ZERO, false, false);
    e.now_ms = 250.0;
    let unchanged = scene::canvas(&e, &mut cache, V2::ZERO, false, false);
    assert_eq!(positions(&still), positions(&unchanged));
}

#[test]
fn inspector_edits_join_undo_and_drive_the_animation_clock() {
    let mut e = editor(800.0, 600.0);
    let mut n = connector();
    n.flow.enabled = false;
    let id = e.add_node(n, None);
    assert!(!e.animating(1234.0));

    inspector::apply(&mut e, "flow animation", &[id], |n| n.flow.enabled = true);
    assert!(e.doc.get(id).unwrap().flow.enabled);
    assert!(e.animating(1234.0));
    e.undo();
    assert!(!e.doc.get(id).unwrap().flow.enabled);
    assert!(!e.animating(1234.0));
    e.redo();
    assert!(e.doc.get(id).unwrap().flow.enabled);

    inspector::apply(&mut e, "flow timing", &[id], |n| {
        n.flow.timing = FlowTiming::EaseOut
    });
    inspector::apply(&mut e, "flow direction", &[id], |n| {
        n.flow.direction = FlowDirection::Alternate
    });
    let flow = e.doc.get(id).unwrap().flow;
    assert_eq!(flow.timing, FlowTiming::EaseOut);
    assert_eq!(flow.direction, FlowDirection::Alternate);
}

#[test]
fn drawio_style_round_trips_flow_animation() {
    let xml = r#"<mxfile><diagram id="d" name="Flow"><mxGraphModel><root>
        <mxCell id="0" /><mxCell id="1" parent="0" />
        <mxCell id="a" value="A" vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="40" as="geometry" /></mxCell>
        <mxCell id="b" value="B" vertex="1" parent="1"><mxGeometry x="300" y="0" width="80" height="40" as="geometry" /></mxCell>
        <mxCell id="e" edge="1" parent="1" source="a" target="b" style="flowAnimation=1;flowAnimationDuration=1200;flowAnimationTimingFunction=ease-in-out;flowAnimationDirection=alternate-reverse;" />
    </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let edge = pages[0]
        .doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .expect("the edge imported");
    assert!(edge.flow.enabled);
    assert_eq!(edge.flow.duration_ms, 1200.0);
    assert_eq!(edge.flow.timing, FlowTiming::EaseInOut);
    assert_eq!(edge.flow.direction, FlowDirection::AlternateReverse);

    let exported = drawio::export(&[("Flow", &pages[0].doc)]);
    assert!(exported.contains("flowAnimation=1"));
    assert!(exported.contains("flowAnimationDuration=1200"));
    assert!(exported.contains("flowAnimationTimingFunction=ease-in-out"));
    assert!(exported.contains("flowAnimationDirection=alternate-reverse"));
    let again = drawio::import_str(&exported).unwrap();
    let edge = again.0[0]
        .doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .unwrap();
    assert_eq!(
        edge.flow,
        pages[0]
            .doc
            .nodes
            .values()
            .find(|n| n.is_connector())
            .unwrap()
            .flow
    );
}

#[test]
fn drawio_legacy_flow_duration_imports() {
    let xml = r#"<mxfile><diagram id="d" name="Flow"><mxGraphModel><root>
        <mxCell id="0" /><mxCell id="1" parent="0" />
        <mxCell id="a" value="A" vertex="1" parent="1"><mxGeometry x="0" y="0" width="80" height="40" as="geometry" /></mxCell>
        <mxCell id="b" value="B" vertex="1" parent="1"><mxGeometry x="300" y="0" width="80" height="40" as="geometry" /></mxCell>
        <mxCell id="e" edge="1" parent="1" source="a" target="b" style="flowAnimation=1;flowDuration=900;" />
    </root></mxGraphModel></diagram></mxfile>"#;
    let (pages, _) = drawio::import_str(xml).unwrap();
    let edge = pages[0]
        .doc
        .nodes
        .values()
        .find(|n| n.is_connector())
        .unwrap();
    assert_eq!(edge.flow.duration_ms, 900.0);
    assert_eq!(edge.flow.timing, FlowTiming::Linear);
    assert_eq!(edge.flow.direction, FlowDirection::Normal);
}

#[test]
fn native_page_json_preserves_flow_settings() {
    let mut doc = Document::default();
    let mut edge = connector();
    edge.flow.duration_ms = 1750.0;
    edge.flow.timing = FlowTiming::EaseIn;
    edge.flow.direction = FlowDirection::Reverse;
    let id = edge.id;
    doc.nodes.insert(id, edge);
    doc.root.push(id);
    let json = serde_json::to_string(&doc).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back.get(id).unwrap().flow, doc.get(id).unwrap().flow);
}
