//! Page-scoped layers (draw.io's layer list), per ADR-009.
//!
//! Layers are membership records, separate from the z-order tree: every node
//! has an optional layer id (children inherit their nearest ancestor's layer),
//! paint order runs layer by layer, and visibility and lock compose with the
//! per-node flags. Z order stays the tree's job.

use serde::{Deserialize, Serialize};

/// Stable identity of a layer inside its page. Ids never repeat or get reused,
/// even after deletes, so saved files keep their memberships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LayerId(pub u64);

impl std::fmt::Display for LayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    #[serde(default = "yes")]
    pub visible: bool,
    #[serde(default)]
    pub locked: bool,
}

fn yes() -> bool {
    true
}

impl Layer {
    /// The default bottom layer every page starts with. All pre-layer content
    /// migrates onto it through the serde default (missing layer lists in old
    /// files) and the inheritance default (missing layer ids on nodes).
    pub fn background() -> Layer {
        Layer {
            id: LayerId(1),
            name: "Background".into(),
            visible: true,
            locked: false,
        }
    }
}
