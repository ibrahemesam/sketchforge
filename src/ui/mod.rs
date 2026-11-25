//! egui chrome: menu bar, layers panel, design (properties) panel, floating toolbar, and the
//! projects home screen.

pub mod findbar;
pub mod generate;
pub mod home;
pub mod import_report;
pub mod inspector;
pub mod library;
pub mod pages;
pub mod palette;
pub mod print;
pub mod quick_add;
pub mod templates;

use crate::editor::shortcuts::{self, Action};
use crate::editor::{Editor, Tool};
use crate::model::{Color, Document, ImageMode, Node, NodeId, NodeKind, SchemeScope};
use crate::theme::{self, Icon};
use egui::{Align2, Color32, RichText, Ui};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct UiState {
    pub collapsed: HashSet<NodeId>,
    pub renaming: Option<(NodeId, String)>,
    /// The rename field is focused once when it opens. Re-requesting every frame
    /// would pin the focus, so committing on Enter or a click away could never fire.
    pub renaming_focus: bool,
    pub show_help: bool,
    /// First-run tour. Once dismissed it never opens again unless reopened
    /// from the help dialog.
    pub welcome: Option<WelcomeUi>,
    /// Export settings (inspector Export section).
    pub export: crate::io::export::Spec,
    /// The window has native chrome: a system menu bar (so no egui menu) and traffic lights
    /// inset into the toolbar / sidebar.
    pub native_chrome: bool,
    /// The sidebar sits over an NSVisualEffectView, so paint it transparent.
    pub translucent_sidebar: bool,
    /// When set, the projects home screen is shown instead of the editor.
    pub home: Option<home::HomeView>,
    /// Name of the open project (shown in the toolbar).
    pub project_name: String,
    /// Autosave state for the toolbar subtitle, e.g. "Saved" or "Saving…".
    pub save_state: String,
    pub pages: pages::PagesUi,
    pub library: library::LibraryUi,
    /// Left sidebar shows the library instead of the layers.
    pub show_library: bool,
    pub palette: palette::PaletteUi,
    /// All panels hidden (⌘\\): just the canvas.
    pub hide_ui: bool,
    /// Layer being dragged in the layers panel, and where it would drop.
    pub layer_drag: Option<(NodeId, Option<LayerDrop>)>,
    /// Row rects of the last layers-panel frame (for drop targeting).
    pub layer_rows: Vec<(NodeId, egui::Rect, usize)>,
    pub options: crate::options::Options,
    pub options_dirty: bool,
    /// Which inspector tab is showing. Diagram and the global Style tab when
    /// nothing is selected, Style | Text | Arrange otherwise.
    pub inspector_tab: InspectorTab,
    /// Format-panel property filter. It is session-only; collapsed section
    /// state lives in Options and survives restart.
    pub inspector_filter: String,
    /// Element Style tab: local filter for the key/value property list.
    pub style_property_filter: String,
    /// Uncommitted values in the key/value property list, keyed by selection
    /// and property name so a different selection gets fresh values.
    pub style_property_drafts: HashMap<String, String>,
    /// Validation message for the last key/value property edit.
    pub style_property_error: Option<String>,
    /// The format panel is closed (× or ⌘⇧F) and can be reopened from the
    /// toolbar. Stored inverted so the default is visible.
    pub hide_inspector: bool,
    /// Arrange tab: keep width/height proportional while editing either field.
    pub lock_aspect: bool,
    /// Ratio captured when the aspect lock was switched on (w / h).
    pub aspect_ratio: f64,
    /// Arrange tab: a copied size (draw.io's Copy Size), applied via Apply Size.
    pub copied_size: Option<(f64, f64)>,
    /// Element Style tab: advanced JSON edit in progress.
    pub style_json: Option<inspector::StyleJsonDialog>,
    /// Diagram tab: Edit Data… dialog rows.
    pub metadata_edit: Option<inspector::MetadataEdit>,
    /// Diagram tab: Background › Change… dialog.
    pub background_open: bool,
    /// Page background color while the dialog is open, before Apply.
    pub background_draft: Option<Color>,
    /// Dark page background color while the dialog is open, before Apply.
    pub background_dark_draft: Option<Color>,
    /// Background image placement while the dialog is open, before Apply.
    pub background_image_mode: ImageMode,
    /// Background image opacity while the dialog is open, before Apply.
    pub background_image_opacity: f64,
    /// Page layers dialog (draw.io layers), separate from the z-order panel.
    pub layers_dialog: Option<inspector::LayersDialog>,
    /// Quick-add picker opened by double-clicking empty canvas (draw.io).
    pub quick_add: Option<quick_add::QuickAdd>,
    /// Find bar state (cross-page search and replace).
    pub find: findbar::FindUi,
    /// Template browser: bundled packs and custom libraries (M7).
    pub templates: templates::TemplateUi,
    /// Insert-diagram dialog for Mermaid, CSV, SQL and JSON (M8).
    pub generate: generate::GenerateUi,
    /// Hovered node and the time its tooltip delay started.
    pub tooltip_hover: Option<(NodeId, f64)>,
    /// Fidelity report shown after a draw.io import.
    pub import_report: Option<import_report::ImportReportUi>,
    /// Print dialog: paper, orientation, scaling and margins (IO-19).
    pub print: Option<print::PrintDialog>,
    /// The quick-add search field is focused once when it opens.
    pub quick_add_focus: bool,
    /// Style tab palette page (draw.io-style swatch pager).
    pub palette_page: usize,
    /// Global Style tab scheme page.
    pub scheme_page: usize,
    /// Global Style tab scheme scope: the whole page or selected subtrees.
    pub scheme_scope: SchemeScope,
    /// The active storage target supports autosave (a project folder).
    pub autosave_available: bool,
    /// The canvas area on screen (set every frame by the layout pass); library
    /// drag-outs use it to know when the pointer is over the diagram.
    pub canvas_rect: Option<egui::Rect>,
    /// Canvas context menu opened by a right click that did not drag.
    pub context_menu: Option<ContextMenuUi>,
}

/// Where the canvas context menu opened. The frame guard stops the same right
/// click that opened it from being read as a click elsewhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextMenuUi {
    pub pos: crate::geom::V2,
    pub opened_at_frame: u64,
}

/// State of the first-run tour. Only the step is session state; the fact that
/// the tour was dismissed is stored in Options.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct WelcomeUi {
    pub step: usize,
}

const WELCOME_STEPS: usize = 3;
/// egui measures line height in points, not as a multiple of the font size.
const WELCOME_BODY_LINE_HEIGHT: f32 = 18.0;

/// Heading, title, body and footer for one tour step.
fn welcome_copy(step: usize) -> (&'static str, &'static str, &'static str, &'static str) {
    match step {
        0 => (
            "drawio-rs",
            "Start with a shape",
            "Press R for a rectangle, O for an ellipse or T for text. Drag on the canvas, then press V to select and adjust what you drew.",
            "Projects live in plain folders on your disk. Nothing needs an account.",
        ),
        1 => (
            "Connect",
            "Join ideas with arrows",
            "Press C and drag from one shape to another. Connections stay attached and reroute when you move either end.",
            "Drag from a small arrow or cross to fix where the arrow meets a shape.",
        ),
        2 => (
            "Export",
            "Take the diagram with you",
            "Open Export in the right panel to write PNG, SVG, PDF or draw.io XML. Every page stays in the same project folder.",
            "Press / at any time for the full shortcut list.",
        ),
        _ => ("", "", "", ""),
    }
}

fn welcome_body(step: usize) -> RichText {
    let (_, _, body, _) = welcome_copy(step);
    RichText::new(body)
        .size(13.0)
        .line_height(Some(WELCOME_BODY_LINE_HEIGHT))
}

fn finish_welcome(st: &mut UiState) {
    st.welcome = None;
    st.options.onboarding_seen = true;
    st.options_dirty = true;
}

fn advance_welcome(st: &mut UiState) {
    let Some(w) = st.welcome.as_mut() else {
        return;
    };
    if w.step + 1 >= WELCOME_STEPS {
        finish_welcome(st);
    } else {
        w.step += 1;
    }
}

fn back_welcome(st: &mut UiState) {
    if let Some(w) = st.welcome.as_mut() {
        w.step = w.step.saturating_sub(1);
    }
}

/// The right panel's draw.io-style tabs.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum InspectorTab {
    #[default]
    Diagram,
    Style,
    Text,
    Arrange,
    Data,
}

impl InspectorTab {
    /// Tabs shown with nothing selected.
    pub const GLOBAL: [InspectorTab; 2] = [InspectorTab::Diagram, InspectorTab::Style];

    /// Tabs shown for the current selection. The Text tab is omitted when no
    /// selected element carries editable text or a label. Connectors only show
    /// the Text tab when they carry a label or extra labels. Connector-only
    /// selections omit Data because Link/Tooltip/Tags/Properties are shown in
    /// Arrange (draw.io-aligned, keeps connector metadata accessible).
    pub fn element_tabs(ids: &[NodeId], doc: &crate::model::Document) -> Vec<InspectorTab> {
        let has_text = ids.iter().any(|id| {
            doc.get(*id).is_some_and(|n| {
                n.kind.text().is_some()
                    || (n.is_connector() && (!n.label.is_empty() || !n.labels.is_empty()))
                    || (!n.is_connector() && n.supports_label())
            })
        });
        let all_connectors = !ids.is_empty()
            && ids
                .iter()
                .all(|id| doc.get(*id).is_some_and(|n| n.is_connector()));
        let mut tabs = vec![InspectorTab::Style];
        if has_text {
            tabs.push(InspectorTab::Text);
        }
        tabs.push(InspectorTab::Arrange);
        if !all_connectors {
            tabs.push(InspectorTab::Data);
        }
        tabs
    }

    pub fn tabs_for(
        has_selection: bool,
        ids: &[NodeId],
        doc: &crate::model::Document,
    ) -> Vec<InspectorTab> {
        if has_selection {
            Self::element_tabs(ids, doc)
        } else {
            Self::GLOBAL.to_vec()
        }
    }

    pub fn valid_for(
        self,
        has_selection: bool,
        ids: &[NodeId],
        doc: &crate::model::Document,
    ) -> bool {
        Self::tabs_for(has_selection, ids, doc).contains(&self)
    }

    pub fn first_for(
        has_selection: bool,
        ids: &[NodeId],
        doc: &crate::model::Document,
    ) -> InspectorTab {
        Self::tabs_for(has_selection, ids, doc)
            .first()
            .copied()
            .unwrap_or(if has_selection {
                InspectorTab::Style
            } else {
                InspectorTab::Diagram
            })
    }
}

/// Where a dragged layer lands relative to a row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerDrop {
    /// Above the row (in front of it in paint order).
    Above(NodeId),
    Below(NodeId),
    /// Inside a frame or group (on top of its children).
    Into(NodeId),
}

pub fn setup_style(ctx: &egui::Context) {
    theme::apply(ctx);
}

pub(crate) fn c32(c: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}
pub(crate) fn from32(c: Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

/// Section header, as in a macOS sidebar or inspector: 11pt semibold, secondary colour.
pub(crate) fn section(ui: &mut Ui, title: &str) {
    ui.label(
        RichText::new(title)
            .font(theme::semibold(theme::SECONDARY))
            .color(theme::pal(ui).secondary),
    );
}

pub(crate) fn title_text(text: impl Into<String>) -> RichText {
    RichText::new(text).font(theme::semibold(theme::BODY))
}

fn theme_button(ui: &mut Ui, options: &mut crate::options::Options, dirty: &mut bool) {
    use crate::options::ThemeChoice;
    let dark = ui.visuals().dark_mode;
    let (icon, tip) = if dark {
        (Icon::Moon, "Dark theme")
    } else {
        (Icon::Sun, "Light theme")
    };
    if theme::icon_button_labeled(ui, icon, 24.0, false, tip)
        .on_hover_text(tip)
        .clicked()
    {
        options.theme = if dark {
            ThemeChoice::Light
        } else {
            ThemeChoice::Dark
        };
        theme::set_preference(ui.ctx(), options.theme);
        *dirty = true;
    }
}

/// Lay out the chrome and return the remaining canvas rectangle (logical px).
pub fn draw(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) -> egui::Rect {
    if let Some(hv) = &mut st.home {
        home::draw(
            ui,
            hv,
            st.native_chrome,
            st.translucent_sidebar,
            &mut st.options,
            &mut st.options_dirty,
        );
        return egui::Rect::NOTHING;
    }
    for ev in std::mem::take(&mut ed.ui_events) {
        use crate::editor::commands::UiEvent;
        match ev {
            UiEvent::TogglePalette => st.palette.toggle(),
            UiEvent::Generate(source) => st.generate.open_with(source),
            UiEvent::Rename(id) => {
                if let Some(n) = ed.doc.get(id) {
                    st.renaming = Some((id, n.name.clone()));
                    st.renaming_focus = false;
                    st.show_library = false;
                    st.hide_ui = false;
                }
            }
            UiEvent::ToggleUi => st.hide_ui = !st.hide_ui,
            UiEvent::OpenFind => st.find.open(),
            UiEvent::FindStep(next) => {
                st.find.open = true;
                st.find.step_current(ed, next);
            }
            UiEvent::ToggleHelp => st.show_help = !st.show_help,
            UiEvent::ToggleGrid => ed.toggle_grid(),
            UiEvent::ToggleInspector => st.hide_inspector = !st.hide_inspector,
            UiEvent::QuickAdd { world } => {
                st.quick_add = Some(crate::ui::quick_add::QuickAdd {
                    world,
                    ..Default::default()
                });
                st.quick_add_focus = true;
            }
            UiEvent::ContextMenu { pos } => {
                st.context_menu = Some(ContextMenuUi {
                    pos,
                    opened_at_frame: ui.ctx().cumulative_frame_nr(),
                });
            }
            UiEvent::CloseMenu => {
                st.context_menu = None;
                ed.context_menu_open = false;
            }
        }
    }
    context_menu(ui.ctx(), ed, st);
    if st.hide_ui {
        let canvas = ui.available_rect_before_wrap();
        palette::draw(ui.ctx(), ed, st, canvas);
        return canvas;
    }
    let p = theme::pal(ui);
    let sidebar_fill = if st.translucent_sidebar {
        Color32::TRANSPARENT
    } else {
        p.sidebar
    };
    egui::Panel::left("layers")
        .resizable(true)
        .default_size(240.0)
        .size_range(180.0..=420.0)
        .frame(
            egui::Frame::NONE
                .fill(sidebar_fill)
                .inner_margin(egui::Margin::symmetric(10, 8)),
        )
        .show(ui, |ui| {
            if st.native_chrome {
                // Room for the traffic lights, which sit over the top of the sidebar.
                ui.add_space(theme::TOOLBAR_H - 8.0);
            }
            layers(ui, ed, st)
        });
    egui::Panel::top("toolbar")
        .exact_size(if st.native_chrome {
            theme::TOOLBAR_H
        } else {
            34.0
        })
        .frame(
            egui::Frame::NONE
                .fill(p.window)
                .inner_margin(egui::Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| top_bar(ui, ed, st));
    if !st.hide_inspector {
        // The panel's laid-out outer rect: the available area's right edge is
        // the panel's fixed edge, and its size comes from the stored state (or
        // the 260 default on first run).
        let avail = ui.available_rect_before_wrap();
        let laid_out_right = avail.right();
        let panel_resp = egui::Panel::right("design")
            .resizable(true)
            .default_size(260.0)
            .size_range(220.0..=420.0)
            .frame(
                egui::Frame::NONE
                    .fill(p.inspector)
                    .inner_margin(egui::Margin::symmetric(14, 10)),
            )
            .show(ui, |ui| inspector::panel(ui, ed, st));
        // egui stores the panel's size from the frame's response rect, which
        // grows with the content. A row wider than the panel would then widen
        // the panel itself, frame after frame. If the content overflowed the
        // laid-out rect, put the stored size back to what was laid out.
        if panel_resp.response.rect.right() > laid_out_right + 0.5 {
            let mut outer_rect = panel_resp.response.rect;
            outer_rect.max.x = laid_out_right;
            ui.ctx().data_mut(|d| {
                d.insert_persisted(
                    egui::Id::new("design"),
                    egui::containers::panel::PanelState { outer_rect },
                );
            });
        }
    }
    egui::Panel::bottom("page-tabs")
        .exact_size(28.0)
        .frame(
            egui::Frame::NONE
                .fill(p.window)
                .inner_margin(egui::Margin::symmetric(8, 2)),
        )
        .show(ui, |ui| {
            pages::bottom_tabs(ui, ed, st);
        });
    inspector::dialogs(ui.ctx(), ed, st);
    templates::dialog(ui.ctx(), ed, st);
    generate::dialog(ui.ctx(), ed, st);
    quick_add::dialog(ui.ctx(), ed, st);
    let canvas = ui.available_rect_before_wrap();
    import_report::dialog(ui.ctx(), st, canvas);
    toolbar(ui.ctx(), ed, canvas);
    status(ui.ctx(), ed, canvas);
    findbar::draw(ui.ctx(), ed, st, canvas);
    tooltip(ui.ctx(), ed, st, canvas);
    palette::draw(ui.ctx(), ed, st, canvas);
    if st.show_help {
        help(ui.ctx(), st, canvas);
    }
    if st.welcome.is_some() {
        welcome(ui.ctx(), st, canvas);
    }
    canvas
}

fn menu_item(ui: &mut Ui, ed: &mut Editor, label: &str, action: Action) {
    if ui
        .add(egui::Button::new(label).shortcut_text(shortcuts::chord_text(action)))
        .clicked()
    {
        ed.run(action);
        ui.close();
    }
}

fn menu_entries(ui: &mut Ui, ed: &mut Editor, entries: &[shortcuts::MenuEntry]) {
    for e in entries {
        match e {
            shortcuts::MenuEntry::Item(label, a) => menu_item(ui, ed, label, *a),
            shortcuts::MenuEntry::Separator => {
                ui.separator();
            }
            shortcuts::MenuEntry::Submenu(label, items) => {
                ui.menu_button(*label, |ui| menu_entries(ui, ed, items));
            }
        }
    }
}

/// One row of the canvas context menu. Separators keep the grouping close to
/// draw.io's menu without bringing over any of its assets.
#[derive(Debug, Clone, Copy, PartialEq)]
enum ContextEntry {
    Separator,
    Item(&'static str, Action),
}

fn context_entries(ed: &Editor) -> Vec<ContextEntry> {
    use ContextEntry::{Item, Separator};
    if ed.selection.is_empty() {
        return vec![
            Item("Paste", Action::Paste),
            Separator,
            Item("Select All", Action::SelectAll),
            Item("Zoom to Fit", Action::ZoomFit),
            Item("Dot Grid", Action::ToggleGrid),
        ];
    }
    let mut entries = Vec::new();
    if ed.selection.len() == 1 {
        entries.extend([Item("Edit…", Action::EditText), Separator]);
    }
    entries.extend([
        Item("Cut", Action::Cut),
        Item("Copy", Action::Copy),
        Item("Paste", Action::Paste),
        Item("Duplicate", Action::Duplicate),
        Item("Delete", Action::Delete),
        Separator,
        Item("Bring to Front", Action::ToFront),
        Item("Send to Back", Action::ToBack),
    ]);
    if ed.selection.len() > 1 {
        entries.push(Item("Group", Action::Group));
    }
    if ed
        .selection
        .iter()
        .any(|id| ed.doc.get(*id).is_some_and(|n| n.is_group()))
    {
        entries.push(Item("Ungroup", Action::Ungroup));
    }
    entries
}

/// The right-click menu on the canvas. It uses the editor's existing actions so
/// the menu, shortcuts and toolbar cannot drift apart.
fn context_menu(ctx: &egui::Context, ed: &mut Editor, st: &mut UiState) {
    let Some(menu) = st.context_menu else {
        return;
    };
    let entries = context_entries(ed);
    let screen_rect = ctx.input(|i| i.viewport_rect());
    let origin = st.canvas_rect.map(|r| r.min).unwrap_or(screen_rect.min);
    let pos = egui::pos2(origin.x + menu.pos.x as f32, origin.y + menu.pos.y as f32);
    let opened_here = menu.opened_at_frame == ctx.cumulative_frame_nr();
    let mut chosen = None;
    let response = egui::Area::new("canvas-context-menu".into())
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .constrain_to(screen_rect)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(190.0);
                for entry in entries {
                    match entry {
                        ContextEntry::Separator => {
                            ui.separator();
                        }
                        ContextEntry::Item(label, action) => {
                            if ui
                                .add(
                                    egui::Button::new(label)
                                        .shortcut_text(shortcuts::chord_text(action)),
                                )
                                .clicked()
                            {
                                chosen = Some(action);
                            }
                        }
                    }
                }
            });
        })
        .response;
    if !opened_here && response.clicked_elsewhere() {
        st.context_menu = None;
        ed.context_menu_open = false;
    }
    if let Some(action) = chosen {
        ed.run(action);
        st.context_menu = None;
        ed.context_menu_open = false;
    }
}

/// The unified toolbar: document title on the left, zoom and help on the right. Without native
/// chrome (Linux) it also carries an egui menu bar.
fn top_bar(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    ui.horizontal_centered(|ui| {
        if !st.native_chrome {
            menu_bar(ui, ed);
            ui.add_space(8.0);
        }
        if theme::icon_button_labeled(ui, theme::Icon::Grid, 26.0, false, "All projects")
            .on_hover_text("All projects")
            .clicked()
        {
            ed.requests.push(crate::editor::AppRequest::Home);
        }
        ui.add_space(2.0);
        let undo = ui.add_enabled_ui(ed.history.can_undo(), |ui| {
            theme::icon_button_labeled(ui, theme::Icon::Undo, 26.0, false, "Undo")
                .on_hover_text("Undo (⌘Z)")
        });
        if undo.inner.clicked() {
            ed.run(Action::Undo);
        }
        let redo = ui.add_enabled_ui(ed.history.can_redo(), |ui| {
            theme::icon_button_labeled(ui, theme::Icon::Redo, 26.0, false, "Redo")
                .on_hover_text("Redo (⌘⇧Z)")
        });
        if redo.inner.clicked() {
            ed.run(Action::Redo);
        }
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(2.0);
        let has_sel = !ed.selection.is_empty();
        let del = ui.add_enabled_ui(has_sel, |ui| {
            theme::icon_button_labeled(ui, theme::Icon::Trash, 26.0, false, "Delete")
                .on_hover_text("Delete (Del)")
        });
        if del.inner.clicked() {
            ed.run(Action::Delete);
        }
        let dup = ui.add_enabled_ui(has_sel, |ui| {
            theme::icon_button_labeled(ui, theme::Icon::Duplicate, 26.0, false, "Duplicate")
                .on_hover_text("Duplicate (⌘D)")
        });
        if dup.inner.clicked() {
            ed.run(Action::Duplicate);
        }
        let to_front = ui.add_enabled_ui(has_sel, |ui| {
            theme::icon_button_labeled(ui, theme::Icon::ToFront, 26.0, false, "Bring to front")
                .on_hover_text("Bring to front (⌘⇧])")
        });
        if to_front.inner.clicked() {
            ed.run(Action::ToFront);
        }
        let to_back = ui.add_enabled_ui(has_sel, |ui| {
            theme::icon_button_labeled(ui, theme::Icon::ToBack, 26.0, false, "Send to back")
                .on_hover_text("Send to back (⌘⇧[)")
        });
        if to_back.inner.clicked() {
            ed.run(Action::ToBack);
        }
        ui.add_space(4.0);
        let title = if st.project_name.is_empty() {
            ed.doc.name.clone()
        } else {
            st.project_name.clone()
        };
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(if st.native_chrome { 6.0 } else { 1.0 });
            ui.label(title_text(title));
            let state = if !st.save_state.is_empty() {
                st.save_state.clone()
            } else if ed.is_dirty() {
                "Edited".into()
            } else {
                String::new()
            };
            let sub = if state.is_empty() {
                ed.page_name().to_string()
            } else {
                format!("{} — {state}", ed.page_name())
            };
            ui.label(RichText::new(sub).size(theme::SECONDARY).color(p.secondary));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            theme_button(ui, &mut st.options, &mut st.options_dirty);
            ui.add_space(4.0);
            if theme::icon_button_labeled(
                ui,
                theme::Icon::Sidebar,
                26.0,
                !st.hide_inspector,
                "Format panel",
            )
            .on_hover_text("Format panel (⌘⇧F)")
            .clicked()
            {
                st.hide_inspector = !st.hide_inspector;
            }
            ui.add_space(4.0);
            if theme::icon_button_labeled(
                ui,
                theme::Icon::Question,
                26.0,
                st.show_help,
                "Keyboard shortcuts",
            )
            .on_hover_text("Keyboard shortcuts")
            .clicked()
            {
                st.show_help = !st.show_help;
            }
            ui.add_space(8.0);
            if theme::icon_button_labeled(ui, theme::Icon::Plus, 26.0, false, "Zoom in")
                .on_hover_text("Zoom in")
                .clicked()
            {
                ed.run(Action::ZoomIn);
            }
            let z = ui.add(
                egui::Button::new(
                    RichText::new(format!("{:.0}%", ed.camera.zoom * 100.0))
                        .font(theme::medium(theme::SECONDARY + 1.0)),
                )
                .frame(false)
                .min_size(egui::vec2(44.0, 22.0)),
            );
            if z.on_hover_text("Zoom to fit").clicked() {
                ed.run(Action::ZoomFit);
            }
            if theme::icon_button_labeled(ui, theme::Icon::Minus, 26.0, false, "Zoom out")
                .on_hover_text("Zoom out")
                .clicked()
            {
                ed.run(Action::ZoomOut);
            }
        });
    });
    // 1px separator under the toolbar.
    let r = ui.max_rect();
    ui.painter().hline(
        r.x_range(),
        r.bottom() - 0.5,
        egui::Stroke::new(1.0, p.separator),
    );
}

/// egui menus for platforms without a native menu bar (inline, so the toolbar row keeps
/// the title and zoom controls).
fn menu_bar(ui: &mut Ui, ed: &mut Editor) {
    for (title, entries) in shortcuts::menus() {
        ui.menu_button(title, |ui| {
            menu_entries(ui, ed, &entries);
        });
    }
}

fn layers(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    pages::panel(ui, ed, st);
    ui.add_space(4.0);
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(
        r.x_range(),
        r.top(),
        egui::Stroke::new(1.0, theme::pal(ui).separator),
    );
    ui.add_space(6.0);
    let p = theme::pal(ui);
    ui.horizontal(|ui| {
        for (lib, label) in [(false, "Layers"), (true, "Shapes")] {
            let on = st.show_library == lib;
            let r = ui.add(
                egui::Label::new(
                    RichText::new(label)
                        .font(theme::semibold(theme::SECONDARY))
                        .color(if on { p.text } else { p.secondary }),
                )
                .sense(egui::Sense::click()),
            );
            if r.clicked() {
                st.show_library = lib;
            }
        }
        if !st.show_library
            && ui
                .small_button("Page layers…")
                .on_hover_text(
                    "Organize the page into layers with their own visibility, \
                                lock and order. The Layers tab is the stacking order.",
                )
                .clicked()
        {
            st.layers_dialog = Some(Default::default());
        }
        if st.show_library {
            return;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let some = !ed.selection.is_empty();
            ui.add_enabled_ui(some, |ui| {
                if theme::icon_button(ui, Icon::ChevronDown, 20.0, false)
                    .on_hover_text("Send backward")
                    .clicked()
                {
                    ed.run(Action::Backward);
                }
                if theme::icon_button(ui, Icon::ChevronUp, 20.0, false)
                    .on_hover_text("Bring forward")
                    .clicked()
                {
                    ed.run(Action::Forward);
                }
            });
        });
    });
    ui.add_space(2.0);
    if st.show_library {
        library::panel(ui, ed, st);
        return;
    }
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let roots = ed.doc.root.clone();
            if roots.is_empty() {
                ui.label(
                    RichText::new("Nothing here yet. Pick a tool below\nand draw on the canvas.")
                        .size(theme::SECONDARY)
                        .color(theme::pal(ui).secondary),
                );
            }
            st.layer_rows.clear();
            for id in roots.iter().rev() {
                layer_row(ui, ed, st, *id, 0);
            }
            layer_drag(ui, ed, st);
        });
}

/// Drag-and-drop reordering in the layers tree: above / below a row, or into a frame/group.
fn layer_drag(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let Some((dragged, _)) = st.layer_drag else {
        return;
    };
    let Some(pos) = ui.input(|i| i.pointer.interact_pos()) else {
        return;
    };
    let target = st
        .layer_rows
        .iter()
        .find(|(_, r, _)| r.y_range().contains(pos.y))
        .copied();
    let drop = target.and_then(|(id, r, _)| {
        if id == dragged || ed.doc.is_ancestor(dragged, id) {
            return None;
        }
        let n = ed.doc.get(id)?;
        let t = (pos.y - r.top()) / r.height();
        Some(if n.is_container() && (0.3..0.7).contains(&t) {
            LayerDrop::Into(id)
        } else if t < 0.5 {
            LayerDrop::Above(id)
        } else {
            LayerDrop::Below(id)
        })
    });
    st.layer_drag = Some((dragged, drop));
    // Indicator.
    if let (Some(d), Some((_, r, depth))) = (drop, target) {
        let x0 = r.left() + 4.0 + depth as f32 * 14.0 + 16.0;
        let stroke = egui::Stroke::new(2.0, theme::accent());
        match d {
            LayerDrop::Above(_) => {
                ui.painter().hline(x0..=r.right(), r.top(), stroke);
            }
            LayerDrop::Below(_) => {
                ui.painter().hline(x0..=r.right(), r.bottom(), stroke);
            }
            LayerDrop::Into(_) => {
                ui.painter()
                    .rect_stroke(r, 6.0, stroke, egui::StrokeKind::Inside);
            }
        }
    }
    if ui.input(|i| i.pointer.any_released()) {
        st.layer_drag = None;
        if let Some(d) = drop {
            drop_layer(ed, dragged, d);
        }
    }
}

/// Move a layer in the tree (keeping its position on the canvas).
pub fn drop_layer(ed: &mut Editor, id: NodeId, d: LayerDrop) {
    let (parent, index) = match d {
        LayerDrop::Into(c) => (Some(c), None),
        LayerDrop::Above(t) | LayerDrop::Below(t) => {
            let parent = ed.doc.get(t).and_then(|n| n.parent);
            let list: Vec<NodeId> = ed
                .doc
                .children(parent)
                .iter()
                .copied()
                .filter(|x| *x != id)
                .collect();
            let i = list.iter().position(|x| *x == t).unwrap_or(0);
            (
                parent,
                Some(if matches!(d, LayerDrop::Above(_)) {
                    i + 1
                } else {
                    i
                }),
            )
        }
    };
    if parent.is_some_and(|p| p == id || ed.doc.is_ancestor(id, p)) {
        return;
    }
    ed.edit("Move layer", |tx| {
        let same = tx.doc().get(id).and_then(|n| n.parent) == parent;
        if same {
            if let Some(i) = index {
                tx.move_to_index(id, i);
            } else {
                let len = tx.doc().children(parent).len();
                tx.move_to_index(id, len);
            }
        } else {
            tx.reparent(id, parent, index);
        }
    });
    ed.selection = vec![id];
}

fn node_icon(n: &crate::model::Node) -> Icon {
    if n.component {
        return Icon::Component;
    }
    if n.instance.is_some() && n.parent.is_none_or(|_| n.is_frame()) && n.is_frame() {
        return Icon::Instance;
    }
    if n.style
        .fills
        .iter()
        .any(|p| matches!(p, crate::model::Paint::Image { .. }))
        && matches!(n.kind, NodeKind::Rect)
    {
        return Icon::Image;
    }
    kind_icon(&n.kind)
}

fn kind_icon(k: &NodeKind) -> Icon {
    match k {
        NodeKind::Group => Icon::Group,
        NodeKind::Connector { .. } => Icon::Arrow,
        NodeKind::Sticky { .. } => Icon::Sticky,
        NodeKind::Path { .. } => Icon::Pen,
        NodeKind::Polygon { .. } => Icon::Polygon,
        NodeKind::Star { .. } => Icon::Star,
        NodeKind::Frame { .. } => Icon::Frame,
        NodeKind::Rect => Icon::Rect,
        NodeKind::Ellipse => Icon::Ellipse,
        NodeKind::Line { .. } => Icon::Line,
        NodeKind::Text { .. } => Icon::Text,
        NodeKind::Stroke { .. } => Icon::Ink,
    }
}

/// One sidebar row, drawn like an NSOutlineView row: 24pt tall, 6pt radius selection in the
/// accent colour, subtle hover, disclosure chevron, symbol, name, and eye/lock on hover.
fn layer_row(ui: &mut Ui, ed: &mut Editor, st: &mut UiState, id: NodeId, depth: usize) {
    let Some(n) = ed.doc.get(id).cloned() else {
        return;
    };
    let p = theme::pal(ui);
    let selected = ed.selection.contains(&id);
    let width = ui.available_width();
    let (row, resp) =
        ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click_and_drag());
    st.layer_rows.push((id, row, depth));
    if resp.drag_started() && !n.locked {
        st.layer_drag = Some((id, None));
    }
    let hovered = ui.rect_contains_pointer(row);
    if selected {
        ui.painter().rect_filled(row, 6.0, theme::accent());
    } else if hovered {
        ui.painter().rect_filled(row, 6.0, p.hover);
    }
    let fg = if selected {
        Color32::WHITE
    } else if n.visible {
        p.text
    } else {
        p.secondary
    };
    let fg2 = if selected {
        Color32::from_white_alpha(200)
    } else {
        p.secondary
    };
    let mut x = row.left() + 4.0 + depth as f32 * 14.0;
    let cy = row.center().y;
    let sq = |x: f32, s: f32| {
        egui::Rect::from_center_size(egui::pos2(x + s / 2.0, cy), egui::vec2(s, s))
    };

    // Disclosure chevron.
    if !n.children.is_empty() {
        let open = !st.collapsed.contains(&id);
        let r = sq(x, 14.0);
        let cr = ui.interact(
            r.expand(2.0),
            ui.id().with(("disclose", id)),
            egui::Sense::click(),
        );
        theme::paint_icon(
            ui.painter(),
            r.shrink(2.0),
            if open {
                Icon::ChevronDown
            } else {
                Icon::ChevronRight
            },
            fg2,
        );
        if cr.clicked() {
            if open {
                st.collapsed.insert(id);
            } else {
                st.collapsed.remove(&id);
            }
        }
    }
    x += 16.0;
    theme::paint_icon(
        ui.painter(),
        sq(x, 14.0),
        node_icon(&n),
        if selected {
            Color32::WHITE
        } else {
            theme::accent()
        },
    );
    x += 20.0;

    // Trailing eye / lock buttons: always shown when hidden or locked, otherwise on hover.
    let mut right = row.right() - 4.0;
    let mut trailing = |ui: &mut Ui, icon: Icon, tip: &str, show: bool| -> bool {
        let r = egui::Rect::from_center_size(egui::pos2(right - 9.0, cy), egui::vec2(18.0, 18.0));
        right -= 20.0;
        if !show {
            return false;
        }
        let b = ui.interact(r, ui.id().with((tip, id)), egui::Sense::click());
        if b.hovered() {
            ui.painter().rect_filled(
                r,
                5.0,
                if selected {
                    Color32::from_white_alpha(40)
                } else {
                    p.hover
                },
            );
        }
        theme::paint_icon(ui.painter(), r.shrink(3.0), icon, fg2);
        b.on_hover_text(tip).clicked()
    };
    let eye = if n.visible { Icon::Eye } else { Icon::EyeSlash };
    if trailing(
        ui,
        eye,
        if n.visible { "Hide" } else { "Show" },
        hovered || !n.visible,
    ) {
        ed.edit(if n.visible { "Hide" } else { "Show" }, |tx| {
            tx.node_mut(id).unwrap().visible = !n.visible
        });
    }
    let lock = if n.locked { Icon::Lock } else { Icon::LockOpen };
    if trailing(
        ui,
        lock,
        if n.locked { "Unlock" } else { "Lock" },
        hovered || n.locked,
    ) {
        ed.edit(if n.locked { "Unlock" } else { "Lock" }, |tx| {
            tx.node_mut(id).unwrap().locked = !n.locked
        });
    }

    // Name, or an inline rename field.
    let name_rect =
        egui::Rect::from_min_max(egui::pos2(x, row.top()), egui::pos2(right, row.bottom()));
    if let Some((rid, name)) = &mut st.renaming
        && *rid == id
    {
        let r = ui.put(
            name_rect.shrink2(egui::vec2(0.0, 2.0)),
            egui::TextEdit::singleline(name),
        );
        // Focus once when the rename opens; a per-frame request would pin the focus
        // and stop Enter and clicks elsewhere from ever firing `lost_focus`.
        if !st.renaming_focus {
            r.request_focus();
            st.renaming_focus = true;
        }
        let (enter, enter_ctrl, escape) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::Enter),
                i.modifiers.ctrl || i.modifiers.command,
                i.key_pressed(egui::Key::Escape),
            )
        });
        if r.has_focus() && escape {
            // Unfocus and discard the typed text (the node keeps its old name).
            st.renaming = None;
        } else if r.has_focus() && enter && enter_ctrl {
            // Ctrl+Enter keeps editing and adds a new line at the caret.
            let at = egui::TextEdit::load_state(ui.ctx(), r.id)
                .and_then(|s| s.cursor.char_range())
                .map_or(name.chars().count(), |c| c.primary.index.0);
            let byte = name.char_indices().nth(at).map_or(name.len(), |(b, _)| b);
            name.insert(byte, '\n');
            if let Some(mut s) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                s.cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor {
                        index: egui::text::CharIndex::from(at + 1),
                        prefer_next_row: true,
                    })));
                s.store(ui.ctx(), r.id);
            }
        } else if r.lost_focus() {
            // Enter (egui unfocuses singleline) or clicking another widget commits.
            let new = name.trim().to_string();
            st.renaming = None;
            if !new.is_empty() && new != n.name {
                ed.edit("Rename", |tx| tx.node_mut(id).unwrap().name = new);
            }
        }
    } else {
        let font = if n.is_frame() {
            theme::medium(theme::BODY)
        } else {
            egui::FontId::proportional(theme::BODY)
        };
        let galley = ui.painter().layout(n.name.clone(), font, fg, f32::INFINITY);
        let clip = ui
            .painter()
            .with_clip_rect(name_rect.intersect(ui.clip_rect()));
        clip.galley(egui::pos2(x, cy - galley.size().y / 2.0), galley, fg);
    }

    if resp.double_clicked() {
        st.renaming = Some((id, n.name.clone()));
        st.renaming_focus = false;
    } else if resp.clicked() {
        ed.end_text_edit();
        if ui.input(|i| i.modifiers.shift || i.modifiers.command) {
            if selected {
                ed.selection.retain(|s| *s != id);
            } else {
                ed.selection.push(id);
            }
        } else {
            ed.selection = vec![id];
        }
    }
    if hovered {
        ed.hover = Some(id);
    }
    if !st.collapsed.contains(&id) {
        for c in n.children.iter().rev() {
            layer_row(ui, ed, st, *c, depth + 1);
        }
    }
}

fn tool_icon(t: Tool) -> Icon {
    match t {
        Tool::Select => Icon::Pointer,
        Tool::Hand => Icon::Hand,
        Tool::Frame => Icon::Frame,
        Tool::Rect => Icon::Rect,
        Tool::Ellipse => Icon::Ellipse,
        Tool::Polygon => Icon::Polygon,
        Tool::Diamond => Icon::Diamond,
        Tool::Arrow => Icon::Arrow,
        Tool::Sticky => Icon::Sticky,
        Tool::Laser => Icon::Laser,
        Tool::Star => Icon::Star,
        Tool::Pen => Icon::Pen,
        Tool::Line => Icon::Line,
        Tool::Text => Icon::Text,
        Tool::Pencil => Icon::Pencil,
        Tool::Eraser => Icon::Eraser,
    }
}

/// Floating tool palette at the bottom of the canvas, styled as a macOS popover.
fn toolbar(ctx: &egui::Context, ed: &mut Editor, canvas: egui::Rect) {
    egui::Area::new("toolbar".into())
        .pivot(Align2::CENTER_BOTTOM)
        .fixed_pos(egui::pos2(canvas.center().x, canvas.bottom() - 16.0))
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(10.0)
                .inner_margin(5.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for t in Tool::ALL {
                            if matches!(t, Tool::Frame | Tool::Line | Tool::Text | Tool::Pencil) {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(9.0, 28.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().vline(
                                    r.center().x,
                                    r.y_range().shrink(6.0),
                                    egui::Stroke::new(1.0, p.separator),
                                );
                            }
                            if theme::icon_button_labeled(
                                ui,
                                tool_icon(t),
                                30.0,
                                ed.tool == t,
                                t.label(),
                            )
                            .on_hover_text(format!("{} ({})", t.label(), t.shortcut()))
                            .clicked()
                            {
                                ed.run(Action::Tool(t));
                            }
                        }
                    });
                });
        });
}

fn status(ctx: &egui::Context, ed: &Editor, canvas: egui::Rect) {
    if ed.status.is_empty() {
        return;
    }
    egui::Area::new("status".into())
        .pivot(Align2::LEFT_BOTTOM)
        .fixed_pos(egui::pos2(canvas.left() + 12.0, canvas.bottom() - 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(&ed.status).weak().small());
        });
}

/// Tooltip text longer than this is clipped with an ellipsis. draw.io wraps
/// tooltips, but one pasted paragraph should not push the popup off screen.
const TOOLTIP_MAX_CHARS: usize = 200;

/// Resolve placeholders in a node's tooltip and prepare its display text.
/// Long text is clipped on a character boundary with a trailing ellipsis so a
/// pasted paragraph cannot grow the popup off the canvas.
fn tooltip_display_text(tooltip: &str, node: &Node, doc: &Document) -> String {
    let resolved = crate::model::placeholders::resolve_text(tooltip, Some(node), doc);
    let Some((cut, _)) = resolved.char_indices().nth(TOOLTIP_MAX_CHARS) else {
        return resolved;
    };
    let clipped = resolved[..cut].trim_end();
    format!("{clipped}…")
}

/// Show a shape's tooltip after the pointer rests on it, resolved through page
/// placeholders. The delay keeps tooltips from flickering while passing over.
fn tooltip(ctx: &egui::Context, ed: &Editor, st: &mut UiState, canvas: egui::Rect) {
    let now = ctx.input(|i| i.time);
    match (ed.hover, st.tooltip_hover) {
        (None, _) => st.tooltip_hover = None,
        (Some(id), Some((hovered, _))) if hovered == id => {}
        (Some(id), _) => st.tooltip_hover = Some((id, now)),
    }
    let Some((id, since)) = st.tooltip_hover else {
        return;
    };
    let Some(n) = ed.doc.get(id) else {
        return;
    };
    if n.tooltip.trim().is_empty() {
        return;
    }
    let delay = 0.6;
    if now - since < delay {
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        return;
    }
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    if !canvas.contains(pos) {
        return;
    }
    let text = tooltip_display_text(&n.tooltip, n, &ed.doc);
    egui::Area::new(egui::Id::new(("shape-tooltip", id.0)))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 18.0))
        .constrain_to(canvas)
        .interactable(false)
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 5))
                .show(ui, |ui| {
                    ui.set_max_width(260.0);
                    ui.label(RichText::new(text).size(12.0));
                });
        });
}

fn welcome(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    let Some(tour) = st.welcome else {
        return;
    };
    let (label, title, _, tip) = welcome_copy(tour.step);
    let width = 520.0f32.min(canvas.width() - 32.0).max(280.0);
    egui::Area::new("welcome".into())
        .order(egui::Order::Foreground)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(canvas.center())
        .show(ctx, |ui| {
            let p = theme::pal(ui);
            egui::Frame::popup(ui.style())
                .fill(p.popover)
                .corner_radius(14.0)
                .inner_margin(18.0)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("WELCOME TO SKETCHFORGE")
                                .font(theme::semibold(11.0))
                                .color(theme::accent()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("Step {} of {WELCOME_STEPS}", tour.step + 1))
                                    .size(11.0)
                                    .color(p.secondary),
                            );
                        });
                    });
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(label)
                            .font(theme::medium(12.0))
                            .color(p.secondary),
                    );
                    ui.label(RichText::new(title).font(theme::semibold(24.0)));
                    ui.add_space(8.0);
                    ui.label(welcome_body(tour.step));
                    ui.add_space(12.0);
                    ui.label(RichText::new(tip).size(12.0).color(p.secondary));
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if tour.step > 0 && ui.button("Back").clicked() {
                            back_welcome(st);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let last = tour.step + 1 >= WELCOME_STEPS;
                            if ui
                                .button(if last { "Start sketching" } else { "Next" })
                                .clicked()
                            {
                                advance_welcome(st);
                            }
                            if ui.button("Skip").clicked() {
                                finish_welcome(st);
                            }
                        });
                    });
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        finish_welcome(st);
    }
}

fn help(ctx: &egui::Context, st: &mut UiState, canvas: egui::Rect) {
    let width = 820.0f32.min(canvas.width() - 40.0);
    egui::Area::new("help".into())
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
                        ui.label(RichText::new("Keyboard shortcuts").font(theme::semibold(15.0)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Done").on_hover_text("Close (Esc)").clicked() {
                                st.show_help = false;
                            }
                            if ui.button("Show welcome").clicked() {
                                st.welcome = Some(WelcomeUi::default());
                                st.show_help = false;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    let sections = shortcuts::help_sections();
                    egui::ScrollArea::vertical()
                        .max_height((canvas.height() - 160.0).max(200.0))
                        .show(ui, |ui| {
                            ui.columns(3, |cols| {
                                // Balance columns by row count.
                                let total: usize = sections.iter().map(|(_, r)| r.len() + 2).sum();
                                let mut acc = 0;
                                for (i, (title, rows)) in sections.iter().enumerate() {
                                    let col = (acc * 3 / total.max(1)).min(2);
                                    acc += rows.len() + 2;
                                    let ui = &mut cols[col];
                                    ui.label(title_text(*title));
                                    egui::Grid::new(("keys", i)).num_columns(2).spacing([10.0, 3.0]).show(ui, |ui| {
                                        for (k, d) in rows {
                                            ui.label(
                                                RichText::new(k)
                                                    .font(theme::medium(11.5))
                                                    .color(p.secondary),
                                            );
                                            ui.label(RichText::new(*d).size(12.0));
                                            ui.end_row();
                                        }
                                    });
                                    ui.add_space(10.0);
                                }
                            });
                        });
                    ui.label(
                        RichText::new("Right-drag or Space-drag pans · wheel pans · ⌘/Ctrl+wheel or pinch zooms · ⌥-drag duplicates · hold ⌥ to measure")
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.show_help = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Options, ThemeChoice};

    /// Lay the toggle out at the top-left of a small screen and optionally click it.
    fn frame(ctx: &egui::Context, options: &mut Options, dirty: &mut bool, click: bool) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(120.0, 40.0),
            )),
            ..Default::default()
        };
        if click {
            let pos = egui::pos2(12.0, 12.0);
            input.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                });
            }
        }
        let mut out = ctx.run_ui(input, |ui| {
            ui.horizontal(|ui| theme_button(ui, options, dirty));
        });
        out.textures_delta.clear();
    }

    #[test]
    fn tooltip_display_text_truncates_long_text_cleanly() {
        let doc = Document::default();
        let mut n = Node::new(
            NodeId(1),
            NodeKind::Rect,
            crate::geom::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        // A short tooltip is shown verbatim.
        n.tooltip = "Short hint".into();
        assert_eq!(tooltip_display_text(&n.tooltip, &n, &doc), "Short hint");

        // A pasted paragraph is clipped on a character boundary and marked with
        // an ellipsis, so the popup cannot grow off the canvas.
        n.tooltip = "x".repeat(TOOLTIP_MAX_CHARS * 3);
        let clipped = tooltip_display_text(&n.tooltip, &n, &doc);
        assert_eq!(clipped.chars().count(), TOOLTIP_MAX_CHARS + 1, "{clipped}");
        assert!(clipped.ends_with('…'), "{clipped}");
    }

    #[test]
    fn tooltip_display_text_resolves_placeholders_before_truncating() {
        let mut doc = Document::default();
        doc.settings.metadata.insert("owner".into(), "Ada".into());
        let n = Node::new(
            NodeId(1),
            NodeKind::Rect,
            crate::geom::Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let text = tooltip_display_text("Owner: %owner%", &n, &doc);
        assert_eq!(text, "Owner: Ada", "{text}");
    }

    #[test]
    fn icon_buttons_expose_accessible_names() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(200.0, 100.0),
                )),
                ..Default::default()
            },
            |ui| {
                crate::theme::icon_button(ui, crate::theme::Icon::Undo, 26.0, false)
                    .on_hover_text("Undo");
            },
        );
        out.textures_delta.clear();
        let update = out
            .platform_output
            .accesskit_update
            .expect("icon button should register an accessibility node");
        let labels: Vec<String> = update
            .nodes
            .iter()
            .filter_map(|(_, node)| node.label().map(str::to_string))
            .collect();
        assert!(
            labels.iter().any(|label| label == "Undo"),
            "expected an Undo label, got {labels:?}"
        );
    }

    #[test]
    fn icon_buttons_take_keyboard_focus() {
        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200.0, 100.0),
            )),
            ..Default::default()
        };
        let mut first = ctx.run_ui(input(), |ui| {
            crate::theme::icon_button(ui, crate::theme::Icon::Undo, 26.0, false);
        });
        first.textures_delta.clear();
        let mut tab = input();
        tab.events.push(egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let mut second = ctx.run_ui(tab, |ui| {
            let r = crate::theme::icon_button(ui, crate::theme::Icon::Undo, 26.0, false);
            assert!(r.has_focus(), "Tab should focus the icon button");
        });
        second.textures_delta.clear();
    }

    #[test]
    fn canvas_context_menu_offers_shape_commands_for_a_selection() {
        let mut ed = Editor::default();
        let id = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                crate::geom::Rect::new(0.0, 0.0, 20.0, 20.0),
            ),
            None,
        );
        ed.selection = vec![id];
        let entries = context_entries(&ed);
        let actions: Vec<Action> = entries
            .iter()
            .filter_map(|e| match e {
                ContextEntry::Item(_, a) => Some(*a),
                ContextEntry::Separator => None,
            })
            .collect();
        assert!(actions.contains(&Action::EditText));
        assert!(actions.contains(&Action::Cut));
        assert!(actions.contains(&Action::Copy));
        assert!(actions.contains(&Action::Paste));
        assert!(actions.contains(&Action::Duplicate));
        assert!(actions.contains(&Action::Delete));
        assert!(actions.contains(&Action::ToFront));
        assert!(actions.contains(&Action::ToBack));
        assert!(!actions.contains(&Action::SelectAll));
    }

    #[test]
    fn canvas_context_menu_offers_canvas_commands_without_a_selection() {
        let ed = Editor::default();
        let entries = context_entries(&ed);
        let actions: Vec<Action> = entries
            .iter()
            .filter_map(|e| match e {
                ContextEntry::Item(_, a) => Some(*a),
                ContextEntry::Separator => None,
            })
            .collect();
        assert_eq!(
            actions,
            vec![
                Action::Paste,
                Action::SelectAll,
                Action::ZoomFit,
                Action::ToggleGrid
            ]
        );
    }

    #[test]
    fn context_menu_event_opens_the_popup_and_escape_closes_it() {
        let ctx = egui::Context::default();
        setup_style(&ctx);
        let mut ed = Editor::default();
        let mut st = UiState::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            ..Default::default()
        };
        ed.ui_events
            .push(crate::editor::commands::UiEvent::ContextMenu {
                pos: crate::geom::v2(20.0, 30.0),
            });
        let mut out = ctx.run_ui(input(), |ui| {
            draw(ui, &mut ed, &mut st);
        });
        out.textures_delta.clear();
        let opened = st.context_menu.expect("context menu should open");
        assert_eq!(opened.pos, crate::geom::v2(20.0, 30.0));

        ed.ui_events
            .push(crate::editor::commands::UiEvent::CloseMenu);
        let mut out = ctx.run_ui(input(), |ui| {
            draw(ui, &mut ed, &mut st);
        });
        out.textures_delta.clear();
        assert!(st.context_menu.is_none());
    }

    #[test]
    fn inspector_tabs_follow_selection_presence() {
        let doc = Document::default();
        let none: [NodeId; 0] = [];
        assert!(InspectorTab::Diagram.valid_for(false, &none, &doc));
        assert!(!InspectorTab::Diagram.valid_for(true, &none, &doc));
        assert!(InspectorTab::Style.valid_for(false, &none, &doc));
        assert!(InspectorTab::Style.valid_for(true, &none, &doc));
        assert!(!InspectorTab::Text.valid_for(false, &none, &doc));
        assert!(!InspectorTab::Arrange.valid_for(false, &none, &doc));
        assert_eq!(
            InspectorTab::first_for(false, &none, &doc),
            InspectorTab::Diagram
        );
        assert_eq!(
            InspectorTab::first_for(true, &none, &doc),
            InspectorTab::Style
        );
        // The Style tab is shared by global and element contexts.
        assert!(
            InspectorTab::Style.valid_for(false, &none, &doc)
                && InspectorTab::Style.valid_for(true, &none, &doc)
        );
    }

    #[test]
    fn inspector_element_tabs_depend_on_text_capability() {
        use crate::geom::Rect;
        let mut doc = Document::default();
        let text_id = NodeId(1);
        let group_id = NodeId(2);
        let connector_id = NodeId(3);
        let mut text = Node::new(
            text_id,
            NodeKind::Text {
                text: "Hi".into(),
                font_size: 14.0,
                auto_width: true,
                align: crate::model::TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 40.0, 20.0),
        );
        text.id = text_id;
        let mut group = Node::new(group_id, NodeKind::Group, Rect::new(0.0, 0.0, 10.0, 10.0));
        group.id = group_id;
        let mut conn = Node::new(
            connector_id,
            NodeKind::Connector {
                points: vec![crate::geom::v2(0.0, 0.0), crate::geom::v2(10.0, 10.0)],
                start: None,
                end: None,
                route: crate::model::Route::Straight,
                start_head: crate::model::Head::None,
                end_head: crate::model::Head::Arrow,
                jump: crate::model::LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        conn.id = connector_id;
        doc.nodes.insert(text_id, text);
        doc.nodes.insert(group_id, group);
        doc.nodes.insert(connector_id, conn);

        assert_eq!(
            InspectorTab::element_tabs(&[text_id], &doc),
            vec![
                InspectorTab::Style,
                InspectorTab::Text,
                InspectorTab::Arrange,
                InspectorTab::Data
            ]
        );
        assert_eq!(
            InspectorTab::element_tabs(&[group_id], &doc),
            vec![
                InspectorTab::Style,
                InspectorTab::Arrange,
                InspectorTab::Data
            ]
        );
        // An unlabeled connector gets a different tab set than a text layer;
        // labeled connectors also get a different tab set than a text layer.
        assert_eq!(
            InspectorTab::element_tabs(&[connector_id], &doc),
            vec![InspectorTab::Style, InspectorTab::Arrange]
        );
        let mut labeled_conn = doc.get(connector_id).cloned().unwrap();
        labeled_conn.label = "Link".into();
        doc.nodes.insert(connector_id, labeled_conn);
        let labeled_tabs = InspectorTab::element_tabs(&[connector_id], &doc);
        assert_eq!(
            labeled_tabs,
            vec![
                InspectorTab::Style,
                InspectorTab::Text,
                InspectorTab::Arrange
            ]
        );
        assert_ne!(
            labeled_tabs,
            InspectorTab::element_tabs(&[text_id], &doc),
            "labeled connector and text layer should get different tab sets"
        );
        assert!(!InspectorTab::Text.valid_for(true, &[group_id], &doc));
    }

    #[test]
    fn connector_without_label_has_different_tab_set_than_text_layer() {
        use crate::geom::Rect;
        let mut doc = Document::default();
        let text_id = NodeId(10);
        let conn_id = NodeId(11);
        let mut text = Node::new(
            text_id,
            NodeKind::Text {
                text: "Label".into(),
                font_size: 14.0,
                auto_width: true,
                align: crate::model::TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 40.0, 20.0),
        );
        text.id = text_id;
        let mut conn = Node::new(
            conn_id,
            NodeKind::Connector {
                points: vec![crate::geom::v2(0.0, 0.0), crate::geom::v2(10.0, 10.0)],
                start: None,
                end: None,
                route: crate::model::Route::Straight,
                start_head: crate::model::Head::None,
                end_head: crate::model::Head::Arrow,
                jump: crate::model::LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        conn.id = conn_id;
        doc.nodes.insert(text_id, text);
        doc.nodes.insert(conn_id, conn);

        let text_tabs = InspectorTab::element_tabs(&[text_id], &doc);
        let conn_tabs = InspectorTab::element_tabs(&[conn_id], &doc);
        assert_ne!(
            text_tabs, conn_tabs,
            "connector and text layer should get different tab sets"
        );
        assert!(text_tabs.contains(&InspectorTab::Text));
        assert!(!conn_tabs.contains(&InspectorTab::Text));
    }

    #[test]
    fn welcome_steps_advance_and_finish_once() {
        let mut st = UiState {
            welcome: Some(WelcomeUi::default()),
            ..UiState::default()
        };
        assert_eq!(st.welcome.as_ref().unwrap().step, 0);
        assert_eq!(welcome_copy(0).0, "drawio-rs");
        advance_welcome(&mut st);
        assert_eq!(st.welcome.as_ref().unwrap().step, 1);
        assert_eq!(welcome_copy(1).0, "Connect");
        advance_welcome(&mut st);
        assert_eq!(st.welcome.as_ref().unwrap().step, 2);
        assert_eq!(welcome_copy(2).0, "Export");
        assert!(!st.options.onboarding_seen);
        advance_welcome(&mut st);
        assert!(st.welcome.is_none());
        assert!(st.options.onboarding_seen);
        assert!(st.options_dirty);
    }

    #[test]
    fn welcome_body_lines_do_not_overlap() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut height = 0.0;
        let mut out = ctx.run_ui(input, |ui| {
            let galley = egui::WidgetText::from(welcome_body(0)).into_galley(
                ui,
                Some(egui::TextWrapMode::Wrap),
                480.0,
                egui::FontSelection::default(),
            );
            height = galley.rect.height();
        });
        out.textures_delta.clear();
        assert!(height >= 30.0, "body line height collapsed to {height}px");
    }

    #[test]
    fn theme_button_flips_between_light_and_dark() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut options = Options::default();
        let mut dirty = false;

        frame(&ctx, &mut options, &mut dirty, false);
        assert_eq!(ctx.theme(), egui::Theme::Dark, "no system theme: dark");

        frame(&ctx, &mut options, &mut dirty, true);
        assert_eq!(options.theme, ThemeChoice::Light);
        assert!(dirty, "the choice is written to options.json");
        assert_eq!(ctx.theme(), egui::Theme::Light);

        dirty = false;
        frame(&ctx, &mut options, &mut dirty, false);
        frame(&ctx, &mut options, &mut dirty, true);
        assert_eq!(options.theme, ThemeChoice::Dark);
        assert!(dirty);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
    }
}
