//! The import report shown after reading a draw.io file: what came in
//! exactly, what was approximated, and what was dropped.

use crate::io::drawio::ImportReport;
use crate::theme;
use egui::{Align2, RichText};

/// An import report plus the file it came from.
pub struct ImportReportUi {
    pub source: String,
    pub report: ImportReport,
}

pub fn dialog(ctx: &egui::Context, st: &mut crate::ui::UiState, canvas: egui::Rect) {
    let Some(ui_state) = &st.import_report else {
        return;
    };
    let width = 620.0f32.min(canvas.width() - 40.0);
    let mut open = true;
    egui::Area::new("import-report".into())
        .order(egui::Order::Foreground)
        .fade_in(false)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(canvas.center())
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(12.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("draw.io import report").font(theme::semibold(15.0)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Done").on_hover_text("Close (Esc)").clicked() {
                                open = false;
                            }
                        });
                    });
                    ui.label(
                        RichText::new(&ui_state.source)
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                    ui.add_space(6.0);
                    let r = &ui_state.report;
                    ui.horizontal_wrapped(|ui| {
                        for (label, value) in [
                            ("Pages", r.pages),
                            ("Shapes", r.shapes),
                            ("Connectors", r.connectors),
                            ("Layers", r.layers),
                            ("Approximated", r.approximated.len()),
                            ("Dropped", r.dropped.len()),
                        ] {
                            ui.label(
                                RichText::new(format!("{value}"))
                                    .font(theme::semibold(theme::BODY)),
                            );
                            ui.label(
                                RichText::new(label)
                                    .size(theme::SECONDARY)
                                    .color(p.secondary),
                            );
                            ui.add_space(10.0);
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height((canvas.height() - 220.0).max(160.0))
                        .show(ui, |ui| {
                            if !r.warnings.is_empty() {
                                ui.add_space(6.0);
                                ui.label(RichText::new("Warnings").font(theme::semibold(13.0)));
                                for w in &r.warnings {
                                    ui.label(RichText::new(format!("• {w}")).size(12.0));
                                }
                            }
                            for (title, items, color) in [
                                ("Approximated", &r.approximated, p.secondary),
                                (
                                    "Dropped",
                                    &r.dropped,
                                    egui::Color32::from_rgb(0xc0, 0x39, 0x2b),
                                ),
                            ] {
                                if items.is_empty() {
                                    continue;
                                }
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new(format!("{title} ({})", items.len()))
                                        .font(theme::semibold(13.0))
                                        .color(color),
                                );
                                for d in items {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} · {} · {}",
                                            d.page, d.cell, d.item
                                        ))
                                        .size(12.0)
                                        .color(color),
                                    );
                                    ui.label(
                                        RichText::new(&d.reason).size(12.0).color(p.secondary),
                                    );
                                }
                            }
                            if r.approximated.is_empty() && r.dropped.is_empty() {
                                ui.add_space(8.0);
                                ui.label(
                                    RichText::new("Everything imported exactly.")
                                        .size(12.0)
                                        .color(p.secondary),
                                );
                            }
                        });
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if !open {
        st.import_report = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::drawio::ImportReport;

    fn input(escape: bool) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        if escape {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            });
        }
        input
    }

    fn draw(ctx: &egui::Context, st: &mut crate::ui::UiState) {
        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut out = ctx.run_ui(input(false), |ui| dialog(ui.ctx(), st, canvas));
        out.textures_delta.clear();
    }

    #[test]
    fn report_renders_and_escape_closes_it() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut st = crate::ui::UiState {
            import_report: Some(ImportReportUi {
                source: "board.drawio".into(),
                report: ImportReport {
                    pages: 2,
                    shapes: 3,
                    connectors: 1,
                    layers: 1,
                    ..Default::default()
                },
            }),
            ..Default::default()
        };
        draw(&ctx, &mut st);
        assert!(st.import_report.is_some(), "stays open without Escape");

        let canvas = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut out = ctx.run_ui(input(true), |ui| dialog(ui.ctx(), &mut st, canvas));
        out.textures_delta.clear();
        assert!(st.import_report.is_none(), "Escape closes the report");
    }
}
