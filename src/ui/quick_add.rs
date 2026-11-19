//! Quick-add (draw.io): double-clicking empty canvas drops a small picker at
//! that canvas point. Searching filters the registry; picking a shape inserts
//! it right there, committed as one undoable step like any insert.

use super::UiState;
use crate::editor::Editor;
use crate::geom::V2;
use crate::registry;
use crate::theme;
use egui::RichText;

/// State of the open quick-add picker.
#[derive(Debug, Default)]
pub struct QuickAdd {
    /// The canvas point the picker was opened at; picked shapes go here.
    pub world: V2,
    pub query: String,
}

pub fn dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut qa) = st.quick_add.take() else {
        return;
    };
    let mut open = true;
    // Anchor: the canvas point, as a screen position (canvas offset + world).
    let anchor = {
        let at = ed.camera.world_to_screen(qa.world);
        let min = st.canvas_rect.map(|c| c.min).unwrap_or_default();
        egui::pos2(min.x + at.x as f32 + 14.0, min.y + at.y as f32 + 14.0)
    };
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if esc {
        return;
    }
    let mut picked: Option<&'static str> = None;
    egui::Window::new("Quick add")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .current_pos(anchor)
        .default_width(240.0)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            let r = ui.add(
                egui::TextEdit::singleline(&mut qa.query)
                    .hint_text("Search shapes")
                    .desired_width(ui.available_width()),
            );
            if st.quick_add_focus {
                r.request_focus();
            }
            st.quick_add_focus = false;
            let results = registry::search(&qa.query);
            if results.is_empty() {
                ui.label(
                    RichText::new("No shapes match.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
            ui.add_space(2.0);
            let cols = 3;
            ui.spacing_mut().item_spacing.y = 4.0;
            for row in results.iter().take(15).collect::<Vec<_>>().chunks(cols) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for def in row {
                        let name = def.name.to_string();
                        if ui
                            .small_button(RichText::new(&name).size(theme::SECONDARY))
                            .on_hover_text(format!("Insert {} here", def.name))
                            .clicked()
                        {
                            picked = Some(def.id);
                        }
                    }
                });
            }
            let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
            if enter && let Some(def) = results.first() {
                picked = Some(def.id);
            }
        });
    if let Some(id) = picked {
        ed.insert_registry_shape(id, Some(qa.world), None, None, None);
        open = false;
    }
    st.quick_add = open.then_some(qa);
}
