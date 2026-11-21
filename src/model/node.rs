//! Scene nodes.

use super::paint::{
    BlendMode, Cap, InstanceLink, Join, LayoutChild, Paint, StrokeAlign, TextStyle,
};
use crate::geom::{Rect, V2};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// draw.io's default marker size. Head sizes scale the native arrow geometry
/// relative to this value, so old documents keep their existing heads.
pub const DEFAULT_HEAD_SIZE: f64 = 6.0;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct NodeId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// sRGB color with straight alpha. Serialized as `#rrggbbaa`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Color {
        Color { r, g, b, a: 255 }
    }
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color { r, g, b, a }
    }
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const INK: Color = Color::rgb(28, 28, 30);
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
    pub fn from_hex(s: &str) -> Option<Color> {
        let s = s.strip_prefix('#')?;
        let p = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
        match s.len() {
            6 => Some(Color::rgb(p(0)?, p(2)?, p(4)?)),
            8 => Some(Color::rgba(p(0)?, p(2)?, p(4)?, p(6)?)),
            _ => None,
        }
    }
    /// Normalized RGBA with the alpha multiplied by `opacity`.
    pub fn to_f32(self, opacity: f64) -> [f32; 4] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
            (self.a as f64 / 255.0 * opacity) as f32,
        ]
    }
    /// Relative luminance as defined by WCAG 2.x (0 = black, 1 = white).
    pub fn luminance(self) -> f64 {
        fn channel(c: u8) -> f64 {
            let s = c as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }
    /// Black or white, whichever reads better on top of this color.
    pub fn contrasting_text(self) -> Color {
        if self.luminance() < 0.45 {
            Color::WHITE
        } else {
            Color::INK
        }
    }

    /// Near-black or near-white. draw.io's Simple adaptive mode flips only
    /// these neutrals and leaves every other authored color alone.
    pub fn is_neutral(self) -> bool {
        let l = self.luminance();
        l <= 0.02 || l >= 0.98
    }

    /// draw.io's automatic dark variant: 93% channel inversion followed by a
    /// 180 degree hue rotation. Alpha is left untouched.
    pub fn inverse(self) -> Color {
        // draw.io rounds the inverted channels before applying the hue matrix.
        let invert = |v: u8| ((255.0 - v as f64) * 0.93 + v as f64 * 0.07 + 0.5).floor();
        let (r, g, b) = (invert(self.r), invert(self.g), invert(self.b));
        let (cos, sin) = (-1.0_f64, 0.0_f64);
        let lr = 0.2126;
        let lg = 0.7152;
        let lb = 0.0722;
        let hr = 0.143;
        let hg = 0.140;
        let hb = 0.283;
        let channel = |m0: f64, m1: f64, m2: f64| -> u8 {
            (m0 * r + m1 * g + m2 * b).round().clamp(0.0, 255.0) as u8
        };
        Color::rgba(
            channel(
                lr + (1.0 - lr) * cos - lr * sin,
                lg - lg * cos - lg * sin,
                lb - lb * cos + (1.0 - lb) * sin,
            ),
            channel(
                lr - lr * cos + hr * sin,
                lg + (1.0 - lg) * cos + hg * sin,
                lb - lb * cos - hb * sin,
            ),
            channel(
                lr - lr * cos - (1.0 - lr) * sin,
                lg - lg * cos + lg * sin,
                lb - lb * cos + lb * sin,
            ),
            self.a,
        )
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Color::from_hex(&s).ok_or_else(|| serde::de::Error::custom(format!("invalid color {s:?}")))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    /// Fill layers, bottom to top.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Paint>,
    /// Follow the diagram scheme or shape default instead of the stored fill.
    /// The stored layers stay remembered for when Auto is switched off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill_auto: bool,
    /// Turning the fill off keeps the fill layers so the color survives.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub fill_enabled: bool,
    /// Last gradient parameters, restored when the Gradient toggle comes back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_gradient: Option<Paint>,
    pub stroke: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_dark: Option<Color>,
    /// Turning the line off keeps its color, width and dash pattern.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub stroke_enabled: bool,
    pub stroke_width: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub stroke_align: StrokeAlign,
    /// Dash pattern (lengths in px); empty = solid.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dash: Vec<f64>,
    #[serde(skip_serializing_if = "is_default")]
    pub cap: Cap,
    #[serde(skip_serializing_if = "is_default")]
    pub join: Join,
    pub corner_radius: f64,
    /// Per-corner radii (top-left, top-right, bottom-right, bottom-left); overrides
    /// `corner_radius` when set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radii: Option<[f64; 4]>,
    pub opacity: f64,
    #[serde(skip_serializing_if = "is_default")]
    pub blend: BlendMode,
    /// Hand-drawn (rough.js style) rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rough: Option<super::rough::Rough>,
    /// Explicit hand-drawn override; None follows the page's Sketch setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<bool>,
    /// Explicit shadow override; None follows the page's Shadow setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    /// Glass effect: a translucent highlight over the fill.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub glass: bool,
    /// Distance kept between the shape perimeter and bound connector ends.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub perimeter: f64,
    /// Clockwise rotation around the node's center, in degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// Keep the label horizontal when the shape rotates. The rotate handle and
    /// the exact Angle rotate the label with the shape; draw.io's "Rotate shape
    /// only by 90°" leaves it upright.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub label_upright: bool,
}

fn is_default<T: Default + PartialEq>(t: &T) -> bool {
    *t == T::default()
}
fn is_true(b: &bool) -> bool {
    *b
}
fn one() -> f64 {
    1.0
}
fn is_one(x: &f64) -> bool {
    *x == 1.0
}
fn is_zero(x: &f64) -> bool {
    *x == 0.0
}
fn default_head_size() -> f64 {
    DEFAULT_HEAD_SIZE
}
fn is_default_head_size(x: &f64) -> bool {
    *x == DEFAULT_HEAD_SIZE
}

impl Default for Style {
    fn default() -> Self {
        Style {
            fills: Vec::new(),
            fill_auto: false,
            fill_enabled: true,
            saved_gradient: None,
            stroke: None,
            stroke_dark: None,
            stroke_enabled: true,
            stroke_width: 1.0,
            stroke_align: StrokeAlign::Center,
            dash: Vec::new(),
            cap: Cap::Butt,
            join: Join::Miter,
            corner_radius: 0.0,
            radii: None,
            opacity: 1.0,
            blend: BlendMode::Normal,
            rough: None,
            sketch: None,
            shadow: None,
            glass: false,
            perimeter: 0.0,
            rotation: 0.0,
            label_upright: false,
        }
    }
}

impl Style {
    /// Repair the numeric fields after loading or hand-editing JSON, so a
    /// malformed value cannot break rendering or hit testing.
    pub fn sanitize(&mut self) {
        if !self.opacity.is_finite() {
            self.opacity = 1.0;
        }
        self.opacity = self.opacity.clamp(0.0, 1.0);
        if !self.stroke_width.is_finite() || self.stroke_width < 0.0 {
            self.stroke_width = 1.0;
        }
        self.stroke_width = self.stroke_width.min(1e4);
        if !self.corner_radius.is_finite() || self.corner_radius < 0.0 {
            self.corner_radius = 0.0;
        }
        if let Some(r) = &mut self.radii {
            for v in r {
                if !v.is_finite() || *v < 0.0 {
                    *v = 0.0;
                }
            }
        }
        if !self.perimeter.is_finite() {
            self.perimeter = 0.0;
        }
        self.perimeter = self.perimeter.clamp(0.0, 1e4);
        if !self.rotation.is_finite() {
            self.rotation = 0.0;
        }
        for p in &mut self.fills {
            p.sanitize();
        }
        if let Some(p) = &mut self.saved_gradient {
            p.sanitize();
        }
        self.dash.retain(|d| d.is_finite() && *d >= 0.0);
    }

    pub fn with_fill(c: Color) -> Style {
        Style {
            fills: vec![Paint::solid(c)],
            ..Style::default()
        }
    }
    /// The top solid fill color, if the top fill is solid.
    pub fn fill(&self) -> Option<Color> {
        self.fill_pair().map(|(color, _)| color)
    }

    /// The top solid fill's light and dark colors, if the top fill is solid.
    pub fn fill_pair(&self) -> Option<(Color, Option<Color>)> {
        match self.fills.last() {
            Some(Paint::Solid { color, dark }) => Some((*color, *dark)),
            _ => None,
        }
    }
    /// Replace all fills with one solid color (or none).
    pub fn set_fill(&mut self, c: Option<Color>) {
        self.fills = c.map(Paint::solid).into_iter().collect();
    }
    /// Corner radii (tl, tr, br, bl).
    pub fn corner_radii(&self) -> [f64; 4] {
        self.radii.unwrap_or([self.corner_radius; 4])
    }
}

/// One sampled input point of a freehand stroke, in node-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    /// 0..=1. Real pen pressure when available, otherwise simulated from velocity.
    pub p: f64,
}

impl StrokePoint {
    pub fn pos(&self) -> V2 {
        V2 {
            x: self.x,
            y: self.y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NodeKind {
    /// Artboard-like container; children are positioned relative to it.
    Frame {
        #[serde(default = "yes")]
        clip: bool,
    },
    Rect,
    Ellipse,
    /// Straight line between two node-local points.
    Line {
        a: V2,
        b: V2,
    },
    Text {
        text: String,
        font_size: f64,
        /// When true the box grows to fit the text; otherwise it wraps at `rect.w`.
        #[serde(default = "yes")]
        auto_width: bool,
        #[serde(default)]
        align: TextAlign,
    },
    /// Groups children; its rect is always the union of the children's rects.
    Group,
    /// Vector path (pen tool, boolean results, imported SVG). Points are node-local.
    Path {
        subpaths: Vec<SubPath>,
        #[serde(default)]
        fill_rule: FillRule,
    },
    /// Regular polygon inscribed in the box, first vertex at the top.
    Polygon {
        sides: u32,
    },
    /// Star inscribed in the box; `inner` is the inner radius ratio.
    Star {
        points: u32,
        inner: f64,
    },
    /// Arrow or connector line; ends may bind to other layers and follow them.
    Connector {
        /// Node-local points (at least two).
        points: Vec<V2>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Binding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Binding>,
        #[serde(default)]
        route: Route,
        #[serde(default)]
        start_head: Head,
        #[serde(default = "arrow_head")]
        end_head: Head,
        /// How the line hops over a crossing connector below it (draw.io's
        /// `jumpStyle`).
        #[serde(default)]
        jump: LineJump,
        /// Half gap of the hop in world units before the stroke width is added
        /// (draw.io's `jumpSize`, default 6).
        #[serde(default = "default_jump_size")]
        jump_size: f64,
    },
    /// Sticky note: a colored square whose text wraps inside it.
    Sticky {
        text: String,
        font_size: f64,
    },
    /// Freehand ink. Points are node-local; the outline is computed on render.
    Stroke {
        points: Vec<StrokePoint>,
        /// Nominal diameter at pressure 0.5.
        width: f64,
        /// How much pressure modulates width (0 = uniform width).
        #[serde(default = "default_thinning")]
        thinning: f64,
    },
}

fn yes() -> bool {
    true
}
fn arrow_head() -> Head {
    Head::Arrow
}

/// How a connector's line is drawn between its points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    #[default]
    Straight,
    /// Smooth curve through the points.
    Curved,
    /// Orthogonal segments routed around the bound shapes.
    Elbow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Head {
    #[default]
    None,
    Arrow,
    Triangle,
    Dot,
    Bar,
    Diamond,
}

/// How a connector's line hops over a crossing connector below it. Matches
/// draw.io's `jumpStyle` edge style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineJump {
    #[default]
    None,
    /// A semicircular bump over the crossing.
    Arc,
    /// The lower line simply interrupts this one.
    Gap,
    /// A sharp sideways step over the crossing.
    Sharp,
    /// A short tick across the line on both sides of the gap.
    Line,
}

impl LineJump {
    pub const ALL: [LineJump; 5] = [
        LineJump::None,
        LineJump::Arc,
        LineJump::Gap,
        LineJump::Sharp,
        LineJump::Line,
    ];

    pub fn label(self) -> &'static str {
        match self {
            LineJump::None => "None",
            LineJump::Arc => "Arc",
            LineJump::Gap => "Gap",
            LineJump::Sharp => "Sharp",
            LineJump::Line => "Line",
        }
    }
}

fn default_jump_size() -> f64 {
    6.0
}

impl Head {
    pub const ALL: [Head; 6] = [
        Head::None,
        Head::Arrow,
        Head::Triangle,
        Head::Dot,
        Head::Bar,
        Head::Diamond,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Head::None => "None",
            Head::Arrow => "Arrow",
            Head::Triangle => "Triangle",
            Head::Dot => "Dot",
            Head::Bar => "Bar",
            Head::Diamond => "Diamond",
        }
    }
}

/// A connector end attached to another layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub node: NodeId,
    /// Attachment point as a fraction of the target's box (0..1 inside it).
    pub fixed: V2,
    /// Inside: the end sits exactly on the fixed point. Otherwise it stops at the outline.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inside: bool,
    /// Floating: recompute the point where the route meets the outline instead of
    /// keeping `fixed`. draw.io writes no `exitX`/`entryX` style for a floating end.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub floating: bool,
}
fn default_thinning() -> f64 {
    0.6
}

impl NodeKind {
    /// Editable text content (text layers and sticky notes).
    pub fn text(&self) -> Option<&String> {
        match self {
            NodeKind::Text { text, .. } | NodeKind::Sticky { text, .. } => Some(text),
            _ => None,
        }
    }
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            NodeKind::Text { text, .. } | NodeKind::Sticky { text, .. } => Some(text),
            _ => None,
        }
    }
    pub fn font_size(&self) -> Option<f64> {
        match self {
            NodeKind::Text { font_size, .. } | NodeKind::Sticky { font_size, .. } => {
                Some(*font_size)
            }
            _ => None,
        }
    }
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::Frame { .. } => "Frame",
            NodeKind::Rect => "Rectangle",
            NodeKind::Ellipse => "Ellipse",
            NodeKind::Line { .. } => "Line",
            NodeKind::Text { .. } => "Text",
            NodeKind::Stroke { .. } => "Drawing",
            NodeKind::Group => "Group",
            NodeKind::Path { .. } => "Vector",
            NodeKind::Polygon { .. } => "Polygon",
            NodeKind::Star { .. } => "Star",
            NodeKind::Connector { .. } => "Arrow",
            NodeKind::Sticky { .. } => "Sticky",
        }
    }
}

/// Spacing between a text layer's box edges and its content (draw.io's
/// Spacing Top/Left/Bottom/Right). Grows the auto-sized box, insets the
/// wrapped line column, and offsets the drawn text.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pad {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Pad {
    pub fn is_zero(&self) -> bool {
        self.left == 0.0 && self.top == 0.0 && self.right == 0.0 && self.bottom == 0.0
    }
}

/// Vertical position of text inside its box (draw.io's Position Top/Middle/
/// Bottom). Only visible when the box is taller than its content; a Middle
/// or Bottom layer also stops auto-fitting its height so the position shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

impl VAlign {
    pub fn is_top(&self) -> bool {
        matches!(self, VAlign::Top)
    }
}

/// Writing direction override for text layers (draw.io's Writing Direction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum WritingDir {
    /// Detect from the content (Arabic shapes bidi; Latin stays LTR).
    #[default]
    Auto,
    Ltr,
    Rtl,
    /// Stack characters top to bottom, one per line.
    Vertical,
}

impl WritingDir {
    pub fn is_auto(&self) -> bool {
        matches!(self, WritingDir::Auto)
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How a connector's dashes move along its route (draw.io's flow animation).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "snake_case")]
pub struct FlowAnimation {
    /// Whether the dashes move on the interactive canvas.
    pub enabled: bool,
    /// Milliseconds for one animation cycle; larger values flow more slowly.
    pub duration_ms: f64,
    pub timing: FlowTiming,
    pub direction: FlowDirection,
}

impl FlowAnimation {
    pub const DEFAULT_DURATION_MS: f64 = 500.0;

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Dash pattern used for the animation. An authored dash wins; a solid
    /// connector gets a simple long-short pattern so the motion stays readable.
    pub fn dash_pattern(&self, authored: &[f64], stroke_width: f64) -> Vec<f64> {
        if authored.iter().any(|d| *d > 0.0) {
            return authored.to_vec();
        }
        let w = stroke_width.max(0.5);
        vec![w * 6.0, w * 4.0]
    }

    /// Distance into the dash pattern at `now_ms`, in world units.
    pub fn dash_offset(&self, now_ms: f64, pattern: &[f64]) -> f64 {
        let cycle: f64 = pattern.iter().map(|d| d.max(0.01)).sum();
        if cycle <= 0.0 || self.duration_ms <= 0.0 {
            return 0.0;
        }
        let t = now_ms.rem_euclid(self.duration_ms) / self.duration_ms;
        let eased = self.timing.ease(t);
        self.direction.progress(eased) * cycle
    }
}

impl Default for FlowAnimation {
    fn default() -> Self {
        Self {
            enabled: false,
            duration_ms: Self::DEFAULT_DURATION_MS,
            timing: FlowTiming::Linear,
            direction: FlowDirection::Normal,
        }
    }
}

/// Easing applied inside each animation cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowTiming {
    #[default]
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl FlowTiming {
    pub const ALL: [Self; 5] = [
        Self::Linear,
        Self::Ease,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Ease => "Ease",
            Self::EaseIn => "Ease In",
            Self::EaseOut => "Ease Out",
            Self::EaseInOut => "Ease In Out",
        }
    }

    pub fn style(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Ease => "ease",
            Self::EaseIn => "ease-in",
            Self::EaseOut => "ease-out",
            Self::EaseInOut => "ease-in-out",
        }
    }

    pub fn from_style(s: &str) -> Self {
        match s {
            "ease" => Self::Ease,
            "ease-in" => Self::EaseIn,
            "ease-out" => Self::EaseOut,
            "ease-in-out" => Self::EaseInOut,
            _ => Self::Linear,
        }
    }

    fn ease(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => t * t * (3.0 - 2.0 * t),
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// Playback direction of the dash cycle, matching CSS animation directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

impl FlowDirection {
    pub const ALL: [Self; 4] = [
        Self::Normal,
        Self::Reverse,
        Self::Alternate,
        Self::AlternateReverse,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Reverse => "Reverse",
            Self::Alternate => "Alternate",
            Self::AlternateReverse => "Alternate Reverse",
        }
    }

    pub fn style(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Reverse => "reverse",
            Self::Alternate => "alternate",
            Self::AlternateReverse => "alternate-reverse",
        }
    }

    pub fn from_style(s: &str) -> Self {
        match s {
            "reverse" => Self::Reverse,
            "alternate" => Self::Alternate,
            "alternate-reverse" => Self::AlternateReverse,
            _ => Self::Normal,
        }
    }

    fn progress(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Normal => t,
            Self::Reverse => 1.0 - t,
            Self::Alternate => {
                if t < 0.5 {
                    t * 2.0
                } else {
                    (1.0 - t) * 2.0
                }
            }
            Self::AlternateReverse => {
                if t < 0.5 {
                    1.0 - t * 2.0
                } else {
                    (t - 0.5) * 2.0
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Bounds relative to the parent frame's origin (or the world for root nodes).
    pub rect: Rect,
    #[serde(default)]
    pub style: Style,
    /// Default connector style for new arrows started from this shape (CON-19).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_default: Option<super::settings::DefaultStyle>,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
    /// Page layer this node is a member of (ADR-009). None inherits the
    /// nearest ancestor's layer, or the Background layer at the root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<super::layer::LayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Paint order, bottom to top.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<NodeId>,
    /// Rich text attributes (text nodes).
    #[serde(default, skip_serializing_if = "TextStyle::is_default")]
    pub text_style: TextStyle,
    /// Auto layout (frames only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<super::paint::AutoLayout>,
    /// How this node sizes and positions itself inside its parent frame.
    #[serde(default, skip_serializing_if = "LayoutChild::is_default")]
    pub layout_child: LayoutChild,
    /// This frame is a main component.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub component: bool,
    /// Instance link (instance roots and every node inside an instance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<InstanceLink>,
    /// Text drawn centered inside the shape. Empty for shapes without a label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Explicit label color; None picks black or white from the shape fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<Color>,
    /// Dark-mode label color paired with `label_color`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color_dark: Option<Color>,
    /// Color to restore when the label color is switched back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color_saved: Option<Color>,
    /// Shape-label font size; None uses the standard label size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_size: Option<f64>,
    /// Shape-label vertical position; None centers it like draw.io's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_valign: Option<VAlign>,
    /// Shape label wraps at the shape width (draw.io's Word Wrap).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub label_wrap: bool,
    /// Label glyph opacity, 0..1; separate from the shape's own opacity.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub label_opacity: f64,
    /// Where a connector label sits along the routed line, 0..1. None centers
    /// it on the line's midpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_pos: Option<f64>,
    /// Offset of a connector label from that route point, in world units.
    /// None keeps the label on the line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_offset: Option<V2>,
    /// Rich-text spans are drawn; turning this off keeps the spans stored.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub text_formatted: bool,
    /// Spacing between a text layer's box edges and its content.
    #[serde(default, skip_serializing_if = "Pad::is_zero")]
    pub pad: Pad,
    /// Text box background (draw.io's Background Color). Kept off `style.fills`,
    /// which carries the FONT color for text layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bg: Option<Color>,
    /// Dark-mode text box background paired with `text_bg`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bg_dark: Option<Color>,
    /// Vertical position of the text inside its box.
    #[serde(default, skip_serializing_if = "VAlign::is_top")]
    pub valign: VAlign,
    /// Writing direction override (draw.io's Writing Direction).
    #[serde(default, skip_serializing_if = "WritingDir::is_auto")]
    pub dir: WritingDir,
    /// Soft drop shadow behind the glyphs (draw.io's Shadow).
    #[serde(default, skip_serializing_if = "is_false")]
    pub text_shadow: bool,
    /// Border color of a shape label's text box (text layers reuse `style.stroke`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_border: Option<Color>,
    /// Dark-mode label border paired with `text_border`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_border_dark: Option<Color>,
    /// Border color to restore when the label border is switched back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_border_saved: Option<Color>,
    /// Background color to restore when the label background is switched back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bg_saved: Option<Color>,
    /// draw.io's Line start Size for a connector head. Other nodes keep the default.
    #[serde(
        default = "default_head_size",
        skip_serializing_if = "is_default_head_size"
    )]
    pub start_head_size: f64,
    /// draw.io's Line end Size for a connector head. Other nodes keep the default.
    #[serde(
        default = "default_head_size",
        skip_serializing_if = "is_default_head_size"
    )]
    pub end_head_size: f64,
    /// draw.io's Line start Offset, in world units. Negative moves the tip inside.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start_head_offset: f64,
    /// draw.io's Line end Offset, in world units. Negative moves the tip inside.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub end_head_offset: f64,
    /// Flow animation for connectors (CON-16). Other nodes keep the default.
    #[serde(default, skip_serializing_if = "FlowAnimation::is_default")]
    pub flow: FlowAnimation,
    /// Extra connector labels beyond the first (draw.io allows several labels
    /// per edge). The first label stays in `label`/`label_pos`/`label_offset`.
    /// Extras inherit the connector's label style and carry only their text
    /// and placement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<ExtraLabel>,
    /// Named string properties (draw.io shape metadata). A `BTreeMap` keeps
    /// keys in a stable order for saving, the inspector and export.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Target followed by a command-click or the Data tab's Open Link button.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    /// Text shown after hovering the node for a moment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tooltip: String,
    /// Free-form tags, in the order they were entered, without duplicates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// A second or later label on a connector. Style (color, size, wrap,
/// background, opacity, font) is inherited from the connector's own label
/// fields, matching draw.io where extra labels copy the edge's style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtraLabel {
    /// Label text; never empty (extras with empty text are pruned).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    /// Same semantics as `Node.label_pos`: distance along the route, 0..1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<f64>,
    /// Same semantics as `Node.label_offset`: world-space displacement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<V2>,
}

impl ExtraLabel {
    pub fn new(text: impl Into<String>) -> ExtraLabel {
        ExtraLabel {
            text: text.into(),
            pos: None,
            offset: None,
        }
    }
}

/// One anchor of a vector path, with optional bezier handles (node-local, absolute).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathPoint {
    pub p: V2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h_in: Option<V2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h_out: Option<V2>,
}

impl PathPoint {
    pub fn corner(p: V2) -> PathPoint {
        PathPoint {
            p,
            h_in: None,
            h_out: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    pub points: Vec<PathPoint>,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

impl Node {
    pub fn new(id: NodeId, kind: NodeKind, rect: Rect) -> Node {
        let gray = Color::rgb(0xd9, 0xd9, 0xd9);
        let style = match &kind {
            NodeKind::Frame { .. } => Style::with_fill(Color::WHITE),
            NodeKind::Rect
            | NodeKind::Ellipse
            | NodeKind::Polygon { .. }
            | NodeKind::Star { .. }
            | NodeKind::Path { .. } => Style::with_fill(gray),
            NodeKind::Line { .. } => Style {
                stroke: Some(Color::INK),
                stroke_width: 2.0,
                ..Style::default()
            },
            NodeKind::Text { .. } | NodeKind::Stroke { .. } => Style::with_fill(Color::INK),
            NodeKind::Group => Style::default(),
            NodeKind::Connector { .. } => Style {
                stroke: Some(Color::INK),
                stroke_width: 2.0,
                ..Style::default()
            },
            NodeKind::Sticky { .. } => Style::with_fill(Color::rgb(0xff, 0xdf, 0x6b)),
        };
        Node {
            id,
            name: kind.type_name().to_string(),
            kind,
            rect,
            style,
            connector_default: None,
            visible: true,
            locked: false,
            layer: None,
            parent: None,
            children: Vec::new(),
            text_style: TextStyle::default(),
            layout: None,
            layout_child: LayoutChild::default(),
            component: false,
            instance: None,
            label: String::new(),
            label_color: None,
            label_color_dark: None,
            label_color_saved: None,
            label_size: None,
            label_valign: None,
            label_wrap: true,
            label_opacity: 1.0,
            label_pos: None,
            label_offset: None,
            text_formatted: true,
            pad: Pad::default(),
            text_bg: None,
            text_bg_dark: None,
            valign: VAlign::Top,
            dir: WritingDir::Auto,
            text_shadow: false,
            text_border: None,
            text_border_dark: None,
            text_border_saved: None,
            text_bg_saved: None,
            start_head_size: DEFAULT_HEAD_SIZE,
            end_head_size: DEFAULT_HEAD_SIZE,
            start_head_offset: 0.0,
            end_head_offset: 0.0,
            flow: FlowAnimation::default(),
            labels: Vec::new(),
            metadata: BTreeMap::new(),
            link: String::new(),
            tooltip: String::new(),
            tags: Vec::new(),
        }
    }

    /// Whether the node can carry a label (draw.io style). Shape labels sit
    /// inside their shape; connector labels sit on the routed line.
    pub fn supports_label(&self) -> bool {
        !matches!(
            self.kind,
            NodeKind::Frame { .. }
                | NodeKind::Text { .. }
                | NodeKind::Sticky { .. }
                | NodeKind::Group
                | NodeKind::Stroke { .. }
        )
    }

    /// Whether this node can hold a default style for connectors started from it.
    pub fn can_have_connector_default(&self) -> bool {
        !matches!(self.kind, NodeKind::Connector { .. } | NodeKind::Group)
    }

    pub fn is_connector(&self) -> bool {
        matches!(self.kind, NodeKind::Connector { .. })
    }

    /// Text that double-clicking the node edits: its own text, a frame title, or a shape label.
    pub fn edited_text(&self) -> Option<&String> {
        if self.is_frame() {
            Some(&self.name)
        } else if self.kind.text().is_some() {
            self.kind.text()
        } else if self.supports_label() {
            Some(&self.label)
        } else {
            None
        }
    }

    pub fn edited_text_mut(&mut self) -> Option<&mut String> {
        if self.is_frame() {
            Some(&mut self.name)
        } else if self.kind.text().is_some() {
            self.kind.text_mut()
        } else if self.supports_label() {
            Some(&mut self.label)
        } else {
            None
        }
    }

    /// Editable text together with the text style that belongs to it.
    pub fn edited_text_and_style_mut(&mut self) -> Option<(&mut String, &mut TextStyle)> {
        if self.is_frame() {
            Some((&mut self.name, &mut self.text_style))
        } else if self.kind.text().is_some() {
            Some((self.kind.text_mut()?, &mut self.text_style))
        } else if self.supports_label() {
            Some((&mut self.label, &mut self.text_style))
        } else {
            None
        }
    }

    /// Number of labels: the main label plus any extras (connectors only in
    /// practice; other nodes never grow extras).
    pub fn label_count(&self) -> usize {
        if self.labels.is_empty() {
            1
        } else {
            1 + self.labels.len()
        }
    }

    /// Text of label `idx`: 0 is the main label, higher indices are extras.
    /// Index 0 may be empty (a connector can carry only extra labels).
    pub fn label_text(&self, idx: usize) -> Option<&str> {
        match idx {
            0 => Some(self.label.as_str()),
            i => self.labels.get(i - 1).map(|l| l.text.as_str()),
        }
    }

    /// Editable text of label `idx`, for caret math when opening the editor.
    pub fn edited_label(&self, idx: usize) -> Option<&String> {
        if idx == 0 {
            self.edited_text()
        } else {
            self.labels.get(idx - 1).map(|l| &l.text)
        }
    }

    /// Editable text of label `idx` together with the style that belongs to
    /// it. Extras share the connector's text style, like draw.io's extra
    /// labels copying the edge style.
    pub fn edited_label_and_style_mut(
        &mut self,
        idx: usize,
    ) -> Option<(&mut String, &mut TextStyle)> {
        if idx == 0 {
            return self.edited_text_and_style_mut();
        }
        let extra = self.labels.get_mut(idx - 1)?;
        Some((&mut extra.text, &mut self.text_style))
    }

    pub fn is_group(&self) -> bool {
        matches!(self.kind, NodeKind::Group)
    }

    /// Frames and groups hold children.
    pub fn is_container(&self) -> bool {
        matches!(self.kind, NodeKind::Frame { .. } | NodeKind::Group)
    }

    pub fn is_frame(&self) -> bool {
        matches!(self.kind, NodeKind::Frame { .. })
    }

    /// Change the local bounds, scaling intrinsic geometry (ink, lines) to match.
    pub fn set_rect_scaling(&mut self, new: Rect) {
        let old = self.rect;
        let sx = if old.w.abs() > 1e-9 {
            new.w / old.w
        } else {
            1.0
        };
        let sy = if old.h.abs() > 1e-9 {
            new.h / old.h
        } else {
            1.0
        };
        match &mut self.kind {
            NodeKind::Stroke { points, .. } => {
                for p in points.iter_mut() {
                    p.x *= sx;
                    p.y *= sy;
                }
            }
            NodeKind::Line { a, b } => {
                a.x *= sx;
                a.y *= sy;
                b.x *= sx;
                b.y *= sy;
            }
            NodeKind::Connector { points, .. } => {
                for p in points.iter_mut() {
                    p.x *= sx;
                    p.y *= sy;
                }
            }
            NodeKind::Path { subpaths, .. } => {
                let sc = |v: &mut V2| {
                    v.x *= sx;
                    v.y *= sy;
                };
                for sp in subpaths.iter_mut() {
                    for pt in sp.points.iter_mut() {
                        sc(&mut pt.p);
                        if let Some(h) = &mut pt.h_in {
                            sc(h);
                        }
                        if let Some(h) = &mut pt.h_out {
                            sc(h);
                        }
                    }
                }
            }
            NodeKind::Text { auto_width, .. } if (new.w - old.w).abs() > 1e-9 => {
                *auto_width = false;
            }
            _ => {}
        }
        self.rect = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_hex_roundtrip() {
        for c in [Color::rgb(1, 2, 255), Color::rgba(0x12, 0x34, 0x56, 0x78)] {
            assert_eq!(Color::from_hex(&c.to_hex()), Some(c));
        }
        assert_eq!(Color::from_hex("#abc"), None);
        assert_eq!(Color::from_hex("ffffff"), None);
    }

    #[test]
    fn scaling_rect_scales_ink() {
        let mut n = Node::new(
            NodeId(1),
            NodeKind::Stroke {
                points: vec![StrokePoint {
                    x: 10.0,
                    y: 20.0,
                    p: 0.5,
                }],
                width: 4.0,
                thinning: 0.5,
            },
            Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        n.set_rect_scaling(Rect::new(5.0, 5.0, 200.0, 50.0));
        let NodeKind::Stroke { points, .. } = &n.kind else {
            unreachable!()
        };
        assert_eq!((points[0].x, points[0].y), (20.0, 10.0));
    }

    #[test]
    fn node_json_shape() {
        let n = Node::new(
            NodeId(7),
            NodeKind::Frame { clip: true },
            Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let s = serde_json::to_string(&n).unwrap();
        assert!(s.contains("\"type\":\"frame\""), "{s}");
        assert!(s.contains("\"color\":\"#ffffff\""), "{s}");
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn glass_flag_round_trips_in_native_json() {
        let mut n = Node::new(NodeId(8), NodeKind::Rect, Rect::new(0.0, 0.0, 40.0, 30.0));
        n.style.glass = true;
        let s = serde_json::to_string(&n).unwrap();
        assert!(s.contains("\"glass\":true"), "{s}");
        let back: Node = serde_json::from_str(&s).unwrap();
        assert!(back.style.glass);
    }

    #[test]
    fn paragraph_list_styles_round_trip_in_native_json() {
        use crate::model::{ListKind, ParagraphStyle};
        let mut n = Node::new(
            NodeId(9),
            NodeKind::Text {
                text: "one\ntwo\nthree".into(),
                font_size: 16.0,
                auto_width: true,
                align: TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 120.0, 60.0),
        );
        n.text_style.paragraphs = vec![
            ParagraphStyle {
                list: ListKind::Bullet,
                indent: 0,
            },
            ParagraphStyle {
                list: ListKind::Bullet,
                indent: 2,
            },
            ParagraphStyle::default(),
        ];
        let s = serde_json::to_string(&n).unwrap();
        assert!(s.contains("\"list\":\"bullet\""), "{s}");
        assert!(s.contains("\"indent\":2"), "{s}");
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back.text_style.paragraphs.len(), 3);
        assert_eq!(back.text_style.paragraphs[0].list, ListKind::Bullet);
        assert_eq!(back.text_style.paragraphs[1].indent, 2);
        assert_eq!(back.text_style.paragraphs[2].list, ListKind::None);
    }

    #[test]
    fn connector_head_defaults_round_trip_without_stored_keys() {
        let n = Node::new(
            NodeId(8),
            NodeKind::Connector {
                points: vec![V2::ZERO, crate::geom::v2(100.0, 0.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 100.0, 0.0),
        );
        assert_eq!(n.start_head_size, DEFAULT_HEAD_SIZE);
        assert_eq!(n.end_head_size, DEFAULT_HEAD_SIZE);
        assert_eq!(n.start_head_offset, 0.0);
        assert_eq!(n.end_head_offset, 0.0);
        let s = serde_json::to_string(&n).unwrap();
        assert!(!s.contains("start_head_size"), "{s}");
        assert!(!s.contains("end_head_offset"), "{s}");
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn non_default_head_size_and_offset_round_trip_in_native_json() {
        let mut n = Node::new(
            NodeId(9),
            NodeKind::Connector {
                points: vec![V2::ZERO, crate::geom::v2(100.0, 0.0)],
                start: None,
                end: None,
                route: Route::Straight,
                start_head: Head::None,
                end_head: Head::Arrow,
                jump: LineJump::None,
                jump_size: 6.0,
            },
            Rect::new(0.0, 0.0, 100.0, 0.0),
        );
        n.start_head_size = 11.0;
        n.end_head_size = 4.0;
        n.start_head_offset = 6.0;
        n.end_head_offset = -2.0;
        let s = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn sty10_per_corner_radius_changes_only_selected_corner() {
        let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 80.0));
        n.style.corner_radius = 4.0;
        assert_eq!(n.style.corner_radii(), [4.0, 4.0, 4.0, 4.0]);
        n.style.radii = Some([12.0, 4.0, 4.0, 4.0]);
        assert_eq!(n.style.corner_radii(), [12.0, 4.0, 4.0, 4.0]);
        let s = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back.style.corner_radii(), [12.0, 4.0, 4.0, 4.0]);
    }

    #[test]
    fn txt15_text_background_color_roundtrips() {
        let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 40.0));
        n.text_bg = Some(Color::rgb(255, 240, 200));
        n.text_bg_dark = Some(Color::rgb(40, 30, 20));
        let s = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&s).unwrap();
        assert_eq!(back.text_bg, Some(Color::rgb(255, 240, 200)));
        assert_eq!(back.text_bg_dark, Some(Color::rgb(40, 30, 20)));
    }

    #[test]
    fn txt17_text_shadow_flag_roundtrips() {
        let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 40.0));
        assert!(!n.text_shadow);
        n.text_shadow = true;
        let s = serde_json::to_string(&n).unwrap();
        let back: Node = serde_json::from_str(&s).unwrap();
        assert!(back.text_shadow);
    }

    #[test]
    fn sty08_line_caps_and_joins_roundtrip() {
        use crate::model::{Cap, Join};
        let mut n = Node::new(NodeId(1), NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 40.0));
        for cap in [Cap::Butt, Cap::Round, Cap::Square] {
            n.style.cap = cap;
            let back: Node = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
            assert_eq!(back.style.cap, cap);
        }
        for join in [Join::Miter, Join::Round, Join::Bevel] {
            n.style.join = join;
            let back: Node = serde_json::from_str(&serde_json::to_string(&n).unwrap()).unwrap();
            assert_eq!(back.style.join, join);
        }
    }
}
