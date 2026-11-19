//! Diagram-wide style schemes (draw.io's global Style tab).
//!
//! A scheme is an immutable palette. Applying one changes node colors and page
//! background/grid settings; geometry, labels, ids and connections are left
//! alone. The selected scheme's name is stored in
//! [`DiagramSettings::scheme`](super::settings::DiagramSettings::scheme).

use super::Color;

/// What a scheme command is allowed to change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchemeScope {
    /// Restyle every eligible element on the page and set the page scheme.
    #[default]
    Page,
    /// Restyle the selected subtrees only. Page settings stay untouched.
    Selection,
}

impl SchemeScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::Page => "Page",
            Self::Selection => "Selection",
        }
    }
}

/// Why one element was not restyled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemeSkipReason {
    Locked,
    Hidden,
    OutOfScope,
    Unstyleable,
}

impl SchemeSkipReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Locked => "locked",
            Self::Hidden => "hidden",
            Self::OutOfScope => "out of scope",
            Self::Unstyleable => "no style",
        }
    }
}

/// Counts reported by one scheme command. The same type serves as the preview.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemeReport {
    pub changed: usize,
    pub locked: usize,
    pub hidden: usize,
    pub out_of_scope: usize,
    pub unstyleable: usize,
}

impl SchemeReport {
    pub fn skipped(&self) -> usize {
        self.locked + self.hidden + self.out_of_scope + self.unstyleable
    }

    pub fn summary(&self) -> String {
        let mut parts = vec![format!("{} changed", self.changed)];
        for (count, label) in [
            (self.locked, SchemeSkipReason::Locked.label()),
            (self.hidden, SchemeSkipReason::Hidden.label()),
            (self.out_of_scope, SchemeSkipReason::OutOfScope.label()),
            (self.unstyleable, SchemeSkipReason::Unstyleable.label()),
        ] {
            if count > 0 {
                parts.push(format!("{count} {label} skipped"));
            }
        }
        parts.join(", ")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    pub name: &'static str,
    pub fill: Color,
    pub line: Color,
    /// Label and text colour that stays readable on the fill.
    pub font: Color,
    pub page: Color,
    pub grid: Color,
}

/// Enough schemes for two pages of ten cards in the inspector.
pub const SCHEMES: &[Scheme] = &[
    Scheme {
        name: "Classic",
        fill: Color::rgb(0xff, 0xff, 0xff),
        line: Color::rgb(0x1a, 0x1a, 0x1a),
        font: Color::rgb(0x1a, 0x1a, 0x1a),
        page: Color::rgb(0xff, 0xff, 0xff),
        grid: Color::rgb(0xd0, 0xd0, 0xd0),
    },
    Scheme {
        name: "Peach",
        fill: Color::rgb(0xfc, 0xe6, 0xc9),
        line: Color::rgb(0xd7, 0x9b, 0x00),
        font: Color::rgb(0x1a, 0x1a, 0x1a),
        page: Color::rgb(0xff, 0xfa, 0xf2),
        grid: Color::rgb(0xea, 0xdc, 0xc8),
    },
    Scheme {
        name: "Teal",
        fill: Color::rgb(0x0e, 0x80, 0x88),
        line: Color::rgb(0x00, 0x5c, 0x61),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xf4, 0xfb, 0xfb),
        grid: Color::rgb(0xcf, 0xe7, 0xe8),
    },
    Scheme {
        name: "Navy",
        fill: Color::rgb(0x1f, 0x3a, 0x5f),
        line: Color::rgb(0x0c, 0x22, 0x3d),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xf6, 0xf8, 0xfb),
        grid: Color::rgb(0xd5, 0xdd, 0xe8),
    },
    Scheme {
        name: "Steel",
        fill: Color::rgb(0x94, 0xb0, 0xc8),
        line: Color::rgb(0x5a, 0x82, 0xa2),
        font: Color::rgb(0x1c, 0x2a, 0x33),
        page: Color::rgb(0xf6, 0xf9, 0xfb),
        grid: Color::rgb(0xd8, 0xe2, 0xea),
    },
    Scheme {
        name: "Sage",
        fill: Color::rgb(0xc9, 0xd8, 0xc3),
        line: Color::rgb(0x4d, 0x6b, 0x50),
        font: Color::rgb(0x24, 0x35, 0x2a),
        page: Color::rgb(0xf8, 0xfb, 0xf7),
        grid: Color::rgb(0xdb, 0xe5, 0xd8),
    },
    Scheme {
        name: "Sunset",
        fill: Color::rgb(0xff, 0xd9, 0x66),
        line: Color::rgb(0xe9, 0x4f, 0xa8),
        font: Color::rgb(0x1a, 0x1a, 0x1a),
        page: Color::rgb(0xff, 0xfd, 0xf5),
        grid: Color::rgb(0xf0, 0xe6, 0xc8),
    },
    Scheme {
        name: "Coral",
        fill: Color::rgb(0xe9, 0x5c, 0x5c),
        line: Color::rgb(0x2f, 0x9d, 0x8f),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xff, 0xf7, 0xf7),
        grid: Color::rgb(0xef, 0xd9, 0xd9),
    },
    Scheme {
        name: "Amber",
        fill: Color::rgb(0xd7, 0x9b, 0x00),
        line: Color::rgb(0x1a, 0x1a, 0x1a),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xff, 0xfa, 0xf0),
        grid: Color::rgb(0xea, 0xdf, 0xce),
    },
    Scheme {
        name: "Cream",
        fill: Color::rgb(0xff, 0xf2, 0xcc),
        line: Color::rgb(0xb4, 0x1e, 0x2e),
        font: Color::rgb(0x1a, 0x1a, 0x1a),
        page: Color::rgb(0xff, 0xfd, 0xf6),
        grid: Color::rgb(0xee, 0xe0, 0xcc),
    },
    Scheme {
        name: "Sky",
        fill: Color::rgb(0xa8, 0xd8, 0xff),
        line: Color::rgb(0x2b, 0x6f, 0xd8),
        font: Color::rgb(0x10, 0x28, 0x48),
        page: Color::rgb(0xf5, 0xfa, 0xff),
        grid: Color::rgb(0xd7, 0xe7, 0xf7),
    },
    Scheme {
        name: "Lavender",
        fill: Color::rgb(0xd7, 0xc2, 0xff),
        line: Color::rgb(0x7b, 0x3c, 0xd6),
        font: Color::rgb(0x24, 0x14, 0x3d),
        page: Color::rgb(0xfb, 0xf8, 0xff),
        grid: Color::rgb(0xe7, 0xdd, 0xf5),
    },
    Scheme {
        name: "Mint",
        fill: Color::rgb(0xb5, 0xe6, 0xa2),
        line: Color::rgb(0x2e, 0x9e, 0x4a),
        font: Color::rgb(0x13, 0x2b, 0x18),
        page: Color::rgb(0xf7, 0xfd, 0xf5),
        grid: Color::rgb(0xdb, 0xef, 0xd5),
    },
    Scheme {
        name: "Blush",
        fill: Color::rgb(0xff, 0xb8, 0xa8),
        line: Color::rgb(0xd6, 0x3f, 0x9e),
        font: Color::rgb(0x38, 0x17, 0x22),
        page: Color::rgb(0xff, 0xf8, 0xf6),
        grid: Color::rgb(0xf2, 0xdd, 0xd8),
    },
    Scheme {
        name: "Sand",
        fill: Color::rgb(0xf5, 0xb7, 0x6b),
        line: Color::rgb(0x8a, 0x4f, 0x12),
        font: Color::rgb(0x3a, 0x21, 0x07),
        page: Color::rgb(0xff, 0xfb, 0xf5),
        grid: Color::rgb(0xef, 0xe1, 0xcf),
    },
    Scheme {
        name: "Forest",
        fill: Color::rgb(0x1e, 0x5b, 0x3a),
        line: Color::rgb(0x97, 0xd0, 0x2b),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xf5, 0xfa, 0xf6),
        grid: Color::rgb(0xd6, 0xe8, 0xda),
    },
    Scheme {
        name: "Plum",
        fill: Color::rgb(0x6b, 0x2d, 0x6b),
        line: Color::rgb(0xff, 0xb8, 0xa8),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xfb, 0xf6, 0xfb),
        grid: Color::rgb(0xeb, 0xda, 0xeb),
    },
    Scheme {
        name: "Ocean",
        fill: Color::rgb(0x2b, 0x6f, 0xd8),
        line: Color::rgb(0x14, 0x3c, 0x7a),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xf4, 0xf9, 0xff),
        grid: Color::rgb(0xd4, 0xe3, 0xf4),
    },
    Scheme {
        name: "Slate",
        fill: Color::rgb(0x2a, 0x2f, 0x3a),
        line: Color::rgb(0x8a, 0x92, 0xa0),
        font: Color::rgb(0xff, 0xff, 0xff),
        page: Color::rgb(0xf6, 0xf7, 0xf9),
        grid: Color::rgb(0xdb, 0xdf, 0xe4),
    },
    Scheme {
        name: "Paper",
        fill: Color::rgb(0xfb, 0xf7, 0xee),
        line: Color::rgb(0x5b, 0x4a, 0x36),
        font: Color::rgb(0x2a, 0x22, 0x18),
        page: Color::rgb(0xfd, 0xfa, 0xf2),
        grid: Color::rgb(0xeb, 0xe3, 0xd3),
    },
];

impl Scheme {
    /// Scheme names in the order the inspector shows them.
    pub fn all() -> &'static [Scheme] {
        SCHEMES
    }
}
