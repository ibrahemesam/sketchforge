//! TXT-24: text follows a curve while the text box stays upright.

use drawio_rs::geom::Rect;
use drawio_rs::io::drawio;
use drawio_rs::model::{Document, Node, NodeId, NodeKind, TextAlign};

fn curved(text: &str) -> Node {
    let mut n = Node::new(
        NodeId(1),
        NodeKind::Text {
            text: text.into(),
            font_size: 32.0,
            auto_width: true,
            align: TextAlign::Center,
        },
        Rect::new(0.0, 0.0, 360.0, 80.0),
    );
    n.text_style.curve = Some(0.8);
    n
}

#[test]
fn text_on_curve_positions_glyphs_along_an_arc() {
    let n = curved("CURVE");
    let l = drawio_rs::text::layout_node(&n).expect("text layout");
    assert_eq!(l.glyphs.len(), 5, "one glyph per character");
    let ys: Vec<f64> = l.glyphs.iter().map(|g| g.baseline).collect();
    assert!(
        ys[0] > ys[2] && ys[4] > ys[2],
        "middle glyphs rise above the ends: {ys:?}"
    );
    assert!(l.height > 32.0, "the layout reserves room for the arc");
}

#[test]
fn text_curve_round_trips_in_native_json() {
    let n = curved("Arc");
    let value = serde_json::to_value(&n).expect("serialize node");
    assert_eq!(value["text_style"]["curve"], 0.8);
    let back: Node = serde_json::from_value(value).expect("deserialize node");
    assert_eq!(back.text_style.curve, Some(0.8));
}

#[test]
fn text_curve_round_trips_through_drawio_style() {
    let n = curved("Arc");
    let mut doc = Document {
        name: "Curve".into(),
        ..Document::default()
    };
    doc.nodes.insert(n.id, n);
    doc.root.push(NodeId(1));
    let xml = drawio::export(&[("Page", &doc)]);
    assert!(xml.contains("textPath=1;"), "{xml}");
    let (pages, _) = drawio::import_str(&xml).expect("reimport");
    assert!(
        pages[0]
            .doc
            .nodes
            .values()
            .any(|node| node.text_style.curve == Some(0.6)),
        "import restores the curve"
    );
}
