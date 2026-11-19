//! Whiteboard tools: arrows that bind to shapes, quick-connect, sticky notes, the laser
//! pointer and the hand-drawn style toggle.
//!
//! Laser decay constants and the sticky-note behavior follow Excalidraw
//! (packages/excalidraw/laserTrails.ts, packages/laser-pointer; Copyright (c) 2020/2023
//! Excalidraw, MIT). See THIRD_PARTY.md.

use super::{Editor, Gesture, Tool};
use crate::connector::{self, BIND_DISTANCE_PX};
use crate::geom::{Rect, V2, v2};
use crate::history::Tx;
use crate::model::rough::Rough;
use crate::model::{Binding, ExtraLabel, Head, LineJump, Node, NodeId, NodeKind, Route};

/// Laser trail points fade over this long (ms), as in Excalidraw.
pub const LASER_DECAY_MS: f64 = 1000.0;
/// Only the newest points are drawn, tapering to nothing.
pub const LASER_MAX_POINTS: usize = 50;
/// Distance between a shape and the copy quick-connect creates.
pub const QUICK_GAP: f64 = 80.0;

/// Directions of the quick-connect knobs: right, down, left, up.
pub const QUICK_DIRS: [V2; 4] = [v2(1.0, 0.0), v2(0.0, 1.0), v2(-1.0, 0.0), v2(0.0, -1.0)];

/// Recompute a connector's points from its bindings and store them (node-local).
pub fn update_connector(tx: &mut Tx, id: NodeId) {
    // Drop bindings to layers that no longer exist.
    let stale = match tx.doc().get(id).map(|n| &n.kind) {
        Some(NodeKind::Connector { start, end, .. }) => (
            start.is_some_and(|b| !tx.doc().nodes.contains_key(&b.node)),
            end.is_some_and(|b| !tx.doc().nodes.contains_key(&b.node)),
        ),
        _ => return,
    };
    if stale.0 || stale.1 {
        let n = tx.node_mut(id).unwrap();
        if let NodeKind::Connector { start, end, .. } = &mut n.kind {
            if stale.0 {
                *start = None;
            }
            if stale.1 {
                *end = None;
            }
        }
    }
    let Some(pts) = connector::resolve(tx.doc(), id) else {
        return;
    };
    set_world_points(tx, id, &pts);
}

/// Switch selected connector ends between fixed points and floating perimeter hits.
/// A switch back to fixed keeps the endpoint's current position as the remembered point.
pub fn set_connector_endpoint_mode(ed: &mut Editor, ids: &[NodeId], which: usize, floating: bool) {
    let changes: Vec<(NodeId, V2)> = ids
        .iter()
        .filter_map(|id| {
            let binding = match &ed.doc.get(*id)?.kind {
                NodeKind::Connector { start, end, .. } => {
                    if which == 0 {
                        start.as_ref()
                    } else {
                        end.as_ref()
                    }
                }
                _ => return None,
            }?
            .to_owned();
            let pts = connector::resolve(&ed.doc, *id)?;
            let p = *pts.get(if which == 0 { 0 } else { pts.len() - 1 })?;
            let wr = ed.doc.world_rect(binding.node)?;
            let f = connector::fixed_point(p, wr);
            Some((*id, v2(f.x.clamp(0.0, 1.0), f.y.clamp(0.0, 1.0))))
        })
        .collect();
    if changes.is_empty() {
        return;
    }
    ed.begin(if floating {
        "Use floating endpoints"
    } else {
        "Use fixed endpoints"
    });
    {
        let mut tx = ed.tx();
        for (id, fixed) in changes {
            if let Some(n) = tx.node_mut(id)
                && let NodeKind::Connector { start, end, .. } = &mut n.kind
            {
                let binding = if which == 0 { start } else { end };
                if let Some(b) = binding {
                    b.floating = floating;
                    b.inside = false;
                    if !floating {
                        b.fixed = fixed;
                    }
                }
            }
        }
        for id in ids {
            update_connector(&mut tx, *id);
        }
    }
    ed.commit();
}

/// Store world points into a connector, refitting its box.
pub fn set_world_points(tx: &mut Tx, id: NodeId, pts: &[V2]) {
    let Some(n) = tx.doc().get(id) else { return };
    let o = tx.doc().parent_origin(n.parent);
    let b = Rect::union_all(
        &pts.iter()
            .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default();
    let local: Vec<V2> = pts.iter().map(|p| *p - b.min()).collect();
    let rect = b.translate(V2::ZERO - o);
    let same =
        matches!(&n.kind, NodeKind::Connector { points, .. } if *points == local) && n.rect == rect;
    if same {
        return;
    }
    let n = tx.node_mut(id).unwrap();
    n.rect = rect;
    if let NodeKind::Connector { points, .. } = &mut n.kind {
        *points = local;
    }
}

impl Editor {
    fn bind_dist(&self) -> f64 {
        BIND_DISTANCE_PX / self.camera.zoom
    }

    /// World points of a connector.
    pub fn connector_points(&self, id: NodeId) -> Option<Vec<V2>> {
        let n = self.doc.get(id)?;
        let wr = self.doc.world_rect(id)?;
        match &n.kind {
            NodeKind::Connector { points, .. } => {
                Some(points.iter().map(|p| wr.min() + *p).collect())
            }
            _ => None,
        }
    }

    /// Connectors that must be re-routed because something in `changed` moved or resized.
    pub(super) fn reroute_connectors(&mut self, changed: &std::collections::HashSet<NodeId>) {
        // World rects of everything that moved, for the obstacle check below.
        let moved: Vec<Rect> = changed
            .iter()
            .filter_map(|id| self.doc.world_rect(*id))
            .collect();
        let affected: Vec<NodeId> = self
            .doc
            .nodes
            .values()
            .filter_map(|n| match &n.kind {
                NodeKind::Connector {
                    start, end, route, ..
                } => {
                    let hit = |b: &Option<Binding>| {
                        b.is_some_and(|b| {
                            changed.contains(&b.node)
                                || !self.doc.nodes.contains_key(&b.node)
                                || self
                                    .doc
                                    .ancestors(b.node)
                                    .iter()
                                    .any(|a| changed.contains(a))
                        })
                    };
                    if hit(start)
                        || hit(end)
                        || (changed.contains(&n.id) && (start.is_some() || end.is_some()))
                    {
                        return Some(n.id);
                    }
                    // Elbow routes also avoid unrelated shapes, so a shape
                    // moving into the route's neighborhood reroutes it too.
                    // The connector's world rect spans its points.
                    if *route == Route::Elbow
                        && !moved.is_empty()
                        && let Some(wr) = self.doc.world_rect(n.id)
                    {
                        let corridor = wr.inflate(connector::ELBOW_PADDING);
                        if moved.iter().any(|r| r.intersects(&corridor)) {
                            return Some(n.id);
                        }
                    }
                    None
                }
                _ => None,
            })
            .collect();
        if affected.is_empty() {
            return;
        }
        let mut tx = self.tx();
        for id in affected {
            update_connector(&mut tx, id);
        }
    }

    pub(super) fn arrow_down(&mut self, world: V2) {
        self.end_text_edit();
        self.begin("Add arrow");
        let start_target = connector::bind_target(&self.doc, world, self.bind_dist(), &[]);
        let start_binding = start_target.map(|t| self.binding_at(world, t));
        let route = self.arrow_route;
        let default = self.connector_default_for(start_binding.map(|b| b.node));
        let id = {
            let mut tx = self.tx();
            let id = tx.alloc_id();
            let mut n = Node::new(
                id,
                NodeKind::Connector {
                    points: vec![V2::ZERO, V2::ZERO],
                    start: start_binding,
                    end: None,
                    route,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(world.x, world.y, 0.0, 0.0),
            );
            if let Some(ds) = default {
                n.style = ds.style;
                n.text_style = ds.text_style;
            }
            tx.insert(n, None, None)
        };
        self.selection = vec![id];
        self.gesture = Gesture::ConnectorEnd {
            id,
            end: 1,
            start: world,
        };
    }

    /// Binding for `world` on `target`, snapped to a fixed point when the pointer
    /// is close enough. The stored fraction keeps the end on that point when the
    /// target is resized.
    fn binding_at(&self, world: V2, target: NodeId) -> Binding {
        let wr = self.doc.world_rect(target).unwrap();
        let (p, snapped) = connector::snap_fixed_state(
            world,
            wr,
            connector::FIXED_SNAP_PX / self.camera.zoom.max(0.01),
        );
        Binding {
            node: target,
            fixed: connector::fixed_point(p, wr),
            inside: false,
            floating: !snapped,
        }
    }

    /// Drag a connector end to `world`, binding to whatever shape is under it.
    pub(super) fn drag_connector_end(&mut self, id: NodeId, end: usize, world: V2) {
        let Some(mut pts) = self.connector_points(id) else {
            return;
        };
        let target = connector::bind_target(&self.doc, world, self.bind_dist(), &[id])
            .filter(|t| !self.doc.is_ancestor(id, *t));
        self.bind_hover = target;
        let idx = if end == 0 { 0 } else { pts.len() - 1 };
        let mut p = world;
        if self.mods.shift && target.is_none() && pts.len() == 2 {
            // 45° steps from the other end.
            let other = pts[1 - idx];
            let d = p - other;
            let a = (d.y.atan2(d.x) / std::f64::consts::FRAC_PI_4).round()
                * std::f64::consts::FRAC_PI_4;
            p = other + v2(a.cos(), a.sin()) * d.len();
        }
        pts[idx] = p;
        let binding = target.map(|t| self.binding_at(world, t));
        let mut tx = self.tx();
        if let Some(n) = tx.node_mut(id)
            && let NodeKind::Connector { start, end: e, .. } = &mut n.kind
        {
            if end == 0 {
                *start = binding;
            } else {
                *e = binding;
            }
        }
        set_world_points(&mut tx, id, &pts);
        update_connector(&mut tx, id);
    }

    pub(super) fn connector_end_up(&mut self, id: NodeId, start: V2, world: V2) {
        self.bind_hover = None;
        let created = self
            .rec
            .as_ref()
            .is_some_and(|(r, _)| r.before(id) == Some(&None));
        // A click without a drag makes a default-length arrow.
        if created && (world - start).len() * self.camera.zoom < 8.0 {
            let pts = vec![start, start + v2(120.0, 0.0)];
            let mut tx = self.tx();
            set_world_points(&mut tx, id, &pts);
            update_connector(&mut tx, id);
        }
        self.commit();
        if created {
            self.tool = Tool::Select;
        }
    }

    /// Screen-space handles of the selected connector: (end index, position). Index 2 is the
    /// midpoint "bend" handle of a two-point connector.
    pub fn connector_handles(&self) -> Vec<(usize, V2)> {
        let [id] = self.selection[..] else {
            return vec![];
        };
        let Some(pts) = self.connector_points(id) else {
            return vec![];
        };
        let s = |p: V2| self.camera.world_to_screen(p);
        let mut out = vec![(0, s(pts[0])), (1, s(*pts.last().unwrap()))];
        let elbow = matches!(
            self.doc.get(id).map(|n| &n.kind),
            Some(NodeKind::Connector {
                route: Route::Elbow,
                ..
            })
        );
        let labeled = self.doc.get(id).is_some_and(|n| !n.label.is_empty());
        if pts.len() == 2 && !elbow && !labeled {
            out.push((2, s((pts[0] + pts[1]) * 0.5)));
        }
        out
    }

    /// Start dragging a connector handle under `pos`, if any.
    pub(super) fn connector_handle_down(&mut self, pos: V2, world: V2) -> bool {
        let Some(&(end, _)) = self
            .connector_handles()
            .iter()
            .find(|(_, p)| p.dist(pos) <= 7.0)
        else {
            return false;
        };
        let id = self.selection[0];
        if end == 2 {
            // With a label present, the midpoint belongs to the label: a press
            // there selects or drags the edge instead of silently bending it.
            if self
                .doc
                .get(id)
                .is_some_and(|n| n.is_connector() && !n.label.is_empty())
            {
                return false;
            }
            // Bend: insert a middle point and make the arrow curved.
            self.begin("Bend arrow");
            let mut pts = self.connector_points(id).unwrap();
            pts.insert(1, world);
            let mut tx = self.tx();
            if let Some(n) = tx.node_mut(id)
                && let NodeKind::Connector { route, .. } = &mut n.kind
            {
                *route = Route::Curved;
            }
            set_world_points(&mut tx, id, &pts);
            self.gesture = Gesture::ConnectorMid { id };
            return true;
        }
        self.begin("Move arrow end");
        self.gesture = Gesture::ConnectorEnd {
            id,
            end,
            start: world,
        };
        true
    }

    pub(super) fn drag_connector_mid(&mut self, id: NodeId, world: V2) {
        let Some(mut pts) = self.connector_points(id) else {
            return;
        };
        if pts.len() >= 3 {
            pts[1] = world;
        }
        let mut tx = self.tx();
        set_world_points(&mut tx, id, &pts);
        update_connector(&mut tx, id);
    }

    /// Move a connector label so its box center lands on `center` (world).
    /// The label keeps the closest point on the route as its anchor and stores
    /// the rest as an offset, so it can sit on the line or away from it.
    /// `idx` picks the label: 0 is the main label, higher indices are extras.
    pub(super) fn set_connector_label_center(&mut self, id: NodeId, idx: usize, center: V2) {
        let Some(wr) = self.doc.world_rect(id) else {
            return;
        };
        let Some((points, route)) = self.doc.get(id).and_then(|n| match &n.kind {
            NodeKind::Connector { points, route, .. } => Some((points.clone(), *route)),
            _ => None,
        }) else {
            return;
        };
        let local = center - wr.min();
        let Some((t, anchor)) = connector::closest_point(&points, route, local) else {
            return;
        };
        let offset = local - anchor;
        let on_line = offset.len() < 0.5;
        let pos = if on_line && (t - 0.5).abs() < 0.01 {
            None
        } else {
            Some(t)
        };
        let mut tx = self.tx();
        let Some(n) = tx.node_mut(id) else {
            return;
        };
        if idx == 0 {
            n.label_offset = (!on_line).then_some(offset);
            n.label_pos = pos;
        } else if let Some(extra) = n.labels.get_mut(idx - 1) {
            extra.offset = (!on_line).then_some(offset);
            extra.pos = pos;
        }
    }

    /// Add an extra label to a connector (draw.io's Add Label). The new label
    /// starts centered on the line with placeholder text; the editor opens it
    /// with the placeholder selected, so typing replaces it. One undo step
    /// for the add, then the usual Edit text step for the typing.
    pub fn add_connector_label(&mut self, id: NodeId) {
        let is_connector = self.doc.get(id).is_some_and(|n| n.is_connector());
        if !is_connector {
            return;
        }
        self.edit("Add label", |tx| {
            if let Some(n) = tx.node_mut(id) {
                n.labels.push(ExtraLabel::new("Label"));
            }
        });
        let idx = self.doc.get(id).map_or(1, |n| n.label_count()) - 1;
        self.start_text_edit_label(id, idx, None);
    }

    /// Swap each selected connector's source and target, matching draw.io's
    /// Reverse (its `turnShapes` edge branch): bindings and waypoints swap,
    /// arrowheads and label positions stay bound to the line's own ends, so
    /// both visually move to the opposite end while the path stays put.
    /// One undo step.
    pub fn reverse_connectors(&mut self, ids: &[NodeId]) {
        let targets: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| self.doc.get(*id).is_some_and(|n| n.is_connector()))
            .collect();
        if targets.is_empty() {
            return;
        }
        self.begin("Reverse connector");
        for id in targets {
            let mut tx = self.tx();
            let Some(n) = tx.node_mut(id) else { continue };
            let NodeKind::Connector {
                points, start, end, ..
            } = &mut n.kind
            else {
                continue;
            };
            points.reverse();
            std::mem::swap(start, end);
        }
        self.commit();
    }

    /// Put connector labels back on the middle of their lines, including a
    /// connector's extra labels. One undo step.
    pub fn reset_connector_label_positions(&mut self, ids: &[NodeId]) {
        let targets: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc.get(*id).is_some_and(|n| {
                    n.is_connector()
                        && ((n.label_pos.is_some() || n.label_offset.is_some())
                            || n.labels
                                .iter()
                                .any(|l| l.pos.is_some() || l.offset.is_some()))
                })
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        self.begin("Reset label positions");
        for id in targets {
            if let Some(n) = self.tx().node_mut(id) {
                n.label_pos = None;
                n.label_offset = None;
                for l in &mut n.labels {
                    l.pos = None;
                    l.offset = None;
                }
            }
        }
        self.commit();
    }

    /// Drop manually bent points from straight and curved connectors. Elbow
    /// routes are generated from their endpoints, so they have nothing to reset.
    pub fn reset_connector_waypoints(&mut self, ids: &[NodeId]) {
        let targets: Vec<NodeId> = ids
            .iter()
            .copied()
            .filter(|id| {
                self.doc.get(*id).is_some_and(|n| match &n.kind {
                    NodeKind::Connector { points, route, .. } => {
                        *route != Route::Elbow && points.len() > 2
                    }
                    _ => false,
                })
            })
            .collect();
        if targets.is_empty() {
            return;
        }
        self.begin("Reset waypoints");
        for id in targets {
            let Some(pts) = self.connector_points(id) else {
                continue;
            };
            let Some(last) = pts.last().copied() else {
                continue;
            };
            let simple = [pts[0], last];
            let mut tx = self.tx();
            set_world_points(&mut tx, id, &simple);
        }
        self.commit();
    }

    // ---------------------------------------------------------------------- quick connect

    /// Screen positions of the quick-connect knobs around a single selected shape.
    pub fn quick_knobs(&self) -> Vec<(usize, V2)> {
        let [id] = self.selection[..] else {
            return vec![];
        };
        self.knobs_for(id)
    }

    /// Knobs around the hovered unselected shape (as in draw.io): press one and
    /// drag to connect an arrow to whatever shape is under the release point.
    pub fn hover_knobs(&self) -> Option<(NodeId, Vec<(usize, V2)>)> {
        let id = self.hover.filter(|h| !self.selection.contains(h))?;
        let knobs = self.knobs_for(id);
        (!knobs.is_empty()).then_some((id, knobs))
    }

    /// A press landing on a knob itself. The knobs sit just outside their shape, so
    /// the press never sets `hover` for it — this finds the owning shape from the
    /// press position instead (draw.io's behavior: press anywhere near the arrow
    /// anchor and drag).
    pub(super) fn knob_hit(&self, pos: V2) -> Option<(NodeId, usize)> {
        let mut best: Option<(f64, NodeId, usize)> = None;
        for id in self.doc.paint_order() {
            if self.selection.contains(&id) {
                continue; // selected shapes already offer quick knobs.
            }
            for (dir, p) in self.knobs_for(id) {
                let d = p.dist(pos);
                if d <= 9.0 && best.is_none_or(|(b, _, _)| d < b) {
                    best = Some((d, id, dir));
                }
            }
        }
        best.map(|(_, id, dir)| (id, dir))
    }

    /// A drag-to-bind connector gesture is in flight (draw.io's arrow linking).
    pub fn is_connecting(&self) -> bool {
        matches!(
            self.gesture,
            Gesture::ConnectorEnd { .. } | Gesture::ConnectorMid { .. }
        )
    }

    /// Knob positions around one connectable shape, just outside its edges.
    fn knobs_for(&self, id: NodeId) -> Vec<(usize, V2)> {
        if self.tool != Tool::Select || self.text_edit.is_some() || self.is_busy() {
            return vec![];
        }
        let Some(n) = self.doc.get(id) else {
            return vec![];
        };
        if !connector::bindable(n) || n.is_frame() || matches!(n.kind, NodeKind::Text { .. }) {
            return vec![];
        }
        let Some(r) = self.doc.world_rect(id) else {
            return vec![];
        };
        let sr = self.camera.world_rect_to_screen(&r);
        if sr.w < 24.0 || sr.h < 24.0 {
            return vec![];
        }
        let c = sr.center();
        QUICK_DIRS
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let off = v2(sr.w * 0.5 + 22.0, sr.h * 0.5 + 22.0);
                (i, c + v2(d.x * off.x, d.y * off.y))
            })
            .collect()
    }

    /// Press on a knob of the hovered shape: drag out an arrow from that side,
    /// binding its end to whatever shape the release lands on.
    pub(super) fn hover_connect_down(&mut self, id: NodeId, dir: usize, world: V2) -> bool {
        let Some(wr) = self.doc.world_rect(id) else {
            return false;
        };
        self.end_text_edit();
        self.begin("Add arrow");
        // The arrow starts pinned to the midpoint of the side the knob sits on.
        let d = QUICK_DIRS[dir];
        let start = Some(Binding {
            node: id,
            fixed: connector::fixed_point(
                v2(
                    wr.x + wr.w * (0.5 + d.x * 0.5),
                    wr.y + wr.h * (0.5 + d.y * 0.5),
                ),
                wr,
            ),
            inside: false,
            floating: false,
        });
        let route = self.arrow_route;
        let default = self.connector_default_for(Some(id));
        let aid = {
            let mut tx = self.tx();
            let id = tx.alloc_id();
            let mut a = Node::new(
                id,
                NodeKind::Connector {
                    points: vec![V2::ZERO, V2::ZERO],
                    start,
                    end: None,
                    route,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(world.x, world.y, 0.0, 0.0),
            );
            if let Some(ds) = default {
                a.style = ds.style;
                a.text_style = ds.text_style;
            }
            tx.insert(a, None, None)
        };
        self.selection = vec![aid];
        self.gesture = Gesture::ConnectorEnd {
            id: aid,
            end: 1,
            start: world,
        };
        true
    }

    /// Duplicate the selected shape in direction `dir` and connect the two with an arrow.
    pub fn quick_connect(&mut self, dir: usize) -> Option<NodeId> {
        let [src] = self.selection[..] else {
            return None;
        };
        let n = self.doc.get(src)?.clone();
        let wr = self.doc.world_rect(src)?;
        let d = QUICK_DIRS[dir];
        let off = v2(d.x * (wr.w + QUICK_GAP), d.y * (wr.h + QUICK_GAP));
        let parent = n.parent;
        let o = self.doc.parent_origin(parent);
        let route = self.arrow_route;
        let rough = n.style.rough;
        let default = self.connector_default_for(Some(src));
        let (copy, arrow) = self.edit("Quick connect", |tx| {
            let cid = tx.alloc_id();
            let mut c = n.clone();
            c.id = cid;
            c.children.clear();
            c.instance = None;
            c.component = false;
            if let Some(t) = c.kind.text_mut() {
                t.clear();
            }
            c.text_style = Default::default();
            c.rect = wr.translate(off - o);
            if let Some(r) = &mut c.style.rough {
                r.seed = r.seed.wrapping_add(7919);
            }
            tx.insert(c, parent, None);
            let aid = tx.alloc_id();
            let fixed_a = v2(0.5 + d.x * 0.5, 0.5 + d.y * 0.5);
            let fixed_b = v2(0.5 - d.x * 0.5, 0.5 - d.y * 0.5);
            let fix = |v: V2| connector::fixed_point(v2(v.x, v.y), Rect::new(0.0, 0.0, 1.0, 1.0));
            let mut a = Node::new(
                aid,
                NodeKind::Connector {
                    points: vec![V2::ZERO, V2::ZERO],
                    start: Some(Binding {
                        node: src,
                        fixed: fix(fixed_a),
                        inside: false,
                        floating: false,
                    }),
                    end: Some(Binding {
                        node: cid,
                        fixed: fix(fixed_b),
                        inside: false,
                        floating: false,
                    }),
                    route,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::default(),
            );
            if let Some(ds) = default {
                a.style = ds.style;
                a.text_style = ds.text_style;
            } else {
                a.style.rough = rough.map(|r| Rough {
                    seed: r.seed.wrapping_add(31),
                    ..r
                });
            }
            tx.insert(a, None, None);
            update_connector(tx, aid);
            (cid, aid)
        });
        let _ = arrow;
        self.selection = vec![copy];
        Some(copy)
    }

    // ---------------------------------------------------------------------- sticky notes

    pub(super) fn sticky_down(&mut self, world: V2) {
        self.end_text_edit();
        self.begin("Add sticky");
        let size = 200.0;
        let parent = self.frame_at(world, &[]);
        let o = self.doc.parent_origin(parent);
        let color = self.sticky_color;
        let id = {
            let mut tx = self.tx();
            let id = tx.alloc_id();
            let mut n = Node::new(
                id,
                NodeKind::Sticky {
                    text: String::new(),
                    font_size: 24.0,
                },
                Rect::new(
                    (world.x - size * 0.5).round() - o.x,
                    (world.y - size * 0.5).round() - o.y,
                    size,
                    size,
                ),
            );
            n.style.set_fill(Some(color));
            tx.insert(n, parent, None)
        };
        self.selection = vec![id];
        self.text_edit = Some(super::TextEdit {
            id,
            label: 0,
            caret: 0,
            anchor: None,
        });
        self.tool = Tool::Select;
    }

    // ---------------------------------------------------------------------- laser

    pub(super) fn laser_point(&mut self, world: V2, time_ms: f64) {
        if self.laser.last().is_some_and(|(p, _)| *p == world) {
            return;
        }
        // Streamline like Excalidraw's laser (0.4).
        let p = match self.laser.last() {
            Some((last, t)) if time_ms - t < LASER_DECAY_MS => last.lerp(world, 0.6),
            _ => world,
        };
        self.laser.push((p, time_ms));
        self.now_ms = self.now_ms.max(time_ms);
    }

    /// Points still visible at `now`, with their size factor (0..1).
    pub fn laser_trail(&self, now: f64) -> Vec<(V2, f64)> {
        let n = self.laser.len();
        let ease = |k: f64| 1.0 - (1.0 - k.clamp(0.0, 1.0)).powi(4);
        self.laser
            .iter()
            .enumerate()
            .filter_map(|(i, (p, t))| {
                let tf = (1.0 - (now - t) / LASER_DECAY_MS).max(0.0);
                let lf = (LASER_MAX_POINTS as f64 - ((n - i) as f64).min(LASER_MAX_POINTS as f64))
                    / LASER_MAX_POINTS as f64;
                let k = ease(tf).min(ease(lf + 1.0 / LASER_MAX_POINTS as f64));
                (k > 0.0).then_some((*p, k))
            })
            .collect()
    }

    /// Whether something on the canvas is animating (a fading laser trail or a
    /// visible connector with flow animation playing).
    pub fn animating(&self, now: f64) -> bool {
        let laser = self
            .laser
            .last()
            .is_some_and(|(_, t)| now - t < LASER_DECAY_MS);
        laser
            || self.doc.nodes.values().any(|n| {
                n.is_connector() && n.flow.enabled && n.visible && self.doc.layer_of(n.id).visible
            })
    }

    /// Drop faded laser points (call with the current time before drawing).
    pub fn tick(&mut self, now: f64) {
        self.now_ms = now;
        self.laser.retain(|(_, t)| now - t < LASER_DECAY_MS);
    }

    // ---------------------------------------------------------------------- sketch style

    /// Toggle the hand-drawn style on the selection, or the page-wide default
    /// when nothing is selected. An explicit element choice overrides the page
    /// setting without discarding the stored rough parameters.
    pub fn toggle_sketch(&mut self) {
        let ids = self.top_selection();
        if ids.is_empty() {
            let on = !self.doc.settings.sketch;
            self.edit_settings("Toggle sketch", |s| s.sketch = on);
            self.status = format!(
                "Hand-drawn style {} for the diagram",
                if on { "on" } else { "off" }
            );
            return;
        }
        let on = ids.iter().any(|id| {
            self.doc.get(*id).is_some_and(|n| {
                !n.style
                    .sketch
                    .unwrap_or(n.style.rough.is_some() || self.doc.settings.sketch)
            })
        });
        self.edit(if on { "Hand-drawn" } else { "Clean lines" }, |tx| {
            for id in ids {
                let Some(n) = tx.node_mut(id) else { continue };
                n.style.sketch = Some(on);
                if on && n.style.rough.is_none() {
                    n.style.rough = Some(Rough {
                        seed: (id.0 as u32).wrapping_mul(2654435761) | 1,
                        ..Rough::default()
                    });
                }
            }
        });
    }
}
