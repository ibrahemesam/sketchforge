//! Template browser (M7): bundled diagram packs plus custom libraries from a
//! local folder. Every entry previews as vector content and inserts through the
//! normal editor paths, so what arrives stays editable.

use super::UiState;
use crate::editor::Editor;
use crate::template::{Template, TemplateLibrary, TemplatePage};
use crate::theme::{self, Palette};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Default)]
pub struct TemplateUi {
    pub open: bool,
    /// Index 0 is always the bundled library; the rest come from the custom
    /// folder, one per file.
    pub libraries: Vec<TemplateLibrary>,
    pub errors: Vec<String>,
    pub query: String,
    /// Empty means every category.
    pub category: String,
    pub selected: Option<(usize, usize)>,
    pub page: usize,
    /// Name for "Save current page as template".
    pub save_name: String,
    pub notice: String,
    preview: Option<(String, egui::Mesh)>,
}

impl TemplateUi {
    /// Load the bundled packs and rescan the custom folder. The folder path
    /// comes from options; a missing or unreadable folder only reports an
    /// error and leaves the bundled packs usable.
    pub fn load(&mut self, folder: Option<PathBuf>) {
        self.libraries.clear();
        self.errors.clear();
        self.libraries.push(TemplateLibrary::bundled());
        if let Some(dir) = folder {
            let (libs, errors) = TemplateLibrary::load_folder(&dir);
            self.libraries.extend(libs);
            self.errors = errors;
        }
        let valid = self.selected.is_some_and(|(li, ti)| {
            self.libraries
                .get(li)
                .is_some_and(|l| ti < l.templates.len())
        });
        if !valid {
            self.selected = None;
        }
        self.preview = None;
    }

    fn categories(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .libraries
            .iter()
            .flat_map(|l| l.templates.iter().map(|t| t.category.clone()))
            .collect();
        out.sort_by_key(|c| c.to_lowercase());
        out.dedup();
        out
    }
}

fn preview(lib: &mut TemplateUi, template: &Template, page: usize) -> egui::Mesh {
    let key = format!("{}|{}|{}", template.id, template.name, page);
    if let Some((k, mesh)) = &lib.preview
        && *k == key
    {
        return mesh.clone();
    }
    let mesh = template
        .preview_item(page)
        .map(|item| super::library::preview_mesh(&item, 260.0))
        .unwrap_or_default();
    lib.preview = Some((key, mesh.clone()));
    mesh
}

pub fn dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    if !st.templates.open {
        return;
    }
    let mut open = true;
    egui::Window::new("Templates")
        .open(&mut open)
        .default_size([760.0, 560.0])
        .min_size([560.0, 420.0])
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| body(ui, ed, st));
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.templates.open = false;
    }
}

fn body(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    folder_row(ui, st, &p);
    if !st.templates.errors.is_empty() {
        let text = st.templates.errors.join("\n");
        ui.label(
            RichText::new(format!(
                "{} library file(s) could not load",
                st.templates.errors.len()
            ))
            .size(theme::SECONDARY)
            .color(Color32::from_rgb(0xd0, 0x5a, 0x4a)),
        )
        .on_hover_text(text);
    }
    if !st.templates.notice.is_empty() {
        ui.label(
            RichText::new(st.templates.notice.clone())
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
    }
    ui.add_space(4.0);

    let height = (ui.available_height() - 12.0).max(240.0);
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(268.0, height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| list(ui, st, &p),
        );
        ui.separator();
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| detail(ui, ed, st, &p),
        );
    });
}

fn folder_row(ui: &mut Ui, st: &mut UiState, p: &Palette) {
    ui.horizontal(|ui| {
        let folder = st.options.template_folder.clone();
        match &folder {
            Some(dir) => {
                ui.label(
                    RichText::new(dir.display().to_string())
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
            None => {
                ui.label(
                    RichText::new("No custom template folder")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
        };
        if ui.button("Choose folder…").clicked()
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            st.options.template_folder = Some(dir.clone());
            st.options_dirty = true;
            st.templates.load(Some(dir));
        }
        if ui
            .add_enabled(folder.is_some(), egui::Button::new("Reload"))
            .on_hover_text("Rescan every .json library in the folder")
            .clicked()
        {
            st.templates.load(folder);
        }
    });
}

fn list(ui: &mut Ui, st: &mut UiState, p: &Palette) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add(
            egui::TextEdit::singleline(&mut st.templates.query)
                .hint_text("Search templates")
                .desired_width(ui.available_width() - 24.0),
        );
        if ui
            .small_button("×")
            .on_hover_text("Clear the search")
            .clicked()
        {
            st.templates.query.clear();
        }
    });
    let categories = st.templates.categories();
    let selected_category = st.templates.category.clone();
    egui::ComboBox::from_id_salt("template-category")
        .selected_text(if selected_category.is_empty() {
            "All categories"
        } else {
            &selected_category
        })
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(selected_category.is_empty(), "All categories")
                .clicked()
            {
                st.templates.category.clear();
            }
            for c in categories {
                if ui.selectable_label(selected_category == c, &c).clicked() {
                    st.templates.category = c;
                }
            }
        });
    ui.add_space(4.0);

    let query = st.templates.query.clone();
    let category = st.templates.category.clone();
    let mut entries: Vec<(usize, usize, String, String)> = Vec::new();
    for (li, lib) in st.templates.libraries.iter().enumerate() {
        for (ti, t) in lib.templates.iter().enumerate() {
            let visible = (category.is_empty() || t.category == category) && t.matches(&query);
            if visible {
                entries.push((li, ti, t.name.clone(), lib.name.clone()));
            }
        }
    }
    if st.templates.selected.is_none()
        && let Some((li, ti, ..)) = entries.first()
    {
        st.templates.selected = Some((*li, *ti));
        st.templates.page = 0;
    }

    egui::ScrollArea::vertical()
        .id_salt("template-list")
        .auto_shrink(false)
        .show(ui, |ui| {
            if entries.is_empty() {
                ui.label(
                    RichText::new("No templates match the search.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
                return;
            }
            let mut last_library = usize::MAX;
            for (li, ti, name, library) in &entries {
                if *li != last_library {
                    if last_library != usize::MAX {
                        ui.add_space(6.0);
                    }
                    ui.label(
                        RichText::new(library)
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                    last_library = *li;
                }
                let selected = st.templates.selected == Some((*li, *ti));
                if ui.selectable_label(selected, name).clicked() {
                    st.templates.selected = Some((*li, *ti));
                    st.templates.page = 0;
                    st.templates.preview = None;
                }
            }
        });
}

fn detail(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, p: &Palette) {
    let Some((li, ti)) = st.templates.selected else {
        ui.label(
            RichText::new("Pick a template to preview it.")
                .size(theme::SECONDARY)
                .color(p.secondary),
        );
        return;
    };
    let Some(template) = st
        .templates
        .libraries
        .get(li)
        .and_then(|l| l.templates.get(ti))
        .cloned()
    else {
        st.templates.selected = None;
        return;
    };
    let page = st
        .templates
        .page
        .min(template.pages.len().saturating_sub(1));

    ui.heading(&template.name);
    ui.label(
        RichText::new(&template.category)
            .size(theme::SECONDARY)
            .color(p.secondary),
    );
    if !template.description.is_empty() {
        ui.label(RichText::new(&template.description).size(theme::SECONDARY));
    }
    if template.pages.len() > 1 {
        let current = template
            .pages
            .get(page)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        egui::ComboBox::from_id_salt("template-page")
            .selected_text(current)
            .show_ui(ui, |ui| {
                for (i, pg) in template.pages.iter().enumerate() {
                    let label = if pg.name.trim().is_empty() {
                        format!("Page {}", i + 1)
                    } else {
                        pg.name.clone()
                    };
                    if ui.selectable_label(page == i, label).clicked() {
                        st.templates.page = i;
                        st.templates.preview = None;
                    }
                }
            });
    }

    let side = ui.available_width().clamp(160.0, 300.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter().rect_filled(rect, 8.0, p.control);
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, p.separator),
        egui::StrokeKind::Inside,
    );
    let mut mesh = preview(&mut st.templates, &template, page);
    let square = egui::vec2(260.0, 260.0);
    mesh.translate(rect.center().to_vec2() - square * 0.5);
    ui.painter().add(egui::Shape::mesh(mesh));

    ui.horizontal(|ui| {
        if ui
            .button("Insert on this page")
            .on_hover_text("Add this page's content to the open diagram")
            .clicked()
        {
            ed.insert_template_page(&template, page, None);
            st.templates.open = false;
        }
        if ui
            .button("Insert as page(s)")
            .on_hover_text("Add every page of the template to the project")
            .clicked()
        {
            ed.insert_template_pages(&template);
            st.templates.open = false;
        }
    });

    ui.add_space(6.0);
    ui.separator();
    ui.label(
        RichText::new("Save the current page")
            .size(theme::SECONDARY)
            .color(p.secondary),
    );
    ui.horizontal(|ui| {
        let hint = ed.page_name().to_string();
        ui.add(
            egui::TextEdit::singleline(&mut st.templates.save_name)
                .hint_text(hint)
                .desired_width(200.0),
        );
        if ui.button("Save").clicked() {
            save_current(ed, st);
        }
    });
}

fn save_current(ed: &mut Editor, st: &mut UiState) {
    let Some(dir) = st.options.template_folder.clone() else {
        st.templates.notice = "Choose a custom template folder first.".into();
        return;
    };
    let page_name = ed.page_name().to_string();
    let name = if st.templates.save_name.trim().is_empty() {
        page_name.clone()
    } else {
        st.templates.save_name.trim().to_string()
    };
    let page = TemplatePage::from_doc(&page_name, &ed.doc, &HashMap::new());
    let template = Template {
        id: slug(&name),
        name: name.clone(),
        description: format!("Saved from the {page_name} page."),
        category: "My templates".into(),
        keywords: Vec::new(),
        thumbnail: None,
        pages: vec![page],
    };
    match TemplateLibrary::save_template(&template, &dir) {
        Ok(path) => {
            st.templates.load(Some(dir));
            st.templates.notice = format!("Saved {}", path.display());
            st.templates.save_name.clear();
            st.templates.selected =
                st.templates
                    .libraries
                    .iter()
                    .enumerate()
                    .find_map(|(li, l)| {
                        l.templates
                            .iter()
                            .position(|t| t.name == name)
                            .map(|ti| (li, ti))
                    });
        }
        Err(e) => st.templates.notice = e,
    }
}

fn slug(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "saved-template".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(escape: bool) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 700.0),
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

    #[test]
    fn bundled_packs_load_without_a_custom_folder() {
        let mut ui = TemplateUi::default();
        ui.load(None);
        assert_eq!(ui.libraries.len(), 1);
        assert!(ui.libraries[0].templates.len() >= 15);
        assert!(ui.errors.is_empty());
        assert!(!ui.categories().is_empty());
    }

    #[test]
    fn a_custom_folder_adds_a_library_and_keeps_bad_files_visible() {
        let dir = std::env::temp_dir().join(format!("sketch-ui-tpl-{:016x}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        let lib = TemplateLibrary {
            name: "Mine".into(),
            templates: vec![TemplateLibrary::bundled().templates.remove(0)],
            ..TemplateLibrary::default()
        };
        lib.save(&dir.join("mine.json")).unwrap();
        std::fs::write(dir.join("bad.json"), b"{}").unwrap();
        let mut ui = TemplateUi::default();
        ui.load(Some(dir.clone()));
        assert_eq!(ui.libraries.len(), 2);
        assert_eq!(ui.libraries[1].name, "Mine");
        assert_eq!(ui.errors.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dialog_opens_previews_and_escape_closes_it() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = crate::editor::Editor::default();
        let mut st = crate::ui::UiState::default();
        st.templates.open = true;
        st.templates.load(None);

        let mut out = ctx.run_ui(input(false), |ui| dialog(ui.ctx(), &mut ed, &mut st));
        out.textures_delta.clear();
        assert!(st.templates.open);
        assert!(
            st.templates.selected.is_some(),
            "the browser picks the first template"
        );

        let mut out = ctx.run_ui(input(true), |ui| dialog(ui.ctx(), &mut ed, &mut st));
        out.textures_delta.clear();
        assert!(!st.templates.open, "Escape closes the browser");
    }
}
