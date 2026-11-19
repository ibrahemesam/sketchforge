//! Registry insertion (ADR-010): turn a shape definition into a regular node.
//! The shape joins the frame under the drop point (like tool-drawn shapes) and,
//! like them, takes the page's default style when the definition is marked
//! `plain`, so a searched shape matches the current diagram.

use super::Editor;
use crate::geom::{Rect, V2};
use crate::model::{LayerId, NodeId};

impl Editor {
    /// Insert a registry shape centered at world `at` (defaults to the viewport
    /// center). `size` overrides the entry's preferred size. `parent` places
    /// the shape in a container directly (None = the frame at `at`, like
    /// tool-drawn shapes). `layer` assigns the whole subtree to a page layer,
    /// so insertion stays layer-aware like `create_shape`. Returns the new
    /// node.
    pub fn insert_registry_shape(
        &mut self,
        id_or_name: &str,
        at: Option<V2>,
        size: Option<(f64, f64)>,
        parent: Option<NodeId>,
        layer: Option<LayerId>,
    ) -> Option<NodeId> {
        let def = crate::registry::by_id(id_or_name)?;
        let (w, h) = size.unwrap_or(def.size);
        let at = at.unwrap_or_else(|| self.camera.screen_to_world(self.viewport * 0.5));
        let parent = parent.or_else(|| self.frame_at(at, &[]));
        let origin = self.doc.parent_origin(parent);
        let rect = Rect::new(at.x - w * 0.5 - origin.x, at.y - h * 0.5 - origin.y, w, h);
        let default_style = self.doc.settings.shape_default.clone();
        self.begin(&format!("Insert {}", def.name));
        let id = {
            let mut tx = self.tx();
            let mut n = crate::registry::build(def, rect, default_style.as_ref());
            n.id = tx.alloc_id();
            let id = tx.insert(n, parent, None);
            if let Some(l) = layer {
                for sid in tx.doc.subtree(id) {
                    if let Some(m) = tx.node_mut(sid) {
                        m.layer = Some(l);
                    }
                }
            }
            id
        };
        self.fit_text(id);
        self.commit();
        self.select(vec![id]);
        Some(id)
    }
}
