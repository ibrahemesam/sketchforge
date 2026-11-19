//! Insert-diagram dialog (M8). One text box feeds the Mermaid, CSV, SQL and
//! JSON parsers from `crate::generate`; the result goes through the ordinary
//! editor insertion paths, so it stays editable and undoable.

use super::UiState;
use crate::editor::Editor;
use crate::generate::{Layout, Options, Source};
use crate::theme;
use egui::{Align2, RichText, Ui};

#[derive(Default)]
pub struct GenerateUi {
    pub open: bool,
    pub source: Source,
    pub text: String,
    pub options: Options,
    pub error: String,
}

impl GenerateUi {
    /// Open the dialog for one source, keeping any text already typed.
    pub fn open_with(&mut self, source: Source) {
        self.open = true;
        self.source = source;
        self.error.clear();
    }

    /// Parse the text and insert the result. Returns false and keeps the
    /// dialog open when the parser rejects the text.
    pub fn insert(&mut self, ed: &mut Editor, as_page: bool) -> bool {
        match crate::generate::generate(self.source, &self.text, &self.options) {
            Ok(generated) => {
                ed.insert_generated(&generated, None, as_page);
                self.error.clear();
                self.open = false;
                true
            }
            Err(e) => {
                self.error = e;
                false
            }
        }
    }
}

pub fn dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    if !st.generate.open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.generate.open = false;
        return;
    }
    let mut open = st.generate.open;
    egui::Window::new("Insert diagram")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(580.0)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| body(ui, ed, st));
    st.generate.open = open;
}

fn body(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        ui.label("Source");
        let before = st.generate.source;
        egui::ComboBox::from_id_salt("generate-source")
            .selected_text(st.generate.source.label())
            .show_ui(ui, |ui| {
                for source in Source::ALL {
                    ui.selectable_value(&mut st.generate.source, source, source.label());
                }
            });
        if before != st.generate.source {
            st.generate.error.clear();
        }
    });
    ui.label(
        RichText::new(st.generate.source.hint())
            .size(theme::SECONDARY)
            .color(p.secondary),
    );
    ui.add_space(6.0);

    if matches!(st.generate.source, Source::Csv | Source::Json) {
        ui.horizontal(|ui| {
            ui.label("Layout");
            egui::ComboBox::from_id_salt("generate-layout")
                .selected_text(st.generate.options.layout.label())
                .show_ui(ui, |ui| {
                    for layout in Layout::ALL {
                        ui.selectable_value(
                            &mut st.generate.options.layout,
                            layout,
                            layout.label(),
                        );
                    }
                });
        });
    }
    if st.generate.source == Source::Csv {
        ui.horizontal(|ui| {
            ui.checkbox(&mut st.generate.options.header, "First row is a header");
            let mut chosen = st.generate.options.label_column.is_some();
            if ui
                .checkbox(&mut chosen, "Use one column as the label")
                .changed()
            {
                st.generate.options.label_column = chosen.then_some(0);
            }
            if let Some(column) = &mut st.generate.options.label_column {
                let mut one_based = *column + 1;
                if ui
                    .add(
                        egui::DragValue::new(&mut one_based)
                            .range(1..=64)
                            .prefix("column "),
                    )
                    .changed()
                {
                    *column = one_based.saturating_sub(1);
                }
            }
        });
    }
    ui.add_space(4.0);

    let width = ui.available_width().max(320.0);
    ui.add_sized(
        egui::vec2(width, 260.0),
        egui::TextEdit::multiline(&mut st.generate.text)
            .code_editor()
            .hint_text("Paste diagram text here")
            .desired_width(width),
    );

    if !st.generate.error.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(&st.generate.error)
                .size(theme::SECONDARY)
                .color(egui::Color32::from_rgb(0xc0, 0x39, 0x2b)),
        );
    }
    ui.add_space(6.0);
    let enabled = !st.generate.text.trim().is_empty();
    ui.horizontal(|ui| {
        if ui
            .add_enabled(enabled, egui::Button::new("Insert"))
            .on_hover_text("Add the diagram to the open page in one undo step")
            .clicked()
        {
            st.generate.insert(ed, false);
        }
        if ui
            .add_enabled(enabled, egui::Button::new("Insert as page"))
            .on_hover_text("Add the diagram as a new project page")
            .clicked()
        {
            st.generate.insert(ed, true);
        }
        if ui.button("Cancel").clicked() {
            st.generate.open = false;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> UiState {
        let mut st = UiState::default();
        st.generate.open_with(Source::Mermaid);
        st.generate.text = "flowchart TD\n A[Start] --> B[Done]\n".into();
        st
    }

    #[test]
    fn insertion_adds_content_and_closes_the_dialog() {
        let mut ed = Editor::default();
        let mut st = state();
        assert!(st.generate.insert(&mut ed, false));
        assert!(!st.generate.open);
        assert!(st.generate.error.is_empty());
        assert!(!ed.doc.nodes.is_empty());
        assert!(!ed.selection.is_empty());
        assert_eq!(
            ed.history.undo_labels().last(),
            Some("Insert Mermaid diagram")
        );
    }

    #[test]
    fn a_parser_error_stays_in_the_dialog() {
        let mut ed = Editor::default();
        let mut st = state();
        st.generate.text = "not a diagram".into();
        assert!(!st.generate.insert(&mut ed, false));
        assert!(st.generate.open);
        assert!(st.generate.error.contains("expected a Mermaid"));
        assert!(ed.doc.nodes.is_empty());
    }

    #[test]
    fn source_choices_and_layouts_round_trip() {
        let mut st = UiState::default();
        st.generate.open_with(Source::Csv);
        st.generate.options.layout = Layout::Grid;
        st.generate.options.header = true;
        assert_eq!(st.generate.source, Source::Csv);
        assert_eq!(st.generate.options.layout, Layout::Grid);
        assert!(st.generate.options.header);
    }

    #[test]
    fn page_insertion_switches_to_the_new_page() {
        let mut ed = Editor::default();
        let mut st = state();
        assert!(st.generate.insert(&mut ed, true));
        assert_eq!(ed.pages.len(), 2);
        assert_eq!(ed.active_page, 1);
        assert!(!ed.doc.nodes.is_empty());
    }

    #[test]
    fn the_dialog_runs_and_escape_closes_it() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = state();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input.clone(), |ui| dialog(ui.ctx(), &mut ed, &mut st));
        out.textures_delta.clear();
        assert!(st.generate.open);
        input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        });
        let mut out = ctx.run_ui(input, |ui| dialog(ui.ctx(), &mut ed, &mut st));
        out.textures_delta.clear();
        assert!(!st.generate.open);
    }
}
