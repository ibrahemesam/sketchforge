//! Runtime for draw.io custom-link actions (DIA-12).
//!
//! Only the safe subset parsed by [`crate::link::action`] reaches here. Layer
//! and node visibility use undoable document edits. Select changes the editor
//! selection and is not part of document history.

use crate::editor::Editor;
use crate::link::action::{Action, TagsMatch};
use crate::model::{Document, LayerId, NodeId};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    Toggle,
    Show,
    Hide,
    Select,
}

struct Resolved {
    effect: Effect,
    cells: Vec<NodeId>,
    layers: Vec<LayerId>,
    layer_cells: Vec<NodeId>,
}

/// Run an already-parsed action link. Returns false for any other link kind.
pub(crate) fn run(ed: &mut Editor, link: &str) -> bool {
    let crate::link::LinkTarget::Actions(actions) = crate::link::classify(link) else {
        return false;
    };
    let resolved: Vec<Resolved> = actions.iter().map(|a| resolve(&ed.doc, a)).collect();
    let mut selected = None;

    ed.edit("Run link actions", |tx| {
        for action in &resolved {
            if action.effect == Effect::Select {
                let mut ids = action.cells.clone();
                ids.extend(action.layer_cells.iter().copied());
                selected = Some(ids);
                continue;
            }
            let show = match action.effect {
                Effect::Show => true,
                Effect::Hide => false,
                Effect::Toggle => {
                    let hidden_layer = action
                        .layers
                        .iter()
                        .any(|id| tx.doc.layer_by_id(*id).is_some_and(|layer| !layer.visible));
                    let hidden_node = action
                        .cells
                        .iter()
                        .any(|id| tx.doc.get(*id).is_some_and(|n| !n.visible));
                    hidden_layer || hidden_node
                }
                Effect::Select => unreachable!(),
            };
            if !action.layers.is_empty() {
                let layers = tx.layers_mut();
                for layer in layers.iter_mut().filter(|l| action.layers.contains(&l.id)) {
                    layer.visible = show;
                }
            }
            for id in &action.cells {
                if let Some(node) = tx.node_mut(*id) {
                    node.visible = show;
                }
            }
        }
    });

    if let Some(ids) = selected {
        ed.select(ids);
    }
    true
}

fn resolve(doc: &Document, action: &Action) -> Resolved {
    let (effect, targets) = match action {
        Action::Toggle(t) => (Effect::Toggle, t),
        Action::Show(t) => (Effect::Show, t),
        Action::Hide(t) => (Effect::Hide, t),
        Action::Select(t) => (Effect::Select, t),
    };
    let mut cells = BTreeSet::new();
    let mut layers = BTreeSet::new();
    let mut layer_cells = BTreeSet::new();

    for id in &targets.cells {
        if id == "*" {
            cells.extend(doc.nodes.keys().copied());
        } else if let Some(id) = node_id(id)
            && doc.nodes.contains_key(&id)
        {
            cells.insert(id);
        }
    }
    for id in &targets.layers {
        let Some(layer) = layer_id(id) else { continue };
        if doc.layer_by_id(layer).is_none() {
            continue;
        }
        layers.insert(layer);
        layer_cells.extend(
            doc.nodes
                .keys()
                .copied()
                .filter(|id| doc.effective_layer(*id) == layer),
        );
    }
    if targets.all_tags {
        cells.extend(
            doc.nodes
                .values()
                .filter(|n| !n.tags.is_empty())
                .map(|n| n.id),
        );
    }
    for tag in &targets.tags {
        cells.extend(
            doc.nodes
                .values()
                .filter(|n| n.tags.contains(tag))
                .map(|n| n.id),
        );
    }
    if targets.tags_match == TagsMatch::And && targets.tags.len() > 1 {
        cells.retain(|id| {
            doc.get(*id)
                .is_some_and(|n| targets.tags.iter().all(|tag| n.tags.contains(tag)))
        });
    }
    for id in &targets.exclude_cells {
        if id == "*" {
            cells.clear();
            layer_cells.clear();
        } else if let Some(id) = node_id(id) {
            cells.remove(&id);
            layer_cells.remove(&id);
        }
    }

    Resolved {
        effect,
        cells: cells.into_iter().collect(),
        layers: layers.into_iter().collect(),
        layer_cells: layer_cells.into_iter().collect(),
    }
}

fn node_id(value: &str) -> Option<NodeId> {
    value
        .strip_prefix('n')
        .unwrap_or(value)
        .parse::<u64>()
        .ok()
        .map(NodeId)
}

fn layer_id(value: &str) -> Option<LayerId> {
    value
        .strip_prefix('l')
        .unwrap_or(value)
        .parse::<u64>()
        .ok()
        .map(LayerId)
}
