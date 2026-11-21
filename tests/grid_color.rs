//! CAN-06: a diagram keeps its custom grid color in light and dark chrome.

mod common;

use drawio_rs::io::drawio;
use drawio_rs::model::{Color, DiagramSettings, Document};

fn doc_with_grid(color: Color) -> Document {
    Document {
        name: "Grid".into(),
        settings: DiagramSettings {
            grid: true,
            grid_size: 10.0,
            grid_color: Some(color),
            ..DiagramSettings::default()
        },
        ..Document::default()
    }
}

#[test]
fn custom_grid_color_round_trips_in_native_page() {
    let doc = doc_with_grid(Color::rgb(0x12, 0x34, 0x56));
    let value = serde_json::to_value(&doc).expect("serialize page");
    assert_eq!(value["settings"]["grid_color"], "#123456");
    let back: Document = serde_json::from_value(value).expect("deserialize page");
    assert_eq!(back.settings.grid_color, Some(Color::rgb(0x12, 0x34, 0x56)));
}

#[test]
fn custom_grid_color_round_trips_through_drawio() {
    let doc = doc_with_grid(Color::rgb(0x12, 0x34, 0x56));
    let xml = drawio::export(&[("Grid", &doc)]);
    assert!(
        xml.contains("__sketchGridColor=\"#123456\""),
        "draw.io export carries the custom grid color: {xml}"
    );
    let (pages, _) = drawio::import_str(&xml).expect("reimport");
    assert_eq!(
        pages[0].doc.settings.grid_color,
        Some(Color::rgb(0x12, 0x34, 0x56))
    );
}

#[test]
fn grid_mesh_uses_the_custom_color() {
    let Some(gpu) = common::gpu() else {
        return;
    };
    let mut red = drawio_rs::editor::Editor::default();
    red.doc.settings.grid_color = Some(Color::rgb(0xff, 0x00, 0x00));
    let mut blue = drawio_rs::editor::Editor::default();
    blue.doc.settings.grid_color = Some(Color::rgb(0x00, 0x00, 0xff));
    let a = common::render_canvas(gpu, &red);
    let b = common::render_canvas(gpu, &blue);
    let changed = a
        .pixels()
        .zip(b.pixels())
        .filter(|(x, y)| x[0] != y[0] || x[1] != y[1] || x[2] != y[2])
        .count();
    assert!(changed > 100, "custom colors change the grid: {changed}");
}
