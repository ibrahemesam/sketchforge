//! Arrows and connectors: bound endpoints, straight / curved / elbow routing, arrowheads.
//!
//! The binding rules, constants and the elbow router (orthogonal grid + A* with a bend
//! penalty and Excalidraw's segment-count heuristic) are adapted from Excalidraw
//! (packages/element/src/binding.ts and elbowArrow.ts, Copyright (c) 2020 Excalidraw, MIT).
//! See THIRD_PARTY.md. The router is a simplified port: obstacles are the padded boxes of the
//! bound shapes, without Excalidraw's dynamic-AABB "side hack".

use crate::geom::{Rect, V2, v2};
use crate::model::{DEFAULT_HEAD_SIZE, Document, Head, LineJump, Node, NodeId, NodeKind, Route};
use crate::render::draw;
use crate::vector::{self, VPath};

/// Gap between an orbit-bound end and the target outline (`5 + strokeWidth / 2`).
pub fn binding_gap(target_stroke: f64) -> f64 {
    5.0 + target_stroke / 2.0
}

/// How close (screen px) the pointer must be to a shape to bind to it.
pub const BIND_DISTANCE_PX: f64 = 15.0;
pub const ELBOW_PADDING: f64 = 40.0;
pub const ELBOW_RADIUS: f64 = 16.0;
/// Clearance an elbow route keeps around unrelated shapes it avoids. Smaller
/// than `ELBOW_PADDING` so routes can still squeeze between nearby shapes.
pub const OBSTACLE_PADDING: f64 = 10.0;

/// The shapes an elbow route must avoid, as padded boxes: every bindable,
/// visible shape on the page except the connector's own endpoints, like
/// draw.io's orthogonal router. Limited to `area`, the neighborhood the search
/// can reach, so the A* grid stays small on large pages.
fn route_obstacles(doc: &Document, area: Rect, skip: &[NodeId]) -> Vec<Rect> {
    let mut out = Vec::new();
    for id in doc.paint_order() {
        if skip.contains(&id) {
            continue;
        }
        let Some(n) = doc.get(id) else { continue };
        if !doc.is_effectively_visible(id) || !bindable(n) {
            continue;
        }
        let Some(r) = doc.world_rect(id) else {
            continue;
        };
        if !r.intersects(&area) {
            continue;
        }
        out.push(r.inflate(OBSTACLE_PADDING));
    }
    out
}

/// Can arrows attach to this node?
pub fn bindable(n: &Node) -> bool {
    match &n.kind {
        NodeKind::Rect
        | NodeKind::Ellipse
        | NodeKind::Polygon { .. }
        | NodeKind::Star { .. }
        | NodeKind::Frame { .. }
        | NodeKind::Text { .. }
        | NodeKind::Sticky { .. } => true,
        NodeKind::Path { subpaths, .. } => subpaths.iter().any(|s| s.closed),
        _ => false,
    }
}

/// The topmost bindable node at `p` (or within `dist` of its outline), skipping `exclude`.
pub fn bind_target(doc: &Document, p: V2, dist: f64, exclude: &[NodeId]) -> Option<NodeId> {
    let order = doc.paint_order();
    let mut best: Option<(NodeId, f64)> = None;
    for id in order.into_iter().rev() {
        if exclude.contains(&id) || !doc.is_effectively_visible(id) || doc.is_effectively_locked(id)
        {
            continue;
        }
        let n = doc.get(id).unwrap();
        if !bindable(n) {
            continue;
        }
        let wr = doc.world_rect(id).unwrap();
        if !wr.inflate(dist).contains(p) {
            continue;
        }
        let path = draw::shape_path(n, wr).unwrap_or_else(|| vector::rounded_rect(wr, [0.0; 4]));
        let inside = path.contains(p, false, 0.5);
        // A label sitting on a shape: bind to the shape, not the text.
        if matches!(n.kind, NodeKind::Text { .. }) && inside {
            let under = doc.paint_order().into_iter().rev().find(|o| {
                *o != id
                    && !exclude.contains(o)
                    && doc.get(*o).is_some_and(|on| {
                        bindable(on) && !matches!(on.kind, NodeKind::Text { .. }) && !on.is_frame()
                    })
                    && doc.world_rect(*o).is_some_and(|r| r.contains(p))
            });
            if under.is_some() {
                continue;
            }
        }
        // Frames bind only from their edge, so arrows can be drawn inside them.
        if inside && n.is_frame() && path.distance(p, 0.5) > dist {
            continue;
        }
        if inside {
            return Some(id);
        }
        let d = path.distance(p, 0.5);
        if d <= dist && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((id, d));
        }
    }
    best.map(|b| b.0)
}

/// Normalized attachment point of `p` in a target box (Excalidraw nudges exact centers to
/// 0.5001 to avoid heading flips).
pub fn fixed_point(p: V2, wr: Rect) -> V2 {
    let f = |c: f64| {
        let c = c.clamp(-10.0, 10.0);
        if (c - 0.5).abs() < 1e-4 { 0.5001 } else { c }
    };
    v2(
        f((p.x - wr.x) / wr.w.max(1.0)),
        f((p.y - wr.y) / wr.h.max(1.0)),
    )
}

pub fn global_fixed(fixed: V2, wr: Rect) -> V2 {
    v2(wr.x + wr.w * fixed.x, wr.y + wr.h * fixed.y)
}

/// The eight fixed attachment points on a bindable box: four corners and four
/// edge midpoints, in clockwise order from the top-left corner.
pub fn fixed_points(wr: Rect) -> [V2; 8] {
    [
        v2(wr.x, wr.y),
        v2(wr.x + wr.w * 0.5, wr.y),
        v2(wr.right(), wr.y),
        v2(wr.right(), wr.y + wr.h * 0.5),
        v2(wr.right(), wr.bottom()),
        v2(wr.x + wr.w * 0.5, wr.bottom()),
        v2(wr.x, wr.bottom()),
        v2(wr.x, wr.y + wr.h * 0.5),
    ]
}

/// Snap distance for fixed attachment points, in screen pixels.
pub const FIXED_SNAP_PX: f64 = 12.0;

/// Snap `p` to the nearest fixed point within `tolerance` world units, and report
/// whether a point won. A miss keeps the pointer position and makes an endpoint floating.
pub fn snap_fixed_state(p: V2, wr: Rect, tolerance: f64) -> (V2, bool) {
    fixed_points(wr)
        .into_iter()
        .filter(|q| q.dist(p) <= tolerance)
        .min_by(|a, b| a.dist(p).total_cmp(&b.dist(p)))
        .map_or((p, false), |q| (q, true))
}

/// Snap `p` to the nearest fixed point within `tolerance` world units, or keep
/// the pointer position when no fixed point is close enough.
pub fn snap_fixed(p: V2, wr: Rect, tolerance: f64) -> V2 {
    snap_fixed_state(p, wr, tolerance).0
}

/// The target's outline grown by `gap`, as polygons.
fn outline_polys(doc: &Document, id: NodeId, gap: f64) -> Vec<Vec<V2>> {
    let (Some(n), Some(wr)) = (doc.get(id), doc.world_rect(id)) else {
        return vec![];
    };
    let grown = Node {
        rect: wr.inflate(gap),
        style: crate::model::Style {
            corner_radius: if n.style.corner_radius > 0.0 {
                n.style.corner_radius + gap
            } else {
                0.0
            },
            radii: n
                .style
                .radii
                .map(|r| r.map(|x| if x > 0.0 { x + gap } else { 0.0 })),
            ..n.style.clone()
        },
        ..n.clone()
    };
    let path = match &n.kind {
        NodeKind::Path { .. } => draw::shape_path(n, wr),
        _ => draw::shape_path(&grown, grown.rect),
    }
    .unwrap_or_else(|| vector::rounded_rect(wr.inflate(gap), [0.0; 4]));
    path.flatten(0.25)
        .into_iter()
        .filter(|p| p.pts.len() > 2)
        .map(|p| p.pts)
        .collect()
}

fn seg_intersect(a: V2, b: V2, c: V2, d: V2) -> Option<V2> {
    let r = b - a;
    let s = d - c;
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * s.y - (c.y - a.y) * s.x) / den;
    let u = ((c.x - a.x) * r.y - (c.y - a.y) * r.x) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Where segment `a→b` crosses the target outline (grown by `gap`), nearest to `b`.
pub fn outline_hit(doc: &Document, id: NodeId, a: V2, b: V2, gap: f64) -> Option<V2> {
    let mut best: Option<(V2, f64)> = None;
    for poly in outline_polys(doc, id, gap) {
        let n = poly.len();
        for i in 0..n {
            if let Some(p) = seg_intersect(a, b, poly[i], poly[(i + 1) % n]) {
                let d = p.dist(b);
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((p, d));
                }
            }
        }
    }
    best.map(|b| b.0)
}

/// Excalidraw's `headingForPointFromElement` for boxes: which side of `r` the point faces.
pub fn heading_from_box(r: Rect, p: V2) -> V2 {
    let c = r.center();
    let d = p - c;
    // Compare against the box's diagonals (aspect-aware).
    let nx = d.x / r.w.max(1e-9);
    let ny = d.y / r.h.max(1e-9);
    if ny.abs() >= nx.abs() {
        if ny < 0.0 {
            v2(0.0, -1.0)
        } else {
            v2(0.0, 1.0)
        }
    } else if nx > 0.0 {
        v2(1.0, 0.0)
    } else {
        v2(-1.0, 0.0)
    }
}

pub fn vector_heading(v: V2) -> V2 {
    if v.x > v.y.abs() {
        v2(1.0, 0.0)
    } else if v.x <= -v.y.abs() {
        v2(-1.0, 0.0)
    } else if v.y > v.x.abs() {
        v2(0.0, 1.0)
    } else {
        v2(0.0, -1.0)
    }
}

/// Recompute the world points of a connector from its bindings. Returns None if unchanged
/// inputs are missing.
pub fn resolve(doc: &Document, id: NodeId) -> Option<Vec<V2>> {
    let n = doc.get(id)?;
    let wr = doc.world_rect(id)?;
    let NodeKind::Connector {
        points,
        start,
        end,
        route,
        start_head,
        end_head,
        ..
    } = &n.kind
    else {
        return None;
    };
    let mut pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
    if pts.len() < 2 {
        return Some(pts);
    }
    let focus = |b: &crate::model::Binding| -> Option<(V2, Rect, f64, f64)> {
        let t = doc.get(b.node)?;
        let tr = doc.world_rect(b.node)?;
        let focus = if b.floating {
            tr.center()
        } else {
            global_fixed(b.fixed, tr)
        };
        Some((focus, tr, t.style.stroke_width, t.style.perimeter.max(0.0)))
    };
    let sf = start.as_ref().and_then(|b| focus(b).map(|f| (b, f)));
    let ef = end.as_ref().and_then(|b| focus(b).map(|f| (b, f)));
    if *route == Route::Elbow {
        let last = pts.len() - 1;
        let s = sf.map_or(pts[0], |(_, (f, _, _, _))| f);
        let e = ef.map_or(pts[last], |(_, (f, _, _, _))| f);
        return Some(route_elbow(
            doc,
            s,
            sf.map(|(b, (_, r, sw, _))| (b.node, r, sw)),
            e,
            ef.map(|(b, (_, r, sw, _))| (b.node, r, sw)),
            *start_head != Head::None,
            *end_head != Head::None,
        ));
    }
    let n_pts = pts.len();
    // Anchor each end toward the other end's focus (2 points) or its neighbor point.
    let s_anchor = if n_pts == 2 {
        ef.map_or(pts[1], |(_, (f, _, _, _))| f)
    } else {
        pts[1]
    };
    let e_anchor = if n_pts == 2 {
        sf.map_or(pts[0], |(_, (f, _, _, _))| f)
    } else {
        pts[n_pts - 2]
    };
    if let Some((b, (f, _, sw, per))) = sf {
        pts[0] = if b.inside {
            f
        } else {
            outline_hit(doc, b.node, f, s_anchor, binding_gap(sw) + per).unwrap_or(f)
        };
    }
    if let Some((b, (f, _, sw, per))) = ef {
        pts[n_pts - 1] = if b.inside {
            f
        } else {
            outline_hit(doc, b.node, f, e_anchor, binding_gap(sw) + per).unwrap_or(f)
        };
    }
    Some(pts)
}

// ---------------------------------------------------------------------------------- elbow

#[derive(Clone, Copy, PartialEq)]
struct Dir(i8, i8);

const UP: Dir = Dir(0, -1);
const RIGHT: Dir = Dir(1, 0);
const DOWN: Dir = Dir(0, 1);
const LEFT: Dir = Dir(-1, 0);

fn dir_of(v: V2) -> Dir {
    let h = vector_heading(v);
    Dir(h.x as i8, h.y as i8)
}

fn flip(d: Dir) -> Dir {
    Dir(-d.0, -d.1)
}

/// Excalidraw's `estimateSegmentCount` lookup (start node `s`, end node `e`).
fn estimate_segments(s: V2, e: V2, sh: Dir, eh: Dir) -> f64 {
    let r = if eh == RIGHT {
        if sh == RIGHT {
            if s.x >= e.x {
                4
            } else if s.y == e.y {
                0
            } else {
                2
            }
        } else if sh == UP {
            if s.y > e.y && s.x < e.x { 1 } else { 3 }
        } else if sh == DOWN {
            if s.y < e.y && s.x < e.x { 1 } else { 3 }
        } else if s.y == e.y {
            4
        } else {
            2
        }
    } else if eh == LEFT {
        if sh == RIGHT {
            if s.y == e.y { 4 } else { 2 }
        } else if sh == UP {
            if s.y > e.y && s.x > e.x { 1 } else { 3 }
        } else if sh == DOWN {
            if s.y < e.y && s.x > e.x { 1 } else { 3 }
        } else if s.x <= e.x {
            4
        } else if s.y == e.y {
            0
        } else {
            2
        }
    } else if eh == UP {
        if sh == RIGHT {
            if s.y > e.y && s.x < e.x { 1 } else { 3 }
        } else if sh == UP {
            if s.y >= e.y {
                4
            } else if s.x == e.x {
                0
            } else {
                2
            }
        } else if sh == DOWN {
            if s.x == e.x { 4 } else { 2 }
        } else if s.y > e.y && s.x > e.x {
            1
        } else {
            3
        }
    } else if sh == RIGHT {
        if s.y < e.y && s.x < e.x { 1 } else { 3 }
    } else if sh == UP {
        if s.x == e.x { 4 } else { 2 }
    } else if sh == DOWN {
        if s.y <= e.y {
            4
        } else if s.x == e.x {
            0
        } else {
            2
        }
    } else if s.y < e.y && s.x > e.x {
        1
    } else {
        3
    };
    r as f64
}

fn manhattan(a: V2, b: V2) -> f64 {
    (a.x - b.x).abs() + (a.y - b.y).abs()
}

fn strictly_inside(r: &Rect, p: V2) -> bool {
    p.x > r.x && p.x < r.right() && p.y > r.y && p.y < r.bottom()
}

/// Orthogonal route from `s` to `e`. Bound ends leave their shape perpendicular to the
/// side they're on; the path avoids the shapes' padded boxes and every other shape on
/// the page near the route (draw.io's orthogonal routing around obstacles).
pub fn route_elbow(
    doc: &Document,
    s: V2,
    s_bound: Option<(NodeId, Rect, f64)>,
    e: V2,
    e_bound: Option<(NodeId, Rect, f64)>,
    s_head: bool,
    e_head: bool,
) -> Vec<V2> {
    // Attached near the center: leave toward the other end; otherwise from the nearest side.
    let centered = |r: Rect, p: V2| {
        let d = p - r.center();
        (d.x / r.w.max(1.0)).abs() < 0.15 && (d.y / r.h.max(1.0)).abs() < 0.15
    };
    let sh = match s_bound {
        Some((_, r, _)) if !centered(r, s) => dir_of(heading_from_box(r, s)),
        _ => dir_of(e - s),
    };
    let eh = match e_bound {
        Some((_, r, _)) if !centered(r, e) => dir_of(heading_from_box(r, e)),
        _ => dir_of(s - e),
    };
    // Endpoints on the outline + gap, then dongles out by the padding.
    let snap = |p: V2, b: Option<(NodeId, Rect, f64)>, h: Dir, head: bool| -> V2 {
        match b {
            Some((_, r, sw)) => {
                // Keep the attachment inside the side's span.
                let p = v2(p.x.clamp(r.x, r.right()), p.y.clamp(r.y, r.bottom()));
                let gap =
                    binding_gap(sw) * if head { 1.0 } else { 0.5 } + if head { 2.0 } else { 0.0 };
                match h {
                    UP => v2(p.x, r.y - gap),
                    DOWN => v2(p.x, r.bottom() + gap),
                    LEFT => v2(r.x - gap, p.y),
                    _ => v2(r.right() + gap, p.y),
                }
            }
            None => p,
        }
    };
    let sp = snap(s, s_bound, sh, s_head);
    let ep = snap(e, e_bound, eh, e_head);
    let pad = |b: Option<(NodeId, Rect, f64)>, p: V2| {
        b.map(|(_, r, _)| r.inflate(ELBOW_PADDING))
            .unwrap_or_else(|| Rect::new(p.x - 2.0, p.y - 2.0, 4.0, 4.0))
    };
    let sa = pad(s_bound, sp);
    let ea = pad(e_bound, ep);
    let dongle = |p: V2, h: Dir, r: Rect, bound: bool| -> V2 {
        if !bound {
            return p;
        }
        match h {
            UP => v2(p.x, r.y),
            DOWN => v2(p.x, r.bottom()),
            LEFT => v2(r.x, p.y),
            _ => v2(r.right(), p.y),
        }
    };
    let sd = dongle(sp, sh, sa, s_bound.is_some());
    let ed = dongle(ep, eh, ea, e_bound.is_some());
    let overlap = strictly_inside(&ea, sd) || strictly_inside(&sa, ed);
    let common = sa.union(&ea).inflate(ELBOW_PADDING);
    // Every shape near the route blocks it, not just the two endpoints'
    // shapes. Skipped when the endpoints' shapes overlap: that route is
    // degenerate anyway and the empty grid keeps it well behaved.
    let extra: Vec<Rect> = if overlap {
        vec![]
    } else {
        let skip: Vec<NodeId> = [s_bound.map(|(id, _, _)| id), e_bound.map(|(id, _, _)| id)]
            .into_iter()
            .flatten()
            .collect();
        route_obstacles(doc, common.inflate(ELBOW_PADDING), &skip)
    };
    let obstacles: Vec<Rect> = if overlap {
        vec![]
    } else {
        [s_bound.map(|_| sa), e_bound.map(|_| ea)]
            .into_iter()
            .flatten()
            .chain(extra.iter().copied())
            .collect()
    };
    // Grid lines.
    let mut xs: Vec<f64> = vec![
        sd.x,
        ed.x,
        sa.x,
        sa.right(),
        ea.x,
        ea.right(),
        common.x,
        common.right(),
    ];
    let mut ys: Vec<f64> = vec![
        sd.y,
        ed.y,
        sa.y,
        sa.bottom(),
        ea.y,
        ea.bottom(),
        common.y,
        common.bottom(),
    ];
    // Obstacle edges become grid lines so the search can path around them.
    for o in &extra {
        xs.push(o.x);
        xs.push(o.right());
        ys.push(o.y);
        ys.push(o.bottom());
    }
    // Midlines between the shapes give the classic Z routes.
    xs.push((sa.right() + ea.x) * 0.5);
    xs.push((ea.right() + sa.x) * 0.5);
    ys.push((sa.bottom() + ea.y) * 0.5);
    ys.push((ea.bottom() + sa.y) * 0.5);
    let dedup = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    };
    dedup(&mut xs);
    dedup(&mut ys);
    let cols = xs.len();
    let idx = |x: f64, y: f64| -> Option<usize> {
        let c = xs.iter().position(|v| (v - x).abs() < 1e-9)?;
        let r = ys.iter().position(|v| (v - y).abs() < 1e-9)?;
        Some(r * cols + c)
    };
    let (Some(start), Some(goal)) = (idx(sd.x, sd.y), idx(ed.x, ed.y)) else {
        return vec![sp, ep];
    };
    let n = xs.len() * ys.len();
    let pos = |i: usize| v2(xs[i % cols], ys[i / cols]);
    let bend = manhattan(sd, ed).max(1.0);
    let mut g = vec![f64::INFINITY; n];
    let mut f = vec![f64::INFINITY; n];
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut closed = vec![false; n];
    let mut open: Vec<usize> = vec![start];
    g[start] = 0.0;
    f[start] = 0.0;
    let mut found = false;
    while !open.is_empty() {
        // Lowest f (first on ties, like the binary heap's strict comparisons).
        let (k, _) = open
            .iter()
            .enumerate()
            .fold((0, f64::INFINITY), |acc, (k, i)| {
                if f[*i] < acc.1 { (k, f[*i]) } else { acc }
            });
        let cur = open.remove(k);
        if closed[cur] {
            continue;
        }
        if cur == goal {
            found = true;
            break;
        }
        closed[cur] = true;
        let (c, r) = (cur % cols, cur / cols);
        let prev = match parent[cur] {
            Some(p) => dir_of(pos(cur) - pos(p)),
            None => sh,
        };
        let nbrs = [
            (r > 0).then(|| cur - cols),
            (c + 1 < cols).then(|| cur + 1),
            (r + 1 < ys.len()).then(|| cur + cols),
            (c > 0).then(|| cur - 1),
        ];
        for (i, nb) in nbrs.into_iter().enumerate() {
            let Some(nb) = nb else { continue };
            if closed[nb] {
                continue;
            }
            let mid = (pos(nb) + pos(cur)) * 0.5;
            if obstacles.iter().any(|o| strictly_inside(o, mid)) {
                continue;
            }
            let nh = [UP, RIGHT, DOWN, LEFT][i];
            if nh == flip(prev) || (nb == start && nh == sh) || (nb == goal && nh == eh) {
                continue;
            }
            let ng =
                g[cur] + manhattan(pos(nb), pos(cur)) + if prev != nh { bend.powi(3) } else { 0.0 };
            if ng < g[nb] {
                parent[nb] = Some(cur);
                g[nb] = ng;
                f[nb] = ng
                    + manhattan(pos(goal), pos(nb))
                    + estimate_segments(pos(nb), pos(goal), nh, eh) * bend * bend;
                open.push(nb);
            }
        }
    }
    let mut path = Vec::new();
    if found {
        let mut cur = Some(goal);
        while let Some(c) = cur {
            path.push(pos(c));
            cur = parent[c];
        }
        path.reverse();
    } else {
        path = vec![sd, v2(ed.x, sd.y), ed];
    }
    let mut pts = vec![sp];
    pts.extend(path);
    pts.push(ep);
    simplify_orthogonal(pts)
}

/// Drop duplicate and collinear points.
fn simplify_orthogonal(pts: Vec<V2>) -> Vec<V2> {
    let mut out: Vec<V2> = Vec::new();
    for p in pts {
        if out.last().is_some_and(|l| l.dist(p) <= 1.0) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let h1 = (b.y - a.y).abs() < (b.x - a.x).abs();
            let h2 = (p.y - b.y).abs() < (p.x - b.x).abs();
            if h1 == h2 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

// ---------------------------------------------------------------------------------- drawing

/// The connector's line through world points.
pub fn line_path(pts: &[V2], route: Route) -> VPath {
    match route {
        Route::Straight => VPath::polyline(pts, false),
        Route::Curved if pts.len() >= 3 => {
            // Catmull-Rom through the points (rough.js `curveToBezier`).
            let mut p = VPath::new();
            let mut q = vec![pts[0]];
            q.extend_from_slice(pts);
            q.push(*pts.last().unwrap());
            p.move_to(pts[0]);
            for i in 1..q.len() - 2 {
                let b1 = q[i] + (q[i + 1] - q[i - 1]) / 6.0;
                let b2 = q[i + 1] - (q[i + 2] - q[i]) / 6.0;
                p.cubic_to(b1, b2, q[i + 1]);
            }
            p
        }
        Route::Curved => VPath::polyline(pts, false),
        Route::Elbow => {
            // Rounded corners (Excalidraw `generateElbowArrowShape`, radius 16).
            let mut p = VPath::new();
            p.move_to(pts[0]);
            for i in 1..pts.len().saturating_sub(1) {
                let (a, c, b) = (pts[i - 1], pts[i], pts[i + 1]);
                let r = ELBOW_RADIUS.min(c.dist(a) / 2.0).min(c.dist(b) / 2.0);
                p.line_to(c + (a - c).normalized() * r);
                p.quad_to(c, c + (b - c).normalized() * r);
            }
            p.line_to(*pts.last().unwrap());
            p
        }
    }
}

/// Flattened route with its total length. The longest subpath wins, as with
/// `path_midpoint`.
pub fn route_polyline(pts: &[V2], route: Route) -> Option<(Vec<V2>, f64)> {
    if pts.len() < 2 {
        return None;
    }
    let path = line_path(pts, route);
    let polys = path.flatten(0.25);
    let poly = polys
        .iter()
        .max_by(|a, b| polyline_length(&a.pts).total_cmp(&polyline_length(&b.pts)))?;
    let total = polyline_length(&poly.pts);
    (total > 1e-9).then(|| (poly.pts.clone(), total))
}

/// Ramer-Douglas-Peucker simplification: keeps points that deviate more than
/// `tol` from the chord, so a flattened curve keeps its shape with far fewer
/// vertices.
fn simplify_polyline(pts: &[V2], tol: f64) -> Vec<V2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (p, q) = (pts[a], pts[b]);
        let d = q - p;
        let len2 = d.dot(d);
        let (mut far, mut best) = (a, 0.0f64);
        for (i, pt) in pts.iter().enumerate().take(b).skip(a + 1) {
            let t = if len2 <= 1e-12 {
                0.0
            } else {
                ((*pt - p).dot(d) / len2).clamp(0.0, 1.0)
            };
            let dist = pt.dist(p + d * t);
            if dist > best {
                best = dist;
                far = i;
            }
        }
        if best > tol {
            keep[far] = true;
            stack.push((a, far));
            stack.push((far, b));
        }
    }
    (0..pts.len())
        .filter(|&i| keep[i])
        .map(|i| pts[i])
        .collect()
}

/// Where the span `a -> b` crosses `hole`, as entry and exit parameters
/// (Liang-Barsky). `None` when the span misses the rectangle entirely.
fn span_hole(a: V2, b: V2, hole: Rect) -> Option<(f64, f64)> {
    let d = b - a;
    let mut t0: f64 = 0.0;
    let mut t1: f64 = 1.0;
    for (p, q) in [
        (-d.x, a.x - hole.x),
        (d.x, hole.x + hole.w - a.x),
        (-d.y, a.y - hole.y),
        (d.y, hole.y + hole.h - a.y),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then_some((t0.clamp(0.0, 1.0), t1.clamp(0.0, 1.0)))
}

/// The routed line with the stretches behind each `holes[i]` removed. A
/// connector label without a solid background cuts the line behind its text
/// so the characters stay readable; spans crossing a hole are trimmed at its
/// boundary. Several labels cut several stretches (draw.io's extra labels).
///
/// Returns `None` when the whole line falls inside the holes. When nothing is
/// cut the original path is returned unchanged, so smooth routes keep their
/// exact curves.
pub fn cut_line_path(pts: &[V2], route: Route, holes: &[Rect]) -> Option<VPath> {
    let path = line_path(pts, route);
    let Some((poly, _)) = route_polyline(pts, route) else {
        return Some(path);
    };
    let poly = simplify_polyline(&poly, 0.25);
    let mut runs: Vec<Vec<V2>> = Vec::new();
    let mut cur: Vec<V2> = Vec::new();
    let mut in_gap = false;
    let mut cut = false;
    for pair in poly.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        // Spans cut by every hole on this segment, merged so overlapping or
        // touching holes act as one stretch.
        let mut spans: Vec<(f64, f64)> = holes.iter().filter_map(|h| span_hole(a, b, *h)).collect();
        spans.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
        let mut merged: Vec<(f64, f64)> = Vec::new();
        for (t0, t1) in spans {
            match merged.last_mut() {
                Some(last) if t0 <= last.1 => last.1 = last.1.max(t1),
                _ => merged.push((t0, t1)),
            }
        }
        if merged.is_empty() {
            if in_gap || cur.is_empty() {
                cur = vec![a];
                in_gap = false;
            }
            cur.push(b);
            continue;
        }
        for &(t0, t1) in &merged {
            cut = true;
            let p_in = a + (b - a) * t0;
            let p_out = a + (b - a) * t1;
            if !in_gap {
                if cur.is_empty() {
                    cur = vec![a];
                }
                if t0 > 1e-9 {
                    cur.push(p_in);
                }
                if cur.len() > 1 {
                    runs.push(std::mem::take(&mut cur));
                } else {
                    cur.clear();
                }
            }
            if t1 < 1.0 - 1e-9 {
                // The line resumes inside this span and continues on.
                cur = vec![p_out];
                in_gap = false;
            } else {
                in_gap = true;
            }
        }
        // After the last span the line continues toward `b`; keep it pending
        // so the next segment decides whether to join or restart.
        let last_t1 = merged.last().map_or(0.0, |s| s.1);
        if last_t1 < 1.0 - 1e-9 {
            cur.push(b);
        }
    }
    if !in_gap && cur.len() > 1 {
        runs.push(cur);
    }
    if !cut {
        return Some(path);
    }
    if runs.is_empty() {
        return None;
    }
    let mut p = VPath::new();
    for run in &runs {
        p.move_to(run[0]);
        for q in &run[1..] {
            p.line_to(*q);
        }
    }
    Some(p)
}

// ---------------------------------------------------------------------------------- line jumps

/// Half gap of a line hop: draw.io computes `(jumpSize - 2) / 2` plus the
/// stroke width, so wider lines get proportionally wider hops.
pub fn jump_half_size(jump_size: f64, stroke_width: f64) -> f64 {
    ((jump_size - 2.0) / 2.0 + stroke_width).max(0.0)
}

/// Where the span `a -> b` crosses `c -> d`. Parallel or collinear spans and
/// touches at the endpoints do not count.
fn seg_crossing(a: V2, b: V2, c: V2, d: V2) -> Option<V2> {
    let r = b - a;
    let s = d - c;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-9 {
        return None;
    }
    let ac = c - a;
    let t = (ac.x * s.y - ac.y * s.x) / denom;
    let u = (ac.x * r.y - ac.y * r.x) / denom;
    (t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9).then_some(a + r * t)
}

/// The points where the routed line crosses one of the `others` polylines,
/// ordered along the line. A connector only hops over connectors below it in
/// paint order, so callers pass the routes drawn earlier. Crossings within
/// `0.5` world units of the end of either span are ignored, so lines that meet
/// or share a corner do not hop (draw.io's threshold rule).
pub fn line_jumps(pts: &[V2], route: Route, others: &[Vec<V2>], half: f64) -> Vec<V2> {
    if others.is_empty() || half <= 0.0 {
        return Vec::new();
    }
    let Some((poly, _)) = route_polyline(pts, route) else {
        return Vec::new();
    };
    let poly = simplify_polyline(&poly, 0.25);
    const THRESH: f64 = 0.5;
    let mut found: Vec<(f64, V2)> = Vec::new();
    let mut dist = 0.0;
    for pair in poly.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = a.dist(b);
        if len > 1e-9 {
            for other in others {
                for op in other.windows(2) {
                    let Some(p) = seg_crossing(a, b, op[0], op[1]) else {
                        continue;
                    };
                    let along = dist + (p - a).dot(b - a) / (len * len) * len;
                    // Not at the ends of either span: lines that meet an
                    // endpoint or pass through a joint do not hop.
                    if along - dist < THRESH
                        || dist + len - along < THRESH
                        || p.dist(op[0]) < THRESH
                        || p.dist(op[1]) < THRESH
                    {
                        continue;
                    }
                    found.push((along, p));
                }
            }
        }
        dist += len;
    }
    found.sort_by(|x, y| x.0.total_cmp(&y.0));
    // One hop per crossing: collapse crossings that land on the same spot.
    let mut out: Vec<V2> = Vec::new();
    for (along, p) in found {
        if out.last().is_some_and(|q: &V2| {
            q.dist(p) < THRESH || (along_of(&poly, *q) - along).abs() < THRESH
        }) {
            continue;
        }
        out.push(p);
    }
    out
}

/// Distance along the polyline of the point on it closest to `p`.
fn along_of(poly: &[V2], p: V2) -> f64 {
    let mut best = (f64::INFINITY, 0.0);
    let mut dist = 0.0;
    for pair in poly.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let d = b - a;
        let len2 = d.dot(d);
        let t = if len2 <= 1e-12 {
            0.0
        } else {
            ((p - a).dot(d) / len2).clamp(0.0, 1.0)
        };
        let q = a + d * t;
        let dd = q.dist(p);
        if dd < best.0 {
            best = (dd, dist + t * d.len());
        }
        dist += d.len();
    }
    best.1
}

/// The routed line with a hop at every crossing. `jumps` are crossing points
/// ordered along the line (see `line_jumps`); `half` is the half gap around
/// each crossing. The styles follow draw.io's connector painting: `Arc` bumps
/// over the crossing, `Gap` interrupts the line, `Sharp` steps sideways, and
/// `Line` draws a tick on each side of the gap. `hole` optionally cuts the
/// label gap out of the remaining runs.
///
/// Returns `None` when there is nothing to hop, so callers keep the original
/// path and smooth routes keep their exact curves.
pub fn jumped_line_path(
    pts: &[V2],
    route: Route,
    jumps: &[V2],
    style: LineJump,
    half: f64,
    holes: &[Rect],
) -> Option<VPath> {
    if jumps.is_empty() || style == LineJump::None || half <= 0.0 {
        return None;
    }
    let (poly, _) = route_polyline(pts, route)?;
    let poly = simplify_polyline(&poly, 0.25);
    let total = polyline_length(&poly);
    if total <= 1e-9 {
        return None;
    }
    // Cut ranges around each crossing, dropping hops that would run past an
    // end of the line.
    let mut cuts: Vec<(f64, V2)> = Vec::new();
    for j in jumps {
        let along = along_of(&poly, *j);
        if along - half < 1e-9 || along + half > total - 1e-9 {
            continue;
        }
        cuts.push((along, *j));
    }
    if cuts.is_empty() {
        return None;
    }
    cuts.sort_by(|x, y| x.0.total_cmp(&y.0));

    // Walk the polyline by distance, drawing runs between the cuts and a hop
    // shape across each cut.
    let at = |d: f64| -> V2 {
        let mut dist = 0.0;
        for pair in poly.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = a.dist(b);
            if dist + len >= d - 1e-9 || dist + len >= total - 1e-9 {
                let t = ((d - dist) / len).clamp(0.0, 1.0);
                return a + (b - a) * t;
            }
            dist += len;
        }
        *poly.last().unwrap()
    };
    let tangent =
        |d: f64| -> V2 { (at((d + 0.5).min(total)) - at((d - 0.5).max(0.0))).normalized() };
    let mut p = VPath::new();
    let mut pen: Option<V2> = None;
    // Emits the polyline between two distances, trimming the label hole out of
    // each straight piece.
    let line = |p: &mut VPath, pen: &mut Option<V2>, from: f64, to: f64| {
        let mut prev = at(from);
        if pen.is_none() {
            p.move_to(prev);
            *pen = Some(prev);
        }
        let mut dist = from;
        for pair in poly.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let len = a.dist(b);
            let end = (dist + len).min(to);
            if end > dist + 1e-9 {
                let next = if end >= to - 1e-9 { at(to) } else { b };
                // The first hole overlapping this piece cuts it; labels
                // rarely share one straight piece, and overlapping holes
                // would cut the same stretch anyway.
                let span = holes
                    .iter()
                    .filter_map(|h| span_hole(prev, next, *h))
                    .next();
                match span {
                    None => {
                        p.line_to(next);
                        *pen = Some(next);
                    }
                    Some((t0, t1)) => {
                        let d = next - prev;
                        let pin = prev + d * t0;
                        let pout = prev + d * t1;
                        if t0 > 1e-9 {
                            p.line_to(pin);
                        }
                        p.move_to(pout);
                        *pen = Some(pout);
                        // Keep drawing to the end of the span; the next
                        // window continues from there.
                        if t1 < 1.0 - 1e-9 {
                            p.line_to(next);
                            *pen = Some(next);
                        }
                    }
                }
                prev = next;
            }
            dist += len;
            if dist >= to - 1e-9 {
                break;
            }
        }
    };
    let mut cursor = 0.0;
    for (along, _) in &cuts {
        let start = (along - half).max(cursor);
        if start > cursor + 1e-9 {
            line(&mut p, &mut pen, cursor, start);
        }
        let dir = tangent(*along);
        let x = at(*along);
        let n = dir * half;
        let p0 = x - n;
        let p1 = x + n;
        // draw.io picks the bump side from the travel direction so hops lean
        // the same way on every line.
        let f = if n.x < -1e-9 || (n.x.abs() <= 1e-9 && n.y <= 1e-9) {
            1.0
        } else {
            -1.0
        };
        let perp = v2(-n.y * f, n.x * f);
        match style {
            LineJump::Arc => {
                // draw.io scales the same sideways offset by 1.3 for the arc
                // (`perp` already carries the side sign).
                p.cubic_to(p0 + perp * 1.3, p1 + perp * 1.3, p1);
                pen = Some(p1);
            }
            LineJump::Sharp => {
                p.line_to(p0 + perp);
                p.line_to(p1 + perp);
                p.line_to(p1);
                pen = Some(p1);
            }
            LineJump::Line => {
                p.move_to(p0 + perp);
                p.line_to(p0 - perp);
                p.move_to(p1 - perp);
                p.line_to(p1 + perp);
                p.move_to(p1);
                pen = Some(p1);
            }
            LineJump::Gap => {
                p.move_to(p1);
                pen = Some(p1);
            }
            LineJump::None => unreachable!("checked above"),
        }
        cursor = along + half;
    }
    if cursor < total - 1e-9 {
        line(&mut p, &mut pen, cursor, total);
    }
    Some(p)
}

/// Point at normalized distance `t` along the routed line, 0 at the start and
/// 1 at the end.
pub fn path_point_at(pts: &[V2], route: Route, t: f64) -> V2 {
    let fallback = match (pts.first(), pts.last()) {
        (Some(a), Some(b)) => (*a + *b) * 0.5,
        (Some(a), None) => *a,
        _ => V2::ZERO,
    };
    let Some((poly, total)) = route_polyline(pts, route) else {
        return fallback;
    };
    let target = total * t.clamp(0.0, 1.0);
    let mut walked = 0.0;
    for pair in poly.windows(2) {
        let seg = pair[0].dist(pair[1]);
        if walked + seg >= target {
            let k = if seg <= 1e-9 {
                0.0
            } else {
                (target - walked) / seg
            };
            return pair[0] + (pair[1] - pair[0]) * k;
        }
        walked += seg;
    }
    *poly.last().unwrap_or(&fallback)
}

/// Closest point on the routed line to `p`, with its normalized position.
pub fn closest_point(pts: &[V2], route: Route, p: V2) -> Option<(f64, V2)> {
    let (poly, total) = route_polyline(pts, route)?;
    let mut walked = 0.0;
    let mut best: Option<(f64, f64, V2)> = None;
    for pair in poly.windows(2) {
        let seg = pair[0].dist(pair[1]);
        let k = if seg <= 1e-9 {
            0.0
        } else {
            ((p - pair[0]).dot(pair[1] - pair[0]) / (seg * seg)).clamp(0.0, 1.0)
        };
        let q = pair[0] + (pair[1] - pair[0]) * k;
        let d = q.dist(p);
        if best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, (walked + seg * k) / total, q));
        }
        walked += seg;
    }
    best.map(|(_, t, q)| (t, q))
}

/// Point halfway along the routed line. Connector labels start here, so an
/// elbow label follows the bend instead of the bounding-box center.
pub fn path_midpoint(pts: &[V2], route: Route) -> V2 {
    path_point_at(pts, route, 0.5)
}

fn polyline_length(pts: &[V2]) -> f64 {
    pts.windows(2).map(|p| p[0].dist(p[1])).sum()
}

/// Move both line tips by draw.io's start/end perimeter spacing. Positive values
/// pull a tip away from its shape, into the line; negative values push it inside
/// the outline.
pub fn offset_endpoints(pts: &mut [V2], start: f64, end: f64) {
    if pts.len() < 2 {
        return;
    }
    let last = pts.len() - 1;
    let into_line = (pts[1] - pts[0]).normalized();
    pts[0] = pts[0] + into_line * start;
    let back_into_line = (pts[last - 1] - pts[last]).normalized();
    pts[last] = pts[last] + back_into_line * end;
}

/// Head geometry at `tip`, arriving from `from`: (stroked lines, filled polygons).
/// `size` uses draw.io's marker scale, where 6 is the default head size.
pub fn head_shapes(
    head: Head,
    tip: V2,
    from: V2,
    sw: f64,
    seg_len: f64,
    size: f64,
) -> (Vec<Vec<V2>>, Vec<Vec<V2>>) {
    let base: f64 = match head {
        Head::Arrow => 25.0,
        Head::Diamond => 12.0,
        _ => 15.0,
    };
    let scale = (size / DEFAULT_HEAD_SIZE).clamp(0.1, 20.0);
    let size =
        (base * scale).min(seg_len * if head == Head::Diamond { 0.25 } else { 0.5 }) + sw * 0.5;
    let dir = (tip - from).normalized();
    if dir == V2::ZERO {
        return (vec![], vec![]);
    }
    let back = tip - dir * size;
    let rot = |p: V2, c: V2, deg: f64| {
        let a = deg.to_radians();
        let (s, co) = a.sin_cos();
        let d = p - c;
        c + v2(d.x * co - d.y * s, d.x * s + d.y * co)
    };
    match head {
        Head::None => (vec![], vec![]),
        Head::Arrow => {
            let a = rot(back, tip, -20.0);
            let b = rot(back, tip, 20.0);
            (vec![vec![a, tip, b]], vec![])
        }
        Head::Bar => {
            let a = rot(back, tip, -90.0);
            let b = rot(back, tip, 90.0);
            let half = (a - b) * 0.5;
            (vec![vec![tip + half * 0.6, tip - half * 0.6]], vec![])
        }
        Head::Triangle => {
            let a = rot(back, tip, -25.0);
            let b = rot(back, tip, 25.0);
            (vec![], vec![vec![tip, a, b]])
        }
        Head::Dot => {
            let r = (size * 0.35).max(sw);
            let c = tip - dir * r;
            let pts = (0..24)
                .map(|i| {
                    let a = i as f64 / 24.0 * std::f64::consts::TAU;
                    c + v2(a.cos(), a.sin()) * r
                })
                .collect();
            (vec![], vec![pts])
        }
        Head::Diamond => {
            let a = rot(back, tip, -25.0);
            let b = rot(back, tip, 25.0);
            let far = tip - dir * size * 2.0;
            (vec![], vec![vec![tip, a, far, b]])
        }
    }
}

/// Where the line must stop so a filled head doesn't show the line through it.
pub fn line_inset(head: Head) -> f64 {
    match head {
        Head::Triangle | Head::Diamond | Head::Dot => 1.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::{Recording, Tx};

    #[test]
    fn con21_elbow_route_avoids_third_shape() {
        let doc = doc_with_shapes(&[
            (1, NodeKind::Rect, Rect::new(0.0, 100.0, 100.0, 60.0), true),
            (
                2,
                NodeKind::Rect,
                Rect::new(400.0, 100.0, 100.0, 60.0),
                true,
            ),
            (3, NodeKind::Rect, Rect::new(200.0, 80.0, 60.0, 100.0), true),
        ]);
        let a = Rect::new(0.0, 100.0, 100.0, 60.0);
        let b = Rect::new(400.0, 100.0, 100.0, 60.0);
        let obstacle = Rect::new(200.0, 80.0, 60.0, 100.0).inflate(OBSTACLE_PADDING);
        let pts = route_elbow(
            &doc,
            v2(100.0, 130.0),
            Some((NodeId(1), a, 2.0)),
            v2(400.0, 130.0),
            Some((NodeId(2), b, 2.0)),
            false,
            false,
        );
        assert!(pts.len() >= 2, "{pts:?}");
        for w in pts.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6,
                "not orthogonal: {pts:?}"
            );
            let m = (w[0] + w[1]) * 0.5;
            assert!(
                !strictly_inside(&obstacle, m),
                "route crosses obstacle: mid={m:?} seg={w:?} pts={pts:?}"
            );
        }
    }

    #[test]
    fn elbow_routes_are_orthogonal_and_leave_perpendicular() {
        let doc = Document::default();
        let a = Rect::new(0.0, 0.0, 100.0, 60.0);
        let b = Rect::new(300.0, 200.0, 100.0, 60.0);
        let pts = route_elbow(
            &doc,
            v2(100.0, 30.0),
            Some((NodeId(1), a, 2.0)),
            v2(300.0, 230.0),
            Some((NodeId(2), b, 2.0)),
            false,
            true,
        );
        assert!(pts.len() >= 3, "{pts:?}");
        for w in pts.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6,
                "not orthogonal: {pts:?}"
            );
        }
        assert!(
            pts[0].x > 100.0 && (pts[0].y - 30.0).abs() < 1e-9,
            "leaves the right side: {pts:?}"
        );
        assert!(
            pts.last().unwrap().x < 300.0,
            "enters the left side: {pts:?}"
        );
        // The route never crosses either box.
        for w in pts.windows(2) {
            let m = (w[0] + w[1]) * 0.5;
            assert!(!strictly_inside(&a, m) && !strictly_inside(&b, m));
        }
    }

    fn doc_with_shapes(rects: &[(u64, NodeKind, Rect, bool)]) -> Document {
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        {
            let mut tx = Tx::new(&mut doc, &mut rec);
            for (id, kind, r, visible) in rects {
                let mut n = Node::new(NodeId(*id), kind.clone(), *r);
                n.visible = *visible;
                tx.insert(n, None, None);
            }
        }
        doc
    }

    fn connector_doc(start_floating: bool) -> Document {
        doc_with_shapes(&[
            (1, NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0), true),
            (2, NodeKind::Rect, Rect::new(300.0, 0.0, 100.0, 60.0), true),
            (
                3,
                NodeKind::Connector {
                    points: vec![v2(0.0, 30.0), v2(400.0, 30.0)],
                    start: Some(crate::model::Binding {
                        node: NodeId(1),
                        fixed: v2(1.0, 0.0),
                        inside: false,
                        floating: start_floating,
                    }),
                    end: Some(crate::model::Binding {
                        node: NodeId(2),
                        fixed: v2(0.5, 0.5),
                        inside: false,
                        floating: false,
                    }),
                    route: Route::Straight,
                    start_head: Head::None,
                    end_head: Head::Arrow,
                    jump: LineJump::None,
                    jump_size: 6.0,
                },
                Rect::new(0.0, 0.0, 400.0, 60.0),
                true,
            ),
        ])
    }

    #[test]
    fn floating_end_follows_the_perimeter_while_a_fixed_end_keeps_its_point() {
        let mut fixed = connector_doc(false);
        let before = resolve(&fixed, NodeId(3)).unwrap()[0];
        fixed.nodes.get_mut(&NodeId(1)).unwrap().rect = Rect::new(0.0, 100.0, 100.0, 60.0);
        let after = resolve(&fixed, NodeId(3)).unwrap()[0];
        let before_y = (before.y - 0.0) / 60.0;
        let after_y = (after.y - 100.0) / 60.0;
        assert!(
            (after_y - before_y).abs() < 0.06,
            "fixed end kept its relative point: {before:?} -> {after:?}"
        );

        let mut floating = connector_doc(true);
        let before = resolve(&floating, NodeId(3)).unwrap()[0];
        floating.nodes.get_mut(&NodeId(1)).unwrap().rect = Rect::new(0.0, 100.0, 100.0, 60.0);
        let after = resolve(&floating, NodeId(3)).unwrap()[0];
        let moved = (((after.y - 100.0) / 60.0) - (before.y / 60.0)).abs();
        assert!(
            moved > 0.25,
            "floating end should change side, moved {moved}: {before:?} -> {after:?}"
        );
    }

    #[test]
    fn elbow_routes_around_an_unrelated_shape() {
        let a = Rect::new(0.0, 0.0, 100.0, 60.0);
        let b = Rect::new(300.0, 200.0, 100.0, 60.0);
        // A wall sitting on every direct Z route between the two shapes.
        let wall = Rect::new(140.0, 100.0, 120.0, 120.0);
        let doc = doc_with_shapes(&[
            (1, NodeKind::Rect, a, true),
            (2, NodeKind::Rect, b, true),
            (3, NodeKind::Rect, wall, true),
        ]);
        let pts = route_elbow(
            &doc,
            v2(100.0, 30.0),
            Some((NodeId(1), a, 2.0)),
            v2(300.0, 230.0),
            Some((NodeId(2), b, 2.0)),
            false,
            true,
        );
        assert!(pts.len() >= 3, "{pts:?}");
        for w in pts.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6,
                "not orthogonal: {pts:?}"
            );
        }
        // No point of the route runs through the wall (boundary touches are
        // allowed, like the padded endpoint boxes).
        let inner = wall.inflate(-0.5);
        for w in pts.windows(2) {
            let steps = (w[0].dist(w[1]) / 2.0).ceil() as usize;
            for k in 0..=steps {
                let p = w[0] + (w[1] - w[0]) * (k as f64 / steps.max(1) as f64);
                assert!(!inner.contains(p), "route point {p:?} inside the wall");
            }
        }
        assert!(
            pts[0].x > 100.0 && (pts[0].y - 30.0).abs() < 1e-9,
            "still leaves the right side: {pts:?}"
        );
        assert!(pts.last().unwrap().x < 300.0, "still enters the left side");
    }

    #[test]
    fn elbow_ignores_shapes_far_from_the_route() {
        let a = Rect::new(0.0, 0.0, 100.0, 60.0);
        let b = Rect::new(300.0, 200.0, 100.0, 60.0);
        let plain = {
            let doc = Document::default();
            route_elbow(
                &doc,
                v2(100.0, 30.0),
                Some((NodeId(1), a, 2.0)),
                v2(300.0, 230.0),
                Some((NodeId(2), b, 2.0)),
                false,
                true,
            )
        };
        let doc = doc_with_shapes(&[(
            3,
            NodeKind::Rect,
            Rect::new(1000.0, 1000.0, 100.0, 100.0),
            true,
        )]);
        let with = route_elbow(
            &doc,
            v2(100.0, 30.0),
            Some((NodeId(1), a, 2.0)),
            v2(300.0, 230.0),
            Some((NodeId(2), b, 2.0)),
            false,
            true,
        );
        assert_eq!(plain, with, "a distant shape must not change the route");
    }

    #[test]
    fn elbow_ignores_connectors_and_hidden_shapes() {
        let a = Rect::new(0.0, 0.0, 100.0, 60.0);
        let b = Rect::new(300.0, 200.0, 100.0, 60.0);
        let plain = {
            let doc = Document::default();
            route_elbow(
                &doc,
                v2(100.0, 30.0),
                Some((NodeId(1), a, 2.0)),
                v2(300.0, 230.0),
                Some((NodeId(2), b, 2.0)),
                false,
                true,
            )
        };
        let wall = Rect::new(140.0, 100.0, 120.0, 120.0);
        let connector = NodeKind::Connector {
            points: vec![v2(0.0, 0.0), v2(100.0, 100.0)],
            start: None,
            end: None,
            route: Route::Straight,
            start_head: Head::None,
            end_head: Head::None,
            jump: LineJump::None,
            jump_size: 6.0,
        };
        let doc = doc_with_shapes(&[
            (1, NodeKind::Rect, a, true),
            (2, NodeKind::Rect, b, true),
            (3, connector, wall, true),
            (4, NodeKind::Rect, wall, false),
        ]);
        let with = route_elbow(
            &doc,
            v2(100.0, 30.0),
            Some((NodeId(1), a, 2.0)),
            v2(300.0, 230.0),
            Some((NodeId(2), b, 2.0)),
            false,
            true,
        );
        assert_eq!(
            plain, with,
            "connectors and hidden shapes are not obstacles"
        );
    }

    #[test]
    fn orbit_binding_stops_at_outline_plus_gap() {
        let mut doc = Document::default();
        let mut rec = Recording::new("t", &doc);
        let mut tx = Tx::new(&mut doc, &mut rec);
        let r = tx.alloc_id();
        tx.insert(
            Node::new(r, NodeKind::Rect, Rect::new(200.0, 0.0, 100.0, 100.0)),
            None,
            None,
        );
        let hit = outline_hit(
            tx.doc(),
            r,
            v2(250.0, 50.0),
            v2(0.0, 50.0),
            binding_gap(1.0),
        )
        .unwrap();
        assert!((hit.x - (200.0 - 5.5)).abs() < 1e-9, "{hit:?}");
        assert_eq!(
            fixed_point(v2(250.0, 50.0), Rect::new(200.0, 0.0, 100.0, 100.0)),
            v2(0.5001, 0.5001)
        );
    }

    fn flattened(p: &VPath) -> Vec<V2> {
        p.flatten(0.25)
            .iter()
            .flat_map(|poly| poly.pts.clone())
            .collect()
    }

    #[test]
    fn cut_line_path_cuts_a_straight_line_behind_the_label() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let hole = Rect::new(80.0, -10.0, 40.0, 20.0);
        let cut = cut_line_path(&pts, Route::Straight, &[hole]).unwrap();
        // Two runs: one move per gap side.
        assert_eq!(
            cut.segs
                .iter()
                .filter(|s| matches!(s, vector::Seg::M(_)))
                .count(),
            2,
            "{cut:?}"
        );
        let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
        for p in flattened(&cut) {
            assert!(!inner.contains(p), "point {p:?} inside the gap");
        }
        let xs: Vec<f64> = flattened(&cut).iter().map(|p| p.x).collect();
        assert!(*xs.first().unwrap() < 1e-9 && *xs.last().unwrap() > 199.0);
        // Sanity: the uncut line does cross the hole.
        assert!(
            line_path(&pts, Route::Straight).distance(hole.center(), 0.25) <= 0.5,
            "the hole sits on the line"
        );
    }

    #[test]
    fn cut_line_path_keeps_the_path_when_the_label_misses_the_line() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let hole = Rect::new(80.0, 100.0, 40.0, 20.0);
        let cut = cut_line_path(&pts, Route::Straight, &[hole]).unwrap();
        assert_eq!(cut, line_path(&pts, Route::Straight));
    }

    #[test]
    fn cut_line_path_hides_the_line_when_the_label_covers_it() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let hole = Rect::new(-10.0, -10.0, 220.0, 20.0);
        assert_eq!(cut_line_path(&pts, Route::Straight, &[hole]), None);
    }

    #[test]
    fn cut_line_path_cuts_a_curved_route_behind_the_label() {
        let pts = vec![v2(0.0, 0.0), v2(100.0, 120.0), v2(200.0, 0.0)];
        let mid = path_midpoint(&pts, Route::Curved);
        let hole = Rect::new(mid.x - 15.0, mid.y - 10.0, 30.0, 20.0);
        let cut = cut_line_path(&pts, Route::Curved, &[hole]).unwrap();
        let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
        let flat = flattened(&cut);
        assert!(
            flat.iter().all(|p| !inner.contains(*p)),
            "a point survives inside the gap"
        );
        assert!(
            flat.iter().any(|p| p.dist(pts[0]) < 1.0) && flat.iter().any(|p| p.dist(pts[2]) < 1.0),
            "the cut line still spans start to end"
        );
    }

    #[test]
    fn cut_line_path_cuts_behind_two_labels() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let holes = [
            Rect::new(40.0, -10.0, 20.0, 20.0),
            Rect::new(120.0, -10.0, 20.0, 20.0),
        ];
        let cut = cut_line_path(&pts, Route::Straight, &holes).unwrap();
        // Three runs: one move per gap side plus the stretch between labels.
        assert_eq!(
            cut.segs
                .iter()
                .filter(|s| matches!(s, vector::Seg::M(_)))
                .count(),
            3,
            "{cut:?}"
        );
        for hole in &holes {
            let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
            for p in flattened(&cut) {
                assert!(!inner.contains(p), "point {p:?} inside the gap");
            }
        }
        let xs: Vec<f64> = flattened(&cut).iter().map(|p| p.x).collect();
        assert!(*xs.first().unwrap() < 1e-9 && *xs.last().unwrap() > 199.0);
    }

    #[test]
    fn head_size_scales_the_arrow_geometry() {
        let extent = |size| {
            let (lines, polys) =
                head_shapes(Head::Arrow, v2(100.0, 0.0), v2(0.0, 0.0), 2.0, 100.0, size);
            lines
                .into_iter()
                .chain(polys)
                .flatten()
                .map(|p| p.dist(v2(100.0, 0.0)))
                .fold(0.0, f64::max)
        };
        let default = extent(DEFAULT_HEAD_SIZE);
        let larger = extent(DEFAULT_HEAD_SIZE * 2.0);
        assert!(default > 20.0, "{default}");
        assert!(larger > default * 1.5, "{default} -> {larger}");
        let smaller = extent(DEFAULT_HEAD_SIZE * 0.5);
        assert!(smaller < default * 0.75, "{default} -> {smaller}");
    }

    #[test]
    fn head_offset_moves_each_tip_away_from_its_shape() {
        let mut pts = vec![v2(10.0, 40.0), v2(110.0, 40.0)];
        offset_endpoints(&mut pts, 8.0, 5.0);
        assert_eq!(pts[0], v2(18.0, 40.0));
        assert_eq!(pts[1], v2(105.0, 40.0));

        let mut pts = vec![v2(10.0, 40.0), v2(110.0, 40.0)];
        offset_endpoints(&mut pts, -8.0, -5.0);
        assert_eq!(pts[0], v2(2.0, 40.0));
        assert_eq!(pts[1], v2(115.0, 40.0));
    }

    // -------------------------------------------------------------------------------- line jumps

    #[test]
    fn jump_half_size_follows_draw_ios_formula() {
        // draw.io: (jumpSize - 2) / 2 + strokeWidth, never negative.
        assert_eq!(jump_half_size(6.0, 2.0), 4.0);
        assert_eq!(jump_half_size(2.0, 0.0), 0.0);
        assert_eq!(jump_half_size(0.0, 2.0), 1.0);
        assert_eq!(jump_half_size(10.0, 1.5), 5.5);
    }

    #[test]
    fn line_jumps_finds_crossings_in_order_along_the_line() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let others = vec![
            vec![v2(50.0, -40.0), v2(50.0, 40.0)],
            vec![v2(120.0, -40.0), v2(120.0, 40.0)],
        ];
        let jumps = line_jumps(&pts, Route::Straight, &others, 4.0);
        assert_eq!(jumps.len(), 2, "{jumps:?}");
        assert!(jumps[0].dist(v2(50.0, 0.0)) < 1e-9, "{jumps:?}");
        assert!(jumps[1].dist(v2(120.0, 0.0)) < 1e-9, "{jumps:?}");
        // The order follows the line, not the input order of the others.
        let flipped = line_jumps(
            &pts,
            Route::Straight,
            &[others[1].clone(), others[0].clone()],
            4.0,
        );
        assert_eq!(flipped, jumps);
    }

    #[test]
    fn line_jumps_skips_lines_that_meet_at_an_endpoint() {
        // A T junction: the vertical starts on the horizontal line.
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let tee = vec![v2(100.0, 0.0), v2(100.0, 80.0)];
        assert!(
            line_jumps(&pts, Route::Straight, &[tee], 4.0).is_empty(),
            "meeting at an endpoint is not a crossing"
        );
        // Same for a line that stops on our end point.
        let into_end = vec![v2(200.0, 80.0), v2(200.0, 0.0)];
        assert!(
            line_jumps(&pts, Route::Straight, &[into_end], 4.0).is_empty(),
            "a line ending on our end does not hop"
        );
    }

    #[test]
    fn line_jumps_collapses_crossings_on_the_same_spot() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let others = vec![
            vec![v2(100.0, -40.0), v2(100.0, 40.0)],
            vec![v2(100.2, -40.0), v2(100.2, 40.0)],
        ];
        let jumps = line_jumps(&pts, Route::Straight, &others, 4.0);
        assert_eq!(jumps.len(), 1, "one hop per spot: {jumps:?}");
    }

    #[test]
    fn line_jumps_needs_others_and_a_positive_half() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let other = vec![v2(100.0, -40.0), v2(100.0, 40.0)];
        assert!(line_jumps(&pts, Route::Straight, &[], 4.0).is_empty());
        assert!(line_jumps(&pts, Route::Straight, &[other], 0.0).is_empty());
    }

    #[test]
    fn jumped_line_path_none_style_and_no_jumps_keep_the_plain_line() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let plain = line_path(&pts, Route::Straight);
        assert_eq!(
            jumped_line_path(
                &pts,
                Route::Straight,
                &[v2(100.0, 0.0)],
                LineJump::None,
                4.0,
                &[]
            ),
            None,
            "None keeps the smooth original"
        );
        assert_eq!(
            jumped_line_path(&pts, Route::Straight, &[], LineJump::Arc, 4.0, &[]),
            None
        );
        assert_eq!(
            jumped_line_path(
                &pts,
                Route::Straight,
                &[v2(100.0, 0.0)],
                LineJump::Arc,
                0.0,
                &[]
            ),
            None
        );
        assert!(
            jumped_line_path(
                &pts,
                Route::Straight,
                &[v2(100.0, 0.0)],
                LineJump::Gap,
                4.0,
                &[]
            )
            .is_some()
        );
        drop(plain);
    }

    #[test]
    fn jumped_line_path_arc_bumps_over_the_crossing() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let p = jumped_line_path(
            &pts,
            Route::Straight,
            &[v2(100.0, 0.0)],
            LineJump::Arc,
            4.0,
            &[],
        )
        .unwrap();
        // draw.io paints the arc as a cubic whose controls sit 1.3x the half
        // gap off the line, on the side the line travels toward.
        assert_eq!(
            p.segs[0],
            crate::vector::Seg::M(v2(0.0, 0.0)),
            "{:?}",
            p.segs
        );
        assert_eq!(p.segs[1], crate::vector::Seg::L(v2(96.0, 0.0)));
        let crate::vector::Seg::C(c1, c2, end) = p.segs[2] else {
            panic!("the hop is a cubic: {:?}", p.segs)
        };
        assert_eq!(end, v2(104.0, 0.0), "the arc lands half a gap past");
        assert_eq!(c1, v2(96.0, -5.2), "control 1.3 * half above");
        assert_eq!(c2, v2(104.0, -5.2));
        // The bump leaves the line: some flattened point is off y = 0.
        assert!(
            flattened(&p).iter().any(|q| q.y < -1.0),
            "the arc rises over the crossing"
        );
    }

    #[test]
    fn jumped_line_path_gap_interrupts_the_line() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let p = jumped_line_path(
            &pts,
            Route::Straight,
            &[v2(100.0, 0.0)],
            LineJump::Gap,
            4.0,
            &[],
        )
        .unwrap();
        // Two runs around the gap.
        assert_eq!(
            p.segs
                .iter()
                .filter(|s| matches!(s, crate::vector::Seg::M(_)))
                .count(),
            2,
            "{p:?}"
        );
        let cross = v2(100.0, 0.0);
        for q in flattened(&p) {
            assert!(q.dist(cross) >= 4.0 - 0.6, "point {q:?} inside the gap");
        }
        assert!(flattened(&p).first().unwrap().x < 1e-9);
        assert!(flattened(&p).last().unwrap().x > 199.0);
    }

    #[test]
    fn jumped_line_path_sharp_steps_sideways() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let p = jumped_line_path(
            &pts,
            Route::Straight,
            &[v2(100.0, 0.0)],
            LineJump::Sharp,
            4.0,
            &[],
        )
        .unwrap();
        // (96,0) -> (96,-4) -> (104,-4) -> (104,0), as draw.io's sharp step.
        assert_eq!(
            p.segs,
            vec![
                crate::vector::Seg::M(v2(0.0, 0.0)),
                crate::vector::Seg::L(v2(96.0, 0.0)),
                crate::vector::Seg::L(v2(96.0, -4.0)),
                crate::vector::Seg::L(v2(104.0, -4.0)),
                crate::vector::Seg::L(v2(104.0, 0.0)),
                crate::vector::Seg::L(v2(200.0, 0.0)),
            ],
            "{p:?}"
        );
    }

    #[test]
    fn jumped_line_path_line_draws_ticks_on_both_sides() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let p = jumped_line_path(
            &pts,
            Route::Straight,
            &[v2(100.0, 0.0)],
            LineJump::Line,
            4.0,
            &[],
        )
        .unwrap();
        // The line runs to (96,0), then ticks across it at both ends of the
        // gap, as draw.io's line style.
        assert_eq!(
            p.segs,
            vec![
                crate::vector::Seg::M(v2(0.0, 0.0)),
                crate::vector::Seg::L(v2(96.0, 0.0)),
                crate::vector::Seg::M(v2(96.0, -4.0)),
                crate::vector::Seg::L(v2(96.0, 4.0)),
                crate::vector::Seg::M(v2(104.0, 4.0)),
                crate::vector::Seg::L(v2(104.0, -4.0)),
                crate::vector::Seg::M(v2(104.0, 0.0)),
                crate::vector::Seg::L(v2(200.0, 0.0)),
            ],
            "{p:?}"
        );
    }

    #[test]
    fn jumped_line_path_drops_hops_that_run_past_an_end() {
        let pts = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        for at in [v2(2.0, 0.0), v2(198.0, 0.0)] {
            assert_eq!(
                jumped_line_path(&pts, Route::Straight, &[at], LineJump::Arc, 4.0, &[]),
                None,
                "a hop at {at:?} would run off the line"
            );
        }
    }

    #[test]
    fn fixed_points_are_the_four_corners_and_edge_midpoints() {
        let wr = Rect::new(10.0, 20.0, 100.0, 60.0);
        assert_eq!(
            fixed_points(wr),
            [
                v2(10.0, 20.0),
                v2(60.0, 20.0),
                v2(110.0, 20.0),
                v2(110.0, 50.0),
                v2(110.0, 80.0),
                v2(60.0, 80.0),
                v2(10.0, 80.0),
                v2(10.0, 50.0),
            ]
        );
    }

    #[test]
    fn snap_fixed_uses_the_nearest_point_only_inside_the_tolerance() {
        let wr = Rect::new(0.0, 0.0, 100.0, 60.0);
        assert_eq!(snap_fixed(v2(97.0, 3.0), wr, 8.0), v2(100.0, 0.0));
        assert_eq!(snap_fixed(v2(49.0, 61.0), wr, 4.0), v2(50.0, 60.0));
        assert_eq!(snap_fixed(v2(30.0, 30.0), wr, 4.0), v2(30.0, 30.0));
    }

    #[test]
    fn jumped_line_path_keeps_the_label_gap_around_the_hops() {
        let pts = vec![v2(0.0, 0.0), v2(300.0, 0.0)];
        let hole = Rect::new(140.0, -10.0, 40.0, 20.0);
        let p = jumped_line_path(
            &pts,
            Route::Straight,
            &[v2(60.0, 0.0), v2(260.0, 0.0)],
            LineJump::Gap,
            4.0,
            &[hole],
        )
        .unwrap();
        // Four runs: before the first hop, between the hops with the label
        // gap cut out of it, and after the second hop.
        assert_eq!(
            p.segs
                .iter()
                .filter(|s| matches!(s, crate::vector::Seg::M(_)))
                .count(),
            4,
            "{p:?}"
        );
        let inner = Rect::new(hole.x + 0.5, hole.y + 0.5, hole.w - 1.0, hole.h - 1.0);
        for q in flattened(&p) {
            assert!(!inner.contains(q), "point {q:?} inside the label gap");
            assert!(
                q.dist(v2(60.0, 0.0)) >= 3.4 && q.dist(v2(260.0, 0.0)) >= 3.4,
                "point {q:?} inside a hop gap"
            );
        }
        // The stretch between the label and the second hop is not lost.
        assert!(
            p.segs.contains(&crate::vector::Seg::L(v2(256.0, 0.0))),
            "the line continues after the label gap: {p:?}"
        );
    }
}
