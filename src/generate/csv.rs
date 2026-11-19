//! CSV to diagram import. Rows become native shapes on a deterministic grid,
//! flow, or tree, with the delimiter detected from the first line. Header and
//! label-column choices only change the labels, never the underlying nodes.

use super::{
    BLUE, Builder, Dir, Generated, Options, Report, grid_positions, label_size, layered_positions,
    rect_at,
};
use crate::geom::V2;
use crate::model::Head;

pub fn parse(text: &str, opts: &Options) -> Result<Generated, String> {
    let delimiter = detect_delimiter(text);
    let rows = parse_rows(text, delimiter)?;
    let rows: Vec<Vec<String>> = rows
        .into_iter()
        .filter(|row| row.iter().any(|cell| !cell.trim().is_empty()))
        .collect();
    if rows.is_empty() {
        return Err("the CSV has no rows".into());
    }

    let mut report = Report::default();
    let data: &[Vec<String>] = if opts.header && rows.len() > 1 {
        &rows[1..]
    } else if opts.header {
        return Err("the CSV has a header but no data rows".into());
    } else {
        &rows[..]
    };
    let header = if opts.header { Some(&rows[0]) } else { None };
    let expected = header.map_or_else(|| rows[0].len(), Vec::len);
    for (i, row) in data.iter().enumerate() {
        if row.len() != expected {
            report.warn(format!(
                "row {} has {} column(s), expected {expected}",
                i + if opts.header { 2 } else { 1 },
                row.len()
            ));
        }
    }

    let layout = if opts.layout == super::Layout::Auto {
        super::Layout::VerticalFlow
    } else {
        opts.layout
    };
    if layout.is_tree() && expected < 2 {
        return Err(
            "tree layouts need an id column and a parent column; this CSV has only one column"
                .into(),
        );
    }
    if let Some(col) = opts.label_column
        && col >= expected
    {
        report.warn(format!(
            "label column {} does not exist; labels fall back to the whole row",
            col + 1
        ));
    }

    let labels: Vec<String> = data
        .iter()
        .map(|row| row_label(row, header, opts))
        .collect();
    let sizes: Vec<(f64, f64)> = labels
        .iter()
        .map(|label| label_size(label, 130.0, 44.0))
        .collect();
    let edges: Vec<(usize, usize)> = if layout.is_tree() {
        tree_edges(data, &mut report)
    } else if layout == super::Layout::Grid {
        Vec::new()
    } else {
        super::chain_edges(labels.len())
    };
    let dir = dir_of(layout);
    let (pos, cycle) = if layout == super::Layout::Grid {
        (grid_positions(&sizes), None)
    } else {
        layered_positions(&sizes, &edges, dir)
    };
    if let Some(w) = cycle {
        report.warn(w);
    }

    let mut b = Builder::new("CSV import");
    let ids: Vec<crate::model::NodeId> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| b.rect(rect_at(pos[i], sizes[i]), label, Some(BLUE), 4.0, None))
        .collect();
    let (hf, ht) = anchors(dir);
    for &(from, to) in &edges {
        if from == to {
            continue;
        }
        b.link(
            (ids[from], hf),
            (ids[to], ht),
            "",
            false,
            (Head::None, Head::Arrow),
        );
    }
    b.finish("CSV import", report)
}

fn dir_of(layout: super::Layout) -> Dir {
    match layout {
        super::Layout::HorizontalFlow | super::Layout::HorizontalTree => Dir::Right,
        _ => Dir::Down,
    }
}

fn anchors(dir: Dir) -> (V2, V2) {
    match dir {
        Dir::Down => (crate::geom::v2(0.5, 1.0), crate::geom::v2(0.5, 0.0)),
        Dir::Up => (crate::geom::v2(0.5, 0.0), crate::geom::v2(0.5, 1.0)),
        Dir::Right => (crate::geom::v2(1.0, 0.5), crate::geom::v2(0.0, 0.5)),
        Dir::Left => (crate::geom::v2(0.0, 0.5), crate::geom::v2(1.0, 0.5)),
    }
}

fn row_label(row: &[String], header: Option<&Vec<String>>, opts: &Options) -> String {
    if let Some(col) = opts.label_column
        && let Some(cell) = row.get(col).map(|c| c.trim()).filter(|c| !c.is_empty())
    {
        return cell.to_string();
    }
    match header {
        Some(header) => {
            let mut lines = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let cell = cell.trim();
                if cell.is_empty() {
                    continue;
                }
                let name = header.get(i).map(|h| h.trim()).unwrap_or("");
                lines.push(if name.is_empty() {
                    cell.to_string()
                } else {
                    format!("{name}: {cell}")
                });
            }
            if lines.is_empty() {
                row.first()
                    .map(|c| c.trim().to_string())
                    .unwrap_or_default()
            } else {
                lines.join("\n")
            }
        }
        None => row
            .iter()
            .map(|c| c.trim())
            .filter(|c| !c.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Tree layouts read the first cell as the row id and the second as its parent.
/// Ids that never appear as a parent stay at the top.
fn tree_edges(data: &[Vec<String>], report: &mut Report) -> Vec<(usize, usize)> {
    let mut index = std::collections::HashMap::new();
    for (i, row) in data.iter().enumerate() {
        let id = cell(row, 0);
        if id.is_empty() {
            continue;
        }
        if index.insert(id.to_string(), i).is_some() {
            report.warn(format!("duplicate id \"{id}\" keeps the first row"));
        }
    }
    let mut edges = Vec::new();
    for (i, row) in data.iter().enumerate() {
        let parent = cell(row, 1);
        if parent.is_empty() {
            continue;
        }
        match index.get(parent) {
            Some(&p) if p != i => edges.push((p, i)),
            Some(_) => report.warn(format!("row {} lists itself as its parent", i + 1)),
            None => report.warn(format!(
                "row {} names parent \"{parent}\", which is not in the CSV",
                i + 1
            )),
        }
    }
    edges
}

fn cell(row: &[String], i: usize) -> &str {
    row.get(i).map(|c| c.trim()).unwrap_or("")
}

/// Pick the delimiter that appears most often outside quotes on the first line.
fn detect_delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or("");
    let candidates = [',', ';', '\t', '|'];
    let mut best = (',', 0usize);
    for c in candidates {
        let mut quoted = false;
        let mut count = 0;
        for ch in first.chars() {
            if ch == '"' {
                quoted = !quoted;
            } else if !quoted && ch == c {
                count += 1;
            }
        }
        if count > best.1 {
            best = (c, count);
        }
    }
    best.0
}

/// Parse delimited text into rows, honouring quoted fields with `""` escapes and
/// newlines inside quotes. Reports the line where a quote is left open.
fn parse_rows(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut quote_line = 0usize;
    let mut line = 1usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                if ch == '\n' {
                    line += 1;
                }
                field.push(ch);
            }
            continue;
        }
        match ch {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
                quote_line = line;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                line += 1;
            }
            c if c == delimiter => row.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    if quoted {
        return Err(format!("unterminated quote opened on line {quote_line}"));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    fn opts(layout: super::super::Layout) -> Options {
        Options {
            layout,
            header: false,
            label_column: None,
        }
    }

    #[test]
    fn a_semicolon_fixture_becomes_one_shape_per_row() {
        let text = "name;role;team\nAda;Engineer;Core\nGrace;Admiral;Ops\nLinus;Kernel;Core\n";
        let g = parse(
            text,
            &Options {
                header: true,
                label_column: Some(0),
                ..opts(super::super::Layout::Grid)
            },
        )
        .unwrap();
        assert_eq!(g.doc.root.len(), 3);
        let labels: Vec<_> = g.doc.nodes.values().map(|n| n.label.clone()).collect();
        assert!(labels.contains(&"Ada".to_string()));
        assert!(labels.contains(&"Grace".to_string()));
        assert!(g.report.warnings.is_empty(), "{:?}", g.report.warnings);
        assert!(g.doc.validate().is_ok());
        assert_eq!(g.doc.root.len(), 3);
    }

    #[test]
    fn a_header_without_a_label_column_names_each_cell() {
        let g = parse(
            "name,role\nAda,Engineer\n",
            &Options {
                header: true,
                ..opts(super::super::Layout::VerticalFlow)
            },
        )
        .unwrap();
        let node = g.doc.nodes.values().next().unwrap();
        assert!(node.label.contains("name: Ada"), "{}", node.label);
        assert!(node.label.contains("role: Engineer"), "{}", node.label);
    }

    #[test]
    fn a_tree_layout_binds_each_row_to_its_parent() {
        let text = "id,parent,label\n1,,Root\n2,1,Child A\n3,1,Child B\n";
        let g = parse(
            text,
            &Options {
                header: true,
                ..opts(super::super::Layout::VerticalTree)
            },
        )
        .unwrap();
        let connectors = g
            .doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Connector { .. }))
            .count();
        assert_eq!(connectors, 2);
        for n in g.doc.nodes.values() {
            if let NodeKind::Connector { start, end, .. } = &n.kind {
                assert!(start.is_some() && end.is_some());
            }
        }
        assert!(g.report.warnings.is_empty(), "{:?}", g.report.warnings);
    }

    #[test]
    fn an_unterminated_quote_and_a_missing_parent_fail_or_warn_clearly() {
        let err = parse("a,b\n\"oops,2\n", &opts(super::super::Layout::Grid)).unwrap_err();
        assert!(err.contains("unterminated quote"), "{err}");
        let g = parse(
            "id,parent\n1,9\n",
            &opts(super::super::Layout::VerticalTree),
        )
        .unwrap();
        assert!(g.report.summary().contains("not in the CSV"));
    }

    #[test]
    fn the_same_text_produces_the_same_layout() {
        let text = "a,b\n1,2\n3,4\n5,6\n";
        let a = parse(text, &opts(super::super::Layout::Grid)).unwrap();
        let b = parse(text, &opts(super::super::Layout::Grid)).unwrap();
        let mut ra: Vec<_> = a.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        let mut rb: Vec<_> = b.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        ra.sort_by_key(|(id, _)| *id);
        rb.sort_by_key(|(id, _)| *id);
        assert_eq!(ra, rb);
    }
}
