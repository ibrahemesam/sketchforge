//! Print output: paper-sized PDF pages with fit-to-page scaling and margins.

use sketch_rs::geom::Rect;
use sketch_rs::io::pdf::{PrintOptions, PrintScale, print_pdf};
use sketch_rs::model::{Document, Node, NodeId, NodeKind, Orientation, PaperSize};

fn media_box(pdf: &[u8]) -> (f64, f64) {
    let s = String::from_utf8_lossy(pdf);
    let at = s.find("/MediaBox [0 0 ").expect("MediaBox present");
    let tail = &s[at + "/MediaBox [0 0 ".len()..];
    let end = tail.find(']').expect("MediaBox closed");
    let mut parts = tail[..end].split_whitespace();
    let w: f64 = parts.next().unwrap().parse().unwrap();
    let h: f64 = parts.next().unwrap().parse().unwrap();
    (w, h)
}

/// The first content stream is Flate-compressed; decompress it so transforms
/// can be inspected.
fn content_transform(pdf: &[u8]) -> String {
    let at = pdf
        .windows(b"stream\n".len())
        .position(|w| w == b"stream\n")
        .expect("stream marker");
    let start = at + b"stream\n".len();
    let end = pdf[start..]
        .windows(b"\nendstream".len())
        .position(|w| w == b"\nendstream")
        .expect("endstream marker")
        + start;
    let raw =
        miniz_oxide::inflate::decompress_to_vec_zlib(&pdf[start..end]).expect("flate content");
    let text = String::from_utf8_lossy(&raw);
    text.lines()
        .find(|line| line.ends_with(" cm"))
        .unwrap_or("")
        .to_string()
}

fn sample_doc() -> (Document, Vec<NodeId>) {
    let mut doc = Document {
        name: "Print test".into(),
        ..Default::default()
    };
    let id = NodeId(1);
    let mut node = Node::new(id, NodeKind::Rect, Rect::new(100.0, 100.0, 400.0, 300.0));
    node.label = "Printed shape".into();
    doc.nodes.insert(id, node);
    doc.root.push(id);
    (doc, vec![id])
}

#[test]
fn print_pdf_uses_paper_size_and_orientation_for_mediabox() {
    let (doc, roots) = sample_doc();

    let portrait = print_pdf(
        &doc,
        &roots,
        &PrintOptions {
            paper: PaperSize::Letter,
            orientation: Orientation::Portrait,
            scale: PrintScale::Fit,
            margin_pt: 18.0,
        },
    );
    let (w, h) = media_box(&portrait);
    assert!((w - 612.0).abs() < 0.01, "portrait width {w}");
    assert!((h - 792.0).abs() < 0.01, "portrait height {h}");

    let landscape = print_pdf(
        &doc,
        &roots,
        &PrintOptions {
            paper: PaperSize::Letter,
            orientation: Orientation::Landscape,
            scale: PrintScale::Fit,
            margin_pt: 18.0,
        },
    );
    let (w, h) = media_box(&landscape);
    assert!((w - 792.0).abs() < 0.01, "landscape width {w}");
    assert!((h - 612.0).abs() < 0.01, "landscape height {h}");
}

#[test]
fn print_pdf_a4_fit_keeps_content_inside_margins() {
    let (doc, roots) = sample_doc();
    let pdf = print_pdf(
        &doc,
        &roots,
        &PrintOptions {
            paper: PaperSize::A4,
            orientation: Orientation::Portrait,
            scale: PrintScale::Fit,
            margin_pt: 36.0,
        },
    );
    let (w, h) = media_box(&pdf);
    assert!((w - 595.28).abs() < 0.01, "A4 width {w}");
    assert!((h - 841.89).abs() < 0.01, "A4 height {h}");

    // A4 portrait is 595.28pt wide; 36pt margins leave 523.28pt. The 400pt-wide
    // shape must be scaled down by at most 523.28 / 400 = 1.3082.
    let transform = content_transform(&pdf);
    let scale: f64 = transform
        .split_whitespace()
        .next()
        .expect("transform scale")
        .parse()
        .expect("numeric scale");
    assert!((scale - 1.3082).abs() < 0.01, "fit scale {scale}");
}

#[test]
fn print_pdf_percent_scale_changes_transform_without_page_size() {
    let (doc, roots) = sample_doc();
    let fit = print_pdf(
        &doc,
        &roots,
        &PrintOptions {
            paper: PaperSize::Letter,
            orientation: Orientation::Portrait,
            scale: PrintScale::Fit,
            margin_pt: 18.0,
        },
    );
    let half = print_pdf(
        &doc,
        &roots,
        &PrintOptions {
            paper: PaperSize::Letter,
            orientation: Orientation::Portrait,
            scale: PrintScale::Percent(50.0),
            margin_pt: 18.0,
        },
    );
    assert_eq!(media_box(&fit), media_box(&half));
    assert_ne!(fit, half, "scale must change the content stream");
}
