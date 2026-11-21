//! Theme visibility tests for the interactive canvas.

mod common;

use common::*;

use drawio_rs::geom::{Rect, V2, v2};
use drawio_rs::model::rough::Rough;
use drawio_rs::model::{
    AdaptiveColors, Color, Head, LineJump, Node, NodeId, NodeKind, Route, TextAlign,
};
use drawio_rs::render::draw::{self, contrast_for_dark};
use drawio_rs::render::gpu::Headless;
use drawio_rs::render::scene;

fn render(g: &Headless, ed: &drawio_rs::editor::Editor, dark: bool) -> image::RgbaImage {
    let frame = scene::canvas(ed, &mut scene::SceneCache::default(), V2::ZERO, false, dark);
    let size = [
        (ed.viewport.x * SCALE) as u32,
        (ed.viewport.y * SCALE) as u32,
    ];
    g.render_frame(&frame, size, SCALE as f32, [1.0; 4])
}

fn arrow_editor() -> drawio_rs::editor::Editor {
    let mut e = editor(800.0, 400.0);
    let mut n = Node::new(
        NodeId(0),
        NodeKind::Connector {
            points: vec![v2(0.0, 40.0), v2(600.0, 40.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::Arrow,
            jump: LineJump::None,
            jump_size: 6.0,
        },
        Rect::new(100.0, 160.0, 600.0, 80.0),
    );
    n.style.stroke = Some(Color::INK);
    n.style.stroke_width = 4.0;
    e.add_node(n, None);
    e
}

fn brightness(p: [u8; 4]) -> u32 {
    p[0] as u32 + p[1] as u32 + p[2] as u32
}

fn text_editor() -> drawio_rs::editor::Editor {
    let mut e = editor(800.0, 400.0);
    let mut n = Node::new(
        NodeId(1),
        NodeKind::Text {
            text: "VISIBLE".into(),
            font_size: 36.0,
            auto_width: false,
            align: TextAlign::Left,
        },
        Rect::new(100.0, 100.0, 300.0, 90.0),
    );
    n.style.set_fill(Some(Color::INK));
    e.add_node(n, None);
    e
}

#[test]
fn dark_theme_keeps_arrows_visible() {
    let Some(g) = gpu() else { return };
    let e = arrow_editor();
    let img = render(g, &e, true);
    if let Ok(path) = std::env::var("DRAWIO_RS_THEME_ARTIFACT") {
        img.save(path).unwrap();
    }

    let bg = px(&img, v2(400.0, 100.0));
    let line = px(&img, v2(400.0, 200.0));
    assert!(
        brightness(line) > brightness(bg) + 180,
        "connector line should stand out on the dark canvas: bg {bg:?}, line {line:?}"
    );

    let head = Rect::new(670.0, 185.0, 30.0, 30.0);
    let bright = count(&img, head, |p| brightness(p) > brightness(bg) + 180);
    assert!(
        bright >= 20,
        "arrowhead should have visible pixels on the dark canvas, found {bright}"
    );

    let light = render(g, &e, false);
    let authored = px(&light, v2(400.0, 200.0));
    assert!(
        close(authored, Color::INK, 4),
        "light canvas keeps the authored connector color, got {authored:?}"
    );
}

#[test]
fn dark_theme_lifts_transparent_connector_labels() {
    let Some(g) = gpu() else { return };
    let mut e = arrow_editor();
    let id = *e.doc.root.first().unwrap();
    let n = e.doc.nodes.get_mut(&id).unwrap();
    n.label = "Linked".into();
    n.label_size = Some(26.0);
    let img = render(g, &e, true);
    let bg = px(&img, v2(400.0, 100.0));
    let above_line = Rect::new(340.0, 174.0, 120.0, 22.0);
    let bright = count(&img, above_line, |p| brightness(p) > brightness(bg) + 180);
    assert!(
        bright >= 40,
        "connector label should be readable above the line, found {bright}"
    );
}

#[test]
fn dark_theme_keeps_text_visible_in_every_adaptive_mode() {
    let Some(g) = gpu() else { return };
    let label = Rect::new(100.0, 100.0, 300.0, 90.0);
    for mode in AdaptiveColors::ALL {
        let mut e = text_editor();
        e.doc.settings.adaptive_colors = mode;
        let img = render(g, &e, true);
        let bg = px(&img, v2(400.0, 50.0));
        let visible = count(&img, label, |p| {
            if mode == AdaptiveColors::None {
                brightness(p) + 180 < brightness(bg)
            } else {
                brightness(p) > brightness(bg) + 180
            }
        });
        assert!(
            visible > 500,
            "{mode:?} text should read against the canvas, found {visible} pixels"
        );
    }
}

#[test]
fn dark_theme_keeps_unstyled_text_visible() {
    let Some(g) = gpu() else { return };
    let mut e = editor(800.0, 400.0);
    let mut n = Node::new(
        NodeId(2),
        NodeKind::Text {
            text: "VISIBLE".into(),
            font_size: 36.0,
            auto_width: false,
            align: TextAlign::Left,
        },
        Rect::new(100.0, 100.0, 300.0, 90.0),
    );
    // No fill and no plate: the glyphs sit directly on the dark canvas.
    n.style.set_fill(None);
    e.add_node(n, None);

    let img = render(g, &e, true);
    let bg = px(&img, v2(400.0, 50.0));
    let visible = count(&img, Rect::new(100.0, 100.0, 300.0, 90.0), |p| {
        brightness(p) > brightness(bg) + 180
    });
    assert!(
        visible > 500,
        "unstyled text should read against the dark canvas, found {visible} pixels"
    );
}

#[test]
fn dark_theme_keeps_authored_shape_colors() {
    let Some(g) = gpu() else { return };
    let mut e = editor(800.0, 400.0);
    for (i, c) in [
        Color::rgb(0x2e, 0x7d, 0x32),
        Color::rgb(0xff, 0x98, 0x00),
        Color::WHITE,
    ]
    .into_iter()
    .enumerate()
    {
        let mut n = Node::new(
            NodeId(10 + i as u64),
            NodeKind::Rect,
            Rect::new(100.0 + i as f64 * 170.0, 100.0, 140.0, 100.0),
        );
        n.style.set_fill(Some(c));
        e.add_node(n, None);
    }

    let img = render(g, &e, true);
    for (i, c) in [
        Color::rgb(0x2e, 0x7d, 0x32),
        Color::rgb(0xff, 0x98, 0x00),
        Color::WHITE,
    ]
    .into_iter()
    .enumerate()
    {
        let p = px(&img, v2(170.0 + i as f64 * 170.0, 140.0));
        assert!(
            close(p, c, 4),
            "shape {i} should keep its authored color in dark theme, got {p:?}"
        );
    }
}

#[test]
fn dark_theme_keeps_sketch_connectors_visible() {
    let Some(g) = gpu() else { return };
    let mut e = arrow_editor();
    let id = *e.doc.root.first().unwrap();
    e.doc.nodes.get_mut(&id).unwrap().style.rough = Some(Rough::with_seed(7));
    let img = render(g, &e, true);
    let bg = px(&img, v2(400.0, 100.0));
    let line = Rect::new(380.0, 190.0, 40.0, 20.0);
    let bright = count(&img, line, |p| brightness(p) > brightness(bg) + 180);
    assert!(
        bright >= 20,
        "sketch connector should stand out on the dark canvas, found {bright}"
    );
}

#[test]
fn dark_theme_keeps_authored_colors_on_a_white_page() {
    let Some(g) = gpu() else { return };
    let mut e = arrow_editor();
    e.doc.settings.page_view = true;
    let img = render(g, &e, true);
    let line = px(&img, v2(400.0, 200.0));
    assert!(
        close(line, Color::INK, 4),
        "a white page keeps the authored connector color in dark theme, got {line:?}"
    );
}

#[test]
fn dark_contrast_lifts_low_contrast_colors_only() {
    let ink = draw::rgba(Color::INK, 1.0);
    let lifted = contrast_for_dark(ink);
    assert_eq!(lifted[3], ink[3], "alpha stays unchanged");
    assert!(lifted[0] > 0.5 && lifted[1] > 0.5 && lifted[2] > 0.5);
}

#[test]
fn dark_contrast_gives_ink_near_white_contrast() {
    let lifted = contrast_for_dark(draw::rgba(Color::INK, 1.0));
    assert!(
        lifted[0] >= 0.9 && lifted[1] >= 0.9 && lifted[2] >= 0.9,
        "default ink should read as white on the dark canvas, got {lifted:?}"
    );

    let white = [1.0, 1.0, 1.0, 1.0];
    assert_eq!(contrast_for_dark(white), white);
    assert_eq!(
        contrast_for_dark([0.0, 0.0, 0.0, 0.0]),
        [0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn theme_change_rebuilds_cached_connectors() {
    let e = arrow_editor();
    let mut cache = scene::SceneCache::default();

    scene::canvas(&e, &mut cache, V2::ZERO, false, false);
    assert_eq!(cache.last_misses, 1);
    scene::canvas(&e, &mut cache, V2::ZERO, false, true);
    assert_eq!(
        cache.last_misses, 1,
        "switching theme must retessellate the connector"
    );
    scene::canvas(&e, &mut cache, V2::ZERO, false, true);
    assert_eq!(cache.last_misses, 0, "same theme reuses cached pieces");
}
