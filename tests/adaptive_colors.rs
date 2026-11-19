//! Adaptive color resolution, storage, UI wiring and export tests for STY-22.

use serde_json::json;
use sketch_rs::io::{export, svg};
use sketch_rs::model::{
    AdaptiveColors, Color, ColorMode, DiagramSettings, Document, Node, NodeId, NodeKind, Paint,
    Style,
};

#[test]
fn adaptive_modes_choose_the_expected_color() {
    let light = Color::rgb(0xda, 0xe8, 0xfc);
    let dark = Color::rgb(0x12, 0x34, 0x56);

    let auto = ColorMode::new(AdaptiveColors::Auto, true);
    assert_eq!(auto.resolve(light, Some(dark)), dark);
    assert_eq!(auto.resolve(light, None), light);

    let simple = ColorMode::new(AdaptiveColors::Simple, true);
    assert_eq!(simple.resolve(light, Some(dark)), dark);
    assert_eq!(simple.resolve(light, None), light);

    let none = ColorMode::new(AdaptiveColors::None, true);
    assert_eq!(none.resolve(light, Some(dark)), light);
    assert_eq!(none.resolve(light, None), light);

    for mode in AdaptiveColors::ALL {
        assert_eq!(
            ColorMode::new(mode, false).resolve(light, Some(dark)),
            light
        );
    }
}

#[test]
fn simple_adaptive_mode_inverts_only_neutral_colors() {
    let simple = ColorMode::new(AdaptiveColors::Simple, true);
    assert_eq!(simple.resolve(Color::INK, None), Color::INK.inverse());
    assert_eq!(simple.resolve(Color::WHITE, None), Color::WHITE.inverse());

    let blue = Color::rgb(0x1a, 0x53, 0xa8);
    assert_eq!(simple.resolve(blue, None), blue);
}

#[test]
fn auto_dark_keeps_authored_colors_without_inverting_them() {
    let auto = ColorMode::new(AdaptiveColors::Auto, true);
    for c in [
        Color::BLACK,
        Color::WHITE,
        Color::rgb(0x2e, 0x7d, 0x32),
        Color::rgb(0xff, 0x98, 0x00),
        Color::rgba(0x6c, 0x8e, 0xbf, 100),
    ] {
        assert_eq!(auto.resolve(c, None), c);
    }
    assert_eq!(
        auto.resolve(
            Color::rgb(0x2e, 0x7d, 0x32),
            Some(Color::rgb(0x0a, 0x3d, 0x0e))
        ),
        Color::rgb(0x0a, 0x3d, 0x0e)
    );
}

fn doc_with_dark_colors() -> Document {
    let light = Color::rgb(0xda, 0xe8, 0xfc);
    let dark = Color::rgb(0x12, 0x34, 0x56);
    let mut n = Node::new(
        NodeId(1),
        NodeKind::Rect,
        sketch_rs::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
    );
    n.style.fills = vec![Paint::solid_pair(light, dark)];
    n.style.stroke = Some(light);
    n.style.stroke_dark = Some(dark);
    n.label_color = Some(light);
    n.label_color_dark = Some(dark);
    let mut doc = Document {
        name: "Adaptive".into(),
        ..Document::default()
    };
    doc.settings.background_color = light;
    doc.settings.background_color_dark = Some(Color::BLACK);
    doc.nodes.insert(n.id, n);
    doc.root.push(NodeId(1));
    doc
}

#[test]
fn native_json_defaults_keep_light_only_documents_unchanged() {
    let settings: DiagramSettings = serde_json::from_value(json!({})).unwrap();
    assert_eq!(settings.adaptive_colors, AdaptiveColors::Auto);
    assert_eq!(settings.background_color_dark, None);
    let style: Style = serde_json::from_value(json!({"stroke": "#6c8ebf"})).unwrap();
    assert_eq!(style.stroke_dark, None);
    assert!(style.fills.is_empty());

    let mut value = serde_json::to_value(doc_with_dark_colors()).unwrap();
    assert_eq!(value["settings"]["adaptive_colors"], json!("auto"));
    assert_eq!(value["settings"]["background_color_dark"], json!("#000000"));
    assert_eq!(
        value["nodes"]["1"]["style"]["stroke_dark"],
        json!("#123456")
    );
    assert_eq!(value["nodes"]["1"]["label_color_dark"], json!("#123456"));
    assert_eq!(
        value["nodes"]["1"]["style"]["fills"][0]["dark"],
        json!("#123456")
    );

    value["settings"]["background_color_dark"] = json!(null);
    value["nodes"]["1"]["style"]["stroke_dark"] = json!(null);
    let legacy: Document = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.settings.background_color_dark, None);
    assert_eq!(legacy.get(NodeId(1)).unwrap().style.stroke_dark, None);
}

#[test]
fn document_appearance_resolution_does_not_rewrite_stored_colors() {
    let doc = doc_with_dark_colors();
    let dark = doc.with_appearance(ColorMode::dark(AdaptiveColors::Simple));
    let n = dark.get(NodeId(1)).unwrap();
    assert_eq!(n.style.fill(), Some(Color::rgb(0x12, 0x34, 0x56)));
    assert_eq!(n.style.stroke, Some(Color::rgb(0x12, 0x34, 0x56)));
    assert_eq!(n.label_color, Some(Color::rgb(0x12, 0x34, 0x56)));
    assert_eq!(dark.settings.background_color, Color::BLACK);
    assert_eq!(n.style.stroke_dark, None);

    // The source document still holds the authored light values and their pairs.
    let source = doc.get(NodeId(1)).unwrap();
    assert_eq!(source.style.fill(), Some(Color::rgb(0xda, 0xe8, 0xfc)));
    assert_eq!(source.style.stroke_dark, Some(Color::rgb(0x12, 0x34, 0x56)));

    let none = doc.with_appearance(ColorMode::dark(AdaptiveColors::None));
    assert_eq!(none.settings.background_color, Color::rgb(0xda, 0xe8, 0xfc));
    assert_eq!(
        none.get(NodeId(1)).unwrap().style.fill(),
        Some(Color::rgb(0xda, 0xe8, 0xfc))
    );
}

#[test]
fn svg_export_bakes_the_requested_appearance() {
    let doc = doc_with_dark_colors();
    let baked = doc.with_appearance(ColorMode::dark(AdaptiveColors::Simple));
    let dark_svg =
        svg::export(&baked, None, Some(Color::BLACK)).expect("svg export should succeed");
    assert!(dark_svg.contains("#123456"), "{dark_svg}");
    assert!(!dark_svg.contains("#dae8fc"), "{dark_svg}");

    let light_svg = svg::export(&doc, None, Some(Color::WHITE)).expect("svg export should succeed");
    assert!(light_svg.contains("#dae8fc"), "{light_svg}");
    assert!(!light_svg.contains("#123456"), "{light_svg}");
}

#[test]
fn export_spec_appearance_is_wired_through_svg_files() {
    let doc = doc_with_dark_colors();
    let mut value = serde_json::to_value(&doc).unwrap();
    value["settings"]["adaptive_colors"] = json!("simple");
    let doc: Document = serde_json::from_value(value).unwrap();
    let items = vec![export::Item {
        name: doc.name.clone(),
        doc: &doc,
        roots: doc.layered_root(),
    }];
    let spec = export::Spec {
        format: export::Format::Svg,
        appearance: export::Appearance::Dark,
        ..export::Spec::default()
    };
    let dir = std::env::temp_dir().join("sketch-sty22-export");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("dark.svg");
    export::write(&spec, &items, &out, None).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(text.contains("#123456"), "{text}");
    assert!(!text.contains("#dae8fc"), "{text}");
}
