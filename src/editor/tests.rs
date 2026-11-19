#![allow(clippy::field_reassign_with_default)]

//! Scenario tests: drive the editor with synthetic input, assert document state.

use super::*;
use crate::geom::Camera;
use crate::model::{
    Color, Head, InstanceLink, Layer, LayerId, LineJump, Paint, Route, SchemeScope, TextScript,
    TextSpan,
};

fn ed() -> Editor {
    let mut e = Editor::default();
    e.viewport = v2(1000.0, 800.0);
    e.camera = Camera {
        pan: V2::ZERO,
        zoom: 1.0,
    };
    e
}

fn down(e: &mut Editor, x: f64, y: f64, t: f64) {
    e.handle(InputEvent::PointerDown {
        pos: v2(x, y),
        button: Button::Primary,
        pressure: None,
        time_ms: t,
    });
}
fn mv(e: &mut Editor, x: f64, y: f64, t: f64) {
    e.handle(InputEvent::PointerMove {
        pos: v2(x, y),
        pressure: None,
        time_ms: t,
    });
}
fn up(e: &mut Editor, x: f64, y: f64, t: f64) {
    e.handle(InputEvent::PointerUp {
        pos: v2(x, y),
        button: Button::Primary,
        time_ms: t,
    });
}
fn drag(e: &mut Editor, from: (f64, f64), to: (f64, f64), t0: f64) {
    down(e, from.0, from.1, t0);
    for i in 1..=10 {
        let k = i as f64 / 10.0;
        mv(
            e,
            from.0 + (to.0 - from.0) * k,
            from.1 + (to.1 - from.1) * k,
            t0 + i as f64 * 16.0,
        );
    }
    up(e, to.0, to.1, t0 + 200.0);
}
fn key(e: &mut Editor, k: Key, m: Modifiers) {
    e.handle(InputEvent::Modifiers(m));
    e.handle(InputEvent::KeyDown {
        key: k,
        repeat: false,
    });
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
}
fn press(e: &mut Editor, c: char) {
    key(e, Key::Char(c), Modifiers::NONE);
}
fn rect_of(e: &Editor, id: NodeId) -> Rect {
    e.doc.world_rect(id).unwrap()
}

// ---------------------------------------------------------------- M1: canvas

#[test]
fn wheel_pans_and_ctrl_wheel_zooms_about_cursor() {
    let mut e = ed();
    e.handle(InputEvent::Scroll {
        delta: v2(30.0, -50.0),
        pos: v2(500.0, 400.0),
    });
    assert_eq!(e.camera.pan, v2(30.0, -50.0));
    let anchor = v2(200.0, 100.0);
    let before = e.screen_to_world(anchor);
    e.handle(InputEvent::Modifiers(Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    }));
    e.handle(InputEvent::Scroll {
        delta: v2(0.0, -100.0),
        pos: anchor,
    });
    assert!(e.camera.zoom > 2.5);
    assert!(e.screen_to_world(anchor).dist(before) < 1e-9);
    e.handle(InputEvent::Zoom {
        factor: 0.5,
        pos: anchor,
    });
    assert!(e.screen_to_world(anchor).dist(before) < 1e-9);
}

#[test]
fn space_drag_and_hand_tool_pan() {
    let mut e = ed();
    e.handle(InputEvent::KeyDown {
        key: Key::Space,
        repeat: false,
    });
    drag(&mut e, (100.0, 100.0), (160.0, 140.0), 0.0);
    e.handle(InputEvent::KeyUp { key: Key::Space });
    assert_eq!(e.camera.pan, v2(-60.0, -40.0));
    assert!(e.doc.nodes.is_empty());
    press(&mut e, 'h');
    drag(&mut e, (100.0, 100.0), (90.0, 100.0), 300.0);
    assert_eq!(e.camera.pan, v2(-50.0, -40.0));
}

#[test]
fn zoom_shortcuts() {
    let mut e = ed();
    key(&mut e, Key::Char('='), Modifiers::cmd());
    assert_eq!(e.camera.zoom, 1.5);
    key(&mut e, Key::Char('-'), Modifiers::cmd());
    key(&mut e, Key::Char('-'), Modifiers::cmd());
    assert_eq!(e.camera.zoom, 0.75);
    key(&mut e, Key::Char('0'), Modifiers::cmd());
    assert_eq!(e.camera.zoom, 1.0);
    e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(5000.0, 5000.0, 100.0, 100.0),
        ),
        None,
    );
    key(&mut e, Key::Char('1'), Modifiers::SHIFT);
    let c = e.camera.world_to_screen(v2(5050.0, 5050.0));
    assert!(c.dist(e.viewport * 0.5) < 1e-6);
}

// ---------------------------------------------------------------- M2: sketching

#[test]
fn pencil_draws_one_smoothed_stroke_per_gesture() {
    let mut e = ed();
    press(&mut e, 'b');
    down(&mut e, 100.0, 100.0, 0.0);
    let mut seed = 7u64;
    for i in 1..=60 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let jitter = ((seed >> 40) % 5) as f64 - 2.0;
        mv(
            &mut e,
            100.0 + i as f64 * 5.0,
            100.0 + jitter,
            i as f64 * 8.0,
        );
    }
    up(&mut e, 400.0, 100.0, 500.0);
    assert_eq!(e.doc.nodes.len(), 1);
    assert_eq!(e.history.undo_len(), 1, "a whole stroke is one undo step");
    let n = e.doc.nodes.values().next().unwrap();
    let NodeKind::Stroke { points, width, .. } = &n.kind else {
        panic!("not a stroke")
    };
    assert!(points.len() > 60, "resampled densely: {}", points.len());
    assert_eq!(*width, e.ink_width);
    let r = rect_of(&e, n.id);
    assert!(r.x < 100.0 && r.right() > 400.0 && r.h < 12.0, "{r:?}");
    // Still the pencil, so the next stroke starts immediately.
    assert_eq!(e.tool, Tool::Pencil);
    e.undo();
    assert!(e.doc.nodes.is_empty());
}

#[test]
fn stroke_width_is_screen_constant_when_zoomed() {
    let mut e = ed();
    e.camera.zoom = 4.0;
    press(&mut e, 'b');
    drag(&mut e, (10.0, 10.0), (200.0, 50.0), 0.0);
    let n = e.doc.nodes.values().next().unwrap();
    let NodeKind::Stroke { width, .. } = n.kind else {
        panic!()
    };
    assert_eq!(width, e.ink_width / 4.0);
}

#[test]
fn eraser_removes_touched_strokes_only() {
    let mut e = ed();
    press(&mut e, 'b');
    drag(&mut e, (100.0, 100.0), (300.0, 100.0), 0.0);
    drag(&mut e, (100.0, 300.0), (300.0, 300.0), 1000.0);
    let rect = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(150.0, 180.0, 50.0, 50.0),
        ),
        None,
    );
    press(&mut e, 'e');
    drag(&mut e, (200.0, 60.0), (200.0, 200.0), 2000.0);
    assert_eq!(e.doc.nodes.len(), 2);
    assert!(e.doc.get(rect).is_some());
    e.undo();
    assert_eq!(e.doc.nodes.len(), 3);
}

// ---------------------------------------------------------------- M3: shapes, frames, text

#[test]
fn shape_tools_create_by_drag_and_click() {
    let mut e = ed();
    press(&mut e, 'r');
    drag(&mut e, (100.0, 100.0), (250.0, 180.0), 0.0);
    let r = e.selection[0];
    assert_eq!(rect_of(&e, r), Rect::new(100.0, 100.0, 150.0, 80.0));
    assert_eq!(e.tool, Tool::Select, "returns to select after creating");

    press(&mut e, 'o');
    e.handle(InputEvent::Modifiers(Modifiers::SHIFT));
    drag(&mut e, (400.0, 100.0), (500.0, 140.0), 500.0);
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
    let o = e.selection[0];
    assert_eq!(
        rect_of(&e, o).w,
        rect_of(&e, o).h,
        "shift constrains to a circle"
    );

    press(&mut e, 'l');
    down(&mut e, 600.0, 600.0, 900.0);
    up(&mut e, 600.0, 600.0, 950.0);
    let l = e.selection[0];
    assert!(matches!(e.doc.get(l).unwrap().kind, NodeKind::Line { b, .. } if b == v2(100.0, 0.0)));
    assert_eq!(e.history.undo_len(), 3);
}

#[test]
fn frames_adopt_children_and_clip_hit_testing() {
    let mut e = ed();
    press(&mut e, 'r');
    drag(&mut e, (120.0, 120.0), (160.0, 160.0), 0.0);
    let inner = e.selection[0];
    press(&mut e, 'f');
    drag(&mut e, (100.0, 100.0), (400.0, 300.0), 500.0);
    let frame = e.selection[0];
    assert!(e.doc.get(frame).unwrap().is_frame());
    assert_eq!(
        e.doc.get(inner).unwrap().parent,
        Some(frame),
        "frame drawn around a shape adopts it"
    );
    assert_eq!(e.doc.get(inner).unwrap().rect.min(), v2(20.0, 20.0));
    // New shapes drawn inside a frame become its children.
    press(&mut e, 'o');
    drag(&mut e, (300.0, 200.0), (350.0, 250.0), 900.0);
    let child = e.selection[0];
    assert_eq!(e.doc.get(child).unwrap().parent, Some(frame));
    // Moving the frame (by its label) moves its children.
    let label = e.frame_labels()[0].1;
    drag(
        &mut e,
        (label.x + 5.0, label.y + 5.0),
        (label.x + 55.0, label.y + 5.0),
        1500.0,
    );
    assert_eq!(rect_of(&e, inner).x, 170.0);
    e.doc.validate().unwrap();
}

#[test]
fn double_click_frame_title_renames_it() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: true },
            Rect::new(100.0, 100.0, 300.0, 200.0),
        ),
        None,
    );
    let label = e.frame_labels()[0].1;
    down(&mut e, label.x + 10.0, label.y + 8.0, 0.0);
    up(&mut e, label.x + 10.0, label.y + 8.0, 10.0);
    down(&mut e, label.x + 10.0, label.y + 8.0, 300.0);
    assert_eq!(e.text_edit.as_ref().map(|t| t.id), Some(id));
    e.handle(InputEvent::Text("Auth flow".into()));
    e.end_text_edit();
    assert_eq!(e.doc.get(id).unwrap().name, "Auth flow");
    assert!(e.frame_labels()[0].1.w >= "Auth flow".len() as f64 * 3.0);
}

#[test]
fn frame_title_caret_tracks_the_zoomed_screen_text() {
    let mut e = ed();
    e.camera.zoom = 2.0;
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: true },
            Rect::new(100.0, 100.0, 300.0, 200.0),
        ),
        None,
    );
    e.doc.nodes.get_mut(&id).unwrap().name = "Auth flow".into();
    let label = e.frame_labels()[0].1;
    down(&mut e, label.x + 6.0, label.y + 8.0, 0.0);
    up(&mut e, label.x + 6.0, label.y + 8.0, 10.0);
    down(&mut e, label.x + 6.0, label.y + 8.0, 300.0);
    assert_eq!(e.text_edit.as_ref().map(|t| t.id), Some(id));

    // A title is drawn at a fixed 12 px, so clicking a character boundary in
    // screen space must land on that boundary even while the canvas is zoomed.
    let (l, o) = e.frame_title(id).unwrap();
    let (x, _, _) = l.caret(4);
    down(&mut e, o.x + x, o.y + 6.0, 600.0);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 4);
    up(&mut e, o.x + x, o.y + 6.0, 610.0);

    let (x0, _, _) = l.caret(1);
    let (x1, _, _) = l.caret(3);
    drag(&mut e, (o.x + x0, o.y + 6.0), (o.x + x1, o.y + 6.0), 700.0);
    let te = e.text_edit.as_ref().unwrap();
    assert_eq!(te.anchor, Some(1));
    assert_eq!(te.caret, 3);
}

#[test]
fn clicking_a_child_selects_it_even_when_its_frame_is_selected() {
    let mut e = ed();
    press(&mut e, 'r');
    drag(&mut e, (120.0, 120.0), (160.0, 160.0), 0.0);
    let child = e.selection[0];
    press(&mut e, 'f');
    drag(&mut e, (100.0, 100.0), (400.0, 300.0), 500.0);
    let frame = e.selection[0];
    assert_eq!(e.selection, vec![frame]);

    down(&mut e, 140.0, 140.0, 1000.0);
    up(&mut e, 140.0, 140.0, 1010.0);
    assert_eq!(
        e.selection,
        vec![child],
        "a child is selectable without deselecting its frame first"
    );
}

#[test]
fn double_click_shape_adds_centered_label() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 200.0, 100.0),
        ),
        None,
    );
    down(&mut e, 200.0, 150.0, 0.0);
    up(&mut e, 200.0, 150.0, 10.0);
    down(&mut e, 200.0, 150.0, 300.0);
    assert_eq!(e.text_edit.as_ref().map(|t| t.id), Some(id));
    e.handle(InputEvent::Text("Hello".into()));
    e.end_text_edit();
    assert_eq!(e.doc.get(id).unwrap().label, "Hello");
    let n = e.doc.get(id).unwrap();
    let l = text::layout_node(n).unwrap();
    let o = text::text_origin(n, &l);
    let glyph_mid = l.glyphs.iter().map(|g| g.x).sum::<f64>() / l.glyphs.len() as f64;
    assert!(
        (o.x + glyph_mid - 100.0).abs() < 8.0,
        "label is roughly centered"
    );
    assert!((o.y + l.height * 0.5 - 50.0).abs() < 1.0);
}

#[test]
fn double_click_connector_edits_label_on_the_line() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(200.0, 100.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(100.0, 100.0, 200.0, 100.0),
        ),
        None,
    );
    down(&mut e, 200.0, 150.0, 0.0);
    up(&mut e, 200.0, 150.0, 10.0);
    down(&mut e, 200.0, 150.0, 300.0);
    assert_eq!(e.text_edit.as_ref().map(|t| t.id), Some(id));
    e.handle(InputEvent::Text("one way".into()));
    e.end_text_edit();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label, "one way");
    assert!(n.supports_label());
    let l = text::layout_node(n).unwrap();
    let o = text::text_origin(n, &l);
    let center = o + v2(text::CONNECTOR_LABEL_WIDTH * 0.5, l.height * 0.5);
    assert!(
        (center - v2(100.0, 50.0)).len() < 1.0,
        "label centers on the routed line, {center:?}"
    );
    e.undo();
    assert_eq!(
        e.doc.get(id).unwrap().label,
        "",
        "undo restores the old label"
    );
}

#[test]
fn connector_label_follows_the_route() {
    let line = vec![v2(0.0, 0.0), v2(200.0, 100.0)];
    assert_eq!(
        crate::connector::path_midpoint(&line, Route::Straight),
        v2(100.0, 50.0)
    );
    // An elbow's label follows the routed line. The rounded corner moves the
    // halfway point nearer the bend than the bounding-box center (50, 50).
    let elbow = vec![v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 100.0)];
    let mid = crate::connector::path_midpoint(&elbow, Route::Elbow);
    let path = crate::connector::line_path(&elbow, Route::Elbow);
    assert!(
        path.distance(mid, 0.25) <= 0.5,
        "elbow midpoint {mid:?} sits on the routed line"
    );
    assert!(
        mid.x > 90.0 && mid.y < 10.0,
        "halfway along the route is near the bend, {mid:?}"
    );
    assert!(
        (mid - v2(50.0, 50.0)).len() > 40.0,
        "not the bounding-box center"
    );
}

#[test]
fn connector_label_position_and_offset_place_the_box() {
    let mut n = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::Arrow,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 200.0, 0.0),
    );
    n.label = "edge".into();
    let default = text::label_box(&n).unwrap();
    assert!((default.center() - v2(100.0, 0.0)).len() < 1e-9);
    n.label_pos = Some(0.25);
    n.label_offset = Some(v2(0.0, -30.0));
    let moved = text::label_box(&n).unwrap();
    assert!(
        (moved.center() - v2(50.0, -30.0)).len() < 1e-9,
        "box center {:?}",
        moved.center()
    );
}

#[test]
fn dragging_a_connector_label_moves_it_along_and_off_the_line() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 200.0, 0.0),
        ),
        None,
    );
    e.edit("label", |tx| tx.node_mut(id).unwrap().label = "edge".into());

    // The default label center sits at the midpoint of the line.
    drag(&mut e, (100.0, 0.0), (50.0, -40.0), 0.0);
    let n = e.doc.get(id).unwrap();
    assert!(
        (n.label_pos.unwrap_or(0.5) - 0.25).abs() < 0.02,
        "position {:?}",
        n.label_pos
    );
    assert!(
        n.label_offset
            .is_some_and(|o| (o - v2(0.0, -40.0)).len() < 1.0),
        "offset {:?}",
        n.label_offset
    );
    let boxr = text::label_box(n).unwrap();
    assert!(
        (boxr.center() - v2(50.0, -40.0)).len() < 1.0,
        "label follows the pointer, center {:?}",
        boxr.center()
    );
    e.undo();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label, "edge", "undo keeps the text");
    assert_eq!(n.label_pos, None);
    assert_eq!(n.label_offset, None);

    // Dragging back onto the line stores only the position.
    drag(&mut e, (100.0, 0.0), (150.0, 0.0), 400.0);
    let n = e.doc.get(id).unwrap();
    assert!(
        (n.label_pos.unwrap_or(0.5) - 0.75).abs() < 0.02,
        "position {:?}",
        n.label_pos
    );
    assert_eq!(n.label_offset, None);
}

#[test]
fn connector_label_hit_testing_and_editing_work_at_zoom() {
    for zoom in [0.5, 2.0] {
        let mut e = ed();
        e.camera.zoom = zoom;
        let id = e.add_node(
            Node::new(
                NodeId(0),
                NodeKind::Connector {
                    points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
                    start: None,
                    end: None,
                    route: Route::Straight,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(100.0, 100.0, 200.0, 0.0),
            ),
            None,
        );
        e.edit("label", |tx| tx.node_mut(id).unwrap().label = "edge".into());
        let center = v2(200.0, 100.0);
        assert_eq!(
            crate::editor::hit::hit_test(&e.doc, center, zoom),
            Some(id),
            "label hit at {zoom}x"
        );
        let screen = center * zoom;
        down(&mut e, screen.x, screen.y, 0.0);
        up(&mut e, screen.x, screen.y, 10.0);
        down(&mut e, screen.x, screen.y, 300.0);
        assert_eq!(
            e.text_edit.as_ref().map(|t| t.id),
            Some(id),
            "double-click edits the label at {zoom}x"
        );
        e.end_text_edit();
    }
}

#[test]
fn reset_label_position_and_waypoints_are_one_undo_step() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(100.0, 50.0), v2(200.0, 0.0)],
                start: None,
                end: None,
                route: Route::Curved,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 200.0, 50.0),
        ),
        None,
    );
    e.edit("label", |tx| {
        let n = tx.node_mut(id).unwrap();
        n.label = "edge".into();
        n.label_pos = Some(0.2);
        n.label_offset = Some(v2(10.0, 10.0));
    });
    e.reset_connector_label_positions(&[id]);
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label_pos, None);
    assert_eq!(n.label_offset, None);
    e.undo();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label_pos, Some(0.2));
    assert_eq!(n.label_offset, Some(v2(10.0, 10.0)));

    e.reset_connector_waypoints(&[id]);
    let n = e.doc.get(id).unwrap();
    let NodeKind::Connector { points, .. } = &n.kind else {
        panic!("still a connector")
    };
    assert_eq!(points.len(), 2, "the bend is gone");
    e.undo();
    let n = e.doc.get(id).unwrap();
    let NodeKind::Connector { points, .. } = &n.kind else {
        panic!("still a connector")
    };
    assert_eq!(points.len(), 3, "undo restores the bend");
}

#[test]
fn moving_a_shape_into_an_elbow_route_reroutes_around_it() {
    let mut e = ed();
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let wall = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Rect,
            Rect::new(600.0, 500.0, 100.0, 100.0),
        ),
        None,
    );
    let id = e.add_node(
        Node::new(
            NodeId(3),
            NodeKind::Connector {
                points: vec![v2(100.0, 30.0), v2(300.0, 230.0)],
                start: Some(crate::model::Binding {
                    node: a,
                    fixed: v2(1.0, 0.5),
                    inside: false,
                    floating: false,
                }),
                end: Some(crate::model::Binding {
                    node: b,
                    fixed: v2(0.0, 0.5),
                    inside: false,
                    floating: false,
                }),
                route: Route::Elbow,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(100.0, 30.0, 200.0, 200.0),
        ),
        None,
    );
    // Park the wall on the direct path, like dropping a box between two
    // connected shapes.
    e.select(vec![wall]);
    e.snapping = false;
    drag(&mut e, (650.0, 550.0), (200.0, 150.0), 0.0);
    let wr = rect_of(&e, wall);
    assert!(
        wr.intersects(&Rect::new(100.0, 30.0, 200.0, 200.0)),
        "the wall landed on the old route: {wr:?}"
    );
    let pts: Vec<V2> = match &e.doc.get(id).unwrap().kind {
        NodeKind::Connector { points, .. } => {
            points.iter().map(|p| rect_of(&e, id).min() + *p).collect()
        }
        _ => panic!("still a connector"),
    };
    let inner = wr.inflate(-0.5);
    for w in pts.windows(2) {
        let steps = (w[0].dist(w[1]) / 2.0).ceil() as usize;
        for k in 0..=steps {
            let p = w[0] + (w[1] - w[0]) * (k as f64 / steps.max(1) as f64);
            assert!(!inner.contains(p), "route point {p:?} inside the wall");
        }
    }
}

#[test]
fn reverse_connector_swaps_bindings_heads_points_and_label() {
    let mut e = ed();
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let id = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(50.0, 40.0), v2(300.0, 200.0)],
                start: Some(crate::model::Binding {
                    node: a,
                    fixed: v2(0.5, 0.5),
                    inside: false,
                    floating: false,
                }),
                end: Some(crate::model::Binding {
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
        ),
        None,
    );
    e.edit("label", |tx| {
        let n = tx.node_mut(id).unwrap();
        n.label = "tag".into();
        n.label_pos = Some(0.8);
    });
    let before: Vec<V2> = match &e.doc.get(id).unwrap().kind {
        NodeKind::Connector { points, .. } => points.clone(),
        _ => panic!("still a connector"),
    };

    e.reverse_connectors(&[id]);
    {
        let n = e.doc.get(id).unwrap();
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
        let mut expected = before.clone();
        expected.reverse();
        assert_eq!(*points, expected, "the point order flipped");
        assert_eq!(start.as_ref().unwrap().node, b, "the target is the source");
        assert_eq!(start.as_ref().unwrap().fixed, v2(0.25, 0.75));
        assert_eq!(end.as_ref().unwrap().node, a);
        assert_eq!(*start_head, Head::None, "the heads stay on the line ends");
        assert_eq!(*end_head, Head::Arrow);
        assert_eq!(
            n.label_pos,
            Some(0.8),
            "the label keeps its relative spot, so it visually mirrors"
        );
    }

    e.undo();
    let n = e.doc.get(id).unwrap();
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
    assert_eq!(*points, before);
    assert_eq!(start.as_ref().unwrap().node, a);
    assert_eq!(end.as_ref().unwrap().node, b);
    assert_eq!(*start_head, Head::None);
    assert_eq!(*end_head, Head::Arrow);
    assert_eq!(n.label_pos, Some(0.8));
}

#[test]
fn reverse_connectors_is_one_undo_step_for_a_selection() {
    let mut e = ed();
    let mk = |e: &mut Editor, id: u64| {
        e.add_node(
            Node::new(
                NodeId(id),
                NodeKind::Connector {
                    points: vec![v2(0.0, 0.0), v2(100.0, 0.0)],
                    start: None,
                    end: None,
                    route: Route::Straight,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(0.0, 0.0, 100.0, 0.0),
            ),
            None,
        )
    };
    let p = mk(&mut e, 0);
    let q = mk(&mut e, 1);
    e.reverse_connectors(&[p, q]);
    for id in [p, q] {
        let n = e.doc.get(id).unwrap();
        let NodeKind::Connector { points, .. } = &n.kind else {
            panic!("still a connector")
        };
        assert_eq!(points[0], v2(100.0, 0.0), "connector {id:?} reversed");
    }
    e.undo();
    for id in [p, q] {
        let n = e.doc.get(id).unwrap();
        let NodeKind::Connector { points, .. } = &n.kind else {
            panic!("still a connector")
        };
        assert_eq!(
            points[0],
            v2(0.0, 0.0),
            "one undo restores connector {id:?}"
        );
    }
}

#[test]
fn adding_and_dragging_an_extra_connector_label() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 200.0, 0.0),
        ),
        None,
    );
    e.edit("label", |tx| tx.node_mut(id).unwrap().label = "edge".into());

    e.add_connector_label(id);
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.labels.len(), 1, "the extra label is stored");
    assert_eq!(n.label_text(1), Some("Label"), "placeholder text");
    let te = e
        .text_edit
        .as_ref()
        .expect("the editor opens the new label");
    assert_eq!(te.label, 1);
    assert_eq!(te.anchor, Some(0), "the placeholder starts selected");
    e.end_text_edit();

    // The new label sits on the line midpoint, on top of the main label, so a
    // press there grabs the extra label.
    drag(&mut e, (100.0, 0.0), (50.0, -40.0), 0.0);
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label_pos, None, "the main label did not move");
    assert_eq!(n.label_offset, None);
    assert!(
        n.labels[0].pos.is_some_and(|t| (t - 0.25).abs() < 0.02),
        "extra position {:?}",
        n.labels[0].pos
    );
    assert!(
        n.labels[0]
            .offset
            .is_some_and(|o| (o - v2(0.0, -40.0)).len() < 1.0),
        "extra offset {:?}",
        n.labels[0].offset
    );

    // Clearing an extra label's text removes it; the main label stays.
    e.start_text_edit_label(id, 1, None);
    key(&mut e, Key::Backspace, Modifiers::NONE);
    e.end_text_edit();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.labels.len(), 0, "the emptied extra label is pruned");
    assert_eq!(n.label, "edge");
}

#[test]
fn two_connector_labels_cut_the_line_and_render() {
    let mut n = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 200.0, 0.0),
    );
    n.label = "edge".into();
    n.labels.push(crate::model::ExtraLabel::new("two"));
    let _ = n.labels[0].pos.insert(0.25);
    let wr = n.rect;
    // Both labels report a gap, at their own positions along the route.
    let gaps = crate::text::label_gap_rects(&n);
    assert_eq!(gaps.len(), 2, "{gaps:?}");
    assert!(
        gaps[1].center().x < gaps[0].center().x,
        "the 0.25 extra sits left of the main label at 0.5: {gaps:?}"
    );
    let drawn = crate::render::draw::node_draw(&n, wr, 1.0);
    let strokes: Vec<&crate::render::draw::DrawOp> = drawn
        .ops
        .iter()
        .filter(|op| matches!(op, crate::render::draw::DrawOp::Stroke { .. }))
        .collect();
    assert_eq!(strokes.len(), 1, "heads are none, so one line stroke");
    let crate::render::draw::DrawOp::Stroke { path, .. } = strokes[0] else {
        unreachable!()
    };
    assert_eq!(
        path.segs
            .iter()
            .filter(|s| matches!(s, crate::vector::Seg::M(_)))
            .count(),
        3,
        "two labels split the line into three runs"
    );
    for hole in &gaps {
        let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
        for poly in path.flatten(0.25) {
            for p in poly.pts {
                assert!(
                    !inner.contains(p),
                    "stroke point {p:?} runs through a label"
                );
            }
        }
    }
    // Both labels paint their glyphs.
    let glyph_ops = drawn
        .ops
        .iter()
        .filter(|op| matches!(op, crate::render::draw::DrawOp::Glyphs { .. }))
        .count();
    assert!(glyph_ops >= 2, "both labels draw glyphs: {glyph_ops}");
}

#[test]
fn transparent_connector_label_cuts_the_line_behind_the_text() {
    let mut n = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 200.0, 0.0),
    );
    n.label = "edge".into();
    let wr = n.rect;
    // Without a background the label reports a gap and the line avoids it.
    let hole = crate::text::label_gap_rect(&n).unwrap();
    let drawn = crate::render::draw::node_draw(&n, wr, 1.0);
    let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
    let strokes: Vec<&crate::render::draw::DrawOp> = drawn
        .ops
        .iter()
        .filter(|op| matches!(op, crate::render::draw::DrawOp::Stroke { .. }))
        .collect();
    assert_eq!(strokes.len(), 1, "heads are none, so one line stroke");
    let crate::render::draw::DrawOp::Stroke { path, .. } = strokes[0] else {
        unreachable!()
    };
    assert_eq!(
        path.segs
            .iter()
            .filter(|s| matches!(s, crate::vector::Seg::M(_)))
            .count(),
        2,
        "the line is split into two runs behind the label"
    );
    for poly in path.flatten(0.25) {
        for p in poly.pts {
            assert!(
                !inner.contains(p),
                "stroke point {p:?} runs through the label"
            );
        }
    }
    // With a solid background the plate paints over one continuous line.
    n.text_bg = Some(Color::WHITE);
    assert_eq!(crate::text::label_gap_rect(&n), None);
    let drawn = crate::render::draw::node_draw(&n, wr, 1.0);
    let strokes: Vec<&crate::render::draw::DrawOp> = drawn
        .ops
        .iter()
        .filter(|op| matches!(op, crate::render::draw::DrawOp::Stroke { .. }))
        .collect();
    assert_eq!(strokes.len(), 1, "one continuous line under the plate");
    let crate::render::draw::DrawOp::Stroke { path, .. } = strokes[0] else {
        unreachable!()
    };
    assert_eq!(
        path.segs
            .iter()
            .filter(|s| matches!(s, crate::vector::Seg::M(_)))
            .count(),
        1,
        "no cut under the plate"
    );
}

#[test]
fn clear_formatting_resets_a_connector_label() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Connector {
                points: vec![v2(0.0, 0.0), v2(120.0, 0.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 120.0, 0.0),
        ),
        None,
    );
    e.edit("label style", |tx| {
        let n = tx.node_mut(id).unwrap();
        n.label = "edge".into();
        n.label_color = Some(Color::rgb(0xff, 0x00, 0x00));
        n.label_size = Some(22.0);
        n.label_wrap = false;
        n.text_bg = Some(Color::WHITE);
        n.text_border = Some(Color::INK);
        n.text_shadow = true;
        n.text_style.align = Some(TextAlign::Right);
    });
    e.selection = vec![id];
    e.clear_text_formatting();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.label, "edge", "the text itself is kept");
    assert_eq!(n.label_color, None);
    assert_eq!(n.label_size, None);
    assert_eq!(n.text_bg, None);
    assert_eq!(n.text_border, None);
    assert!(!n.text_shadow);
    assert_eq!(n.text_style.align, None);
    e.undo();
    assert_eq!(
        e.doc.get(id).unwrap().label_color,
        Some(Color::rgb(0xff, 0x00, 0x00)),
        "clear formatting is one undo step"
    );
}

#[test]
fn labels_contrast_with_dark_fills() {
    let mut n = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0));
    n.style.set_fill(Some(Color::rgb(0x20, 0x20, 0x20)));
    n.label = "Dark".into();
    let l = text::layout_node(&n).unwrap();
    assert_eq!(l.glyphs[0].color, Color::WHITE);

    n.style.set_fill(Some(Color::rgb(0xf0, 0xf0, 0xf0)));
    let l = text::layout_node(&n).unwrap();
    assert_eq!(l.glyphs[0].color, Color::INK);
}

#[test]
fn frame_drags_its_children_by_its_title() {
    let mut e = ed();
    let frame = Node::new(
        NodeId(0),
        NodeKind::Frame { clip: false },
        Rect::new(100.0, 100.0, 400.0, 300.0),
    );
    let c = e.add_node(frame, None);
    let child = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(20.0, 20.0, 50.0, 50.0)),
        Some(c),
    );
    assert_eq!(e.doc.world_rect(child).unwrap().min(), v2(120.0, 120.0));
    let label = e.frame_labels()[0].1;
    drag(
        &mut e,
        (label.x + 5.0, label.y + 5.0),
        (label.x + 55.0, label.y + 75.0),
        0.0,
    );
    assert_eq!(e.doc.world_rect(c).unwrap().min(), v2(150.0, 170.0));
    assert_eq!(
        e.doc.world_rect(child).unwrap().min(),
        v2(170.0, 190.0),
        "child follows the frame"
    );
    e.doc.validate().unwrap();
}

#[test]
fn text_tool_types_edits_and_removes_empty() {
    let mut e = ed();
    press(&mut e, 't');
    down(&mut e, 100.0, 100.0, 0.0);
    up(&mut e, 100.0, 100.0, 10.0);
    let id = e.text_edit.as_ref().unwrap().id;
    e.handle(InputEvent::Text("Hello".into()));
    // Letters typed while editing must not trigger tool shortcuts.
    e.handle(InputEvent::KeyDown {
        key: Key::Char('r'),
        repeat: false,
    });
    e.handle(InputEvent::Text("r".into()));
    key(&mut e, Key::Enter, Modifiers::NONE);
    e.handle(InputEvent::Text("world".into()));
    key(&mut e, Key::Backspace, Modifiers::NONE);
    e.end_text_edit();
    assert_eq!(e.tool, Tool::Select);
    let NodeKind::Text { text, .. } = &e.doc.get(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text, "Hellor\nworl");
    let r = rect_of(&e, id);
    assert!(
        r.w > 30.0 && (r.h - 2.0 * text::line_height(16.0)).abs() < 1e-6,
        "{r:?}"
    );
    assert_eq!(
        e.history.undo_len(),
        1,
        "creating + typing is one undo step"
    );

    // Double-click to edit again; all text is selected.
    let (px, py) = (r.x + 4.0, r.y + text::line_height(16.0) * 0.5);
    down(&mut e, px, py, 1000.0);
    up(&mut e, px, py, 1010.0);
    down(&mut e, px, py, 1100.0);
    let te = e.text_edit.as_ref().unwrap();
    assert_eq!(te.anchor, Some(0));
    assert!(te.caret > 0);
    e.handle(InputEvent::Text(">".into()));
    // Typing replaced the whole selection, and Escape keeps it.
    let NodeKind::Text { text, .. } = &e.doc.get(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text, ">");
    key(&mut e, Key::Escape, Modifiers::NONE);
    let NodeKind::Text { text, .. } = &e.doc.get(id).unwrap().kind else {
        panic!()
    };
    assert_eq!(text, ">");

    // Empty text boxes disappear without leaving history.
    let before = e.history.undo_len();
    press(&mut e, 't');
    down(&mut e, 500.0, 500.0, 2000.0);
    up(&mut e, 500.0, 500.0, 2010.0);
    key(&mut e, Key::Escape, Modifiers::NONE);
    assert_eq!(e.doc.nodes.len(), 1);
    assert_eq!(e.history.undo_len(), before);
}

#[test]
fn superscript_and_subscript_toggle_on_text_and_undo() {
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "E = mc2".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 120.0, 24.0)),
        None,
    );
    e.selection = vec![id];
    e.toggle_superscript();
    assert_eq!(e.doc.get(id).unwrap().text_style.script, TextScript::Super);
    assert_eq!(e.history.undo_labels().last(), Some("Change script"));
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().text_style.script, TextScript::Normal);
    e.toggle_subscript();
    assert_eq!(e.doc.get(id).unwrap().text_style.script, TextScript::Sub);
    // Pressing the same button again returns the run to the baseline.
    e.toggle_subscript();
    assert_eq!(e.doc.get(id).unwrap().text_style.script, TextScript::Normal);
    e.toggle_superscript();
    // Switching directly from subscript to superscript replaces it.
    e.toggle_subscript();
    assert_eq!(e.doc.get(id).unwrap().text_style.script, TextScript::Sub);
}

#[test]
fn ctrl_arrows_jump_over_words_and_shift_selects_them() {
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "Man is smart".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 120.0, 24.0)),
        None,
    );
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    };
    let ctrl_shift = Modifiers {
        ctrl: true,
        ..Modifiers::SHIFT
    };
    e.start_text_edit(id, None); // selects all
    key(&mut e, Key::Right, Modifiers::NONE); // collapse to the end
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 12);

    // Ctrl+Left walks back one word at a time: smart, is, Man.
    key(&mut e, Key::Left, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 7);
    key(&mut e, Key::Left, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 4);
    key(&mut e, Key::Left, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 0);
    // Ctrl+Right jumps over the word and the space that follows it: "Man ".
    key(&mut e, Key::Right, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 4);
    key(&mut e, Key::Right, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 7);
    key(&mut e, Key::Right, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 12);

    // The same keys with Shift extend the selection by one word.
    key(&mut e, Key::Left, ctrl);
    key(&mut e, Key::Left, ctrl);
    key(&mut e, Key::Left, ctrl);
    assert_eq!(e.text_edit.as_ref().unwrap().caret, 0);
    key(&mut e, Key::Right, ctrl_shift);
    assert_eq!(e.text_selection(), Some((0, 4)), "extends over \"Man \"");
    key(&mut e, Key::Right, ctrl_shift);
    assert_eq!(e.text_selection(), Some((0, 7)), "extends over \"is \"");
}

#[test]
fn hover_over_selection_handles_shows_resize_cursors() {
    let mut e = ed();
    let a = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 100.0),
        ),
        None,
    );
    assert_eq!(e.hover_cursor(), None, "no selection, no handle cursor");
    e.select(vec![a]);
    e.pointer = v2(150.0, 150.0);
    assert_eq!(e.hover_cursor(), None, "the middle is not a handle");

    // Corners: ⤢ on the nw/se pair, ⤡ on the ne/sw pair.
    e.pointer = v2(100.0, 100.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NwSe));
    e.pointer = v2(200.0, 200.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NwSe));
    e.pointer = v2(200.0, 100.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NeSw));
    e.pointer = v2(100.0, 200.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NeSw));
    // Sides: ↕ on the top/bottom handles (they change the height), ⟷ on the
    // left/right ones (they change the width).
    e.pointer = v2(150.0, 100.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ns));
    e.pointer = v2(150.0, 200.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ns));
    e.pointer = v2(100.0, 150.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ew));
    e.pointer = v2(200.0, 150.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ew));

    // The shape stays while the resize drag runs.
    down(&mut e, 100.0, 150.0, 0.0);
    mv(&mut e, 40.0, 150.0, 16.0);
    assert!(matches!(e.gesture, Gesture::Resize { .. }));
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ew));
    up(&mut e, 40.0, 150.0, 32.0);
    e.pointer = v2(150.0, 150.0);
    assert_eq!(e.hover_cursor(), None);
}

#[test]
fn hovering_unselected_shape_shows_resize_handle_cursor() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![a]);
    // Handles of the other, unselected shape: b is 50x50 at (400, 300).
    e.hover = Some(b);
    // Corners.
    e.pointer = v2(400.0, 300.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NwSe));
    e.pointer = v2(450.0, 350.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NwSe));
    e.pointer = v2(450.0, 300.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NeSw));
    e.pointer = v2(400.0, 350.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::NeSw));
    // Sides: ↕ on the top/bottom handles, ⟷ on the left/right ones.
    e.pointer = v2(425.0, 300.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ns));
    e.pointer = v2(425.0, 350.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ns));
    e.pointer = v2(400.0, 325.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ew));
    e.pointer = v2(450.0, 325.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Ew));
    e.pointer = v2(425.0, 325.0);
    assert_eq!(e.hover_cursor(), None, "no cursor in the middle of b");

    // No cursor over handles of a locked shape.
    e.edit("Lock", |tx| {
        tx.node_mut(b).unwrap().locked = true;
    });
    e.pointer = v2(450.0, 325.0);
    assert_eq!(e.hover_cursor(), None);
    e.edit("Unlock", |tx| {
        tx.node_mut(b).unwrap().locked = false;
    });
}

#[test]
fn press_hover_knob_and_drag_connects_two_unselected_shapes() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    // b's left-side quick-connect knob sits 22px left of its edge (425 -> 396).
    e.hover = Some(b);
    down(&mut e, 378.0, 325.0, 0.0);
    assert!(
        matches!(e.gesture, Gesture::ConnectorEnd { .. }),
        "knob press starts the drag-out gesture"
    );
    assert_eq!(e.selection.len(), 1);
    let c = e.selection[0];
    assert_ne!(c, a);
    assert_ne!(c, b, "the arrow is selected, not the shapes");
    let start_of = |e: &Editor, id: NodeId| match &e.doc.get(id).unwrap().kind {
        NodeKind::Connector { start, .. } => start.as_ref().map(|bd| bd.node),
        _ => None,
    };
    assert_eq!(
        start_of(&e, c),
        Some(b),
        "the arrow starts at the hovered shape"
    );
    // Drag the free end onto `a` and release.
    mv(&mut e, 150.0, 150.0, 16.0);
    if let NodeKind::Connector {
        end: Some(endb), ..
    } = &e.doc.get(c).unwrap().kind
    {
        assert_eq!(endb.node, a, "the end binds to the shape under the drag");
    } else {
        panic!("connector should land bound to a");
    }
    up(&mut e, 150.0, 150.0, 32.0);
    assert_eq!(start_of(&e, c), Some(b));
    // Undo removes the arrow in one step, leaving both shapes untouched.
    e.undo();
    assert!(e.doc.get(c).is_none(), "arrow deleted");
    assert!(e.doc.get(a).is_some() && e.doc.get(b).is_some());
}

#[test]
fn press_on_hovered_unselected_handle_resizes_without_selecting() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    // b grows 50px wider by dragging its right-side handle.
    e.hover = Some(b);
    down(&mut e, 450.0, 325.0, 0.0);
    assert!(matches!(e.gesture, Gesture::Resize { .. }));
    mv(&mut e, 500.0, 325.0, 16.0);
    assert!(
        e.selection.is_empty(),
        "hover resize does not change selection"
    );
    up(&mut e, 500.0, 325.0, 32.0);
    let rb = rect_of(&e, b);
    assert_eq!(rb.w, 100.0);
    assert_eq!(rb.h, 50.0, "side handle only changes the width");
    assert!(
        rect_of(&e, a).min().dist(v2(100.0, 100.0)) < 1e-9,
        "a untouched"
    );
}

#[test]
fn resizing_magnetizes_only_to_the_grid() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![b]);
    // Drag b's left edge toward a's right edge (x = 200): no element magnet,
    // the edge falls to the grid (10) instead and no guide appears.
    down(&mut e, 400.0, 325.0, 0.0);
    mv(&mut e, 193.0, 325.0, 16.0);
    let r = rect_of(&e, b);
    assert_eq!(r.x, 190.0, "left edge snapped to the grid, not to a");
    assert_eq!(r.right(), 450.0, "the opposite edge stays put");
    assert!(e.guides.is_empty(), "no element guides while resizing");
    let _ = a;
    up(&mut e, 193.0, 325.0, 32.0);
}

#[test]
fn creating_magnetizes_only_to_the_grid() {
    let mut e = ed();
    e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 100.0),
        ),
        None,
    );
    // The dragged end that would have landed on the sibling's right edge
    // (x = 200) falls to the grid instead and no guide appears.
    press(&mut e, 'r');
    drag(&mut e, (180.0, 120.0), (193.4, 160.0), 0.0);
    let b = e.selection[0];
    let nb = rect_of(&e, b);
    assert_eq!(nb.right(), 190.0, "dragged edge snapped to the grid");
    assert_eq!(nb.x, 180.0, "start corner stays put");
    assert!(e.guides.is_empty(), "no element guides while creating");
    assert!(e.doc.get(b).unwrap().parent.is_none());
}

#[test]
fn creating_uses_grid_when_no_anchor_hit() {
    let mut e = ed();
    // Sibling far away so no magnet anchor is within reach at the end point.
    e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(600.0, 500.0, 50.0, 50.0),
        ),
        None,
    );
    e.grid_step = 20.0;
    press(&mut e, 'r');
    // End off-grid: x 228.4 snaps to the 20 grid (220), y 170 to 180.
    drag(&mut e, (200.0, 120.0), (228.4, 170.0), 0.0);
    let nb = rect_of(&e, e.selection[0]);
    assert_eq!(nb.right(), 220.0, "end edge fell back to the grid");
    assert_eq!(nb.bottom(), 180.0, "bottom fell back to the grid");
    assert_eq!(nb.x, 200.0, "start corner stays on the grid");
    assert!(e.guides.is_empty());
}

#[test]
fn resizing_uses_grid_when_no_anchor_hit() {
    let mut e = ed();
    let (_, b) = two_rects(&mut e);
    e.grid_step = 20.0;
    e.select(vec![b]);
    // Drag the right edge to an off-grid spot with no anchor nearby.
    down(&mut e, 446.0, 325.0, 0.0);
    mv(&mut e, 453.2, 325.0, 16.0);
    let r = rect_of(&e, b);
    assert_eq!(r.right(), 460.0, "dragged edge fell back to the grid");
    assert_eq!(r.x, 400.0, "the opposite edge stays put");
    assert!(e.guides.is_empty());
    // Resizing a corner likewise snaps only the dragged edges.
    up(&mut e, 453.2, 325.0, 32.0);
    e.select(vec![b]);
    down(&mut e, 453.0, 353.0, 100.0);
    mv(&mut e, 433.0, 359.4, 116.0);
    let r = rect_of(&e, b);
    assert_eq!(r.x, 400.0, "left edge stays put");
    assert_eq!(r.y, 300.0, "top edge stays put");
    assert_eq!(r.right(), 440.0, "dragged right edge snapped to the grid");
    assert_eq!(r.bottom(), 360.0, "dragged bottom snapped to the grid");
}

#[test]
fn creating_inside_frame_snaps_to_the_grid_not_the_sibling() {
    let mut e = ed();
    press(&mut e, 'r');
    drag(&mut e, (120.0, 120.0), (160.0, 160.0), 0.0);
    press(&mut e, 'f');
    drag(&mut e, (100.0, 100.0), (400.0, 300.0), 500.0);
    // A second rect drawn inside the frame snaps its dragged end to the grid
    // (10), not onto the sibling's right edge (x = 160).
    press(&mut e, 'r');
    drag(&mut e, (180.0, 120.0), (166.0, 220.0), 900.0);
    let nb = rect_of(&e, e.selection[0]);
    assert_eq!(nb.x, 170.0, "dragged end snapped to the grid");
    assert_eq!(nb.right(), 180.0, "start corner stays put");
}

// ---------------------------------------------------------------- M4: select, move, resize, snap

fn two_rects(e: &mut Editor) -> (NodeId, NodeId) {
    let a = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 100.0),
        ),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(400.0, 300.0, 50.0, 50.0),
        ),
        None,
    );
    (a, b)
}

#[test]
fn marquee_selects_children_of_grazed_frame_but_not_the_frame() {
    let mut e = ed();
    press(&mut e, 'r');
    drag(&mut e, (120.0, 120.0), (160.0, 160.0), 0.0);
    let inner = e.selection[0];
    press(&mut e, 'f');
    drag(&mut e, (100.0, 100.0), (400.0, 300.0), 500.0);
    let frame = e.selection[0];
    assert!(e.doc.get(frame).unwrap().is_frame());
    assert_eq!(e.doc.get(inner).unwrap().parent, Some(frame));
    press(&mut e, 'r');
    drag(&mut e, (300.0, 200.0), (350.0, 250.0), 900.0);
    let child = e.selection[0];
    assert_eq!(e.doc.get(child).unwrap().parent, Some(frame));
    press(&mut e, 'v'); // back to Select for the marquee drags
    e.select(Vec::new());

    // The marquee only grazes the frame (it is not fully enclosed), so the
    // frame itself is not selected — contained children inside it are. The
    // rectangle holds `inner` whole but only clips `child`'s corner, so
    // only `inner` is picked up.
    drag(&mut e, (40.0, 40.0), (330.0, 230.0), 2000.0);
    assert!(!e.selection.contains(&frame), "grazed frame not selected");
    assert_eq!(e.selection, vec![inner], "only fully contained nodes");

    // A marquee that fully contains one child of the frame selects only that
    // child (starting outside the frame: dragging from its body moves it).
    e.select(Vec::new());
    drag(&mut e, (495.0, 260.0), (290.0, 190.0), 3000.0);
    assert_eq!(e.selection, vec![child]);

    // Enclosing the whole frame selects the frame itself.
    e.select(Vec::new());
    drag(&mut e, (50.0, 50.0), (450.0, 350.0), 4000.0);
    assert_eq!(e.selection, vec![frame]);

    // A locked child stays out of the marquee.
    e.edit("Lock", |tx| {
        tx.node_mut(child).unwrap().locked = true;
    });
    e.select(Vec::new());
    drag(&mut e, (495.0, 260.0), (290.0, 190.0), 5000.0);
    assert!(e.selection.is_empty(), "locked child not selected");
}

#[test]
fn marquee_selects_only_fully_contained_nodes() {
    let mut e = ed();
    let (_a, _b) = two_rects(&mut e);
    // a = (100,100)-(200,200), b = (400,300)-(450,350).

    // A rectangle that clips a's corner but does not hold it whole selects
    // nothing, even though it intersects. It starts on empty canvas so it
    // becomes a marquee, not a move.
    e.select(Vec::new());
    drag(&mut e, (80.0, 80.0), (180.0, 180.0), 0.0);
    assert!(e.selection.is_empty(), "partial overlap must not select");

    // The rectangle spans both nodes exactly, so both are contained.
    e.select(Vec::new());
    drag(&mut e, (90.0, 90.0), (460.0, 360.0), 1000.0);
    assert_eq!(e.selection.len(), 2, "exact bounds contain both");

    // The rectangle vanishes when the drag ends.
    assert!(e.marquee.is_none());
}

#[test]
fn click_shift_click_and_marquee_selection() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    down(&mut e, 150.0, 150.0, 0.0);
    up(&mut e, 150.0, 150.0, 10.0);
    assert_eq!(e.selection, vec![a]);
    e.handle(InputEvent::Modifiers(Modifiers::SHIFT));
    down(&mut e, 420.0, 320.0, 1000.0);
    up(&mut e, 420.0, 320.0, 1010.0);
    assert_eq!(e.selection, vec![a, b]);
    down(&mut e, 420.0, 320.0, 2000.0);
    up(&mut e, 420.0, 320.0, 2010.0);
    assert_eq!(e.selection, vec![a], "shift-click toggles off");
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
    down(&mut e, 700.0, 700.0, 3000.0);
    up(&mut e, 700.0, 700.0, 3010.0);
    assert!(e.selection.is_empty(), "click on empty canvas clears");
    drag(&mut e, (50.0, 50.0), (460.0, 360.0), 4000.0);
    assert_eq!(e.selection, vec![a, b]);
    assert!(e.marquee.is_none());
    key(&mut e, Key::Char('a'), Modifiers::cmd());
    assert_eq!(e.selection.len(), 2);
}

#[test]
fn drag_moves_multi_selection_as_one_undo_step() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![a, b]);
    e.snapping = false;
    drag(&mut e, (150.0, 150.0), (180.7, 140.2), 0.0);
    assert!(rect_of(&e, a).min().dist(v2(130.7, 90.2)) < 1e-9);
    assert!(rect_of(&e, b).min().dist(v2(430.7, 290.2)) < 1e-9);
    e.undo();
    assert_eq!(rect_of(&e, a).min(), v2(100.0, 100.0));
    assert_eq!(rect_of(&e, b).min(), v2(400.0, 300.0));
    e.redo();
    assert!(rect_of(&e, b).min().dist(v2(430.7, 290.2)) < 1e-9);
}

#[test]
fn move_snaps_centers_into_rows_and_columns() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![b]);
    // b's center (425, 325) dragged to x ~150 lands within 6 px of a's center
    // column (150): the center snaps, giving the same column.
    down(&mut e, 410.0, 310.0, 0.0);
    mv(&mut e, 300.0, 310.0, 16.0);
    mv(&mut e, 135.4, 310.0, 32.0);
    assert!(
        e.guides
            .iter()
            .any(|g| matches!(g, snap::Guide::V { x, .. } if *x == 150.0)),
        "column guide through both centers, guides={:?}",
        e.guides
    );
    up(&mut e, 135.4, 310.0, 48.0);
    let r = rect_of(&e, b);
    assert_eq!(r.center().x, 150.0, "center aligned to a's column");
    assert!(e.guides.is_empty());
    let _ = a;
}

#[test]
fn move_does_not_magnet_to_edges_anymore() {
    let mut e = ed();
    let (_a, b) = two_rects(&mut e);
    e.select(vec![b]);
    // b's left edge (400 -> 193) passes a's right edge (200) but the centers
    // stay far apart: no element magnet, the move falls back to the grid.
    down(&mut e, 410.0, 310.0, 0.0);
    mv(&mut e, 300.0, 310.0, 16.0);
    mv(&mut e, 203.0, 150.4, 32.0);
    assert!(e.guides.is_empty(), "no edge guides, guides={:?}", e.guides);
    up(&mut e, 203.0, 150.4, 48.0);
    let r = rect_of(&e, b);
    assert_eq!(r.x, 190.0, "min corner on the grid, not on a's edge");
}

#[test]
fn move_snaps_to_the_grid() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(103.0, 97.0, 60.0, 40.0),
        ),
        None,
    );
    drag(&mut e, (130.0, 115.0), (148.4, 132.8), 0.0);
    assert_eq!(rect_of(&e, id).min(), v2(120.0, 110.0));
}

#[test]
fn grid_size_option_changes_move_fallback() {
    let mut e = ed();
    e.grid_step = 20.0;
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(103.0, 97.0, 60.0, 40.0),
        ),
        None,
    );
    // No siblings nearby: away from anchors, moves fall back to the configured step.
    drag(&mut e, (130.0, 115.0), (148.4, 132.8), 0.0);
    assert_eq!(rect_of(&e, id).min(), v2(120.0, 120.0));
}

#[test]
fn default_style_remembered_for_new_shapes_but_not_frames() {
    let mut e = ed();
    let src = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 60.0),
        ),
        None,
    );
    e.edit("style", |tx| {
        let n = tx.node_mut(src).unwrap();
        n.style.fills = vec![Paint::solid(Color::rgb(255, 0, 0))];
        n.style.stroke = Some(Color::rgb(0, 0, 255));
        n.style.corner_radius = 8.0;
    });
    e.selection = vec![src];
    e.set_default_style();

    // A new shape takes on the remembered style...
    press(&mut e, 'r');
    drag(&mut e, (210.0, 120.0), (280.0, 180.0), 0.0);
    let new_id = e.selection[0];
    let st = &e.doc.get(new_id).unwrap().style;
    assert!(
        matches!(st.fills.last(), Some(Paint::Solid { color, .. }) if *color == Color::rgb(255, 0, 0)),
        "fills carry over, {:?}",
        st.fills
    );
    assert!(st.stroke == Some(Color::rgb(0, 0, 255)));
    assert_eq!(st.corner_radius, 8.0);

    // ...but a frame stays transparent (no remembered fill).
    press(&mut e, 'f');
    drag(&mut e, (400.0, 400.0), (560.0, 520.0), 100.0);
    let frame_id = e.selection[0];
    assert!(e.doc.get(frame_id).unwrap().is_frame());
    assert!(
        !matches!(
            e.doc.get(frame_id).unwrap().style.fills.last(),
            Some(Paint::Solid { color, .. }) if *color == Color::rgb(255, 0, 0)
        ),
        "frames skip the remembered fill"
    );
}

#[test]
fn source_connector_default_is_one_undo_step_and_wins_over_the_page_default() {
    use crate::model::{DefaultStyle, Style, TextStyle};
    let mut e = ed();
    let src = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 120.0, 60.0),
        ),
        None,
    );
    let other = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(400.0, 100.0, 120.0, 60.0),
        ),
        None,
    );
    let page_default = DefaultStyle::new(
        Style {
            stroke: Some(Color::rgb(0, 0, 255)),
            stroke_width: 3.0,
            ..Style::default()
        },
        TextStyle::default(),
    );
    e.edit_settings("page connector default", |s| {
        s.connector_default = Some(page_default.clone());
    });
    e.set_connector_default_from_page(&[src]);
    assert_eq!(
        e.doc.get(src).unwrap().connector_default,
        Some(page_default.clone())
    );
    e.undo();
    assert_eq!(e.doc.get(src).unwrap().connector_default, None);
    e.redo();
    assert_eq!(
        e.doc.get(src).unwrap().connector_default,
        Some(page_default.clone())
    );

    let source_default = DefaultStyle::new(
        Style {
            stroke: Some(Color::rgb(255, 0, 0)),
            stroke_width: 5.0,
            ..Style::default()
        },
        TextStyle {
            bold: true,
            ..TextStyle::default()
        },
    );
    e.edit("source default", |tx| {
        tx.node_mut(src).unwrap().connector_default = Some(source_default.clone());
    });

    e.tool = Tool::Arrow;
    down(&mut e, 150.0, 130.0, 0.0);
    mv(&mut e, 320.0, 130.0, 16.0);
    up(&mut e, 320.0, 130.0, 32.0);
    let from_source = e.selection[0];
    let n = e.doc.get(from_source).unwrap();
    assert_eq!(n.style, source_default.style);
    assert_eq!(n.text_style, source_default.text_style);
    assert!(matches!(
        &n.kind,
        NodeKind::Connector {
            start: Some(b),
            ..
        } if b.node == src
    ));

    e.tool = Tool::Arrow;
    down(&mut e, 450.0, 130.0, 100.0);
    mv(&mut e, 620.0, 130.0, 116.0);
    up(&mut e, 620.0, 130.0, 132.0);
    let from_other = e.selection[0];
    assert_eq!(e.doc.get(from_other).unwrap().style, page_default.style);
    assert_eq!(e.doc.get(other).unwrap().connector_default, None);
}

#[test]
fn quick_connect_inherits_the_hovered_shapes_connector_default() {
    use crate::model::{DefaultStyle, Style};
    let mut e = ed();
    let (_a, b) = two_rects(&mut e);
    let ds = DefaultStyle::new(
        Style {
            stroke: Some(Color::rgb(0x0a, 0x0b, 0x0c)),
            stroke_width: 6.0,
            ..Style::default()
        },
        crate::model::TextStyle::default(),
    );
    e.edit("source default", |tx| {
        tx.node_mut(b).unwrap().connector_default = Some(ds.clone());
    });
    e.hover = Some(b);
    // b's left quick-connect knob sits 22px left of its edge (425 -> 396).
    down(&mut e, 378.0, 325.0, 0.0);
    let arrow = e.selection[0];
    let n = e.doc.get(arrow).unwrap();
    assert_eq!(n.style, ds.style);
    assert_eq!(n.text_style, ds.text_style);
    assert!(matches!(
        &n.kind,
        NodeKind::Connector { start: Some(binding), .. } if binding.node == b
    ));
}

#[test]
fn connector_style_copies_to_its_source_shape_and_clears_in_one_step() {
    use crate::model::{DefaultStyle, Style};
    let mut e = ed();
    let src = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 120.0, 60.0),
        ),
        None,
    );
    e.tool = Tool::Arrow;
    down(&mut e, 150.0, 130.0, 0.0);
    mv(&mut e, 320.0, 130.0, 16.0);
    up(&mut e, 320.0, 130.0, 32.0);
    let arrow = e.selection[0];
    let styled = DefaultStyle::new(
        Style {
            stroke: Some(Color::rgb(0x12, 0x34, 0x56)),
            stroke_width: 7.0,
            ..Style::default()
        },
        crate::model::TextStyle::default(),
    );
    e.edit("style arrow", |tx| {
        let n = tx.node_mut(arrow).unwrap();
        n.style = styled.style.clone();
        n.text_style = styled.text_style.clone();
    });
    e.selection = vec![arrow];
    e.set_connector_default_from_selection();
    assert_eq!(e.doc.get(src).unwrap().connector_default, Some(styled));
    assert_eq!(
        e.history.undo_labels().last(),
        Some("Set source connector default")
    );

    e.selection = vec![src];
    e.clear_connector_defaults(&[src]);
    assert_eq!(e.doc.get(src).unwrap().connector_default, None);
    e.undo();
    assert!(e.doc.get(src).unwrap().connector_default.is_some());
    e.redo();
    assert_eq!(e.doc.get(src).unwrap().connector_default, None);
}

#[test]
fn word_wrap_toggles_fixed_width_and_refits() {
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "the quick brown fox jumps over the lazy dog".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 300.0, 24.0)),
        None,
    );
    // add_node() fits the width to the content; narrow the box so the text
    // has something to wrap at.
    e.edit("narrow", |tx| {
        tx.node_mut(id).unwrap().rect.w = 200.0;
    });
    e.select(vec![id]);
    e.set_word_wrap(&[id], true);
    let n = e.doc.get(id).unwrap();
    let NodeKind::Text { auto_width, .. } = &n.kind else {
        panic!("not text");
    };
    assert!(!*auto_width, "wrap keeps a fixed line width");
    assert!(n.rect.h > 30.0, "now two lines, {:?}", n.rect);

    // Turning wrap off grows the box back to one line.
    e.set_word_wrap(&[id], false);
    let n = e.doc.get(id).unwrap();
    let NodeKind::Text { auto_width, .. } = &n.kind else {
        panic!("not text");
    };
    assert!(*auto_width);
    assert!(
        n.rect.w > 300.0 && n.rect.h < 30.0,
        "single line again, {:?}",
        n.rect
    );
}

#[test]
fn clear_formatting_strips_character_overrides() {
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "Hello big styled text".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 300.0, 24.0)),
        None,
    );
    e.select(vec![id]);
    // Emulate a selection-wide bold + red size override.
    let len = "Hello big styled text".chars().count();
    {
        let mut tx = e.tx();
        let n = tx.node_mut(id).unwrap();
        n.text_style.apply_range(0, len, |sp| {
            sp.bold = Some(true);
            sp.size = Some(24.0);
        });
    }
    {
        let mut tx = e.tx();
        let n = tx.node_mut(id).unwrap();
        n.text_style.bold = true;
    }
    e.clear_text_formatting();
    let n = e.doc.get(id).unwrap();
    assert!(n.text_style.spans.is_empty(), "spans stripped");
    assert!(!n.text_style.bold);
}

#[test]
fn bulleted_list_stores_metadata_and_outdent_joins_undo() {
    use crate::model::ListKind;
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "one\ntwo\nthree".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 200.0, 60.0)),
        None,
    );
    e.select(vec![id]);
    e.set_list_paragraphs(ListKind::Bullet);
    let style = &e.doc.get(id).unwrap().text_style;
    assert_eq!(style.paragraphs.len(), 3);
    assert!(style.paragraphs.iter().all(|p| p.list == ListKind::Bullet));
    e.set_paragraph_indent(1, 1);
    assert_eq!(e.doc.get(id).unwrap().text_style.paragraphs[1].indent, 1);
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().text_style.paragraphs[1].indent, 0);
    e.redo();
    assert_eq!(e.doc.get(id).unwrap().text_style.paragraphs[1].indent, 1);
}

#[test]
fn active_paragraph_follows_caret_and_selection_start() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Text {
                text: "one\ntwo\nthree".into(),
                font_size: 16.0,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(100.0, 100.0, 200.0, 60.0),
        ),
        None,
    );
    e.text_edit = Some(TextEdit {
        id,
        label: 0,
        caret: 8,
        anchor: Some(5),
    });
    assert_eq!(e.active_paragraph(id), 1);
    e.text_edit = Some(TextEdit {
        id,
        label: 0,
        caret: 10,
        anchor: None,
    });
    assert_eq!(e.active_paragraph(id), 2);
    e.text_edit = None;
    assert_eq!(e.active_paragraph(id), 0);
}

#[test]
fn text_padding_grows_box_and_offsets_content() {
    let mut e = ed();
    let kind = NodeKind::Text {
        text: "wraps when the column narrows enough".into(),
        font_size: 16.0,
        auto_width: true,
        align: TextAlign::Left,
    };
    let id = e.add_node(
        Node::new(NodeId(0), kind, Rect::new(100.0, 100.0, 300.0, 24.0)),
        None,
    );
    let bare = e.doc.get(id).unwrap().rect;
    e.edit("pad", |tx| {
        tx.node_mut(id).unwrap().pad = crate::model::Pad {
            left: 20.0,
            top: 10.0,
            right: 30.0,
            bottom: 5.0,
        };
    });
    e.fit_text(id);
    let padded = e.doc.get(id).unwrap().rect;
    let l = crate::text::layout_node(e.doc.get(id).unwrap()).unwrap();
    assert!(
        (padded.w - (l.width + 50.0)).abs() < 1e-9,
        "width grows by the side pads, {:?} vs {}",
        padded,
        l.width
    );
    assert!(
        (padded.h - (bare.h + 15.0)).abs() < 1e-9,
        "height grows by the top/bottom pads: {:?} vs {:?}",
        padded,
        bare
    );
    assert_eq!(
        crate::text::text_origin(e.doc.get(id).unwrap(), &l),
        v2(20.0, 10.0),
        "content starts at the left/top pad"
    );

    // With wrap on, the line column insets: same box width, fewer words per
    // line than without padding.
    e.set_word_wrap(&[id], true);
    let padded = e.doc.get(id).unwrap().rect;
    e.edit("pad more", |tx| {
        tx.node_mut(id).unwrap().pad.right = 200.0;
    });
    e.fit_text(id);
    let n = e.doc.get(id).unwrap();
    let l = crate::text::layout_node(n).unwrap();
    assert!(
        (n.rect.h - (l.height + 15.0)).abs() < 1e-9,
        "height tracks the wrapped lines plus pads, {:?} vs {}",
        n.rect,
        l.height
    );
    assert!(n.rect.h > padded.h, "more padding, taller wrapped box");
}

#[test]
fn stickies_keep_their_fixed_inset() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Sticky {
                text: "note".into(),
                font_size: 16.0,
            },
            Rect::new(100.0, 100.0, 120.0, 120.0),
        ),
        None,
    );
    let n = e.doc.get(id).unwrap();
    let l = crate::text::layout_node(n).unwrap();
    assert_eq!(
        crate::text::text_origin(n, &l),
        v2(crate::text::STICKY_PAD, crate::text::STICKY_PAD),
        "sticky notes inset by their own constant, not the pad"
    );
}

#[test]
fn valign_keeps_height_and_offsets_the_box() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Text {
                text: "hello".into(),
                font_size: 16.0,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(100.0, 100.0, 60.0, 20.0),
        ),
        None,
    );
    // Make the box taller than its content, then center vertically.
    e.edit("resize", |tx| {
        tx.node_mut(id).unwrap().rect.h = 60.0;
    });
    e.edit("valign", |tx| {
        tx.node_mut(id).unwrap().valign = crate::model::VAlign::Middle;
    });
    let n = e.doc.get(id).unwrap();
    let l = crate::text::layout_node(n).unwrap();
    assert_eq!(n.rect.h, 60.0, "Middle/Bottom keeps the user-set height");
    let extra = 60.0 - n.pad.top - n.pad.bottom - l.height;
    assert!(
        extra > 10.0,
        "box taller than content: h {} vs {}",
        n.rect.h,
        l.height
    );
    let mid = crate::text::text_origin(n, &l);
    assert!(
        (mid.y - extra * 0.5).abs() < 1e-9,
        "middle centers the content; origin y {} vs extra {}",
        mid.y,
        extra
    );

    e.edit("valign bottom", |tx| {
        tx.node_mut(id).unwrap().valign = crate::model::VAlign::Bottom;
    });
    let n = e.doc.get(id).unwrap();
    let l = crate::text::layout_node(n).unwrap();
    let bot = crate::text::text_origin(n, &l);
    assert!(
        (bot.y - mid.y - extra * 0.5).abs() < 1e-9,
        "bottom = middle + extra/2; {} vs {}",
        bot.y,
        mid.y
    );

    // Back to Top re-fits the height to the content.
    e.edit("valign top", |tx| {
        tx.node_mut(id).unwrap().valign = crate::model::VAlign::Top;
    });
    e.fit_text(id);
    let n = e.doc.get(id).unwrap();
    let l = crate::text::layout_node(n).unwrap();
    assert!(
        (n.rect.h - l.height).abs() < 1e-9,
        "Top auto-fits again, h {} vs content {}",
        n.rect.h,
        l.height
    );
    assert_eq!(crate::text::text_origin(n, &l), v2(0.0, 0.0));
}

#[test]
fn writing_direction_overrides_paragraph_bidi() {
    // "Hi Ally." under RTL: the trailing dot resolves to the paragraph
    // level and moves to the LEFT edge, while the Latin run keeps its
    // order; under LTR (and auto, which detects LTR) the dot trails.
    let mk = |dir: crate::model::WritingDir| {
        let mut n = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: "Hi Ally.".into(),
                font_size: 16.0,
                auto_width: false,
                align: TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 300.0, 24.0),
        );
        n.dir = dir;
        crate::text::layout_node(&n).unwrap()
    };
    let ltr = mk(crate::model::WritingDir::Ltr);
    let rtl = mk(crate::model::WritingDir::Rtl);
    assert_eq!(ltr.glyphs.len(), 8, "one glyph per character");
    assert_eq!(ltr.glyphs[0].char_index, 0, "H first under LTR");
    assert_eq!(
        ltr.glyphs.last().unwrap().char_index,
        7,
        "dot last under LTR"
    );
    assert_eq!(rtl.glyphs.last().unwrap().char_index, 6, "y still last");
    assert_eq!(
        rtl.glyphs[0].char_index, 7,
        "the neutral dot leads the line under RTL"
    );
    // LTR and Automatic agree on pure-Latin content.
    let auto = mk(crate::model::WritingDir::Auto);
    assert_eq!(
        auto.glyphs.iter().map(|g| g.gid).collect::<Vec<_>>(),
        ltr.glyphs.iter().map(|g| g.gid).collect::<Vec<_>>()
    );
}

#[test]
fn resize_handles_with_shift_and_multi_scale() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![a]);
    e.snapping = false;
    // Bottom-right handle of a at (200, 200).
    drag(&mut e, (200.0, 200.0), (300.0, 250.0), 0.0);
    assert_eq!(rect_of(&e, a), Rect::new(100.0, 100.0, 200.0, 150.0));
    // Shift keeps aspect ratio (4:3).
    e.handle(InputEvent::Modifiers(Modifiers::SHIFT));
    drag(&mut e, (300.0, 250.0), (500.0, 260.0), 1000.0);
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
    let r = rect_of(&e, a);
    assert!((r.w / r.h - 200.0 / 150.0).abs() < 1e-9, "{r:?}");
    e.undo();
    e.undo();
    // Multi-selection scales both proportionally from the bounds' top-left handle.
    e.select(vec![a, b]);
    let b0 = e.selection_bounds().unwrap();
    assert_eq!(b0, Rect::new(100.0, 100.0, 350.0, 250.0));
    drag(&mut e, (450.0, 350.0), (800.0, 600.0), 2000.0);
    assert_eq!(
        e.selection_bounds().unwrap(),
        Rect::new(100.0, 100.0, 700.0, 500.0)
    );
    assert_eq!(rect_of(&e, b), Rect::new(700.0, 500.0, 100.0, 100.0));
}

#[test]
fn nudge_delete_duplicate_copy_paste_and_order() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![a]);
    key(&mut e, Key::Right, Modifiers::NONE);
    key(&mut e, Key::Down, Modifiers::SHIFT);
    assert_eq!(rect_of(&e, a).min(), v2(101.0, 110.0));
    let n = e.history.undo_len();
    key(&mut e, Key::Right, Modifiers::NONE);
    assert_eq!(e.history.undo_len(), n, "consecutive nudges coalesce");

    key(&mut e, Key::Char('d'), Modifiers::cmd());
    let dup = e.selection[0];
    assert_ne!(dup, a);
    assert_eq!(rect_of(&e, dup).min(), v2(118.0, 126.0));
    assert_eq!(e.doc.root, vec![a, b, dup]);
    key(&mut e, Key::Char('['), Modifiers::cmd_shift());
    assert_eq!(e.doc.root, vec![dup, a, b]);
    key(&mut e, Key::Char(']'), Modifiers::cmd());
    assert_eq!(e.doc.root, vec![a, dup, b]);

    key(&mut e, Key::Char('c'), Modifiers::cmd());
    key(&mut e, Key::Char('v'), Modifiers::cmd());
    key(&mut e, Key::Char('v'), Modifiers::cmd());
    assert_eq!(e.doc.nodes.len(), 5);
    key(&mut e, Key::Delete, Modifiers::NONE);
    assert_eq!(e.doc.nodes.len(), 4);
    assert!(e.selection.is_empty());
    e.doc.validate().unwrap();
}

#[test]
fn copy_paste_frame_with_children_and_undo() {
    let mut e = ed();
    let mut f = Node::new(
        NodeId(0),
        NodeKind::Frame { clip: true },
        Rect::new(0.0, 0.0, 200.0, 200.0),
    );
    f.name = "F".into();
    let f = e.add_node(f, None);
    e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(10.0, 10.0, 20.0, 20.0)),
        Some(f),
    );
    e.select(vec![f]);
    key(&mut e, Key::Char('c'), Modifiers::cmd());
    key(&mut e, Key::Char('v'), Modifiers::cmd());
    let copy = e.selection[0];
    assert_eq!(e.doc.get(copy).unwrap().children.len(), 1);
    assert_eq!(e.doc.nodes.len(), 4);
    e.doc.validate().unwrap();
    e.undo();
    assert_eq!(e.doc.nodes.len(), 2);
    e.doc.validate().unwrap();
}

#[test]
fn dragging_into_and_out_of_frames_reparents() {
    let mut e = ed();
    let f = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: true },
            Rect::new(300.0, 300.0, 300.0, 300.0),
        ),
        None,
    );
    let r = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(50.0, 50.0, 40.0, 40.0)),
        None,
    );
    e.snapping = false;
    drag(&mut e, (70.0, 70.0), (420.0, 420.0), 0.0);
    assert_eq!(e.doc.get(r).unwrap().parent, Some(f));
    assert_eq!(rect_of(&e, r).min(), v2(400.0, 400.0));
    drag(&mut e, (420.0, 420.0), (70.0, 70.0), 1000.0);
    assert_eq!(e.doc.get(r).unwrap().parent, None);
    e.undo();
    assert_eq!(e.doc.get(r).unwrap().parent, Some(f));
    e.doc.validate().unwrap();
}

#[test]
fn locked_and_hidden_nodes_are_not_hit() {
    let mut e = ed();
    let (a, _) = two_rects(&mut e);
    e.select(vec![a]);
    key(&mut e, Key::Char('l'), Modifiers::cmd_shift());
    assert!(e.doc.get(a).unwrap().locked);
    down(&mut e, 150.0, 150.0, 0.0);
    up(&mut e, 150.0, 150.0, 10.0);
    assert!(e.selection.is_empty());
    e.edit("unlock", |tx| tx.node_mut(a).unwrap().locked = false);
    e.select(vec![a]);
    key(&mut e, Key::Char('h'), Modifiers::cmd_shift());
    assert!(!e.doc.get(a).unwrap().visible);
    assert_eq!(hit::hit_test(&e.doc, v2(150.0, 150.0), 1.0), None);
}

#[test]
fn escape_cancels_a_drag_in_progress() {
    let mut e = ed();
    let (a, _) = two_rects(&mut e);
    e.select(vec![a]);
    down(&mut e, 150.0, 150.0, 0.0);
    mv(&mut e, 250.0, 250.0, 16.0);
    assert_ne!(rect_of(&e, a).min(), v2(100.0, 100.0));
    key(&mut e, Key::Escape, Modifiers::NONE);
    assert_eq!(rect_of(&e, a).min(), v2(100.0, 100.0));
    up(&mut e, 250.0, 250.0, 32.0);
    assert_eq!(rect_of(&e, a).min(), v2(100.0, 100.0));
}

// ---------------------------------------------------------------- M5: layers + undo/redo

#[test]
fn undo_redo_round_trips_a_whole_session() {
    let mut e = ed();
    let mut snapshots = vec![e.doc.clone()];
    press(&mut e, 'f');
    drag(&mut e, (0.0, 0.0), (500.0, 400.0), 0.0);
    snapshots.push(e.doc.clone());
    press(&mut e, 'r');
    drag(&mut e, (50.0, 50.0), (150.0, 150.0), 1000.0);
    snapshots.push(e.doc.clone());
    press(&mut e, 'b');
    drag(&mut e, (200.0, 200.0), (300.0, 260.0), 2000.0);
    snapshots.push(e.doc.clone());
    // Esc: pencil -> select tool; Esc: rect -> parent frame; Cmd+A then selects top level.
    key(&mut e, Key::Escape, Modifiers::NONE);
    key(&mut e, Key::Escape, Modifiers::NONE);
    key(&mut e, Key::Char('a'), Modifiers::cmd());
    key(&mut e, Key::Right, Modifiers::SHIFT);
    snapshots.push(e.doc.clone());
    key(
        &mut e,
        Key::Char('g'),
        Modifiers {
            alt: true,
            ..Modifiers::cmd()
        },
    );
    snapshots.push(e.doc.clone());
    key(&mut e, Key::Delete, Modifiers::NONE);
    snapshots.push(e.doc.clone());
    assert!(e.doc.nodes.is_empty());

    for s in snapshots.iter().rev().skip(1) {
        key(&mut e, Key::Char('z'), Modifiers::cmd());
        assert_eq!(&e.doc, s);
    }
    for s in snapshots.iter().skip(1) {
        key(&mut e, Key::Char('z'), Modifiers::cmd_shift());
        assert_eq!(&e.doc, s);
        e.doc.validate().unwrap();
    }
    assert!(e.is_dirty());
    e.mark_saved();
    assert!(!e.is_dirty());
    e.undo();
    assert!(e.is_dirty());
}

#[test]
fn wrap_in_frame_groups_siblings() {
    let mut e = ed();
    let (a, b) = two_rects(&mut e);
    e.select(vec![a, b]);
    e.run(shortcuts::Action::WrapInFrame);
    let f = e.selection[0];
    assert_eq!(e.doc.root, vec![f]);
    assert_eq!(e.doc.get(f).unwrap().children, vec![a, b]);
    assert_eq!(rect_of(&e, f), Rect::new(100.0, 100.0, 350.0, 250.0));
    assert_eq!(rect_of(&e, b).min(), v2(400.0, 300.0));
}

#[test]
fn diagram_settings_undo_and_redo() {
    let mut e = ed();
    assert!(e.doc.settings.grid);
    e.toggle_grid();
    assert!(!e.doc.settings.grid);
    e.set_grid_step(25.0);
    assert_eq!(e.doc.settings.grid_size, 25.0);
    assert_eq!(e.grid_step, 25.0);

    e.undo();
    assert_eq!(e.doc.settings.grid_size, 10.0);
    assert_eq!(e.grid_step, 10.0);
    assert!(!e.doc.settings.grid);
    e.undo();
    assert!(e.doc.settings.grid);
    e.redo();
    assert!(!e.doc.settings.grid);

    e.edit_settings("Page paper", |s| {
        s.paper = crate::model::PaperSize::A4;
        s.orientation = crate::model::Orientation::Portrait;
    });
    let (w, h) = e.doc.settings.paper_size();
    assert!((w - 595.28).abs() < 0.01 && (h - 841.89).abs() < 0.01);
    e.undo();
    assert_eq!(e.doc.settings.paper, crate::model::PaperSize::Letter);
}

#[test]
fn dragging_from_a_ruler_adds_a_guide_with_undo() {
    use crate::model::{GuideAxis, ManualGuide};
    let mut e = ed();
    e.doc.settings.rulers = true;

    // Down out of the top ruler: a horizontal guide at the release y.
    drag(&mut e, (300.0, 9.0), (300.0, 200.0), 0.0);
    assert_eq!(
        e.doc.settings.manual_guides,
        vec![ManualGuide::horizontal(200.0)]
    );
    e.undo();
    assert!(e.doc.settings.manual_guides.is_empty());
    e.redo();
    assert_eq!(
        e.doc.settings.manual_guides,
        vec![ManualGuide::horizontal(200.0)]
    );

    // Out of the left ruler: a vertical guide.
    drag(&mut e, (9.0, 420.0), (260.0, 420.0), 500.0);
    assert_eq!(
        e.doc.settings.manual_guides.last().unwrap().axis,
        GuideAxis::Vertical
    );
    assert!((e.doc.settings.manual_guides.last().unwrap().pos - 260.0).abs() < 1e-9);
}

#[test]
fn a_click_on_the_ruler_does_not_add_a_guide() {
    let mut e = ed();
    e.doc.settings.rulers = true;
    down(&mut e, 300.0, 9.0, 0.0);
    up(&mut e, 300.0, 9.0, 100.0);
    assert!(e.doc.settings.manual_guides.is_empty());
}

#[test]
fn guides_move_and_delete_when_dropped_on_the_ruler() {
    use crate::model::ManualGuide;
    let mut e = ed();
    e.doc.settings.rulers = true;
    e.edit_settings("Add guide", |s| {
        s.manual_guides.push(ManualGuide::vertical(200.0))
    });

    drag(&mut e, (200.0, 300.0), (260.0, 300.0), 0.0);
    assert_eq!(
        e.doc.settings.manual_guides,
        vec![ManualGuide::vertical(260.0)]
    );

    // Dropping it back on its ruler removes it.
    drag(&mut e, (260.0, 300.0), (9.0, 300.0), 500.0);
    assert!(e.doc.settings.manual_guides.is_empty());
    assert_eq!(e.history.undo_labels().last(), Some("Delete guide"));
    e.undo();
    assert_eq!(
        e.doc.settings.manual_guides,
        vec![ManualGuide::vertical(260.0)]
    );
}

#[test]
fn moving_a_shape_snaps_to_a_manual_guide() {
    use crate::model::ManualGuide;
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    e.doc
        .settings
        .manual_guides
        .push(ManualGuide::vertical(303.0));

    // The drag would leave the left edge at 300.5, near the 303 guide.
    drag(&mut e, (140.0, 130.0), (340.5, 130.0), 0.0);
    let r = rect_of(&e, id);
    assert!((r.x - 303.0).abs() < 1e-9, "{r:?}");
    assert!((r.y - 100.0).abs() < 1e-9);
}

#[test]
fn resizing_snaps_the_dragged_edge_to_a_manual_guide() {
    use crate::model::ManualGuide;
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    e.select(vec![id]);
    e.doc
        .settings
        .manual_guides
        .push(ManualGuide::vertical(303.0));

    drag(&mut e, (180.0, 130.0), (301.0, 130.0), 0.0);
    let r = rect_of(&e, id);
    assert!((r.right() - 303.0).abs() < 1e-9, "{r:?}");
    assert!((r.x - 100.0).abs() < 1e-9);
}

#[test]
fn scheme_apply_is_undoable() {
    use crate::model::scheme::SCHEMES;
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    e.edit("fill", |tx| {
        let n = tx.node_mut(id).unwrap();
        n.style.fills = vec![crate::model::Paint::solid(crate::model::Color::rgb(
            9, 9, 9,
        ))];
    });
    let old = e.doc.get(id).unwrap().style.clone();
    let scheme = SCHEMES.iter().find(|s| s.name == "Teal").unwrap();
    e.apply_scheme(scheme);
    let n = e.doc.get(id).unwrap();
    assert!(matches!(
        n.style.fills.last(),
        Some(crate::model::Paint::Solid { color, .. }) if *color == scheme.fill
    ));
    assert_eq!(n.style.stroke, Some(scheme.line));
    assert_eq!(e.doc.settings.scheme.as_deref(), Some("Teal"));
    assert_eq!(e.doc.settings.background_color, scheme.page);

    e.undo();
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.style, old);
    assert_eq!(e.doc.settings.scheme, None);
}

#[test]
fn scheme_apply_skips_locked_hidden_and_preserves_adaptive_pairs() {
    use crate::model::scheme::SCHEMES;

    let mut e = ed();
    let open = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    let locked = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(220.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    let hidden = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Rect,
            Rect::new(340.0, 100.0, 80.0, 60.0),
        ),
        None,
    );
    e.doc.nodes.get_mut(&locked).unwrap().locked = true;
    e.doc.nodes.get_mut(&hidden).unwrap().visible = false;
    let locked_before = e.doc.get(locked).unwrap().clone();
    let hidden_before = e.doc.get(hidden).unwrap().clone();
    e.edit("adaptive pair", |tx| {
        let n = tx.node_mut(open).unwrap();
        n.style.fills = vec![Paint::solid_pair(
            Color::rgb(9, 9, 9),
            Color::rgb(240, 240, 240),
        )];
        n.style.stroke = Some(Color::rgb(9, 9, 9));
        n.style.stroke_dark = Some(Color::rgb(240, 240, 240));
        n.style.radii = Some([1.0, 2.0, 3.0, 4.0]);
        n.metadata.insert("owner".into(), "Ada".into());
        n.instance = Some(InstanceLink {
            main: NodeId(9),
            overrides: vec!["fill".into()],
        });
        n.text_style.spans.push(TextSpan {
            start: 0,
            end: 2,
            color: Some(Color::rgb(1, 2, 3)),
            color_dark: Some(Color::rgb(250, 240, 230)),
            ..TextSpan::default()
        });
    });

    let open_before = e.doc.get(open).unwrap().clone();
    let scheme = SCHEMES.iter().find(|s| s.name == "Teal").unwrap();
    assert_eq!(e.scheme_preview(SchemeScope::Page).changed, 1);
    let report = e.apply_scheme_scoped(scheme, SchemeScope::Page);

    assert_eq!(report, e.scheme_preview(SchemeScope::Page));
    assert_eq!(report.changed, 1);
    assert_eq!(report.locked, 1);
    assert_eq!(report.hidden, 1);
    assert_eq!(report.skipped(), 2);
    assert!(report.summary().contains("1 locked skipped"));
    assert_eq!(e.doc.get(locked).unwrap(), &locked_before);
    assert_eq!(e.doc.get(hidden).unwrap(), &hidden_before);
    let n = e.doc.get(open).unwrap().clone();
    assert!(
        matches!(
            n.style.fills.first(),
            Some(Paint::Solid { dark: Some(_), .. })
        ),
        "the scheme dropped the fill's dark value"
    );
    assert!(n.style.stroke_dark.is_some());
    assert_eq!(n.style.radii, Some([1.0, 2.0, 3.0, 4.0]));
    assert_eq!(n.metadata.get("owner").map(String::as_str), Some("Ada"));
    assert_eq!(n.instance.as_ref().unwrap().overrides, ["fill"]);
    assert_eq!(n.text_style.spans.len(), 1);
    assert!(n.text_style.spans[0].color_dark.is_some());

    e.undo();
    assert_eq!(e.doc.get(open).unwrap(), &open_before);
    assert_eq!(e.doc.settings.scheme, None);
    e.redo();
    assert_eq!(e.doc.get(open).unwrap(), &n);
}

#[test]
fn scheme_selection_scope_restyles_only_selected_subtrees() {
    use crate::model::scheme::SCHEMES;

    let mut e = ed();
    // An empty group is dropped when it is committed on its own, so build the
    // hierarchy in one document as the autosize tests do.
    let group = NodeId(1);
    let child = NodeId(2);
    let outside = NodeId(3);
    let mut group_node = Node::new(group, NodeKind::Group, Rect::new(100.0, 100.0, 80.0, 60.0));
    let mut child_node = Node::new(child, NodeKind::Rect, Rect::new(100.0, 100.0, 80.0, 60.0));
    let outside_node = Node::new(outside, NodeKind::Rect, Rect::new(220.0, 100.0, 80.0, 60.0));
    child_node.parent = Some(group);
    group_node.children = vec![child];
    e.doc = doc_with(vec![group_node, child_node, outside_node]);
    e.doc.root = vec![group, outside];
    e.doc.next_id = 4;

    e.select(vec![group]);
    let child_before = e.doc.get(child).unwrap().clone();
    let outside_before = e.doc.get(outside).unwrap().clone();
    let settings_before = e.doc.settings.clone();
    let scheme = SCHEMES.iter().find(|s| s.name == "Teal").unwrap();

    let report = e.apply_scheme_scoped(scheme, SchemeScope::Selection);

    assert_eq!(report.changed, 1, "report={report:?}");
    assert_eq!(report.unstyleable, 1);
    assert_eq!(report.out_of_scope, 1);
    assert_eq!(report.locked, 0);
    assert_eq!(report.hidden, 0);
    assert_ne!(e.doc.get(child).unwrap(), &child_before);
    assert_eq!(e.doc.get(outside).unwrap(), &outside_before);
    assert_eq!(e.doc.settings, settings_before);

    e.undo();
    assert_eq!(e.doc.get(child).unwrap(), &child_before);
    e.redo();
    assert_ne!(e.doc.get(child).unwrap(), &child_before);
}

#[test]
fn rotate_and_flip_are_undoable() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Line {
                a: v2(0.0, 0.0),
                b: v2(60.0, 30.0),
            },
            Rect::new(100.0, 100.0, 60.0, 30.0),
        ),
        None,
    );
    e.rotate_90(&[id]);
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 90.0);
    assert!(e.doc.get(id).unwrap().style.label_upright);
    e.rotate_90(&[id]);
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 180.0);
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 90.0);
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 0.0);
    assert!(!e.doc.get(id).unwrap().style.label_upright);

    let before = e.doc.get(id).unwrap().kind.clone();
    e.select(vec![id]);
    e.flip(0);
    let after = e.doc.get(id).unwrap().kind.clone();
    assert_ne!(before, after);
    assert!(matches!(
        &after,
        NodeKind::Line { a, b } if a.x == 60.0 && b.x == 0.0 && a.y == 0.0 && b.y == 30.0
    ));
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().kind, before);
}

#[test]
fn rotation_handle_rotates_in_snapped_steps_and_is_undoable() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 50.0),
        ),
        None,
    );
    e.select(vec![id]);
    // The handle sits 20 px above the selection's top-right corner (200, 100).
    let center = v2(150.0, 125.0);
    let handle = v2(200.0, 80.0);
    e.pointer = handle;
    assert_eq!(e.hover_cursor(), Some(CursorShape::Rotate));
    down(&mut e, handle.x, handle.y, 0.0);
    assert!(matches!(e.gesture, Gesture::Rotate { .. }));
    // Sweep the pointer a quarter turn clockwise from where it was grabbed.
    let start = (handle - center).y.atan2((handle - center).x);
    let a = start + std::f64::consts::FRAC_PI_2;
    let to = center + v2(100.0 * a.cos(), 100.0 * a.sin());
    mv(&mut e, to.x, to.y, 16.0);
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 90.0);
    assert_eq!(e.hover_cursor(), Some(CursorShape::Rotate));
    up(&mut e, to.x, to.y, 32.0);
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 90.0);
    e.undo();
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 0.0);
}

#[test]
fn shift_narrows_the_rotation_step_to_one_degree() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 50.0),
        ),
        None,
    );
    e.select(vec![id]);
    let center = v2(150.0, 125.0);
    let handle = v2(200.0, 80.0);
    down(&mut e, handle.x, handle.y, 0.0);
    e.handle(InputEvent::Modifiers(Modifiers {
        shift: true,
        ..Modifiers::NONE
    }));
    // A ten degree sweep is under the coarse 15-degree step, so Shift must keep it.
    let start = (handle - center).y.atan2((handle - center).x);
    let a = start + 10.0_f64.to_radians();
    let to = center + v2(100.0 * a.cos(), 100.0 * a.sin());
    mv(&mut e, to.x, to.y, 16.0);
    assert_eq!(e.doc.get(id).unwrap().style.rotation, 10.0);
    up(&mut e, to.x, to.y, 32.0);
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
}

#[test]
fn rotated_shapes_hit_test_through_the_rotation() {
    let mut e = ed();
    let id = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Rect,
            Rect::new(100.0, 100.0, 100.0, 50.0),
        ),
        None,
    );
    e.doc.nodes.get_mut(&id).unwrap().style.rotation = 90.0;
    // The 100x50 box turned a quarter turn covers x 125..175, y 75..175.
    assert!(hit::node_contains(&e.doc, id, v2(150.0, 160.0), 1.0));
    assert!(!hit::node_contains(&e.doc, id, v2(110.0, 125.0), 1.0));
    assert_eq!(
        hit::hit_test(&e.doc, v2(150.0, 160.0), 1.0),
        Some(id),
        "the rotated area is clickable"
    );
}

#[test]
fn label_upright_keeps_glyphs_horizontal_when_rotated() {
    use crate::render::draw::DrawOp;
    let wr = Rect::new(0.0, 0.0, 200.0, 100.0);
    let mut n = Node::new(NodeId(1), NodeKind::Rect, wr);
    n.style.fills.clear();
    n.style.stroke = None;
    n.label = "MMMM".into();
    n.style.rotation = 90.0;
    let text_bounds = |ops: &[DrawOp]| {
        let mut rects = Vec::new();
        for op in ops {
            match op {
                DrawOp::Fill { path, .. } => rects.extend(path.bounds()),
                DrawOp::Glyphs {
                    face, size, glyphs, ..
                } => {
                    for (gid, p) in glyphs {
                        rects.extend(crate::text::glyph_path(*face, *gid, *p, *size).bounds());
                    }
                }
                DrawOp::Stroke { .. } => {}
            }
        }
        Rect::union_all(&rects).expect("the label produced drawn glyphs")
    };
    let rotated =
        crate::render::draw::node_draw_with(&n, wr, 1.0, false, false, None, &[], false, 0.0);
    let vertical = text_bounds(&rotated.ops);
    n.style.label_upright = true;
    let upright =
        crate::render::draw::node_draw_with(&n, wr, 1.0, false, false, None, &[], false, 0.0);
    let level = text_bounds(&upright.ops);
    assert!(
        vertical.h > vertical.w,
        "a rotated label run turns vertical: {vertical:?}"
    );
    assert!(
        level.w > level.h,
        "an upright label stays horizontal: {level:?}"
    );
}

#[test]
fn label_styling_survives_serialization() {
    use crate::model::{TextAlign, TextStyle, VAlign};
    let mut n = Node::new(NodeId(7), NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 80.0));
    n.label = "Running".into();
    n.label_size = Some(11.5);
    n.label_valign = Some(VAlign::Bottom);
    n.label_wrap = true;
    n.label_opacity = 0.4;
    n.text_formatted = true;
    n.text_border = Some(crate::model::Color::rgb(1, 2, 3));
    n.text_bg = Some(crate::model::Color::rgb(4, 5, 6));
    n.text_style = TextStyle {
        family: Some("DejaVu Sans".into()),
        align: Some(TextAlign::Right),
        letter_spacing: 1.25,
        ..TextStyle::default()
    };
    let s = serde_json::to_string(&n).unwrap();
    let back: Node = serde_json::from_str(&s).unwrap();
    assert_eq!(back, n);
}

#[test]
fn advanced_style_json_is_sanitized() {
    use crate::model::{Paint, Style};
    let mut style: Style = serde_json::from_str(
        r##"{"fills":[{"type":"linear","stops":[{"pos":9.0,"color":"#ff0000ff"},{"pos":-2.0,"color":"#00ff00ff"}],"from":{"x":0.0,"y":0.0},"to":{"x":1.0,"y":1.0}}],"stroke_width":-5,"opacity":4.0,"corner_radius":-2,"rotation":90.0}"##,
    )
    .unwrap();
    style.sanitize();
    assert_eq!(style.stroke_width, 1.0);
    assert_eq!(style.opacity, 1.0);
    assert_eq!(style.corner_radius, 0.0);
    assert_eq!(style.rotation, 90.0);
    let Paint::Linear { stops, .. } = &style.fills[0] else {
        panic!("gradient kept");
    };
    assert_eq!(stops.len(), 2);
    assert!(stops[0].pos <= stops[1].pos);
    assert!(stops.iter().all(|s| (0.0..=1.0).contains(&s.pos)));
}

// ------------------------------------------------------------------------------------ line jumps

#[test]
fn jump_style_and_size_round_trip_through_json() {
    let n = Node::new(
        NodeId(3),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(100.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::Arrow,
            jump: LineJump::Arc,
            jump_size: 9.5,
        },
        Rect::new(0.0, 0.0, 100.0, 0.0),
    );
    let s = serde_json::to_string(&n).unwrap();
    assert!(s.contains("\"jump\":\"arc\""), "{s:?}");
    assert!(s.contains("\"jump_size\":9.5"), "{s:?}");
    let back: Node = serde_json::from_str(&s).unwrap();
    assert_eq!(back, n);

    // Files saved before line jumps existed load with the defaults.
    let mut v: serde_json::Value = serde_json::from_str(&s).unwrap();
    v.as_object_mut()
        .unwrap()
        .retain(|key, _| key != "jump" && key != "jump_size");
    let plain: Node = serde_json::from_value(v).unwrap();
    let NodeKind::Connector {
        jump, jump_size, ..
    } = &plain.kind
    else {
        panic!("still a connector")
    };
    assert_eq!(*jump, LineJump::None);
    assert_eq!(*jump_size, 6.0, "draw.io's defaultJumpSize");
}

/// A document holding the given nodes, in paint order.
fn doc_with(nodes: Vec<Node>) -> crate::model::Document {
    let mut doc = crate::model::Document::default();
    for n in nodes {
        doc.nodes.insert(n.id, n);
    }
    doc
}

#[test]
fn line_jumps_hop_only_over_connectors_painted_earlier() {
    use crate::render::draw::JumpWalk;
    // A vertical line painted first, a horizontal jumper painted second. With
    // both rects at the origin the local points are world points, and the two
    // lines cross at (0, 0).
    let vertical = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, -50.0), v2(0.0, 50.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 0.0, 0.0),
    );
    let mut jumper = Node::new(
        NodeId(1),
        NodeKind::Connector {
            points: vec![v2(-100.0, 0.0), v2(100.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::Arc,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 0.0, 0.0),
    );
    let mut walk = JumpWalk::new(&doc_with(vec![vertical.clone(), jumper.clone()]));

    // The first connector hops over nothing.
    assert!(walk.points_for(&vertical, vertical.rect).is_empty());
    walk.push(&vertical, vertical.rect);
    // The jumper crosses the vertical line at (0, 0).
    let pts = walk.points_for(&jumper, jumper.rect);
    assert_eq!(pts.len(), 1, "{pts:?}");
    assert!(pts[0].dist(v2(0.0, 0.0)) < 1e-9, "{pts:?}");

    // With every jump style None the walk stays inactive: no hops anywhere.
    if let NodeKind::Connector { jump, .. } = &mut jumper.kind {
        *jump = LineJump::None;
    }
    let walk = JumpWalk::new(&doc_with(vec![vertical, jumper.clone()]));
    assert!(walk.points_for(&jumper, jumper.rect).is_empty());
}

#[test]
fn jumped_connector_draw_ops_include_the_hop() {
    // A straight line with a gap hop over a crossing at (100, 0) splits into
    // two runs; with no jump it stays one run.
    let mut n = Node::new(
        NodeId(5),
        NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(200.0, 0.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::Gap,
            jump_size: 6.0,
        },
        Rect::new(0.0, 0.0, 200.0, 0.0),
    );
    n.style.stroke_width = 2.0;
    let drawn = crate::render::draw::node_draw_with(
        &n,
        n.rect,
        1.0,
        false,
        false,
        None,
        &[v2(100.0, 0.0)],
        false,
        0.0,
    );
    let crate::render::draw::DrawOp::Stroke { path, .. } = &drawn.ops[0] else {
        panic!("the line is a stroke")
    };
    assert_eq!(
        path.segs
            .iter()
            .filter(|s| matches!(s, crate::vector::Seg::M(_)))
            .count(),
        2,
        "the line is interrupted at the crossing: {path:?}"
    );

    // Arc keeps one run but bends it over the crossing.
    if let NodeKind::Connector { jump, .. } = &mut n.kind {
        *jump = LineJump::Arc;
    }
    let drawn = crate::render::draw::node_draw_with(
        &n,
        n.rect,
        1.0,
        false,
        false,
        None,
        &[v2(100.0, 0.0)],
        false,
        0.0,
    );
    let crate::render::draw::DrawOp::Stroke { path, .. } = &drawn.ops[0] else {
        panic!("the line is a stroke")
    };
    assert!(
        path.segs
            .iter()
            .any(|s| matches!(s, crate::vector::Seg::C(..))),
        "the hop is a cubic: {path:?}"
    );
}

// --------------------------------------------------------------------- page layers (M3)

fn top_layer(e: &mut Editor) -> LayerId {
    e.layer_add()
}

#[test]
fn every_page_starts_with_a_background_layer() {
    let e = ed();
    assert_eq!(
        e.doc.layers,
        vec![Layer {
            id: LayerId(1),
            name: "Background".into(),
            visible: true,
            locked: false,
        }]
    );
    let mut e = ed();
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 50.0, 50.0)),
        None,
    );
    let b = e.add_node(
        Node::new(NodeId(1), NodeKind::Rect, Rect::new(60.0, 0.0, 50.0, 50.0)),
        None,
    );
    // Unnamed content paints on the background layer and the two orders agree.
    assert_eq!(e.doc.effective_layer(a), LayerId(1));
    assert_eq!(e.doc.effective_layer(b), LayerId(1));
    assert_eq!(e.doc.layered_root(), e.doc.root);
    e.doc.validate().unwrap();
}

#[test]
fn setting_a_layer_moves_the_whole_subtree_and_survives_validation() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let frame = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 300.0, 300.0),
        ),
        None,
    );
    let child = e.add_node(
        Node::new(NodeId(1), NodeKind::Rect, Rect::new(10.0, 10.0, 50.0, 50.0)),
        Some(frame),
    );
    // Selecting the deep child moves the whole frame subtree.
    e.select(vec![child]);
    e.move_selection_to_layer(&[child], top);
    assert_eq!(e.doc.effective_layer(frame), top);
    assert_eq!(e.doc.effective_layer(child), top);
    e.doc.validate().unwrap();
    // Undo returns everything to the background layer.
    e.undo();
    assert_eq!(e.doc.effective_layer(frame), LayerId(1));
    assert_eq!(e.doc.effective_layer(child), LayerId(1));
    assert_eq!(e.doc.layer_by_id(top).unwrap().name, "Layer 2");
    e.doc.validate().unwrap();
}

#[test]
fn hiding_a_layer_hides_its_shapes_and_hit_testing() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let bg_shape = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    let top_shape = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(120.0, 0.0, 100.0, 100.0),
        ),
        None,
    );
    e.move_selection_to_layer(&[top_shape], top);
    e.layer_set_visible(top, false);
    assert!(!e.doc.is_effectively_visible(top_shape));
    assert!(e.doc.is_effectively_visible(bg_shape));
    assert_eq!(hit::hit_test(&e.doc, v2(170.0, 50.0), 1.0), None);
    assert_eq!(hit::hit_test(&e.doc, v2(50.0, 50.0), 1.0), Some(bg_shape));
    // Showing it again brings hit testing back without touching the shapes.
    e.layer_set_visible(top, true);
    assert!(e.doc.is_effectively_visible(top_shape));
    assert_eq!(hit::hit_test(&e.doc, v2(170.0, 50.0), 1.0), Some(top_shape));
}

#[test]
fn locking_a_layer_blocks_hit_testing_until_unlocked() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let s = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    e.move_selection_to_layer(&[s], top);
    e.layer_set_locked(top, true);
    assert!(e.doc.is_effectively_locked(s));
    assert_eq!(hit::hit_test(&e.doc, v2(50.0, 50.0), 1.0), None);
    e.layer_set_locked(top, false);
    assert_eq!(hit::hit_test(&e.doc, v2(50.0, 50.0), 1.0), Some(s));
}

#[test]
fn deleting_a_layer_removes_its_shapes_but_keeps_the_last_layer() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let bg_shape = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    let top_shape = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(120.0, 0.0, 100.0, 100.0),
        ),
        None,
    );
    e.move_selection_to_layer(&[top_shape], top);
    e.layer_delete(top);
    assert!(!e.doc.nodes.contains_key(&top_shape));
    assert!(e.doc.nodes.contains_key(&bg_shape));
    assert_eq!(e.doc.layers.len(), 1);
    e.undo();
    assert!(e.doc.nodes.contains_key(&top_shape));
    assert_eq!(e.doc.layers.len(), 2);
    // Any non-last layer can go, including Background; its shapes leave too.
    e.layer_delete(LayerId(1));
    assert_eq!(e.doc.layers.len(), 1);
    assert!(!e.doc.nodes.contains_key(&bg_shape));
    assert!(e.doc.nodes.contains_key(&top_shape));
    assert_eq!(e.doc.layered_root(), vec![top_shape]);
    e.doc.validate().unwrap();
}

#[test]
fn duplicating_a_layer_copies_its_shapes_with_fresh_ids() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(120.0, 0.0, 100.0, 100.0),
        ),
        None,
    );
    e.move_selection_to_layer(&[a], top); // b stays on Background
    let copy = e.layer_duplicate(top).unwrap();
    // The copy sits right above its source and carries only its shapes.
    let on_copy: Vec<NodeId> = e
        .doc
        .layered_root()
        .into_iter()
        .filter(|id| e.doc.effective_layer(*id) == copy)
        .collect();
    assert_eq!(on_copy.len(), 1, "the copy carries exactly one shape");
    assert_ne!(on_copy[0], a, "the copy uses fresh ids");
    assert!(e.doc.nodes.contains_key(&a)); // the original is untouched
    assert!(e.doc.nodes.contains_key(&b));
    assert_eq!(rect_of(&e, a), e.doc.get(on_copy[0]).unwrap().rect);
    assert_eq!(e.doc.layer_by_id(copy).unwrap().name, "Copy of Layer 2");
    e.doc.validate().unwrap();
    e.undo();
    assert_eq!(e.doc.layers.len(), 2, "undo removes the copy");
    assert!(e.doc.nodes.len() == 2, "undo removes the copied shapes");
}

#[test]
fn reordering_layers_only_changes_the_paint_order() {
    let mut e = ed();
    let bg_shape = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    let top = top_layer(&mut e);
    let top_shape = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(120.0, 0.0, 100.0, 100.0),
        ),
        None,
    );
    e.move_selection_to_layer(&[top_shape], top);
    // Background layer to the top: its shapes now paint in front.
    e.layer_reorder(LayerId(1), 1);
    assert_eq!(
        e.doc.layered_root(),
        vec![top_shape, bg_shape],
        "background shapes paint in front after the reorder"
    );
    assert_eq!(
        e.doc.root,
        vec![bg_shape, top_shape],
        "the root list never changes"
    );
    e.undo();
    assert_eq!(e.doc.layered_root(), vec![bg_shape, top_shape]);
}

#[test]
fn connectors_bind_across_layers_and_keep_their_binding() {
    let mut e = ed();
    let top = top_layer(&mut e);
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    e.move_selection_to_layer(&[b], top);
    let id = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Connector {
                points: vec![v2(100.0, 30.0), v2(300.0, 230.0)],
                start: Some(crate::model::Binding {
                    node: a,
                    fixed: v2(1.0, 0.5),
                    inside: false,
                    floating: false,
                }),
                end: Some(crate::model::Binding {
                    node: b,
                    fixed: v2(0.0, 0.5),
                    inside: false,
                    floating: false,
                }),
                route: Route::Elbow,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(100.0, 30.0, 200.0, 200.0),
        ),
        None,
    );
    let before = crate::connector::resolve(&e.doc, id).unwrap();
    assert!(
        before.len() >= 2,
        "the route resolves with a cross-layer binding"
    );
    // Moving the bound shape on another layer keeps the binding alive.
    e.select(vec![b]);
    e.snapping = false;
    drag(&mut e, (350.0, 230.0), (350.0, 430.0), 0.0);
    let after = crate::connector::resolve(&e.doc, id).unwrap();
    assert!(after.len() >= 2, "the route still resolves after the move");
    assert_ne!(before, after, "the route follows b across layers");
}

// --------------------------------------------------- M4: registry, autosize (ADR-010)

#[test]
fn registry_insert_is_one_undoable_node_centered_at_the_point() {
    let mut e = ed();
    let id = e
        .insert_registry_shape("Database", Some(v2(400.0, 300.0)), None, None, None)
        .expect("a known id or name inserts");
    let n = e.doc.get(id).unwrap();
    assert_eq!(n.name, "Database");
    assert!(
        matches!(n.kind, NodeKind::Path { .. }),
        "the cylinder is a native path node"
    );
    assert_eq!(rect_of(&e, id), Rect::new(340.0, 245.0, 120.0, 110.0));
    assert_eq!(e.selection, vec![id], "the new shape is selected");
    assert!(
        e.insert_registry_shape("no-such-shape", None, None, None, None)
            .is_none()
    );
    // The insert is a single history step, not an add followed by a move.
    e.undo();
    assert!(e.doc.get(id).is_none());
    e.redo();
    assert!(e.doc.get(id).is_some());
    e.doc.validate().unwrap();
}

#[test]
fn registry_shapes_follow_the_page_default_style_except_pen_shapes() {
    let mut e = ed();
    let mut style = crate::model::Style::default();
    style.fills = vec![Paint::Solid {
        color: Color::rgb(0x11, 0x22, 0x33),
        dark: None,
    }];
    style.stroke = Some(Color::rgb(0x44, 0x55, 0x66));
    let mut text = crate::model::TextStyle::default();
    text.bold = true;
    let ds = crate::model::DefaultStyle::new(style.clone(), text.clone());
    e.edit_settings("shape default", |s| s.shape_default = Some(ds.clone()));

    let plain = e
        .insert_registry_shape("process", Some(v2(200.0, 200.0)), None, None, None)
        .unwrap();
    let n = e.doc.get(plain).unwrap();
    assert_eq!(n.style, style);
    assert_eq!(n.text_style, text);
    // The search tile builds from the same rule, so a preview matches what a
    // click inserts (CUS-16).
    let def = crate::registry::by_id("process").unwrap();
    let preview =
        crate::registry::build(def, Rect::new(0.0, 0.0, def.size.0, def.size.1), Some(&ds));
    assert_eq!(preview.style, n.style);
    assert_eq!(preview.text_style, n.text_style);

    // Pen-art shapes keep their own stroke look instead of the fill default.
    let actor = e
        .insert_registry_shape("actor", Some(v2(400.0, 200.0)), None, None, None)
        .unwrap();
    let a = e.doc.get(actor).unwrap();
    assert!(!a.style.fill_enabled);
    assert!(a.style.fills.is_empty());
    assert_eq!(a.style.stroke, Some(Color::INK));
}

#[test]
fn registry_insert_joins_the_frame_under_the_point_or_the_named_parent() {
    let mut e = ed();
    let frame = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: false },
            Rect::new(100.0, 100.0, 400.0, 300.0),
        ),
        None,
    );
    let inside = e
        .insert_registry_shape("queue", Some(v2(300.0, 250.0)), None, None, None)
        .unwrap();
    assert_eq!(e.doc.get(inside).unwrap().parent, Some(frame));
    assert_eq!(rect_of(&e, inside).center(), v2(300.0, 250.0));
    let outside = e
        .insert_registry_shape("queue", Some(v2(900.0, 700.0)), None, None, None)
        .unwrap();
    assert_eq!(e.doc.get(outside).unwrap().parent, None);
    // An explicit parent wins over the point, like the MCP insert_shape tool.
    let named = e
        .insert_registry_shape("note", Some(v2(900.0, 700.0)), None, Some(frame), None)
        .unwrap();
    assert_eq!(e.doc.get(named).unwrap().parent, Some(frame));
    assert_eq!(rect_of(&e, named).center(), v2(900.0, 700.0));
    e.doc.validate().unwrap();
}

#[test]
fn registry_insert_can_target_a_layer_in_the_same_undo_step() {
    let mut e = ed();
    let top = e.layer_add();
    let id = e
        .insert_registry_shape(
            "cloud",
            Some(v2(200.0, 200.0)),
            Some((200.0, 120.0)),
            None,
            Some(top),
        )
        .unwrap();
    assert_eq!(rect_of(&e, id).w, 200.0);
    assert_eq!(rect_of(&e, id).h, 120.0);
    assert_eq!(e.doc.effective_layer(id), top);
    e.undo();
    assert!(e.doc.get(id).is_none());
    assert!(e.doc.layered_root().is_empty());
    e.doc.validate().unwrap();
}

#[test]
fn autosize_hugs_nested_containers_and_keeps_child_world_positions() {
    let mut e = ed();
    // Build the nested groups directly: adding an empty group in its own
    // transaction would make the editor drop it. Child rects are relative to
    // their parent, and only the world rects matter here.
    let outer = NodeId(1);
    let inner = NodeId(2);
    let a = NodeId(3);
    let b = NodeId(4);
    let mut outer_node = Node::new(
        outer,
        NodeKind::Group,
        Rect::new(-50.0, -50.0, 600.0, 500.0),
    );
    let mut inner_node = Node::new(
        inner,
        NodeKind::Group,
        Rect::new(100.0, 100.0, 300.0, 300.0),
    );
    let mut a_node = Node::new(a, NodeKind::Rect, Rect::new(20.0, 20.0, 60.0, 40.0));
    let mut b_node = Node::new(b, NodeKind::Rect, Rect::new(180.0, 160.0, 50.0, 50.0));
    inner_node.parent = Some(outer);
    a_node.parent = Some(inner);
    b_node.parent = Some(inner);
    outer_node.children = vec![inner];
    inner_node.children = vec![a, b];
    e.doc = doc_with(vec![outer_node, inner_node, a_node, b_node]);
    e.doc.root = vec![outer];
    e.doc.next_id = 5;
    let before = [rect_of(&e, a), rect_of(&e, b)];
    e.select(vec![outer]);
    e.autosize_selection();
    // Groups hug exactly, children first: the inner group becomes the union
    // of its two shapes and the outer group becomes the inner group.
    assert_eq!(rect_of(&e, inner), Rect::new(70.0, 70.0, 210.0, 190.0));
    assert_eq!(rect_of(&e, outer), Rect::new(70.0, 70.0, 210.0, 190.0));
    assert_eq!(
        [rect_of(&e, a), rect_of(&e, b)],
        before,
        "autosize moves the outline, never the shapes"
    );
    // One history step covers the whole nested pass.
    e.undo();
    assert_eq!(rect_of(&e, outer), Rect::new(-50.0, -50.0, 600.0, 500.0));
    assert_eq!(rect_of(&e, inner), Rect::new(50.0, 50.0, 300.0, 300.0));
    assert_eq!([rect_of(&e, a), rect_of(&e, b)], before);
    e.doc.validate().unwrap();
}

#[test]
fn autosize_leaves_a_frame_a_title_strip_and_runs_from_the_shortcut() {
    let mut e = ed();
    let frame = e.add_node(
        Node::new(
            NodeId(0),
            NodeKind::Frame { clip: false },
            Rect::new(0.0, 0.0, 500.0, 500.0),
        ),
        None,
    );
    let child = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(100.0, 120.0, 80.0, 60.0),
        ),
        Some(frame),
    );
    e.select(vec![frame]);
    key(
        &mut e,
        Key::Char('f'),
        Modifiers {
            alt: true,
            ..Modifiers::NONE
        },
    );
    // 20 px of headroom on every side covers the frame title.
    assert_eq!(rect_of(&e, frame), Rect::new(80.0, 100.0, 120.0, 100.0));
    assert_eq!(rect_of(&e, child), Rect::new(100.0, 120.0, 80.0, 60.0));
}

#[test]
fn autosize_skips_shapes_and_an_empty_container() {
    let mut e = ed();
    let rect = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 100.0)),
        None,
    );
    let frame = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Frame { clip: false },
            Rect::new(200.0, 200.0, 80.0, 80.0),
        ),
        None,
    );
    e.select(vec![rect, frame]);
    e.autosize_selection();
    assert_eq!(rect_of(&e, rect), Rect::new(0.0, 0.0, 100.0, 100.0));
    assert_eq!(
        rect_of(&e, frame),
        Rect::new(200.0, 200.0, 80.0, 80.0),
        "a container with no children keeps its outline"
    );
}

// ---------------------------------------------------------------- M6: find, links, data

#[test]
fn find_actions_open_the_bar_and_request_a_step() {
    let mut e = ed();
    e.run(shortcuts::Action::Find);
    assert!(matches!(
        e.ui_events.as_slice(),
        [commands::UiEvent::OpenFind]
    ));
    e.ui_events.clear();
    e.run(shortcuts::Action::FindStep(true));
    assert!(matches!(
        e.ui_events.as_slice(),
        [commands::UiEvent::FindStep(true)]
    ));
    e.ui_events.clear();
    e.run(shortcuts::Action::FindStep(false));
    assert!(matches!(
        e.ui_events.as_slice(),
        [commands::UiEvent::FindStep(false)]
    ));
}

#[test]
fn activate_link_switches_pages_routes_external_links_and_blocks_unsafe_ones() {
    let mut e = ed();
    let specs = e.add_page_in_background(Some("Specs"));
    let id = e.pages[specs].id.clone();
    e.switch_page(0);
    assert_eq!(e.page_name(), "Page 1");

    e.activate_link("page:Specs");
    assert_eq!(e.active_page, specs);
    e.switch_page(0);
    e.activate_link(&format!("data:page/id,{id}"));
    assert_eq!(e.active_page, specs);
    e.switch_page(0);
    e.activate_link("data:page/Specs");
    assert_eq!(e.active_page, specs);
    e.switch_page(0);
    e.activate_link("page:Missing");
    assert_eq!(e.active_page, 0);
    assert!(e.status.contains("No page matches"));

    e.requests.clear();
    e.activate_link("https://example.com/docs");
    assert!(matches!(
        e.requests.as_slice(),
        [AppRequest::OpenLink(url)] if url == "https://example.com/docs"
    ));

    e.requests.clear();
    e.activate_link("javascript:alert(1)");
    assert!(e.requests.is_empty());
    assert!(e.status.contains("blocked"), "{}", e.status);
}

#[test]
fn dropping_a_connector_end_near_a_fixed_point_stores_that_point() {
    let mut e = ed();
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let id = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Connector {
                points: vec![v2(50.0, 30.0), v2(350.0, 230.0)],
                start: Some(crate::model::Binding {
                    node: a,
                    fixed: v2(0.5, 0.5),
                    inside: false,
                    floating: false,
                }),
                end: Some(crate::model::Binding {
                    node: b,
                    fixed: v2(0.5, 0.5),
                    inside: false,
                    floating: false,
                }),
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(50.0, 30.0, 300.0, 200.0),
        ),
        None,
    );

    // Three world units from the target's top-left corner snaps to that point.
    e.drag_connector_end(id, 1, v2(303.0, 203.0));
    let NodeKind::Connector { end, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    let end = end.expect("end stays bound");
    assert_eq!(end.node, b);
    assert!(end.fixed.x.abs() < 1e-4, "snapped x: {}", end.fixed.x);
    assert!(end.fixed.y.abs() < 1e-4, "snapped y: {}", end.fixed.y);

    // The fixed fraction tracks the same corner after the target is resized.
    e.edit("resize target", |tx| {
        tx.node_mut(b).unwrap().rect = Rect::new(250.0, 150.0, 200.0, 120.0);
    });
    e.reroute_connectors(&std::collections::HashSet::from([b]));
    let wr = rect_of(&e, b);
    let NodeKind::Connector { end, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    let end = end.expect("end stays bound");
    assert_eq!(
        crate::connector::global_fixed(end.fixed, wr),
        wr.min(),
        "the endpoint keeps the top-left corner"
    );
}

#[test]
fn endpoint_mode_toggle_is_undoable_and_keeps_the_current_position() {
    let mut e = ed();
    let a = e.add_node(
        Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0)),
        None,
    );
    let b = e.add_node(
        Node::new(
            NodeId(1),
            NodeKind::Rect,
            Rect::new(300.0, 200.0, 100.0, 60.0),
        ),
        None,
    );
    let id = e.add_node(
        Node::new(
            NodeId(2),
            NodeKind::Connector {
                points: vec![v2(50.0, 30.0), v2(350.0, 230.0)],
                start: Some(crate::model::Binding {
                    node: a,
                    fixed: v2(1.0, 0.0),
                    inside: false,
                    floating: false,
                }),
                end: Some(crate::model::Binding {
                    node: b,
                    fixed: v2(0.5, 0.5),
                    inside: false,
                    floating: false,
                }),
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(50.0, 0.0, 300.0, 230.0),
        ),
        None,
    );
    let fixed_point = crate::connector::resolve(&e.doc, id).unwrap()[0];

    crate::editor::whiteboard::set_connector_endpoint_mode(&mut e, &[id], 0, true);
    let NodeKind::Connector { start, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    assert!(start.unwrap().floating);
    let floating_point = crate::connector::resolve(&e.doc, id).unwrap()[0];
    assert!(
        fixed_point.dist(floating_point) > 1.0,
        "changing the endpoint mode should reroute: {fixed_point:?} -> {floating_point:?}"
    );

    e.undo();
    let NodeKind::Connector { start, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    assert!(!start.unwrap().floating, "undo restores a fixed end");
    e.redo();
    let NodeKind::Connector { start, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    assert!(start.unwrap().floating, "redo restores the floating end");

    crate::editor::whiteboard::set_connector_endpoint_mode(&mut e, &[id], 0, false);
    let NodeKind::Connector { start, .. } = &e.doc.get(id).unwrap().kind else {
        panic!("still a connector");
    };
    let binding = start.unwrap();
    assert!(!binding.floating);
    assert!(
        (binding.fixed.x - 1.0).abs() > 1e-6 || (binding.fixed.y - 0.0).abs() > 1e-6,
        "a fixed end remembers where the floating end was: {:?}",
        binding.fixed
    );
    let restored = crate::connector::resolve(&e.doc, id).unwrap()[0];
    assert!(
        restored.dist(floating_point) < 4.0,
        "switching back should keep the position: {floating_point:?} -> {restored:?}"
    );
}

#[test]
fn clicking_a_linked_shape_with_command_follows_its_link() {
    let mut e = ed();
    let mut a = Node::new(NodeId(0), NodeKind::Rect, Rect::new(10.0, 10.0, 80.0, 40.0));
    a.link = "https://example.com".into();
    let a = e.add_node(a, None);
    e.handle(InputEvent::Modifiers(Modifiers {
        ctrl: true,
        ..Modifiers::NONE
    }));
    down(&mut e, 50.0, 30.0, 0.0);
    up(&mut e, 50.0, 30.0, 10.0);
    e.handle(InputEvent::Modifiers(Modifiers::NONE));
    assert!(e.selection.is_empty(), "link clicks do not select");
    assert!(matches!(
        e.requests.as_slice(),
        [AppRequest::OpenLink(url)] if url == "https://example.com"
    ));
    assert!(e.doc.get(a).is_some());
}
