//! Document + camera → world-space cached meshes, plus the screen-space overlay.

use super::draw;
use super::mesh::{Batch, MeshBuilder};
use super::tess::{Piece, Tess};
use crate::editor::{Editor, RULER_PX, hit, snap::Guide};
use crate::geom::{Camera, Rect, V2, v2};
use crate::model::{BlendMode, ColorMode, Node};
use crate::model::{Color, Document, GuideAxis, NodeId, NodeKind, TextAlign};
use crate::text;
use lyon::tessellation::{FillRule, LineCap};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

pub const CANVAS_BG_LIGHT: Color = Color::rgb(0xee, 0xee, 0xf0);
pub const CANVAS_BG_DARK: Color = draw::DARK_CANVAS;
pub const GRID_DOT_LIGHT: Color = Color::rgb(0xcf, 0xcf, 0xd6);
pub const GRID_DOT_DARK: Color = Color::rgb(0x3a, 0x3a, 0x3c);
/// Draw.io-style grid: square lines every 10 world units, with a stronger line every 5.
pub const GRID_STEP: f64 = 10.0;
pub const ACCENT: Color = Color::rgb(0x2f, 0x6f, 0xf3);
pub const GUIDE: Color = Color::rgb(0xf2, 0x2f, 0x7a);

pub fn canvas_bg(dark: bool) -> Color {
    if dark {
        CANVAS_BG_DARK
    } else {
        CANVAS_BG_LIGHT
    }
}

pub fn grid_dot(dark: bool) -> Color {
    if dark { GRID_DOT_DARK } else { GRID_DOT_LIGHT }
}

pub fn rgba(c: Color) -> [f32; 4] {
    c.to_f32(1.0)
}

/// What to draw and where.
pub struct View<'a> {
    pub doc: &'a Document,
    pub camera: Camera,
    /// Canvas size in logical px.
    pub viewport: V2,
    /// Canvas top-left in the render target, logical px.
    pub offset: V2,
}

impl View<'_> {
    fn to_screen(&self, p: V2) -> V2 {
        self.camera.world_to_screen(p) + self.offset
    }
    fn rect_to_screen(&self, r: &Rect) -> Rect {
        self.camera.world_rect_to_screen(r).translate(self.offset)
    }
    fn canvas_rect(&self) -> Rect {
        Rect::new(
            self.offset.x,
            self.offset.y,
            self.viewport.x,
            self.viewport.y,
        )
    }
}

/// Square grid lines, like draw.io. Spacing adapts to the zoom so lines never get too dense.
pub fn grid(mb: &mut MeshBuilder, v: &View, step_world: f64, dark: bool, custom: Option<Color>) {
    let z = v.camera.zoom;
    let step_world = if step_world > 0.0 {
        step_world
    } else {
        GRID_STEP
    };
    let mut spacing = step_world;
    while spacing * z < 8.0 {
        spacing *= 5.0;
    }
    let vis = v.camera.visible_world(v.viewport);
    let x0 = (vis.x / spacing).floor() as i64;
    let y0 = (vis.y / spacing).floor() as i64;
    let x1 = (vis.right() / spacing).ceil() as i64;
    let y1 = (vis.bottom() / spacing).ceil() as i64;
    let base = custom.unwrap_or_else(|| grid_dot(dark));
    let minor = rgba(Color::rgba(base.r, base.g, base.b, 150));
    let major = rgba(base);
    let (w, h) = (v.viewport.x, v.viewport.y);
    for xi in x0..=x1 {
        let s = v.to_screen(v2(xi as f64 * spacing, 0.0));
        let c = if xi % 5 == 0 { major } else { minor };
        mb.rect(Rect::new(s.x.round(), v.offset.y, 1.0, h), c);
    }
    for yi in y0..=y1 {
        let s = v.to_screen(v2(0.0, yi as f64 * spacing));
        let c = if yi % 5 == 0 { major } else { minor };
        mb.rect(Rect::new(v.offset.x, s.y.round(), w, 1.0), c);
    }
}

// ---------------------------------------------------------------------------------- world scene

/// One node's cached triangles plus where they are clipped and how they blend.
#[derive(Clone)]
pub struct Item {
    pub id: NodeId,
    pub pieces: Arc<Vec<Piece>>,
    /// World-space clip rectangle (from clipping frames).
    pub clip: Option<Rect>,
    pub blend: BlendMode,
}

struct Entry {
    node: Node,
    wr: Rect,
    opacity: f64,
    bucket: i32,
    pieces: Arc<Vec<Piece>>,
    /// Hash of the connector's line-jump crossings, which depend on the other
    /// connectors below it and so cannot come from the node alone.
    jump_sig: u64,
    used: bool,
}

fn jump_sig(pts: &[V2]) -> u64 {
    use std::hash::{Hash, Hasher};
    if pts.is_empty() {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    pts.len().hash(&mut h);
    for p in pts {
        p.x.to_bits().hash(&mut h);
        p.y.to_bits().hash(&mut h);
    }
    h.finish()
}

/// Per-node tessellation cache. A node is re-tessellated only when it (or its world position,
/// inherited opacity or the zoom bucket) changes, so panning and idle frames do no work.
#[derive(Default)]
pub struct SceneCache {
    entries: HashMap<NodeId, Entry>,
    /// Item list of the previous frame and what it was built for (doc stamp, zoom bucket,
    /// culled world rect): reused while the view stays inside that rect.
    last: Option<(u64, i32, Rect, bool, Vec<Item>)>,
    /// The font-library epoch the cached pieces were rendered with. System
    /// families resolve only after the background scan lands, so pieces built
    /// earlier show the fallback font and must be rebuilt once it does.
    font_epoch: u64,
    /// Theme the cached pieces were built for. Dark connectors use lighter colors, so a
    /// theme change rebuilds them just like a font change does.
    dark: Option<bool>,
    /// Adaptive-color appearance the cached pieces were built for. A mode change
    /// rebuilds every node even when the light/dark flag stays the same.
    appearance: Option<ColorMode>,
    /// Tessellations performed during the last `items` call (for tests and profiling).
    pub last_misses: usize,
}

/// Zoom buckets are powers of two: geometry built for 2^b is accurate up to 2^(b+1).
pub fn zoom_bucket(zoom: f64) -> i32 {
    zoom.max(1e-6).log2().floor() as i32
}

impl SceneCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.last = None;
    }

    /// Items for `roots` (or the whole page) in paint order. `cull` skips subtrees outside a
    /// world rect.
    pub fn items(
        &mut self,
        doc: &Document,
        roots: Option<&[NodeId]>,
        zoom: f64,
        cull: Option<Rect>,
        dark: bool,
        now_ms: f64,
    ) -> Vec<Item> {
        let bucket = zoom_bucket(zoom);
        let tz = 2f64.powi(bucket);
        let mut tess = Tess::new(tz);
        let mut out = Vec::new();
        self.last_misses = 0;
        // Dark connectors need a different display color; rebuild the pieces on a theme change.
        if self.dark != Some(dark) {
            self.dark = Some(dark);
            self.entries.clear();
            self.last = None;
        }
        // The font scan finishing changes how every text piece renders.
        let epoch = crate::text::library_epoch();
        if epoch != self.font_epoch {
            self.font_epoch = epoch;
            self.entries.clear();
            self.last = None;
        }
        for e in self.entries.values_mut() {
            e.used = false;
        }
        let all = doc.layered_root();
        let mut jw = draw::JumpWalk::new(doc);
        for id in roots.unwrap_or(&all) {
            // A hidden layer hides its whole subtree in paint order.
            if !doc.layer_of(*id).visible {
                continue;
            }
            let op = draw::inherited_opacity(doc, *id);
            let origin = doc.parent_origin(doc.get(*id).and_then(|n| n.parent));
            self.walk(
                doc, *id, origin, None, op, bucket, &mut tess, cull, &mut jw, &mut out, dark,
                now_ms,
            );
        }
        // Evict entries for deleted or long-unseen nodes when the cache grows large.
        let used = self.entries.values().filter(|e| e.used).count();
        if self.entries.len() > used * 4 + 2000 {
            self.entries.retain(|_, e| e.used);
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        doc: &Document,
        id: NodeId,
        origin: V2,
        clip: Option<Rect>,
        parent_opacity: f64,
        bucket: i32,
        tess: &mut Tess,
        cull: Option<Rect>,
        jw: &mut draw::JumpWalk,
        out: &mut Vec<Item>,
        dark: bool,
        now_ms: f64,
    ) {
        let Some(n) = doc.get(id) else { return };
        if !n.visible {
            return;
        }
        // Placeholders resolve for drawing only; the document keeps the raw
        // label for editing and saving.
        let n = crate::model::placeholders::resolve_node(n, doc);
        let n = &*n;
        let wr = n.rect.translate(origin);
        let opacity = parent_opacity * n.style.opacity;
        let bounds = draw::draw_bounds_with_shadow(n, wr, doc.settings.shadow);
        let visible = cull.is_none_or(|c| c.intersects(&bounds))
            && clip.is_none_or(|c| c.intersects(&bounds));
        if visible && !n.is_group() {
            let jump_pts = jw.points_for(n, wr);
            let sig = jump_sig(&jump_pts);
            // A flowing connector changes every frame, so its mesh must be
            // rebuilt rather than served from the per-node cache.
            let flowing = n.is_connector() && n.flow.enabled;
            let hit = !flowing
                && self.entries.get(&id).is_some_and(|e| {
                    e.bucket == bucket
                        && e.wr == wr
                        && e.opacity == opacity
                        && e.node == *n
                        && e.jump_sig == sig
                });
            if hit {
                let e = self.entries.get_mut(&id).unwrap();
                e.used = true;
                if !e.pieces.is_empty() {
                    out.push(Item {
                        id,
                        pieces: e.pieces.clone(),
                        clip,
                        blend: n.style.blend,
                    });
                }
            } else {
                self.last_misses += 1;
                let d = draw::node_draw_with(
                    n,
                    wr,
                    opacity,
                    doc.settings.shadow,
                    doc.settings.sketch,
                    doc.settings.auto_fill(),
                    &jump_pts,
                    dark,
                    now_ms,
                );
                let pieces = Arc::new(tess.ops(&d.ops));
                if flowing {
                    if !pieces.is_empty() {
                        out.push(Item {
                            id,
                            pieces,
                            clip,
                            blend: n.style.blend,
                        });
                    }
                } else {
                    self.entries.insert(
                        id,
                        Entry {
                            node: n.clone(),
                            wr,
                            opacity,
                            bucket,
                            pieces: pieces.clone(),
                            jump_sig: sig,
                            used: true,
                        },
                    );
                    if !pieces.is_empty() {
                        out.push(Item {
                            id,
                            pieces,
                            clip,
                            blend: n.style.blend,
                        });
                    }
                }
            }
        }
        // Later connectors hop over this one, so remember its route even when
        // the tessellation came from the cache.
        if visible {
            jw.push(n, wr);
        }
        if n.children.is_empty() {
            return;
        }
        let child_clip = match n.kind {
            NodeKind::Frame { clip: true } => {
                match clip.map_or(Some(wr), |c| c.intersection(&wr)) {
                    Some(c) => Some(c),
                    None => return,
                }
            }
            _ => clip,
        };
        if matches!(n.kind, NodeKind::Frame { clip: true })
            && cull.is_some_and(|c| !c.intersects(&wr))
        {
            return;
        }
        let o = wr.min();
        for c in &n.children {
            self.walk(
                doc, *c, o, child_clip, opacity, bucket, tess, cull, jw, out, dark, now_ms,
            );
        }
    }
}

/// Everything the GPU draws for the canvas: screen-space underlay (background, grid), the
/// world-space document, and the screen-space overlay (selection, guides, labels).
pub struct CanvasFrame {
    pub under: Vec<Batch>,
    pub items: Vec<Item>,
    pub over: Vec<Batch>,
    pub camera: Camera,
    /// Canvas top-left in the render target, logical px.
    pub offset: V2,
    /// Canvas rect in logical px (world items are scissored to it).
    pub viewport: Rect,
}

/// Draw text with its box's top-left at `origin` (screen px) at screen scale `z`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    mb: &mut MeshBuilder,
    t: &str,
    font_size: f64,
    wrap: Option<f64>,
    align: TextAlign,
    origin: V2,
    z: f64,
    color: [f32; 4],
) {
    let layout = text::layout(t, font_size, wrap, align);
    let px = font_size * z;
    if px < 2.5 {
        // Too small to read: draw "greeked" bars instead of glyphs.
        for l in &layout.lines {
            let lh = l.height * z;
            if l.width > 0.0 {
                let x = origin.x + l.carets[0] * z;
                mb.rect(
                    Rect::new(x, origin.y + l.top * z + lh * 0.3, l.width * z, lh * 0.4),
                    [color[0], color[1], color[2], color[3] * 0.35],
                );
            }
        }
        return;
    }
    for g in &layout.glyphs {
        let s = (g.size / text::face_of(g.face).units_per_em() as f64 * z) as f32;
        let m = text::glyph_mesh(g.face, g.gid);
        if m.indices.is_empty() {
            continue;
        }
        let o = origin + v2(g.x, g.baseline) * z;
        let (ox, oy) = (o.x as f32, o.y as f32);
        mb.triangles(
            m.vertices.iter().map(|v| [ox + v[0] * s, oy - v[1] * s]),
            &m.indices,
            color,
        );
    }
}

/// Interactive overlay: hover, selection, handles, marquee, guides, labels, caret.
pub fn overlay(mb: &mut MeshBuilder, ed: &Editor, offset: V2, dark: bool) {
    let v = View {
        doc: &ed.doc,
        camera: ed.camera,
        viewport: ed.viewport,
        offset,
    };
    let accent = rgba(ACCENT);
    let z = ed.camera.zoom;
    mb.set_clip(Some(v.canvas_rect()));

    // Frame name labels.
    for (id, r) in ed.frame_labels() {
        let n = ed.doc.get(id).unwrap();
        let selected = ed.selection.contains(&id);
        let color = if selected {
            accent
        } else {
            [0.42, 0.42, 0.46, 1.0]
        };
        if ed.text_edit.as_ref().is_some_and(|te| te.id == id)
            && let Some((_, o)) = ed.frame_title(id)
        {
            // Keep the whole title visible while it is being edited, so the caret
            // and the selection line up with the text.
            draw_text(
                mb,
                &n.name,
                text::FRAME_TITLE_SIZE,
                None,
                TextAlign::Left,
                o + offset,
                1.0,
                color,
            );
            continue;
        }
        let r = r.translate(offset);
        let mut name = n.name.clone();
        let max_w = r.w;
        while !name.is_empty()
            && text::layout(&name, text::FRAME_TITLE_SIZE, None, TextAlign::Left).width > max_w
        {
            name.pop();
        }
        draw_text(
            mb,
            &name,
            text::FRAME_TITLE_SIZE,
            None,
            TextAlign::Left,
            v2(r.x, r.y + 2.0),
            1.0,
            color,
        );
    }

    if let Some(h) = ed.hover.filter(|h| !ed.selection.contains(h))
        && let Some(wr) = ed.doc.world_rect(h)
    {
        outline_node(mb, &v, h, v.rect_to_screen(&wr), accent, 1.5);
        // draw.io-style: the hovered unselected shape also grows resize handles
        // (no edge handles on tiny shapes), matching the press routing in
        // `select_down`.
        let resizes = ed
            .doc
            .get(h)
            .is_some_and(|n| !matches!(n.kind, NodeKind::Connector { .. }));
        if resizes && !ed.doc.is_effectively_locked(h) {
            let sb = v.rect_to_screen(&wr);
            let tiny = sb.w.min(sb.h) < 24.0;
            for (hx, hy) in hit::HANDLES {
                if tiny && (hx == 0.5 || hy == 0.5) {
                    continue;
                }
                let c = v2(sb.x + sb.w * hx, sb.y + sb.h * hy);
                let r = Rect::new(c.x - 4.0, c.y - 4.0, 8.0, 8.0);
                mb.rect(r, [1.0, 1.0, 1.0, 1.0]);
                mb.rect_outline(r, 1.0, accent);
            }
        }
    }

    for id in &ed.selection {
        if let Some(wr) = ed.doc.world_rect(*id) {
            outline_node(mb, &v, *id, v.rect_to_screen(&wr), accent, 1.0);
        }
    }

    if let Some(te) = &ed.text_edit {
        // Frame titles are drawn in screen space at a fixed size; text nodes scale
        // with the canvas. Place the caret in whichever space matches the text.
        let target = ed.doc.get(te.id).and_then(|n| {
            if n.is_frame() {
                ed.frame_title(te.id).map(|(l, o)| (l, o + offset, 1.0))
            } else {
                let l = text::layout_node_label(n, te.label)?;
                let wr = ed.doc.world_rect(te.id)?;
                let o = v.to_screen(wr.min() + text::text_origin_label(n, &l, te.label));
                Some((l, o, z))
            }
        });
        if let Some((l, o, z)) = target {
            if let Some(a) = te.anchor.filter(|a| *a != te.caret) {
                for (x, top, w, h) in l.selection_rects(a, te.caret) {
                    mb.rect(
                        Rect::new(o.x + x * z, o.y + top * z, w * z, h * z),
                        [accent[0], accent[1], accent[2], 0.25],
                    );
                }
            }
            let (x, top, h) = l.caret(te.caret);
            mb.rect(
                Rect::new(o.x + x * z - 0.75, o.y + top * z, 1.5, h * z),
                accent,
            );
        }
    } else if let Some(b) = ed.selection_bounds() {
        let sb = v.rect_to_screen(&b);
        if ed.selection.len() > 1 {
            mb.rect_outline(sb, 1.0, accent);
        }
        let tiny = sb.w.min(sb.h) < 24.0;
        for (hx, hy) in hit::HANDLES {
            if tiny && (hx == 0.5 || hy == 0.5) {
                continue;
            }
            let c = v2(sb.x + sb.w * hx, sb.y + sb.h * hy);
            let r = Rect::new(c.x - 4.0, c.y - 4.0, 8.0, 8.0);
            mb.rect(r, [1.0, 1.0, 1.0, 1.0]);
            mb.rect_outline(r, 1.0, accent);
        }
        if ed.rotation_target().is_some() {
            // draw.io's rotate grab handle: a circle on a short stem above the
            // selection's top-right corner, clear of the edge connection knobs.
            let hp = hit::rotate_handle_pos(&sb);
            let corner = v2(sb.right(), sb.y);
            mb.line(corner, hp, 1.0, accent);
            let hr = Rect::new(hp.x - 5.0, hp.y - 5.0, 10.0, 10.0);
            mb.circle(hp, 5.0, [1.0, 1.0, 1.0, 1.0]);
            mb.stroke_path(&MeshBuilder::ellipse_path(hr), 1.5, accent, LineCap::Round);
        }
        // Size badge below the selection.
        let label = format!("{} × {}", fmt_num(b.w), fmt_num(b.h));
        let w = text::layout(&label, 11.0, None, TextAlign::Left).width;
        let pill = Rect::new(
            sb.center().x - w * 0.5 - 6.0,
            sb.bottom() + 8.0,
            w + 12.0,
            18.0,
        );
        mb.fill_path(
            &MeshBuilder::rounded_rect_path(pill, 4.0),
            accent,
            FillRule::NonZero,
        );
        draw_text(
            mb,
            &label,
            11.0,
            None,
            TextAlign::Left,
            v2(pill.x + 6.0, pill.y + 2.0),
            1.0,
            [1.0; 4],
        );
    }

    if let Some(m) = ed.marquee {
        let sm = v.rect_to_screen(&m);
        mb.rect(sm, [accent[0], accent[1], accent[2], 0.08]);
        mb.rect_outline(sm, 1.0, accent);
    }

    let guide = rgba(GUIDE);
    if ed.doc.settings.guides {
        for g in ed.guides.iter().chain(&ed.measures) {
            match *g {
                Guide::V { x, y0, y1 } => {
                    mb.line(v.to_screen(v2(x, y0)), v.to_screen(v2(x, y1)), 1.0, guide)
                }
                Guide::H { y, x0, x1 } => {
                    mb.line(v.to_screen(v2(x0, y)), v.to_screen(v2(x1, y)), 1.0, guide)
                }
                Guide::Gap { a, b } => gap_guide(mb, &v, a, b, guide),
            }
        }
    }
    manual_guides(mb, &v, ed);
    vector_edit_overlay(mb, ed, &v, accent);
    whiteboard_overlay(mb, ed, &v, accent);
    if ed.doc.settings.rulers {
        rulers(mb, &v, dark);
    }
    mb.set_clip(None);
}

/// Manual guides as thin blue lines across the canvas, under the ruler strips.
fn manual_guides(mb: &mut MeshBuilder, v: &View, ed: &Editor) {
    if !(ed.doc.settings.guides || ed.doc.settings.rulers) {
        return;
    }
    let c = v.canvas_rect();
    let (top, left) = if ed.doc.settings.rulers {
        (c.y + RULER_PX, c.x + RULER_PX)
    } else {
        (c.y, c.x)
    };
    let color = [0.18, 0.44, 0.95, 0.65];
    for g in &ed.doc.settings.manual_guides {
        match g.axis {
            GuideAxis::Vertical => {
                let x = v.to_screen(v2(g.pos, 0.0)).x;
                if x >= c.x && x <= c.right() {
                    mb.rect(
                        Rect::new(x.round() - 0.5, top, 1.0, c.bottom() - top),
                        color,
                    );
                }
            }
            GuideAxis::Horizontal => {
                let y = v.to_screen(v2(0.0, g.pos)).y;
                if y >= c.y && y <= c.bottom() {
                    mb.rect(
                        Rect::new(left, y.round() - 0.5, c.right() - left, 1.0),
                        color,
                    );
                }
            }
        }
    }
}

/// Ruler strips along the canvas top and left edges. The labeled interval
/// follows zoom in a 1-2-5 sequence so labels stay readable at any scale.
fn rulers(mb: &mut MeshBuilder, v: &View, dark: bool) {
    let c = v.canvas_rect();
    if c.w <= RULER_PX || c.h <= RULER_PX {
        return;
    }
    let bg = if dark {
        Color::rgb(0x2a, 0x2a, 0x2e)
    } else {
        Color::rgb(0xf7, 0xf7, 0xf9)
    };
    let corner = if dark {
        Color::rgb(0x33, 0x33, 0x38)
    } else {
        Color::rgb(0xeb, 0xeb, 0xf0)
    };
    let edge = if dark {
        Color::rgb(0x45, 0x45, 0x4b)
    } else {
        Color::rgb(0xc8, 0xc8, 0xd0)
    };
    let tick = if dark {
        Color::rgb(0x9a, 0x9a, 0xa4)
    } else {
        Color::rgb(0x62, 0x62, 0x6e)
    };
    mb.rect(Rect::new(c.x, c.y, c.w, RULER_PX), rgba(bg));
    mb.rect(Rect::new(c.x, c.y, RULER_PX, c.h), rgba(bg));
    mb.rect(Rect::new(c.x, c.y, RULER_PX, RULER_PX), rgba(corner));
    mb.rect(Rect::new(c.x, c.y + RULER_PX - 1.0, c.w, 1.0), rgba(edge));
    mb.rect(Rect::new(c.x + RULER_PX - 1.0, c.y, 1.0, c.h), rgba(edge));

    let step = ruler_spacing(v.camera.zoom);
    let minor = step / 5.0;
    let vis = v.camera.visible_world(v.viewport);
    let tick_color = rgba(tick);
    let x0 = (vis.x / minor).floor() as i64;
    let x1 = (vis.right() / minor).ceil() as i64;
    let y0 = (vis.y / minor).floor() as i64;
    let y1 = (vis.bottom() / minor).ceil() as i64;

    for i in x0..=x1 {
        let sx = v.to_screen(v2(i as f64 * minor, 0.0)).x;
        if sx < c.x + RULER_PX || sx > c.right() {
            continue;
        }
        let len = if i % 5 == 0 { 8.0 } else { 4.0 };
        mb.rect(
            Rect::new(sx.round(), c.y + RULER_PX - len, 1.0, len - 1.0),
            tick_color,
        );
    }
    for i in y0..=y1 {
        let sy = v.to_screen(v2(0.0, i as f64 * minor)).y;
        if sy < c.y + RULER_PX || sy > c.bottom() {
            continue;
        }
        let len = if i % 5 == 0 { 8.0 } else { 4.0 };
        mb.rect(
            Rect::new(c.x + RULER_PX - len, sy.round(), len - 1.0, 1.0),
            tick_color,
        );
    }

    mb.set_clip(Some(Rect::new(
        c.x + RULER_PX,
        c.y,
        c.w - RULER_PX,
        RULER_PX,
    )));
    for i in x0..=x1 {
        if i % 5 != 0 {
            continue;
        }
        let world = i as f64 * minor;
        let sx = v.to_screen(v2(world, 0.0)).x;
        if sx < c.x + RULER_PX + 2.0 || sx > c.right() {
            continue;
        }
        draw_text(
            mb,
            &fmt_num(world),
            9.0,
            None,
            TextAlign::Left,
            v2(sx + 3.0, c.y + 1.0),
            1.0,
            rgba(tick),
        );
    }
    mb.set_clip(Some(Rect::new(
        c.x,
        c.y + RULER_PX,
        RULER_PX,
        c.h - RULER_PX,
    )));
    for i in y0..=y1 {
        if i % 5 != 0 {
            continue;
        }
        let world = i as f64 * minor;
        let sy = v.to_screen(v2(0.0, world)).y;
        if sy < c.y + RULER_PX + 1.0 || sy > c.bottom() {
            continue;
        }
        draw_text(
            mb,
            &fmt_num(world),
            9.0,
            None,
            TextAlign::Left,
            v2(c.x + 1.0, sy + 1.0),
            1.0,
            rgba(tick),
        );
    }
    mb.set_clip(Some(c));
}

/// Labeled ruler interval: the first 1, 2, or 5 times a power of ten that is at
/// least 50 screen pixels wide.
fn ruler_spacing(zoom: f64) -> f64 {
    let target = 50.0 / zoom.max(1e-6);
    let mut base = 0.1;
    loop {
        for m in [1.0, 2.0, 5.0] {
            let s = base * m;
            if s >= target {
                return s;
            }
        }
        base *= 10.0;
        if base > 1e9 {
            return base;
        }
    }
}

/// Screen triangle for a quick connection handle. The tip points away from the
/// shape so a press and drag starts an arrow on that side, as in draw.io.
fn arrow_handle(pos: V2, dir: V2) -> [V2; 3] {
    let perp = v2(-dir.y, dir.x);
    [
        pos + dir * 8.0,
        pos - dir * 2.0 + perp * 4.0,
        pos - dir * 2.0 - perp * 4.0,
    ]
}

/// Arrow handles, bind targets, quick-connect knobs and the laser trail.
fn whiteboard_overlay(mb: &mut MeshBuilder, ed: &Editor, v: &View, accent: [f32; 4]) {
    let off = v.offset;
    if let Some(t) = ed.bind_hover
        && let Some(wr) = ed.doc.world_rect(t)
    {
        let n = ed.doc.get(t).unwrap();
        let path = draw::shape_path(n, wr.inflate(4.0 / v.camera.zoom))
            .unwrap_or_else(|| crate::vector::rounded_rect(wr, [0.0; 4]));
        let sp = path.map(|p| v.to_screen(p));
        mb.stroke_path(
            &sp.to_lyon(),
            3.0,
            [accent[0], accent[1], accent[2], 0.45],
            LineCap::Round,
        );
    }
    // Fixed attachment points as small crosses: on the shape a connector end is
    // being dragged over, and on the hovered or selected shape while idle.
    if ed.doc.settings.connection_points {
        let target = ed.bind_hover.or_else(|| {
            if ed.is_busy() {
                None
            } else {
                ed.hover.or_else(|| ed.selection.first().copied())
            }
        });
        if let Some(t) = target
            && let Some(wr) = ed.doc.world_rect(t)
            && ed.doc.get(t).is_some_and(crate::connector::bindable)
            && !ed.doc.is_effectively_locked(t)
            && ed.doc.is_effectively_visible(t)
        {
            let color = [accent[0], accent[1], accent[2], 0.9];
            for p in crate::connector::fixed_points(wr) {
                let p = v.to_screen(p);
                let arm = 4.0;
                mb.line(p - v2(arm, arm), p + v2(arm, arm), 1.5, color);
                mb.line(p - v2(arm, -arm), p + v2(arm, -arm), 1.5, color);
            }
        }
    }
    for (end, p) in ed.connector_handles() {
        let p = p + off;
        if end == 2 {
            mb.circle(p, 4.0, [accent[0], accent[1], accent[2], 0.5]);
        } else {
            mb.circle(p, 5.5, accent);
            mb.circle(p, 4.0, [1.0; 4]);
        }
    }
    let mut knob = |dir: usize, p: V2| {
        let p = p + off;
        let d = crate::editor::whiteboard::QUICK_DIRS[dir];
        let pts = arrow_handle(p, d);
        let path = MeshBuilder::polygon_path(&pts, true);
        mb.fill_path(
            &path,
            [accent[0], accent[1], accent[2], 0.95],
            FillRule::NonZero,
        );
        mb.stroke_path(&path, 1.0, [1.0, 1.0, 1.0, 0.9], LineCap::Round);
    };
    if ed.doc.settings.connection_arrows {
        for (dir, p) in ed.quick_knobs() {
            knob(dir, p);
        }
        // Knobs on the hovered unselected shape: press and drag to connect an arrow.
        if let Some((_, knobs)) = ed.hover_knobs() {
            for (dir, p) in knobs {
                knob(dir, p);
            }
        }
    }
    let trail = ed.laser_trail(ed.now_ms);
    if trail.len() >= 2 {
        let red = [0.98, 0.18, 0.2, 1.0];
        for w in trail.windows(2) {
            let (a, ka) = w[0];
            let (b, kb) = w[1];
            let width = 4.0 * (ka + kb) * 0.5;
            mb.line(v.to_screen(a), v.to_screen(b), width, red);
            mb.circle(v.to_screen(b), width * 0.5, red);
        }
        let (head, _) = *trail.last().unwrap();
        mb.circle(v.to_screen(head), 3.5, [1.0, 0.6, 0.6, 1.0]);
    } else if let Some((p, _)) = trail.first() {
        mb.circle(v.to_screen(*p), 3.0, [0.98, 0.18, 0.2, 1.0]);
    }
}

/// A measured distance: line with end ticks and a pill showing the length.
fn gap_guide(mb: &mut MeshBuilder, v: &View, a: V2, b: V2, color: [f32; 4]) {
    let (sa, sb) = (v.to_screen(a), v.to_screen(b));
    mb.line(sa, sb, 1.0, color);
    let n = (sb - sa).normalized().perp() * 4.0;
    mb.line(sa - n, sa + n, 1.0, color);
    mb.line(sb - n, sb + n, 1.0, color);
    let label = fmt_num(a.dist(b));
    let w = text::layout(&label, 11.0, None, TextAlign::Left).width;
    let c = (sa + sb) * 0.5;
    let horizontal = (sb.x - sa.x).abs() >= (sb.y - sa.y).abs();
    let pill = if horizontal {
        Rect::new(c.x - w * 0.5 - 4.0, c.y + 4.0, w + 8.0, 16.0)
    } else {
        Rect::new(c.x + 6.0, c.y - 8.0, w + 8.0, 16.0)
    };
    mb.fill_path(
        &MeshBuilder::rounded_rect_path(pill, 3.0),
        color,
        FillRule::NonZero,
    );
    draw_text(
        mb,
        &label,
        11.0,
        None,
        TextAlign::Left,
        v2(pill.x + 4.0, pill.y + 1.0),
        1.0,
        [1.0; 4],
    );
}

/// Anchors and handles while drawing with the pen or editing a path.
fn vector_edit_overlay(mb: &mut MeshBuilder, ed: &Editor, v: &View, accent: [f32; 4]) {
    let (id, selected, pen) = match (&ed.pen, &ed.path_edit) {
        (Some(p), _) => (p.id, None, true),
        (None, Some(pe)) => (pe.id, pe.selected, false),
        _ => return,
    };
    let (Some(n), Some(wr)) = (ed.doc.get(id), ed.doc.world_rect(id)) else {
        return;
    };
    let NodeKind::Path { subpaths, .. } = &n.kind else {
        return;
    };
    let s = |p: V2| v.to_screen(wr.min() + p);
    // Path outline in accent so it stays visible over any fill.
    let path = crate::vector::from_subpaths(wr.min(), subpaths).map(|p| v.to_screen(p));
    mb.stroke_path(&path.to_lyon(), 1.0, accent, LineCap::Round);
    for (si, sp) in subpaths.iter().enumerate() {
        let last = sp.points.len().saturating_sub(1);
        for (pi, pt) in sp.points.iter().enumerate() {
            let show_handles = selected == Some((si, pi)) || (pen && pi == last);
            if show_handles {
                for h in [pt.h_in, pt.h_out].into_iter().flatten() {
                    mb.line(s(pt.p), s(h), 1.0, accent);
                    mb.circle(s(h), 3.5, accent);
                    mb.circle(s(h), 2.0, [1.0; 4]);
                }
            }
            let c = s(pt.p);
            let r = Rect::new(c.x - 3.5, c.y - 3.5, 7.0, 7.0);
            let sel = selected == Some((si, pi));
            mb.rect(r, if sel { accent } else { [1.0; 4] });
            mb.rect_outline(r, 1.0, accent);
        }
    }
    // Rubber band from the last pen point to the pointer.
    if pen
        && !ed.is_busy()
        && let Some(last) = subpaths.first().and_then(|sp| sp.points.last())
    {
        mb.line(
            s(last.p),
            ed.pointer + v.offset,
            1.0,
            [accent[0], accent[1], accent[2], 0.6],
        );
    }
}

fn outline_node(mb: &mut MeshBuilder, v: &View, id: NodeId, sr: Rect, color: [f32; 4], w: f64) {
    let n = v.doc.get(id).unwrap();
    let z = v.camera.zoom;
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect | NodeKind::Text { .. } | NodeKind::Group => {
            mb.rect_outline(sr, w, color)
        }
        NodeKind::Stroke { points, .. } if points.len() > 1 => {
            let pts: Vec<V2> = points.iter().map(|q| sr.min() + q.pos() * z).collect();
            mb.stroke_path(
                &MeshBuilder::polygon_path(&pts, false),
                w as f32,
                color,
                LineCap::Round,
            );
            mb.rect_outline(sr, 1.0, [color[0], color[1], color[2], 0.5]);
        }
        _ => {
            let wr = v.doc.world_rect(id).unwrap();
            if let Some(path) = draw::shape_path(n, wr) {
                let sp = path.map(|p| v.to_screen(p));
                mb.stroke_path(&sp.to_lyon(), w as f32, color, LineCap::Round);
            }
        }
    }
}

fn fmt_num(x: f64) -> String {
    let r = (x * 10.0).round() / 10.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r:.1}")
    }
}

/// Tessellate a page background image in screen space and add it to an
/// underlay. The caller sets the batch clip to the page so Fill mode crops.
fn background_image_batch(under: &mut MeshBuilder, page: Rect, bg: &crate::model::BackgroundImage) {
    use crate::model::Paint;
    let paint = Paint::Image {
        asset: bg.asset.clone(),
        mode: bg.mode,
        opacity: bg.opacity,
    };
    let Some(spec) = draw::paint_spec(&paint, page, 1.0) else {
        return;
    };
    let ops = [draw::DrawOp::Fill {
        path: crate::vector::rounded_rect(page, [0.0; 4]),
        paint: spec,
        even_odd: false,
    }];
    for piece in Tess::new(1.0).ops(&ops) {
        if piece.mesh.is_empty() {
            continue;
        }
        under.set_image(piece.image);
        under.append_mesh(&piece.mesh);
        under.set_image(None);
    }
}

/// Full interactive canvas: background grid, document, overlay.
pub fn canvas(
    ed: &Editor,
    cache: &mut SceneCache,
    offset: V2,
    show_grid: bool,
    dark: bool,
) -> CanvasFrame {
    let v = View {
        doc: &ed.doc,
        camera: ed.camera,
        viewport: ed.viewport,
        offset,
    };
    let settings = &ed.doc.settings;
    let page = v.rect_to_screen(&settings.page_rect());
    let painted_page = settings.page_view || settings.background_enabled;
    let authored_page_bg = if settings.background_enabled {
        settings.background_color
    } else {
        Color::WHITE
    };
    // Connectors on a white page keep their authored colors even in dark UI theme.
    // A page set to None always draws in light mode, grid included.
    let dark_content = dark
        && settings.adaptive_colors != crate::model::AdaptiveColors::None
        && (!painted_page || authored_page_bg.luminance() < 0.2);
    let mode = ColorMode::new(settings.adaptive_colors, dark_content);
    let page_bg = mode.resolve(authored_page_bg, settings.background_color_dark);
    let mut under = MeshBuilder::default();
    under.set_clip(Some(v.canvas_rect()));
    under.rect(v.canvas_rect(), rgba(canvas_bg(dark_content)));
    if painted_page {
        let shadow = Rect::new(page.x + 4.0, page.y + 5.0, page.w, page.h);
        under.rect(shadow, [0.0, 0.0, 0.0, 0.25]);
        under.rect(page, rgba(page_bg));
    }
    if let Some(bg) = &settings.background_image
        && let Some(clip) = page.intersection(&v.canvas_rect())
    {
        under.set_clip(Some(clip));
        background_image_batch(&mut under, page, bg);
        under.set_clip(Some(v.canvas_rect()));
    }
    if show_grid {
        // Page view confines the grid to the printable page, as in draw.io. The
        // infinite canvas keeps drawing it edge to edge.
        if settings.page_view {
            if let Some(clip) = page.intersection(&v.canvas_rect()) {
                under.set_clip(Some(clip));
                grid(
                    &mut under,
                    &v,
                    ed.grid_step,
                    dark_content,
                    settings.grid_color,
                );
                under.set_clip(Some(v.canvas_rect()));
            }
        } else {
            grid(
                &mut under,
                &v,
                ed.grid_step,
                dark_content,
                settings.grid_color,
            );
        }
    }
    if settings.page_view {
        under.rect_outline(page, 1.0, rgba(Color::rgba(0, 0, 0, 70)));
    }
    let vis = ed.camera.visible_world(ed.viewport);
    let bucket = zoom_bucket(ed.camera.zoom);
    // Flowing connectors need fresh dash offsets each frame, so the previous
    // item list is only reusable while no visible connector is animated.
    let flowing =
        ed.doc.nodes.values().any(|n| {
            n.is_connector() && n.flow.enabled && n.visible && ed.doc.layer_of(n.id).visible
        });
    if cache.appearance != Some(mode) {
        cache.appearance = Some(mode);
        cache.clear();
    }
    let draw_doc: Cow<'_, Document> = if mode.is_dark() {
        Cow::Owned(ed.doc.with_appearance(mode))
    } else {
        Cow::Borrowed(&ed.doc)
    };
    let items = match &cache.last {
        Some((stamp, b, cull, theme, items))
            if !flowing
                && *stamp == ed.doc_stamp
                && *b == bucket
                && cull.contains_rect(&vis)
                && *theme == dark_content =>
        {
            cache.last_misses = 0;
            items.clone()
        }
        _ => {
            let cull = vis.inflate(vis.w.max(vis.h) * 0.5);
            let items = cache.items(
                draw_doc.as_ref(),
                None,
                ed.camera.zoom,
                Some(cull),
                dark_content,
                ed.now_ms,
            );
            if !flowing {
                cache.last = Some((ed.doc_stamp, bucket, cull, dark_content, items.clone()));
            }
            items
        }
    };
    let mut over = MeshBuilder::default();
    overlay(&mut over, ed, offset, dark);
    let fade = ed.page_transition_alpha(ed.now_ms);
    if fade > 0.0 {
        let mut veil = canvas_bg(dark_content);
        veil.a = (fade * 255.0) as u8;
        over.rect(v.canvas_rect(), rgba(veil));
    }
    CanvasFrame {
        under: under.finish(),
        items,
        over: over.finish(),
        camera: ed.camera,
        offset,
        viewport: v.canvas_rect(),
    }
}

/// A frame that draws only `roots` (or the page) with no chrome: exports and thumbnails.
pub fn export_frame(
    doc: &Document,
    roots: Option<&[NodeId]>,
    camera: Camera,
    size_px: V2,
    background: Option<Color>,
) -> CanvasFrame {
    let mut under = MeshBuilder::default();
    if let Some(bg) = background {
        under.rect(Rect::new(0.0, 0.0, size_px.x, size_px.y), rgba(bg));
    }
    if let Some(bg) = &doc.settings.background_image {
        let v = View {
            doc,
            camera,
            viewport: size_px,
            offset: V2::ZERO,
        };
        let page = v.rect_to_screen(&doc.settings.page_rect());
        if let Some(clip) = page.intersection(&Rect::new(0.0, 0.0, size_px.x, size_px.y)) {
            under.set_clip(Some(clip));
            background_image_batch(&mut under, page, bg);
            under.set_clip(None);
        }
    }
    // The caller bakes the appearance into `doc`; re-resolving here would invert
    // automatic dark colors a second time.
    let items = SceneCache::default().items(doc, roots, camera.zoom, None, false, 0.0);
    CanvasFrame {
        under: under.finish(),
        items,
        over: Vec::new(),
        camera,
        offset: V2::ZERO,
        viewport: Rect::new(0.0, 0.0, size_px.x, size_px.y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor_with_shape() -> Editor {
        let mut ed = Editor::default();
        ed.viewport = v2(1000.0, 800.0);
        ed.camera = Camera {
            pan: V2::ZERO,
            zoom: 1.0,
        };
        let id = ed.add_node(
            Node::new(
                NodeId(1),
                NodeKind::Rect,
                Rect::new(100.0, 100.0, 100.0, 60.0),
            ),
            None,
        );
        ed.selection = vec![id];
        ed
    }

    fn overlay_vertices(ed: &Editor) -> Vec<(f32, f32)> {
        let v = View {
            doc: &ed.doc,
            camera: ed.camera,
            viewport: ed.viewport,
            offset: V2::ZERO,
        };
        let mut mb = MeshBuilder::default();
        whiteboard_overlay(&mut mb, ed, &v, rgba(ACCENT));
        mb.batches
            .into_iter()
            .flat_map(|b| b.mesh.vertices.into_iter().map(|v| (v.pos[0], v.pos[1])))
            .collect()
    }

    fn has_point_near(pts: &[(f32, f32)], p: V2, tolerance: f64) -> bool {
        pts.iter().any(|(x, y)| {
            (*x as f64 - p.x).abs() <= tolerance && (*y as f64 - p.y).abs() <= tolerance
        })
    }

    fn has_point(pts: &[(f32, f32)], p: V2) -> bool {
        has_point_near(pts, p, 0.2)
    }

    #[test]
    fn selected_shape_connection_handles_are_outward_arrows() {
        let ed = editor_with_shape();
        let pts = overlay_vertices(&ed);
        // The right knob sits at (222,130). Its arrow points right.
        assert!(has_point(&pts, v2(230.0, 130.0)), "missing right arrow tip");
        assert!(
            has_point(&pts, v2(220.0, 126.0)),
            "missing right arrow base"
        );
        assert!(
            has_point(&pts, v2(220.0, 134.0)),
            "missing right arrow base"
        );
        // The bottom knob sits at (150,182). Its arrow points down.
        assert!(
            has_point(&pts, v2(150.0, 190.0)),
            "missing bottom arrow tip"
        );
    }

    #[test]
    fn selected_shape_shows_fixed_attachment_crosses() {
        let ed = editor_with_shape();
        let pts = overlay_vertices(&ed);
        // Right edge midpoint is (200,130). Stroke vertices sit half a line
        // width off the ideal endpoint, so allow a pixel of slack.
        assert!(
            has_point_near(&pts, v2(196.0, 126.0), 1.2),
            "missing first cross arm"
        );
        assert!(
            has_point_near(&pts, v2(204.0, 134.0), 1.2),
            "missing second cross arm"
        );
    }
}
