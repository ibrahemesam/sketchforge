//! Template insertion (M7). A template page becomes either editable content on
//! the open page or a set of new project pages. Both paths reuse the existing
//! library and page machinery, so undo, selection, layers, and autosave behave
//! exactly like hand-drawn content.

use super::Editor;
use crate::geom::V2;
use crate::model::{LayerId, NodeId};
use crate::template::Template;
use std::collections::HashMap;

/// What an insertion produced and which registry shapes it could not build.
#[derive(Debug, Clone, Default)]
pub struct TemplateInsertion {
    pub ids: Vec<NodeId>,
    pub missing: Vec<String>,
    /// First new page index for page insertion; None for in-page insertion.
    pub page: Option<usize>,
}

impl Editor {
    /// Insert one template page into the open page, centered on `at` (or the
    /// viewport center). Returns the new root ids, or nothing for a bad index.
    pub fn insert_template_page(
        &mut self,
        template: &Template,
        page: usize,
        at: Option<V2>,
    ) -> Option<TemplateInsertion> {
        let (clean, missing) = template.page(page)?;
        let layers = self.map_template_layers(&clean.layers);
        let item = clean.to_item(&template.name, &layers);
        let at = at.unwrap_or_else(|| self.camera.screen_to_world(self.viewport * 0.5));
        let ids = self.insert_library_item(&item, at);
        self.status = if missing.is_empty() {
            format!("Inserted {}", template.name)
        } else {
            format!(
                "Inserted {}; skipped missing shape(s) {}",
                template.name,
                missing.join(", ")
            )
        };
        Some(TemplateInsertion {
            ids,
            missing,
            page: None,
        })
    }

    /// Add every page of a template as new project pages and switch to the
    /// first one. Existing pages stay untouched.
    pub fn insert_template_pages(&mut self, template: &Template) -> TemplateInsertion {
        let (pages, missing) = template.page_data();
        if pages.is_empty() {
            return TemplateInsertion {
                missing,
                ..Default::default()
            };
        }
        let first = self.pages.len();
        self.append_pages(pages);
        self.status = if missing.is_empty() {
            format!(
                "Added {} page(s) from {}",
                template.pages.len(),
                template.name
            )
        } else {
            format!(
                "Added {} page(s) from {}; skipped missing shape(s) {}",
                template.pages.len(),
                template.name,
                missing.join(", ")
            )
        };
        TemplateInsertion {
            ids: self.selection.clone(),
            missing,
            page: Some(first),
        }
    }

    /// Map template layers onto the open page: a layer keeps its page when the
    /// names match, and everything else lands on the page background. This is
    /// the same "nearest sensible place" rule a pasted item uses.
    fn map_template_layers(
        &self,
        layers: &[crate::model::Layer],
    ) -> HashMap<LayerId, Option<LayerId>> {
        let mut map = HashMap::new();
        for layer in layers {
            let target = self
                .doc
                .layers
                .iter()
                .find(|l| l.name.trim().eq_ignore_ascii_case(layer.name.trim()))
                .map(|l| l.id);
            map.insert(layer.id, target);
        }
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::packs;

    fn template(id: &str) -> Template {
        packs::all().into_iter().find(|t| t.id == id).unwrap()
    }

    #[test]
    fn inserting_a_page_is_one_undo_step_and_selects_the_content() {
        let mut ed = Editor::default();
        let before = ed.doc.nodes.len();
        let t = template("flowchart");
        let out = ed
            .insert_template_page(&t, 0, Some(V2::ZERO))
            .expect("page exists");
        assert!(out.missing.is_empty());
        assert!(!out.ids.is_empty());
        assert_eq!(ed.selection, out.ids);
        assert!(ed.doc.nodes.len() > before);
        assert!(ed.is_dirty());
        assert_eq!(ed.history.undo_labels().last(), Some("Insert Order flow"));
        ed.undo();
        assert_eq!(ed.doc.nodes.len(), before);
        assert!(ed.doc.root.is_empty());
    }

    #[test]
    fn insertion_keeps_connectors_bound_to_the_new_nodes() {
        let mut ed = Editor::default();
        let t = template("org-chart");
        ed.insert_template_page(&t, 0, Some(V2::ZERO)).unwrap();
        let mut bound = Vec::new();
        for n in ed.doc.nodes.values() {
            if let crate::model::NodeKind::Connector { start, end, .. } = &n.kind {
                bound.extend(start.as_ref().map(|b| b.node));
                bound.extend(end.as_ref().map(|b| b.node));
            }
        }
        assert!(!bound.is_empty());
        for id in bound {
            assert!(
                ed.doc.nodes.contains_key(&id),
                "a bound connector points at a node outside the insertion"
            );
        }
    }

    #[test]
    fn template_pages_arrive_with_layers_and_names() {
        let mut ed = Editor::default();
        let t = template("project-plan");
        let out = ed.insert_template_pages(&t);
        assert!(out.missing.is_empty());
        assert_eq!(out.page, Some(1));
        assert_eq!(ed.pages.len(), 4);
        assert_eq!(ed.active_page, 1);
        assert_eq!(ed.doc.layers.len(), 1);
        for i in 2..4 {
            assert_eq!(ed.pages[i].stash.as_ref().unwrap().doc.layers.len(), 1);
        }
        assert!(ed.pages[1].name.starts_with("Project plan"));
    }

    #[test]
    fn a_missing_shape_is_reported_without_losing_the_rest() {
        let mut ed = Editor::default();
        let mut t = template("flowchart");
        let before = t.pages[0].nodes.len();
        t.pages[0].nodes[0].shape = Some("retired-shape".into());
        let out = ed.insert_template_page(&t, 0, Some(V2::ZERO)).unwrap();
        assert_eq!(out.missing, vec!["retired-shape".to_string()]);
        assert!(out.ids.len() > 1);
        assert_eq!(ed.doc.nodes.len(), before - 1);
        assert!(ed.status.contains("retired-shape"));
    }

    #[test]
    fn a_layer_with_the_same_name_keeps_its_page_layer() {
        let mut ed = Editor::default();
        let extra = ed.layer_add();
        ed.layer_rename(extra, "Notes");
        let mut t = template("flowchart");
        t.pages[0].layers.push(crate::model::Layer {
            id: crate::model::LayerId(2),
            name: "Notes".into(),
            visible: true,
            locked: false,
        });
        for node in &mut t.pages[0].nodes {
            node.node.layer = Some(crate::model::LayerId(2));
        }
        ed.insert_template_page(&t, 0, Some(V2::ZERO)).unwrap();
        assert!(
            ed.doc.layers.iter().any(|l| l.id == extra),
            "the existing layer was kept"
        );
        assert!(
            ed.doc.nodes.values().all(|n| n.layer == Some(extra)),
            "every inserted node joined the named layer"
        );
    }
}
