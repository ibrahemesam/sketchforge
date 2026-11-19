//! The diagram find bar: cross-page search, match stepping and replace.

use super::UiState;
use crate::editor::Editor;
use crate::find::{self, FindMatch, Query};
use crate::model::NodeId;
use crate::theme;
use egui::{Align2, RichText};

#[derive(Default)]
pub struct FindUi {
    pub open: bool,
    pub query: String,
    pub replacement: String,
    pub show_replace: bool,
    pub regex: bool,
    pub case_sensitive: bool,
    pub matches: Vec<FindMatch>,
    pub current: usize,
    pub error: Option<String>,
    pub status: String,
    /// Focus the search field once per opening.
    pub focus: bool,
    sig: Option<Signature>,
}

#[derive(PartialEq)]
struct Signature {
    query: String,
    regex: bool,
    case_sensitive: bool,
    stamp: u64,
    pages: usize,
}

impl FindUi {
    pub fn open(&mut self) {
        self.open = true;
        self.focus = true;
    }

    /// Drop the cached result so the next frame searches again.
    fn invalidate(&mut self) {
        self.sig = None;
    }

    fn step(&mut self, ed: &mut Editor, next: bool) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len();
        self.current = if next {
            (self.current + 1) % n
        } else {
            (self.current + n - 1) % n
        };
        focus_match(ed, &self.matches[self.current]);
    }

    /// Search now (used before menu-driven stepping).
    pub fn ensure_match(&mut self, ed: &Editor) {
        self.refresh(ed);
    }

    /// Step to the next or previous match after ensuring the results are current.
    pub fn step_current(&mut self, ed: &mut Editor, next: bool) {
        self.refresh(ed);
        self.step(ed, next);
    }

    fn refresh(&mut self, ed: &Editor) {
        let sig = Signature {
            query: self.query.clone(),
            regex: self.regex,
            case_sensitive: self.case_sensitive,
            stamp: ed.doc_stamp,
            pages: ed.pages.len(),
        };
        if self.sig.as_ref().is_some_and(|s| s == &sig) {
            return;
        }
        let query_changed = self.sig.as_ref().is_none_or(|s| {
            s.query != sig.query || s.regex != sig.regex || s.case_sensitive != sig.case_sensitive
        });
        self.sig = Some(sig);
        self.error = None;
        self.status.clear();
        if self.query.is_empty() {
            self.matches.clear();
            self.current = 0;
            return;
        }
        match Query::new(&self.query, self.regex, self.case_sensitive) {
            Ok(q) => {
                self.matches = find::search(ed, &q);
                if query_changed {
                    self.current = 0;
                }
                if self.matches.is_empty() {
                    self.status = format!("No matches for “{}”.", self.query);
                } else if self.current >= self.matches.len() {
                    self.current = self.matches.len() - 1;
                }
            }
            Err(e) => {
                self.error = Some(e);
                self.matches.clear();
                self.current = 0;
            }
        }
    }
}

fn focus_match(ed: &mut Editor, m: &FindMatch) {
    if m.page != ed.active_page && m.page < ed.pages.len() {
        ed.switch_page(m.page);
    }
    let Some(id) = m.node else {
        return;
    };
    ed.select(vec![id]);
    if let Some(wr) = ed.doc.world_rect(id) {
        let c = wr.center();
        ed.camera.pan = c - ed.viewport / (2.0 * ed.camera.zoom);
        ed.revision += 1;
    }
}

/// Center a node in the viewport without changing the zoom.
pub fn center_on(ed: &mut Editor, id: NodeId) {
    if let Some(wr) = ed.doc.world_rect(id) {
        let c = wr.center();
        ed.camera.pan = c - ed.viewport / (2.0 * ed.camera.zoom);
        ed.revision += 1;
    }
}

pub fn draw(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState, canvas: egui::Rect) {
    if !st.find.open {
        return;
    }
    st.find.refresh(ed);
    let mut close = false;
    let mut step: Option<bool> = None;
    let mut replace_all = false;
    egui::Area::new("find-bar".into())
        .order(egui::Order::Foreground)
        .anchor(Align2::RIGHT_TOP, [-16.0, 44.0])
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(10.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.set_width(300.0);
                    ui.horizontal(|ui| {
                        let r = ui.add_sized(
                            [176.0, 22.0],
                            egui::TextEdit::singleline(&mut st.find.query)
                                .hint_text("Find in diagram"),
                        );
                        if st.find.focus {
                            r.request_focus();
                            st.find.focus = false;
                        }
                        st.find.invalidate_if_changed(&r);
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            step = Some(!ui.input(|i| i.modifiers.shift));
                        }
                        let count = if st.find.matches.is_empty() {
                            "0".to_string()
                        } else {
                            format!("{} of {}", st.find.current + 1, st.find.matches.len())
                        };
                        ui.label(
                            RichText::new(count)
                                .size(theme::SECONDARY)
                                .color(p.secondary),
                        );
                        if ui
                            .add_enabled(
                                !st.find.matches.is_empty(),
                                egui::Button::new("‹").frame(false),
                            )
                            .on_hover_text("Previous match")
                            .clicked()
                        {
                            step = Some(false);
                        }
                        if ui
                            .add_enabled(
                                !st.find.matches.is_empty(),
                                egui::Button::new("›").frame(false),
                            )
                            .on_hover_text("Next match")
                            .clicked()
                        {
                            step = Some(true);
                        }
                        if ui.button("×").on_hover_text("Close (Esc)").clicked() {
                            close = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut st.find.regex, "Regex").changed() {
                            st.find.invalidate();
                        }
                        if ui
                            .checkbox(&mut st.find.case_sensitive, "Aa")
                            .on_hover_text("Match case")
                            .changed()
                        {
                            st.find.invalidate();
                        }
                        if ui.checkbox(&mut st.find.show_replace, "Replace").changed()
                            && st.find.show_replace
                        {
                            st.find.focus = true;
                        }
                    });
                    if st.find.show_replace {
                        ui.horizontal(|ui| {
                            let r = ui.add_sized(
                                [176.0, 22.0],
                                egui::TextEdit::singleline(&mut st.find.replacement)
                                    .hint_text("Replace with"),
                            );
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                replace_all = true;
                            }
                            if ui
                                .add_enabled(
                                    !st.find.query.is_empty(),
                                    egui::Button::new("Replace All"),
                                )
                                .clicked()
                            {
                                replace_all = true;
                            }
                        });
                        if st.find.regex {
                            ui.label(
                                RichText::new("$0, $1 and named captures work in the replacement.")
                                    .size(theme::SECONDARY)
                                    .color(p.secondary),
                            );
                        }
                    }
                    if let Some(e) = &st.find.error {
                        ui.label(
                            RichText::new(e)
                                .size(theme::SECONDARY)
                                .color(egui::Color32::from_rgb(0xd1, 0x3b, 0x3b)),
                        );
                    } else if !st.find.status.is_empty() {
                        ui.label(
                            RichText::new(&st.find.status)
                                .size(theme::SECONDARY)
                                .color(p.secondary),
                        );
                    }
                    if let Some(m) = st.find.matches.get(st.find.current) {
                        let where_ = match m.node {
                            Some(id) => ed
                                .doc
                                .get(id)
                                .map(|n| n.name.clone())
                                .unwrap_or_else(|| format!("shape {}", id.0)),
                            None => format!("page {}", m.page + 1),
                        };
                        ui.label(
                            RichText::new(format!(
                                "{} · {} · {}",
                                where_,
                                m.field.label(),
                                m.preview
                            ))
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                        );
                    }
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if replace_all {
        let had_query = !st.find.query.is_empty();
        if had_query {
            match find::replace_all(
                ed,
                &st.find.query,
                &st.find.replacement,
                st.find.regex,
                st.find.case_sensitive,
            ) {
                Ok(r) => {
                    let mut msg = if r.replaced == 0 {
                        "Nothing to replace.".to_string()
                    } else {
                        format!("Replaced {} match(es) on {} page(s).", r.replaced, r.pages)
                    };
                    if r.skipped > 0 {
                        msg.push_str(&format!(
                            " Skipped {} in locked or hidden shapes.",
                            r.skipped
                        ));
                    }
                    st.find.status = msg;
                }
                Err(e) => st.find.error = Some(e),
            }
            st.find.invalidate();
        }
    }
    if let Some(next) = step {
        st.find.step(ed, next);
    }
    if close {
        st.find.open = false;
    }
    let _ = canvas;
}

impl FindUi {
    fn invalidate_if_changed(&mut self, r: &egui::Response) {
        if r.changed() {
            self.invalidate();
        }
    }
}
