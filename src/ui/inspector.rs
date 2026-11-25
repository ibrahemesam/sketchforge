//! The Design inspector: macOS-style grouped sections for the selection's geometry, layout,
//! constraints, text, fills, stroke and layer properties. Edits are coalesced per field so a
//! drag on a number is one undo step, and edits inside instances become overrides.

use super::{InspectorTab, UiState, c32, from32, title_text};
use crate::editor::shortcuts;
use crate::editor::vector_ops::BoolOp;
use crate::editor::{AppRequest, Editor};
use crate::geom::v2;
use crate::model::scheme::{SCHEMES, SchemeScope};
use crate::model::{
    AdaptiveColors, BlendMode, Cap, Color, Constraint, FlowDirection, FlowTiming, LayerId,
    LineJump, Node, NodeId, NodeKind, Paint, Route, Sizing, Stop, TextAlign,
};
use crate::theme;
use egui::{RichText, Ui};
use std::cell::{Cell, RefCell};

const LABEL_W: f32 = 64.0;

thread_local! {
    /// The active section's row filter. `None` means every row is visible.
    static ROW_FILTER: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Sections drawn in the current panel frame, for the empty-filter message.
    static SECTION_COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Apply a property edit as a coalesced transaction keyed by field + selection.
pub fn apply(ed: &mut Editor, field: &str, ids: &[NodeId], f: impl Fn(&mut Node)) {
    let key = format!("{field}:{ids:?}");
    ed.begin_coalescing(&format!("Change {field}"), Some(&key));
    {
        let mut tx = ed.tx();
        for id in ids {
            if let Some(n) = tx.node_mut(*id) {
                f(n);
            }
        }
    }
    ed.mark_overrides(ids, field);
    for id in ids {
        ed.fit_text(*id);
    }
    ed.commit();
}

fn seal_on_release(ed: &mut Editor, r: &egui::Response) {
    if r.drag_stopped() || r.lost_focus() {
        ed.history.seal();
    }
}

fn normalize_query(text: &str) -> String {
    text.trim().to_lowercase()
}

fn query_matches(query: &str, text: &str) -> bool {
    query.is_empty() || text.to_lowercase().contains(query)
}

/// A section is kept when its title or one of its property labels matches.
fn filter_matches(query: &str, title: &str, labels: &[&str]) -> bool {
    let query = normalize_query(query);
    query.is_empty()
        || query_matches(&query, title)
        || labels.iter().any(|l| query_matches(&query, l))
}

fn section_open(st: &UiState, key: &str) -> bool {
    st.options
        .inspector_sections
        .get(key)
        .copied()
        .unwrap_or(true)
}

fn set_section_open(st: &mut UiState, key: &str, open: bool) {
    if section_open(st, key) == open {
        return;
    }
    st.options.inspector_sections.insert(key.to_string(), open);
    st.options_dirty = true;
}

fn replace_row_filter(filter: Option<String>) -> Option<String> {
    ROW_FILTER.with(|f| {
        let mut slot = f.borrow_mut();
        std::mem::replace(&mut *slot, filter)
    })
}

fn row_visible(label: &str) -> bool {
    ROW_FILTER.with(|f| f.borrow().as_ref().is_none_or(|q| query_matches(q, label)))
}

fn count_section() {
    SECTION_COUNT.with(|c| c.set(c.get() + 1));
}

fn section_header(ui: &mut Ui, st: &mut UiState, key: &str, title: &str, forced: bool) -> bool {
    let mut open = forced || section_open(st, key);
    ui.horizontal(|ui| {
        let icon = if open {
            theme::Icon::ChevronDown
        } else {
            theme::Icon::ChevronRight
        };
        let r = icon_btn(
            ui,
            icon,
            false,
            if forced {
                "Clear the filter to collapse sections"
            } else if open {
                "Collapse section"
            } else {
                "Expand section"
            },
        );
        if r && !forced {
            open = !open;
            set_section_open(st, key, open);
        }
        ui.label(title_text(title));
    });
    open
}

struct SectionGuard {
    previous: Option<String>,
}

impl Drop for SectionGuard {
    fn drop(&mut self) {
        replace_row_filter(self.previous.take());
    }
}

/// Open a format-panel section. The guard restores the parent row filter when
/// it drops, so sections can nest and callers may still borrow other `UiState`
/// fields for the body.
fn section_begin(
    ui: &mut Ui,
    st: &mut UiState,
    key: &str,
    title: &str,
    labels: &[&str],
) -> Option<SectionGuard> {
    let query = normalize_query(&st.inspector_filter);
    if !filter_matches(&query, title, labels) {
        return None;
    }
    count_section();
    let forced = !query.is_empty();
    let open = section_header(ui, st, key, title, forced);
    if !open {
        return None;
    }
    let row_filter = if query.is_empty() || query_matches(&query, title) {
        None
    } else {
        Some(query)
    };
    Some(SectionGuard {
        previous: replace_row_filter(row_filter),
    })
}

/// Convenience wrapper for sections that do not need `UiState` in their body.
fn section<R>(
    ui: &mut Ui,
    st: &mut UiState,
    key: &str,
    title: &str,
    labels: &[&str],
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let _guard = section_begin(ui, st, key, title, labels)?;
    Some(add(ui))
}

/// A row: secondary label on the left, controls on the right.
fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    let visible = row_visible(label);
    ui.scope(|ui| {
        if !visible {
            ui.set_invisible();
        }
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(LABEL_W, 20.0), egui::Sense::hover());
            ui.painter().text(
                r.left_center(),
                egui::Align2::LEFT_CENTER,
                label,
                egui::FontId::proportional(theme::SECONDARY + 1.0),
                theme::pal(ui).secondary,
            );
            add(ui)
        })
        .inner
    })
    .inner
}

/// A compact number field with a leading letter (like Figma's "X 120").
fn num_field(
    ui: &mut Ui,
    ed: &mut Editor,
    prefix: &str,
    value: f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    width: f32,
) -> Option<f64> {
    let mut v = value;
    let r = ui.add_sized(
        egui::vec2(width, 20.0),
        egui::DragValue::new(&mut v)
            .speed(speed)
            .range(range)
            .max_decimals(2)
            .prefix(prefix),
    );
    seal_on_release(ed, &r);
    (r.changed() && v != value).then_some(v)
}

fn color_button(ui: &mut Ui, ed: &mut Editor, c: Color) -> Option<Color> {
    let mut c2 = c32(c);
    let r = ui.color_edit_button_srgba(&mut c2);
    seal_on_release(ed, &r);
    (r.changed() && from32(c2) != c).then(|| from32(c2))
}

/// A secondary swatch for an explicit dark-mode color. The displayed value falls
/// back to the light color until a dark override is stored.
fn dark_color_button(ui: &mut Ui, ed: &mut Editor, c: Color, what: &str) -> Option<Color> {
    ui.label(
        RichText::new("Dark")
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
    )
    .on_hover_text(format!("Dark-mode {what}"));
    color_button(ui, ed, c)
}

fn combo<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    current: T,
    options: &[(T, &str)],
    width: f32,
) -> Option<T> {
    let mut v = current;
    let label = options
        .iter()
        .find(|(o, _)| *o == current)
        .map_or("—", |(_, l)| *l);
    egui::ComboBox::from_id_salt(id)
        .selected_text(label)
        .width(width)
        .show_ui(ui, |ui| {
            for (o, l) in options {
                ui.selectable_value(&mut v, *o, *l);
            }
        });
    (v != current).then_some(v)
}

fn separator(ui: &mut Ui) {
    ui.add_space(6.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(
        r.x_range(),
        r.top(),
        egui::Stroke::new(1.0, theme::pal(ui).separator),
    );
    ui.add_space(8.0);
}

fn icon_btn(ui: &mut Ui, icon: theme::Icon, on: bool, tip: &str) -> bool {
    theme::icon_button(ui, icon, 24.0, on)
        .on_hover_text(tip)
        .clicked()
}

fn icon_toggle(ui: &mut Ui, label: &str, on: bool, tip: &str) -> bool {
    ui.add(
        egui::Button::selectable(on, RichText::new(label).font(theme::semibold(12.0)))
            .min_size(egui::vec2(24.0, 20.0)),
    )
    .on_hover_text(tip)
    .clicked()
}

/// Modal-ish inspector dialogs (advanced style, diagram data, background).
/// Called every frame, even when the format panel itself is closed.
pub fn dialogs(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    style_json_dialog(ctx, ed, st);
    metadata_dialog(ctx, ed, st);
    background_dialog(ctx, ed, st);
    layers_dialog(ctx, ed, st);
    crate::ui::print::dialog(ctx, ed, st);
}

/// State of the open Layers dialog.
#[derive(Debug, Default)]
pub struct LayersDialog {
    /// The layer whose name is being edited inline, with the field buffer.
    pub rename: Option<(LayerId, String)>,
    /// The rename field is focused once when it opens.
    pub focus: bool,
}

/// The page-layers window: membership, visibility, lock and layer stacking
/// (draw.io's layer list), separate from the z-order Layers panel in the sidebar.
fn layers_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut dlg) = st.layers_dialog.take() else {
        return;
    };
    let mut open = true;
    let mut want_focus = false;
    egui::Window::new("Layers")
        .collapsible(false)
        .resizable(true)
        .default_width(430.0)
        .open(&mut open)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            ui.label(
                RichText::new(
                    "Page layers with their own visibility, lock and stacking. Moving a shape
                     inside a group moves the whole group. The Layers tab next to Library is
                     the z-order tree, not this list.",
                )
                .size(theme::SECONDARY)
                .color(p.secondary),
            );
            ui.add_space(2.0);
            if ui.button("+ Add layer").clicked() {
                ed.layer_add();
            }
            ui.add_space(2.0);
            let layers = ed.doc.layers.clone();
            let sel = ed.selection.clone();
            // Frontmost first, like draw.io's layer list; the doc list runs
            // bottom first, so a row's "up" moves it later in the doc list.
            for layer in layers.iter().rev() {
                let id = layer.id;
                let pos = layers.iter().position(|l| l.id == id).unwrap_or(0);
                ui.separator();
                ui.horizontal(|ui| {
                    let editing = dlg
                        .rename
                        .as_ref()
                        .map(|(rid, _)| *rid == id)
                        .unwrap_or(false);
                    // Eye: visibility.
                    let eye = theme::icon_button(
                        ui,
                        if layer.visible {
                            theme::Icon::Eye
                        } else {
                            theme::Icon::EyeSlash
                        },
                        20.0,
                        !layer.visible,
                    );
                    if eye.clicked() {
                        ed.layer_set_visible(id, !layer.visible);
                    }
                    eye.on_hover_text(if layer.visible {
                        format!("Hide {}", layer.name)
                    } else {
                        format!("Show {}", layer.name)
                    });
                    // Padlock: lock.
                    let lock = theme::icon_button(
                        ui,
                        if layer.locked {
                            theme::Icon::Lock
                        } else {
                            theme::Icon::LockOpen
                        },
                        20.0,
                        layer.locked,
                    );
                    if lock.clicked() {
                        ed.layer_set_locked(id, !layer.locked);
                    }
                    lock.on_hover_text(if layer.locked {
                        format!("Unlock {}", layer.name)
                    } else {
                        format!("Lock {}", layer.name)
                    });
                    if editing {
                        let Some((_, text)) = dlg.rename.as_mut() else {
                            return;
                        };
                        if dlg.focus {
                            want_focus = true;
                            dlg.focus = false;
                        }
                        let r = ui.add(
                            egui::TextEdit::singleline(text)
                                .id(egui::Id::new("layers-rename"))
                                .desired_width(130.0),
                        );
                        let entered = r.lost_focus()
                            || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                        if entered {
                            let name = text.trim().to_string();
                            if !name.is_empty() && name != layer.name {
                                ed.layer_rename(id, name);
                            }
                            dlg.rename = None;
                        }
                    } else {
                        let name_click = ui
                            .label(
                                RichText::new(&layer.name)
                                    .font(theme::semibold(13.0))
                                    .color(if layer.locked || !layer.visible {
                                        p.secondary
                                    } else {
                                        p.text
                                    }),
                            )
                            .interact(egui::Sense::click())
                            .clicked();
                        if name_click {
                            dlg.rename = Some((id, layer.name.clone()));
                            dlg.focus = true;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if layers.len() > 1
                            && theme::icon_button(ui, theme::Icon::Trash, 20.0, false)
                                .on_hover_text(format!("Delete {} with its shapes", layer.name))
                                .clicked()
                        {
                            ed.layer_delete(id);
                        }
                        if theme::icon_button(ui, theme::Icon::Duplicate, 20.0, false)
                            .on_hover_text(format!("Duplicate {} with its shapes", layer.name))
                            .clicked()
                        {
                            ed.layer_duplicate(id);
                        }
                        if pos + 1 < layers.len()
                            && theme::icon_button(ui, theme::Icon::ChevronUp, 20.0, false)
                                .on_hover_text("Move this layer up one spot")
                                .clicked()
                        {
                            ed.layer_reorder(id, pos + 1);
                        }
                        if pos > 0
                            && theme::icon_button(ui, theme::Icon::ChevronDown, 20.0, false)
                                .on_hover_text("Move this layer down one spot")
                                .clicked()
                        {
                            ed.layer_reorder(id, pos - 1);
                        }
                        if theme::icon_button(ui, theme::Icon::SelectAll, 20.0, false)
                            .on_hover_text("Select every shape on this layer")
                            .clicked()
                        {
                            let ids: Vec<NodeId> = ed
                                .doc
                                .layered_root()
                                .into_iter()
                                .filter(|rid| {
                                    ed.doc.effective_layer(*rid) == id
                                        && ed.doc.is_effectively_visible(*rid)
                                        && !ed.doc.is_effectively_locked(*rid)
                                })
                                .collect();
                            ed.select(ids);
                        }
                        if !sel.is_empty()
                            && theme::icon_button(ui, theme::Icon::ArrowRight, 20.0, false)
                                .on_hover_text("Move the selection to this layer")
                                .clicked()
                        {
                            ed.move_selection_to_layer(&sel, id);
                        }
                    });
                });
            }
            ui.separator();
            let orphans = ed
                .doc
                .root
                .iter()
                .filter(|id| {
                    ed.doc
                        .get(**id)
                        .map(|n| {
                            let eff = ed.doc.effective_layer(n.id);
                            ed.doc.layer_by_id(eff).is_none()
                        })
                        .unwrap_or(false)
                })
                .count();
            if orphans > 0 {
                ui.label(
                    RichText::new(format!(
                        "{orphans} shape(s) name a layer this page no longer has; they paint \
                         above everything until they are moved."
                    ))
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
                );
            }
        });
    if want_focus {
        ctx.memory_mut(|m| m.request_focus(egui::Id::new("layers-rename")));
    }
    if open {
        st.layers_dialog = Some(dlg);
    }
}

fn style_json_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut dlg) = st.style_json.take() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    let mut do_apply = false;
    egui::Window::new("Edit Style")
        .collapsible(false)
        .resizable(true)
        .default_width(380.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new("Element style as JSON. Geometry, labels, connections and text style are separate.")
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
            let r = ui.add(
                egui::TextEdit::multiline(&mut dlg.json)
                    .code_editor()
                    .desired_rows(12)
                    .desired_width(f32::INFINITY),
            );
            if let Some(e) = &dlg.error {
                ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xd1, 0x3b, 0x3b)));
            }
            ui.horizontal(|ui| {
                let enter = r.has_focus()
                    && ui.input(|i| {
                        i.key_pressed(egui::Key::Enter) && (i.modifiers.command || i.modifiers.ctrl)
                    });
                if ui.button("Apply").clicked() || enter {
                    do_apply = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close {
        open = false;
    }
    if do_apply {
        match serde_json::from_str::<crate::model::Style>(&dlg.json) {
            Ok(mut style) => {
                style.sanitize();
                let ids = ed.top_selection();
                if ids.is_empty() {
                    dlg.error = Some("Nothing is selected any more.".into());
                } else {
                    apply(ed, "advanced style", &ids, |n| n.style = style.clone());
                    open = false;
                }
            }
            Err(e) => dlg.error = Some(format!("Invalid style: {e}")),
        }
    }
    if open {
        st.style_json = Some(dlg);
    }
}

fn metadata_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(mut dlg) = st.metadata_edit.take() else {
        return;
    };
    let mut open = true;
    let mut close = false;
    let mut do_apply = false;
    let is_page = matches!(dlg.target, MetadataTarget::Page);
    let title = if is_page {
        "Diagram Data"
    } else {
        "Shape Data"
    };
    let hint = if is_page {
        "Key/value properties stored with this page."
    } else {
        "Key/value properties stored with the selected shapes."
    };
    egui::Window::new(title)
        .collapsible(false)
        .resizable(true)
        .default_width(380.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(hint)
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
            let mut remove = None;
            for (i, (key, value)) in dlg.rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(key)
                            .hint_text("Key")
                            .desired_width(110.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(value)
                            .hint_text("Value")
                            .desired_width(170.0),
                    );
                    if ui
                        .small_button("−")
                        .on_hover_text("Remove property")
                        .clicked()
                    {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                dlg.rows.remove(i);
            }
            if ui.button("+ Add property").clicked() {
                dlg.rows.push((String::new(), String::new()));
            }
            if let Some(e) = &dlg.error {
                ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xd1, 0x3b, 0x3b)));
            }
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    do_apply = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close {
        open = false;
    }
    if do_apply {
        let mut map = std::collections::BTreeMap::new();
        let mut error = None;
        for (key, value) in &dlg.rows {
            let key = key.trim();
            if key.is_empty() {
                if !value.trim().is_empty() {
                    error = Some("A value needs a key.".to_string());
                    break;
                }
                continue;
            }
            if map.insert(key.to_string(), value.clone()).is_some() {
                error = Some(format!("Duplicate key “{key}”."));
                break;
            }
        }
        if let Some(e) = error {
            dlg.error = Some(e);
        } else {
            match &dlg.target {
                MetadataTarget::Page => {
                    ed.edit_settings("Edit diagram data", |s| s.metadata = map);
                    open = false;
                }
                MetadataTarget::Nodes(ids) => {
                    let ids: Vec<NodeId> = ids
                        .iter()
                        .copied()
                        .filter(|id| ed.doc.get(*id).is_some())
                        .collect();
                    if ids.is_empty() {
                        dlg.error = Some("Nothing is selected any more.".into());
                    } else {
                        ed.edit("Edit shape data", |tx| {
                            for id in &ids {
                                if let Some(n) = tx.node_mut(*id) {
                                    n.metadata = map.clone();
                                }
                            }
                        });
                        open = false;
                    }
                }
            }
        }
    }
    if open {
        st.metadata_edit = Some(dlg);
    }
}

fn background_dialog(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    if !st.background_open {
        return;
    }
    let mut open = true;
    let mut close = false;
    let mut do_apply = false;
    let mut draft = st
        .background_draft
        .unwrap_or(ed.doc.settings.background_color);
    let mut dark_draft = st
        .background_dark_draft
        .unwrap_or_else(|| ed.doc.settings.background_color_dark.unwrap_or(draft));
    egui::Window::new("Background")
        .collapsible(false)
        .resizable(false)
        .default_width(280.0)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.label(title_text("Page background"));
            ui.horizontal(|ui| {
                ui.label("Light");
                let mut c = c32(draft);
                if ui.color_edit_button_srgba(&mut c).changed() {
                    draft = from32(c);
                }
                ui.label("Dark");
                let mut dc = c32(dark_draft);
                if ui.color_edit_button_srgba(&mut dc).changed() {
                    dark_draft = from32(dc);
                }
            });
            if let Some(c) = swatches(ui, draft) {
                draft = c;
            }
            ui.separator();
            ui.label(
                RichText::new("Background image")
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
            let has_image = ed.doc.settings.background_image.is_some();
            ui.horizontal(|ui| {
                if ui.button("Choose image…").clicked() {
                    ed.requests.push(AppRequest::BackgroundImage);
                }
                if has_image && ui.button("Clear").clicked() {
                    ed.edit_settings("Clear background image", |s| s.background_image = None);
                }
            });
            if has_image {
                ui.horizontal(|ui| {
                    ui.label("Fit");
                    let modes = [
                        (crate::model::ImageMode::Fill, "Fill"),
                        (crate::model::ImageMode::Fit, "Fit"),
                        (crate::model::ImageMode::Stretch, "Stretch"),
                        (crate::model::ImageMode::Tile, "Tile"),
                    ];
                    if let Some(mode) =
                        combo(ui, "bg-image-mode", st.background_image_mode, &modes, 90.0)
                    {
                        st.background_image_mode = mode;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Opacity");
                    ui.add(
                        egui::Slider::new(&mut st.background_image_opacity, 0.0..=1.0)
                            .text("")
                            .show_value(true),
                    );
                });
            }
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    do_apply = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
    if close {
        open = false;
    }
    st.background_draft = Some(draft);
    st.background_dark_draft = Some(dark_draft);
    if do_apply {
        ed.edit_settings("Change page background", |s| {
            s.background_color = draft;
            s.background_color_dark = Some(dark_draft);
            s.background_enabled = true;
            if let Some(bg) = s.background_image.as_mut() {
                bg.mode = st.background_image_mode;
                bg.opacity = st.background_image_opacity;
            }
        });
        open = false;
    }
    st.background_open = open;
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    if st.hide_inspector {
        return;
    }
    let ids = ed.top_selection();
    let has_sel = !ids.is_empty();
    // Keep the active tab when it still applies; otherwise fall back to the
    // first one for the current selection context.
    if !st.inspector_tab.valid_for(has_sel, &ids, &ed.doc) {
        st.inspector_tab = InspectorTab::first_for(has_sel, &ids, &ed.doc);
    }
    let vis = ui.available_width();
    tabs(ui, st, has_sel, &ids, ed, vis);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let clear_w = if st.inspector_filter.is_empty() {
            0.0
        } else {
            24.0
        };
        let width = (vis - clear_w).max(80.0);
        ui.add(
            egui::TextEdit::singleline(&mut st.inspector_filter)
                .hint_text("Filter format properties…")
                .desired_width(width),
        );
        if !st.inspector_filter.is_empty()
            && ui
                .small_button("×")
                .on_hover_text("Clear the property filter")
                .clicked()
        {
            st.inspector_filter.clear();
        }
    });
    ui.add_space(4.0);
    SECTION_COUNT.with(|c| c.set(0));
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            // `vis` comes from the panel itself, outside this ScrollArea. With
            // `auto_shrink(false)` the scroll area adopts the previous frame's
            // content width, so a width read inside here grows a little every
            // frame, and any row sized from it grows with it — the panel would
            // creep wider forever.
            if !has_sel {
                match st.inspector_tab {
                    InspectorTab::Style => global_style_tab(ui, ed, st, vis),
                    _ => diagram_tab(ui, ed, st, vis),
                }
            } else {
                let first = ed.doc.get(ids[0]).cloned().unwrap();
                selection_line(ui, ed, &ids, &first);
                ui.add_space(1.0);
                match st.inspector_tab {
                    InspectorTab::Text => text_tab(ui, ed, st, vis, &ids, &first),
                    InspectorTab::Arrange => arrange_section(ui, ed, st, vis, &ids, &first),
                    InspectorTab::Data => data_tab(ui, ed, st, vis, &ids, &first),
                    _ => style_tab(ui, ed, st, vis, &ids, &first),
                }
            }
            if !st.inspector_filter.trim().is_empty() && SECTION_COUNT.with(|c| c.get()) == 0 {
                ui.label(
                    RichText::new("No matching properties.")
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            }
        });
}

/// The draw.io-style inspector tabs, plus the close button. Global context
/// shows Diagram and Style; element selections show a context-sensitive subset.
fn tabs(ui: &mut Ui, st: &mut UiState, has_sel: bool, ids: &[NodeId], ed: &Editor, vis: f32) {
    let list = InspectorTab::tabs_for(has_sel, ids, &ed.doc);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        let close_w = 22.0;
        let n = list.len() as f32;
        let w = ((vis - close_w - 3.0 * n) / n).max(40.0);
        for tab in &list {
            let active = st.inspector_tab == *tab;
            let txt = RichText::new(tab.label()).size(theme::BODY);
            let btn = egui::Button::selectable(active, if active { txt.strong() } else { txt });
            if ui.add_sized([w, 22.0], btn).clicked() {
                st.inspector_tab = *tab;
            }
        }
        if ui
            .add_sized([close_w, 22.0], egui::Button::new("×").frame(false))
            .on_hover_text("Close the format panel (⌘⇧F)")
            .clicked()
        {
            st.hide_inspector = true;
        }
    });
}

impl InspectorTab {
    pub fn label(self) -> &'static str {
        match self {
            InspectorTab::Diagram => "Diagram",
            InspectorTab::Style => "Style",
            InspectorTab::Text => "Text",
            InspectorTab::Arrange => "Arrange",
            InspectorTab::Data => "Data",
        }
    }
}

/// One-line context above the tab body: the element's kind and name, or the
/// number of selected layers.
fn selection_line(ui: &mut Ui, ed: &Editor, ids: &[NodeId], first: &Node) {
    let text = if ids.len() > 1 {
        format!("{} elements selected", ids.len())
    } else {
        let kind = if first.component {
            "Component"
        } else if ed.is_instance_root(first.id) {
            "Instance"
        } else {
            first.kind.type_name()
        };
        format!("{kind} · {}", first.name)
    };
    ui.label(
        RichText::new(text)
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
    );
}

/// Whether every value in `values` is equal, for a Mixed state.
fn all_equal<T: PartialEq>(values: &[T]) -> bool {
    values
        .first()
        .is_none_or(|first| values.iter().all(|v| v == first))
}

/// A checkbox that shows egui's indeterminate dash when the selection holds
/// different values. Returns the new value when clicked.
fn mixed_checkbox(ui: &mut Ui, label: &str, values: &[bool]) -> Option<bool> {
    let first = *values.first()?;
    let mixed = !all_equal(values);
    let mut v = first;
    let r = ui.add(egui::Checkbox::new(&mut v, label).indeterminate(mixed));
    r.changed().then_some(v)
}

fn mixed_badge(ui: &mut Ui) {
    ui.label(
        RichText::new("Mixed")
            .size(theme::SECONDARY)
            .color(theme::accent()),
    );
}

/// FigJam/Excalidraw-style sticky colours.
pub const STICKY_COLORS: [Color; 6] = [
    Color::rgb(0xff, 0xdf, 0x6b),
    Color::rgb(0xff, 0xb8, 0xa8),
    Color::rgb(0xb5, 0xe6, 0xa2),
    Color::rgb(0xa8, 0xd8, 0xff),
    Color::rgb(0xd7, 0xc2, 0xff),
    Color::rgb(0xe8, 0xe8, 0xec),
];

/// draw.io-style palette colors shown at the top of the Style tab. Paged in
/// groups of eight, like the reference's swatch pager.
const PALETTE_COLORS: [Color; 24] = [
    Color::rgb(0x14, 0x15, 0x1a),
    Color::rgb(0x97, 0xd0, 0x2b),
    Color::rgb(0x2e, 0x9e, 0x4a),
    Color::rgb(0x3b, 0xb7, 0xd4),
    Color::rgb(0x2b, 0x6f, 0xd8),
    Color::rgb(0x7b, 0x3c, 0xd6),
    Color::rgb(0xd6, 0x3f, 0x9e),
    Color::rgb(0xb4, 0x1e, 0x2e),
    Color::rgb(0xf5, 0xb7, 0x6b),
    Color::rgb(0xff, 0xff, 0xff),
    Color::rgb(0x6b, 0x72, 0x80),
    Color::rgb(0xd1, 0xd5, 0xdb),
    Color::rgb(0x1f, 0x3a, 0x5f),
    Color::rgb(0x0e, 0x80, 0x88),
    Color::rgb(0x2a, 0x9d, 0x8f),
    Color::rgb(0xc9, 0xd8, 0xc3),
    Color::rgb(0xff, 0xd9, 0x66),
    Color::rgb(0xfc, 0xe6, 0xc9),
    Color::rgb(0x94, 0xb0, 0xc8),
    Color::rgb(0x5a, 0x82, 0xa2),
    Color::rgb(0xff, 0xb8, 0xa8),
    Color::rgb(0xe8, 0xe8, 0xec),
    Color::rgb(0x8b, 0x5e, 0x3c),
    Color::rgb(0x4d, 0x6b, 0x50),
];
const PALETTE_PER_PAGE: usize = 8;

/// Advanced Style tab editor: the element's `Style` as JSON.
#[derive(Default)]
pub struct StyleJsonDialog {
    pub json: String,
    pub error: Option<String>,
}

/// Diagram tab's Edit Data… dialog. Rows stay a list while editing so empty
/// and duplicate keys can be rejected before they reach the document.
#[derive(Default)]
pub struct MetadataEdit {
    pub rows: Vec<(String, String)>,
    pub error: Option<String>,
    pub target: MetadataTarget,
}

/// Where an Edit Data dialog applies its properties.
#[derive(Default, Clone, PartialEq, Eq)]
pub enum MetadataTarget {
    #[default]
    Page,
    Nodes(Vec<NodeId>),
}

fn swatches(ui: &mut Ui, current: Color) -> Option<Color> {
    let mut out = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for c in STICKY_COLORS {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
            resp.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::Button, true, c == current, c.to_hex())
            });
            ui.painter().rect_filled(r, 5.0, c32(c));
            if c == current {
                ui.painter().rect_stroke(
                    r,
                    5.0,
                    egui::Stroke::new(2.0, theme::accent()),
                    egui::StrokeKind::Outside,
                );
            }
            if resp.clicked() {
                out = Some(c);
            }
        }
    });
    out
}

fn sticky_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    section(
        ui,
        st,
        "style.sticky",
        "Sticky note",
        &["Font size"],
        |ui| {
            let cur = first.style.fill().unwrap_or(STICKY_COLORS[0]);
            if let Some(c) = swatches(ui, cur) {
                apply(ed, "fill", ids, |n| n.style.set_fill(Some(c)));
                ed.sticky_color = c;
            }
            if let NodeKind::Sticky { font_size, .. } = first.kind {
                row(ui, "Font size", |ui| {
                    if let Some(v) = num_field(ui, ed, "", font_size, 0.5, 8.0..=200.0, 70.0) {
                        apply(ed, "font size", ids, |n| {
                            if let NodeKind::Sticky { font_size, .. } = &mut n.kind {
                                *font_size = v;
                            }
                        });
                    }
                });
            }
        },
    );
}

fn endpoint_modes(ed: &Editor, ids: &[NodeId], which: usize) -> Vec<bool> {
    ids.iter()
        .filter_map(|id| match &ed.doc.get(*id)?.kind {
            NodeKind::Connector { start, end, .. } => {
                let binding = if which == 0 { *start } else { *end };
                binding.map(|b| b.floating)
            }
            _ => None,
        })
        .collect()
}

fn endpoint_mode_row(
    ui: &mut Ui,
    ed: &mut Editor,
    ids: &[NodeId],
    label: &str,
    which: usize,
    states: &[bool],
) {
    row(ui, label, |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let enabled = !states.is_empty() && ids.iter().any(|id| !ed.doc.is_effectively_locked(*id));
        let all_fixed = !states.is_empty() && states.iter().all(|f| !f);
        let all_floating = !states.is_empty() && states.iter().all(|f| *f);
        let half = (ui.available_width() - if states.is_empty() { 0.0 } else { 48.0 }) * 0.5;
        let (fixed, floating) = ui
            .add_enabled_ui(enabled, |ui| {
                let fixed = ui.add_sized(
                    [half.max(44.0), 20.0],
                    egui::Button::selectable(all_fixed, "Fixed"),
                );
                let floating = ui.add_sized(
                    [half.max(56.0), 20.0],
                    egui::Button::selectable(all_floating, "Floating"),
                );
                (fixed, floating)
            })
            .inner;
        if !enabled {
            let hint = if states.is_empty() {
                "Bind this end to a shape first."
            } else {
                "Unlock the connector to change its endpoint."
            };
            fixed.on_disabled_hover_text(hint);
            floating.on_disabled_hover_text(hint);
        } else {
            if fixed.clicked() {
                crate::editor::whiteboard::set_connector_endpoint_mode(ed, ids, which, false);
            }
            if floating.clicked() {
                crate::editor::whiteboard::set_connector_endpoint_mode(ed, ids, which, true);
            }
            if states.iter().any(|f| *f) && !all_floating {
                mixed_badge(ui);
            }
        }
    });
}

fn arrow_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    use crate::model::{Head, Route};
    let NodeKind::Connector {
        route,
        start_head,
        end_head,
        start,
        end,
        jump,
        jump_size,
        ..
    } = &first.kind
    else {
        return;
    };
    let start_modes = endpoint_modes(ed, ids, 0);
    let end_modes = endpoint_modes(ed, ids, 1);
    section(
        ui,
        st,
        "style.arrow",
        "Arrow",
        &[
            "Add label",
            "Type",
            "Heads",
            "Head Size",
            "Size",
            "Head Offset",
            "Offset",
            "Start",
            "End",
            "Fixed",
            "Floating",
            "Line Jumps",
            "Flow",
            "Duration",
            "Timing",
            "Direction",
        ],
        |ui| {
            if ids.len() == 1
                && first.is_connector()
                && first.label.is_empty()
                && first.labels.is_empty()
                && !ed.doc.is_effectively_locked(ids[0])
            {
                let r = ui
                    .add_sized([ui.available_width(), 20.0], egui::Button::new("Add label"))
                    .on_hover_text("Add a text label to this connector");
                if r.clicked() {
                    ed.add_connector_label(ids[0]);
                }
                ui.add_space(2.0);
            }
            let routes = [
                (Route::Straight, "Straight"),
                (Route::Curved, "Curved"),
                (Route::Elbow, "Elbow"),
            ];
            row(ui, "Type", |ui| {
                if let Some(v) = combo(ui, "route", *route, &routes, 96.0) {
                    apply(ed, "route", ids, |n| {
                        if let NodeKind::Connector { route, points, .. } = &mut n.kind {
                            *route = v;
                            if v != Route::Elbow && points.len() > 2 && v == Route::Straight {
                                let (a, b) = (points[0], *points.last().unwrap());
                                *points = vec![a, b];
                            }
                        }
                    });
                    ed.arrow_route = v;
                    // Re-route through the binding logic.
                    ed.begin("Change route");
                    {
                        let mut tx = ed.tx();
                        for id in ids {
                            crate::editor::whiteboard::update_connector(&mut tx, *id);
                        }
                    }
                    ed.commit();
                }
            });
            let heads: Vec<(Head, &str)> = Head::ALL.iter().map(|h| (*h, h.label())).collect();
            row(ui, "Heads", |ui| {
                if let Some(v) = combo(ui, "h-start", *start_head, &heads, 72.0) {
                    apply(ed, "heads", ids, |n| {
                        if let NodeKind::Connector { start_head, .. } = &mut n.kind {
                            *start_head = v;
                        }
                    });
                }
                if let Some(v) = combo(ui, "h-end", *end_head, &heads, 72.0) {
                    apply(ed, "heads", ids, |n| {
                        if let NodeKind::Connector { end_head, .. } = &mut n.kind {
                            *end_head = v;
                        }
                    });
                }
            });
            row(ui, "Head Size", |ui| {
                if let Some(v) =
                    num_field(ui, ed, "S ", first.start_head_size, 0.5, 1.0..=60.0, 64.0)
                {
                    apply(ed, "start head size", ids, |n| n.start_head_size = v);
                }
                if let Some(v) = num_field(ui, ed, "E ", first.end_head_size, 0.5, 1.0..=60.0, 64.0)
                {
                    apply(ed, "end head size", ids, |n| n.end_head_size = v);
                }
            });
            row(ui, "Head Offset", |ui| {
                if let Some(v) = num_field(
                    ui,
                    ed,
                    "S ",
                    first.start_head_offset,
                    0.5,
                    -200.0..=200.0,
                    64.0,
                ) {
                    apply(ed, "start head offset", ids, |n| n.start_head_offset = v);
                }
                if let Some(v) = num_field(
                    ui,
                    ed,
                    "E ",
                    first.end_head_offset,
                    0.5,
                    -200.0..=200.0,
                    64.0,
                ) {
                    apply(ed, "end head offset", ids, |n| n.end_head_offset = v);
                }
            });
            endpoint_mode_row(ui, ed, ids, "Start", 0, &start_modes);
            endpoint_mode_row(ui, ed, ids, "End", 1, &end_modes);
            // Where this line hops over a crossing connector drawn below it, as in
            // draw.io's Style tab ("Line Jumps" select and size input).
            let jumps: Vec<(LineJump, &str)> =
                LineJump::ALL.iter().map(|j| (*j, j.label())).collect();
            row(ui, "Line Jumps", |ui| {
                if let Some(v) = combo(ui, "line-jump", *jump, &jumps, 72.0) {
                    apply(ed, "line jumps", ids, |n| {
                        if let NodeKind::Connector { jump, .. } = &mut n.kind {
                            *jump = v;
                        }
                    });
                }
                if let Some(v) = num_field(ui, ed, "", *jump_size, 0.5, 0.0..=999.0, 58.0) {
                    apply(ed, "jump size", ids, |n| {
                        if let NodeKind::Connector { jump_size, .. } = &mut n.kind {
                            *jump_size = v;
                        }
                    });
                }
            });
            // draw.io's Style tab: Flow Animation plus its timing, duration and direction.
            let flows: Vec<crate::model::FlowAnimation> = ids
                .iter()
                .filter_map(|id| ed.doc.get(*id).map(|n| n.flow))
                .collect();
            let enabled: Vec<bool> = flows.iter().map(|f| f.enabled).collect();
            let duration = flows.first().map_or(500.0, |f| f.duration_ms);
            let timing = flows.first().map_or(FlowTiming::Linear, |f| f.timing);
            let direction = flows.first().map_or(FlowDirection::Normal, |f| f.direction);
            row(ui, "Flow", |ui| {
                if let Some(v) = mixed_checkbox(ui, "Animation", &enabled) {
                    apply(ed, "flow animation", ids, |n| n.flow.enabled = v);
                }
            });
            row(ui, "Duration", |ui| {
                if let Some(v) = num_field(ui, ed, "", duration, 10.0, 1.0..=60000.0, 70.0) {
                    apply(ed, "flow duration", ids, |n| n.flow.duration_ms = v);
                }
            });
            let timings: Vec<(FlowTiming, &str)> =
                FlowTiming::ALL.iter().map(|t| (*t, t.label())).collect();
            row(ui, "Timing", |ui| {
                if let Some(v) = combo(ui, "flow-timing", timing, &timings, 92.0) {
                    apply(ed, "flow timing", ids, |n| n.flow.timing = v);
                }
            });
            let directions: Vec<(FlowDirection, &str)> =
                FlowDirection::ALL.iter().map(|d| (*d, d.label())).collect();
            row(ui, "Direction", |ui| {
                if let Some(v) = combo(ui, "flow-direction", direction, &directions, 110.0) {
                    apply(ed, "flow direction", ids, |n| n.flow.direction = v);
                }
            });
            let bound = |b: &Option<crate::model::Binding>| {
                b.and_then(|b| ed.doc.get(b.node).map(|n| n.name.clone()))
                    .unwrap_or_else(|| "—".into())
            };
            ui.label(
                RichText::new(format!("Connects {} → {}", bound(start), bound(end)))
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
            if let Some(binding) = start
                && let Some(source) = ed.doc.get(binding.node)
            {
                let set = source.connector_default.is_some();
                let locked = ed.doc.is_effectively_locked(binding.node);
                let label = if set {
                    "Update source connector default"
                } else {
                    "Set as source connector default"
                };
                let btn = ui
                    .add_enabled(!locked, egui::Button::new(label))
                    .on_hover_text(if set {
                        "Copy this connector's style onto its source shape for new connectors"
                    } else {
                        "New connectors started from this shape will copy this connector's style"
                    });
                if locked {
                    btn.on_disabled_hover_text(
                        "Unlock the source shape to change its connector default.",
                    );
                } else if btn.clicked() {
                    ed.set_connector_default_from_selection();
                }
            }
        },
    );
}

fn sketch_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    use crate::model::rough::{FillStyle, Rough};
    section(
        ui,
        st,
        "style.hand-drawn",
        "Hand-drawn",
        &["Sketch", "Sloppiness", "Fill style"],
        |ui| {
            row(ui, "Sketch", |ui| {
                let mut on = first.style.rough.is_some();
                if ui
                    .checkbox(&mut on, "")
                    .on_hover_text("Sketchy rough.js-style lines (⌘⌥H)")
                    .changed()
                {
                    ed.toggle_sketch();
                }
            });
            let Some(r) = first.style.rough else { return };
            let levels = [(0u8, "Architect"), (1, "Artist"), (2, "Cartoonist")];
            let cur = r.roughness.round().clamp(0.0, 2.0) as u8;
            row(ui, "Sloppiness", |ui| {
                if let Some(v) = combo(ui, "rough", cur, &levels, 96.0) {
                    apply(ed, "rough", ids, |n| {
                        if let Some(r) = &mut n.style.rough {
                            r.roughness = v as f64;
                        }
                    });
                }
            });
            let fills: Vec<(FillStyle, &str)> =
                FillStyle::ALL.iter().map(|f| (*f, f.label())).collect();
            row(ui, "Fill style", |ui| {
                if let Some(v) = combo(ui, "fillstyle", r.fill_style, &fills, 96.0) {
                    apply(ed, "rough", ids, |n| {
                        if let Some(r) = &mut n.style.rough {
                            r.fill_style = v;
                        }
                    });
                }
            });
            if ui
                .small_button("New wobble")
                .on_hover_text("Re-roll the random seed")
                .clicked()
            {
                apply(ed, "rough", ids, |n| {
                    if let Some(r) = &mut n.style.rough {
                        *r = Rough {
                            seed: r.seed.wrapping_mul(1103515245).wrapping_add(12345) | 1,
                            ..*r
                        };
                    }
                });
            }
        },
    );
}

/// Export settings and button, like Figma's Export section.
fn export_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    use crate::io::export::{Format, Scope};
    let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.export",
        "Export",
        &["Export", "Data", "Transparent background"],
    ) else {
        return;
    };
    let spec = &mut st.export;
    ui.horizontal(|ui| {
        let fmts: Vec<(Format, &str)> = Format::ALL
            .iter()
            .map(|f| {
                (
                    *f,
                    match f {
                        Format::Png => "PNG",
                        Format::Svg => "SVG",
                        Format::Pdf => "PDF",
                        Format::Drawio => "draw.io",
                    },
                )
            })
            .collect();
        if let Some(v) = combo(ui, "exp-format", spec.format, &fmts, 64.0) {
            spec.format = v;
            if v == Format::Drawio {
                spec.scope = Scope::AllPages;
            }
        }
        if spec.format == Format::Png {
            let scales = [(1u8, "1x"), (2, "2x"), (3, "3x"), (4, "4x")];
            if let Some(v) = combo(ui, "exp-scale", spec.scale as u8, &scales, 48.0) {
                spec.scale = v as f64;
            }
        }
    });
    if spec.format == Format::Drawio {
        row(ui, "Export", |ui| {
            ui.label("All pages in one .drawio file");
        });
    } else {
        let scopes: Vec<(Scope, &str)> = Scope::ALL.iter().map(|s| (*s, s.label())).collect();
        row(ui, "Export", |ui| {
            if let Some(v) = combo(ui, "exp-scope", spec.scope, &scopes, 110.0) {
                spec.scope = v;
            }
        });
        if spec.format != Format::Svg || spec.transparent {
            ui.checkbox(&mut spec.transparent, "Transparent background");
        }
    }
    if spec.format.can_embed() {
        row(ui, "Data", |ui| {
            ui.checkbox(&mut spec.embed_data, "Embed pages for reopening");
        });
    }
    let what = match (spec.scope, ed.top_selection().len()) {
        (Scope::Selection, 0) => format!("page “{}”", ed.page_name()),
        (Scope::Selection, 1) => ed
            .doc
            .get(ed.top_selection()[0])
            .map_or("selection".into(), |n| format!("“{}”", n.name)),
        (Scope::Selection, n) => format!("{n} items"),
        (Scope::Frames, _) => "each frame".into(),
        (Scope::Page, _) => format!("page “{}”", ed.page_name()),
        (Scope::AllPages, _) => format!("{} pages", ed.pages.len()),
    };
    if ui
        .add(
            egui::Button::new(format!("Export {what}…"))
                .min_size(egui::vec2(ui.available_width(), 24.0)),
        )
        .clicked()
    {
        ed.requests.push(AppRequest::Export(*spec));
    }
    ui.horizontal(|ui| {
        if ui.small_button("Copy as PNG").clicked() {
            ed.run(crate::editor::shortcuts::Action::CopyAsPng);
        }
        if ui.small_button("Copy as SVG").clicked() {
            ed.run(crate::editor::shortcuts::Action::CopyAsSvg);
        }
    });
}

/// draw.io's Diagram tab: page/view settings, guide and autosave options,
/// paper size, diagram metadata and the shape defaults.
fn diagram_tab(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, vis: f32) {
    use crate::model::settings::{Orientation, PageTransition, PaperSize};

    if let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.view",
        "View",
        &[
            "Grid",
            "Page View",
            "Rulers",
            "Background",
            "Background Color",
            "Dark background",
            "Adaptive Colors",
            "Transition",
            "Fade on page switch",
            "Shadow",
            "Sketch",
        ],
    ) {
        ui.horizontal(|ui| {
            let mut on = ed.doc.settings.grid;
            if ui.checkbox(&mut on, "Grid").changed() {
                ed.toggle_grid();
            }
            ui.add_space(4.0);
            let mut size = ed.doc.settings.grid_size;
            let r = ui.add_sized(
                [58.0, 20.0],
                egui::DragValue::new(&mut size)
                    .range(1.0..=1000.0)
                    .speed(0.5)
                    .max_decimals(1)
                    .suffix(" pt"),
            );
            if r.changed() {
                ed.edit_settings_coalescing("Change grid spacing", "grid-size", |s| {
                    s.grid_size = size;
                });
                st.options.grid_size = size;
                st.options_dirty = true;
            }
            seal_on_release(ed, &r);
            let mut gc = c32(ed
                .doc
                .settings
                .grid_color
                .unwrap_or_else(|| Color::rgb(0xd0, 0xd0, 0xd0)));
            if ui
                .color_edit_button_srgba(&mut gc)
                .on_hover_text("Grid color (kept in draw.io export)")
                .changed()
            {
                let c = from32(gc);
                ed.edit_settings("Change grid color", |s| s.grid_color = Some(c));
            }
            if ed.doc.settings.grid_color.is_some()
                && ui
                    .small_button("×")
                    .on_hover_text("Use the theme grid color")
                    .clicked()
            {
                ed.edit_settings("Reset grid color", |s| s.grid_color = None);
            }
        });
        row(ui, "Transition", |ui| {
            let mut fade = ed.doc.settings.page_transition == PageTransition::Fade;
            if ui
                .checkbox(&mut fade, "Fade on page switch")
                .on_hover_text("Fade in the new page when the editor switches pages")
                .changed()
            {
                ed.edit_settings("Change page transition", |s| {
                    s.page_transition = if fade {
                        PageTransition::Fade
                    } else {
                        PageTransition::None
                    };
                });
            }
            if fade {
                let mut ms = ed.doc.settings.page_transition_ms as f64;
                let r = ui.add_sized(
                    [58.0, 20.0],
                    egui::DragValue::new(&mut ms)
                        .range(100.0..=3000.0)
                        .speed(10.0)
                        .suffix(" ms"),
                );
                if r.changed() {
                    ed.edit_settings_coalescing(
                        "Change page transition",
                        "page-transition-ms",
                        |s| {
                            s.page_transition_ms = ms.round() as u64;
                        },
                    );
                }
                seal_on_release(ed, &r);
            }
        });
        let mut page_view = ed.doc.settings.page_view;
        if ui
            .checkbox(&mut page_view, "Page View")
            .on_hover_text("Paper preview; this view option is not saved with the document")
            .changed()
        {
            ed.edit_settings("Toggle page view", |s| s.page_view = page_view);
        }
        let mut rulers = ed.doc.settings.rulers;
        if ui
            .checkbox(&mut rulers, "Rulers")
            .on_hover_text("Ruler strips along the top and left edges; drag one out to add a guide")
            .changed()
        {
            ed.edit_settings("Toggle rulers", |s| s.rulers = rulers);
        }
        row(ui, "Background", |ui| {
            if ui
                .add_sized(
                    [(vis - LABEL_W - 8.0).max(80.0), 20.0],
                    egui::Button::new("Change…"),
                )
                .on_hover_text("Choose the page background")
                .clicked()
            {
                st.background_draft = Some(ed.doc.settings.background_color);
                if let Some(bg) = &ed.doc.settings.background_image {
                    st.background_image_mode = bg.mode;
                    st.background_image_opacity = bg.opacity;
                } else {
                    st.background_image_mode = crate::model::ImageMode::Stretch;
                    st.background_image_opacity = 1.0;
                }
                st.background_open = true;
            }
        });
        ui.horizontal(|ui| {
            let mut on = ed.doc.settings.background_enabled;
            if ui.checkbox(&mut on, "Background Color").changed() {
                // The chosen color stays stored while the background is off.
                ed.edit_settings("Toggle page background", |s| s.background_enabled = on);
            }
            let mut c = c32(ed.doc.settings.background_color);
            if ui.color_edit_button_srgba(&mut c).changed() {
                let color = from32(c);
                ed.edit_settings("Change page background", |s| {
                    s.background_color = color;
                    s.background_enabled = true;
                });
            }
        });
        row(ui, "Dark background", |ui| {
            let mut c = c32(ed
                .doc
                .settings
                .background_color_dark
                .unwrap_or(ed.doc.settings.background_color));
            if ui.color_edit_button_srgba(&mut c).changed() {
                let color = from32(c);
                ed.edit_settings("Change dark page background", |s| {
                    s.background_color_dark = Some(color);
                    s.background_enabled = true;
                });
            }
        });
        row(ui, "Adaptive Colors", |ui| {
            let modes: Vec<(AdaptiveColors, &str)> = AdaptiveColors::ALL
                .iter()
                .map(|m| (*m, m.label()))
                .collect();
            if let Some(mode) = combo(
                ui,
                "adaptive-colors",
                ed.doc.settings.adaptive_colors,
                &modes,
                96.0,
            ) {
                ed.edit_settings("Change adaptive colors", |s| {
                    s.adaptive_colors = mode;
                });
            }
        });
        let w = ui.available_width();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let half = (w - 4.0) * 0.5;
            let mut shadow = ed.doc.settings.shadow;
            let r = ui.add_sized([half, 20.0], egui::Checkbox::new(&mut shadow, "Shadow"));
            if r.changed() {
                ed.edit_settings("Toggle diagram shadow", |s| s.shadow = shadow);
            }
            let mut sketch = ed.doc.settings.sketch;
            let r = ui.add_sized([half, 20.0], egui::Checkbox::new(&mut sketch, "Sketch"));
            if r.changed() {
                ed.set_global_sketch(sketch);
            }
        });
    }
    separator(ui);

    if let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.options",
        "Options",
        &[
            "Connection Arrows",
            "Connection Points",
            "Guides",
            "Clear Guides",
            "Autosave",
        ],
    ) {
        let mut arrows = ed.doc.settings.connection_arrows;
        if ui
            .checkbox(&mut arrows, "Connection Arrows")
            .on_hover_text("Hover arrows that start a connection")
            .changed()
        {
            ed.edit_settings("Toggle connection arrows", |s| s.connection_arrows = arrows);
        }
        let mut points = ed.doc.settings.connection_points;
        if ui
            .checkbox(&mut points, "Connection Points")
            .on_hover_text("Anchors shown while connecting")
            .changed()
        {
            ed.edit_settings("Toggle connection points", |s| s.connection_points = points);
        }
        let mut guides = ed.doc.settings.guides;
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut guides, "Guides")
                .on_hover_text("Alignment guides and snapping")
                .changed()
            {
                ed.edit_settings("Toggle guides", |s| s.guides = guides);
            }
            let count = ed.doc.settings.manual_guides.len();
            let clear = ui
                .add_enabled(count > 0, egui::Button::new("Clear Guides"))
                .on_hover_text("Delete every manual guide on this page");
            if clear.clicked() {
                ed.edit_settings("Clear guides", |s| s.manual_guides.clear());
            }
        });
        let mut autosave = ed.doc.settings.autosave;
        let r = ui
            .add_enabled_ui(st.autosave_available, |ui| {
                ui.checkbox(&mut autosave, "Autosave")
                    .on_hover_text("Save changes automatically")
            })
            .inner;
        if !st.autosave_available {
            r.on_disabled_hover_text(
                "Autosave needs a project folder; this diagram has no file yet.",
            );
        } else if r.changed() {
            ed.edit_settings("Toggle autosave", |s| s.autosave = autosave);
        }
    }
    separator(ui);

    if let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.paper",
        "Paper Size",
        &["Paper Size", "Portrait", "Landscape"],
    ) {
        let papers: Vec<(PaperSize, &str)> =
            PaperSize::ALL.iter().map(|p| (*p, p.label())).collect();
        combo(ui, "paper-size", ed.doc.settings.paper, &papers, vis);
        let mut orient = ed.doc.settings.orientation;
        ui.horizontal(|ui| {
            ui.radio_value(&mut orient, Orientation::Portrait, "Portrait");
            ui.radio_value(&mut orient, Orientation::Landscape, "Landscape");
        });
        if orient != ed.doc.settings.orientation {
            ed.edit_settings("Change orientation", |s| s.orientation = orient);
        }
    }
    separator(ui);

    if let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.data",
        "Diagram",
        &["Edit Data…", "Clear Default Style"],
    ) {
        if ui
            .add_sized([vis, 20.0], egui::Button::new("Edit Data…"))
            .on_hover_text("Diagram properties, saved with the page")
            .clicked()
        {
            st.metadata_edit = Some(MetadataEdit {
                rows: ed
                    .doc
                    .settings
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                error: None,
                target: MetadataTarget::Page,
            });
        }
        let has_defaults =
            ed.doc.settings.shape_default.is_some() || ed.doc.settings.connector_default.is_some();
        let r = ui
            .add_enabled_ui(has_defaults, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Clear Default Style"))
                    .on_hover_text("Reset the remembered styles for new shapes and connectors")
            })
            .inner;
        if has_defaults {
            if r.clicked() {
                ed.clear_default_styles();
            }
        } else {
            r.on_disabled_hover_text("No custom default style is set.");
        }
        ui.label(
            RichText::new(if has_defaults {
                "New shapes use the remembered default style."
            } else {
                "New shapes use the built-in defaults."
            })
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
        );
    }
    separator(ui);

    if let Some(_guard) = section_begin(ui, st, "diagram.page", "Page", &["Name"]) {
        let mut name = ed.page_name().to_string();
        if ui
            .add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY))
            .changed()
        {
            let i = ed.active_page;
            ed.rename_page(i, &name);
        }
        ui.label(
            RichText::new(format!(
                "{} shapes · {} × {} pt",
                ed.doc.nodes.len(),
                ed.doc.settings.paper_size().0.round(),
                ed.doc.settings.paper_size().1.round()
            ))
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
        );
    }
    separator(ui);

    if let Some(_guard) = section_begin(
        ui,
        st,
        "diagram.new-shapes",
        "New shapes",
        &["Arrows", "Sticky", "Pencil"],
    ) {
        let routes = [
            (crate::model::Route::Straight, "Straight"),
            (crate::model::Route::Curved, "Curved"),
            (crate::model::Route::Elbow, "Elbow"),
        ];
        row(ui, "Arrows", |ui| {
            if let Some(v) = combo(ui, "new-route", ed.arrow_route, &routes, 96.0) {
                ed.arrow_route = v;
            }
        });
        row(ui, "Sticky", |ui| {
            if let Some(c) = swatches(ui, ed.sticky_color) {
                ed.sticky_color = c;
            }
        });
        row(ui, "Pencil", |ui| {
            // Two sliders plus a color swatch must fit the row: anything wider
            // than the panel makes egui store the overflow as the panel's size.
            let w = ((vis - LABEL_W - 46.0) * 0.5).max(40.0);
            ui.add_sized(
                [w, 20.0],
                egui::Slider::new(&mut ed.ink_width, 0.5..=24.0).show_value(false),
            )
            .on_hover_text("Ink width");
            ui.add_sized(
                [w, 20.0],
                egui::Slider::new(&mut ed.ink_smoothing, 0.0..=1.0).show_value(false),
            )
            .on_hover_text("Ink smoothing");
            let mut c = c32(ed.ink_color);
            if ui.color_edit_button_srgba(&mut c).changed() {
                ed.ink_color = from32(c);
            }
        });
    }
    separator(ui);
    export_section(ui, ed, st);
}

/// The global Style tab: diagram-wide Sketch/Rounded and the scheme cards.
fn global_style_tab(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, vis: f32) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.global",
        "Global style",
        &["Sketch", "Rounded", "Scope", "Scheme"],
    ) else {
        return;
    };
    ui.horizontal(|ui| {
        let mut sketch = ed.doc.settings.sketch;
        if ui.checkbox(&mut sketch, "Sketch").changed() {
            ed.set_global_sketch(sketch);
        }
        ui.add_space(6.0);
        let rounded = ed.doc.settings.rounded;
        let w = (vis - 110.0).max(80.0);
        if ui
            .add_sized([w, 20.0], egui::Button::selectable(rounded, "Rounded"))
            .on_hover_text("Round or square every compatible shape")
            .clicked()
        {
            ed.toggle_global_rounded();
        }
    });
    scheme_controls(ui, ed, st, vis);
}

/// Scheme scope, preview count, and the scheme cards. Shared by the global
/// Style tab and the selected-element Style tab so selection scope is reachable
/// in the running app.
fn scheme_controls(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, vis: f32) {
    let preview = ed.scheme_preview(st.scheme_scope);
    ui.horizontal(|ui| {
        ui.label("Scope");
        if ui
            .add(egui::Button::selectable(
                st.scheme_scope == SchemeScope::Page,
                "Page",
            ))
            .clicked()
        {
            st.scheme_scope = SchemeScope::Page;
        }
        let has_selection = !ed.selection.is_empty();
        if ui
            .add_enabled(
                has_selection,
                egui::Button::selectable(st.scheme_scope == SchemeScope::Selection, "Selection"),
            )
            .on_hover_text(if has_selection {
                "Restyle only the selected subtrees"
            } else {
                "Select elements to use selection scope"
            })
            .clicked()
        {
            st.scheme_scope = SchemeScope::Selection;
        }
        ui.label(
            RichText::new(format!(
                "{} changed, {} skipped",
                preview.changed,
                preview.skipped()
            ))
            .size(theme::SECONDARY)
            .color(theme::pal(ui).secondary),
        );
    });
    ui.add_space(2.0);
    const PER_PAGE: usize = 10;
    let pages = SCHEMES.len().div_ceil(PER_PAGE).max(1);
    st.scheme_page = st.scheme_page.min(pages - 1);
    let start = st.scheme_page * PER_PAGE;
    let end = (start + PER_PAGE).min(SCHEMES.len());
    let card_w = (vis - 6.0) * 0.5;
    // An egui Grid sizes its columns from the widgets' reported minimum width,
    // which is zero for the custom-painted cards; the cards then overlap. Two
    // explicit horizontal rows keep each card at `card_w` and side by side.
    for pair in SCHEMES[start..end].chunks(2) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for scheme in pair {
                let selected = ed.doc.settings.scheme.as_deref() == Some(scheme.name);
                if scheme_card(ui, scheme, selected, card_w) {
                    ed.apply_scheme_scoped(scheme, st.scheme_scope);
                }
            }
        });
    }
    if pages > 1 {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(st.scheme_page > 0, egui::Button::new("‹"))
                .on_hover_text("Previous schemes")
                .clicked()
            {
                st.scheme_page -= 1;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(st.scheme_page + 1 < pages, egui::Button::new("›"))
                    .on_hover_text("More schemes")
                    .clicked()
                {
                    st.scheme_page += 1;
                }
                ui.label(
                    RichText::new(format!("Page {} of {pages}", st.scheme_page + 1))
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            });
        });
    }
}

/// One scheme preview: a labeled shape above a connector, in scheme colors.
fn scheme_card(ui: &mut Ui, scheme: &crate::model::Scheme, selected: bool, width: f32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 82.0), egui::Sense::click());
    resp.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, scheme.name)
    });
    let p = ui.painter();
    let pal = theme::pal(ui);
    p.rect(
        rect,
        6.0,
        c32(scheme.page),
        egui::Stroke::new(1.0, pal.separator),
        egui::StrokeKind::Inside,
    );
    if selected {
        p.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(2.0, theme::accent()),
            egui::StrokeKind::Inside,
        );
    }
    let shape = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 16.0, rect.top() + 8.0),
        egui::vec2((rect.width() - 32.0).max(20.0), 24.0),
    );
    p.rect(
        shape,
        2.0,
        c32(scheme.fill),
        egui::Stroke::new(1.5, c32(scheme.line)),
        egui::StrokeKind::Inside,
    );
    p.text(
        shape.center(),
        egui::Align2::CENTER_CENTER,
        "Shape",
        egui::FontId::proportional(11.0),
        c32(scheme.font),
    );
    // Connector: down from the shape, across, then up into a small arrow head.
    let a = egui::pos2(shape.center().x, shape.bottom());
    let elbow = egui::pos2(a.x, shape.bottom() + 16.0);
    let end = egui::pos2(shape.center().x + 14.0, shape.bottom() + 6.0);
    let stroke = egui::Stroke::new(1.5, c32(scheme.line));
    p.line_segment([a, elbow], stroke);
    p.line_segment([elbow, egui::pos2(end.x, elbow.y)], stroke);
    p.line_segment([egui::pos2(end.x, elbow.y), end], stroke);
    p.line_segment([end, end + egui::vec2(-4.0, 5.0)], stroke);
    p.line_segment([end, end + egui::vec2(4.0, 5.0)], stroke);
    p.text(
        egui::pos2(rect.center().x, rect.bottom() - 10.0),
        egui::Align2::CENTER_CENTER,
        "Connector",
        egui::FontId::proportional(10.0),
        c32(scheme.line),
    );
    resp.on_hover_text(scheme.name).clicked()
}

/// The selected-element Style tab.
fn style_tab(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
) {
    style_palette(ui, ed, st, ids);
    separator(ui);
    if ids
        .iter()
        .any(|id| ed.doc.get(*id).is_some_and(fill_applies))
    {
        fill_section(ui, ed, st, ids, first);
        separator(ui);
    }
    if ids
        .iter()
        .any(|id| ed.doc.get(*id).is_some_and(|n| !n.is_group()))
    {
        line_section(ui, ed, st, ids, first);
        separator(ui);
    }
    opacity_section(ui, ed, st, ids, first);
    separator(ui);
    effects_section(ui, ed, st, ids, first);
    if let Some(conn) = ids
        .iter()
        .find_map(|id| ed.doc.get(*id).filter(|n| n.is_connector()).cloned())
    {
        separator(ui);
        arrow_section(ui, ed, st, ids, &conn);
    }
    if let Some(sticky) = ids.iter().find_map(|id| {
        ed.doc
            .get(*id)
            .filter(|n| matches!(n.kind, NodeKind::Sticky { .. }))
            .cloned()
    }) {
        separator(ui);
        sticky_section(ui, ed, st, ids, &sticky);
    }
    if let Some(sketchy) = ids.iter().find_map(|id| {
        ed.doc
            .get(*id)
            .filter(|n| {
                n.style.sketch.is_some_and(|s| s)
                    || (n.style.sketch.is_none() && n.style.rough.is_some())
            })
            .cloned()
    }) {
        separator(ui);
        sketch_section(ui, ed, st, ids, &sketchy);
    }
    separator(ui);
    style_actions(ui, ed, st, vis, ids, first);
    separator(ui);
    properties_section(ui, ed, st, ids, first);
    separator(ui);
    if let Some(_guard) = section_begin(
        ui,
        st,
        "style.global",
        "Global style",
        &["Scheme", "Scope", "Rounded", "Sketch"],
    ) {
        scheme_controls(ui, ed, st, vis);
    }
}

fn fill_applies(n: &Node) -> bool {
    !matches!(
        n.kind,
        NodeKind::Text { .. } | NodeKind::Stroke { .. } | NodeKind::Group
    ) && crate::render::draw::is_area(n)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FillMode {
    Auto,
    Solid,
    Linear,
    Radial,
    Image,
}

impl FillMode {
    fn of(n: &Node) -> FillMode {
        if n.style.fill_auto {
            FillMode::Auto
        } else {
            match n.style.fills.last() {
                Some(Paint::Linear { .. }) => FillMode::Linear,
                Some(Paint::Radial { .. }) => FillMode::Radial,
                Some(Paint::Image { .. }) => FillMode::Image,
                _ => FillMode::Solid,
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            FillMode::Auto => "Auto",
            FillMode::Solid => "Solid",
            FillMode::Linear => "Linear",
            FillMode::Radial => "Radial",
            FillMode::Image => "Image",
        }
    }

    const ALL: [FillMode; 5] = [
        FillMode::Auto,
        FillMode::Solid,
        FillMode::Linear,
        FillMode::Radial,
        FillMode::Image,
    ];
}

fn fill_color_of(n: &Node, ed: &Editor) -> Color {
    if n.style.fill_auto {
        ed.doc
            .settings
            .auto_fill()
            .or_else(|| n.style.fills.last().map(|p| p.preview_color()))
            .unwrap_or_else(|| Color::rgb(0xd9, 0xd9, 0xd9))
    } else {
        n.style
            .fills
            .last()
            .map_or_else(|| Color::rgb(0xd9, 0xd9, 0xd9), |p| p.preview_color())
    }
}

fn hex_edit(ui: &mut Ui, salt: &str, ids: &[NodeId], c: Color, mixed: bool) -> Option<Color> {
    let id = ui.make_persistent_id(("hex", salt, ids.first().copied()));
    let stored = if mixed {
        String::new()
    } else {
        c.to_hex().to_string()
    };
    if !ui.ctx().memory(|m| m.has_focus(id)) {
        ui.ctx().data_mut(|d| d.insert_temp(id, stored.clone()));
    }
    let mut draft = ui
        .ctx()
        .data(|d| d.get_temp::<String>(id))
        .unwrap_or(stored);
    let edit = egui::TextEdit::singleline(&mut draft)
        .desired_width(62.0)
        .char_limit(9)
        .font(egui::TextStyle::Monospace);
    let r = ui.add(edit);
    if r.changed() {
        ui.ctx().data_mut(|d| d.insert_temp(id, draft.clone()));
    }
    let mut out = None;
    if r.lost_focus() {
        if let Some(mut nc) = Color::from_hex(draft.trim()) {
            if draft.trim().len() == 7 && c.a != 255 {
                nc.a = c.a;
            }
            if nc != c {
                out = Some(nc);
            }
        }
        let reset = if mixed {
            String::new()
        } else {
            c.to_hex().to_string()
        };
        ui.ctx().data_mut(|d| d.insert_temp(id, reset));
    }
    out
}

/// Quick-style palette with the reference's arrows, two rows of swatches and
/// page dots. A click recolors every compatible selected element.
fn style_palette(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId]) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.palette",
        "Palette",
        &["Swatches", "Previous swatches", "More swatches"],
    ) else {
        return;
    };
    let pages = PALETTE_COLORS.len().div_ceil(PALETTE_PER_PAGE).max(1);
    st.palette_page = st.palette_page.min(pages - 1);
    let start = st.palette_page * PALETTE_PER_PAGE;
    let end = (start + PALETTE_PER_PAGE).min(PALETTE_COLORS.len());
    let current = ids
        .first()
        .and_then(|id| ed.doc.get(*id))
        .map(|n| fill_color_of(n, ed));
    let mut clicked = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(st.palette_page > 0, egui::Button::new("‹").frame(false))
            .on_hover_text("Previous swatches")
            .clicked()
        {
            st.palette_page -= 1;
        }
        egui::Grid::new("style-palette")
            .spacing([4.0, 4.0])
            .show(ui, |ui| {
                for (i, c) in PALETTE_COLORS[start..end].iter().enumerate() {
                    if i % 4 == 0 && i > 0 {
                        ui.end_row();
                    }
                    let (r, resp) =
                        ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
                    resp.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::WidgetType::Button,
                            true,
                            current == Some(*c),
                            c.to_hex(),
                        )
                    });
                    ui.painter().rect(
                        r,
                        3.0,
                        c32(*c),
                        egui::Stroke::new(1.0, theme::pal(ui).separator),
                        egui::StrokeKind::Inside,
                    );
                    if current == Some(*c) {
                        ui.painter().rect_stroke(
                            r,
                            3.0,
                            egui::Stroke::new(2.0, theme::accent()),
                            egui::StrokeKind::Outside,
                        );
                    }
                    if resp.clicked() {
                        clicked = Some(*c);
                    }
                }
            });
        if ui
            .add_enabled(
                st.palette_page + 1 < pages,
                egui::Button::new("›").frame(false),
            )
            .on_hover_text("More swatches")
            .clicked()
        {
            st.palette_page += 1;
        }
    });
    ui.horizontal(|ui| {
        ui.add_space(24.0);
        for p in 0..pages {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::click());
            let on = p == st.palette_page;
            ui.painter().circle_filled(
                r.center(),
                if on { 3.5 } else { 2.5 },
                if on {
                    theme::accent()
                } else {
                    theme::pal(ui).secondary
                },
            );
            if resp.clicked() {
                st.palette_page = p;
            }
        }
    });
    if let Some(c) = clicked {
        apply(ed, "palette", ids, |n| match n.kind {
            NodeKind::Connector { .. } | NodeKind::Line { .. } | NodeKind::Stroke { .. } => {
                n.style.stroke = Some(c);
                n.style.stroke_enabled = true;
            }
            NodeKind::Text { .. } => n.style.fills = vec![Paint::solid(c)],
            NodeKind::Group => {}
            _ => {
                n.style.fills = vec![Paint::solid(c)];
                n.style.fill_auto = false;
                n.style.fill_enabled = true;
            }
        });
    }
}

fn set_fill_color(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "fill", ids, |n| {
        if !fill_applies(n) {
            return;
        }
        n.style.fill_auto = false;
        n.style.fill_enabled = true;
        match n.style.fills.last_mut() {
            Some(Paint::Solid { color, .. }) => *color = c,
            _ => n.style.fills = vec![Paint::solid(c)],
        }
    });
}

fn set_fill_color_dark(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "dark fill", ids, |n| {
        if !fill_applies(n) {
            return;
        }
        n.style.fill_auto = false;
        n.style.fill_enabled = true;
        let light = n
            .style
            .fills
            .last()
            .map_or(Color::WHITE, |p| p.preview_color());
        match n.style.fills.last_mut() {
            Some(Paint::Solid { dark, .. }) => *dark = Some(c),
            _ => n.style.fills = vec![Paint::solid_pair(light, c)],
        }
    });
}

fn fill_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.fill",
        "Fill",
        &["Fill", "Fill mode", "Gradient", "Stops"],
    ) else {
        return;
    };
    let caps: Vec<Node> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .filter(|n| fill_applies(n))
        .cloned()
        .collect();
    let enabled: Vec<bool> = caps.iter().map(|n| n.style.fill_enabled).collect();
    let colors: Vec<Color> = caps.iter().map(|n| fill_color_of(n, ed)).collect();
    let color_mixed = !all_equal(&colors);
    ui.horizontal(|ui| {
        if let Some(on) = mixed_checkbox(ui, "", &enabled) {
            apply(ed, "fill", ids, |n| {
                if fill_applies(n) {
                    n.style.fill_enabled = on;
                }
            });
        }
        let modes: Vec<(FillMode, &str)> = FillMode::ALL.iter().map(|m| (*m, m.label())).collect();
        let mode = FillMode::of(first);
        if let Some(m) = combo(ui, "fill-mode", mode, &modes, 70.0) {
            if m == FillMode::Image {
                ed.requests.push(AppRequest::ImageFill(
                    ids.to_vec(),
                    first.style.fills.len().saturating_sub(1),
                ));
            } else {
                apply(ed, "fill mode", ids, |n| {
                    if !fill_applies(n) {
                        return;
                    }
                    n.style.fill_enabled = true;
                    match m {
                        FillMode::Auto => n.style.fill_auto = true,
                        FillMode::Solid => {
                            n.style.fill_auto = false;
                            let c = n.style.fills.last().map_or_else(
                                || Color::rgb(0xd9, 0xd9, 0xd9),
                                |p| p.preview_color(),
                            );
                            if !matches!(n.style.fills.last(), Some(Paint::Solid { .. })) {
                                n.style.fills = vec![Paint::solid(c)];
                            }
                        }
                        FillMode::Linear | FillMode::Radial => {
                            n.style.fill_auto = false;
                            let base = n
                                .style
                                .fills
                                .last()
                                .map_or(Color::WHITE, |p| p.preview_color());
                            let fallback = if m == FillMode::Linear {
                                Paint::linear(base, Color::rgba(base.r, base.g, base.b, 0))
                            } else {
                                Paint::radial(base, Color::rgba(base.r, base.g, base.b, 0))
                            };
                            let gradient = match (&n.style.saved_gradient, m) {
                                (Some(p @ Paint::Linear { .. }), FillMode::Linear)
                                | (Some(p @ Paint::Radial { .. }), FillMode::Radial) => p.clone(),
                                _ => fallback,
                            };
                            n.style.fills = vec![gradient];
                        }
                        FillMode::Image => {}
                    }
                });
            }
        }
        let c = colors
            .first()
            .copied()
            .unwrap_or_else(|| Color::rgb(0xd9, 0xd9, 0xd9));
        if let Some(nc) = color_button(ui, ed, c) {
            set_fill_color(ed, ids, nc);
        }
        let dark = caps
            .first()
            .and_then(|n| n.style.fill_pair())
            .and_then(|(_, d)| d)
            .unwrap_or(c);
        if let Some(nc) = dark_color_button(ui, ed, dark, "fill color") {
            set_fill_color_dark(ed, ids, nc);
        }
        if let Some(nc) = hex_edit(ui, "fill", ids, c, color_mixed) {
            set_fill_color(ed, ids, nc);
        }
        if color_mixed {
            mixed_badge(ui);
        }
    });
    if !all_equal(&caps.iter().map(FillMode::of).collect::<Vec<_>>()) {
        mixed_badge(ui);
    }
    let gradients: Vec<bool> = caps
        .iter()
        .map(|n| n.style.fills.last().is_some_and(|p| p.is_gradient()))
        .collect();
    if let Some(on) = mixed_checkbox(ui, "Gradient", &gradients) {
        apply(ed, "gradient", ids, |n| {
            if !fill_applies(n) {
                return;
            }
            if on {
                let base = n
                    .style
                    .fills
                    .last()
                    .map_or(Color::WHITE, |p| p.preview_color());
                let gradient = match n.style.saved_gradient.clone() {
                    Some(p) if p.is_gradient() => p,
                    _ => Paint::linear(base, Color::rgba(base.r, base.g, base.b, 0)),
                };
                n.style.fills = vec![gradient];
                n.style.fill_auto = false;
                n.style.fill_enabled = true;
            } else if let Some(p) = n.style.fills.last().filter(|p| p.is_gradient()).cloned() {
                n.style.saved_gradient = Some(p.clone());
                n.style.fills = vec![Paint::solid(p.preview_color())];
            }
        });
    }
    if first.style.fill_auto {
        ui.label(
            RichText::new("Auto follows the diagram scheme or the shape default.")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
    }
    if let Some(p) = first
        .style
        .fills
        .last()
        .filter(|p| p.is_gradient())
        .cloned()
    {
        gradient_controls(ui, ed, ids, &p);
    }
}

/// Stop colors (and the linear angle) for the selected element's gradient.
fn gradient_controls(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], paint: &Paint) {
    let (stops, linear, from, to) = match paint {
        Paint::Linear { stops, from, to } => (stops.clone(), true, *from, *to),
        Paint::Radial {
            stops,
            center,
            radius,
        } => (stops.clone(), false, *center, *radius),
        _ => return,
    };
    let rebuild = |stops: Vec<Stop>, from: crate::geom::V2, to: crate::geom::V2| {
        if linear {
            Paint::Linear { stops, from, to }
        } else {
            Paint::Radial {
                stops,
                center: from,
                radius: to,
            }
        }
    };
    ui.horizontal(|ui| {
        ui.add_space(LABEL_W * 0.5);
        ui.label(
            RichText::new("Stops")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
        for (i, s) in stops.iter().enumerate().take(6) {
            if let Some(c) = color_button(ui, ed, s.color) {
                let mut ns = stops.clone();
                ns[i].color = c;
                let np = rebuild(ns, from, to);
                apply(ed, "gradient", ids, |n| {
                    if fill_applies(n) {
                        n.style.fills = vec![np.clone()];
                    }
                });
            }
            if let Some(dc) = dark_color_button(ui, ed, s.dark.unwrap_or(s.color), "gradient stop")
            {
                let mut ns = stops.clone();
                ns[i].dark = Some(dc);
                let np = rebuild(ns, from, to);
                apply(ed, "dark gradient stop", ids, |n| {
                    if fill_applies(n) {
                        n.style.fills = vec![np.clone()];
                    }
                });
            }
        }
        if linear {
            let d = to - from;
            let ang = d.y.atan2(d.x).to_degrees();
            if let Some(v) = num_field(ui, ed, "", ang, 1.0, -360.0..=360.0, 54.0) {
                let a = v.to_radians();
                let half = v2(a.cos(), a.sin()) * 0.5;
                let c = v2(0.5, 0.5);
                let np = Paint::Linear {
                    stops: stops.clone(),
                    from: c - half,
                    to: c + half,
                };
                apply(ed, "gradient", ids, |n| {
                    if fill_applies(n) {
                        n.style.fills = vec![np.clone()];
                    }
                });
            }
        }
    });
}

fn line_on(n: &Node) -> bool {
    n.style.stroke_enabled && n.style.stroke.is_some()
}

fn line_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.line",
        "Line",
        &["Line", "Style", "Perimeter"],
    ) else {
        return;
    };
    let nodes: Vec<&Node> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .filter(|n| !n.is_group())
        .collect();
    let states: Vec<bool> = nodes.iter().map(|n| line_on(n)).collect();
    let colors: Vec<Option<Color>> = nodes.iter().map(|n| n.style.stroke).collect();
    let color_mixed = !all_equal(&colors);
    let dash_kinds: Vec<u8> = nodes
        .iter()
        .map(|n| match n.style.dash.as_slice() {
            [] => 0u8,
            [a, _] if *a <= 2.0 => 2,
            _ => 1,
        })
        .collect();
    let dash_mixed = !all_equal(&dash_kinds);
    let widths: Vec<f64> = nodes.iter().map(|n| n.style.stroke_width).collect();
    let width_mixed = !all_equal(&widths);
    let perimeters: Vec<f64> = nodes.iter().map(|n| n.style.perimeter).collect();
    let perimeter_mixed = !all_equal(&perimeters);
    ui.horizontal(|ui| {
        if let Some(on) = mixed_checkbox(ui, "Line", &states) {
            apply(ed, "line", ids, |n| {
                if n.is_group() {
                    return;
                }
                n.style.stroke_enabled = on;
                if on && n.style.stroke.is_none() {
                    n.style.stroke = Some(Color::INK);
                }
            });
        }
        let c = first.style.stroke.unwrap_or(Color::INK);
        let r = ui.add_enabled_ui(first.style.stroke_enabled, |ui| color_button(ui, ed, c));
        if let Some(nc) = r.inner {
            apply(ed, "stroke", ids, |n| {
                n.style.stroke = Some(nc);
                n.style.stroke_enabled = true;
            });
        }
        if color_mixed {
            mixed_badge(ui);
        }
    });
    row(ui, "Style", |ui| {
        let s = &first.style;
        let dash_kind = match s.dash.as_slice() {
            [] => 0u8,
            [a, _] if *a <= 2.0 => 2,
            _ => 1,
        };
        let dashes = [(0u8, "Solid"), (1, "Dashed"), (2, "Dotted")];
        if let Some(k) = combo(ui, "dash", dash_kind, &dashes, 80.0) {
            let w = s.stroke_width.max(1.0);
            apply(ed, "dash", ids, |n| {
                n.style.dash = match k {
                    0 => vec![],
                    1 => vec![w * 4.0, w * 3.0],
                    _ => vec![w.min(2.0), w * 2.5],
                };
                if k == 2 {
                    n.style.cap = Cap::Round;
                }
            });
        }
        if dash_mixed {
            mixed_badge(ui);
        }
        let mut w = s.stroke_width;
        let r = ui.add_sized(
            [62.0, 20.0],
            egui::DragValue::new(&mut w)
                .range(0.0..=500.0)
                .speed(0.1)
                .max_decimals(2)
                .suffix(" pt"),
        );
        if r.changed() {
            apply(ed, "stroke width", ids, |n| n.style.stroke_width = w);
        }
        seal_on_release(ed, &r);
        if width_mixed {
            mixed_badge(ui);
        }
    });
    ui.horizontal(|ui| {
        if let Some(v) = num_field(
            ui,
            ed,
            "Perimeter  ",
            first.style.perimeter,
            0.5,
            0.0..=1e4,
            76.0,
        ) {
            apply(ed, "perimeter", ids, |n| n.style.perimeter = v);
        }
        if perimeter_mixed {
            mixed_badge(ui);
        }
    });
}

fn opacity_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], _first: &Node) {
    let Some(_guard) = section_begin(ui, st, "style.opacity", "Opacity", &["Opacity"]) else {
        return;
    };
    let values: Vec<f64> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .map(|n| (n.style.opacity * 100.0).round())
        .collect();
    let mixed = !all_equal(&values);
    row(ui, "Opacity", |ui| {
        let mut v = values.first().copied().unwrap_or(100.0);
        let r = ui.add_sized(
            [72.0, 20.0],
            egui::DragValue::new(&mut v)
                .range(0.0..=100.0)
                .speed(1.0)
                .suffix(" %"),
        );
        if r.changed() {
            apply(ed, "opacity", ids, |n| {
                n.style.opacity = (v / 100.0).clamp(0.0, 1.0)
            });
        }
        seal_on_release(ed, &r);
        if mixed {
            mixed_badge(ui);
        }
    });
}

fn rounded_shape(n: &Node) -> bool {
    matches!(
        n.kind,
        NodeKind::Rect | NodeKind::Frame { .. } | NodeKind::Polygon { .. } | NodeKind::Star { .. }
    )
}

fn sketch_effective(n: &Node, ed: &Editor) -> bool {
    n.style
        .sketch
        .unwrap_or(n.style.rough.is_some() || ed.doc.settings.sketch)
}

fn glass_capable(n: &Node) -> bool {
    fill_applies(n) || matches!(n.kind, NodeKind::Path { .. } | NodeKind::Ellipse)
}

fn effects_section(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, ids: &[NodeId], first: &Node) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.effects",
        "Effects",
        &["Rounded", "Sketch", "Glass", "Shadow"],
    ) else {
        return;
    };
    let nodes: Vec<&Node> = ids.iter().filter_map(|id| ed.doc.get(*id)).collect();
    let rounded: Vec<bool> = nodes
        .iter()
        .filter(|n| rounded_shape(n))
        .map(|n| n.style.corner_radius > 0.5 || n.style.radii.is_some())
        .collect();
    let sketch: Vec<bool> = nodes.iter().map(|n| sketch_effective(n, ed)).collect();
    let glass: Vec<bool> = nodes
        .iter()
        .filter(|n| glass_capable(n))
        .map(|n| n.style.glass)
        .collect();
    let shadow: Vec<bool> = nodes
        .iter()
        .filter(|n| !n.is_group())
        .map(|n| n.style.shadow.unwrap_or(ed.doc.settings.shadow))
        .collect();
    let can_round = !rounded.is_empty();
    let can_sketch = !sketch.is_empty();
    let can_glass = !glass.is_empty();
    let can_shadow = !shadow.is_empty();
    if can_round || can_sketch {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if can_round
                && let Some(on) = effect_checkbox(
                    ui,
                    "Rounded",
                    &rounded,
                    true,
                    "Rounded corners (rectangles, frames, polygons and stars)",
                )
            {
                apply(ed, "rounded", ids, |n| {
                    if rounded_shape(n) {
                        n.style.corner_radius = if on { 8.0 } else { 0.0 };
                        n.style.radii = None;
                    }
                });
            }
            if can_sketch
                && let Some(on) = effect_checkbox(
                    ui,
                    "Sketch",
                    &sketch,
                    true,
                    "Hand-drawn style for this element (overrides the diagram setting)",
                )
            {
                apply(ed, "sketch", ids, |n| {
                    n.style.sketch = Some(on);
                    if on && n.style.rough.is_none() {
                        n.style.rough = Some(crate::model::rough::Rough::default());
                    }
                });
            }
        });
    }
    if can_glass || can_shadow {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if can_glass
                && let Some(on) =
                    effect_checkbox(ui, "Glass", &glass, true, "Glossy highlight over the fill")
            {
                apply(ed, "glass", ids, |n| n.style.glass = on);
            }
            if can_shadow
                && let Some(on) = effect_checkbox(
                    ui,
                    "Shadow",
                    &shadow,
                    true,
                    "Drop shadow for this element (overrides the diagram setting)",
                )
            {
                apply(ed, "shadow", ids, |n| n.style.shadow = Some(on));
            }
        });
    }
    let _ = first;
}

fn effect_checkbox(
    ui: &mut Ui,
    label: &str,
    values: &[bool],
    enabled: bool,
    tip: &str,
) -> Option<bool> {
    let mut v = values.first().copied().unwrap_or(false);
    let mixed = !all_equal(values);
    let r = ui.add_enabled(
        enabled,
        egui::Checkbox::new(&mut v, label).indeterminate(mixed),
    );
    if enabled && r.clone().on_hover_text(tip).changed() {
        Some(v)
    } else {
        if !enabled {
            r.on_disabled_hover_text(tip);
        }
        None
    }
}

/// Style actions: JSON edit, style clipboard and defaults.
fn style_actions(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.actions",
        "Actions",
        &[
            "Edit",
            "Edit Style JSON…",
            "Copy Style",
            "Paste Style",
            "Set as Default Style",
        ],
    ) else {
        return;
    };
    let mut edit = false;
    egui::ComboBox::from_id_salt("style-edit")
        .selected_text("Edit")
        .width(vis)
        .show_ui(ui, |ui| {
            if ui.selectable_label(false, "Edit Style JSON…").clicked() {
                edit = true;
            }
        });
    if edit {
        st.style_json = Some(StyleJsonDialog {
            json: serde_json::to_string_pretty(&first.style).unwrap_or_else(|_| "{}".into()),
            error: None,
        });
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let w = (vis - 4.0) * 0.5;
        if ui
            .add_sized([w, 20.0], egui::Button::new("Copy Style"))
            .on_hover_text("Copy the visual and text style of the first selected element")
            .clicked()
        {
            ed.copy_props();
        }
        let r = ui
            .add_enabled_ui(ed.has_style_clipboard(), |ui| {
                ui.add_sized([w, 20.0], egui::Button::new("Paste Style"))
                    .on_hover_text("Apply the copied style to the selection")
            })
            .inner;
        if ed.has_style_clipboard() {
            if r.clicked() {
                ed.paste_props();
            }
        } else {
            r.on_disabled_hover_text("Copy a style first.");
        }
    });
    let _ = ids;
    if ui
        .add_sized([vis, 20.0], egui::Button::new("Set as Default Style"))
        .on_hover_text("New shapes or connectors use this style")
        .clicked()
    {
        ed.set_default_style();
    }
}

#[derive(Clone, Debug, PartialEq)]
enum StyleValue {
    Bool(bool),
    Number(f64),
    Blend(BlendMode),
    Dash(Vec<f64>),
}

struct StyleProperty {
    key: &'static str,
    applies: fn(&Node) -> bool,
    get: fn(&Node, &Editor) -> String,
    parse: fn(&str) -> Result<StyleValue, String>,
    set: fn(&mut Editor, &[NodeId], StyleValue),
}

fn is_connector(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Connector { .. })
}

fn any_element(n: &Node) -> bool {
    !n.is_group()
}

fn any_shape(n: &Node) -> bool {
    any_element(n) && !is_connector(n)
}

fn applies_corner(n: &Node) -> bool {
    any_shape(n) && rounded_shape(n)
}

fn flag_text(v: bool) -> &'static str {
    if v { "1" } else { "0" }
}

fn parse_number(raw: &str, min: f64, max: f64) -> Result<f64, String> {
    let v: f64 = raw
        .trim()
        .parse()
        .map_err(|_| format!("enter a number from {min} to {max}"))?;
    if !v.is_finite() || v < min || v > max {
        Err(format!("enter a number from {min} to {max}"))
    } else {
        Ok(v)
    }
}

fn parse_flag(raw: &str) -> Result<StyleValue, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(StyleValue::Bool(true)),
        "0" | "false" | "no" | "off" => Ok(StyleValue::Bool(false)),
        _ => Err("use 1 or 0".into()),
    }
}

fn parse_dash(raw: &str) -> Result<StyleValue, String> {
    let parts: Vec<&str> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() > 16 {
        return Err("use at most 16 dash numbers".into());
    }
    if !parts.len().is_multiple_of(2) {
        return Err("dash numbers come in on/off pairs".into());
    }
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let v: f64 = part
            .parse()
            .map_err(|_| format!("invalid dash number {part:?}"))?;
        if !v.is_finite() || v <= 0.0 || v > 10_000.0 {
            return Err("dash numbers must be greater than 0 and at most 10000".into());
        }
        out.push(v);
    }
    Ok(StyleValue::Dash(out))
}

fn parse_blend(raw: &str) -> Result<StyleValue, String> {
    let want = raw.trim().to_ascii_lowercase();
    BlendMode::ALL
        .iter()
        .copied()
        .find(|b| b.css() == want)
        .map(StyleValue::Blend)
        .ok_or_else(|| "use a CSS blend mode name such as normal or multiply".into())
}

fn get_opacity(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.style.opacity * 100.0)
}

fn parse_opacity(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, 0.0, 100.0).map(|v| StyleValue::Number(v / 100.0))
}

fn set_opacity(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "opacity", ids, |n| n.style.opacity = v.clamp(0.0, 1.0));
    }
}

fn get_rotation(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.style.rotation)
}

fn parse_rotation(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, -360.0, 360.0).map(StyleValue::Number)
}

fn set_rotation_prop(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        ed.set_rotation(ids, v);
    }
}

fn get_stroke_width(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.style.stroke_width)
}

fn parse_stroke_width(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, 0.0, 500.0).map(StyleValue::Number)
}

fn set_stroke_width(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "stroke width", ids, |n| n.style.stroke_width = v);
    }
}

fn get_dash(n: &Node, _ed: &Editor) -> String {
    n.style
        .dash
        .iter()
        .map(|v| fmt_num(*v))
        .collect::<Vec<_>>()
        .join(" ")
}

fn set_dash(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Dash(v) = value {
        apply(ed, "dash", ids, |n| n.style.dash = v.clone());
    }
}

fn get_blend(n: &Node, _ed: &Editor) -> String {
    n.style.blend.css().to_string()
}

fn set_blend(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Blend(v) = value {
        apply(ed, "blend", ids, |n| n.style.blend = v);
    }
}

fn get_shadow(n: &Node, ed: &Editor) -> String {
    flag_text(n.style.shadow.unwrap_or(ed.doc.settings.shadow)).into()
}

fn set_shadow(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "shadow", ids, |n| n.style.shadow = Some(v));
    }
}

fn get_sketch(n: &Node, ed: &Editor) -> String {
    flag_text(sketch_effective(n, ed)).into()
}

fn set_sketch(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "sketch", ids, |n| n.style.sketch = Some(v));
    }
}

fn get_glass(n: &Node, _ed: &Editor) -> String {
    flag_text(n.style.glass).into()
}

fn set_glass(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "glass", ids, |n| n.style.glass = v);
    }
}

fn get_fill_enabled(n: &Node, _ed: &Editor) -> String {
    flag_text(n.style.fill_enabled).into()
}

fn set_fill_enabled(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "fill", ids, |n| {
            if fill_applies(n) {
                n.style.fill_enabled = v;
            }
        });
    }
}

fn get_fill_auto(n: &Node, _ed: &Editor) -> String {
    flag_text(n.style.fill_auto).into()
}

fn set_fill_auto(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "fill mode", ids, |n| {
            if fill_applies(n) {
                n.style.fill_auto = v;
            }
        });
    }
}

fn get_stroke_enabled(n: &Node, _ed: &Editor) -> String {
    flag_text(n.style.stroke_enabled).into()
}

fn set_stroke_enabled(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "line", ids, |n| n.style.stroke_enabled = v);
    }
}

fn get_corner_radius(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.style.corner_radius)
}

fn parse_corner_radius(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, 0.0, 10_000.0).map(StyleValue::Number)
}

fn set_corner_radius(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "radius", ids, |n| n.style.corner_radius = v);
    }
}

fn get_perimeter(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.style.perimeter)
}

fn parse_perimeter(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, 0.0, 10_000.0).map(StyleValue::Number)
}

fn set_perimeter(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "perimeter", ids, |n| n.style.perimeter = v);
    }
}

fn get_label_upright(n: &Node, _ed: &Editor) -> String {
    flag_text(n.style.label_upright).into()
}

fn set_label_upright(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Bool(v) = value {
        apply(ed, "upright label", ids, |n| n.style.label_upright = v);
    }
}

fn get_start_size(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.start_head_size)
}

fn get_end_size(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.end_head_size)
}

fn parse_head_size(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, 0.0, 100.0).map(StyleValue::Number)
}

fn set_start_size(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "start head size", ids, |n| {
            if is_connector(n) {
                n.start_head_size = v;
            }
        });
    }
}

fn set_end_size(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "end head size", ids, |n| {
            if is_connector(n) {
                n.end_head_size = v;
            }
        });
    }
}

fn get_start_offset(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.start_head_offset)
}

fn get_end_offset(n: &Node, _ed: &Editor) -> String {
    fmt_num(n.end_head_offset)
}

fn parse_head_offset(raw: &str) -> Result<StyleValue, String> {
    parse_number(raw, -100.0, 100.0).map(StyleValue::Number)
}

fn set_start_offset(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "start head offset", ids, |n| {
            if is_connector(n) {
                n.start_head_offset = v;
            }
        });
    }
}

fn set_end_offset(ed: &mut Editor, ids: &[NodeId], value: StyleValue) {
    if let StyleValue::Number(v) = value {
        apply(ed, "end head offset", ids, |n| {
            if is_connector(n) {
                n.end_head_offset = v;
            }
        });
    }
}

static STYLE_PROPERTIES: &[StyleProperty] = &[
    StyleProperty {
        key: "opacity",
        applies: any_element,
        get: get_opacity,
        parse: parse_opacity,
        set: set_opacity,
    },
    StyleProperty {
        key: "rotation",
        applies: any_element,
        get: get_rotation,
        parse: parse_rotation,
        set: set_rotation_prop,
    },
    StyleProperty {
        key: "strokeWidth",
        applies: any_element,
        get: get_stroke_width,
        parse: parse_stroke_width,
        set: set_stroke_width,
    },
    StyleProperty {
        key: "dash",
        applies: any_element,
        get: get_dash,
        parse: parse_dash,
        set: set_dash,
    },
    StyleProperty {
        key: "blend",
        applies: any_element,
        get: get_blend,
        parse: parse_blend,
        set: set_blend,
    },
    StyleProperty {
        key: "shadow",
        applies: any_element,
        get: get_shadow,
        parse: parse_flag,
        set: set_shadow,
    },
    StyleProperty {
        key: "sketch",
        applies: any_element,
        get: get_sketch,
        parse: parse_flag,
        set: set_sketch,
    },
    StyleProperty {
        key: "glass",
        applies: glass_capable,
        get: get_glass,
        parse: parse_flag,
        set: set_glass,
    },
    StyleProperty {
        key: "fillEnabled",
        applies: fill_applies,
        get: get_fill_enabled,
        parse: parse_flag,
        set: set_fill_enabled,
    },
    StyleProperty {
        key: "fillAuto",
        applies: fill_applies,
        get: get_fill_auto,
        parse: parse_flag,
        set: set_fill_auto,
    },
    StyleProperty {
        key: "strokeEnabled",
        applies: any_element,
        get: get_stroke_enabled,
        parse: parse_flag,
        set: set_stroke_enabled,
    },
    StyleProperty {
        key: "cornerRadius",
        applies: applies_corner,
        get: get_corner_radius,
        parse: parse_corner_radius,
        set: set_corner_radius,
    },
    StyleProperty {
        key: "perimeter",
        applies: any_shape,
        get: get_perimeter,
        parse: parse_perimeter,
        set: set_perimeter,
    },
    StyleProperty {
        key: "labelUpright",
        applies: any_shape,
        get: get_label_upright,
        parse: parse_flag,
        set: set_label_upright,
    },
    StyleProperty {
        key: "startSize",
        applies: is_connector,
        get: get_start_size,
        parse: parse_head_size,
        set: set_start_size,
    },
    StyleProperty {
        key: "endSize",
        applies: is_connector,
        get: get_end_size,
        parse: parse_head_size,
        set: set_end_size,
    },
    StyleProperty {
        key: "startOffset",
        applies: is_connector,
        get: get_start_offset,
        parse: parse_head_offset,
        set: set_start_offset,
    },
    StyleProperty {
        key: "endOffset",
        applies: is_connector,
        get: get_end_offset,
        parse: parse_head_offset,
        set: set_end_offset,
    },
];

fn set_style_property(ed: &mut Editor, ids: &[NodeId], key: &str, raw: &str) -> Result<(), String> {
    let prop = STYLE_PROPERTIES
        .iter()
        .find(|p| p.key == key)
        .ok_or_else(|| format!("unknown property {key}"))?;
    let value = (prop.parse)(raw)?;
    (prop.set)(ed, ids, value);
    Ok(())
}

fn properties_section(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    ids: &[NodeId],
    first: &Node,
) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "style.properties",
        "Property / Value",
        &[
            "Value",
            "Search properties",
            "Fill",
            "Fill mode",
            "Gradient",
            "Line",
            "Dash",
            "Perimeter",
            "Opacity",
            "Corner",
            "Rotation",
            "Blend",
            "Sketch",
            "Shadow",
            "Glass",
            "opacity",
            "rotation",
            "strokeWidth",
            "dash",
            "blend",
            "fillEnabled",
            "fillAuto",
            "strokeEnabled",
            "cornerRadius",
            "perimeter",
            "labelUpright",
            "startSize",
            "endSize",
            "startOffset",
            "endOffset",
        ],
    ) else {
        return;
    };
    let mixed = {
        let styles: Vec<&crate::model::Style> = ids
            .iter()
            .filter_map(|id| ed.doc.get(*id))
            .map(|n| &n.style)
            .collect();
        !all_equal(&styles)
    };
    if mixed {
        mixed_badge(ui);
    }
    let s = first.style.clone();
    if fill_applies(first) {
        let enabled: Vec<bool> = ids
            .iter()
            .filter_map(|id| ed.doc.get(*id))
            .filter(|n| fill_applies(n))
            .map(|n| n.style.fill_enabled)
            .collect();
        let gradients: Vec<bool> = ids
            .iter()
            .filter_map(|id| ed.doc.get(*id))
            .filter(|n| fill_applies(n))
            .map(|n| n.style.fills.last().is_some_and(|p| p.is_gradient()))
            .collect();
        row(ui, "Fill", |ui| {
            if let Some(on) = mixed_checkbox(ui, "", &enabled) {
                apply(ed, "fill", ids, |n| {
                    if fill_applies(n) {
                        n.style.fill_enabled = on;
                    }
                });
            }
            let c = fill_color_of(first, ed);
            if let Some(nc) = color_button(ui, ed, c) {
                set_fill_color(ed, ids, nc);
            }
        });
        row(ui, "Fill mode", |ui| {
            let modes: Vec<(FillMode, &str)> =
                FillMode::ALL[..4].iter().map(|m| (*m, m.label())).collect();
            if let Some(m) = combo(ui, "prop-fill-mode", FillMode::of(first), &modes, 80.0) {
                apply(ed, "fill mode", ids, |n| {
                    if !fill_applies(n) {
                        return;
                    }
                    n.style.fill_auto = m == FillMode::Auto;
                    n.style.fill_enabled = true;
                });
            }
        });
        row(ui, "Gradient", |ui| {
            if let Some(on) = mixed_checkbox(ui, "", &gradients) {
                apply(ed, "gradient", ids, |n| {
                    if !fill_applies(n) {
                        return;
                    }
                    if on && !n.style.fills.last().is_some_and(|p| p.is_gradient()) {
                        let base = n
                            .style
                            .fills
                            .last()
                            .map_or(Color::WHITE, |p| p.preview_color());
                        n.style.fills =
                            vec![Paint::linear(base, Color::rgba(base.r, base.g, base.b, 0))];
                        n.style.fill_auto = false;
                    }
                });
            }
        });
    }
    if !first.is_group() {
        let states: Vec<bool> = ids
            .iter()
            .filter_map(|id| ed.doc.get(*id))
            .filter(|n| !n.is_group())
            .map(line_on)
            .collect();
        row(ui, "Line", |ui| {
            if let Some(on) = mixed_checkbox(ui, "", &states) {
                apply(ed, "line", ids, |n| n.style.stroke_enabled = on);
            }
            let c = s.stroke.unwrap_or(Color::INK);
            if let Some(nc) = color_button(ui, ed, c) {
                set_stroke_color(ed, ids, nc);
            }
            let dark = s.stroke_dark.unwrap_or(c);
            if let Some(nc) = dark_color_button(ui, ed, dark, "line color") {
                set_stroke_color_dark(ed, ids, nc);
            }
            if let Some(v) = num_field(ui, ed, "", s.stroke_width, 0.1, 0.0..=500.0, 58.0) {
                apply(ed, "stroke width", ids, |n| n.style.stroke_width = v);
            }
        });
        row(ui, "Dash", |ui| {
            let kind = match s.dash.as_slice() {
                [] => 0u8,
                [a, _] if *a <= 2.0 => 2,
                _ => 1,
            };
            let opts = [(0u8, "Solid"), (1, "Dashed"), (2, "Dotted")];
            if let Some(k) = combo(ui, "prop-dash", kind, &opts, 80.0) {
                let w = s.stroke_width.max(1.0);
                apply(ed, "dash", ids, |n| {
                    n.style.dash = match k {
                        0 => vec![],
                        1 => vec![w * 4.0, w * 3.0],
                        _ => vec![w.min(2.0), w * 2.5],
                    };
                });
            }
        });
        row(ui, "Perimeter", |ui| {
            if let Some(v) = num_field(ui, ed, "", s.perimeter, 0.5, 0.0..=1e4, 70.0) {
                apply(ed, "perimeter", ids, |n| n.style.perimeter = v);
            }
        });
    }
    row(ui, "Opacity", |ui| {
        if let Some(v) = num_field(ui, ed, "", s.opacity * 100.0, 1.0, 0.0..=100.0, 62.0) {
            apply(ed, "opacity", ids, |n| {
                n.style.opacity = (v / 100.0).clamp(0.0, 1.0)
            });
        }
    });
    if rounded_shape(first) {
        row(ui, "Corner", |ui| {
            let mut r = s.corner_radius;
            let resp = ui.add_sized(
                [70.0, 20.0],
                egui::DragValue::new(&mut r)
                    .range(0.0..=1e4)
                    .speed(0.5)
                    .max_decimals(2)
                    .suffix(" pt"),
            );
            if resp.changed() && r.is_finite() {
                apply(ed, "radius", ids, |n| n.style.corner_radius = r);
            }
        });
    }
    let all_connectors = !ids.is_empty()
        && ids
            .iter()
            .all(|id| ed.doc.get(*id).is_some_and(|n| n.is_connector()));
    if !all_connectors {
        row(ui, "Rotation", |ui| {
            if let Some(v) = num_field(ui, ed, "", s.rotation, 1.0, -360.0..=360.0, 68.0) {
                ed.set_rotation(ids, v);
            }
        });
    }
    row(ui, "Blend", |ui| {
        let opts: Vec<(BlendMode, &str)> = BlendMode::ALL.iter().map(|b| (*b, b.label())).collect();
        if let Some(v) = combo(ui, "prop-blend", s.blend, &opts, 100.0) {
            apply(ed, "blend", ids, |n| n.style.blend = v);
        }
    });
    let sketches: Vec<bool> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .map(|n| sketch_effective(n, ed))
        .collect();
    row(ui, "Sketch", |ui| {
        if let Some(on) = mixed_checkbox(ui, "", &sketches) {
            apply(ed, "sketch", ids, |n| n.style.sketch = Some(on));
        }
    });
    let shadows: Vec<bool> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .filter(|n| !n.is_group())
        .map(|n| n.style.shadow.unwrap_or(ed.doc.settings.shadow))
        .collect();
    row(ui, "Shadow", |ui| {
        if let Some(on) = mixed_checkbox(ui, "", &shadows) {
            apply(ed, "shadow", ids, |n| n.style.shadow = Some(on));
        }
    });
    let glasses: Vec<bool> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .filter(|n| glass_capable(n))
        .map(|n| n.style.glass)
        .collect();
    if !glasses.is_empty() {
        row(ui, "Glass", |ui| {
            if let Some(on) = mixed_checkbox(ui, "", &glasses) {
                apply(ed, "glass", ids, |n| n.style.glass = on);
            }
        });
    }

    separator(ui);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(
            RichText::new("Search properties")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
        ui.add_sized(
            [ui.available_width().max(80.0), 20.0],
            egui::TextEdit::singleline(&mut st.style_property_filter).hint_text("key"),
        );
    });
    let query = normalize_query(&st.style_property_filter);
    let mut shown = 0usize;
    for prop in STYLE_PROPERTIES {
        if !(prop.applies)(first) || !query_matches(&query, prop.key) {
            continue;
        }
        shown += 1;
        let draft_key = format!("{ids:?}:{}", prop.key);
        let current = (prop.get)(first, ed);
        let mut commit = None;
        row(ui, prop.key, |ui| {
            let draft = st
                .style_property_drafts
                .entry(draft_key.clone())
                .or_insert_with(|| current.clone());
            let response = ui.add_sized(
                [ui.available_width().max(80.0), 20.0],
                egui::TextEdit::singleline(draft).id_source(egui::Id::new(draft_key.clone())),
            );
            let enter = response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.lost_focus() || enter {
                commit = Some(draft.clone());
            }
        });
        if let Some(raw) = commit {
            if raw == current || raw.trim().is_empty() {
                st.style_property_drafts.remove(&draft_key);
            } else {
                match set_style_property(ed, ids, prop.key, &raw) {
                    Ok(()) => {
                        st.style_property_drafts.remove(&draft_key);
                        st.style_property_error = None;
                    }
                    Err(e) => {
                        st.style_property_error = Some(format!("{}: {}", prop.key, e));
                    }
                }
            }
        }
    }
    if shown == 0 {
        ui.label(
            RichText::new("No matching style keys.")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
    }
    if let Some(e) = &st.style_property_error {
        ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xd1, 0x3b, 0x3b)));
    }
}

fn set_stroke_color(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "stroke", ids, |n| {
        n.style.stroke = Some(c);
        n.style.stroke_enabled = true;
    });
}

fn set_stroke_color_dark(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "dark stroke", ids, |n| {
        n.style.stroke.get_or_insert(Color::INK);
        n.style.stroke_dark = Some(c);
        n.style.stroke_enabled = true;
    });
}

/// Structural actions that used to sit in the panel header: boolean
/// operations, grouping, components, instances and path editing.
fn has_selection_actions(ed: &Editor, ids: &[NodeId], first: &Node) -> bool {
    if ids.len() > 1 {
        return true;
    }
    if first.component
        || ed.is_instance_root(first.id)
        || (first.is_frame() || first.is_group())
        || first
            .instance
            .as_ref()
            .is_some_and(|l| !l.overrides.is_empty())
        || (first.is_container() && !first.children.is_empty())
    {
        return true;
    }
    !first.is_container() || matches!(first.kind, NodeKind::Path { .. })
}

fn selection_actions(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    if ids.len() > 1 {
        ui.horizontal_wrapped(|ui| {
            for op in [
                BoolOp::Union,
                BoolOp::Subtract,
                BoolOp::Intersect,
                BoolOp::Exclude,
            ] {
                if ui
                    .small_button(op.label())
                    .on_hover_text("Combine the selected shapes")
                    .clicked()
                {
                    ed.boolean(op);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("Frame selection").clicked() {
                ed.run(crate::editor::shortcuts::Action::WrapInFrame);
            }
            if ui.small_button("Component").clicked() {
                ed.create_component();
            }
            if ids.iter().any(|id| {
                ed.doc
                    .get(*id)
                    .is_some_and(|n| n.is_container() && !n.children.is_empty())
            }) && ui.small_button("Ungroup").clicked()
            {
                ed.ungroup_selection();
            }
        });
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if first.component {
            if ui.small_button("Create instance").clicked() {
                ed.instance_of_selection();
            }
        } else if ed.is_instance_root(first.id) {
            if ui.small_button("Go to main").clicked() {
                ed.go_to_main();
            }
            if ui.small_button("Detach").clicked() {
                ed.detach_instance();
            }
        } else if (first.is_frame() || first.is_group())
            && ui.small_button("Create component").clicked()
        {
            ed.create_component();
        }
        if first.instance.is_some()
            && first
                .instance
                .as_ref()
                .is_some_and(|l| !l.overrides.is_empty())
            && ui.small_button("Reset overrides").clicked()
        {
            ed.reset_overrides();
        }
        if first.is_container()
            && !first.children.is_empty()
            && ui.small_button("Ungroup").clicked()
        {
            ed.ungroup_selection();
        }
        if !first.is_container()
            && !matches!(first.kind, NodeKind::Path { .. })
            && ui.small_button("Flatten").clicked()
        {
            ed.flatten_selection();
        }
        if matches!(first.kind, NodeKind::Path { .. }) && ui.small_button("Edit points").clicked() {
            ed.start_path_edit(first.id);
        }
    });
    if let Some(l) = &first.instance
        && !l.overrides.is_empty()
    {
        ui.label(
            RichText::new(format!("Overrides: {}", l.overrides.join(", ")))
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
    }
}

fn align_row(ui: &mut Ui, ed: &mut Editor, _ids: &[NodeId]) {
    use theme::Icon as I;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (icon, axis, to, tip) in [
            (I::AlignLeft, 0, 0.0, "Align left (⌥A)"),
            (I::AlignHCenter, 0, 0.5, "Align horizontal centers (⌥H)"),
            (I::AlignRight, 0, 1.0, "Align right (⌥D)"),
            (I::AlignTop, 1, 0.0, "Align top (⌥W)"),
            (I::AlignVCenter, 1, 0.5, "Align vertical centers (⌥V)"),
            (I::AlignBottom, 1, 1.0, "Align bottom (⌥S)"),
        ] {
            if icon_btn(ui, icon, false, tip) {
                ed.align(axis, to);
            }
        }
        ui.add_space(8.0);
        if icon_btn(
            ui,
            I::DistributeH,
            false,
            "Distribute horizontal spacing (⌥⇧H)",
        ) {
            ed.distribute(0);
        }
        if icon_btn(
            ui,
            I::DistributeV,
            false,
            "Distribute vertical spacing (⌥⇧V)",
        ) {
            ed.distribute(1);
        }
    });
}

/// The Arrange tab, as in draw.io: z-order, size and position with the
/// aspect lock, rotation, flip, grid snap, grouping and locking.
fn arrange_section(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
) {
    let w = (vis - 4.0) * 0.5;
    let mut order = |ui: &mut Ui, l: &str, a: shortcuts::Action| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui.add_sized([w, 20.0], egui::Button::new(l)).clicked() {
            ed.run(a);
        }
    };
    ui.horizontal(|ui| {
        order(ui, "To Front", shortcuts::Action::ToFront);
        order(ui, "To Back", shortcuts::Action::ToBack);
    });
    ui.horizontal(|ui| {
        order(ui, "Bring Forward", shortcuts::Action::Forward);
        order(ui, "Send Backward", shortcuts::Action::Backward);
    });
    drop(order);
    let all_connectors = !ids.is_empty()
        && ids
            .iter()
            .all(|id| ed.doc.get(*id).is_some_and(|n| n.is_connector()));
    if !all_connectors
        && ui
            .add_sized([vis * 2.0 + 4.0, 20.0], egui::Button::new("Autosize"))
            .on_hover_text("Shrink groups and containers to their contents (Alt+F)")
            .clicked()
    {
        ed.run(shortcuts::Action::Autosize);
    }
    separator(ui);

    let locked = ids.iter().any(|id| ed.doc.is_effectively_locked(*id));
    let all_locked = !ids.is_empty() && ids.iter().all(|id| ed.doc.is_effectively_locked(*id));
    if !all_connectors {
        arrange_size(ui, ed, st, vis, ids, first, locked);
        separator(ui);
        arrange_position(ui, ed, st, vis, ids, locked);
        separator(ui);

        row(ui, "Angle", |ui| {
            let values: Vec<f64> = ids
                .iter()
                .filter_map(|id| ed.doc.get(*id))
                .map(|n| n.style.rotation)
                .collect();
            let v = ui
                .add_enabled_ui(!locked, |ui| {
                    num_field(ui, ed, "", first.style.rotation, 1.0, -360.0..=360.0, 72.0)
                })
                .inner;
            if let Some(v) = v {
                ed.set_rotation(ids, v);
            }
            if !all_equal(&values) {
                mixed_badge(ui);
            }
        });
        let r = ui
            .add_enabled_ui(!locked, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Rotate shape only by 90°"))
                    .on_hover_text("Add a quarter turn, leaving the label upright")
            })
            .inner;
        if locked {
            r.on_disabled_hover_text("Unlock the selection to rotate it.");
        } else if r.clicked() {
            ed.rotate_90(ids);
        }
        row(ui, "Label", |ui| {
            let states: Vec<bool> = ids
                .iter()
                .filter_map(|id| ed.doc.get(*id))
                .map(|n| n.style.label_upright)
                .collect();
            if let Some(on) = ui
                .add_enabled_ui(!locked, |ui| {
                    mixed_checkbox(ui, "Keep label upright", &states)
                })
                .inner
            {
                apply(ed, "label upright", ids, |n| n.style.label_upright = on);
            }
        });
        if ids.iter().any(|id| {
            ed.doc
                .get(*id)
                .is_some_and(|n| n.can_have_connector_default())
        }) {
            row(ui, "Connector", |ui| {
                let page_default = ed.doc.settings.connector_default.is_some();
                let any_set = ids.iter().any(|id| {
                    ed.doc
                        .get(*id)
                        .is_some_and(|n| n.connector_default.is_some())
                });
                let all_set = ids.iter().all(|id| {
                    ed.doc
                        .get(*id)
                        .is_some_and(|n| n.connector_default.is_some())
                });
                let use_page = ui
                    .add_enabled(
                        !locked && page_default,
                        egui::Button::new("Use page default"),
                    )
                    .on_hover_text(
                        "Copy the page's connector default onto these shapes for new arrows",
                    );
                if !page_default {
                    use_page.on_disabled_hover_text("Set a connector default on the page first.");
                } else if use_page.clicked() {
                    ed.set_connector_default_from_page(ids);
                }
                let clear = ui
                    .add_enabled(!locked && any_set, egui::Button::new("Clear"))
                    .on_hover_text("Remove the source connector default from these shapes");
                if clear.clicked() {
                    ed.clear_connector_defaults(ids);
                }
                if any_set && !all_set {
                    mixed_badge(ui);
                }
            });
        }
        separator(ui);
    }

    row(ui, "Flip", |ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let half = (ui.available_width() - 4.0) * 0.5;
        let h = ui
            .add_enabled_ui(!all_locked, |ui| {
                ui.add_sized([half, 20.0], egui::Button::new("Horizontal"))
                    .on_hover_text("Mirror around the element's own center")
            })
            .inner;
        if all_locked {
            h.on_disabled_hover_text("Unlock the selection to flip it.");
        } else if h.clicked() {
            ed.run(shortcuts::Action::FlipH);
        }
        let v = ui
            .add_enabled_ui(!all_locked, |ui| {
                ui.add_sized([half, 20.0], egui::Button::new("Vertical"))
                    .on_hover_text("Mirror around the element's own center")
            })
            .inner;
        if all_locked {
            v.on_disabled_hover_text("Unlock the selection to flip it.");
        } else if v.clicked() {
            ed.run(shortcuts::Action::FlipV);
        }
    });
    separator(ui);

    let connectors: Vec<NodeId> = ids
        .iter()
        .copied()
        .filter(|id| ed.doc.get(*id).is_some_and(|n| n.is_connector()))
        .collect();
    if !connectors.is_empty() {
        let r = ui
            .add_sized([vis, 20.0], egui::Button::new("Reverse"))
            .on_hover_text("Swap the source and target of the selected connectors; arrowheads and labels move to the other end");
        if r.clicked() {
            ed.reverse_connectors(&connectors);
        }
        ui.add_space(2.0);
        ui.label(title_text("Reset"));
        let can_labels = connectors.iter().any(|id| {
            ed.doc.get(*id).is_some_and(|n| {
                (n.label_pos.is_some() || n.label_offset.is_some())
                    || n.labels
                        .iter()
                        .any(|l| l.pos.is_some() || l.offset.is_some())
            })
        });
        let can_waypoints = connectors.iter().any(|id| {
            ed.doc.get(*id).is_some_and(|n| match &n.kind {
                NodeKind::Connector { points, route, .. } => {
                    *route != Route::Elbow && points.len() > 2
                }
                _ => false,
            })
        });
        let r = ui
            .add_enabled_ui(can_labels, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Label Positions"))
                    .on_hover_text(
                        "Put the selected connector labels back on the middle of their lines",
                    )
            })
            .inner;
        if !can_labels {
            r.on_disabled_hover_text("The connector labels are already in the default position.");
        } else if r.clicked() {
            ed.reset_connector_label_positions(&connectors);
        }
        let r = ui
            .add_enabled_ui(can_waypoints, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Waypoints"))
                    .on_hover_text("Remove manual bends from the selected connectors")
            })
            .inner;
        if !can_waypoints {
            r.on_disabled_hover_text("The selected connectors have no manual waypoints.");
        } else if r.clicked() {
            ed.reset_connector_waypoints(&connectors);
        }
        separator(ui);
    }

    if !all_connectors {
        ui.label(title_text("Align"));
        align_row(ui, ed, ids);
        ui.add_space(2.0);
        let snap = ui
            .add_enabled_ui(!locked, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Snap to Grid"))
                    .on_hover_text("Round every selected element to the nearest grid line")
            })
            .inner;
        if locked {
            snap.on_disabled_hover_text("Unlock the selection to snap it.");
        } else if snap.clicked() {
            let g = ed.grid_step;
            ed.edit("Snap to grid", |tx| {
                for id in ids {
                    if let Some(n) = tx.node_mut(*id) {
                        n.rect.x = crate::editor::snap::grid(n.rect.x, g);
                        n.rect.y = crate::editor::snap::grid(n.rect.y, g);
                    }
                }
            });
        }
        separator(ui);
    }

    let r = ui
        .add_enabled_ui(ids.len() > 1, |ui| {
            ui.add_sized([vis, 20.0], egui::Button::new("Group"))
                .on_hover_text("Group the selection (⌘G)")
        })
        .inner;
    if ids.len() <= 1 {
        r.on_disabled_hover_text("Select two or more elements to group them.");
    } else if r.clicked() {
        ed.run(shortcuts::Action::Group);
    }
    let any_group = ids
        .iter()
        .any(|id| ed.doc.get(*id).is_some_and(|n| n.is_group()));
    if any_group
        && ui
            .add_sized([vis, 20.0], egui::Button::new("Ungroup"))
            .on_hover_text("Release the selected groups (⌘⇧G)")
            .clicked()
    {
        ed.run(shortcuts::Action::Ungroup);
    }
    if !all_connectors {
        let r = ui
            .add_enabled_ui(ids.len() == 1, |ui| {
                ui.add_sized([vis, 20.0], egui::Button::new("Copy Size"))
                    .on_hover_text("Remember this element's size")
            })
            .inner;
        if ids.len() != 1 {
            r.on_disabled_hover_text("Select one element to copy its size.");
        } else if r.clicked() {
            st.copied_size = Some((first.rect.w, first.rect.h));
        }
        let r = ui
            .add_enabled_ui(st.copied_size.is_some() && !locked, |ui| {
                let (cw, ch) = st.copied_size.unwrap_or_default();
                ui.add_sized(
                    [vis, 20.0],
                    egui::Button::new(format!("Apply Size {} × {}", fmt_num(cw), fmt_num(ch))),
                )
                .on_hover_text("Give every selected element the copied size")
            })
            .inner;
        if st.copied_size.is_none() {
            r.on_disabled_hover_text("Copy a size first.");
        } else if locked {
            r.on_disabled_hover_text("Unlock the selection to resize it.");
        } else if r.clicked()
            && let Some((cw, ch)) = st.copied_size
        {
            let dims = ids.iter().map(|id| (*id, v2(cw, ch))).collect();
            resize_many(ed, ids, dims);
        }
    }
    if ui
        .add_sized([vis, 20.0], egui::Button::new("Lock/Unlock"))
        .on_hover_text("Lock or unlock every selected element (⌘⇧L)")
        .clicked()
    {
        ed.run(shortcuts::Action::ToggleLock);
    }
    separator(ui);
    if !all_connectors && has_selection_actions(ed, ids, first) {
        ui.label(title_text("Selection"));
        selection_actions(ui, ed, ids, first);
    }

    // Connectors can sit inside a frame but draw.io never gives them auto-layout
    // sizing or frame constraints. Keep Layout for shapes and mixed selections.
    let has_layout = !all_connectors && (first.layout.is_some() || first.parent.is_some());
    if has_layout {
        separator(ui);
        ui.label(title_text("Layout"));
        child_layout(ui, ed, ids, first);
    }

    if all_connectors {
        separator(ui);
        data_tab(ui, ed, st, vis, ids, first);
    }
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

fn arrange_size(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
    locked: bool,
) {
    let _ = vis;
    let sizes: Vec<(f64, f64)> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .map(|n| (n.rect.w, n.rect.h))
        .collect();
    let mixed = !all_equal(&sizes);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Size")
                .size(theme::SECONDARY + 1.0)
                .color(theme::pal(ui).secondary),
        );
        ui.add_space(4.0);
        let w = ui
            .add_enabled_ui(!locked, |ui| {
                num_field(ui, ed, "W ", first.rect.w, 1.0, 0.0..=1e6, 76.0)
            })
            .inner;
        if let Some(v) = w {
            let dims = ids
                .iter()
                .filter_map(|id| {
                    let n = ed.doc.get(*id)?;
                    let h = if st.lock_aspect && n.rect.w.abs() > 1e-9 {
                        v * n.rect.h / n.rect.w
                    } else {
                        n.rect.h
                    };
                    Some((*id, v2(v, h.max(0.0))))
                })
                .collect();
            resize_many(ed, ids, dims);
        }
        let h = ui
            .add_enabled_ui(!locked, |ui| {
                num_field(ui, ed, "H ", first.rect.h, 1.0, 0.0..=1e6, 76.0)
            })
            .inner;
        if let Some(v) = h {
            let dims = ids
                .iter()
                .filter_map(|id| {
                    let n = ed.doc.get(*id)?;
                    let w = if st.lock_aspect && n.rect.h.abs() > 1e-9 {
                        v * n.rect.w / n.rect.h
                    } else {
                        n.rect.w
                    };
                    Some((*id, v2(w.max(0.0), v)))
                })
                .collect();
            resize_many(ed, ids, dims);
        }
    });
    let mut lock = st.lock_aspect;
    if ui
        .checkbox(&mut lock, "Constrain Proportions")
        .on_hover_text("Scale width and height together")
        .changed()
    {
        st.lock_aspect = lock;
        if lock && first.rect.h.abs() > 1e-9 {
            st.aspect_ratio = first.rect.w / first.rect.h;
        }
    }
    if mixed {
        mixed_badge(ui);
    }
    if ids.len() > 1 {
        ui.label(
            RichText::new("Width and height apply to each selected element.")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
    }
}

fn arrange_position(
    ui: &mut Ui,
    ed: &mut Editor,
    _st: &mut UiState,
    _vis: f32,
    ids: &[NodeId],
    locked: bool,
) {
    let union = crate::geom::Rect::union_all(
        ids.iter()
            .filter_map(|id| ed.doc.world_rect(*id))
            .collect::<Vec<_>>()
            .iter(),
    );
    let Some(union) = union else { return };
    let mixed = if ids.len() > 1 {
        let rects: Vec<(f64, f64)> = ids
            .iter()
            .filter_map(|id| ed.doc.world_rect(*id))
            .map(|r| (r.x, r.y))
            .collect();
        !all_equal(&rects)
    } else {
        false
    };
    row(ui, "Position", |ui| {
        let x = ui
            .add_enabled_ui(!locked, |ui| {
                num_field(ui, ed, "X ", union.x, 1.0, f64::MIN..=f64::MAX, 82.0)
            })
            .inner;
        if let Some(v) = x {
            let dx = v - union.x;
            translate_many(ed, ids, v2(dx, 0.0));
        }
        let y = ui
            .add_enabled_ui(!locked, |ui| {
                num_field(ui, ed, "Y ", union.y, 1.0, f64::MIN..=f64::MAX, 82.0)
            })
            .inner;
        if let Some(v) = y {
            let dy = v - union.y;
            translate_many(ed, ids, v2(0.0, dy));
        }
        if mixed {
            mixed_badge(ui);
        }
    });
}

fn translate_many(ed: &mut Editor, ids: &[NodeId], d: crate::geom::V2) {
    if !d.x.is_finite() || !d.y.is_finite() || (d.x == 0.0 && d.y == 0.0) {
        return;
    }
    ed.begin_coalescing("Move selection", Some("move-selection"));
    {
        let mut tx = ed.tx();
        for id in ids {
            if let Some(n) = tx.node_mut(*id) {
                n.rect.x += d.x;
                n.rect.y += d.y;
            }
        }
    }
    ed.commit();
}

fn resize_many(ed: &mut Editor, ids: &[NodeId], dims: Vec<(NodeId, crate::geom::V2)>) {
    ed.begin_coalescing("Resize", Some("resize-selection"));
    {
        let mut tx = ed.tx();
        for (id, size) in dims {
            let Some(old) = tx.doc().get(id).map(|n| n.rect) else {
                continue;
            };
            let new = crate::geom::Rect::new(old.x, old.y, size.x.max(0.0), size.y.max(0.0));
            if (new.w - old.w).abs() < 1e-9 && (new.h - old.h).abs() < 1e-9 {
                continue;
            }
            {
                let n = tx.node_mut(id).unwrap();
                n.set_rect_scaling(new);
                if (new.w - old.w).abs() > 1e-9 {
                    n.layout_child.sizing[0] = Sizing::Fixed;
                }
                if (new.h - old.h).abs() > 1e-9 {
                    n.layout_child.sizing[1] = Sizing::Fixed;
                }
            }
            crate::layout::resize_children(&mut tx, id, old, new);
        }
    }
    for id in ids {
        ed.fit_text(*id);
    }
    ed.mark_overrides(ids, "size");
    ed.commit();
}

fn child_layout(ui: &mut Ui, ed: &mut Editor, ids: &[NodeId], first: &Node) {
    let parent = first.parent.and_then(|p| ed.doc.get(p)).cloned();
    let in_auto = parent.as_ref().is_some_and(|p| p.layout.is_some());
    let sizing_opts_for = |axis: usize| -> Vec<(Sizing, &'static str)> {
        let mut v = vec![(Sizing::Fixed, "Fixed")];
        if first.layout.is_some() || matches!(first.kind, NodeKind::Text { .. }) {
            v.push((Sizing::Hug, "Hug"));
        }
        if in_auto && !first.layout_child.absolute {
            v.push((Sizing::Fill, "Fill"));
        }
        let _ = axis;
        v
    };
    let show_sizing = first.layout.is_some() || (in_auto && !first.layout_child.absolute);
    if show_sizing {
        ui.horizontal(|ui| {
            for (axis, label) in [(0usize, "W"), (1, "H")] {
                ui.label(RichText::new(label).color(theme::pal(ui).secondary));
                let opts = sizing_opts_for(axis);
                if let Some(v) = combo(
                    ui,
                    &format!("sizing{axis}"),
                    first.layout_child.sizing[axis],
                    &opts,
                    66.0,
                ) {
                    apply(ed, "layout", ids, |n| {
                        n.layout_child.sizing[axis] = v;
                        if let NodeKind::Text { auto_width, .. } = &mut n.kind
                            && axis == 0
                        {
                            *auto_width = v == Sizing::Hug;
                        }
                    });
                }
            }
        });
    }
    if in_auto {
        let mut abs = first.layout_child.absolute;
        if ui.checkbox(&mut abs, "Ignore auto layout").changed() {
            apply(ed, "layout", ids, |n| n.layout_child.absolute = abs);
        }
    }
    let constrained =
        parent.as_ref().is_some_and(|p| p.is_frame()) && (!in_auto || first.layout_child.absolute);
    if constrained {
        let opts_h = [
            (Constraint::Min, "Left"),
            (Constraint::Max, "Right"),
            (Constraint::Stretch, "Left & right"),
            (Constraint::Center, "Center"),
            (Constraint::Scale, "Scale"),
        ];
        let opts_v = [
            (Constraint::Min, "Top"),
            (Constraint::Max, "Bottom"),
            (Constraint::Stretch, "Top & bottom"),
            (Constraint::Center, "Center"),
            (Constraint::Scale, "Scale"),
        ];
        row(ui, "Constraints", |ui| {
            if let Some(v) = combo(ui, "con-h", first.layout_child.constraints.h, &opts_h, 74.0) {
                apply(ed, "layout", ids, |n| n.layout_child.constraints.h = v);
            }
            if let Some(v) = combo(ui, "con-v", first.layout_child.constraints.v, &opts_v, 74.0) {
                apply(ed, "layout", ids, |n| n.layout_child.constraints.v = v);
            }
        });
    }
}

fn text_target(n: &Node) -> bool {
    n.supports_label() || n.kind.text().is_some()
}

/// Text layers, shape labels and connector labels carry a text box. Sticky
/// notes do not expose padding, wrapping or a box background.
fn text_box_target(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Text { .. }) || n.supports_label()
}

/// Padding only applies where the text is laid out inside a box. A connector
/// label sits on the line, so its offsets come from the route instead.
fn text_pad_target(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Text { .. }) || (n.supports_label() && !n.is_connector())
}

fn text_size(n: &Node) -> Option<f64> {
    match &n.kind {
        NodeKind::Text { font_size, .. } | NodeKind::Sticky { font_size, .. } => Some(*font_size),
        _ if n.supports_label() => Some(n.label_size.unwrap_or(crate::text::LABEL_FONT_SIZE)),
        _ => None,
    }
}

fn text_color(n: &Node) -> Color {
    if matches!(n.kind, NodeKind::Text { .. }) {
        n.style
            .fills
            .last()
            .map_or(Color::INK, |p| p.preview_color())
    } else {
        crate::text::label_color(n)
    }
}

fn text_color_dark(n: &Node) -> Color {
    let light = text_color(n);
    let dark = if matches!(n.kind, NodeKind::Text { .. }) {
        n.style.fill_pair().and_then(|(_, d)| d)
    } else {
        n.label_color_dark
    };
    dark.unwrap_or(light)
}

fn text_align(n: &Node) -> TextAlign {
    match &n.kind {
        NodeKind::Text { align, .. } => *align,
        _ => n.text_style.align.unwrap_or(TextAlign::Center),
    }
}

fn text_valign(n: &Node) -> crate::model::VAlign {
    if matches!(n.kind, NodeKind::Text { .. }) {
        n.valign
    } else {
        n.label_valign.unwrap_or(crate::model::VAlign::Middle)
    }
}

fn text_wrap(n: &Node) -> bool {
    match &n.kind {
        NodeKind::Text { auto_width, .. } => !*auto_width,
        _ => n.label_wrap,
    }
}

fn span_all(n: &Node, pick: fn(&crate::model::TextSpan) -> Option<bool>) -> bool {
    let Some(text) = n.edited_text() else {
        return false;
    };
    let len = text.chars().count();
    if len == 0 {
        return false;
    }
    let mut hit = vec![false; len];
    for sp in &n.text_style.spans {
        if pick(sp) == Some(true) {
            for h in hit.iter_mut().take(sp.end.min(len)).skip(sp.start.min(len)) {
                *h = true;
            }
        }
    }
    hit.iter().all(|x| *x)
}

fn set_text_wrap(ed: &mut Editor, ids: &[NodeId], wrap: bool) {
    ed.begin("Word wrap");
    {
        let mut tx = ed.tx();
        for id in ids {
            let Some(n) = tx.node_mut(*id) else { continue };
            if let NodeKind::Text { auto_width, .. } = &mut n.kind {
                *auto_width = !wrap;
            } else if n.supports_label() {
                n.label_wrap = wrap;
            }
        }
    }
    for id in ids {
        ed.fit_text(*id);
    }
    ed.commit();
}

fn set_text_color(ed: &mut Editor, ids: &[NodeId], c: Color) {
    if ed.text_selection().is_some() {
        ed.style_text("color", |s| s.color = Some(c));
        return;
    }
    apply(ed, "font color", ids, |n| {
        if matches!(n.kind, NodeKind::Text { .. }) {
            n.style.fills = vec![Paint::solid(c)];
        } else if n.supports_label() || n.kind.text().is_some() {
            n.label_color = Some(c);
        }
    });
}

fn set_text_color_dark(ed: &mut Editor, _ids: &[NodeId], c: Color) {
    ed.style_text("dark font color", |s| s.color_dark = Some(c));
}

fn text_bg_dark_color(n: &Node) -> Color {
    n.text_bg_dark
        .unwrap_or_else(|| n.text_bg.unwrap_or(Color::WHITE))
}

fn text_border_on(n: &Node) -> bool {
    if n.supports_label() {
        n.text_border.is_some()
    } else {
        n.style.stroke.is_some()
    }
}

fn text_border_color(n: &Node) -> Color {
    if n.supports_label() {
        n.text_border.or(n.text_border_saved).unwrap_or(Color::INK)
    } else {
        n.style.stroke.unwrap_or(Color::INK)
    }
}

fn text_border_dark_color(n: &Node) -> Color {
    if n.supports_label() {
        n.text_border_dark.or(n.text_border).unwrap_or(Color::INK)
    } else {
        n.style.stroke_dark.unwrap_or_else(|| text_border_color(n))
    }
}

fn set_text_border(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "text border", ids, |n| {
        if n.supports_label() {
            n.text_border = Some(c);
        } else if text_target(n) {
            n.style.stroke = Some(c);
            n.style.stroke_enabled = true;
            if n.style.stroke_width <= 0.0 {
                n.style.stroke_width = 1.0;
            }
        }
    });
}

fn set_text_bg_dark(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "dark text background", ids, |n| {
        if text_box_target(n) {
            n.text_bg.get_or_insert(Color::WHITE);
            n.text_bg_dark = Some(c);
        }
    });
}

fn set_text_border_dark(ed: &mut Editor, ids: &[NodeId], c: Color) {
    apply(ed, "dark text border", ids, |n| {
        if n.supports_label() {
            n.text_border.get_or_insert(Color::INK);
            n.text_border_dark = Some(c);
        } else if text_target(n) {
            n.style.stroke.get_or_insert(Color::INK);
            n.style.stroke_dark = Some(c);
            n.style.stroke_enabled = true;
            if n.style.stroke_width <= 0.0 {
                n.style.stroke_width = 1.0;
            }
        }
    });
}

fn text_opacity(n: &Node) -> f64 {
    if n.kind.text().is_some() {
        n.style.opacity
    } else {
        n.label_opacity
    }
}

/// The Text tab: font, emphasis, alignment, writing direction, colors, the
/// text box and spacing. Applies to text layers, sticky notes and shape labels.
/// The Data tab: custom links, tooltip, tags and shape properties.
fn data_tab(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
) {
    if let Some(_guard) = section_begin(
        ui,
        st,
        "data.link",
        "Link",
        &["Open Link", "Clear", "Internal page link", "External link"],
    ) {
        let mut link = first.link.clone();
        let r = ui.add_sized(
            [vis, 22.0],
            egui::TextEdit::singleline(&mut link)
                .hint_text("https://… or page:Name")
                .desired_width(vis),
        );
        if r.changed() {
            apply(ed, "link", ids, |n| n.link = link.clone());
        }
        seal_on_release(ed, &r);
        ui.horizontal(|ui| {
            let has = !first.link.trim().is_empty();
            if ui
                .add_enabled(has, egui::Button::new("Open Link"))
                .on_hover_text("Follow the link (asks before opening an external target)")
                .clicked()
            {
                ed.activate_link(&first.link);
            }
            if ui
                .add_enabled(has, egui::Button::new("Clear"))
                .on_hover_text("Remove the link")
                .clicked()
            {
                apply(ed, "link", ids, |n| n.link.clear());
            }
        });
        if !first.link.trim().is_empty() {
            let desc = match crate::link::classify(&first.link) {
                crate::link::LinkTarget::Page(_) => "Internal page link.".to_string(),
                crate::link::LinkTarget::External(_) => {
                    "External link; Sketch asks before opening it.".to_string()
                }
                crate::link::LinkTarget::Actions(_) => {
                    "Custom action link. Runs layer and visibility actions.".to_string()
                }
                crate::link::LinkTarget::Unsafe(reason) => format!("Blocked: {reason}."),
            };
            ui.label(
                RichText::new(desc)
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
        }
    }
    separator(ui);

    if let Some(_guard) = section_begin(ui, st, "data.tooltip", "Tooltip", &["Tooltip"]) {
        let mut tooltip = first.tooltip.clone();
        let r = ui.add_sized(
            [vis, 22.0],
            egui::TextEdit::singleline(&mut tooltip)
                .hint_text("Text shown on hover")
                .desired_width(vis),
        );
        if r.changed() {
            apply(ed, "tooltip", ids, |n| n.tooltip = tooltip.clone());
        }
        seal_on_release(ed, &r);
    }
    separator(ui);

    if let Some(_guard) = section_begin(ui, st, "data.tags", "Tags", &["Tags"]) {
        let mut tags = first.tags.join(", ");
        let r = ui.add_sized(
            [vis, 22.0],
            egui::TextEdit::singleline(&mut tags)
                .hint_text("Comma separated")
                .desired_width(vis),
        );
        if r.changed() {
            let parsed = parse_tags(&tags);
            apply(ed, "tags", ids, |n| n.tags = parsed.clone());
        }
        seal_on_release(ed, &r);
    }
    separator(ui);

    if let Some(_guard) = section_begin(
        ui,
        st,
        "data.properties",
        "Properties",
        &["Properties", "Edit Data…", "No custom properties."],
    ) {
        if first.metadata.is_empty() {
            ui.label(
                RichText::new("No custom properties.")
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
        } else {
            for (key, value) in &first.metadata {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(key)
                            .size(theme::SECONDARY)
                            .color(theme::pal(ui).secondary),
                    );
                    ui.label(RichText::new(value).size(theme::SECONDARY));
                });
            }
        }
        if ui
            .add_sized([vis, 20.0], egui::Button::new("Edit Data…"))
            .on_hover_text("Add, change or remove shape properties")
            .clicked()
        {
            st.metadata_edit = Some(MetadataEdit {
                rows: first
                    .metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                error: None,
                target: MetadataTarget::Nodes(ids.to_vec()),
            });
        }
        if ids.len() > 1 {
            ui.label(
                RichText::new(format!("Edits apply to all {} selected shapes.", ids.len()))
                    .size(theme::SECONDARY)
                    .color(theme::pal(ui).secondary),
            );
        }
    }
}

/// Split a comma-separated tag field, trimming blanks and dropping duplicates.
pub fn parse_tags(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tag in text.split(',') {
        let tag = tag.trim();
        if !tag.is_empty() && !out.iter().any(|t| t == tag) {
            out.push(tag.to_string());
        }
    }
    out
}

fn text_tab(
    ui: &mut Ui,
    ed: &mut Editor,
    st: &mut UiState,
    vis: f32,
    ids: &[NodeId],
    first: &Node,
) {
    let Some(_guard) = section_begin(
        ui,
        st,
        "text.format",
        "Text",
        &[
            "Add label",
            "Wrap",
            "Word wrap",
            "Font",
            "Font Color",
            "Position",
            "Direction",
            "Background",
            "Border",
            "Shadow",
            "Formatted text",
            "Bulleted list",
            "Numbered list",
            "Indent",
            "Outdent",
            "Opacity",
            "Clear formatting",
        ],
    ) else {
        return;
    };
    let targets: Vec<Node> = ids
        .iter()
        .filter_map(|id| ed.doc.get(*id))
        .filter(|n| text_target(n))
        .cloned()
        .collect();
    if targets.is_empty() {
        ui.label(
            RichText::new("The selection has no editable text.")
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
        );
        return;
    }
    let editing = ed.text_edit.is_some();
    let cs = ed.current_char_style();
    ui.horizontal(|ui| {
        if editing {
            ui.label(
                RichText::new(if ed.text_selection().is_some() {
                    "selection"
                } else {
                    "caret"
                })
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
            );
        }
    });

    // draw.io lets one connector carry several labels. Extra labels share the
    // edge's label style; each is dragged and edited on the canvas, and
    // clearing its text removes it again.
    if ids.len() == 1
        && ed
            .doc
            .get(ids[0])
            .is_some_and(|n| n.is_connector() && !ed.doc.is_effectively_locked(ids[0]))
    {
        let id = ids[0];
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new(
                    RichText::new("Add label").size(theme::BODY),
                ))
                .clicked()
            {
                ed.add_connector_label(id);
            }
            let count = ed.doc.get(id).map_or(0, |n| n.label_count());
            ui.label(
                RichText::new(format!(
                    "{count} label{}",
                    if count == 1 { "" } else { "s" }
                ))
                .size(theme::SECONDARY)
                .color(theme::pal(ui).secondary),
            );
        });
        ui.add_space(2.0);
    }

    let wrap_caps: Vec<&Node> = targets.iter().filter(|n| text_box_target(n)).collect();
    if !wrap_caps.is_empty() {
        let wraps: Vec<bool> = wrap_caps.iter().map(|n| text_wrap(n)).collect();
        row(ui, "Wrap", |ui| {
            if let Some(on) = mixed_checkbox(ui, "Word wrap", &wraps) {
                set_text_wrap(ed, ids, on);
            }
        });
    }

    // Font family. "Inter" is the built-in face and serializes as None.
    let families = crate::text::font_families();
    let family_of = |n: &Node| {
        n.text_style
            .family
            .clone()
            .unwrap_or_else(|| "Inter".to_string())
    };
    let current_family = family_of(&targets[0]);
    let family_index = families
        .iter()
        .position(|f| f.eq_ignore_ascii_case(&current_family))
        .unwrap_or(0);
    let family_options: Vec<(usize, &str)> = families
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.as_str()))
        .collect();
    let family_names: Vec<String> = targets.iter().map(family_of).collect();
    let family_mixed = !all_equal(&family_names);
    row(ui, "Font", |ui| {
        let width = (vis - LABEL_W - 12.0).max(80.0);
        if let Some(i) = combo(ui, "font-family", family_index, &family_options, width) {
            let name = families[i].clone();
            apply(ed, "font family", ids, |n| {
                if text_target(n) {
                    n.text_style.family =
                        (!name.eq_ignore_ascii_case("Inter")).then(|| name.clone());
                }
            });
        }
        if family_mixed {
            mixed_badge(ui);
        }
    });

    let char_styles: Vec<crate::model::CharStyle> = targets
        .iter()
        .map(|n| {
            n.text_style
                .char_style(0, text_size(n).unwrap_or(16.0), text_color(n))
        })
        .collect();
    let state = cs.or_else(|| char_styles.first().copied());
    let sizes: Vec<f64> = targets.iter().filter_map(text_size).collect();
    let size_mixed = !all_equal(&sizes);
    let underline_states: Vec<bool> = targets
        .iter()
        .map(|n| span_all(n, |s| s.underline))
        .collect();
    let strike_states: Vec<bool> = targets.iter().map(|n| span_all(n, |s| s.strike)).collect();
    let bold = state.is_some_and(|c| c.bold);
    let italic = state.is_some_and(|c| c.italic);
    let underline = cs.map_or_else(
        || !underline_states.is_empty() && underline_states.iter().all(|x| *x),
        |c| c.underline,
    );
    let strike = cs.map_or_else(
        || !strike_states.is_empty() && strike_states.iter().all(|x| *x),
        |c| c.strike,
    );
    let dir_caps: Vec<&Node> = targets
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Text { .. }) || n.supports_label())
        .collect();
    let vertical = !dir_caps.is_empty()
        && dir_caps
            .iter()
            .all(|n| n.dir == crate::model::WritingDir::Vertical);
    ui.horizontal(|ui| {
        if let Some(v) = num_field(
            ui,
            ed,
            "",
            state.map_or(16.0, |c| c.size),
            0.5,
            1.0..=1000.0,
            54.0,
        ) {
            ed.style_text("font size", |s| s.size = Some(v));
        }
        if size_mixed {
            mixed_badge(ui);
        }
        ui.add_space(4.0);
        if icon_toggle(ui, "B", bold, "Bold (⌘B)") {
            if cs.is_some() {
                ed.toggle_bold();
            } else {
                ed.style_text("bold", |s| s.bold = Some(!bold));
            }
        }
        if icon_toggle(ui, "I", italic, "Italic (⌘I)") {
            if cs.is_some() {
                ed.toggle_italic();
            } else {
                ed.style_text("italic", |s| s.italic = Some(!italic));
            }
        }
        if icon_toggle(ui, "U", underline, "Underline (⌘U)") {
            if cs.is_some() {
                ed.toggle_underline();
            } else {
                ed.style_text("font", |s| s.underline = Some(!underline));
            }
        }
        if icon_toggle(ui, "S", strike, "Strikethrough (⌘⇧X)") {
            if cs.is_some() {
                ed.toggle_strike();
            } else {
                ed.style_text("font", |s| s.strike = Some(!strike));
            }
        }
        let script = state.map_or(crate::model::TextScript::Normal, |c| c.script);
        if icon_toggle(ui, "x²", script.is_super(), "Superscript") {
            ed.toggle_superscript();
        }
        if icon_toggle(ui, "x₂", script.is_sub(), "Subscript") {
            ed.toggle_subscript();
        }
        if !dir_caps.is_empty() && icon_toggle(ui, "V", vertical, "Vertical writing direction") {
            let v = if vertical {
                crate::model::WritingDir::Auto
            } else {
                crate::model::WritingDir::Vertical
            };
            apply(ed, "writing direction", ids, |n| n.dir = v);
        }
        let curved = !dir_caps.is_empty() && dir_caps.iter().all(|n| n.text_style.curve.is_some());
        if !dir_caps.is_empty() && icon_toggle(ui, "ARC", curved, "Text on a curve") {
            let curve = if curved { None } else { Some(0.6) };
            apply(ed, "text curve", ids, |n| n.text_style.curve = curve);
        }
    });

    let active_para = ed.active_paragraph(first.id);
    ui.label(
        RichText::new(
            "List buttons apply to all paragraphs. Indent and Outdent affect the active paragraph.",
        )
        .size(theme::SECONDARY)
        .color(theme::pal(ui).secondary),
    );
    let list_state = first
        .text_style
        .paragraphs
        .get(active_para)
        .copied()
        .unwrap_or_default();
    ui.horizontal(|ui| {
        if icon_toggle(
            ui,
            "•",
            list_state.list == crate::model::ListKind::Bullet,
            "Bulleted list",
        ) {
            ed.set_list_paragraphs(crate::model::ListKind::Bullet);
        }
        if icon_toggle(
            ui,
            "1.",
            list_state.list == crate::model::ListKind::Number,
            "Numbered list",
        ) {
            ed.set_list_paragraphs(crate::model::ListKind::Number);
        }
        if ui
            .button("Indent")
            .on_hover_text("Indent this paragraph")
            .clicked()
        {
            ed.set_paragraph_indent(active_para, 1);
        }
        if ui
            .button("Outdent")
            .on_hover_text("Outdent this paragraph")
            .clicked()
        {
            ed.set_paragraph_indent(active_para, -1);
        }
    });

    let align_caps: Vec<&Node> = targets
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Text { .. }) || n.supports_label())
        .collect();
    if !align_caps.is_empty() {
        let aligns: Vec<TextAlign> = align_caps.iter().map(|n| text_align(n)).collect();
        ui.horizontal(|ui| {
            for (a, ic, tip) in [
                (TextAlign::Left, theme::Icon::AlignLeft, "Align left"),
                (TextAlign::Center, theme::Icon::AlignHCenter, "Align center"),
                (TextAlign::Right, theme::Icon::AlignRight, "Align right"),
                (TextAlign::Justify, theme::Icon::AlignJustify, "Justify"),
            ] {
                let active = aligns.iter().all(|x| *x == a);
                if icon_btn(ui, ic, active, tip) {
                    apply(ed, "text align", ids, |n| {
                        if let NodeKind::Text { align, .. } = &mut n.kind {
                            *align = a;
                        } else if n.supports_label() {
                            n.text_style.align = Some(a);
                        }
                    });
                }
            }
            if !all_equal(&aligns) {
                mixed_badge(ui);
            }
        });
    }

    let val_caps: Vec<&Node> = targets
        .iter()
        .filter(|n| {
            !n.is_connector() && (matches!(n.kind, NodeKind::Text { .. }) || n.supports_label())
        })
        .collect();
    if !val_caps.is_empty() {
        let vals: Vec<crate::model::VAlign> = val_caps.iter().map(|n| text_valign(n)).collect();
        let current = vals
            .first()
            .copied()
            .unwrap_or(crate::model::VAlign::Middle);
        let options = [
            (crate::model::VAlign::Top, "Top"),
            (crate::model::VAlign::Middle, "Center"),
            (crate::model::VAlign::Bottom, "Bottom"),
        ];
        row(ui, "Position", |ui| {
            if let Some(v) = combo(ui, "text-position", current, &options, 88.0) {
                apply(ed, "vertical text position", ids, |n| {
                    if matches!(n.kind, NodeKind::Text { .. }) {
                        n.valign = v;
                    } else if n.supports_label() && !n.is_connector() {
                        n.label_valign = Some(v);
                    }
                });
            }
            if !all_equal(&vals) {
                mixed_badge(ui);
            }
        });
    }

    if !dir_caps.is_empty() {
        let dirs: Vec<crate::model::WritingDir> = dir_caps.iter().map(|n| n.dir).collect();
        let current = dirs
            .first()
            .copied()
            .unwrap_or(crate::model::WritingDir::Auto);
        let options = [
            (crate::model::WritingDir::Auto, "Automatic"),
            (crate::model::WritingDir::Ltr, "Left to right"),
            (crate::model::WritingDir::Rtl, "Right to left"),
            (crate::model::WritingDir::Vertical, "Vertical"),
        ];
        row(ui, "Direction", |ui| {
            if let Some(v) = combo(ui, "writing-direction", current, &options, 100.0) {
                apply(ed, "writing direction", ids, |n| n.dir = v);
            }
            if !all_equal(&dirs) {
                mixed_badge(ui);
            }
        });
    }

    let colors: Vec<Color> = targets.iter().map(text_color).collect();
    let color_mixed = !all_equal(&colors);
    let labels_only = targets
        .iter()
        .all(|n| !matches!(n.kind, NodeKind::Text { .. }));
    row(ui, "Font Color", |ui| {
        let mut enabled = true;
        if labels_only {
            let states: Vec<bool> = targets.iter().map(|n| n.label_color.is_some()).collect();
            if let Some(on) = mixed_checkbox(ui, "", &states) {
                apply(ed, "label color", ids, |n| {
                    if n.supports_label() {
                        if on {
                            let inherited = if n.is_connector() {
                                n.style.stroke.unwrap_or(Color::INK)
                            } else {
                                Color::INK
                            };
                            n.label_color = n.label_color_saved.or(Some(inherited));
                        } else if n.label_color.is_some() {
                            n.label_color_saved = n.label_color;
                            n.label_color = None;
                        }
                    }
                });
            }
            enabled = targets.iter().any(|n| n.label_color.is_some());
        }
        let c = colors.first().copied().unwrap_or(Color::INK);
        if let Some(nc) = ui
            .add_enabled_ui(enabled, |ui| color_button(ui, ed, c))
            .inner
        {
            set_text_color(ed, ids, nc);
        }
        let dark = targets.first().map_or(c, text_color_dark);
        if let Some(nc) = ui
            .add_enabled_ui(enabled, |ui| dark_color_button(ui, ed, dark, "font color"))
            .inner
        {
            set_text_color_dark(ed, ids, nc);
        }
        if color_mixed {
            mixed_badge(ui);
        }
    });

    let bg_caps: Vec<&Node> = targets.iter().filter(|n| text_box_target(n)).collect();
    if !bg_caps.is_empty() {
        let backgrounds: Vec<bool> = bg_caps.iter().map(|n| n.text_bg.is_some()).collect();
        row(ui, "Background", |ui| {
            if let Some(on) = mixed_checkbox(ui, "", &backgrounds) {
                apply(ed, "text background", ids, |n| {
                    if !text_box_target(n) {
                        return;
                    }
                    if on {
                        n.text_bg = n.text_bg_saved.or(Some(Color::WHITE));
                    } else if n.text_bg.is_some() {
                        n.text_bg_saved = n.text_bg;
                        n.text_bg = None;
                    }
                });
            }
            let enabled = bg_caps.iter().any(|n| n.text_bg.is_some());
            let c = bg_caps
                .iter()
                .find_map(|n| n.text_bg)
                .unwrap_or(Color::WHITE);
            if let Some(nc) = ui
                .add_enabled_ui(enabled, |ui| color_button(ui, ed, c))
                .inner
            {
                apply(ed, "text background", ids, |n| {
                    if text_box_target(n) {
                        n.text_bg = Some(nc);
                    }
                });
            }
            let dark = bg_caps.first().map_or(c, |n| text_bg_dark_color(n));
            if let Some(nc) = ui
                .add_enabled_ui(enabled, |ui| {
                    dark_color_button(ui, ed, dark, "text background")
                })
                .inner
            {
                set_text_bg_dark(ed, ids, nc);
            }
        });
    }

    let borders: Vec<bool> = targets.iter().map(text_border_on).collect();
    row(ui, "Border", |ui| {
        if let Some(on) = mixed_checkbox(ui, "", &borders) {
            apply(ed, "text border", ids, |n| {
                if !text_target(n) {
                    return;
                }
                if on {
                    let c = n.text_border_saved.or(Some(Color::INK));
                    if n.supports_label() {
                        n.text_border = c;
                    } else {
                        n.style.stroke = c;
                        n.style.stroke_enabled = true;
                        if n.style.stroke_width <= 0.0 {
                            n.style.stroke_width = 1.0;
                        }
                    }
                } else if n.supports_label() {
                    if n.text_border.is_some() {
                        n.text_border_saved = n.text_border;
                        n.text_border = None;
                    }
                } else if n.style.stroke.is_some() {
                    n.text_border_saved = n.style.stroke;
                    n.style.stroke = None;
                }
            });
        }
        let enabled = targets.iter().any(text_border_on);
        let c = targets.first().map_or(Color::INK, text_border_color);
        if let Some(nc) = ui
            .add_enabled_ui(enabled, |ui| color_button(ui, ed, c))
            .inner
        {
            set_text_border(ed, ids, nc);
        }
        let dark = targets.first().map_or(c, text_border_dark_color);
        if let Some(nc) = ui
            .add_enabled_ui(enabled, |ui| dark_color_button(ui, ed, dark, "text border"))
            .inner
        {
            set_text_border_dark(ed, ids, nc);
        }
    });

    let shadows: Vec<bool> = targets.iter().map(|n| n.text_shadow).collect();
    if let Some(on) = mixed_checkbox(ui, "Shadow", &shadows) {
        apply(ed, "text shadow", ids, |n| n.text_shadow = on);
    }
    let formats: Vec<bool> = targets.iter().map(|n| n.text_formatted).collect();
    if let Some(on) = mixed_checkbox(ui, "Formatted text", &formats) {
        apply(ed, "formatted text", ids, |n| n.text_formatted = on);
    }
    let ops: Vec<f64> = targets
        .iter()
        .map(|n| (text_opacity(n) * 100.0).round())
        .collect();
    row(ui, "Opacity", |ui| {
        if let Some(v) = num_field(
            ui,
            ed,
            "",
            (text_opacity(&targets[0]) * 100.0).round(),
            1.0,
            0.0..=100.0,
            64.0,
        ) {
            let v = (v / 100.0).clamp(0.0, 1.0);
            apply(ed, "text opacity", ids, |n| {
                if n.kind.text().is_some() {
                    n.style.opacity = v;
                } else if n.supports_label() {
                    n.label_opacity = v;
                }
            });
        }
        if !all_equal(&ops) {
            mixed_badge(ui);
        }
    });
    if editing {
        if ui.button("Clear formatting").clicked() {
            ed.clear_text_formatting();
        }
    } else if ui
        .button("Clear formatting")
        .on_hover_text("Drop character overrides and reset label styling")
        .clicked()
    {
        ed.clear_text_formatting();
    }

    let pad_caps: Vec<&Node> = targets.iter().filter(|n| text_pad_target(n)).collect();
    if !pad_caps.is_empty() {
        separator(ui);
        ui.label(title_text("Spacing"));
        let three = (vis - 8.0) / 3.0;
        let pads: Vec<crate::model::Pad> = pad_caps.iter().map(|n| n.pad).collect();
        let pad_mixed = !all_equal(&pads);
        let p = pad_caps[0].pad;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let all = p.top.max(p.bottom.max(p.left.max(p.right)));
            if let Some(v) = num_field(ui, ed, "All  ", all, 1.0, 0.0..=1e4, three) {
                apply(ed, "spacing", ids, |n| {
                    if text_pad_target(n) {
                        n.pad = crate::model::Pad {
                            left: v,
                            top: v,
                            right: v,
                            bottom: v,
                        };
                    }
                });
            }
            if let Some(v) = num_field(ui, ed, "Top  ", p.top, 1.0, 0.0..=1e4, three) {
                apply(ed, "spacing top", ids, |n| {
                    if text_pad_target(n) {
                        n.pad.top = v;
                    }
                });
            }
            if pad_mixed {
                mixed_badge(ui);
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (label, side) in [("Left ", 0usize), ("Bottom ", 1), ("Right ", 2)] {
                let value = match side {
                    0 => p.left,
                    1 => p.bottom,
                    _ => p.right,
                };
                if let Some(v) = num_field(ui, ed, label, value, 1.0, 0.0..=1e4, three) {
                    apply(ed, "spacing", ids, |n| {
                        if !text_pad_target(n) {
                            return;
                        }
                        match side {
                            0 => n.pad.left = v,
                            1 => n.pad.bottom = v,
                            _ => n.pad.right = v,
                        }
                    });
                }
            }
        });
    }

    let body = targets.first().and_then(|n| match &n.kind {
        NodeKind::Text { text, .. } => Some((text.clone(), false)),
        _ if n.supports_label() => Some((n.label.clone(), true)),
        _ => None,
    });
    if let Some((mut t, label_mode)) = body
        && ed.text_edit.is_none()
    {
        let r = ui.add(
            egui::TextEdit::multiline(&mut t)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        );
        if r.changed() {
            let len = t.chars().count();
            apply(ed, "text", ids, |n| {
                if label_mode {
                    if n.supports_label() {
                        n.label = t.clone();
                    }
                } else if let NodeKind::Text { text, .. } = &mut n.kind {
                    *text = t.clone();
                }
                n.text_style.spans.retain(|s| s.end <= len);
            });
        }
        seal_on_release(ed, &r);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AdaptiveColors, Color, Node, NodeId, NodeKind, Paint, STICKY_COLORS, SectionGuard, apply,
        filter_matches, normalize_query, panel, parse_tags, replace_row_filter, row_visible,
        section_open, set_section_open, swatches,
    };
    use crate::editor::Editor;
    use crate::ui::UiState;

    #[test]
    fn tags_split_on_commas_trim_and_drop_duplicates() {
        assert_eq!(
            parse_tags(" web, external ,,web ,  "),
            vec!["web".to_string(), "external".to_string()]
        );
        assert!(parse_tags("   ").is_empty());
    }

    #[test]
    fn sticky_swatches_expose_color_names_to_accesskit() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200.0, 60.0),
                )),
                ..Default::default()
            },
            |ui| {
                swatches(ui, STICKY_COLORS[0]);
            },
        );
        out.textures_delta.clear();
        let update = out
            .platform_output
            .accesskit_update
            .expect("swatches should register accessibility nodes");
        let labels: Vec<String> = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().map(str::to_string))
            .collect();
        for color in STICKY_COLORS {
            let hex = color.to_hex();
            assert!(
                labels.iter().any(|label| label == &hex),
                "expected a {hex} label, got {labels:?}"
            );
        }
    }

    #[test]
    fn section_states_default_to_expanded_and_mark_options_dirty() {
        let mut st = UiState::default();
        assert!(section_open(&st, "style.fill"));

        set_section_open(&mut st, "style.fill", false);
        assert!(!section_open(&st, "style.fill"));
        assert_eq!(
            st.options.inspector_sections.get("style.fill"),
            Some(&false)
        );
        assert!(st.options_dirty);

        set_section_open(&mut st, "style.fill", true);
        assert!(section_open(&st, "style.fill"));
        assert_eq!(st.options.inspector_sections.get("style.fill"), Some(&true));
    }

    #[test]
    fn filter_matches_titles_and_labels_case_insensitively() {
        let labels = ["Font", "Opacity"];
        assert!(filter_matches("", "Text", &labels));
        assert!(filter_matches("TEXT", "Text", &labels));
        assert!(filter_matches(" font ", "Text", &labels));
        assert!(!filter_matches("zoom", "Text", &labels));
        assert_eq!(normalize_query("  Fill Mode "), "fill mode");
    }

    #[test]
    fn row_filter_restores_when_a_section_guard_drops() {
        assert!(row_visible("Opacity"));
        let outer = replace_row_filter(Some("font".into()));
        assert!(!row_visible("Opacity"));
        assert!(row_visible("Font"));
        {
            let _guard = SectionGuard {
                previous: replace_row_filter(None),
            };
            assert!(row_visible("Opacity"));
        }
        assert!(!row_visible("Opacity"));
        replace_row_filter(outer);
        assert!(row_visible("Opacity"));
    }

    fn panel_text(query: &str, collapsed: bool) -> String {
        fn collect(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push_str(&text.galley.job.text);
                    out.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, out);
                    }
                }
                _ => {}
            }
        }

        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = UiState {
            inspector_filter: query.to_string(),
            ..Default::default()
        };
        if collapsed {
            set_section_open(&mut st, "diagram.view", false);
            st.options_dirty = false;
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(ui, &mut ed, &mut st));
        out.textures_delta.clear();
        let mut text = String::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut text);
        }
        text
    }

    fn tab_panel_text(
        tab: crate::ui::InspectorTab,
        query: &str,
        selected: Option<NodeId>,
    ) -> String {
        tab_panel_text_filtered(tab, query, selected, "")
    }

    fn tab_panel_text_filtered(
        tab: crate::ui::InspectorTab,
        query: &str,
        selected: Option<NodeId>,
        style_filter: &str,
    ) -> String {
        fn collect(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push_str(&text.galley.job.text);
                    out.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, out);
                    }
                }
                _ => {}
            }
        }

        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        if let Some(id) = selected {
            let id = ed.add_node(
                Node::new(
                    id,
                    NodeKind::Rect,
                    crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
                ),
                None,
            );
            ed.select(vec![id]);
        }
        let mut st = UiState {
            inspector_tab: tab,
            inspector_filter: query.to_string(),
            style_property_filter: style_filter.to_string(),
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 1600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(ui, &mut ed, &mut st));
        out.textures_delta.clear();
        let mut text = String::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut text);
        }
        text
    }

    fn selected_connector_panel_text() -> String {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let a = ed.add_node(
            crate::model::Node::new(
                crate::model::NodeId(1),
                crate::model::NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            crate::model::Node::new(
                crate::model::NodeId(2),
                crate::model::NodeKind::Rect,
                crate::geom::Rect::new(300.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let id = ed.add_node(
            crate::model::Node::new(
                crate::model::NodeId(3),
                crate::model::NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![id];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 1600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(ui, &mut ed, &mut st));
        out.textures_delta.clear();
        let mut text = String::new();
        for clipped in &out.shapes {
            fn collect(shape: &egui::Shape, out: &mut String) {
                match shape {
                    egui::Shape::Text(text) => {
                        out.push_str(&text.galley.job.text);
                        out.push('\n');
                    }
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            collect(shape, out);
                        }
                    }
                    _ => {}
                }
            }
            collect(&clipped.shape, &mut text);
        }
        text
    }

    #[test]
    fn arrow_section_exposes_endpoint_modes() {
        let text = selected_connector_panel_text();
        assert!(text.contains("Arrow"), "{text}");
        assert!(text.contains("Start"), "{text}");
        assert!(text.contains("End"), "{text}");
        assert!(text.contains("Floating"), "{text}");
        assert!(text.contains("Fixed"), "{text}");
    }

    #[test]
    fn arrow_section_exposes_head_size_and_offset() {
        let text = selected_connector_panel_text();
        assert!(text.contains("Head Size"), "{text}");
        assert!(text.contains("Head Offset"), "{text}");
    }

    #[test]
    fn diagram_tab_exposes_adaptive_color_controls() {
        let text = panel_text("", false);
        assert!(text.contains("Adaptive Colors"), "{text}");
        assert!(text.contains("Dark background"), "{text}");
        assert!(text.contains("Automatic"), "{text}");
    }

    #[test]
    fn diagram_tab_exposes_page_transition_controls() {
        let text = panel_text("Transition", false);
        assert!(text.contains("Transition"), "{text}");
        assert!(text.contains("Fade on page switch"), "{text}");
    }

    #[test]
    fn background_dialog_exposes_image_controls() {
        fn collect(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push_str(&text.galley.job.text);
                    out.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, out);
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = UiState {
            background_open: true,
            ..Default::default()
        };
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_ui(input.clone(), |ui| {
            super::dialogs(ui.ctx(), &mut ed, &mut st)
        });
        first.textures_delta.clear();
        let mut out = ctx.run_ui(input, |ui| super::dialogs(ui.ctx(), &mut ed, &mut st));
        out.textures_delta.clear();
        let mut text = String::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut text);
        }
        assert!(text.contains("Background image"), "{text}");
        assert!(text.contains("Choose image…"), "{text}");
    }

    #[test]
    fn global_style_tab_exposes_scheme_scope_and_preview() {
        let text = tab_panel_text(crate::ui::InspectorTab::Style, "", None);
        assert!(text.contains("Scope"), "{text}");
        assert!(text.contains("Page"), "{text}");
        assert!(text.contains("Selection"), "{text}");
        assert!(text.contains("0 changed, 0 skipped"), "{text}");
    }

    #[test]
    fn selected_style_tab_exposes_scheme_scope() {
        let text = tab_panel_text(crate::ui::InspectorTab::Style, "", Some(NodeId(9)));
        assert!(text.contains("Global style"), "{text}");
        assert!(text.contains("Scope"), "{text}");
        assert!(text.contains("Selection"), "{text}");
        assert!(text.contains("1 changed, 0 skipped"), "{text}");
    }

    #[test]
    fn text_tab_exposes_list_and_indent_controls() {
        let text = tab_panel_text(crate::ui::InspectorTab::Text, "", Some(NodeId(9)));
        assert!(text.contains('•'), "{text}");
        assert!(text.contains("1."), "{text}");
        assert!(text.contains("Indent"), "{text}");
        assert!(text.contains("Outdent"), "{text}");
    }

    #[test]
    fn text_tab_exposes_curve_toggle() {
        let text = tab_panel_text(crate::ui::InspectorTab::Text, "", Some(NodeId(9)));
        assert!(text.contains("ARC"), "{text}");
    }

    #[test]
    fn text_tab_help_lists_applies_to_all_paragraphs() {
        let text = tab_panel_text(crate::ui::InspectorTab::Text, "", Some(NodeId(9)));
        assert!(
            text.contains("List buttons apply to all paragraphs"),
            "{text}"
        );
        assert!(
            text.contains("Indent and Outdent affect the active paragraph"),
            "{text}"
        );
    }

    #[test]
    fn head_size_and_offset_edits_join_undo_and_redo() {
        let mut ed = Editor::default();
        let id = ed.add_node(
            crate::model::Node::new(
                crate::model::NodeId(1),
                crate::model::NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 0.0), crate::geom::v2(200.0, 0.0)],
                    start: None,
                    end: None,
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 200.0, 0.0),
            ),
            None,
        );
        let ids = [id];
        super::apply(&mut ed, "start head size", &ids, |n| {
            n.start_head_size = 12.0
        });
        super::apply(&mut ed, "end head offset", &ids, |n| {
            n.end_head_offset = -6.0
        });
        assert_eq!(ed.doc.get(id).unwrap().start_head_size, 12.0);
        assert_eq!(ed.doc.get(id).unwrap().end_head_offset, -6.0);

        ed.undo();
        assert_eq!(ed.doc.get(id).unwrap().start_head_size, 12.0);
        assert_eq!(ed.doc.get(id).unwrap().end_head_offset, 0.0);
        ed.undo();
        assert_eq!(
            ed.doc.get(id).unwrap().start_head_size,
            crate::model::DEFAULT_HEAD_SIZE
        );
        ed.redo();
        assert_eq!(ed.doc.get(id).unwrap().start_head_size, 12.0);
        ed.redo();
        assert_eq!(ed.doc.get(id).unwrap().end_head_offset, -6.0);
    }

    #[test]
    fn the_panel_renders_a_filter_field_and_filters_sections() {
        let all = panel_text("", false);
        assert!(all.contains("Filter format properties"));
        assert!(all.contains("View"));
        assert!(all.contains("Grid"));

        let filtered = panel_text("background", false);
        assert!(filtered.contains("View"));
        assert!(filtered.contains("background"));

        let empty = panel_text("zzz-no-such-property", false);
        assert!(empty.contains("No matching properties."));
        assert!(!empty.contains("Paper Size"));
    }

    #[test]
    fn filtering_opens_a_collapsed_section_without_changing_saved_state() {
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = UiState {
            inspector_filter: "background".into(),
            ..Default::default()
        };
        set_section_open(&mut st, "diagram.view", false);
        st.options_dirty = false;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(320.0, 700.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(ui, &mut ed, &mut st));
        out.textures_delta.clear();
        assert_eq!(
            st.options.inspector_sections.get("diagram.view"),
            Some(&false)
        );
        assert!(!st.options_dirty);
    }

    fn shape_editor() -> (Editor, NodeId) {
        let mut ed = Editor::default();
        ed.viewport = crate::geom::v2(800.0, 600.0);
        let id = NodeId(1);
        let mut n = Node::new(
            id,
            NodeKind::Rect,
            crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
        );
        n.style.fills = vec![Paint::solid(Color::rgb(0xda, 0xe8, 0xfc))];
        n.style.stroke = Some(Color::rgb(0x6c, 0x8e, 0xbf));
        ed.add_node(n, None);
        (ed, id)
    }

    #[test]
    fn dark_color_controls_edit_real_state_and_join_undo() {
        use super::{set_fill_color_dark, set_stroke_color_dark};
        let (mut ed, id) = shape_editor();

        set_fill_color_dark(&mut ed, &[id], Color::rgb(0x12, 0x34, 0x56));
        set_stroke_color_dark(&mut ed, &[id], Color::BLACK);
        let n = ed.doc.get(id).unwrap();
        assert_eq!(
            n.style.fill_pair().and_then(|(_, dark)| dark),
            Some(Color::rgb(0x12, 0x34, 0x56))
        );
        assert_eq!(n.style.stroke_dark, Some(Color::BLACK));

        ed.undo();
        assert_eq!(ed.doc.get(id).unwrap().style.stroke_dark, None);
        ed.undo();
        assert_eq!(
            ed.doc
                .get(id)
                .unwrap()
                .style
                .fill_pair()
                .and_then(|(_, d)| d),
            None
        );
        ed.redo();
        ed.redo();
        let n = ed.doc.get(id).unwrap();
        assert_eq!(
            n.style.fill_pair().and_then(|(_, dark)| dark),
            Some(Color::rgb(0x12, 0x34, 0x56))
        );
        assert_eq!(n.style.stroke_dark, Some(Color::BLACK));
    }

    #[test]
    fn shadow_controls_edit_page_and_element_state_and_join_undo() {
        let (mut ed, id) = shape_editor();
        assert!(!ed.doc.settings.shadow);
        assert_eq!(ed.doc.get(id).unwrap().style.shadow, None);

        ed.edit_settings("Toggle diagram shadow", |s| s.shadow = true);
        assert!(ed.doc.settings.shadow);
        assert_eq!(ed.doc.get(id).unwrap().style.shadow, None);

        apply(&mut ed, "shadow", &[id], |n| n.style.shadow = Some(false));
        assert_eq!(ed.doc.get(id).unwrap().style.shadow, Some(false));
        ed.undo();
        assert_eq!(ed.doc.get(id).unwrap().style.shadow, None);
        assert!(ed.doc.settings.shadow);
        ed.undo();
        assert!(!ed.doc.settings.shadow);
        ed.redo();
        ed.redo();
        assert!(ed.doc.settings.shadow);
        assert_eq!(ed.doc.get(id).unwrap().style.shadow, Some(false));
    }

    #[test]
    fn selected_style_tab_exposes_glass_effect() {
        let text = tab_panel_text(crate::ui::InspectorTab::Style, "", Some(NodeId(10)));
        assert!(text.contains("Glass"), "{text}");
    }

    #[test]
    fn glass_effect_edit_joins_undo_and_redo() {
        let (mut ed, id) = shape_editor();
        assert!(!ed.doc.get(id).unwrap().style.glass);
        apply(&mut ed, "glass", &[id], |n| n.style.glass = true);
        assert!(ed.doc.get(id).unwrap().style.glass);
        ed.undo();
        assert!(!ed.doc.get(id).unwrap().style.glass);
        ed.redo();
        assert!(ed.doc.get(id).unwrap().style.glass);
    }

    #[test]
    fn page_adaptive_mode_edit_is_undoable() {
        let (mut ed, _) = shape_editor();
        assert_eq!(ed.doc.settings.adaptive_colors, AdaptiveColors::Auto);
        ed.edit_settings("Change adaptive colors", |s| {
            s.adaptive_colors = AdaptiveColors::Simple;
        });
        assert_eq!(ed.doc.settings.adaptive_colors, AdaptiveColors::Simple);
        ed.undo();
        assert_eq!(ed.doc.settings.adaptive_colors, AdaptiveColors::Auto);
        ed.redo();
        assert_eq!(ed.doc.settings.adaptive_colors, AdaptiveColors::Simple);
    }

    #[test]
    fn selected_style_tab_exposes_searchable_property_values() {
        let text = tab_panel_text(crate::ui::InspectorTab::Style, "", Some(NodeId(11)));
        assert!(text.contains("Property / Value"), "{text}");
        assert!(text.contains("Search properties"), "{text}");
        assert!(text.contains("opacity"), "{text}");
        assert!(text.contains("strokeWidth"), "{text}");
        assert!(text.contains("glass"), "{text}");
    }

    #[test]
    fn style_property_search_filters_keys() {
        let all = tab_panel_text_filtered(crate::ui::InspectorTab::Style, "", Some(NodeId(12)), "");
        assert!(all.contains("strokeWidth"), "{all}");
        assert!(all.contains("glass"), "{all}");

        let filtered = tab_panel_text_filtered(
            crate::ui::InspectorTab::Style,
            "",
            Some(NodeId(12)),
            "glass",
        );
        assert!(filtered.contains("glass"), "{filtered}");
        assert!(!filtered.contains("strokeWidth"), "{filtered}");
    }

    #[test]
    fn connector_property_list_uses_connector_keys() {
        let text = selected_connector_panel_text();
        assert!(text.contains("startSize"), "{text}");
        assert!(text.contains("endOffset"), "{text}");
        assert!(!text.contains("cornerRadius"), "{text}");
    }

    #[test]
    fn style_property_edits_change_real_state_and_join_undo() {
        let (mut ed, id) = shape_editor();
        assert!(super::set_style_property(&mut ed, &[id], "opacity", "101").is_err());
        assert_eq!(ed.doc.get(id).unwrap().style.opacity, 1.0);

        super::set_style_property(&mut ed, &[id], "opacity", "50").unwrap();
        assert_eq!(ed.doc.get(id).unwrap().style.opacity, 0.5);
        super::set_style_property(&mut ed, &[id], "glass", "1").unwrap();
        assert!(ed.doc.get(id).unwrap().style.glass);
        super::set_style_property(&mut ed, &[id], "dash", "4 2").unwrap();
        assert_eq!(ed.doc.get(id).unwrap().style.dash, vec![4.0, 2.0]);

        ed.undo();
        assert!(ed.doc.get(id).unwrap().style.dash.is_empty());
        ed.undo();
        assert!(!ed.doc.get(id).unwrap().style.glass);
        ed.undo();
        assert_eq!(ed.doc.get(id).unwrap().style.opacity, 1.0);
        ed.redo();
        ed.redo();
        ed.redo();
        assert_eq!(ed.doc.get(id).unwrap().style.opacity, 0.5);
        assert!(ed.doc.get(id).unwrap().style.glass);
        assert_eq!(ed.doc.get(id).unwrap().style.dash, vec![4.0, 2.0]);
    }

    fn panel_text_for(ed: &mut Editor, st: &mut UiState, width: f32, height: f32) -> String {
        fn collect(shape: &egui::Shape, out: &mut String) {
            match shape {
                egui::Shape::Text(text) => {
                    out.push_str(&text.galley.job.text);
                    out.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, out);
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        crate::ui::setup_style(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, height),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| panel(ui, ed, st));
        out.textures_delta.clear();
        let mut text = String::new();
        for clipped in &out.shapes {
            collect(&clipped.shape, &mut text);
        }
        text
    }

    #[test]
    fn style_tab_shows_connector_sections_for_connector_selection() {
        let mut ed = Editor::default();
        let a = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(300.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(text.contains("Arrow"), "{text}");
        assert!(!text.contains("Fill\nAuto"), "{text}");
    }

    #[test]
    fn style_tab_hides_arrow_and_fill_for_text_only_selection() {
        let mut ed = Editor::default();
        let text_id = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Text {
                    text: "Hello".into(),
                    font_size: 16.0,
                    auto_width: true,
                    align: crate::model::TextAlign::Left,
                },
                crate::geom::Rect::new(0.0, 0.0, 120.0, 24.0),
            ),
            None,
        );
        ed.selection = vec![text_id];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(!text.contains("\nArrow\n"), "{text}");
        assert!(!text.contains("Fill\nAuto"), "{text}");
    }

    #[test]
    fn mixed_selection_shows_arrow_section_when_any_connector_selected() {
        let mut ed = Editor::default();
        let rect = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let a = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(200.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Rect,
                crate::geom::Rect::new(400.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(4),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(200.0, 30.0), crate::geom::v2(500.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(200.0, 0.0, 300.0, 60.0),
            ),
            None,
        );
        // Rect is first, connector is second - arrow section should still show.
        ed.selection = vec![rect, conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(text.contains("Arrow"), "{text}");
    }

    #[test]
    fn unlabeled_connector_gets_no_text_tab_and_shows_add_label() {
        let mut ed = Editor::default();
        let a = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(300.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(!text.contains("\nText\n"), "{text}");
        assert!(text.contains("Add label"), "{text}");
        assert!(text.contains("Arrow"), "{text}");
    }

    #[test]
    fn connector_only_arrange_tab_hides_shape_only_controls() {
        let mut ed = Editor::default();
        let a = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(300.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(text.contains("Arrange"), "{text}");
        assert!(!text.contains("Copy Size"), "{text}");
        assert!(!text.contains("Apply Size"), "{text}");
        assert!(!text.contains("Constrain Proportions"), "{text}");
        assert!(!text.contains("Autosize"), "{text}");
        assert!(!text.contains("Rotate shape only by 90°"), "{text}");
        assert!(!text.contains("Keep label upright"), "{text}");
        assert!(!text.contains("Flatten"), "{text}");
        assert!(!text.contains("\nSelection\n"), "{text}");
        assert!(text.contains("Flip"), "{text}");
    }

    #[test]
    fn connector_only_arrange_tab_hides_size_and_position_fields() {
        let mut ed = Editor::default();
        let a = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(300.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(!text.contains("\nSize\n"), "{text}");
        assert!(!text.contains("\nW \n"), "{text}");
        assert!(!text.contains("\nH \n"), "{text}");
        assert!(!text.contains("\nPosition\n"), "{text}");
        assert!(!text.contains("\nX \n"), "{text}");
        assert!(!text.contains("\nY \n"), "{text}");
        assert!(!text.contains("\nAlign\n"), "{text}");
        assert!(!text.contains("Snap to Grid"), "{text}");
    }

    #[test]
    fn shape_arrange_tab_keeps_size_position_and_grid_controls() {
        let mut ed = Editor::default();
        let id = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![id];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(text.contains("\nSize\n"), "{text}");
        assert!(text.contains("\nPosition\n"), "{text}");
        assert!(text.contains("\nAlign\n"), "{text}");
        assert!(text.contains("Snap to Grid"), "{text}");
        assert!(text.contains("Flip"), "{text}");
        assert!(text.contains("\nSelection\n"), "{text}");
        assert!(text.contains("Flatten"), "{text}");
    }

    #[test]
    fn connector_only_arrange_tab_hides_layout_section() {
        let mut ed = Editor::default();
        let frame = ed.add_node(
            Node::new(
                NodeId(0),
                NodeKind::Frame { clip: false },
                crate::geom::Rect::new(0.0, 0.0, 500.0, 200.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(20.0, 100.0), crate::geom::v2(480.0, 100.0)],
                    start: Some(bind(frame)),
                    end: Some(bind(frame)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 500.0, 200.0),
            ),
            Some(frame),
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(!text.contains("\nLayout\n"), "{text}");
        assert!(!text.contains("Constraints"), "{text}");
    }

    #[test]
    fn shape_in_frame_arrange_tab_keeps_layout_section() {
        let mut ed = Editor::default();
        let frame = ed.add_node(
            Node::new(
                NodeId(0),
                NodeKind::Frame { clip: false },
                crate::geom::Rect::new(0.0, 0.0, 500.0, 200.0),
            ),
            None,
        );
        let shape = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(20.0, 40.0, 80.0, 60.0),
            ),
            Some(frame),
        );
        ed.selection = vec![shape];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1600.0);
        assert!(text.contains("\nLayout\n"), "{text}");
        assert!(text.contains("Constraints"), "{text}");
    }

    #[test]
    fn connector_only_style_tab_hides_shape_only_effects() {
        let mut ed = Editor::default();
        let a = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let b = ed.add_node(
            Node::new(
                NodeId(2),
                NodeKind::Rect,
                crate::geom::Rect::new(200.0, 0.0, 100.0, 60.0),
            ),
            None,
        );
        let bind = |node| crate::model::Binding {
            node,
            fixed: crate::geom::v2(0.5, 0.5),
            inside: false,
            floating: false,
        };
        let conn = ed.add_node(
            Node::new(
                NodeId(3),
                NodeKind::Connector {
                    points: vec![crate::geom::v2(0.0, 30.0), crate::geom::v2(400.0, 30.0)],
                    start: Some(bind(a)),
                    end: Some(bind(b)),
                    route: crate::model::Route::Straight,
                    start_head: crate::model::Head::None,
                    end_head: crate::model::Head::Arrow,
                    jump: crate::model::LineJump::None,
                    jump_size: 6.0,
                },
                crate::geom::Rect::new(0.0, 0.0, 400.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![conn];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Style,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1800.0);
        assert!(!text.contains("Rounded"), "{text}");
        assert!(!text.contains("Glass"), "{text}");
        assert!(!text.contains("Corner"), "{text}");
        assert!(!text.contains("Rotation"), "{text}");
    }

    #[test]
    fn shape_arrange_tab_exposes_position_size_and_size_tools() {
        let mut ed = Editor::default();
        let id = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(20.0, 30.0, 240.0, 140.0),
            ),
            None,
        );
        ed.selection = vec![id];
        let mut st = UiState {
            inspector_tab: crate::ui::InspectorTab::Arrange,
            ..Default::default()
        };
        let text = panel_text_for(&mut ed, &mut st, 320.0, 1800.0);
        assert!(text.contains("W "), "{text}");
        assert!(text.contains("H "), "{text}");
        assert!(text.contains("X "), "{text}");
        assert!(text.contains("Y "), "{text}");
        assert!(text.contains("Constrain Proportions"), "{text}");
        assert!(text.contains("Copy Size"), "{text}");
    }

    #[test]
    fn style_tab_exposes_palette_section() {
        let text = tab_panel_text(crate::ui::InspectorTab::Style, "", Some(NodeId(9)));
        assert!(text.contains("Palette"), "{text}");
    }
}
