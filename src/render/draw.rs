//! Nodes → backend-neutral vector draw operations in world coordinates.
//!
//! The GPU tessellator, the SVG writer and the PDF writer all consume these ops, so every
//! backend draws the same geometry: shape outlines, gradients, images, strokes and glyphs.

use crate::assets;
use crate::geom::{Rect, V2, v2};
use crate::ink;
use crate::model::{
    BlendMode, Cap, Color, Document, FillRule, ImageMode, Join, LineJump, Node, NodeId, NodeKind,
    Paint, Stop, StrokeAlign,
};
use crate::text::{self, FaceId};
use crate::vector::{self, VPath};

#[derive(Debug, Clone, PartialEq)]
pub enum PaintSpec {
    /// Straight-alpha RGBA, opacity already applied.
    Solid([f32; 4]),
    Linear {
        p0: V2,
        p1: V2,
        stops: Vec<Stop>,
        opacity: f32,
    },
    /// Ellipse with center and the two radius vectors.
    Radial {
        center: V2,
        rx: V2,
        ry: V2,
        stops: Vec<Stop>,
        opacity: f32,
    },
    Image {
        asset: String,
        /// World rect of one copy of the image.
        place: Rect,
        tile: bool,
        opacity: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    Fill {
        path: VPath,
        paint: PaintSpec,
        even_odd: bool,
    },
    Stroke {
        path: VPath,
        color: [f32; 4],
        width: f64,
        cap: Cap,
        join: Join,
        dash: Vec<f64>,
        /// Distance into the dash pattern at the start of the path. Flow
        /// animation advances it; authored static strokes leave it at zero.
        dash_offset: f64,
    },
    /// Glyphs sharing one face, size and color; positions are world baselines.
    Glyphs {
        face: FaceId,
        size: f64,
        color: [f32; 4],
        glyphs: Vec<(u16, V2)>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeDraw {
    pub ops: Vec<DrawOp>,
    pub blend: BlendMode,
}

pub fn rgba(c: Color, opacity: f64) -> [f32; 4] {
    c.to_f32(opacity)
}

/// Background the interactive dark theme paints behind the document. Display-only color
/// adaptation uses this value; saved documents and exports keep their authored colors.
pub const DARK_CANVAS: Color = Color::rgb(0x1e, 0x1e, 0x20);

/// Glyphs and strokes need to read as black on light and white on dark, not as a
/// middle gray. WCAG 4.5:1 is the accessibility floor; 14:1 keeps default ink
/// close to white on the dark canvas while still preserving hue and alpha.
const GLYPH_CONTRAST: f64 = 14.0;

fn srgb_channel(v: f64) -> f64 {
    let s = v.clamp(0.0, 1.0);
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

fn composited_luminance(c: [f32; 4], bg: [f32; 4]) -> f64 {
    let a = c[3].clamp(0.0, 1.0) as f64;
    let r = c[0] as f64 * a + bg[0] as f64 * (1.0 - a);
    let g = c[1] as f64 * a + bg[1] as f64 * (1.0 - a);
    let b = c[2] as f64 * a + bg[2] as f64 * (1.0 - a);
    0.2126 * srgb_channel(r) + 0.7152 * srgb_channel(g) + 0.0722 * srgb_channel(b)
}

fn contrast_ratio(c: [f32; 4], bg: [f32; 4]) -> f64 {
    let a = composited_luminance(c, bg);
    let b = composited_luminance(bg, [1.0, 1.0, 1.0, 1.0]);
    let (hi, lo) = if a > b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Lift a low-contrast color until it reads clearly against `bg`. Alpha and hue
/// are preserved; only enough white is mixed in to reach `GLYPH_CONTRAST`.
fn contrast_on(color: [f32; 4], bg: [f32; 4]) -> [f32; 4] {
    if color[3] <= 0.0 {
        return color;
    }
    if contrast_ratio(color, bg) >= GLYPH_CONTRAST {
        return color;
    }
    let mut lo = 0.0f64;
    let mut hi = 1.0f64;
    for _ in 0..18 {
        let t = (lo + hi) * 0.5;
        let mixed = [
            color[0] + (1.0 - color[0]) * t as f32,
            color[1] + (1.0 - color[1]) * t as f32,
            color[2] + (1.0 - color[2]) * t as f32,
            color[3],
        ];
        if contrast_ratio(mixed, bg) >= GLYPH_CONTRAST {
            hi = t;
        } else {
            lo = t;
        }
    }
    let t = hi as f32;
    [
        color[0] + (1.0 - color[0]) * t,
        color[1] + (1.0 - color[1]) * t,
        color[2] + (1.0 - color[2]) * t,
        color[3],
    ]
}

/// Lift a low-contrast stroke or solid head color until it reads clearly against the dark
/// canvas. The alpha and hue are preserved; only enough white is mixed in for high contrast.
pub fn contrast_for_dark(color: [f32; 4]) -> [f32; 4] {
    contrast_on(color, rgba(DARK_CANVAS, 1.0))
}

/// Background directly behind a label. An explicit plate wins; otherwise a
/// filled shape carries its own label. Text and connectors without a plate sit
/// on the canvas, so they return `None` and use the dark canvas color.
fn text_background(n: &Node, auto_fill: Option<Color>) -> Option<Color> {
    if let Some(bg) = n.text_bg {
        return Some(bg);
    }
    if is_area(n) && n.style.fill_enabled {
        return if n.style.fill_auto {
            auto_fill
        } else {
            n.style.fill()
        };
    }
    None
}

/// Display-only text lift for the interactive dark canvas. Exports call this
/// with `dark = false`, so authored colors stay in the saved document.
fn text_color_for_dark(
    n: &Node,
    color: Color,
    opacity: f64,
    auto_fill: Option<Color>,
    dark: bool,
) -> [f32; 4] {
    let authored = rgba(color, opacity);
    if !dark {
        return authored;
    }
    match text_background(n, auto_fill) {
        Some(bg) if bg.luminance() < 0.2 => contrast_on(authored, rgba(bg, 1.0)),
        Some(_) => authored,
        None => contrast_for_dark(authored),
    }
}

/// The node's outline in world space (None for text and groups).
pub fn shape_path(n: &Node, wr: Rect) -> Option<VPath> {
    Some(match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect => vector::rounded_rect(wr, n.style.corner_radii()),
        NodeKind::Ellipse => vector::ellipse(wr),
        NodeKind::Polygon { sides } => {
            vector::rounded_polygon(&vector::polygon_points(wr, *sides), n.style.corner_radius)
        }
        NodeKind::Star { points, inner } => vector::rounded_polygon(
            &vector::star_points(wr, *points, *inner),
            n.style.corner_radius,
        ),
        NodeKind::Path { subpaths, .. } => vector::from_subpaths(wr.min(), subpaths),
        NodeKind::Line { a, b } => VPath::polyline(&[wr.min() + *a, wr.min() + *b], false),
        NodeKind::Stroke {
            points,
            width,
            thinning,
        } => {
            let o = ink::outline(points, *width, *thinning);
            let pts: Vec<V2> = o.iter().map(|q| wr.min() + *q).collect();
            VPath::polyline(&pts, true)
        }
        NodeKind::Connector { points, route, .. } => {
            let pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
            crate::connector::line_path(&pts, *route)
        }
        NodeKind::Sticky { .. } => vector::rounded_rect(wr, [n.style.corner_radius.max(2.0); 4]),
        NodeKind::Text { .. } | NodeKind::Group => return None,
    })
}

/// Whether the outline is a closed area (fills apply).
pub fn is_area(n: &Node) -> bool {
    !matches!(
        n.kind,
        NodeKind::Line { .. }
            | NodeKind::Text { .. }
            | NodeKind::Group
            | NodeKind::Connector { .. }
    ) && !matches!(&n.kind, NodeKind::Path { subpaths, .. } if subpaths.iter().all(|s| !s.closed))
}

/// Stroke outline honoring inside/outside alignment for box shapes.
fn stroke_path(n: &Node, wr: Rect) -> Option<VPath> {
    let d = match n.style.stroke_align {
        StrokeAlign::Center => 0.0,
        StrokeAlign::Inside => -n.style.stroke_width * 0.5,
        StrokeAlign::Outside => n.style.stroke_width * 0.5,
    };
    if d == 0.0 {
        return shape_path(n, wr);
    }
    let r = wr.inflate(d);
    if r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    match &n.kind {
        NodeKind::Frame { .. } | NodeKind::Rect => Some(vector::rounded_rect(
            r,
            n.style
                .corner_radii()
                .map(|x| if x > 0.0 { (x + d).max(0.0) } else { 0.0 }),
        )),
        NodeKind::Ellipse => Some(vector::ellipse(r)),
        NodeKind::Polygon { sides } => Some(vector::rounded_polygon(
            &vector::polygon_points(r, *sides),
            n.style.corner_radius,
        )),
        NodeKind::Star { points, inner } => Some(vector::rounded_polygon(
            &vector::star_points(r, *points, *inner),
            n.style.corner_radius,
        )),
        _ => shape_path(n, wr),
    }
}

/// Where one copy of an image goes for a given mode.
pub fn image_place(mode: ImageMode, wr: Rect, iw: f64, ih: f64) -> Rect {
    let (iw, ih) = (iw.max(1.0), ih.max(1.0));
    match mode {
        ImageMode::Stretch => wr,
        ImageMode::Tile => Rect::new(wr.x, wr.y, iw, ih),
        ImageMode::Fill | ImageMode::Fit => {
            let s = if mode == ImageMode::Fill {
                (wr.w / iw).max(wr.h / ih)
            } else {
                (wr.w / iw).min(wr.h / ih)
            };
            let size = v2(iw * s, ih * s);
            let c = wr.center();
            Rect::new(c.x - size.x * 0.5, c.y - size.y * 0.5, size.x, size.y)
        }
    }
}

pub fn paint_spec(p: &Paint, wr: Rect, opacity: f64) -> Option<PaintSpec> {
    let at = |u: V2| v2(wr.x + u.x * wr.w, wr.y + u.y * wr.h);
    Some(match p {
        Paint::Solid { color, .. } => PaintSpec::Solid(rgba(*color, opacity)),
        Paint::Linear { stops, from, to } => PaintSpec::Linear {
            p0: at(*from),
            p1: at(*to),
            stops: sorted(stops),
            opacity: opacity as f32,
        },
        Paint::Radial {
            stops,
            center,
            radius,
        } => PaintSpec::Radial {
            center: at(*center),
            rx: v2(radius.x * wr.w, 0.0),
            ry: v2(0.0, radius.y * wr.h),
            stops: sorted(stops),
            opacity: opacity as f32,
        },
        Paint::Image {
            asset,
            mode,
            opacity: o,
        } => {
            let a = assets::get(asset);
            let (iw, ih) = a
                .as_ref()
                .map_or((wr.w, wr.h), |a| (a.width as f64, a.height as f64));
            if a.is_none() {
                // Missing asset: a neutral placeholder.
                return Some(PaintSpec::Solid(rgba(
                    Color::rgb(0xcc, 0xcc, 0xd2),
                    opacity * o,
                )));
            }
            PaintSpec::Image {
                asset: asset.clone(),
                place: image_place(*mode, wr, iw, ih),
                tile: *mode == ImageMode::Tile,
                opacity: (opacity * o) as f32,
            }
        }
    })
}

fn sorted(stops: &[Stop]) -> Vec<Stop> {
    let mut s = stops.to_vec();
    s.sort_by(|a, b| a.pos.total_cmp(&b.pos));
    s
}

/// Collects the connectors drawn below the current one so a connector can hop
/// over them (draw.io paints the jump on the edge painted last). Inactive and
/// free while no connector in the document enables line jumps.
pub struct JumpWalk {
    active: bool,
    /// Flattened routes of the connectors seen so far, in paint order.
    below: Vec<Vec<V2>>,
}

impl JumpWalk {
    pub fn new(doc: &Document) -> Self {
        let active = doc.nodes.values().any(
            |n| matches!(&n.kind, NodeKind::Connector { jump, .. } if *jump != LineJump::None),
        );
        JumpWalk {
            active,
            below: Vec::new(),
        }
    }

    /// The crossings where this connector hops over the ones below it.
    pub fn points_for(&self, n: &Node, wr: Rect) -> Vec<V2> {
        if !self.active {
            return Vec::new();
        }
        let NodeKind::Connector {
            points,
            route,
            jump,
            jump_size,
            ..
        } = &n.kind
        else {
            return Vec::new();
        };
        if *jump == LineJump::None || self.below.is_empty() {
            return Vec::new();
        }
        let pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
        let half = crate::connector::jump_half_size(*jump_size, n.style.stroke_width.max(0.5));
        crate::connector::line_jumps(&pts, *route, &self.below, half)
    }

    /// Remembers this connector's route for the connectors drawn after it.
    pub fn push(&mut self, n: &Node, wr: Rect) {
        if !self.active {
            return;
        }
        let NodeKind::Connector { points, route, .. } = &n.kind else {
            return;
        };
        let pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
        if let Some((poly, _)) = crate::connector::route_polyline(&pts, *route) {
            self.below.push(poly);
        }
    }
}

/// True when `roots` are exactly the document's top-level paint order, i.e. a
/// whole-page export rather than a frame or selection.
pub fn is_whole_page(doc: &Document, roots: &[NodeId]) -> bool {
    let all = doc.layered_root();
    all.len() == roots.len() && all.iter().all(|r| roots.contains(r))
}

/// The page's background image as one world-space fill op, in paint order
/// before the nodes. Frame and selection exports omit it.
pub fn background_ops(doc: &Document, roots: &[NodeId]) -> Vec<DrawOp> {
    let Some(bg) = &doc.settings.background_image else {
        return Vec::new();
    };
    if !is_whole_page(doc, roots) {
        return Vec::new();
    }
    let page = doc.settings.page_rect();
    let paint = Paint::Image {
        asset: bg.asset.clone(),
        mode: bg.mode,
        opacity: bg.opacity,
    };
    let Some(paint) = paint_spec(&paint, page, 1.0) else {
        return Vec::new();
    };
    vec![DrawOp::Fill {
        path: vector::rounded_rect(page, [0.0; 4]),
        paint,
        even_odd: false,
    }]
}

/// Draw ops for one node with no page-wide effect defaults.
pub fn node_draw(n: &Node, wr: Rect, opacity: f64) -> NodeDraw {
    node_draw_with(n, wr, opacity, false, false, None, &[], false, 0.0)
}

/// Draw ops for one node (not its children), in world space.
///
/// `shadow_default` and `sketch_default` are the page settings; explicit
/// per-element choices win. `auto_fill` resolves Auto fill mode from the
/// diagram scheme or shape default. `jump_pts` lists the crossings where this
/// connector's line hops over connectors drawn below it (see
/// `connector::line_jumps`); other node kinds ignore it. `dark` lifts connector colors for the
/// interactive dark canvas only; exports and saved documents keep the authored colors.
/// `now_ms` drives flow animation on connectors; exports pass zero.
#[allow(clippy::too_many_arguments)]
pub fn node_draw_with(
    n: &Node,
    wr: Rect,
    opacity: f64,
    shadow_default: bool,
    sketch_default: bool,
    auto_fill: Option<Color>,
    jump_pts: &[V2],
    dark: bool,
    now_ms: f64,
) -> NodeDraw {
    let mut ops = Vec::new();
    let rough_on = n
        .style
        .sketch
        .unwrap_or(n.style.rough.is_some() || sketch_default);
    let use_rough = rough_on
        && !matches!(
            n.kind,
            NodeKind::Text { .. } | NodeKind::Group | NodeKind::Stroke { .. }
        );
    if use_rough {
        // Auto fill is resolved before the rough renderer sees the node, so
        // hachure and zigzag fills follow the scheme too.
        let auto_node;
        let source = if n.style.fill_auto && auto_fill.is_some() {
            auto_node = {
                let mut t = n.clone();
                if let Some(c) = auto_fill {
                    t.style.fills = vec![Paint::solid(c)];
                }
                t
            };
            &auto_node
        } else {
            n
        };
        let r = source.style.rough.unwrap_or_default();
        ops = super::rough::node_ops(source, wr, opacity, &r, jump_pts, dark, now_ms);
    } else {
        match &n.kind {
            NodeKind::Group => {}
            NodeKind::Text { .. } => {
                // draw.io's Background Color for the text box.
                if let Some(bg) = n.text_bg {
                    ops.push(DrawOp::Fill {
                        path: vector::rounded_rect(wr, [n.style.corner_radius; 4]),
                        paint: PaintSpec::Solid(rgba(bg, opacity)),
                        even_odd: false,
                    });
                }
                // ... and Border Color.
                if let Some(sc) = n.style.stroke
                    && n.style.stroke_enabled
                    && n.style.stroke_width > 0.0
                {
                    ops.push(DrawOp::Stroke {
                        path: vector::rounded_rect(wr, [n.style.corner_radius; 4]),
                        color: rgba(sc, opacity),
                        width: n.style.stroke_width,
                        cap: n.style.cap,
                        join: n.style.join,
                        dash: n.style.dash.clone(),
                        dash_offset: 0.0,
                    });
                }
                text_ops(n, wr, opacity, auto_fill, dark, &mut ops);
            }
            NodeKind::Connector { .. } => {
                connector_ops(n, wr, opacity, &mut ops, false, jump_pts, dark, now_ms)
            }
            NodeKind::Sticky { .. } => {
                let c = if n.style.fill_auto {
                    auto_fill.unwrap_or_else(|| Color::rgb(0xff, 0xdf, 0x6b))
                } else {
                    n.style
                        .fills
                        .last()
                        .map_or(Color::rgb(0xff, 0xdf, 0x6b), |p| p.preview_color())
                };
                let rad = [n.style.corner_radius.max(2.0); 4];
                // Soft drop shadow, then the note and its text.
                for (i, a) in [(3.0, 0.05), (1.5, 0.07)] {
                    ops.push(DrawOp::Fill {
                        path: vector::rounded_rect(wr.translate(v2(0.0, i)).inflate(i * 0.5), rad),
                        paint: PaintSpec::Solid(rgba(
                            Color::rgba(0, 0, 0, (a * 255.0) as u8),
                            opacity,
                        )),
                        even_odd: false,
                    });
                }
                ops.push(DrawOp::Fill {
                    path: vector::rounded_rect(wr, rad),
                    paint: PaintSpec::Solid(rgba(c, opacity)),
                    even_odd: false,
                });
                if let Some(sc) = n.style.stroke
                    && n.style.stroke_enabled
                    && n.style.stroke_width > 0.0
                {
                    ops.push(DrawOp::Stroke {
                        path: vector::rounded_rect(wr, rad),
                        color: rgba(sc, opacity),
                        width: n.style.stroke_width,
                        cap: n.style.cap,
                        join: n.style.join,
                        dash: n.style.dash.clone(),
                        dash_offset: 0.0,
                    });
                }
                text_ops(n, wr, opacity, auto_fill, dark, &mut ops);
            }
            NodeKind::Line { .. } => {
                if n.style.stroke_enabled
                    && let (Some(c), Some(path)) = (n.style.stroke, shape_path(n, wr))
                {
                    ops.push(DrawOp::Stroke {
                        path,
                        color: rgba(c, opacity),
                        width: n.style.stroke_width.max(0.0),
                        cap: if n.style.cap == Cap::Butt {
                            Cap::Round
                        } else {
                            n.style.cap
                        },
                        join: n.style.join,
                        dash: n.style.dash.clone(),
                        dash_offset: 0.0,
                    });
                }
            }
            NodeKind::Stroke { .. } => {
                let c = n
                    .style
                    .fills
                    .last()
                    .map_or(Color::INK, |p| p.preview_color());
                if let Some(path) = shape_path(n, wr) {
                    ops.push(DrawOp::Fill {
                        path,
                        paint: PaintSpec::Solid(rgba(c, opacity)),
                        even_odd: false,
                    });
                }
            }
            _ => {
                if let Some(path) = shape_path(n, wr) {
                    let even_odd = matches!(
                        n.kind,
                        NodeKind::Path {
                            fill_rule: FillRule::EvenOdd,
                            ..
                        }
                    );
                    if is_area(n) && n.style.fill_enabled {
                        if n.style.fill_auto {
                            if let Some(c) = auto_fill {
                                ops.push(DrawOp::Fill {
                                    path: path.clone(),
                                    paint: PaintSpec::Solid(rgba(c, opacity)),
                                    even_odd,
                                });
                            }
                        } else {
                            for p in &n.style.fills {
                                if let Some(paint) = paint_spec(p, wr, opacity) {
                                    ops.push(DrawOp::Fill {
                                        path: path.clone(),
                                        paint,
                                        even_odd,
                                    });
                                }
                            }
                        }
                    }
                    if let Some(c) = n.style.stroke
                        && n.style.stroke_enabled
                        && n.style.stroke_width > 0.0
                        && let Some(sp) = stroke_path(n, wr)
                    {
                        ops.push(DrawOp::Stroke {
                            path: sp,
                            color: rgba(c, opacity),
                            width: n.style.stroke_width,
                            cap: n.style.cap,
                            join: n.style.join,
                            dash: n.style.dash.clone(),
                            dash_offset: 0.0,
                        });
                    }
                }
            }
        }
    }
    if n.supports_label() && !n.label.is_empty() {
        text_ops(n, wr, opacity, auto_fill, dark, &mut ops);
    }
    let shadow = n.style.shadow.unwrap_or(shadow_default) && use_rough_or_shape(n);
    if shadow {
        let mut shadows = Vec::new();
        for op in &ops {
            match op {
                DrawOp::Fill { path, .. } => shadows.push(DrawOp::Fill {
                    path: path.translate(v2(3.0, 4.0)),
                    paint: PaintSpec::Solid(rgba(Color::rgba(0, 0, 0, 46), opacity)),
                    even_odd: false,
                }),
                DrawOp::Stroke {
                    path,
                    width,
                    cap,
                    join,
                    dash,
                    dash_offset,
                    ..
                } => shadows.push(DrawOp::Stroke {
                    path: path.translate(v2(3.0, 4.0)),
                    color: rgba(Color::rgba(0, 0, 0, 40), opacity),
                    width: *width,
                    cap: *cap,
                    join: *join,
                    dash: dash.clone(),
                    dash_offset: *dash_offset,
                }),
                DrawOp::Glyphs { .. } => {}
            }
        }
        shadows.append(&mut ops);
        ops = shadows;
    }
    if n.style.glass
        && let Some(path) = shape_path(n, wr)
    {
        let top = v2(wr.x, wr.y);
        let bottom = v2(wr.x, wr.bottom());
        let glass = DrawOp::Fill {
            path: path.clone(),
            paint: PaintSpec::Linear {
                p0: top,
                p1: bottom,
                stops: vec![
                    Stop {
                        pos: 0.0,
                        color: Color::rgba(255, 255, 255, 120),
                        dark: None,
                    },
                    Stop {
                        pos: 0.55,
                        color: Color::rgba(255, 255, 255, 18),
                        dark: None,
                    },
                    Stop {
                        pos: 1.0,
                        color: Color::rgba(255, 255, 255, 0),
                        dark: None,
                    },
                ],
                opacity: opacity as f32,
            },
            even_odd: false,
        };
        let at = ops
            .iter()
            .position(|o| matches!(o, DrawOp::Glyphs { .. }))
            .unwrap_or(ops.len());
        ops.insert(at, glass);
    }
    if n.style.rotation != 0.0 {
        let center = wr.center();
        let angle = n.style.rotation.to_radians();
        let upright_from = n.style.label_upright.then(|| {
            // Everything from the first glyph run on is the label (its
            // underline and strikethrough follow the glyphs). Leave it in its
            // unrotated place so the text stays horizontal while the shape
            // still turns.
            ops.iter()
                .position(|o| matches!(o, DrawOp::Glyphs { .. }))
                .unwrap_or(ops.len())
        });
        match upright_from {
            Some(first) => rotate_ops(&mut ops[..first], center, angle),
            None => rotate_ops(&mut ops, center, angle),
        }
    }
    NodeDraw {
        ops,
        blend: n.style.blend,
    }
}

fn use_rough_or_shape(n: &Node) -> bool {
    !matches!(
        n.kind,
        NodeKind::Group | NodeKind::Text { .. } | NodeKind::Stroke { .. }
    )
}

fn rotate_point(p: V2, c: V2, a: f64) -> V2 {
    let (s, co) = a.sin_cos();
    let d = p - c;
    c + v2(d.x * co - d.y * s, d.x * s + d.y * co)
}

/// Rotate every op's geometry. Glyph runs become outlines so the glyphs
/// themselves rotate, which keeps the canvas, SVG and PDF exports identical.
fn rotate_ops(ops: &mut [DrawOp], center: V2, angle: f64) {
    let r = |p: V2| rotate_point(p, center, angle);
    for op in ops {
        match op {
            DrawOp::Fill { path, paint, .. } => {
                *path = path.map(r);
                rotate_paint(paint, center, angle);
            }
            DrawOp::Stroke { path, .. } => *path = path.map(r),
            DrawOp::Glyphs {
                face,
                size,
                color,
                glyphs,
            } => {
                let mut path = VPath::new();
                for (gid, p) in glyphs.iter() {
                    let g = text::glyph_path(*face, *gid, *p, *size);
                    path.append(&g.map(r));
                }
                let color = *color;
                *op = DrawOp::Fill {
                    path,
                    paint: PaintSpec::Solid(color),
                    even_odd: false,
                };
            }
        }
    }
}

fn rotate_paint(paint: &mut PaintSpec, center: V2, angle: f64) {
    let r = |p: V2| rotate_point(p, center, angle);
    match paint {
        PaintSpec::Linear { p0, p1, .. } => {
            *p0 = r(*p0);
            *p1 = r(*p1);
        }
        PaintSpec::Radial {
            center: c, rx, ry, ..
        } => {
            let nc = r(*c);
            *rx = r(*c + *rx) - nc;
            *ry = r(*c + *ry) - nc;
            *c = nc;
        }
        _ => {}
    }
}

/// A connector's line and heads. With `rough`, only the heads are produced (the line comes
/// from the rough renderer). `jump_pts` are this line's crossings over connectors below it.
/// `dark` lifts the display color on the interactive dark canvas only.
#[allow(clippy::too_many_arguments)]
pub fn connector_ops(
    n: &Node,
    wr: Rect,
    opacity: f64,
    ops: &mut Vec<DrawOp>,
    heads_only: bool,
    jump_pts: &[V2],
    dark: bool,
    now_ms: f64,
) {
    let NodeKind::Connector {
        points,
        route,
        start_head,
        end_head,
        jump,
        jump_size,
        ..
    } = &n.kind
    else {
        return;
    };
    if !n.style.stroke_enabled {
        return;
    }
    let Some(c) = n.style.stroke else { return };
    let authored = rgba(c, opacity);
    let color = if dark {
        contrast_for_dark(authored)
    } else {
        authored
    };
    let sw = n.style.stroke_width.max(0.5);
    let (dash, dash_offset) = if n.flow.enabled {
        let pattern = n.flow.dash_pattern(&n.style.dash, sw);
        let offset = if heads_only {
            0.0
        } else {
            n.flow.dash_offset(now_ms, &pattern)
        };
        (pattern, offset)
    } else {
        (n.style.dash.clone(), 0.0)
    };
    let mut pts: Vec<V2> = points.iter().map(|p| wr.min() + *p).collect();
    if pts.len() < 2 {
        return;
    }
    crate::connector::offset_endpoints(&mut pts, n.start_head_offset, n.end_head_offset);
    if !heads_only {
        // Labels without a solid background cut the line behind their text so
        // the characters stay readable; a plate paints over the line instead.
        // Jumps take precedence: the hop shapes span the cut if they overlap.
        let gaps = text::label_gap_rects(n);
        let half = crate::connector::jump_half_size(*jump_size, sw);
        let jumped = (!jump_pts.is_empty() && *jump != LineJump::None)
            .then(|| crate::connector::jumped_line_path(&pts, *route, jump_pts, *jump, half, &gaps))
            .flatten();
        let path = jumped.or_else(|| {
            if gaps.is_empty() {
                Some(crate::connector::line_path(&pts, *route))
            } else {
                crate::connector::cut_line_path(&pts, *route, &gaps)
            }
        });
        if let Some(path) = path {
            ops.push(DrawOp::Stroke {
                path,
                color,
                width: sw,
                cap: Cap::Round,
                join: Join::Round,
                dash,
                dash_offset,
            });
        }
    }
    let last = pts.len() - 1;
    for (head, tip, from, size) in [
        (*start_head, pts[0], pts[1], n.start_head_size),
        (*end_head, pts[last], pts[last - 1], n.end_head_size),
    ] {
        let (lines, polys) =
            crate::connector::head_shapes(head, tip, from, sw, tip.dist(from), size);
        for l in lines {
            ops.push(DrawOp::Stroke {
                path: VPath::polyline(&l, false),
                color,
                width: sw,
                cap: Cap::Round,
                join: Join::Round,
                dash: Vec::new(),
                dash_offset: 0.0,
            });
        }
        for p in polys {
            let path = VPath::polyline(&p, true);
            ops.push(DrawOp::Fill {
                path: path.clone(),
                paint: PaintSpec::Solid(color),
                even_odd: false,
            });
            ops.push(DrawOp::Stroke {
                path,
                color,
                width: sw,
                cap: Cap::Round,
                join: Join::Round,
                dash: Vec::new(),
                dash_offset: 0.0,
            });
        }
    }
}

fn text_ops(
    n: &Node,
    wr: Rect,
    opacity: f64,
    auto_fill: Option<Color>,
    dark: bool,
    ops: &mut Vec<DrawOp>,
) {
    // The main label plus any extra connector labels, each with its own
    // placement along the route.
    for idx in 0..n.label_count() {
        text_label_ops(n, wr, opacity, idx, auto_fill, dark, ops);
    }
}

fn text_label_ops(
    n: &Node,
    wr: Rect,
    opacity: f64,
    idx: usize,
    auto_fill: Option<Color>,
    dark: bool,
    ops: &mut Vec<DrawOp>,
) {
    let Some(l) = text::layout_node_label(n, idx) else {
        return;
    };
    // Shape labels have their own opacity, separate from the shape fill and
    // outline. Text layers follow the element opacity as before.
    let opacity = if n.supports_label() {
        opacity * n.label_opacity.clamp(0.0, 1.0)
    } else {
        opacity
    };
    let o = wr.min() + text::text_origin_label(n, &l, idx);
    // Shape and connector labels can carry their own background and border
    // box. Text layers draw theirs around the whole box in `node_draw`.
    if n.supports_label() && (n.text_bg.is_some() || n.text_border.is_some()) {
        let boxr = text::glyph_plate(&l).translate(o);
        if let Some(bg) = n.text_bg {
            ops.push(DrawOp::Fill {
                path: vector::rounded_rect(boxr, [n.style.corner_radius.min(4.0); 4]),
                paint: PaintSpec::Solid(rgba(bg, opacity)),
                even_odd: false,
            });
        }
        if let Some(bc) = n.text_border {
            ops.push(DrawOp::Stroke {
                path: vector::rounded_rect(boxr, [n.style.corner_radius.min(4.0); 4]),
                color: rgba(bc, opacity),
                width: 1.0,
                cap: Cap::Butt,
                join: Join::Miter,
                dash: Vec::new(),
                dash_offset: 0.0,
            });
        }
    }
    // Emit the glyphs of one styled run, either normally or as the shadow
    // pass (draw.io's Shadow): same shapes, black, slightly down-right.
    let mut i = 0;
    while i < l.glyphs.len() {
        let g0 = l.glyphs[i];
        let mut glyphs = Vec::new();
        while i < l.glyphs.len() {
            let g = &l.glyphs[i];
            if g.face != g0.face || g.size != g0.size || g.color != g0.color {
                break;
            }
            glyphs.push((g.gid, o + v2(g.x, g.baseline)));
            i += 1;
        }
        if n.text_shadow {
            let dxy = v2(g0.size * 0.07, g0.size * 0.10);
            ops.push(DrawOp::Glyphs {
                face: g0.face,
                size: g0.size,
                color: rgba(Color::rgba(0, 0, 0, 96), opacity),
                glyphs: glyphs.iter().map(|(g, p)| (*g, *p + dxy)).collect(),
            });
        }
        ops.push(DrawOp::Glyphs {
            face: g0.face,
            size: g0.size,
            color: text_color_for_dark(n, g0.color, opacity, auto_fill, dark),
            glyphs,
        });
    }
    // Underline / strikethrough as thin rectangles.
    for g in l.glyphs.iter().filter(|g| g.underline || g.strike) {
        let th = (g.size / 14.0).max(0.5);
        for (on, dy) in [(g.underline, g.size * 0.12), (g.strike, -g.size * 0.28)] {
            if on {
                let r = Rect::new(o.x + g.x, o.y + g.baseline + dy, g.advance, th);
                ops.push(DrawOp::Fill {
                    path: vector::rounded_rect(r, [0.0; 4]),
                    paint: PaintSpec::Solid(text_color_for_dark(
                        n, g.color, opacity, auto_fill, dark,
                    )),
                    even_odd: false,
                });
            }
        }
    }
}

/// Product of ancestor opacities.
pub fn inherited_opacity(doc: &Document, id: NodeId) -> f64 {
    doc.ancestors(id)
        .iter()
        .filter_map(|a| doc.get(*a))
        .map(|n| n.style.opacity)
        .product()
}

/// World bounds a node draws into (rect plus stroke overhang), without a shadow.
pub fn draw_bounds(n: &Node, wr: Rect) -> Rect {
    draw_bounds_with_shadow(n, wr, false)
}

/// World bounds a node draws into, including its shadow when active.
///
/// `shadow_default` is the page setting; an explicit per-element choice wins.
/// The draw ops translate a shadow by (3, 4) before the shape's rotation is
/// applied, so a rotated shape needs the same rotation on its shadow offset.
pub fn draw_bounds_with_shadow(n: &Node, wr: Rect, shadow_default: bool) -> Rect {
    let mut wr = wr;
    let mut rotation = None;
    if n.style.rotation != 0.0 {
        let c = wr.center();
        let a = n.style.rotation.to_radians();
        rotation = Some(a);
        let pts = [
            wr.min(),
            v2(wr.right(), wr.y),
            v2(wr.right(), wr.bottom()),
            v2(wr.x, wr.bottom()),
        ]
        .map(|p| rotate_point(p, c, a));
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        wr = Rect::new(x0, y0, x1 - x0, y1 - y0);
    }
    let mut pad = match n.style.stroke_align {
        StrokeAlign::Inside => 0.0,
        StrokeAlign::Center => n.style.stroke_width * 0.5,
        StrokeAlign::Outside => n.style.stroke_width,
    };
    if n.style.stroke.is_none() && !matches!(n.kind, NodeKind::Line { .. }) {
        pad = 0.0;
    }
    if n.style.rough.is_some() {
        pad += n.style.stroke_width * 2.0 + 4.0;
    }
    if matches!(n.kind, NodeKind::Line { .. }) {
        pad = pad.max(n.style.stroke_width * 0.5);
    }
    if matches!(n.kind, NodeKind::Connector { .. }) {
        // Arrowheads reach past the end points.
        pad = pad.max(n.style.stroke_width + 20.0);
        // Connector labels sit on the line and can reach past the line's
        // bounding box, so export, culling, and hit bounds have to include
        // every label, not just the main one.
        for idx in 0..n.label_count() {
            if let Some(b) = text::label_box_label(n, idx) {
                wr = wr.union(&Rect::new(wr.x + b.x, wr.y + b.y, b.w, b.h));
            }
        }
    }
    if matches!(n.kind, NodeKind::Sticky { .. }) {
        pad = pad.max(4.0);
    }
    if n.style.shadow.unwrap_or(shadow_default) && use_rough_or_shape(n) {
        let offset = match rotation {
            Some(a) => {
                let (s, c) = a.sin_cos();
                v2(3.0 * c - 4.0 * s, 3.0 * s + 4.0 * c)
            }
            None => v2(3.0, 4.0),
        };
        wr = wr.union(&wr.translate(offset));
    }
    wr.inflate(pad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Paint;

    fn filled_rect(shadow: Option<bool>) -> Node {
        let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(10.0, 10.0, 20.0, 20.0));
        n.style.fills = vec![Paint::solid(Color::rgb(20, 120, 180))];
        n.style.shadow = shadow;
        n
    }

    fn ops_with(node: &Node, default: bool) -> Vec<DrawOp> {
        node_draw_with(node, node.rect, 1.0, default, false, None, &[], false, 0.0).ops
    }

    #[test]
    fn shadow_draw_ops_follow_page_default_and_override() {
        let inherited = filled_rect(None);
        let ops = ops_with(&inherited, true);
        assert_eq!(ops.len(), 2, "page shadow adds one offset copy");
        let DrawOp::Fill { path, .. } = &ops[0] else {
            panic!("the first op must be the shadow fill");
        };
        let b = path.bounds().unwrap();
        assert!(
            (b.x - 13.0).abs() < 1e-6
                && (b.y - 14.0).abs() < 1e-6
                && (b.w - 20.0).abs() < 1e-6
                && (b.h - 20.0).abs() < 1e-6,
            "shadow fill should be translated by (3, 4), got {b:?}"
        );

        let overridden = filled_rect(Some(false));
        assert_eq!(ops_with(&overridden, true).len(), 1);
        let explicit = filled_rect(Some(true));
        assert_eq!(ops_with(&explicit, false).len(), 2);
    }

    #[test]
    fn shadow_bounds_include_the_offset_for_rotated_shapes() {
        let mut n = filled_rect(Some(true));
        n.style.rotation = 90.0;
        let b = draw_bounds_with_shadow(&n, n.rect, false);
        assert!(
            b.x <= 6.0 && b.y <= 10.0 && b.right() >= 30.0 && b.bottom() >= 33.0,
            "a quarter turn maps the shadow offset to (-4, 3), got {b:?}"
        );
    }

    #[test]
    fn glass_overlay_draws_over_fill_below_label() {
        let mut n = filled_rect(None);
        n.label = "Glass".into();
        n.style.glass = true;
        let ops = ops_with(&n, false);
        let glass = ops
            .iter()
            .position(|op| {
                matches!(
                    op,
                    DrawOp::Fill {
                        paint: PaintSpec::Linear { .. },
                        ..
                    }
                )
            })
            .expect("glass should add a gradient fill");
        let glyph = ops
            .iter()
            .position(|op| matches!(op, DrawOp::Glyphs { .. }))
            .expect("the label should draw");
        assert!(glass < glyph, "the highlight must sit below the label");
        let DrawOp::Fill {
            paint: PaintSpec::Linear { stops, .. },
            ..
        } = &ops[glass]
        else {
            unreachable!()
        };
        assert_eq!(stops.len(), 3, "the highlight fades from top to bottom");
        assert!(stops[0].color.a > stops[2].color.a);
    }
}
