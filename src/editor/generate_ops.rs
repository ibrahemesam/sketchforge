//! Insertion of generated diagrams (M8). Parsers produce an ordinary document;
//! this turns it into either content on the open page, in one undo step, or a
//! new project page. Nothing here special-cases generated nodes, so selection,
//! styling, connectors, layers, autosave, and export all behave as usual.

use super::Editor;
use crate::generate::Generated;
use crate::geom::{Rect, V2};
use crate::model::{Node, NodeId};
use crate::project::PageData;

#[derive(Debug, Clone, Default)]
pub struct GeneratedInsertion {
    pub ids: Vec<NodeId>,
    /// First new page index; None for in-page insertion.
    pub page: Option<usize>,
}

impl Editor {
    /// Insert a generated diagram into the open page, centered on `at` (or the
    /// viewport center), or append it as a new page.
    pub fn insert_generated(
        &mut self,
        generated: &Generated,
        at: Option<V2>,
        as_page: bool,
    ) -> GeneratedInsertion {
        let warnings = generated.report.summary();
        if as_page {
            let first = self.pages.len();
            self.append_pages(vec![PageData {
                id: crate::project::format::new_id(),
                name: generated.name.clone(),
                doc: generated.doc.clone(),
                camera: None,
            }]);
            self.status = if warnings.is_empty() {
                format!("Added {} as a page", generated.name)
            } else {
                format!("Added {} as a page; {warnings}", generated.name)
            };
            // `append_pages` switches to the new page, so its roots are the
            // content that just arrived.
            return GeneratedInsertion {
                ids: self.doc.root.clone(),
                page: Some(first),
            };
        }

        let nodes: Vec<Node> = generated.doc.nodes.values().cloned().collect();
        let roots = generated.doc.root.clone();
        let world: Vec<Rect> = roots
            .iter()
            .map(|id| generated.doc.world_rect(*id).unwrap_or_default())
            .collect();
        let bounds = Rect::union_all(&world).unwrap_or_default();
        let at = at.unwrap_or_else(|| self.camera.screen_to_world(self.viewport * 0.5));
        let offset = (at - bounds.center()).round();
        let ids = self.insert_subtrees(
            &format!("Insert {}", generated.name),
            &nodes,
            &roots,
            &world,
            offset,
            true,
        );
        self.selection = ids.clone();
        self.status = if warnings.is_empty() {
            format!("Inserted {}", generated.name)
        } else {
            format!("Inserted {}; {warnings}", generated.name)
        };
        GeneratedInsertion { ids, page: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::{Options, Source, generate};

    fn diagram() -> Generated {
        generate(
            Source::Mermaid,
            "flowchart TD\n A[Start] --> B[Done]\n",
            &Options::default(),
        )
        .unwrap()
    }

    #[test]
    fn in_page_insertion_is_one_undo_step_and_keeps_edges_bound() {
        let mut ed = Editor::default();
        let g = diagram();
        let out = ed.insert_generated(&g, Some(V2::ZERO), false);
        assert!(out.page.is_none());
        assert!(!out.ids.is_empty());
        assert_eq!(ed.selection, out.ids);
        assert!(ed.is_dirty());
        assert_eq!(
            ed.history.undo_labels().last(),
            Some("Insert Mermaid diagram")
        );
        let mut bound = Vec::new();
        for node in ed.doc.nodes.values() {
            if let crate::model::NodeKind::Connector { start, end, .. } = &node.kind {
                bound.extend(start.as_ref().map(|b| b.node));
                bound.extend(end.as_ref().map(|b| b.node));
            }
        }
        assert_eq!(bound.len(), 2);
        for id in bound {
            assert!(ed.doc.nodes.contains_key(&id));
        }
        let before = ed.doc.nodes.len();
        ed.undo();
        assert_eq!(ed.doc.nodes.len(), before - g.doc.nodes.len());
    }

    #[test]
    fn page_insertion_appends_switches_and_leaves_the_first_page_alone() {
        let mut ed = Editor::default();
        let g = diagram();
        let out = ed.insert_generated(&g, None, true);
        assert_eq!(out.page, Some(1));
        assert_eq!(out.ids, g.doc.root);
        assert_eq!(ed.pages.len(), 2);
        assert_eq!(ed.active_page, 1);
        assert_eq!(ed.doc.nodes.len(), g.doc.nodes.len());
        ed.switch_page(0);
        assert!(ed.doc.nodes.is_empty());
        assert!(ed.status.contains("as a page"));
    }

    #[test]
    fn warnings_are_carried_into_the_status_line() {
        let mut ed = Editor::default();
        let g = generate(
            Source::Csv,
            "id,parent\n1,nope\n",
            &Options {
                layout: crate::generate::Layout::VerticalTree,
                header: true,
                label_column: None,
            },
        )
        .unwrap();
        assert!(!g.report.warnings.is_empty());
        ed.insert_generated(&g, Some(V2::ZERO), false);
        assert!(ed.status.contains("not in the CSV"), "{}", ed.status);
    }
}
