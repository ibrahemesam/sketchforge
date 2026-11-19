//! Diagram-wide find and replace.
//!
//! Search looks at every page's labels, connector labels, text, frame names,
//! tooltips, tags, node properties and page properties. Matches carry the page
//! and node so the find bar can jump to them. Replace rewrites one page's
//! matches as a single undo step and never touches a locked or hidden node
//! without reporting how many it skipped.

use crate::editor::Editor;
use crate::model::{Document, Node, NodeId};

/// One searchable field of a node or a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindField {
    Label,
    ExtraLabel(usize),
    Text,
    FrameName,
    Tooltip,
    Tag(usize),
    MetadataKey(String),
    MetadataValue(String),
    PageKey(String),
    PageValue(String),
}

impl FindField {
    /// Short description shown in the find bar and the status line.
    pub fn label(&self) -> String {
        match self {
            FindField::Label => "label".into(),
            FindField::ExtraLabel(i) => format!("label {}", i + 2),
            FindField::Text => "text".into(),
            FindField::FrameName => "frame name".into(),
            FindField::Tooltip => "tooltip".into(),
            FindField::Tag(_) => "tag".into(),
            FindField::MetadataKey(k) => format!("property {k}"),
            FindField::MetadataValue(k) => format!("property {k}"),
            FindField::PageKey(k) => format!("page property {k}"),
            FindField::PageValue(k) => format!("page property {k}"),
        }
    }

    /// Property names are searchable but not rewritten, so a replace can never
    /// silently drop a key that labels or placeholders depend on.
    pub fn replaceable(&self) -> bool {
        !matches!(self, FindField::MetadataKey(_) | FindField::PageKey(_))
    }
}

/// A single occurrence, with character offsets into its field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMatch {
    pub page: usize,
    pub node: Option<NodeId>,
    pub field: FindField,
    /// Character offset of the match's first character.
    pub start: usize,
    /// Character offset just past the match.
    pub end: usize,
    /// The field text around the match, on one line.
    pub preview: String,
}

/// A compiled query. Regex mode honours capture replacements; literal mode
/// compares characters case-insensitively unless asked not to.
#[derive(Debug, Clone)]
pub struct Query {
    regex: Option<regex::Regex>,
    literal: Vec<char>,
    case_sensitive: bool,
}

impl Query {
    pub fn new(pattern: &str, use_regex: bool, case_sensitive: bool) -> Result<Query, String> {
        if pattern.is_empty() {
            return Err("Type something to find.".into());
        }
        if use_regex {
            let re = regex::RegexBuilder::new(pattern)
                .case_insensitive(!case_sensitive)
                .build()
                .map_err(|e| format!("Invalid regular expression: {e}"))?;
            Ok(Query {
                regex: Some(re),
                literal: Vec::new(),
                case_sensitive,
            })
        } else {
            Ok(Query {
                regex: None,
                literal: pattern.chars().collect(),
                case_sensitive,
            })
        }
    }

    /// Character ranges of every match in `text`.
    pub fn ranges(&self, text: &str) -> Vec<(usize, usize)> {
        if let Some(re) = &self.regex {
            re.find_iter(text)
                .filter(|m| m.start() != m.end())
                .map(|m| (char_at(text, m.start()), char_at(text, m.end())))
                .collect()
        } else {
            literal_ranges(text, &self.literal, self.case_sensitive)
        }
    }

    /// Replace every match in `text`, returning the new text and the count.
    pub fn replace(&self, text: &str, replacement: &str) -> (String, usize) {
        if let Some(re) = &self.regex {
            let count = re.find_iter(text).filter(|m| m.start() != m.end()).count();
            if count == 0 {
                return (text.to_string(), 0);
            }
            (re.replace_all(text, replacement).into_owned(), count)
        } else {
            replace_literal(text, &self.literal, replacement, self.case_sensitive)
        }
    }
}

fn char_at(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

fn same_char(a: char, b: char, case_sensitive: bool) -> bool {
    if a == b || case_sensitive {
        return a == b;
    }
    a.to_lowercase().eq(b.to_lowercase())
}

fn literal_ranges(text: &str, needle: &[char], case_sensitive: bool) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let hay: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if (0..needle.len()).all(|k| same_char(hay[i + k], needle[k], case_sensitive)) {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn replace_literal(
    text: &str,
    needle: &[char],
    replacement: &str,
    case_sensitive: bool,
) -> (String, usize) {
    let ranges = literal_ranges(text, needle, case_sensitive);
    if ranges.is_empty() {
        return (text.to_string(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in &ranges {
        out.extend(chars[at..*start].iter());
        out.push_str(replacement);
        at = *end;
    }
    out.extend(chars[at..].iter());
    (out, ranges.len())
}

/// Fields of a node that can carry searchable text, in a stable order.
pub fn node_fields(n: &Node) -> Vec<FindField> {
    let mut out = Vec::new();
    if n.is_frame() {
        out.push(FindField::FrameName);
    } else if n.supports_label() && !n.label.is_empty() {
        out.push(FindField::Label);
    }
    if n.kind.text().is_some() {
        out.push(FindField::Text);
    }
    for i in 0..n.labels.len() {
        out.push(FindField::ExtraLabel(i));
    }
    if !n.tooltip.is_empty() {
        out.push(FindField::Tooltip);
    }
    for i in 0..n.tags.len() {
        out.push(FindField::Tag(i));
    }
    for key in n.metadata.keys() {
        out.push(FindField::MetadataKey(key.clone()));
        out.push(FindField::MetadataValue(key.clone()));
    }
    out
}

/// Text of a node field, if the field still exists.
pub fn node_field_text(n: &Node, field: &FindField) -> Option<String> {
    match field {
        FindField::Label => Some(n.label.clone()),
        FindField::ExtraLabel(i) => n.labels.get(*i).map(|l| l.text.clone()),
        FindField::Text => n.kind.text().cloned(),
        FindField::FrameName => n.is_frame().then(|| n.name.clone()),
        FindField::Tooltip => Some(n.tooltip.clone()),
        FindField::Tag(i) => n.tags.get(*i).cloned(),
        FindField::MetadataKey(k) => Some(k.clone()),
        FindField::MetadataValue(k) => n.metadata.get(k).cloned(),
        FindField::PageKey(_) | FindField::PageValue(_) => None,
    }
}

fn set_node_field(n: &mut Node, field: &FindField, value: String) {
    match field {
        FindField::Label => n.label = value,
        FindField::ExtraLabel(i) => {
            if let Some(l) = n.labels.get_mut(*i) {
                l.text = value;
            }
        }
        FindField::Text => {
            if let Some(t) = n.kind.text_mut() {
                *t = value;
            }
        }
        FindField::FrameName => n.name = value,
        FindField::Tooltip => n.tooltip = value,
        FindField::Tag(i) => {
            if let Some(t) = n.tags.get_mut(*i) {
                *t = value;
            }
        }
        FindField::MetadataValue(k) => {
            n.metadata.insert(k.clone(), value);
        }
        FindField::MetadataKey(_) => {}
        FindField::PageKey(_) | FindField::PageValue(_) => {}
    }
}

/// Preview text: the field on one line, trimmed around the match.
fn preview(text: &str, start: usize, end: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let from = start.saturating_sub(18);
    let to = (end + 18).min(chars.len());
    let mut s = String::new();
    if from > 0 {
        s.push('…');
    }
    s.extend(
        chars[from..to]
            .iter()
            .map(|c| if *c == '\n' { ' ' } else { *c }),
    );
    if to < chars.len() {
        s.push('…');
    }
    s
}

fn push_matches(
    out: &mut Vec<FindMatch>,
    page: usize,
    node: Option<NodeId>,
    field: FindField,
    text: &str,
    query: &Query,
) {
    for (start, end) in query.ranges(text) {
        out.push(FindMatch {
            page,
            node,
            field: field.clone(),
            start,
            end,
            preview: preview(text, start, end),
        });
    }
}

/// Search one page in paint order, so matches step from the bottom layer up.
pub fn search_page(doc: &Document, page: usize, query: &Query) -> Vec<FindMatch> {
    let mut out = Vec::new();
    for id in doc.paint_order() {
        let Some(n) = doc.get(id) else { continue };
        for field in node_fields(n) {
            if let Some(text) = node_field_text(n, &field) {
                push_matches(&mut out, page, Some(id), field, &text, query);
            }
        }
    }
    let keys: Vec<String> = doc.settings.metadata.keys().cloned().collect();
    for key in keys {
        if let Some(text) = doc.settings.metadata.get(&key) {
            push_matches(
                &mut out,
                page,
                None,
                FindField::PageValue(key.clone()),
                text,
                query,
            );
        }
        push_matches(
            &mut out,
            page,
            None,
            FindField::PageKey(key.clone()),
            &key,
            query,
        );
    }
    out
}

/// Search every page. Errors only for a malformed regex.
pub fn search(ed: &Editor, query: &Query) -> Vec<FindMatch> {
    let mut out = Vec::new();
    for page in 0..ed.pages.len() {
        out.extend(search_page(ed.page_doc(page), page, query));
    }
    out
}

/// What a replace pass changed, and what it refused to touch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReplaceReport {
    pub replaced: usize,
    pub skipped: usize,
    pub pages: usize,
}

/// Replace every match on every page. Each page commits one undo step.
pub fn replace_all(
    ed: &mut Editor,
    pattern: &str,
    replacement: &str,
    use_regex: bool,
    case_sensitive: bool,
) -> Result<ReplaceReport, String> {
    let query = Query::new(pattern, use_regex, case_sensitive)?;
    let mut report = ReplaceReport::default();
    for page in 0..ed.pages.len() {
        let doc = ed.page_doc(page);
        let mut node_edits: Vec<(NodeId, Vec<FindField>)> = Vec::new();
        let mut page_edits: Vec<FindField> = Vec::new();
        for id in doc.paint_order() {
            let Some(n) = doc.get(id) else { continue };
            let editable = doc.is_effectively_visible(id) && !doc.is_effectively_locked(id);
            let mut fields = Vec::new();
            for field in node_fields(n) {
                if !field.replaceable() {
                    continue;
                }
                let Some(text) = node_field_text(n, &field) else {
                    continue;
                };
                let count = query.ranges(&text).len();
                if count == 0 {
                    continue;
                }
                if editable {
                    fields.push(field);
                } else {
                    report.skipped += count;
                }
            }
            if !fields.is_empty() {
                node_edits.push((id, fields));
            }
        }
        let keys: Vec<String> = doc.settings.metadata.keys().cloned().collect();
        for key in keys {
            let Some(text) = doc.settings.metadata.get(&key) else {
                continue;
            };
            if !query.ranges(text).is_empty() {
                page_edits.push(FindField::PageValue(key));
            }
        }
        if node_edits.is_empty() && page_edits.is_empty() {
            continue;
        }
        report.pages += 1;
        let mut replaced = 0usize;
        ed.with_page(page, |ed| {
            ed.edit("Replace all", |tx| {
                for (id, fields) in &node_edits {
                    let Some(n) = tx.node_mut(*id) else { continue };
                    for field in fields {
                        let Some(text) = node_field_text(n, field) else {
                            continue;
                        };
                        let (new, count) = query.replace(&text, replacement);
                        if count > 0 {
                            set_node_field(n, field, new);
                            replaced += count;
                        }
                    }
                }
                if !page_edits.is_empty() {
                    let settings = tx.settings_mut();
                    for field in &page_edits {
                        let FindField::PageValue(key) = field else {
                            continue;
                        };
                        let Some(text) = settings.metadata.get(key).cloned() else {
                            continue;
                        };
                        let (new, count) = query.replace(&text, replacement);
                        if count > 0 {
                            settings.metadata.insert(key.clone(), new);
                            replaced += count;
                        }
                    }
                }
            });
        });
        report.replaced += replaced;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;
    use crate::geom::Rect;
    use crate::model::{NodeId, NodeKind};

    fn editor() -> (Editor, NodeId) {
        let mut ed = Editor::default();
        let id = ed.doc.alloc_id();
        let mut a = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 100.0, 60.0));
        a.label = "Alpha Beta".into();
        a.tooltip = "beta tooltip".into();
        a.tags = vec!["beta tag".into()];
        a.metadata.insert("owner".into(), "Beta team".into());
        ed.doc.nodes.insert(id, a);
        ed.doc.root.push(id);
        ed.doc
            .settings
            .metadata
            .insert("title".into(), "beta page".into());
        (ed, id)
    }

    #[test]
    fn literal_search_is_case_insensitive_by_default() {
        let (ed, _) = editor();
        let q = Query::new("beta", false, false).unwrap();
        let m = search(&ed, &q);
        assert_eq!(m.len(), 5, "{m:#?}");
        assert!(m.iter().any(|m| m.field == FindField::Label));
        assert!(
            m.iter()
                .any(|m| m.field == FindField::MetadataValue("owner".into()))
        );
        assert!(
            m.iter()
                .any(|m| m.field == FindField::PageValue("title".into()))
        );
    }

    #[test]
    fn regex_mode_and_capture_replacement() {
        let (ed, _) = editor();
        let q = Query::new(r"(?i)beta", true, false).unwrap();
        assert_eq!(search(&ed, &q).len(), 5);
        let (out, n) = q.replace("Beta Beta", "[$0]");
        assert_eq!(n, 2);
        assert_eq!(out, "[Beta] [Beta]");
    }

    #[test]
    fn replace_all_is_one_undo_step_and_reports_skipped() {
        let (mut ed, visible) = editor();
        let id = ed.doc.alloc_id();
        let mut hidden = Node::new(id, NodeKind::Rect, Rect::new(0.0, 0.0, 10.0, 10.0));
        hidden.label = "beta hidden".into();
        hidden.visible = false;
        ed.doc.nodes.insert(id, hidden);
        ed.doc.root.push(id);
        let r = replace_all(&mut ed, "beta", "gamma", false, false).unwrap();
        assert_eq!(r.replaced, 5, "{r:?}");
        assert_eq!(r.skipped, 1);
        assert_eq!(r.pages, 1);
        assert_eq!(ed.doc.get(visible).unwrap().label, "Alpha gamma");
        assert_eq!(ed.doc.get(visible).unwrap().tooltip, "gamma tooltip");
        assert_eq!(
            ed.doc.get(visible).unwrap().tags,
            vec!["gamma tag".to_string()]
        );
        assert_eq!(ed.doc.get(visible).unwrap().metadata["owner"], "gamma team");
        assert_eq!(ed.doc.settings.metadata["title"], "gamma page");
        assert_eq!(ed.doc.get(id).unwrap().label, "beta hidden");
        ed.history.undo(&mut ed.doc);
        assert_eq!(ed.doc.settings.metadata["title"], "beta page");
    }

    #[test]
    fn empty_query_is_an_error_and_bad_regex_explains_itself() {
        assert!(Query::new("", false, false).is_err());
        let e = Query::new("(", true, false).unwrap_err();
        assert!(e.contains("Invalid regular expression"));
    }
}
