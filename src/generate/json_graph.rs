//! JSON graph import. Accepts `{"nodes": [...], "edges": [...]}` with optional
//! per-node coordinates, sizes, shapes, and colours. Coordinates are kept only
//! when every node has them; otherwise the graph is laid out deterministically.
//! Unknown keys are ignored so exported data can carry extra fields.

use super::{
    BLUE, Builder, Dir, Generated, Layout, Report, grid_positions, label_size, layered_positions,
    rect_at,
};
use crate::geom::V2;
use crate::model::{Color, Head, NodeId};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct JNode {
    label: String,
    shape: String,
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
    fill: Option<Color>,
    stroke: Option<Color>,
}

#[derive(Debug, Clone)]
struct JEdge {
    from: usize,
    to: usize,
    label: String,
    dashed: bool,
    arrow: bool,
}

pub fn parse(text: &str, layout: Layout) -> Result<Generated, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        format!(
            "invalid JSON at line {} column {}: {}",
            e.line(),
            e.column(),
            e
        )
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| "the JSON must be an object with a nodes array".to_string())?;
    let raw_nodes = object
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| "the JSON needs a nodes array".to_string())?;
    if raw_nodes.is_empty() {
        return Err("the graph has no nodes".into());
    }

    let mut report = Report::default();
    let mut nodes: Vec<JNode> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, value) in raw_nodes.iter().enumerate() {
        let Some(obj) = value.as_object() else {
            report.warn(format!("node {} is not an object and was skipped", i + 1));
            continue;
        };
        let id = match obj.get("id") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(Value::Number(n)) => n.to_string(),
            _ => {
                report.warn(format!("node {} has no id and was skipped", i + 1));
                continue;
            }
        };
        if index.contains_key(&id) {
            report.warn(format!("duplicate node id \"{id}\" keeps the first one"));
            continue;
        }
        let label = obj
            .get("label")
            .or_else(|| obj.get("text"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(&id)
            .to_string();
        let num = |key: &str| obj.get(key).and_then(Value::as_f64);
        let color = |key: &str| {
            obj.get(key)
                .and_then(Value::as_str)
                .and_then(crate::io::excalidraw::css_color)
        };
        index.insert(id, nodes.len());
        nodes.push(JNode {
            label,
            shape: obj
                .get("shape")
                .and_then(Value::as_str)
                .unwrap_or("rect")
                .to_string(),
            x: num("x"),
            y: num("y"),
            w: num("width"),
            h: num("height"),
            fill: color("fill").or(Some(BLUE)),
            stroke: color("stroke"),
        });
    }
    if nodes.is_empty() {
        return Err("the graph has no usable nodes".into());
    }

    let mut edges: Vec<JEdge> = Vec::new();
    if let Some(raw_edges) = object.get("edges") {
        let list = raw_edges
            .as_array()
            .ok_or_else(|| "the JSON edges value must be an array".to_string())?;
        for (i, value) in list.iter().enumerate() {
            let Some(obj) = value.as_object() else {
                report.warn(format!("edge {} is not an object and was skipped", i + 1));
                continue;
            };
            let from_id = obj
                .get("from")
                .or_else(|| obj.get("source"))
                .and_then(Value::as_str);
            let to_id = obj
                .get("to")
                .or_else(|| obj.get("target"))
                .and_then(Value::as_str);
            let (Some(from_id), Some(to_id)) = (from_id, to_id) else {
                report.warn(format!("edge {} needs from and to ids", i + 1));
                continue;
            };
            let (Some(&from), Some(&to)) = (index.get(from_id), index.get(to_id)) else {
                report.warn(format!(
                    "edge {} connects \"{from_id}\" to \"{to_id}\", which is not a node",
                    i + 1
                ));
                continue;
            };
            edges.push(JEdge {
                from,
                to,
                label: obj
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                dashed: obj.get("dashed").and_then(Value::as_bool).unwrap_or(false),
                arrow: obj.get("arrow").and_then(Value::as_bool).unwrap_or(true),
            });
        }
    }

    let coords_known = nodes.iter().all(|n| n.x.is_some() && n.y.is_some());
    let use_coords = layout == Layout::Auto && coords_known;
    let sizes: Vec<(f64, f64)> = nodes
        .iter()
        .map(|n| {
            if use_coords {
                (
                    n.w.unwrap_or(140.0).clamp(40.0, 600.0),
                    n.h.unwrap_or(56.0).clamp(24.0, 400.0),
                )
            } else {
                label_size(&n.label, 140.0, 50.0)
            }
        })
        .collect();
    let dir = match layout {
        Layout::HorizontalFlow | Layout::HorizontalTree => Dir::Right,
        Layout::VerticalTree => Dir::Down,
        _ => Dir::Down,
    };
    let pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let pos: Vec<V2> = if use_coords {
        let min_x = nodes
            .iter()
            .map(|n| n.x.unwrap_or(0.0))
            .fold(f64::INFINITY, f64::min);
        let min_y = nodes
            .iter()
            .map(|n| n.y.unwrap_or(0.0))
            .fold(f64::INFINITY, f64::min);
        nodes
            .iter()
            .map(|n| crate::geom::v2(n.x.unwrap_or(0.0) - min_x, n.y.unwrap_or(0.0) - min_y))
            .collect()
    } else if layout == Layout::Grid {
        grid_positions(&sizes)
    } else {
        let pairs = if pairs.is_empty() {
            super::chain_edges(nodes.len())
        } else {
            pairs
        };
        let (p, cycle) = layered_positions(&sizes, &pairs, dir);
        if let Some(w) = cycle {
            report.warn(w);
        }
        p
    };

    let mut b = Builder::new("JSON graph");
    let ids: Vec<NodeId> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let rect = rect_at(pos[i], sizes[i]);
            let shape = n.shape.to_ascii_lowercase();
            let id = match shape.as_str() {
                "ellipse" | "circle" => b.ellipse(rect, &n.label, n.fill),
                "diamond" | "decision" => b.polygon(rect, &n.label, 4, n.fill),
                "hexagon" => b.polygon(rect, &n.label, 6, n.fill),
                "rounded" | "round" | "stadium" => b.rect(rect, &n.label, n.fill, 14.0, None),
                _ => b.rect(rect, &n.label, n.fill, 4.0, None),
            };
            if let Some(stroke) = n.stroke {
                b.set_style(id, n.fill, Some(stroke));
            }
            id
        })
        .collect();
    for edge in &edges {
        if edge.from == edge.to {
            continue;
        }
        let a = b.world_rect(ids[edge.from]).unwrap_or_default();
        let z = b.world_rect(ids[edge.to]).unwrap_or_default();
        let (hf, ht) = facing(a, z);
        b.link(
            (ids[edge.from], hf),
            (ids[edge.to], ht),
            &edge.label,
            edge.dashed,
            (
                Head::None,
                if edge.arrow { Head::Arrow } else { Head::None },
            ),
        );
    }
    b.finish("JSON graph", report)
}

fn facing(a: crate::geom::Rect, b: crate::geom::Rect) -> (V2, V2) {
    let dx = b.center().x - a.center().x;
    let dy = b.center().y - a.center().y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (crate::geom::v2(1.0, 0.5), crate::geom::v2(0.0, 0.5))
        } else {
            (crate::geom::v2(0.0, 0.5), crate::geom::v2(1.0, 0.5))
        }
    } else if dy >= 0.0 {
        (crate::geom::v2(0.5, 1.0), crate::geom::v2(0.5, 0.0))
    } else {
        (crate::geom::v2(0.5, 0.0), crate::geom::v2(0.5, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    #[test]
    fn explicit_coordinates_are_kept_when_every_node_has_them() {
        let text = r#"{
            "nodes": [
                {"id": "a", "label": "Start", "x": 40, "y": 10, "width": 120, "height": 50},
                {"id": "b", "label": "End", "x": 260, "y": 160, "width": 120, "height": 50}
            ],
            "edges": [{"from": "a", "to": "b", "label": "go"}]
        }"#;
        let g = parse(text, Layout::Auto).unwrap();
        assert_eq!(g.doc.root.len(), 3);
        let start = g.doc.nodes.values().find(|n| n.label == "Start").unwrap();
        assert_eq!((start.rect.x, start.rect.y), (0.0, 0.0));
        let end = g.doc.nodes.values().find(|n| n.label == "End").unwrap();
        assert_eq!((end.rect.x, end.rect.y), (220.0, 150.0));
        assert!(g.report.warnings.is_empty());
        assert!(g.doc.validate().is_ok());
    }

    #[test]
    fn a_graph_without_coordinates_gets_a_deterministic_layout() {
        let text = r#"{"nodes":[{"id":"a"},{"id":"b"},{"id":"c"}],"edges":[{"source":"a","target":"b"},{"source":"b","target":"c"}]}"#;
        let a = parse(text, Layout::Auto).unwrap();
        let b = parse(text, Layout::Auto).unwrap();
        let mut ra: Vec<_> = a.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        let mut rb: Vec<_> = b.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        ra.sort_by_key(|(id, _)| *id);
        rb.sort_by_key(|(id, _)| *id);
        assert_eq!(ra, rb);
        let y = |g: &Generated, label: &str| {
            g.doc
                .nodes
                .values()
                .find(|n| n.label == label)
                .unwrap()
                .rect
                .y
        };
        assert!(y(&a, "a") < y(&a, "b") && y(&a, "b") < y(&a, "c"));
    }

    #[test]
    fn shapes_and_dangling_edges_are_handled() {
        let text = r#"{"nodes":[{"id":"a","shape":"diamond"},{"id":"b","shape":"ellipse"}],"edges":[{"from":"a","to":"missing"}]}"#;
        let g = parse(text, Layout::Auto).unwrap();
        assert!(g.report.summary().contains("not a node"));
        let shapes: Vec<_> = g
            .doc
            .nodes
            .values()
            .map(|n| std::mem::discriminant(&n.kind))
            .collect();
        assert!(shapes.contains(&std::mem::discriminant(&NodeKind::Polygon { sides: 4 })));
        assert!(shapes.contains(&std::mem::discriminant(&NodeKind::Ellipse)));
    }

    #[test]
    fn invalid_json_and_an_empty_graph_fail_with_a_message() {
        let err = parse("{\"nodes\": [}", Layout::Auto).unwrap_err();
        assert!(err.contains("invalid JSON at line 1"), "{err}");
        let err = parse("{\"edges\":[]}", Layout::Auto).unwrap_err();
        assert!(err.contains("nodes array"), "{err}");
        let err = parse("{\"nodes\":[]}", Layout::Auto).unwrap_err();
        assert!(err.contains("no nodes"), "{err}");
    }
}
