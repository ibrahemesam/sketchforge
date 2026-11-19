//! Hit testing in world space.

use crate::geom::{Rect, V2, v2};
use crate::ink;
use crate::model::{Document, NodeId, NodeKind};
use crate::render::draw;

/// Tolerance for thin geometry, in screen pixels.
const HIT_SLOP_PX: f64 = 4.0;

/// Whether `p` (world) is inside the clip regions of all of the node's ancestors.
pub fn inside_ancestor_clips(doc: &Document, id: NodeId, p: V2) -> bool {
    doc.ancestors(id).into_iter().all(|a| {
        let n = doc.get(a).unwrap();
        !matches!(n.kind, NodeKind::Frame { clip: true })
            || doc.world_rect(a).is_some_and(|r| r.contains(p))
    })
}

/// Shape test for a single node, ignoring children and clipping.
pub fn node_contains(doc: &Document, id: NodeId, p: V2, zoom: f64) -> bool {
    let Some(n) = doc.get(id) else { return false };
    let Some(r) = doc.world_rect(id) else {
        return false;
    };
    let slop = HIT_SLOP_PX / zoom;
    // Rotation happens around the node's center, so undo it before testing the
    // unrotated geometry. `p` stays in world space for the broad-phase bounds.
    let local = if n.style.rotation != 0.0 {
        p.rotate_about(r.center(), -n.style.rotation.to_radians())
    } else {
        p
    };
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect | NodeKind::Text { .. } => r.contains(local),
        NodeKind::Group => false,
        NodeKind::Stroke {
            points,
            width,
            thinning,
        } => {
            if !r.inflate(slop).contains(local) {
                return false;
            }
            let local = local - r.min();
            let max_r = ink::radius(*width, *thinning, 1.0);
            ink::distance_to_polyline(points, local) <= max_r + slop
        }
        NodeKind::Connector { .. } => {
            // The line is thin, so the routed path decides the hit before the
            // label box does. A label can extend past the line's bounding box
            // on a short connector, so test it on its own.
            let tol = 0.25 / zoom;
            if let Some(path) = draw::shape_path(n, r)
                && path.distance(local, tol) <= n.style.stroke_width * 0.5 + slop
            {
                return true;
            }
            for idx in 0..n.label_count() {
                if let Some(b) = crate::text::label_box_label(n, idx) {
                    let boxr = Rect::new(r.x + b.x, r.y + b.y, b.w, b.h);
                    if boxr.inflate(slop).contains(local) {
                        return true;
                    }
                }
            }
            false
        }
        _ => {
            if !draw::draw_bounds(n, r).inflate(slop).contains(p) {
                return false;
            }
            let Some(path) = draw::shape_path(n, r) else {
                return false;
            };
            let tol = 0.25 / zoom;
            let even_odd = matches!(
                n.kind,
                NodeKind::Path {
                    fill_rule: crate::model::FillRule::EvenOdd,
                    ..
                }
            );
            // Tiny shapes are easier to grab by their box.
            if r.w.min(r.h) * zoom < 8.0 && draw::is_area(n) && r.inflate(slop).contains(local) {
                return true;
            }
            let filled =
                draw::is_area(n) && (!n.style.fills.is_empty() || n.style.stroke.is_none());
            (filled && path.contains(local, even_odd, tol))
                || path.distance(local, tol) <= n.style.stroke_width * 0.5 + slop
        }
    }
}

/// Topmost visible, unlocked node under `p`, deepest first.
pub fn hit_test(doc: &Document, p: V2, zoom: f64) -> Option<NodeId> {
    doc.paint_order().into_iter().rev().find(|id| {
        doc.is_effectively_visible(*id)
            && !doc.is_effectively_locked(*id)
            && node_contains(doc, *id, p, zoom)
            && inside_ancestor_clips(doc, *id, p)
    })
}

/// Screen-space rectangle of a top-level frame's title label, centered above the frame.
pub fn frame_label_rect(frame_screen: &Rect, name_width_px: f64) -> Rect {
    let w = name_width_px.max(24.0) + 4.0;
    Rect::new(
        frame_screen.x + (frame_screen.w - w) * 0.5,
        frame_screen.y - 20.0,
        w,
        18.0,
    )
}

/// Which of the 8 resize handles (fractions of the bounds) is under the screen point.
pub fn handle_at(bounds_screen: &Rect, p: V2) -> Option<(f64, f64)> {
    const R: f64 = 7.0;
    let mut best: Option<((f64, f64), f64)> = None;
    for (hx, hy) in HANDLES {
        let hp = v2(
            bounds_screen.x + bounds_screen.w * hx,
            bounds_screen.y + bounds_screen.h * hy,
        );
        let d = (hp.x - p.x).abs().max((hp.y - p.y).abs());
        // Edge handles are hidden on tiny selections; corners win ties.
        if (hx == 0.5 || hy == 0.5) && bounds_screen.w.min(bounds_screen.h) < 24.0 {
            continue;
        }
        if d <= R && best.is_none_or(|(_, bd)| d < bd) {
            best = Some(((hx, hy), d));
        }
    }
    best.map(|b| b.0)
}

pub const HANDLES: [(f64, f64); 8] = [
    (0.0, 0.0),
    (0.5, 0.0),
    (1.0, 0.0),
    (1.0, 0.5),
    (1.0, 1.0),
    (0.5, 1.0),
    (0.0, 1.0),
    (0.0, 0.5),
];

/// Screen distance from the selection's top edge to the center of the rotate handle.
pub const ROTATE_OFFSET: f64 = 20.0;
/// How close the pointer must be to the rotate handle, in screen pixels.
const ROTATE_R: f64 = 9.0;

/// Center of the rotate handle above the selection's top-right corner, in
/// screen space. draw.io puts it at the top right, clear of the connection
/// knobs at the middle of each edge.
pub fn rotate_handle_pos(bounds_screen: &Rect) -> V2 {
    v2(bounds_screen.right(), bounds_screen.y - ROTATE_OFFSET)
}

/// Whether `p` (screen) is on the rotate handle above the selection bounds.
pub fn rotate_handle_at(bounds_screen: &Rect, p: V2) -> bool {
    rotate_handle_pos(bounds_screen).dist(p) <= ROTATE_R
}
