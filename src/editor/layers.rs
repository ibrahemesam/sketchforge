//! Page layers (draw.io's layer list), per ADR-009.
//!
//! Layers are page-scoped membership records, separate from the z-order tree the
//! Layers panel shows. Every op here is one undoable transaction recorded as
//! node state plus a layer-list snapshot. Membership resolves through the
//! nearest ancestor, so a move writes the layer id across whole top-level
//! subtrees: one subtree always paints on one layer.

use super::Editor;
use super::insert_subtrees_tx;
use crate::geom::V2;
use crate::model::{Layer, LayerId, Node, NodeId};

impl Editor {
    /// A new layer on top of the stack.
    pub fn layer_add(&mut self) -> LayerId {
        self.edit("Add layer", |tx| {
            let id = tx.doc.alloc_layer_id();
            let name = format!("Layer {}", tx.doc.layers.len() + 1);
            tx.layers_mut().push(Layer {
                id,
                name,
                visible: true,
                locked: false,
            });
            id
        })
    }

    pub fn layer_rename(&mut self, id: LayerId, name: impl Into<String>) {
        let name = name.into();
        self.edit("Rename layer", |tx| {
            if let Some(r) = tx.layers_mut().iter_mut().find(|l| l.id == id) {
                r.name = name;
            }
        });
    }

    /// A copy of `id` sitting right above it, with a copy of every shape on it.
    pub fn layer_duplicate(&mut self, id: LayerId) -> Option<LayerId> {
        self.edit("Duplicate layer", move |tx| {
            let src = tx.doc.layer_by_id(id).cloned()?;
            let new_id = tx.doc.alloc_layer_id();
            // Snapshots first, so the tx borrows stay independent.
            let members: Vec<Node> = tx
                .doc
                .layered_root()
                .into_iter()
                .filter_map(|rid| {
                    if tx.doc.effective_layer(rid) == id {
                        tx.doc.get(rid).cloned()
                    } else {
                        None
                    }
                })
                .collect();
            let roots: Vec<NodeId> = members.iter().map(|n| n.id).collect();
            let worlds: Vec<crate::geom::Rect> = roots
                .iter()
                .filter_map(|rid| tx.doc.world_rect(*rid))
                .collect();
            let at = tx.doc.layers.iter().position(|l| l.id == id)? + 1;
            let fresh: Vec<NodeId> = insert_subtrees_tx(
                tx,
                &members.clone(),
                &roots.clone(),
                &worlds,
                V2::ZERO,
                true,
            );
            tx.layers_mut().insert(
                at,
                Layer {
                    id: new_id,
                    name: format!("Copy of {}", src.name),
                    visible: src.visible,
                    locked: false,
                },
            );
            for top in fresh {
                for sid in tx.doc.subtree(top) {
                    if let Some(n) = tx.node_mut(sid) {
                        n.layer = Some(new_id);
                    }
                }
            }
            Some(new_id)
        })
    }

    /// Delete a layer together with every shape on it (draw.io's behavior).
    /// The last layer cannot be deleted; its shapes stay.
    pub fn layer_delete(&mut self, id: LayerId) {
        self.edit("Delete layer", move |tx| {
            if tx.doc.layers.len() <= 1 {
                return;
            }
            let doomed: Vec<NodeId> = tx
                .doc
                .layered_root()
                .into_iter()
                .filter(|rid| tx.doc.effective_layer(*rid) == id)
                .collect();
            for rid in &doomed {
                tx.remove(*rid);
            }
            tx.layers_mut().retain(|l| l.id != id);
        });
    }

    /// Move a layer record so it paints at 0-based bottom-first position
    /// `index` (clamped to the stack).
    pub fn layer_reorder(&mut self, id: LayerId, index: usize) {
        self.edit("Reorder layers", move |tx| {
            let dst = index.min(tx.doc.layers.len() - 1);
            if let Some((from, _)) = tx.doc.layers.iter().enumerate().find(|(_, l)| l.id == id)
                && from != dst
            {
                let layers = tx.layers_mut();
                let layer = layers.remove(from);
                layers.insert(dst, layer);
            }
        });
    }

    /// Show or hide every shape on a layer without touching the shapes.
    pub fn layer_set_visible(&mut self, id: LayerId, visible: bool) {
        self.edit(
            if visible { "Show layer" } else { "Hide layer" },
            move |tx| {
                if let Some(r) = tx.layers_mut().iter_mut().find(|l| l.id == id) {
                    r.visible = visible;
                }
            },
        );
    }

    /// Lock or unlock every shape on a layer without touching the shapes.
    pub fn layer_set_locked(&mut self, id: LayerId, locked: bool) {
        self.edit(
            if locked { "Lock layer" } else { "Unlock layer" },
            move |tx| {
                if let Some(r) = tx.layers_mut().iter_mut().find(|l| l.id == id) {
                    r.locked = locked;
                }
            },
        );
    }

    /// Move the selection to a layer. Membership resolves through ancestors,
    /// so a selection moves whole top-level subtrees: moving a shape inside a
    /// group moves the whole group (a child cannot sit on a different layer
    /// from its parent).
    pub fn move_selection_to_layer(&mut self, ids: &[NodeId], layer: LayerId) {
        self.edit("Move to layer", move |tx| {
            if !tx.doc.layers.iter().any(|l| l.id == layer) {
                return;
            }
            // The top-level ancestor of each selection (or the node itself).
            let mut roots: Vec<NodeId> = Vec::new();
            for id in ids {
                let mut top = *id;
                while let Some(p) = tx.doc.get(top).and_then(|n| n.parent) {
                    top = p;
                }
                if !roots.contains(&top) {
                    roots.push(top);
                }
            }
            for top in roots {
                if !tx.doc.nodes.contains_key(&top) {
                    continue;
                }
                for sid in tx.doc.subtree(top) {
                    if let Some(n) = tx.node_mut(sid) {
                        n.layer = Some(layer);
                    }
                }
            }
        });
    }
}
