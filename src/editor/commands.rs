//! Smaller editing commands: sibling navigation, flipping, copy/paste of properties, and
//! UI-level requests the chrome handles (palette, rename, hide UI).

use super::Editor;
use crate::geom::{Rect, V2};
use crate::history::Tx;
use crate::model::{
    Color, Node, NodeId, NodeKind, Paint, Scheme, SchemeReport, SchemeScope, Style, TextStyle,
};
use std::collections::HashSet;

/// Repaint one paint layer with a scheme color. Image layers stay as they are.
/// A layer that carried an explicit dark color keeps that value after the restyle.
fn scheme_paint(old: &Paint, light: Color) -> Paint {
    match old {
        Paint::Image { .. } => old.clone(),
        Paint::Solid { dark, .. } => {
            if let Some(dark) = dark {
                Paint::solid_pair(light, *dark)
            } else {
                Paint::solid(light)
            }
        }
        Paint::Linear { stops, from, to } => Paint::Linear {
            stops: stops
                .iter()
                .map(|s| crate::model::Stop {
                    pos: s.pos,
                    color: light,
                    dark: s.dark,
                })
                .collect(),
            from: *from,
            to: *to,
        },
        Paint::Radial {
            stops,
            center,
            radius,
        } => Paint::Radial {
            stops: stops
                .iter()
                .map(|s| crate::model::Stop {
                    pos: s.pos,
                    color: light,
                    dark: s.dark,
                })
                .collect(),
            center: *center,
            radius: *radius,
        },
    }
}

fn scheme_stroke(style: &mut Style, light: Color) {
    style.stroke = Some(light);
    if style.stroke_dark.is_some() {
        style.stroke_dark = Some(light);
    }
    style.stroke_enabled = true;
}

fn scheme_label(node: &mut Node, light: Color) {
    node.label_color = Some(light);
    if node.label_color_dark.is_some() {
        node.label_color_dark = Some(light);
    }
}

fn scheme_fills(fills: &mut Vec<Paint>, light: Color) {
    if fills.is_empty() {
        fills.push(Paint::solid(light));
    } else {
        for paint in fills.iter_mut() {
            *paint = scheme_paint(paint, light);
        }
    }
}

fn apply_scheme_node(node: &mut Node, scheme: &Scheme, page_scope: bool) {
    if matches!(node.kind, NodeKind::Group) {
        return;
    }
    if matches!(
        node.kind,
        NodeKind::Connector { .. } | NodeKind::Stroke { .. } | NodeKind::Line { .. }
    ) {
        scheme_stroke(&mut node.style, scheme.line);
        return;
    }
    if matches!(node.kind, NodeKind::Path { .. }) && node.style.fills.is_empty() {
        scheme_stroke(&mut node.style, scheme.line);
        return;
    }
    if matches!(node.kind, NodeKind::Text { .. }) {
        scheme_fills(&mut node.style.fills, scheme.font);
        scheme_label(node, scheme.font);
        return;
    }
    if !(page_scope && node.style.fill_auto) {
        scheme_fills(&mut node.style.fills, scheme.fill);
        node.style.fill_enabled = true;
    }
    scheme_stroke(&mut node.style, scheme.line);
    scheme_label(node, scheme.font);
    if node.is_frame() {
        node.style.fills.clear();
    }
}

/// Requests for the egui chrome (consumed by `ui::draw`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UiEvent {
    TogglePalette,
    Rename(NodeId),
    ToggleUi,
    ToggleHelp,
    ToggleGrid,
    ToggleInspector,
    /// Double-click on empty canvas: open the quick-add picker at that spot.
    QuickAdd {
        world: crate::geom::V2,
    },
    /// Open the insert-diagram dialog for this source (M8).
    Generate(crate::generate::Source),
    /// Show the find bar and focus its field.
    OpenFind,
    /// Step to the next or previous find match.
    FindStep(bool),
    /// Open the canvas context menu at a canvas-local position.
    ContextMenu {
        pos: crate::geom::V2,
    },
    /// Dismiss the canvas context menu without running an action.
    CloseMenu,
}

impl Editor {
    /// Autosize (draw.io SHP-17): shrink containers to their contents,
    /// deepest container first. Auto-layout frames lay out through layout;
    /// plain groups and frames hug the union of their children, growing from
    /// the children's top-left so child positions stay put.
    pub fn autosize_selection(&mut self) {
        let targets: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| {
                self.doc
                    .get(*id)
                    .is_some_and(|n| n.is_group() || n.is_frame())
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        self.edit("Autosize", |tx| {
            for &root in &targets {
                autosize_subtree(tx, root);
            }
        });
    }

    /// Tab / Shift+Tab: the next or previous sibling of the selection (in paint order).
    pub fn select_sibling(&mut self, forward: bool) {
        let first = self.selection.first().copied();
        let parent = first.and_then(|id| self.doc.get(id)).and_then(|n| n.parent);
        let list: Vec<NodeId> = self
            .doc
            .children(parent)
            .iter()
            .copied()
            .filter(|id| self.doc.get(*id).is_some_and(|n| n.visible))
            .collect();
        if list.is_empty() {
            return;
        }
        let next = match first.and_then(|f| list.iter().position(|x| *x == f)) {
            Some(i) if forward => list[(i + list.len() - 1) % list.len()],
            Some(i) => list[(i + 1) % list.len()],
            None => *list.last().unwrap(),
        };
        self.end_text_edit();
        self.selection = vec![next];
    }

    pub fn select_parent(&mut self) {
        let parents: Vec<NodeId> = self
            .selection
            .iter()
            .filter_map(|id| self.doc.get(*id).and_then(|n| n.parent))
            .collect();
        if !parents.is_empty() {
            self.selection = super::dedup(parents);
        }
    }

    /// Mirror every selected element around its own center. Bound connectors
    /// keep their bindings; only the element's own geometry changes.
    pub fn flip(&mut self, axis: usize) {
        let ids: Vec<NodeId> = self
            .top_selection()
            .into_iter()
            .filter(|id| !self.doc.is_effectively_locked(*id))
            .collect();
        if ids.is_empty() {
            return;
        }
        self.edit(
            if axis == 0 {
                "Flip horizontal"
            } else {
                "Flip vertical"
            },
            |tx| {
                for id in ids {
                    flip_node(tx, id, axis);
                }
            },
        );
    }

    /// Set the clockwise rotation of every selected element, in degrees.
    pub fn set_rotation(&mut self, ids: &[NodeId], degrees: f64) {
        if !degrees.is_finite() {
            return;
        }
        let value = normalize_angle(degrees);
        self.edit_coalescing("Rotate", "angle", |tx| {
            for id in ids {
                if let Some(n) = tx.node_mut(*id) {
                    n.style.rotation = value;
                }
            }
        });
    }

    /// draw.io's "Rotate shape only by 90°": add a quarter turn per click and
    /// keep the label horizontal.
    pub fn rotate_90(&mut self, ids: &[NodeId]) {
        self.edit("Rotate 90°", |tx| {
            for id in ids {
                let Some(n) = tx.node_mut(*id) else { continue };
                n.style.rotation = normalize_angle(n.style.rotation + 90.0);
                n.style.label_upright = true;
            }
        });
    }

    /// Remember a selection's style for new shapes or connectors. Only the
    /// matching type's default changes; existing elements stay untouched.
    pub fn set_default_style(&mut self) {
        let Some(id) = self.top_selection().first().copied() else {
            return;
        };
        let Some(n) = self.doc.get(id) else { return };
        let ds = crate::model::DefaultStyle::new(n.style.clone(), n.text_style.clone());
        let connector = matches!(n.kind, NodeKind::Connector { .. });
        self.edit_settings(
            if connector {
                "Set connector default style"
            } else {
                "Set shape default style"
            },
            |s| {
                if connector {
                    s.connector_default = Some(ds);
                } else {
                    s.shape_default = Some(ds);
                }
            },
        );
        self.status = if connector {
            "Default connector style set"
        } else {
            "Default shape style set"
        }
        .into();
    }

    pub fn clear_default_styles(&mut self) {
        self.edit_settings("Clear default styles", |s| {
            s.shape_default = None;
            s.connector_default = None;
        });
        self.status = "Default styles cleared".into();
    }

    /// Style for a connector started at `source`: the shape's own source
    /// default first, then the page connector default.
    pub(super) fn connector_default_for(
        &self,
        source: Option<NodeId>,
    ) -> Option<crate::model::DefaultStyle> {
        source
            .and_then(|id| self.doc.get(id))
            .filter(|n| n.can_have_connector_default())
            .and_then(|n| n.connector_default.clone())
            .or_else(|| self.doc.settings.connector_default.clone())
    }

    /// Copy each selected connector's style onto the shape its line starts
    /// from. Existing connectors keep their own style; only future ones drawn
    /// from that shape take the stored default (CON-19).
    pub fn set_connector_default_from_selection(&mut self) {
        let mut updates: Vec<(NodeId, crate::model::DefaultStyle)> = Vec::new();
        for id in self.top_selection() {
            let Some(n) = self.doc.get(id) else { continue };
            let NodeKind::Connector {
                start: Some(binding),
                ..
            } = n.kind
            else {
                continue;
            };
            let source = binding.node;
            let Some(target) = self.doc.get(source) else {
                continue;
            };
            if !target.can_have_connector_default()
                || self.doc.is_effectively_locked(source)
                || self.doc.is_ancestor(id, source)
                || updates.iter().any(|(id, _)| *id == source)
            {
                continue;
            }
            updates.push((
                source,
                crate::model::DefaultStyle::new(n.style.clone(), n.text_style.clone()),
            ));
        }
        if updates.is_empty() {
            return;
        }
        let label = if updates.len() == 1 {
            "Set source connector default"
        } else {
            "Set source connector defaults"
        };
        self.edit(label, |tx| {
            for (source, ds) in updates {
                if let Some(n) = tx.node_mut(source) {
                    n.connector_default = Some(ds);
                }
            }
        });
        self.status = "Source connector default set".into();
    }

    /// Copy the page connector default onto the selected shapes. This gives a
    /// shape-level override without needing a connector to copy from.
    pub fn set_connector_default_from_page(&mut self, ids: &[NodeId]) {
        let Some(ds) = self.doc.settings.connector_default.clone() else {
            self.status = "Set a page connector default first".into();
            return;
        };
        let targets: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc.get(*id).is_some_and(|n| {
                    n.can_have_connector_default() && !self.doc.is_effectively_locked(*id)
                })
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let label = if targets.len() == 1 {
            "Set source connector default"
        } else {
            "Set source connector defaults"
        };
        self.edit(label, |tx| {
            for id in targets {
                if let Some(n) = tx.node_mut(id) {
                    n.connector_default = Some(ds.clone());
                }
            }
        });
        self.status = "Source connector default set".into();
    }

    /// Remove the source connector default from the selected shapes.
    pub fn clear_connector_defaults(&mut self, ids: &[NodeId]) {
        let targets: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc.get(*id).is_some_and(|n| {
                    n.can_have_connector_default()
                        && n.connector_default.is_some()
                        && !self.doc.is_effectively_locked(*id)
                })
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        let label = if targets.len() == 1 {
            "Clear source connector default"
        } else {
            "Clear source connector defaults"
        };
        self.edit(label, |tx| {
            for id in targets {
                if let Some(n) = tx.node_mut(id) {
                    n.connector_default = None;
                }
            }
        });
        self.status = "Source connector default cleared".into();
    }

    fn scheme_plan(&self, scope: SchemeScope) -> (SchemeReport, Vec<NodeId>) {
        let mut report = SchemeReport::default();
        let mut changed = Vec::new();
        let selected: HashSet<NodeId> = if scope == SchemeScope::Selection {
            self.selection
                .iter()
                .flat_map(|id| self.doc.subtree(*id))
                .collect()
        } else {
            HashSet::new()
        };
        for id in self.doc.nodes.keys().copied() {
            if scope == SchemeScope::Selection && !selected.contains(&id) {
                report.out_of_scope += 1;
                continue;
            }
            if !self.doc.is_effectively_visible(id) {
                report.hidden += 1;
                continue;
            }
            if self.doc.is_effectively_locked(id) {
                report.locked += 1;
                continue;
            }
            if self
                .doc
                .get(id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Group))
            {
                report.unstyleable += 1;
                continue;
            }
            changed.push(id);
        }
        report.changed = changed.len();
        (report, changed)
    }

    /// Count what a scheme command would change without touching the document.
    pub fn scheme_preview(&self, scope: SchemeScope) -> SchemeReport {
        self.scheme_plan(scope).0
    }

    /// Apply a palette with an explicit scope. Locked, hidden and unstyleable
    /// elements are skipped and counted. Page scope also sets the page palette,
    /// background and grid; selection scope changes only selected subtrees.
    pub fn apply_scheme_scoped(&mut self, scheme: &Scheme, scope: SchemeScope) -> SchemeReport {
        let (report, ids) = self.scheme_plan(scope);
        let name = scheme.name;
        let label = if scope == SchemeScope::Page {
            format!("Apply {name} scheme")
        } else {
            format!("Apply {name} scheme to selection")
        };
        if ids.is_empty() && scope == SchemeScope::Selection {
            self.status = format!("{name} selection: {}", report.summary());
            return report;
        }
        self.edit(&label, |tx| {
            for id in ids {
                if let Some(node) = tx.node_mut(id) {
                    apply_scheme_node(node, scheme, scope == SchemeScope::Page);
                }
            }
            if scope == SchemeScope::Page {
                let s = tx.settings_mut();
                let had_dark = s.background_color_dark.is_some();
                s.scheme = Some(name.to_string());
                s.grid_color = Some(scheme.grid);
                s.background_enabled = true;
                s.background_color = scheme.page;
                if had_dark {
                    s.background_color_dark = Some(scheme.page);
                }
            }
        });
        self.status = format!(
            "{name} {}: {}",
            scope.label().to_lowercase(),
            report.summary()
        );
        report
    }

    /// Apply a palette to the whole page. Kept for callers that only need the
    /// default scope; the panel uses `apply_scheme_scoped`.
    pub fn apply_scheme(&mut self, scheme: &Scheme) {
        let _ = self.apply_scheme_scoped(scheme, SchemeScope::Page);
    }

    /// Global Rounded action: toggles every compatible shape at once and
    /// records the state so the button can show it as active.
    pub fn toggle_global_rounded(&mut self) {
        let on = !self.doc.settings.rounded;
        self.edit(
            if on {
                "Round corners"
            } else {
                "Square corners"
            },
            |tx| {
                let ids: Vec<NodeId> = tx.doc().nodes.keys().copied().collect();
                for id in ids {
                    let Some(n) = tx.node_mut(id) else { continue };
                    if matches!(
                        n.kind,
                        NodeKind::Rect
                            | NodeKind::Frame { .. }
                            | NodeKind::Polygon { .. }
                            | NodeKind::Star { .. }
                    ) {
                        n.style.corner_radius = if on { 8.0 } else { 0.0 };
                        n.style.radii = None;
                    }
                }
                tx.settings_mut().rounded = on;
            },
        );
    }

    pub fn set_global_sketch(&mut self, on: bool) {
        self.edit_settings("Toggle sketch", |s| s.sketch = on);
    }

    pub fn copy_props(&mut self) {
        if let Some(n) = self.selection.first().and_then(|id| self.doc.get(*id)) {
            self.clipboard_style = Some((n.style.clone(), n.text_style.clone()));
            self.status = "Copied properties".into();
        }
    }

    /// Whether a style was copied and can be pasted.
    pub fn has_style_clipboard(&self) -> bool {
        self.clipboard_style.is_some()
    }

    pub fn paste_props(&mut self) {
        let Some((style, ts)) = self.clipboard_style.clone() else {
            return;
        };
        let ids = self.top_selection();
        self.edit("Paste properties", |tx| {
            for id in &ids {
                let Some(n) = tx.node_mut(*id) else { continue };
                n.style = Style { ..style.clone() };
                if n.kind.text().is_some() {
                    n.text_style = TextStyle {
                        spans: Vec::new(),
                        ..ts.clone()
                    };
                }
            }
        });
        self.mark_overrides(&ids, "fills");
        for id in ids {
            self.fit_text(id);
        }
        self.commit();
    }
}

pub(crate) fn normalize_angle(mut a: f64) -> f64 {
    a %= 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    // Keep values tidy in the field and in saved files.
    (a * 1000.0).round() / 1000.0
}

fn mirror(v: &mut V2, w: f64, h: f64, axis: usize) {
    if axis == 0 {
        v.x = w - v.x;
    } else {
        v.y = h - v.y;
    }
}

/// Mirror a node's own geometry and its children inside its box.
fn flip_node(tx: &mut Tx, id: NodeId, axis: usize) {
    let Some(n) = tx.doc().get(id) else { return };
    let (w, h) = (n.rect.w, n.rect.h);
    let kids = n.children.clone();
    let node = tx.node_mut(id).unwrap();
    match &mut node.kind {
        NodeKind::Path { subpaths, .. } => {
            for sp in subpaths.iter_mut() {
                for p in sp.points.iter_mut() {
                    mirror(&mut p.p, w, h, axis);
                    if let Some(q) = &mut p.h_in {
                        mirror(q, w, h, axis);
                    }
                    if let Some(q) = &mut p.h_out {
                        mirror(q, w, h, axis);
                    }
                }
            }
        }
        NodeKind::Line { a, b } => {
            mirror(a, w, h, axis);
            mirror(b, w, h, axis);
        }
        NodeKind::Connector { points, .. } => {
            for p in points.iter_mut() {
                mirror(p, w, h, axis);
            }
        }
        NodeKind::Stroke { points, .. } => {
            for p in points.iter_mut() {
                if axis == 0 {
                    p.x = w - p.x;
                } else {
                    p.y = h - p.y;
                }
            }
        }
        _ => {}
    }
    for k in kids {
        let Some(c) = tx.doc().get(k) else { continue };
        let r = c.rect;
        let n = tx.node_mut(k).unwrap();
        if axis == 0 {
            n.rect.x = w - r.x - r.w;
        } else {
            n.rect.y = h - r.y - r.h;
        }
        flip_node(tx, k, axis);
    }
}

/// Deepest-first autosize of one container: auto-layout frames lay out through
/// layout; a plain group or frame hugs its children's world-rect union. Child
/// points never move: the container's origin follows the children.
fn autosize_subtree(tx: &mut Tx, id: NodeId) {
    let kids: Vec<NodeId> = tx.doc().children(Some(id)).to_vec();
    for k in kids {
        autosize_subtree(tx, k);
    }
    let Some(n) = tx.doc().get(id).cloned() else {
        return;
    };
    if !(n.is_group() || n.is_frame()) {
        return;
    }
    if n.layout.is_some() {
        // Auto-layout containers resolve through layout, which hugs first.
        crate::layout::layout_frame(tx, id);
        return;
    }
    let rects: Vec<Rect> = tx
        .doc()
        .children(Some(id))
        .iter()
        .filter(|c| tx.doc().get(**c).is_some_and(|x| x.visible))
        .filter_map(|c| tx.doc().world_rect(*c))
        .collect();
    let Some(cover) = Rect::union_all(&rects) else {
        return;
    };
    // The frame title overlays the top edge, so leave headroom there; groups
    // hug exactly.
    let pad = if n.is_frame() { 20.0 } else { 0.0 };
    let cover = cover.inflate(pad);
    let cur = match tx.doc().world_rect(id) {
        Some(r) => r,
        None => return,
    };
    // Move the outline onto the children and size it to cover them. Child
    // rects are parent-relative, so shift them by the opposite amount: their
    // world positions stay put.
    let d = cover.min() - cur.min();
    if d != V2::ZERO {
        for k in tx.doc().children(Some(id)).to_vec() {
            if let Some(c) = tx.node_mut(k) {
                c.rect.x -= d.x;
                c.rect.y -= d.y;
            }
        }
    }
    let mut rect = n.rect;
    rect.x += d.x;
    rect.y += d.y;
    rect.w = cover.w.max(1.0);
    rect.h = cover.h.max(1.0);
    if rect != n.rect {
        tx.node_mut(id).unwrap().rect = rect;
    }
}

#[cfg(test)]
mod tests {
    use crate::editor::Editor;
    use crate::geom::{Rect, v2};
    use crate::model::{Color, FillRule, Node, NodeId, NodeKind, Paint, PathPoint, SubPath};

    #[test]
    fn shp10_flip_horizontal_mirrors_geometry_and_keeps_box_and_gradient() {
        let mut ed = Editor::default();
        let mut node = Node::new(
            NodeId(0),
            NodeKind::Path {
                subpaths: vec![SubPath {
                    points: vec![
                        PathPoint::corner(v2(10.0, 20.0)),
                        PathPoint::corner(v2(80.0, 30.0)),
                        PathPoint::corner(v2(60.0, 90.0)),
                    ],
                    closed: true,
                }],
                fill_rule: FillRule::NonZero,
            },
            Rect::new(100.0, 50.0, 100.0, 100.0),
        );
        node.style.fills = vec![Paint::linear(Color::rgb(255, 0, 0), Color::rgb(0, 0, 255))];
        let id = ed.add_node(node, None);
        ed.selection = vec![id];
        let before_rect = ed.doc.get(id).unwrap().rect;
        let before_fills = ed.doc.get(id).unwrap().style.fills.clone();

        ed.flip(0);

        let n = ed.doc.get(id).unwrap();
        assert_eq!(n.rect, before_rect, "flip should keep the box in place");
        assert_eq!(
            n.style.fills, before_fills,
            "gradient fill should be preserved"
        );
        let NodeKind::Path { subpaths, .. } = &n.kind else {
            panic!("expected path");
        };
        let pts: Vec<_> = subpaths[0].points.iter().map(|p| (p.p.x, p.p.y)).collect();
        assert_eq!(pts, vec![(90.0, 20.0), (20.0, 30.0), (40.0, 90.0)]);
    }

    #[test]
    fn sty19_copy_paste_style_transfers_properties_and_clears_text_spans() {
        use crate::model::{TextSpan, TextStyle};

        let mut ed = Editor::default();
        let mut src = Node::new(NodeId(0), NodeKind::Rect, Rect::new(0.0, 0.0, 80.0, 40.0));
        src.style.fills = vec![Paint::solid(Color::rgb(10, 20, 30))];
        src.style.stroke = Some(Color::rgb(1, 2, 3));
        src.style.stroke_width = 3.5;
        src.style.corner_radius = 7.0;
        src.text_style = TextStyle {
            bold: true,
            italic: true,
            letter_spacing: 2.0,
            ..TextStyle::default()
        };
        let src_id = ed.add_node(src, None);

        let t1 = ed.add_node(
            Node::new(NodeId(0), NodeKind::Rect, Rect::new(100.0, 0.0, 80.0, 40.0)),
            None,
        );
        let mut text_node = Node::new(
            NodeId(0),
            NodeKind::Text {
                text: "hello".into(),
                font_size: 14.0,
                auto_width: true,
                align: crate::model::TextAlign::Left,
            },
            Rect::new(200.0, 0.0, 80.0, 40.0),
        );
        text_node.text_style.spans = vec![TextSpan {
            start: 0,
            end: 5,
            color: Some(Color::rgb(255, 0, 0)),
            ..TextSpan::default()
        }];
        let t2 = ed.add_node(text_node, None);

        ed.selection = vec![src_id];
        ed.copy_props();
        assert!(ed.has_style_clipboard());

        ed.selection = vec![t1, t2];
        ed.paste_props();

        for id in [t1, t2] {
            let n = ed.doc.get(id).unwrap();
            assert_eq!(n.style.fills, vec![Paint::solid(Color::rgb(10, 20, 30))]);
            assert_eq!(n.style.stroke, Some(Color::rgb(1, 2, 3)));
            assert_eq!(n.style.stroke_width, 3.5);
            assert_eq!(n.style.corner_radius, 7.0);
        }
        let text_n = ed.doc.get(t2).unwrap();
        assert!(text_n.text_style.bold);
        assert!(text_n.text_style.italic);
        assert_eq!(text_n.text_style.letter_spacing, 2.0);
        assert!(
            text_n.text_style.spans.is_empty(),
            "paste style should clear rich-text spans on text layers"
        );
    }
}
