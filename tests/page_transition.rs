//! DIA-10: a page can fade in when the editor switches to it.

mod common;

use sketch_rs::editor::Editor;
use sketch_rs::io::drawio;
use sketch_rs::model::{DiagramSettings, Document, PageTransition};

fn fade_doc() -> Document {
    Document {
        name: "Fade".into(),
        settings: DiagramSettings {
            page_transition: PageTransition::Fade,
            page_transition_ms: 400,
            ..DiagramSettings::default()
        },
        ..Document::default()
    }
}

#[test]
fn page_transition_round_trips_in_native_page() {
    let doc = fade_doc();
    let value = serde_json::to_value(&doc).expect("serialize page");
    assert_eq!(value["settings"]["page_transition"], "fade");
    assert_eq!(value["settings"]["page_transition_ms"], 400);
    let back: Document = serde_json::from_value(value).expect("deserialize page");
    assert_eq!(back.settings.page_transition, PageTransition::Fade);
    assert_eq!(back.settings.page_transition_ms, 400);
}

#[test]
fn page_transition_round_trips_through_drawio() {
    let doc = fade_doc();
    let xml = drawio::export(&[("Fade", &doc)]);
    assert!(
        xml.contains("__sketchPageTransition=\"fade\""),
        "export carries the transition: {xml}"
    );
    assert!(
        xml.contains("__sketchPageTransitionMs=\"400\""),
        "export carries the duration: {xml}"
    );
    let (pages, _) = drawio::import_str(&xml).expect("reimport");
    assert_eq!(pages[0].doc.settings.page_transition, PageTransition::Fade);
    assert_eq!(pages[0].doc.settings.page_transition_ms, 400);
}

fn editor_with_fade_page() -> Editor {
    let mut ed = Editor::default();
    ed.add_page_in_background(Some("Fade"));
    let i = ed.pages.len() - 1;
    ed.with_page(i, |e| {
        e.doc.settings.page_transition = PageTransition::Fade;
        e.doc.settings.page_transition_ms = 400;
    });
    ed
}

#[test]
fn switching_to_a_fade_page_starts_a_transition() {
    let mut ed = editor_with_fade_page();
    let i = ed.pages.len() - 1;
    ed.tick(1000.0);
    assert_eq!(ed.page_transition_alpha(1000.0), 0.0);
    ed.switch_page(i);
    ed.tick(1000.0);
    assert_eq!(ed.page_transition_alpha(1000.0), 1.0);
    assert_eq!(ed.page_transition_alpha(1200.0), 0.5);
    assert_eq!(ed.page_transition_alpha(1400.0), 0.0);
    assert!(ed.animating(1200.0));
    ed.tick(1400.0);
    assert!(!ed.animating(1400.0));
    ed.switch_page(0);
    assert_eq!(ed.page_transition_alpha(1400.0), 0.0);
}

#[test]
fn transition_ignores_a_stale_editor_clock() {
    let mut ed = editor_with_fade_page();
    let i = ed.pages.len() - 1;
    ed.now_ms = 0.0;
    ed.switch_page(i);
    assert_eq!(ed.page_transition_alpha(5000.0), 1.0);
    ed.tick(5000.0);
    assert_eq!(ed.page_transition_alpha(5000.0), 1.0);
    assert_eq!(ed.page_transition_alpha(5200.0), 0.5);
    assert!(!ed.page_transition_pending);
}

#[test]
fn fade_transition_changes_a_rendered_frame() {
    let Some(gpu) = common::gpu() else {
        return;
    };
    let mut ed = editor_with_fade_page();
    let i = ed.pages.len() - 1;
    ed.tick(1000.0);
    ed.switch_page(i);
    ed.tick(1000.0);
    let covered = common::render_canvas(gpu, &ed);
    ed.tick(1600.0);
    let clear = common::render_canvas(gpu, &ed);
    let changed = covered
        .pixels()
        .zip(clear.pixels())
        .filter(|(a, b)| a != b)
        .count();
    assert!(changed > 100, "fade veil covers the page: {changed}");
}
