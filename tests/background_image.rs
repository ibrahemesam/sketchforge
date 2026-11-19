//! DOC-09: a background image belongs to a page, survives save and reload,
//! draws behind the grid and content, and exports with the page.

mod common;

use common::*;
use sketch_rs::assets;
use sketch_rs::editor::Editor;
use sketch_rs::geom::{Rect, v2};
use sketch_rs::io::export::{self, Format, Scope, Spec};
use sketch_rs::model::{BackgroundImage, Color, ImageMode, Node, NodeId, NodeKind, Paint};
use sketch_rs::project::Store;
use sketch_rs::session::Session;

const BG: Color = Color::rgb(220, 20, 180);

fn bg_image() -> image::RgbaImage {
    image::RgbaImage::from_fn(8, 6, |_, _| image::Rgba([220, 20, 180, 255]))
}

fn set_background(ed: &mut Editor, asset: &str) {
    ed.edit_settings("Set background image", |s| {
        s.background_image = Some(BackgroundImage {
            asset: asset.to_string(),
            mode: ImageMode::Stretch,
            opacity: 1.0,
        });
    });
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("sketch-bg-{tag}-{:016x}", fastrand::u64(..)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn background_image_survives_project_save_and_reload() {
    let id = assets::register_rgba(&bg_image());
    let root = std::env::temp_dir().join(format!("sketch-bg-store-{:016x}", fastrand::u64(..)));
    let store = Store::open(root.clone()).unwrap();
    let mut sess = Session::new(store.clone());
    let mut ed = Editor::default();
    sess.create(&mut ed, "Background").unwrap();
    let dir = sess.project.as_ref().unwrap().dir.clone();

    set_background(&mut ed, &id);
    sess.save(&mut ed).unwrap();

    let asset_path = dir.join("assets").join(format!("{id}.png"));
    assert!(asset_path.exists(), "background asset was written");

    let mut reopened = Editor::default();
    Session::new(store.clone())
        .open(&mut reopened, &dir)
        .unwrap();
    let bg = reopened
        .doc
        .settings
        .background_image
        .as_ref()
        .expect("background image restored");
    assert_eq!(bg.asset, id);
    assert_eq!(bg.mode, ImageMode::Stretch);
    assert!((bg.opacity - 1.0).abs() < 1e-9);
    std::fs::remove_dir_all(root).ok();
}

#[test]
fn background_image_exports_to_svg_and_pdf() {
    let id = assets::register_rgba(&bg_image());
    let mut ed = common::editor(400.0, 300.0);
    let mut node = Node::new(
        NodeId(0),
        NodeKind::Rect,
        Rect::new(100.0, 80.0, 60.0, 40.0),
    );
    node.style.fills = vec![Paint::solid(Color::rgb(0, 180, 0))];
    ed.add_node(node, None);
    set_background(&mut ed, &id);
    let pages = [("Page", &ed.doc)];
    let dir = tmp("export");

    let spec = Spec {
        format: Format::Svg,
        scope: Scope::Page,
        ..Spec::default()
    };
    let files = export::write(
        &spec,
        &export::items(&spec, &pages, 0, &[]),
        &dir.join("bg.svg"),
        None,
    )
    .unwrap();
    let svg = std::fs::read_to_string(&files[0]).unwrap();
    assert!(
        svg.contains("data:image/png;base64,"),
        "image embedded in SVG"
    );
    let after_defs = svg.split("</defs>").nth(1).unwrap_or(&svg);
    let img_at = after_defs
        .find("<image")
        .expect("image element in SVG body");
    let node_at = after_defs.find("<path").expect("node path in SVG body");
    assert!(img_at < node_at, "background image draws before the node");

    let spec = Spec {
        format: Format::Pdf,
        scope: Scope::Page,
        ..Spec::default()
    };
    let files = export::write(
        &spec,
        &export::items(&spec, &pages, 0, &[]),
        &dir.join("bg.pdf"),
        None,
    )
    .unwrap();
    let pdf_bytes = std::fs::read(&files[0]).unwrap();
    let pdf = String::from_utf8_lossy(&pdf_bytes);
    assert!(pdf.contains("/Subtype /Image"), "image XObject in PDF");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn background_image_renders_in_png_export() {
    let Some(g) = gpu() else { return };
    let id = assets::register_rgba(&bg_image());
    let mut ed = common::editor(400.0, 300.0);
    set_background(&mut ed, &id);
    let pages = [("Page", &ed.doc)];
    let dir = tmp("png");

    let spec = Spec {
        format: Format::Png,
        scope: Scope::Page,
        scale: 1.0,
        ..Spec::default()
    };
    let files = export::write(
        &spec,
        &export::items(&spec, &pages, 0, &[]),
        &dir.join("bg.png"),
        Some(g),
    )
    .unwrap();
    let img = image::open(&files[0]).unwrap().to_rgba8();
    assert!(img.width() >= 700, "page bounds included: {}", img.width());
    let pixel = img.get_pixel(10, 10).0;
    assert!(close(pixel, BG, 12), "background pixel {pixel:?}");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn background_image_draws_behind_grid_without_moving_content() {
    let Some(g) = gpu() else { return };
    let id = assets::register_rgba(&bg_image());
    let mut ed = common::editor(400.0, 300.0);
    let mut node = Node::new(
        NodeId(0),
        NodeKind::Rect,
        Rect::new(100.0, 80.0, 60.0, 40.0),
    );
    node.style.fills = vec![Paint::solid(Color::rgb(0, 180, 0))];
    ed.add_node(node, None);
    let before = render_canvas(g, &ed);
    let rect_before = ed.doc.world_rect(NodeId(0));

    set_background(&mut ed, &id);
    let after = render_canvas(g, &ed);
    let rect_after = ed.doc.world_rect(NodeId(0));

    assert_eq!(rect_before, rect_after, "content did not move");
    let node_before = px(&before, v2(120.0, 100.0));
    let node_after = px(&after, v2(120.0, 100.0));
    assert_eq!(node_before, node_after, "node pixel unchanged");

    // Pick a point away from the 10-unit grid lines and away from the node.
    let empty_before = px(&before, v2(303.0, 253.0));
    let empty_after = px(&after, v2(303.0, 253.0));
    assert!(
        !close(empty_before, BG, 12),
        "no image before: {empty_before:?}"
    );
    assert!(
        close(empty_after, BG, 12),
        "image behind grid: {empty_after:?}"
    );
}
