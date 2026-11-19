//! draw.io-style placeholders in labels and tooltips.
//!
//! Placeholders are `%name%` tokens resolved when the diagram is drawn or
//! exported. A shape's own property wins over the closest container's, which
//! wins over the page's property. A missing value stays visible as
//! `«name?»` instead of disappearing, so an empty label never hides the typo.
//! The raw text stays in the document, so editing and native saving are not
//! affected.

use super::{Document, Node, NodeKind};
use std::borrow::Cow;
use std::time::{SystemTime, UNIX_EPOCH};

/// Resolve every placeholder in `text` for `node` on `doc`.
pub fn resolve_text(text: &str, node: Option<&Node>, doc: &Document) -> String {
    resolve_text_at(text, node, doc, now_secs())
}

/// Resolve placeholders with a fixed clock, for tests and reproducible exports.
pub fn resolve_text_at(text: &str, node: Option<&Node>, doc: &Document, unix_secs: i64) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '%' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // `%%` prints one literal percent sign (draw.io's escape).
        if i + 1 < chars.len() && chars[i + 1] == '%' {
            out.push('%');
            i += 2;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|j| chars[*j] == '%') else {
            out.extend(chars[i..].iter());
            break;
        };
        let name: String = chars[i + 1..close].iter().collect();
        if !placeholder_syntax(&name) {
            // Not placeholder-shaped (for example "50% done and 20% more"):
            // keep the percent sign and keep scanning.
            out.push('%');
            i += 1;
            continue;
        }
        match resolve_one(&name, node, doc, unix_secs) {
            Some(v) => out.push_str(&v),
            None => {
                out.push('«');
                out.push_str(&name);
                out.push_str("?»");
            }
        }
        i = close + 1;
    }
    out
}

/// A copy of `node` with its label, text, frame title and extra labels
/// resolved. Borrows the original when no field could contain a placeholder.
pub fn resolve_node<'a>(node: &'a Node, doc: &Document) -> Cow<'a, Node> {
    let has = node.label.contains('%')
        || node.tooltip.contains('%')
        || node.name.contains('%')
        || node.kind.text().is_some_and(|t| t.contains('%'))
        || node.labels.iter().any(|l| l.text.contains('%'));
    if !has {
        return Cow::Borrowed(node);
    }
    let now = now_secs();
    let mut c = node.clone();
    c.label = resolve_text_at(&c.label, Some(node), doc, now);
    if let Some(text) = c.kind.text_mut() {
        *text = resolve_text_at(text, Some(node), doc, now);
    }
    if matches!(c.kind, NodeKind::Frame { .. }) {
        c.name = resolve_text_at(&c.name, Some(node), doc, now);
    }
    for label in &mut c.labels {
        label.text = resolve_text_at(&label.text, Some(node), doc, now);
    }
    Cow::Owned(c)
}

fn placeholder_syntax(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && name.chars().count() <= 120
        && name.chars().all(|c| {
            c.is_alphanumeric()
                || matches!(
                    c,
                    '_' | '-' | '.' | ':' | '{' | '}' | '+' | ' ' | '/' | '\\' | ',' | ';'
                )
        })
}

fn resolve_one(name: &str, node: Option<&Node>, doc: &Document, unix_secs: i64) -> Option<String> {
    // Shape and container properties first, then page properties.
    if let Some(n) = node
        && let Some(v) = metadata_scope(n, doc, name)
    {
        return Some(v);
    }
    if let Some(v) = doc.settings.metadata.get(name) {
        return Some(v.clone());
    }
    if let Some(rest) = name.strip_prefix("date{").and_then(|r| r.strip_suffix('}')) {
        return Some(format_timestamp(unix_secs, rest));
    }
    if let Some(rest) = name.strip_prefix("date:").or_else(|| {
        name.strip_prefix("timestamp{")
            .and_then(|r| r.strip_suffix('}'))
    }) {
        return Some(format_timestamp(unix_secs, rest));
    }
    match name {
        "id" => Some(node.map(|n| n.id.0.to_string()).unwrap_or_default()),
        "width" => Some(fmt_num(node?.rect.w)),
        "height" => Some(fmt_num(node?.rect.h)),
        "length" => Some(fmt_num(connector_length(node?))),
        "date" => Some(format_timestamp(unix_secs, "yyyy-mm-dd")),
        "time" => Some(format_timestamp(unix_secs, "HH:MM:ss")),
        "timestamp" => Some(format_timestamp(unix_secs, "yyyy-mm-dd HH:MM:ss")),
        "page" => Some(doc.context.page_name.clone()),
        "filename" => Some(doc.context.file_name.clone()),
        _ => resolve_expression(name, node, doc),
    }
}

/// Shape property, nearest ancestor property, then page property.
fn metadata_scope(node: &Node, doc: &Document, name: &str) -> Option<String> {
    if let Some(v) = node.metadata.get(name) {
        return Some(v.clone());
    }
    let mut cur = node.parent;
    let mut guard = 0;
    while let Some(id) = cur {
        if let Some(n) = doc.get(id)
            && let Some(v) = n.metadata.get(name)
        {
            return Some(v.clone());
        }
        cur = doc.get(id).and_then(|n| n.parent);
        guard += 1;
        if guard > 256 {
            break;
        }
    }
    doc.settings.metadata.get(name).cloned()
}

fn resolve_expression(name: &str, node: Option<&Node>, doc: &Document) -> Option<String> {
    if let Some(unit) = name.strip_prefix("width:") {
        return dimension(node?.rect.w, unit);
    }
    if let Some(unit) = name.strip_prefix("height:") {
        return dimension(node?.rect.h, unit);
    }
    if let Some(unit) = name.strip_prefix("length:") {
        return dimension(connector_length(node?), unit);
    }
    let (base, offset) = split_offset(name);
    match base {
        "pagenumber" => Some((doc.context.page_number as i64 + offset).max(0).to_string()),
        "pagecount" => Some((doc.context.page_count as i64 + offset).max(0).to_string()),
        _ => None,
    }
}

/// `%pagenumber-1%`, `%pagecount+2%`.
fn split_offset(name: &str) -> (&str, i64) {
    let bytes = name.as_bytes();
    if let Some(i) = bytes.iter().position(|b| *b == b'+' || *b == b'-') {
        let (base, rest) = name.split_at(i);
        if let Ok(v) = rest.parse::<i64>() {
            return (base, v);
        }
    }
    (name, 0)
}

fn dimension(v: f64, unit: &str) -> Option<String> {
    let unit = unit.trim();
    let factor: f64 = match unit {
        "" | "current" | "pt" | "px" => 1.0,
        "in" => 1.0 / 72.0,
        "mm" => 25.4 / 72.0,
        "cm" => 2.54 / 72.0,
        "m" => 0.0254 / 72.0,
        _ => return None,
    };
    if (factor - 1.0).abs() < f64::EPSILON {
        Some(fmt_num(v))
    } else {
        Some(format!("{:.2}", v * factor))
    }
}

fn connector_length(node: &Node) -> f64 {
    let NodeKind::Connector { points, .. } = &node.kind else {
        return 0.0;
    };
    points.windows(2).map(|w| w[0].dist(w[1])).sum()
}

fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 100.0).round() / 100.0;
    if r.fract() == 0.0 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

struct Stamp {
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millis: u32,
    weekday: usize,
}

fn stamp(unix_secs: i64) -> Stamp {
    let days = unix_secs.div_euclid(86_400);
    let rem = unix_secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    if month <= 2 {
        year += 1;
    }
    Stamp {
        year,
        month,
        day,
        hour: (rem / 3_600) as u32,
        minute: (rem % 3_600 / 60) as u32,
        second: (rem % 60) as u32,
        millis: 0,
        weekday: (days + 4).rem_euclid(7) as usize,
    }
}

/// Format a timestamp with the draw.io mask tokens used most often. The
/// clock is UTC; the app stores and displays no local timezone database.
pub fn format_timestamp(unix_secs: i64, mask: &str) -> String {
    let mask = match mask.trim() {
        "default" => return format_timestamp(unix_secs, "ddd mmm dd yyyy HH:MM:ss"),
        "shortDate" => "m/d/yy",
        "mediumDate" => "mmm d, yyyy",
        "longDate" => "mmmm d, yyyy",
        "fullDate" => "dddd, mmmm d, yyyy",
        "shortTime" => "h:MM TT",
        "mediumTime" => "h:MM:ss TT",
        "longTime" => "h:MM:ss TT Z",
        "isoDate" => "yyyy-mm-dd",
        "isoTime" => "HH:MM:ss",
        "isoDateTime" => "yyyy-mm-dd'T'HH:MM:ss",
        "isoUtcDateTime" => "yyyy-mm-dd'T'HH:MM:ss'Z'",
        other => other,
    };
    let s = stamp(unix_secs);
    let chars: Vec<char> = mask.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // Quoted literal text, e.g. 'T' or 'Z'.
        if chars[i] == '\'' || chars[i] == '"' {
            let quote = chars[i];
            i += 1;
            while i < chars.len() && chars[i] != quote {
                out.push(chars[i]);
                i += 1;
            }
            i += usize::from(i < chars.len());
            continue;
        }
        let rest: String = chars[i..].iter().collect();
        let token = [
            "yyyy", "yy", "mmmm", "mmm", "mm", "m", "dddd", "ddd", "dd", "d", "HH", "H", "hh", "h",
            "MM", "M", "ss", "s", "l", "L", "tt", "TT", "t", "T", "o", "Z", "S",
        ]
        .into_iter()
        .find(|t| rest.starts_with(t));
        let Some(token) = token else {
            out.push(chars[i]);
            i += 1;
            continue;
        };
        i += token.chars().count();
        let hour12 = match s.hour % 12 {
            0 => 12,
            h => h,
        };
        match token {
            "yyyy" => out.push_str(&format!("{:04}", s.year)),
            "yy" => out.push_str(&format!("{:02}", s.year.rem_euclid(100))),
            "mmmm" => out.push_str(MONTHS[(s.month - 1) as usize]),
            "mmm" => out.push_str(&MONTHS[(s.month - 1) as usize][..3]),
            "mm" => out.push_str(&format!("{:02}", s.month)),
            "m" => out.push_str(&s.month.to_string()),
            "dddd" => out.push_str(WEEKDAYS[s.weekday]),
            "ddd" => out.push_str(&WEEKDAYS[s.weekday][..3]),
            "dd" => out.push_str(&format!("{:02}", s.day)),
            "d" => out.push_str(&s.day.to_string()),
            "HH" => out.push_str(&format!("{:02}", s.hour)),
            "H" => out.push_str(&s.hour.to_string()),
            "hh" => out.push_str(&format!("{hour12:02}")),
            "h" => out.push_str(&hour12.to_string()),
            "MM" => out.push_str(&format!("{:02}", s.minute)),
            "M" => out.push_str(&s.minute.to_string()),
            "ss" => out.push_str(&format!("{:02}", s.second)),
            "s" => out.push_str(&s.second.to_string()),
            "l" => out.push_str(&format!("{:03}", s.millis)),
            "L" => out.push_str(&format!("{:02}", s.millis / 10)),
            "tt" => out.push_str(if s.hour < 12 { "am" } else { "pm" }),
            "TT" => out.push_str(if s.hour < 12 { "AM" } else { "PM" }),
            "t" => out.push(if s.hour < 12 { 'a' } else { 'p' }),
            "T" => out.push(if s.hour < 12 { 'A' } else { 'P' }),
            "o" => out.push_str("+0000"),
            "Z" => out.push_str("UTC"),
            "S" => out.push_str(ordinal(s.day)),
            _ => out.push_str(token),
        }
    }
    out
}

fn ordinal(day: u32) -> &'static str {
    match day % 100 {
        11..=13 => "th",
        _ => match day % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::model::{NodeId, NodeKind};

    fn doc() -> Document {
        let mut d = Document::default();
        d.settings
            .metadata
            .insert("owner".into(), "page owner".into());
        d.settings.metadata.insert("name".into(), "page".into());
        d.context = super::super::PageContext::new(2, 4, "Detail", "board");
        d
    }

    fn shape(label: &str) -> Node {
        let mut n = Node::new(NodeId(7), NodeKind::Rect, Rect::new(0.0, 0.0, 120.0, 60.0));
        n.label = label.into();
        n
    }

    #[test]
    fn shape_value_beats_page_value_and_missing_values_stay_visible() {
        let d = doc();
        let mut n = shape("%name% / %owner% / %bogus%");
        n.metadata.insert("name".into(), "shape".into());
        assert_eq!(
            resolve_text_at(&n.label, Some(&n), &d, 0),
            "shape / page owner / «bogus?»"
        );
    }

    #[test]
    fn nearest_container_wins_and_percent_escapes() {
        let mut d = doc();
        let parent = Node::new(NodeId(1), NodeKind::Group, Rect::new(0.0, 0.0, 10.0, 10.0));
        let mut child = shape("%unit%");
        child.parent = Some(parent.id);
        let mut p = parent.clone();
        p.metadata.insert("unit".into(), "parent unit".into());
        d.nodes.insert(p.id, p);
        d.nodes.insert(child.id, child.clone());
        assert_eq!(
            resolve_text_at("%unit%", Some(&child), &d, 0),
            "parent unit"
        );
        assert_eq!(resolve_text_at("%%unit%", Some(&child), &d, 0), "%unit%");
    }

    #[test]
    fn predefined_page_geometry_and_date_values() {
        let d = doc();
        let n = shape("p %pagenumber%/%pagecount% %page% %filename% %id% %width%pt");
        assert_eq!(
            resolve_text_at(&n.label, Some(&n), &d, 0),
            "p 2/4 Detail board 7 120pt"
        );
        assert_eq!(resolve_text_at("%pagenumber+1%", Some(&n), &d, 0), "3");
        assert_eq!(resolve_text_at("%pagecount-2%", Some(&n), &d, 0), "2");
        assert_eq!(resolve_text_at("%date%", None, &d, 0), "1970-01-01");
        assert_eq!(
            resolve_text_at("%date{isoDate}%", None, &d, 0),
            "1970-01-01"
        );
        assert_eq!(
            resolve_text_at("%date{dddd, mmmm d, yyyy}%", None, &d, 0),
            "Thursday, January 1, 1970"
        );
        assert_eq!(resolve_text_at("%width:mm%", Some(&n), &d, 0), "42.33");
    }

    #[test]
    fn ordinary_percent_text_is_left_alone() {
        let d = doc();
        let n = shape("50% done and 20% more");
        assert_eq!(
            resolve_text_at(&n.label, Some(&n), &d, 0),
            "50% done and 20% more"
        );
    }

    #[test]
    fn resolve_node_rewrites_labels_and_text_but_not_tooltips() {
        let mut d = doc();
        let mut n = shape("%name%");
        n.tooltip = "%owner%".into();
        n.metadata.insert("name".into(), "shape".into());
        let r = resolve_node(&n, &d);
        assert_eq!(r.label, "shape");
        assert_eq!(r.tooltip, "%owner%");
        let mut t = Node::new(
            NodeId(8),
            NodeKind::Text {
                text: "%owner%".into(),
                font_size: 16.0,
                auto_width: true,
                align: crate::model::TextAlign::Left,
            },
            Rect::new(0.0, 0.0, 50.0, 20.0),
        );
        t.metadata.insert("owner".into(), "me".into());
        d.nodes.insert(t.id, t.clone());
        assert_eq!(resolve_node(&t, &d).kind.text().unwrap(), "me");
    }
}
