//! Print dialog: paper size, orientation, scaling and margins for a paper-sized
//! PDF of the current page or selection (draw.io's print output, IO-19).

use crate::editor::Editor;
use crate::io::pdf::{PrintOptions, PrintScale, print_pdf};
use crate::model::{NodeId, Orientation, PaperSize};
use crate::theme;
use crate::ui::UiState;
use std::path::PathBuf;

/// Which content goes on the printed page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintScope {
    #[default]
    Page,
    Selection,
}

/// State of the open Print dialog. The chosen values are session state; they
/// are not written into the document, so printing never changes the diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintDialog {
    pub paper: PaperSize,
    pub orientation: Orientation,
    /// Whether to fit the content to the printable area or use an explicit scale.
    pub fit: bool,
    /// Explicit scale percentage, used when `fit` is false.
    pub percent: f64,
    /// Margin on every side, in PostScript points.
    pub margin_pt: f64,
    pub scope: PrintScope,
    /// Where the primary Print button writes the PDF. When absent, a folder
    /// under the system temp directory is used.
    pub output_dir: Option<PathBuf>,
    /// Validation or success message shown under the controls.
    pub message: String,
}

impl Default for PrintDialog {
    fn default() -> Self {
        Self {
            paper: PaperSize::Letter,
            orientation: Orientation::Portrait,
            fit: true,
            percent: 100.0,
            margin_pt: 18.0,
            scope: PrintScope::Page,
            output_dir: None,
            message: String::new(),
        }
    }
}

impl PrintDialog {
    /// Effective print options from the dialog state.
    pub fn options(&self) -> PrintOptions {
        let margin_pt = self.margin_pt.clamp(0.0, 500.0);
        PrintOptions {
            paper: self.paper,
            orientation: self.orientation,
            scale: if self.fit {
                PrintScale::Fit
            } else {
                PrintScale::Percent(self.percent.clamp(1.0, 1000.0))
            },
            margin_pt,
        }
    }

    /// Paper dimensions in points after orientation is applied.
    pub fn paper_points(&self) -> (f64, f64) {
        let (w, h) = self.paper.portrait_points();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    /// Scale factor that fits `bounds` inside the printable area. This mirrors
    /// the computation in `io::pdf::print_pdf` so the preview matches the file.
    pub fn fit_scale(&self, bounds: (f64, f64)) -> f64 {
        let (pw, ph) = self.paper_points();
        let m = self.margin_pt.clamp(0.0, 500.0);
        let avail_w = (pw - 2.0 * m).max(1.0);
        let avail_h = (ph - 2.0 * m).max(1.0);
        (avail_w / bounds.0.max(1.0)).min(avail_h / bounds.1.max(1.0))
    }
}

/// Render the modal-ish Print window. Called every frame from `ui::dialogs`.
pub fn dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut dlg) = st.print.take() else {
        return;
    };
    let mut open = true;
    let mut cancel = false;
    let mut print_now = false;
    let mut print_as = false;
    egui::Window::new("Print")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            ui.label("Print the current page as a paper-sized PDF.");

            ui.separator();
            egui::Grid::new("print-options")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Paper");
                    let papers = PaperSize::ALL;
                    let labels: Vec<String> =
                        papers.iter().map(|p| p.label().to_string()).collect();
                    let mut idx = papers.iter().position(|p| *p == dlg.paper).unwrap_or(0);
                    egui::ComboBox::from_id_salt("print-paper")
                        .selected_text(labels[idx].clone())
                        .show_ui(ui, |ui| {
                            for (i, label) in labels.iter().enumerate() {
                                ui.selectable_value(&mut idx, i, label);
                            }
                        });
                    dlg.paper = papers[idx];

                    ui.label("Orientation");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dlg.orientation, Orientation::Portrait, "Portrait");
                        ui.radio_value(&mut dlg.orientation, Orientation::Landscape, "Landscape");
                    });
                    ui.end_row();

                    ui.label("Scale");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dlg.fit, true, "Fit to page");
                        ui.radio_value(&mut dlg.fit, false, "Custom");
                    });
                    ui.end_row();

                    if !dlg.fit {
                        ui.label("Percentage");
                        ui.add(
                            egui::DragValue::new(&mut dlg.percent)
                                .range(1.0..=1000.0)
                                .speed(1.0)
                                .suffix("%"),
                        );
                        ui.end_row();
                    }

                    ui.label("Margin");
                    ui.add(
                        egui::DragValue::new(&mut dlg.margin_pt)
                            .range(0.0..=500.0)
                            .speed(1.0)
                            .suffix(" pt"),
                    );
                    ui.end_row();

                    ui.label("Print");
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dlg.scope, PrintScope::Page, "Whole page");
                        ui.radio_value(&mut dlg.scope, PrintScope::Selection, "Selection");
                    });
                    ui.end_row();
                });

            let (pw, ph) = dlg.paper_points();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "Page: {} × {} pt · margins {} pt",
                    pw.round() as i64,
                    ph.round() as i64,
                    dlg.margin_pt.round() as i64,
                ))
                .color(theme::pal(ui).secondary),
            );
            if !dlg.message.is_empty() {
                ui.label(&dlg.message);
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                print_now = ui.button("Print to PDF").clicked();
                print_as = ui.button("Choose location…").clicked();
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });

    if cancel {
        open = false;
    }
    if print_now || print_as {
        let name = crate::project::folder_name(&ed.doc.name);
        // The primary button writes to a deterministic folder so the result is
        // easy to find and to verify; "Choose location…" opens a save picker.
        let path = if print_as {
            rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(format!("{name}.pdf"))
                .save_file()
        } else {
            let dir = dlg
                .output_dir
                .clone()
                .unwrap_or_else(|| std::env::temp_dir().join("sketch-prints"));
            Some(dir.join(format!("{name}.pdf")))
        };
        if let Some(path) = path {
            let result =
                std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")))
                    .map_err(|e| format!("Could not create folder: {e}"))
                    .and_then(|_| {
                        let roots: Vec<NodeId> = match dlg.scope {
                            PrintScope::Page => ed.doc.layered_root(),
                            PrintScope::Selection => ed.top_selection(),
                        };
                        if roots.is_empty() {
                            return Err("Nothing to print: the selection is empty.".into());
                        }
                        let pdf = print_pdf(&ed.doc, &roots, &dlg.options());
                        std::fs::write(&path, &pdf).map_err(|e| format!("Could not write PDF: {e}"))
                    });
            match result {
                Ok(()) => {
                    dlg.message = format!("Saved {}", path.display());
                    ed.status = format!("Printed PDF to {}", path.display());
                }
                Err(e) => dlg.message = e,
            }
        }
    }

    if open {
        st.print = Some(dlg);
    }
}

/// Directory the primary Print button writes into: the project folder when a
/// project is open, otherwise a folder under the system temp directory.
pub fn default_output_dir(project_dir: Option<&std::path::Path>) -> PathBuf {
    project_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("sketch-prints"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_print_dialog_uses_letter_portrait_fit() {
        let dlg = PrintDialog::default();
        assert_eq!(dlg.paper, PaperSize::Letter);
        assert_eq!(dlg.orientation, Orientation::Portrait);
        assert_eq!(dlg.options().scale, PrintScale::Fit);
        assert_eq!(dlg.paper_points(), (612.0, 792.0));
    }

    #[test]
    fn landscape_swaps_paper_dimensions() {
        let dlg = PrintDialog {
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        assert_eq!(dlg.paper_points(), (792.0, 612.0));
    }

    #[test]
    fn fit_scale_shrinks_content_to_printable_area() {
        let dlg = PrintDialog::default();
        // 612pt wide, 18pt margins -> 576pt printable; a 400pt-wide shape fits
        // at most 1.44. 792pt tall -> 756pt printable; a 300pt shape fits at
        // most 2.52. Width wins.
        let scale = dlg.fit_scale((400.0, 300.0));
        assert!((scale - 1.44).abs() < 0.01, "fit scale {scale}");
    }

    #[test]
    fn custom_scale_uses_the_explicit_percentage() {
        let dlg = PrintDialog {
            fit: false,
            percent: 75.0,
            ..Default::default()
        };
        assert_eq!(dlg.options().scale, PrintScale::Percent(75.0));
    }

    #[test]
    fn output_dir_prefers_the_project_folder() {
        let project = PathBuf::from("/tmp/my-project");
        assert_eq!(default_output_dir(Some(&project)), project);
        assert!(default_output_dir(None).ends_with("sketch-prints"));
    }
}
