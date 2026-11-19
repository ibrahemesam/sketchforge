//! Mermaid flowchart and sequence import.
//!
//! Flowcharts support the common node shapes, chained and labelled edges, and
//! the four directions. Sequence diagrams support participants, actors,
//! messages, and notes. Unsupported directives are reported and skipped, so a
//! pasted diagram still becomes editable shapes instead of failing wholesale.

use super::{
    BLUE, Builder, Dir, GREEN, Generated, MUTED, PURPLE, Report, YELLOW, label_size,
    layered_positions, rect_at,
};
use crate::geom::{Rect, v2};
use crate::model::Head;
use std::collections::HashMap;

pub fn parse(text: &str) -> Result<Generated, String> {
    let first = text
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty() && !l.starts_with("%%"));
    match first {
        Some(l) if l.starts_with("sequenceDiagram") => parse_sequence(text),
        Some(l) if is_flow_header(l) => parse_flowchart(text),
        Some(l) => Err(format!(
            "expected a Mermaid flowchart (graph TD, flowchart LR) or sequenceDiagram, found \"{}\"",
            first_chars(l, 40)
        )),
        None => Err("the Mermaid text has no diagram header".into()),
    }
}

fn first_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn is_flow_header(line: &str) -> bool {
    let mut words = line.split_whitespace();
    matches!(words.next(), Some("graph") | Some("flowchart"))
}

// ---------------------------------------------------------------------------------- flowchart

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Rect,
    Rounded,
    Circle,
    Diamond,
    Hexagon,
}

#[derive(Debug, Clone)]
struct FNode {
    label: String,
    shape: Shape,
}

#[derive(Debug, Clone)]
struct FEdge {
    from: String,
    to: String,
    label: String,
    dashed: bool,
    thick: bool,
    arrow: bool,
}

fn parse_flowchart(text: &str) -> Result<Generated, String> {
    let mut lines = text.lines().map(str::trim);
    let header = lines
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .ok_or_else(|| "the Mermaid text has no diagram header".to_string())?;
    let dir = flow_dir(header);
    let mut nodes: HashMap<String, FNode> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut edges: Vec<FEdge> = Vec::new();
    let mut report = Report::default();
    let mut ignored = 0usize;
    let mut subgraphs = 0usize;

    for line in lines {
        let line = strip_comment(line);
        if line.is_empty() {
            continue;
        }
        if line == "end" {
            continue;
        }
        if line.starts_with("subgraph") {
            subgraphs += 1;
            continue;
        }
        if line.starts_with("classDef")
            || line.starts_with("class ")
            || line.starts_with("classDef ")
            || line.starts_with("style ")
            || line.starts_with("linkStyle")
            || line.starts_with("click ")
            || line.starts_with("direction ")
            || line.starts_with("accTitle")
            || line.starts_with("accDescr")
        {
            ignored += 1;
            continue;
        }
        if let Err(e) = parse_flow_line(&line, &mut nodes, &mut order, &mut edges) {
            report.warn(format!("\"{}\": {e}", first_chars(&line, 40)));
        }
    }
    if ignored > 0 {
        report.warn(format!("{ignored} style or link directive(s) were ignored"));
    }
    if subgraphs > 0 {
        report.warn(format!("{subgraphs} subgraph(s) were flattened"));
    }
    if order.is_empty() {
        return Err("the flowchart has no nodes".into());
    }

    let index: HashMap<String, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    let sizes: Vec<(f64, f64)> = order
        .iter()
        .map(|id| {
            let n = &nodes[id];
            match n.shape {
                Shape::Rect => label_size(&n.label, 120.0, 52.0),
                Shape::Rounded => label_size(&n.label, 120.0, 52.0),
                Shape::Circle => {
                    let (w, h) = label_size(&n.label, 96.0, 96.0);
                    (w.max(h) * 0.8, w.max(h) * 0.8)
                }
                Shape::Diamond => {
                    let (w, h) = label_size(&n.label, 160.0, 90.0);
                    (w.max(150.0), h.max(86.0))
                }
                Shape::Hexagon => {
                    let (w, h) = label_size(&n.label, 150.0, 68.0);
                    (w.max(140.0), h.max(66.0))
                }
            }
        })
        .collect();
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|e| {
            let a = index.get(&e.from)?;
            let b = index.get(&e.to)?;
            Some((*a, *b))
        })
        .collect();
    let (pos, cycle) = layered_positions(&sizes, &pairs, dir);
    if let Some(w) = cycle {
        report.warn(w);
    }

    let mut b = Builder::new("Mermaid diagram");
    let mut ids: HashMap<String, crate::model::NodeId> = HashMap::new();
    for (i, id) in order.iter().enumerate() {
        let n = &nodes[id];
        let r = rect_at(pos[i], sizes[i]);
        let node_id = match n.shape {
            Shape::Rect => b.rect(r, &n.label, Some(BLUE), 4.0, None),
            Shape::Rounded => b.rect(r, &n.label, Some(BLUE), 14.0, None),
            Shape::Circle => b.ellipse(r, &n.label, Some(GREEN)),
            Shape::Diamond => b.polygon(r, &n.label, 4, Some(YELLOW)),
            Shape::Hexagon => b.polygon(r, &n.label, 6, Some(PURPLE)),
        };
        ids.insert(id.clone(), node_id);
    }
    for e in &edges {
        let (Some(&from), Some(&to)) = (ids.get(&e.from), ids.get(&e.to)) else {
            continue;
        };
        let (hf, ht) = match dir {
            Dir::Down => (v2(0.5, 1.0), v2(0.5, 0.0)),
            Dir::Up => (v2(0.5, 0.0), v2(0.5, 1.0)),
            Dir::Right => (v2(1.0, 0.5), v2(0.0, 0.5)),
            Dir::Left => (v2(0.0, 0.5), v2(1.0, 0.5)),
        };
        let head = if e.arrow { Head::Arrow } else { Head::None };
        let cid = if from == to {
            self_loop(&mut b, from)
        } else {
            b.link((from, hf), (to, ht), &e.label, e.dashed, (Head::None, head))
        };
        if e.thick
            && let Some(n) = b.doc.nodes.get_mut(&cid)
        {
            n.style.stroke_width = 2.8;
        }
        if e.dashed {
            b.set_dash(cid, &[6.0, 4.0]);
        }
    }
    b.finish("Mermaid diagram", report)
}

fn self_loop(b: &mut Builder, id: crate::model::NodeId) -> crate::model::NodeId {
    let r = b.world_rect(id).unwrap_or_default();
    let a = v2(r.right(), r.y + r.h * 0.35);
    let points = vec![
        a,
        a + v2(44.0, 0.0),
        a + v2(44.0, -44.0),
        v2(r.x + r.w * 0.7, r.y),
    ];
    b.connector(points, None, None, "", false, (Head::None, Head::Arrow))
}

fn flow_dir(header: &str) -> Dir {
    match header.split_whitespace().nth(1).unwrap_or("TD") {
        "LR" => Dir::Right,
        "RL" => Dir::Left,
        "BT" => Dir::Up,
        _ => Dir::Down,
    }
}

fn strip_comment(line: &str) -> String {
    match line.find("%%") {
        Some(i) => line[..i].trim().to_string(),
        None => line.trim().to_string(),
    }
}

fn parse_flow_line(
    line: &str,
    nodes: &mut HashMap<String, FNode>,
    order: &mut Vec<String>,
    edges: &mut Vec<FEdge>,
) -> Result<(), String> {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    skip_ws(&chars, &mut i);
    let mut group = parse_group(&chars, &mut i, nodes, order)?;
    loop {
        skip_ws(&chars, &mut i);
        if i >= chars.len() {
            break;
        }
        let Some(op) = parse_edge(&chars, &mut i)? else {
            return Err("expected an arrow between nodes".into());
        };
        skip_ws(&chars, &mut i);
        let next = parse_group(&chars, &mut i, nodes, order)?;
        for a in &group {
            for b in &next {
                if op.hidden {
                    continue;
                }
                let (from, to) = if op.reversed {
                    (b.clone(), a.clone())
                } else {
                    (a.clone(), b.clone())
                };
                edges.push(FEdge {
                    from,
                    to,
                    label: op.label.clone(),
                    dashed: op.dashed,
                    thick: op.thick,
                    arrow: op.arrow,
                });
            }
        }
        group = next;
    }
    Ok(())
}

fn skip_ws(chars: &[char], i: &mut usize) {
    while *i < chars.len() && chars[*i].is_whitespace() {
        *i += 1;
    }
}

fn parse_group(
    chars: &[char],
    i: &mut usize,
    nodes: &mut HashMap<String, FNode>,
    order: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let mut out = vec![parse_node_ref(chars, i, nodes, order)?];
    loop {
        skip_ws(chars, i);
        if *i < chars.len() && chars[*i] == '&' {
            *i += 1;
            skip_ws(chars, i);
            out.push(parse_node_ref(chars, i, nodes, order)?);
        } else {
            break;
        }
    }
    Ok(out)
}

fn parse_node_ref(
    chars: &[char],
    i: &mut usize,
    nodes: &mut HashMap<String, FNode>,
    order: &mut Vec<String>,
) -> Result<String, String> {
    skip_ws(chars, i);
    let start = *i;
    while *i < chars.len() && (chars[*i].is_alphanumeric() || matches!(chars[*i], '_' | '.')) {
        *i += 1;
    }
    if *i == start {
        return Err("expected a node name".into());
    }
    let id: String = chars[start..*i].iter().collect();
    let shape = parse_shape(chars, i);
    let entry = nodes.entry(id.clone()).or_insert_with(|| FNode {
        label: id.clone(),
        shape: Shape::Rect,
    });
    if let Some((shape, label)) = shape {
        entry.shape = shape;
        if !label.is_empty() {
            entry.label = label;
        }
    }
    if !order.iter().any(|x| x == &id) {
        order.push(id.clone());
    }
    Ok(id)
}

fn parse_shape(chars: &[char], i: &mut usize) -> Option<(Shape, String)> {
    let rest: String = chars[*i..].iter().collect();
    let cases: &[(&str, &str, Shape)] = &[
        ("((", "))", Shape::Circle),
        ("{{", "}}", Shape::Hexagon),
        ("[[", "]]", Shape::Rect),
        ("([", "])", Shape::Rounded),
        ("[(", ")]", Shape::Rect),
        ("[/", "/]", Shape::Rect),
        ("[\\", "\\]", Shape::Rect),
        ("[", "]", Shape::Rect),
        ("(", ")", Shape::Rounded),
        ("{", "}", Shape::Diamond),
        (">", "]", Shape::Rect),
    ];
    for (open, close, shape) in cases {
        if !rest.starts_with(open) {
            continue;
        }
        let after = &rest[open.len()..];
        let end = after.find(close)?;
        let mut label = after[..end].trim().to_string();
        if (label.starts_with('"') && label.ends_with('"') && label.len() >= 2)
            || (label.starts_with('\'') && label.ends_with('\'') && label.len() >= 2)
        {
            label = label[1..label.len() - 1].to_string();
        }
        let consumed = open.chars().count() + after[..end].chars().count() + close.chars().count();
        *i += consumed;
        return Some((*shape, html_breaks(&label)));
    }
    None
}

fn html_breaks(s: &str) -> String {
    s.replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("<br>", "\n")
}

struct EdgeOp {
    dashed: bool,
    thick: bool,
    arrow: bool,
    reversed: bool,
    hidden: bool,
    label: String,
}

fn parse_edge(chars: &[char], i: &mut usize) -> Result<Option<EdgeOp>, String> {
    let rest: String = chars[*i..].iter().collect();
    let mut op = EdgeOp {
        dashed: false,
        thick: false,
        arrow: true,
        reversed: false,
        hidden: false,
        label: String::new(),
    };
    let consumed = if rest.starts_with("<--") {
        op.reversed = true;
        3
    } else if rest.starts_with("<-") {
        op.reversed = true;
        2
    } else if rest.starts_with("~~~") {
        op.hidden = true;
        3
    } else if rest.starts_with("---") {
        op.arrow = false;
        3
    } else if rest.starts_with("-->") || rest.starts_with("--x") || rest.starts_with("--o") {
        3
    } else if rest.starts_with("-.->") {
        op.dashed = true;
        4
    } else if rest.starts_with("==>") || rest.starts_with("===") {
        op.thick = true;
        op.arrow = rest.starts_with("==>");
        3
    } else if let Some(after) = rest.strip_prefix("--") {
        let Some((token, at)) = find_any(after, &["-->", "---", "--x", "--o"]) else {
            return Err("unterminated -- edge".into());
        };
        op.arrow = token != "---";
        op.label = after[..at].trim().to_string();
        2 + after[..at].chars().count() + token.chars().count()
    } else if rest.starts_with(".-") || rest.starts_with("-.") {
        let offset = 2;
        let after = &rest[offset..];
        let Some((token, at)) = find_any(after, &[".->", ".-x", ".-o", ".-"]) else {
            return Err("unterminated dashed edge".into());
        };
        op.dashed = true;
        op.arrow = token != ".-";
        op.label = after[..at].trim().to_string();
        offset + after[..at].chars().count() + token.chars().count()
    } else if let Some(after) = rest.strip_prefix("==") {
        let Some((token, at)) = find_any(after, &["==>", "==="]) else {
            return Err("unterminated == edge".into());
        };
        op.thick = true;
        op.arrow = token == "==>";
        op.label = after[..at].trim().to_string();
        2 + after[..at].chars().count() + token.chars().count()
    } else if rest.starts_with("->") {
        2
    } else {
        return Ok(None);
    };
    *i += consumed;
    // `-->|yes|` style labels.
    let after: String = chars[*i..].iter().collect();
    if let Some(inner) = after.strip_prefix('|')
        && let Some(end) = inner.find('|')
    {
        op.label = inner[..end].trim().to_string();
        *i += 1 + inner[..end].chars().count() + 1;
    }
    Ok(Some(op))
}

fn find_any<'a>(s: &'a str, needles: &[&'a str]) -> Option<(&'a str, usize)> {
    needles
        .iter()
        .filter_map(|n| s.find(n).map(|at| (*n, at)))
        .min_by_key(|(_, at)| *at)
}

// ---------------------------------------------------------------------------------- sequence

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MsgKind {
    Message,
    Note,
}

#[derive(Debug, Clone)]
struct SeqParticipant {
    label: String,
}

#[derive(Debug, Clone)]
struct SeqItem {
    kind: MsgKind,
    from: usize,
    to: usize,
    text: String,
    dashed: bool,
    arrow: bool,
}

fn parse_sequence(text: &str) -> Result<Generated, String> {
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut by_id: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<SeqItem> = Vec::new();
    let mut report = Report::default();
    let mut ignored = 0usize;

    let touch =
        |id: &str, participants: &mut Vec<SeqParticipant>, by_id: &mut HashMap<String, usize>| {
            let id = id.trim().trim_start_matches(['+', '-']).to_string();
            if id.is_empty() {
                return 0;
            }
            if let Some(&i) = by_id.get(&id) {
                return i;
            }
            let i = participants.len();
            participants.push(SeqParticipant { label: id.clone() });
            by_id.insert(id, i);
            i
        };

    for raw in text.lines() {
        let line = strip_comment(raw);
        if line.is_empty() || line.starts_with("sequenceDiagram") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("participant ") || lower.starts_with("actor ") {
            let rest = if lower.starts_with("participant ") {
                &line["participant ".len()..]
            } else {
                &line["actor ".len()..]
            };
            let (id, label) = match rest.split_once(" as ") {
                Some((id, label)) => (id.trim(), label.trim()),
                None => (rest.trim(), rest.trim()),
            };
            let i = touch(id, &mut participants, &mut by_id);
            participants[i].label = label.to_string();
            continue;
        }
        if lower.starts_with("note ") {
            let rest = &line[5..];
            let (over, text) = match rest.split_once(':') {
                Some((a, b)) => (a.trim(), b.trim()),
                None => (rest.trim(), ""),
            };
            let names = over
                .trim_start_matches("over ")
                .trim_start_matches("left of ")
                .trim_start_matches("right of ")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>();
            if names.is_empty() {
                ignored += 1;
                continue;
            }
            let mut idx = names
                .iter()
                .map(|n| touch(n, &mut participants, &mut by_id))
                .collect::<Vec<_>>();
            idx.sort_unstable();
            idx.dedup();
            items.push(SeqItem {
                kind: MsgKind::Note,
                from: idx[0],
                to: *idx.last().unwrap(),
                text: html_breaks(text),
                dashed: false,
                arrow: false,
            });
            continue;
        }
        if lower.starts_with("loop")
            || lower.starts_with("alt")
            || lower.starts_with("else")
            || lower.starts_with("end")
            || lower.starts_with("opt")
            || lower.starts_with("par")
            || lower.starts_with("and")
            || lower.starts_with("rect")
            || lower.starts_with("critical")
            || lower.starts_with("break")
            || lower.starts_with("box")
            || lower.starts_with("create")
            || lower.starts_with("destroy")
            || lower.starts_with("activate")
            || lower.starts_with("deactivate")
            || lower.starts_with("autonumber")
            || lower.starts_with("link")
            || lower.starts_with("properties")
        {
            ignored += 1;
            continue;
        }
        let (before, message) = match line.split_once(':') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (line.trim(), ""),
        };
        let Some((arrow, at)) = find_message_arrow(before) else {
            ignored += 1;
            continue;
        };
        let from = before[..at].trim();
        let to = before[at + arrow.len()..]
            .trim()
            .trim_start_matches(['+', '-'])
            .trim();
        if from.is_empty() || to.is_empty() {
            ignored += 1;
            continue;
        }
        let fi = touch(from, &mut participants, &mut by_id);
        let ti = touch(to, &mut participants, &mut by_id);
        items.push(SeqItem {
            kind: MsgKind::Message,
            from: fi,
            to: ti,
            text: html_breaks(message),
            dashed: arrow.starts_with("--"),
            arrow: arrow.ends_with('>') || arrow.ends_with('x') || arrow.ends_with(')'),
        });
    }
    if participants.is_empty() {
        return Err("the sequence diagram has no participants".into());
    }
    if !items.iter().any(|i| i.kind == MsgKind::Message) {
        return Err("the sequence diagram has no messages; add a line like A->>B: hello".into());
    }
    if ignored > 0 {
        report.warn(format!("{ignored} sequence directive(s) were ignored"));
    }

    let col_w = participants
        .iter()
        .map(|p| label_size(&p.label, 120.0, 40.0).0)
        .fold(160.0, f64::max)
        + 80.0;
    let boxes: Vec<Rect> = participants
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let w = label_size(&participants[i].label, 120.0, 40.0)
                .0
                .min(col_w - 30.0);
            Rect::new(i as f64 * col_w + (col_w - w) / 2.0, 0.0, w, 44.0)
        })
        .collect();
    let bottom = 70.0 + items.len() as f64 * 56.0 + 30.0;

    let mut b = Builder::new("Mermaid sequence");
    // Lifelines first so participant boxes paint above them.
    for (i, p) in participants.iter().enumerate() {
        let x = boxes[i].center().x;
        let id = b.line(v2(x, boxes[i].bottom()), v2(x, bottom), MUTED, 1.2, None);
        if let Some(n) = b.doc.nodes.get_mut(&id) {
            n.name = format!("{} lifeline", p.label);
        }
    }
    for (i, p) in participants.iter().enumerate() {
        b.rect(boxes[i], &p.label, Some(BLUE), 6.0, None);
    }
    let mut y = 74.0;
    for item in &items {
        match item.kind {
            MsgKind::Note => {
                let x0 = boxes[item.from].x.max(0.0) - 20.0;
                let x1 = boxes[item.to].right() + 20.0;
                let w = x1 - x0;
                let (_, h) = label_size(&item.text, 80.0, 44.0);
                let r = Rect::new(x0, y, w, h.max(44.0));
                b.rect(r, &item.text, Some(YELLOW), 4.0, None);
            }
            MsgKind::Message => {
                if item.from == item.to {
                    let r = boxes[item.from];
                    let a = v2(r.right(), y + 12.0);
                    let points = vec![
                        a,
                        a + v2(52.0, 0.0),
                        a + v2(52.0, 30.0),
                        v2(r.right() - 20.0, y + 30.0),
                    ];
                    b.connector(
                        points,
                        None,
                        None,
                        &item.text,
                        item.dashed,
                        (
                            Head::None,
                            if item.arrow { Head::Arrow } else { Head::None },
                        ),
                    );
                } else {
                    let a = v2(boxes[item.from].center().x, y);
                    let z = v2(boxes[item.to].center().x, y);
                    b.connector(
                        vec![a, z],
                        None,
                        None,
                        &item.text,
                        item.dashed,
                        (
                            Head::None,
                            if item.arrow { Head::Arrow } else { Head::None },
                        ),
                    );
                }
            }
        }
        y += 56.0;
    }
    b.finish("Mermaid sequence", report)
}

fn find_message_arrow(s: &str) -> Option<(&'static str, usize)> {
    const ARROWS: [&str; 10] = [
        "-->>", "->>", "--x", "-x", "--)", "-)", "-->", "->", "--", "-",
    ];
    let mut best: Option<(&'static str, usize)> = None;
    for a in ARROWS {
        if let Some(at) = s.find(a)
            && best.is_none_or(|(_, b)| at < b)
        {
            best = Some((a, at));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    #[test]
    fn a_flowchart_becomes_bound_editable_nodes() {
        let g = parse(
            "flowchart TD\n  A[Start] --> B{Ready?}\n  B -->|yes| C(Done)\n  A -->|retry| C\n",
        )
        .unwrap();
        assert_eq!(
            g.doc
                .root
                .iter()
                .filter(|id| !g.doc.get(**id).unwrap().is_connector())
                .count(),
            3
        );
        assert!(g.report.warnings.is_empty(), "{:?}", g.report.warnings);
        let mut shapes = 0;
        let mut connectors = 0;
        for n in g.doc.nodes.values() {
            match &n.kind {
                NodeKind::Connector { start, end, .. } => {
                    connectors += 1;
                    assert!(start.is_some() && end.is_some(), "edges are bound");
                }
                _ => shapes += 1,
            }
        }
        assert_eq!(shapes, 3);
        assert_eq!(connectors, 3);
        assert!(g.doc.validate().is_ok());
        assert!(g.doc.nodes.values().any(|n| n.label == "yes"));
    }

    #[test]
    fn direction_changes_the_main_axis() {
        let down = parse("graph TD\n A[A] --> B[B]\n").unwrap();
        let right = parse("graph LR\n A[A] --> B[B]\n").unwrap();
        let (a, b) = roots(&down);
        let (c, d) = roots(&right);
        assert!(a.1 < b.1, "TD stacks vertically");
        assert!(c.0 < d.0, "LR places horizontally");
    }

    fn roots(g: &Generated) -> ((f64, f64), (f64, f64)) {
        let mut v = Vec::new();
        for id in &g.doc.root {
            let n = g.doc.get(*id).unwrap();
            if !n.is_connector() {
                v.push((n.rect.x, n.rect.y));
            }
        }
        assert_eq!(v.len(), 2);
        (v[0], v[1])
    }

    #[test]
    fn a_sequence_diagram_gets_participants_lifelines_and_messages() {
        let g = parse(
            "sequenceDiagram\n participant A as Alice\n actor B as Bob\n A->>B: Hello\n B-->>A: Hi\n Note over A,B: thinking\n",
        )
        .unwrap();
        assert!(g.doc.nodes.values().any(|n| n.label == "Alice"));
        assert!(g.doc.nodes.values().any(|n| n.label == "Hello"));
        assert!(g.doc.nodes.values().any(|n| n.label == "thinking"));
        let lines = g
            .doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Line { .. }))
            .count();
        assert_eq!(lines, 2, "one lifeline per participant");
        assert!(g.report.warnings.is_empty());
        assert!(g.doc.validate().is_ok());
    }

    #[test]
    fn unsupported_directives_are_reported_not_fatal() {
        let g =
            parse("graph LR\n A[A] --> B[B]\n classDef big font-size:20px\n style A fill:#fff\n")
                .unwrap();
        assert_eq!(g.report.warnings.len(), 1);
        assert!(g.report.summary().contains("ignored"));
        assert_eq!(
            g.doc
                .root
                .iter()
                .filter(|id| !g.doc.get(**id).unwrap().is_connector())
                .count(),
            2
        );
    }

    #[test]
    fn a_bad_header_and_a_header_only_diagram_fail_cleanly() {
        let err = parse("hello world").unwrap_err();
        assert!(err.contains("expected a Mermaid"), "{err}");
        let err = parse("flowchart TD").unwrap_err();
        assert!(err.contains("no nodes"), "{err}");
        let err = parse("sequenceDiagram\n participant A\n").unwrap_err();
        assert!(err.contains("no messages"), "{err}");
    }

    #[test]
    fn the_same_text_lays_out_identically() {
        let text = "graph LR\n A[A] --> B[B]\n A --> C[C]\n B --> D[D]\n";
        let a = parse(text).unwrap().doc;
        let b = parse(text).unwrap().doc;
        let mut ra: Vec<_> = a.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        let mut rb: Vec<_> = b.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        ra.sort_by_key(|(id, _)| *id);
        rb.sort_by_key(|(id, _)| *id);
        assert_eq!(ra, rb);
    }

    #[test]
    fn chained_edges_and_ampersands_create_every_connection() {
        let g = parse("graph TD\n A --> B --> C\n A & B --> D\n").unwrap();
        let connectors = g
            .doc
            .nodes
            .values()
            .filter(|n| matches!(n.kind, NodeKind::Connector { .. }))
            .count();
        assert_eq!(connectors, 1 + 1 + 2);
        assert_eq!(
            g.doc
                .root
                .iter()
                .filter(|id| !g.doc.get(**id).unwrap().is_connector())
                .count(),
            4
        );
    }

    #[test]
    fn a_self_loop_is_one_connector() {
        let g = parse("graph TD\n A[A] --> A\n").unwrap();
        assert_eq!(g.doc.root.len(), 2);
        assert_eq!(
            g.doc
                .nodes
                .values()
                .filter(|n| matches!(n.kind, NodeKind::Connector { .. }))
                .count(),
            1
        );
    }
}
