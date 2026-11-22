//! The Library tab: a searchable shape registry (ADR-010), the scratchpad of
//! pinned and recent shapes, and personal library items. Everything shares one
//! tile: click to insert, drag out onto the canvas to place it, with vector
//! previews tessellated the same way the canvas is.

use super::UiState;
use crate::editor::Editor;
use crate::geom::{Rect, V2, v2};
use crate::library::{LibItem, Library, Scratchpad};
use crate::model::{DefaultStyle, Node, NodeId};
use crate::registry::{self, ShapeDef};
use crate::render::draw;
use crate::render::tess::Tess;
use crate::theme::{self, Icon, Palette};
use egui::{Color32, RichText, Ui};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

#[derive(Default)]
pub struct LibraryUi {
    pub library: Library,
    /// The library changed and should be saved.
    pub dirty: bool,
    pub renaming: Option<(String, String)>,
    /// The scratchpad: pinned shapes first, then recently used ones.
    pub scratch: Scratchpad,
    /// The scratchpad changed and should be saved.
    pub scratch_dirty: bool,
    /// The registry search text.
    pub query: String,
    /// A shape being dragged out to the canvas.
    pub drag: Option<DragSource>,
    previews: HashMap<String, (usize, egui::Mesh)>,
}

/// What a drag carries: a registry entry, or a saved item (scratchpad or library).
#[derive(Clone)]
pub enum DragSource {
    Registry(&'static ShapeDef),
    /// Scratchpad and library items insert through `insert_library_item`.
    Item(LibItem),
}

const TILE: f32 = 92.0;

/// Tessellate an item into an egui mesh fitted in a `size` box at the origin.
pub(crate) fn preview_mesh(item: &LibItem, size: f32) -> egui::Mesh {
    let doc = item.doc();
    let mut mesh = egui::Mesh::default();
    let Some(b) = item.bounds() else { return mesh };
    let s = ((size as f64 - 12.0) / b.w.max(b.h).max(1.0)).min(1.0);
    let mut tess = Tess::new(s.max(0.05));
    for id in doc.paint_order() {
        let (Some(n), Some(wr)) = (doc.get(id), doc.world_rect(id)) else {
            continue;
        };
        if !n.visible {
            continue;
        }
        let d = draw::node_draw_with(
            n,
            wr,
            draw::inherited_opacity(&doc, id) * n.style.opacity,
            doc.settings.shadow,
            doc.settings.sketch,
            doc.settings.auto_fill(),
            &[],
            false,
            0.0,
        );
        for piece in tess.ops(&d.ops) {
            let base = mesh.vertices.len() as u32;
            for v in &piece.mesh.vertices {
                let x = ((v.pos[0] as f64 - b.x) * s) as f32 + (size - (b.w * s) as f32) * 0.5;
                let y = ((v.pos[1] as f64 - b.y) * s) as f32 + (size - (b.h * s) as f32) * 0.5;
                let c = if v.kind == 0 {
                    v.color
                } else {
                    // Gradients and images preview as a neutral tone.
                    [0.75, 0.75, 0.8, v.color[3]]
                };
                let to8 = |f: f32| (f.clamp(0.0, 1.0) * 255.0).round() as u8;
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: egui::pos2(x, y),
                    uv: egui::epaint::WHITE_UV,
                    color: Color32::from_rgba_unmultiplied(
                        to8(c[0]),
                        to8(c[1]),
                        to8(c[2]),
                        to8(c[3]),
                    ),
                });
            }
            mesh.indices
                .extend(piece.mesh.indices.iter().map(|i| i + base));
        }
    }
    mesh
}

/// A fingerprint of the styling a preview depends on, so previews follow the
/// diagram's default style without rebuilding every frame.
fn node_fp(n: &Node) -> usize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!(
        "{:?}",
        (
            n.style.fills.clone(),
            n.style.stroke,
            n.style.stroke_width,
            n.style.corner_radius,
            n.style.dash.clone()
        )
    )
    .hash(&mut h);
    h.finish() as usize
}

/// The fingerprint of a whole item: every node's look folded together.
fn item_fp(item: &LibItem) -> usize {
    item.nodes
        .iter()
        .map(node_fp)
        .fold(0usize, |a, x| a.wrapping_add(x))
}

/// The cached preview for any tile source, keyed and fingerprinted.
fn cached_preview(
    lib: &mut LibraryUi,
    id: &str,
    fp: usize,
    make: impl FnOnce() -> egui::Mesh,
) -> egui::Mesh {
    let key = format!("{}|{}", id, fp);
    let entry = lib
        .previews
        .entry(key)
        .or_insert_with(|| (usize::MAX, egui::Mesh::default()));
    if entry.0 != fp {
        *entry = (fp, make());
    }
    entry.1.clone()
}

/// A registry entry as a one-node item. Plain shapes take the page's default
/// style here, so previews match what an insert produces.
pub fn def_item(def: &ShapeDef, ds: Option<&DefaultStyle>) -> LibItem {
    let (w, h) = def.size;
    let mut n = registry::build(def, Rect::new(0.0, 0.0, w, h), ds);
    n.id = NodeId(1);
    LibItem {
        id: format!("reg-{}", def.id),
        name: def.name.into(),
        nodes: vec![n],
        roots: vec![NodeId(1)],
    }
}

/// One preview tile: mesh centered in a rounded box, with hover, click and
/// drag sense. Returns the tile rect and its response; starting the drag and
/// the actions stay with the caller, which knows the source.
fn tile(
    ui: &mut Ui,
    p: &Palette,
    lib: &mut LibraryUi,
    id: &str,
    fp: usize,
    make: impl FnOnce() -> egui::Mesh,
) -> (egui::Rect, egui::Response) {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(TILE, TILE), egui::Sense::click_and_drag());
    let bg = if resp.dragged() || resp.hovered() {
        p.hover
    } else {
        p.control
    };
    ui.painter().rect_filled(rect, 8.0, bg);
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, p.separator),
        egui::StrokeKind::Inside,
    );
    let mut m = cached_preview(lib, id, fp, make);
    m.translate(rect.min.to_vec2());
    ui.painter().add(egui::Shape::mesh(m));
    (rect, resp)
}

/// Insert a dragged or clicked source at `at` (None = the viewport center).
fn insert_source(
    ed: &mut Editor,
    lib: &mut LibraryUi,
    src: &DragSource,
    at: Option<V2>,
    ds: Option<&DefaultStyle>,
) {
    let center = ed.camera.screen_to_world(ed.viewport * 0.5);
    match src {
        DragSource::Registry(def) => {
            ed.insert_registry_shape(def.id, Some(at.unwrap_or(center)), None, None, None);
            let item = def_item(def, ds);
            lib.scratch.add(item, false);
            lib.scratch_dirty = true;
        }
        DragSource::Item(it) => {
            ed.insert_library_item(it, at.unwrap_or(center));
        }
    }
}

/// Import a draw.io `<mxlibrary>` file into the personal library. Kept separate
/// from the button so the file operation can be tested without a native dialog.
pub fn import_xml_file(st: &mut LibraryUi, ed: &mut Editor, path: &std::path::Path) {
    let result = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| crate::io::library_xml::import_library(&text));
    match result {
        Ok(items) => {
            let n = items.len();
            st.library.items.extend(items);
            st.dirty = true;
            ed.status = format!("Imported {n} shape(s) from {}", path.display());
        }
        Err(e) => ed.status = format!("Library import failed: {e}"),
    }
}

/// Export the personal library as a draw.io `<mxlibrary>` file.
pub fn export_xml_file(st: &LibraryUi, ed: &mut Editor, path: &std::path::Path) {
    let xml = crate::io::library_xml::export_library(&st.library.items);
    match std::fs::write(path, xml) {
        Ok(()) => {
            ed.status = format!(
                "Exported {} shape(s) to {}",
                st.library.items.len(),
                path.display()
            )
        }
        Err(e) => ed.status = format!("Library export failed: {e}"),
    }
}

pub fn panel(ui: &mut Ui, ed: &mut Editor, st: &mut UiState) {
    let p = theme::pal(ui);
    let ctx = ui.ctx().clone();
    let center = ed.camera.screen_to_world(ed.viewport * 0.5);
    let ds = ed.doc.settings.shape_default.clone();
    let lib = &mut st.library;

    // ---------------------------------------------------------------- search
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.add(
            egui::TextEdit::singleline(&mut lib.query)
                .hint_text("Search shapes")
                .desired_width(ui.available_width() - 24.0),
        );
        if ui
            .small_button("×")
            .on_hover_text("Clear the search")
            .clicked()
        {
            lib.query.clear();
        }
    });
    ui.add_space(4.0);
    if ui
        .button("Templates…")
        .on_hover_text("Browse diagram packs and custom template libraries")
        .clicked()
    {
        st.templates.open = true;
        if st.templates.libraries.is_empty() {
            st.templates.load(st.options.template_folder.clone());
        }
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        if ui
            .button("Import XML…")
            .on_hover_text("Import a draw.io custom shape library (.xml)")
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Shape library", &["xml"])
                .pick_file()
        {
            import_xml_file(lib, ed, &path);
        }
        if ui
            .button("Export XML…")
            .on_hover_text("Export the personal library as a draw.io shape library (.xml)")
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .add_filter("Shape library", &["xml"])
                .save_file()
        {
            export_xml_file(lib, ed, &path);
        }
    });
    ui.add_space(4.0);

    egui::ScrollArea::vertical()
        .id_salt("library")
        .auto_shrink(false)
        .show(ui, |ui| {
            // ---------------------------------------------------- scratchpad
            egui::CollapsingHeader::new("Scratchpad")
                .default_open(true)
                .show(ui, |ui| {
                    let can_add = !ed.top_selection().is_empty();
                    if ui
                        .add_enabled(can_add, egui::Button::new("+ Add selection"))
                        .on_hover_text("Add the selection to the scratchpad")
                        .clicked()
                    {
                        let ids = ed.top_selection();
                        let name = ids
                            .first()
                            .and_then(|id| ed.doc.get(*id))
                            .map_or("Item".to_string(), |n| n.name.clone());
                        if let Some(item) = LibItem::from_selection(&ed.doc, &ids, &name) {
                            lib.scratch.add(item, false);
                            lib.scratch_dirty = true;
                        }
                    }
                    if lib.scratch.items.is_empty() {
                        ui.label(
                            RichText::new("Pinned and recently used shapes appear here.")
                                .size(theme::SECONDARY)
                                .color(p.secondary),
                        );
                    }
                    let scratch = lib.scratch.items.clone();
                    for (row_i, row) in scratch.chunks(4).enumerate() {
                        if row_i > 0 {
                            ui.add_space(4.0);
                        }
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for it in row {
                                let id = it.item.id.clone();
                                let fp = item_fp(&it.item);
                                let pinned = it.pinned;
                                let item = it.item.clone();
                                let (_, resp) =
                                    tile(ui, &p, lib, &id, fp, || preview_mesh(&item, TILE));
                                let started = resp.drag_started();
                                let clicked = resp.clicked();
                                let hover = format!(
                                    "{} — drag to the canvas, or click to insert{}",
                                    item.name,
                                    if pinned { " (pinned)" } else { "" }
                                );
                                resp.on_hover_text(hover).context_menu(|ui| {
                                    if ui.button("Insert").clicked() {
                                        ed.insert_library_item(&item, center);
                                        ui.close();
                                    }
                                    if ui.button(if pinned { "Unpin" } else { "Pin" }).clicked() {
                                        lib.scratch.set_pinned(&item.id, !pinned);
                                        lib.scratch_dirty = true;
                                        ui.close();
                                    }
                                    if ui.button("Remove").clicked() {
                                        lib.scratch.remove(&item.id);
                                        lib.scratch_dirty = true;
                                        ui.close();
                                    }
                                });
                                if started {
                                    lib.drag = Some(DragSource::Item(item.clone()));
                                }
                                if clicked {
                                    ed.insert_library_item(&item, center);
                                }
                            }
                        });
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // ------------------------------------------------- registry grid
            ui.label(
                RichText::new("Shapes")
                    .size(theme::SECONDARY)
                    .color(p.secondary),
            );
            let query = lib.query.trim().to_string();
            let cols = ((ui.available_width() + 6.0) / (TILE + 6.0))
                .floor()
                .max(1.0) as usize;
            let draw_shape_tile = |ui: &mut Ui,
                                   lib: &mut LibraryUi,
                                   ed: &mut Editor,
                                   def: &'static ShapeDef,
                                   ds: Option<&DefaultStyle>| {
                let it = def_item(def, ds);
                let fp = node_fp(&it.nodes[0]);
                let id = it.id.clone();
                let insert_item = it.clone();
                let (_, resp) = tile(ui, &p, lib, &id, fp, || preview_mesh(&insert_item, TILE));
                if resp.drag_started() {
                    lib.drag = Some(DragSource::Registry(def));
                }
                if resp.clicked() {
                    insert_source(ed, lib, &DragSource::Registry(def), None, ds);
                }
                resp.on_hover_text(format!(
                    "{} · {} — drag to the canvas, or click to insert",
                    def.name,
                    def.category.name()
                ))
                .context_menu(|ui| {
                    if ui.button("Insert").clicked() {
                        insert_source(ed, lib, &DragSource::Registry(def), None, ds);
                        ui.close();
                    }
                    if ui.button("Pin to scratchpad").clicked() {
                        lib.scratch.add(def_item(def, ds), true);
                        lib.scratch_dirty = true;
                        ui.close();
                    }
                    if ui.button("Add to library").clicked() {
                        lib.library.items.push(def_item(def, ds));
                        lib.dirty = true;
                        ui.close();
                    }
                });
            };
            if query.is_empty() {
                let all = registry::search("");
                let mut cats: Vec<crate::registry::Category> = Vec::new();
                for d in &all {
                    if !cats.contains(&d.category) {
                        cats.push(d.category);
                    }
                }
                for cat in cats {
                    let defs: Vec<&'static ShapeDef> =
                        all.iter().copied().filter(|d| d.category == cat).collect();
                    egui::CollapsingHeader::new(cat.name())
                        .default_open(cat == crate::registry::Category::Flowchart)
                        .show(ui, |ui| {
                            for (row_i, row) in defs.chunks(cols).enumerate() {
                                if row_i > 0 {
                                    ui.add_space(6.0);
                                }
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for def in row {
                                        draw_shape_tile(ui, lib, ed, def, ds.as_ref());
                                    }
                                });
                            }
                        });
                    ui.add_space(2.0);
                }
            } else {
                let results = registry::search(&query);
                if results.is_empty() {
                    ui.label(
                        RichText::new("No shapes match the search.")
                            .size(theme::SECONDARY)
                            .color(p.secondary),
                    );
                }
                ui.add_space(4.0);
                for (row_i, row) in results.chunks(cols).enumerate() {
                    if row_i > 0 {
                        ui.add_space(6.0);
                    }
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for def in row {
                            draw_shape_tile(ui, lib, ed, def, ds.as_ref());
                        }
                    });
                }
            }

            ui.add_space(8.0);
            if ui
                .button("More Shapes...")
                .on_hover_text("Browse additional diagram templates and shape packs")
                .clicked()
            {
                st.templates.open = true;
                if st.templates.libraries.is_empty() {
                    st.templates.load(st.options.template_folder.clone());
                }
            }
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // ----------------------------------------------------- my library
            ui.label(
                RichText::new("My library")
                    .size(theme::SECONDARY)
                    .color(p.secondary),
            );
            let can_add = !ed.top_selection().is_empty();
            if ui
                .add_enabled(can_add, egui::Button::new("+ Add selection"))
                .on_hover_text("Save the selected layers as a reusable item")
                .clicked()
            {
                let ids = ed.top_selection();
                let name = ids
                    .first()
                    .and_then(|id| ed.doc.get(*id))
                    .map_or("Item".to_string(), |n| n.name.clone());
                if let Some(item) = LibItem::from_selection(&ed.doc, &ids, &name) {
                    lib.library.items.push(item);
                    lib.dirty = true;
                }
            }
            if lib.library.items.is_empty() {
                ui.label(
                    RichText::new("Select layers and choose\n+ Add selection to keep them here.")
                        .size(theme::SECONDARY)
                        .color(p.secondary),
                );
            }
            ui.add_space(4.0);
            let items = lib.library.items.clone();
            let mut remove = None;
            let mut to_scratch = None;
            for (row_i, row) in items.chunks(cols).enumerate() {
                if row_i > 0 {
                    ui.add_space(6.0);
                }
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for it in row {
                        let fp = item_fp(it);
                        let (rect, resp) = tile(ui, &p, lib, &it.id, fp, || preview_mesh(it, TILE));
                        let renaming = lib.renaming.as_ref().is_some_and(|(id, _)| *id == it.id);
                        if renaming {
                            let (_, name) = lib.renaming.as_mut().unwrap();
                            let r = ui.add(egui::TextEdit::singleline(name).desired_width(TILE));
                            r.request_focus();
                            if r.lost_focus() {
                                let (id, name) = lib.renaming.take().unwrap();
                                if let Some(x) = lib.library.items.iter_mut().find(|x| x.id == id) {
                                    x.name = name;
                                    lib.dirty = true;
                                }
                            }
                        } else {
                            ui.add(
                                egui::Label::new(RichText::new(&it.name).size(theme::SECONDARY))
                                    .truncate(),
                            );
                        }
                        if resp.drag_started() {
                            lib.drag = Some(DragSource::Item(it.clone()));
                        }
                        if resp.clicked() {
                            ed.insert_library_item(it, center);
                        }
                        resp.on_hover_text("Click to insert, drag to place")
                            .context_menu(|ui| {
                                if ui.button("Insert").clicked() {
                                    ed.insert_library_item(it, center);
                                    ui.close();
                                }
                                if ui.button("Rename").clicked() {
                                    lib.renaming = Some((it.id.clone(), it.name.clone()));
                                    ui.close();
                                }
                                if ui.button("Copy to scratchpad").clicked() {
                                    to_scratch = Some(it.clone());
                                    ui.close();
                                }
                                if ui.button("Remove from library").clicked() {
                                    remove = Some(it.id.clone());
                                    ui.close();
                                }
                            });
                        let _ = rect;
                    }
                });
            }
            if let Some(id) = remove {
                lib.library.items.retain(|x| x.id != id);
                lib.dirty = true;
            }
            if let Some(it) = to_scratch {
                lib.scratch.add(it, false);
                lib.scratch_dirty = true;
            }
        });

    // ------------------------------------------------------------ drag out
    // The pointer is tracked from here each frame while a drag runs, so the
    // drop can land anywhere on the canvas, past the egui widgets.
    if let Some(src) = st.library.drag.clone() {
        let (down, released, pos) = ctx.input(|i| {
            (
                i.pointer.primary_down(),
                i.pointer.any_released(),
                i.pointer.latest_pos(),
            )
        });
        let over_ui = ctx.is_pointer_over_egui();
        let on_canvas = st
            .canvas_rect
            .is_some_and(|cr| pos.is_some_and(|pt| cr.contains(pt)))
            && !over_ui;
        if released || !down {
            if on_canvas
                && let Some(pt) = pos
                && let Some(cr) = st.canvas_rect
            {
                let world =
                    ed.screen_to_world(v2((pt.x - cr.min.x) as f64, (pt.y - cr.min.y) as f64));
                insert_source(ed, &mut st.library, &src, Some(world), ds.as_ref());
            }
            st.library.drag = None;
        } else if on_canvas && let Some(pt) = pos {
            // Ghost preview under the pointer while it is over the diagram.
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Foreground,
                egui::Id::from("library-drag-ghost"),
            ));
            painter.rect_filled(
                egui::Rect::from_center_size(pt, egui::vec2(TILE, TILE)),
                8.0,
                Color32::from_rgba_unmultiplied(0, 0, 0, 12),
            );
            let (mesh, rect) = drag_ghost(st, &src, ds.as_ref(), pt);
            let mut m = mesh;
            m.translate(rect.min.to_vec2());
            painter.add(egui::Shape::mesh(m));
            painter.rect_stroke(
                rect,
                8.0,
                egui::Stroke::new(1.0, theme::accent()),
                egui::StrokeKind::Inside,
            );
        }
    }
    let _ = Icon::Library;
}

// The pointer left the window mid-drag (LMB held outside the app) is covered
// by `!down` in the caller; nothing extra is needed.

/// The ghost preview for a drag: positioned just above the pointer, centered
/// on the drop point.
fn drag_ghost(
    st: &mut UiState,
    src: &DragSource,
    ds: Option<&DefaultStyle>,
    pt: egui::Pos2,
) -> (egui::Mesh, egui::Rect) {
    let crect = egui::Rect::from_center_size(pt, egui::vec2(TILE, TILE));
    let mesh = match src {
        DragSource::Registry(def) => {
            let def = *def;
            let d = ds.cloned();
            cached_preview(
                &mut st.library,
                def.id,
                node_fp(&def_item(def, d.as_ref()).nodes[0]),
                || preview_mesh(&def_item(def, d.as_ref()), TILE),
            )
        }
        DragSource::Item(it) => {
            let it = it.clone();
            let fp = item_fp(&it);
            let id = it.id.clone();
            cached_preview(&mut st.library, &id, fp, || preview_mesh(&it, TILE))
        }
    };
    (mesh, crect)
}
