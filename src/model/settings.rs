//! Diagram/page settings from draw.io's Diagram tab and global Style tab.
//!
//! These belong to the document (and therefore each page), not to element
//! styles. Grid visibility, guide toggles and the diagram-wide sketch/shadow
//! defaults travel with the file and participate in undo/redo through
//! `history::Recording`.

use super::{Color, ImageMode, Style, TextStyle};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// PostScript points per inch, the unit used by all paper dimensions here.
pub const POINTS_PER_INCH: f64 = 72.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PaperSize {
    #[default]
    Letter,
    Legal,
    Tabloid,
    A3,
    A4,
}

impl PaperSize {
    pub const ALL: [PaperSize; 5] = [
        PaperSize::Letter,
        PaperSize::Legal,
        PaperSize::Tabloid,
        PaperSize::A3,
        PaperSize::A4,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaperSize::Letter => "US-Letter (8.5 × 11 in)",
            PaperSize::Legal => "US-Legal (8.5 × 14 in)",
            PaperSize::Tabloid => "US-Tabloid (11 × 17 in)",
            PaperSize::A3 => "A3 (297 × 420 mm)",
            PaperSize::A4 => "A4 (210 × 297 mm)",
        }
    }

    /// Portrait dimensions in points (1 pt = 1 world unit).
    pub fn portrait_points(self) -> (f64, f64) {
        match self {
            PaperSize::Letter => (8.5 * POINTS_PER_INCH, 11.0 * POINTS_PER_INCH),
            PaperSize::Legal => (8.5 * POINTS_PER_INCH, 14.0 * POINTS_PER_INCH),
            PaperSize::Tabloid => (11.0 * POINTS_PER_INCH, 17.0 * POINTS_PER_INCH),
            PaperSize::A3 => (841.89, 1190.55),
            PaperSize::A4 => (595.28, 841.89),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    Portrait,
    #[default]
    Landscape,
}

/// Effect played when the editor switches to a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PageTransition {
    #[default]
    None,
    Fade,
}

fn default_transition_ms() -> u64 {
    400
}

impl Orientation {
    pub fn label(self) -> &'static str {
        match self {
            Orientation::Portrait => "Portrait",
            Orientation::Landscape => "Landscape",
        }
    }
}

/// draw.io's per-page adaptive color mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdaptiveColors {
    /// Keep authored colors, use explicit dark variants when present, and let the
    /// canvas and foreground text adapt. This is the page default.
    #[default]
    Auto,
    /// Keep authored colors except black and white, which flip for contrast.
    Simple,
    /// Always draw the page in its light colors.
    None,
}

impl AdaptiveColors {
    pub const ALL: [AdaptiveColors; 3] = [
        AdaptiveColors::Auto,
        AdaptiveColors::Simple,
        AdaptiveColors::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AdaptiveColors::Auto => "Automatic",
            AdaptiveColors::Simple => "Simple",
            AdaptiveColors::None => "None",
        }
    }

    pub fn is_auto(&self) -> bool {
        *self == AdaptiveColors::Auto
    }
}

/// The appearance a document is being drawn in. Light values stay authoritative;
/// dark values are selected only when the page mode allows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorMode {
    pub adaptive: AdaptiveColors,
    pub dark: bool,
}

impl ColorMode {
    pub fn new(adaptive: AdaptiveColors, dark: bool) -> Self {
        ColorMode { adaptive, dark }
    }

    pub fn light(adaptive: AdaptiveColors) -> Self {
        Self::new(adaptive, false)
    }

    pub fn dark(adaptive: AdaptiveColors) -> Self {
        Self::new(adaptive, true)
    }

    pub fn is_dark(self) -> bool {
        self.dark && self.adaptive != AdaptiveColors::None
    }

    /// Resolve one authored color pair for this appearance.
    pub fn resolve(self, light: Color, dark: Option<Color>) -> Color {
        if !self.is_dark() {
            return light;
        }
        match dark {
            Some(c) => c,
            // Automatic keeps the color the user picked. Text on the dark canvas
            // is lifted separately by the renderer, so a fill never needs to be
            // inverted just to keep a label readable.
            None if self.adaptive == AdaptiveColors::Auto => light,
            // Simple is an explicit opt-in for draw.io's neutral flip.
            None if self.adaptive == AdaptiveColors::Simple && light.is_neutral() => {
                light.inverse()
            }
            None => light,
        }
    }
}

/// Which way a manual guide runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GuideAxis {
    /// A vertical line at a world x.
    Vertical,
    /// A horizontal line at a world y.
    Horizontal,
}

/// A guide dragged out of a ruler. Positions are world coordinates, so guides
/// stay on the same diagram point while the canvas pans and zooms.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ManualGuide {
    pub axis: GuideAxis,
    pub pos: f64,
}

impl ManualGuide {
    pub fn vertical(x: f64) -> Self {
        ManualGuide {
            axis: GuideAxis::Vertical,
            pos: x,
        }
    }

    pub fn horizontal(y: f64) -> Self {
        ManualGuide {
            axis: GuideAxis::Horizontal,
            pos: y,
        }
    }
}

/// A remembered style for shapes or connectors (draw.io's Set as Default Style).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DefaultStyle {
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub text_style: TextStyle,
}

impl DefaultStyle {
    pub fn new(style: Style, text_style: TextStyle) -> Self {
        DefaultStyle { style, text_style }
    }
}

fn one() -> f64 {
    1.0
}
fn is_one(x: &f64) -> bool {
    *x == 1.0
}

/// An image drawn behind the grid on one page (draw.io's Background Image).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackgroundImage {
    /// Content hash of an image registered in [`crate::assets`].
    pub asset: String,
    #[serde(default)]
    pub mode: ImageMode,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiagramSettings {
    pub grid: bool,
    pub grid_size: f64,
    /// None follows the editor theme (light or dark).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid_color: Option<Color>,
    /// Ruler strips along the canvas top and left edges. Manual guides are
    /// dragged out of them.
    pub rulers: bool,
    /// Manual guides dragged from the rulers, in world coordinates.
    pub manual_guides: Vec<ManualGuide>,
    /// Paper preview around the page. This is a view preference like draw.io's
    /// View menu option, so it starts off and is not saved with the document.
    #[serde(skip)]
    pub page_view: bool,
    pub background_enabled: bool,
    pub background_color: Color,
    /// Background used when the page is drawn in dark mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_color_dark: Option<Color>,
    /// Image painted behind the grid and content on this page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<BackgroundImage>,
    /// How fixed and explicit dark colors adapt in dark mode.
    #[serde(default)]
    pub adaptive_colors: AdaptiveColors,
    /// Diagram-wide element shadow default. Explicit per-element choices win.
    pub shadow: bool,
    /// Diagram-wide rough rendering default. Explicit per-element choices win.
    pub sketch: bool,
    /// The last global Rounded action left corners rounded.
    pub rounded: bool,
    /// Selected card in the global scheme grid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    pub connection_arrows: bool,
    pub connection_points: bool,
    pub guides: bool,
    pub autosave: bool,
    pub paper: PaperSize,
    pub orientation: Orientation,
    pub page_transition: PageTransition,
    #[serde(default = "default_transition_ms")]
    pub page_transition_ms: u64,
    pub metadata: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape_default: Option<DefaultStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector_default: Option<DefaultStyle>,
}

impl Default for DiagramSettings {
    fn default() -> Self {
        DiagramSettings {
            grid: true,
            grid_size: 10.0,
            grid_color: None,
            rulers: false,
            manual_guides: Vec::new(),
            page_view: false,
            background_enabled: false,
            background_color: Color::WHITE,
            background_color_dark: None,
            background_image: None,
            adaptive_colors: AdaptiveColors::Auto,
            shadow: false,
            sketch: false,
            rounded: false,
            scheme: None,
            connection_arrows: true,
            connection_points: true,
            guides: true,
            autosave: true,
            paper: PaperSize::Letter,
            orientation: Orientation::Landscape,
            page_transition: PageTransition::None,
            page_transition_ms: default_transition_ms(),
            metadata: BTreeMap::new(),
            shape_default: None,
            connector_default: None,
        }
    }
}

impl DiagramSettings {
    pub fn is_default(&self) -> bool {
        *self == DiagramSettings::default()
    }

    /// Page dimensions in world units (points), respecting orientation.
    pub fn paper_size(&self) -> (f64, f64) {
        let (w, h) = self.paper.portrait_points();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    /// The page rectangle, anchored at the world origin like an artboard.
    pub fn page_rect(&self) -> crate::geom::Rect {
        let (w, h) = self.paper_size();
        crate::geom::Rect::new(0.0, 0.0, w, h)
    }

    /// Fill inherited by elements whose Fill mode is Auto: the active scheme,
    /// then the remembered shape default, then no override.
    pub fn auto_fill(&self) -> Option<Color> {
        self.scheme
            .as_deref()
            .and_then(|name| super::scheme::SCHEMES.iter().find(|s| s.name == name))
            .map(|s| s.fill)
            .or_else(|| self.shape_default.as_ref().and_then(|d| d.style.fill()))
    }

    /// Auto fill resolved for one appearance. Scheme fills have no stored dark
    /// value, so Automatic keeps the scheme's authored color in every theme.
    pub fn auto_fill_with(&self, mode: ColorMode) -> Option<Color> {
        self.scheme
            .as_deref()
            .and_then(|name| super::scheme::SCHEMES.iter().find(|s| s.name == name))
            .map(|s| mode.resolve(s.fill, None))
            .or_else(|| {
                self.shape_default.as_ref().and_then(|d| {
                    let (light, dark) = d.style.fill_pair()?;
                    Some(mode.resolve(light, dark))
                })
            })
    }

    /// Clamp/repair values loaded from a file.
    pub fn validate(&mut self) {
        if !self.grid_size.is_finite() || self.grid_size <= 0.0 {
            self.grid_size = 10.0;
        }
        self.grid_size = self.grid_size.clamp(1.0, 1000.0);
        self.page_transition_ms = self.page_transition_ms.clamp(100, 3000);
        self.manual_guides.retain(|g| g.pos.is_finite());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_dark_keeps_authored_colors() {
        let mode = ColorMode::dark(AdaptiveColors::Auto);
        let green = Color::rgb(0x2e, 0x7d, 0x32);
        let orange = Color::rgb(0xff, 0x98, 0x00);
        assert_eq!(mode.resolve(green, None), green);
        assert_eq!(mode.resolve(orange, None), orange);
        assert_eq!(mode.resolve(Color::WHITE, None), Color::WHITE);
        assert_eq!(
            mode.resolve(green, Some(Color::rgb(0x0a, 0x3d, 0x0e))),
            Color::rgb(0x0a, 0x3d, 0x0e)
        );
    }

    #[test]
    fn background_image_defaults_to_none_and_round_trips() {
        let s = DiagramSettings::default();
        assert_eq!(s.background_image, None);
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("background_image"), "omitted when absent");

        let mut s = s;
        s.background_image = Some(BackgroundImage {
            asset: "abc123".into(),
            mode: ImageMode::Tile,
            opacity: 0.5,
        });
        let json = serde_json::to_string(&s).unwrap();
        let back: DiagramSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.background_image, s.background_image);
    }
}
