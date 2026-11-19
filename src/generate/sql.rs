//! SQL schema to ER diagram import. Each CREATE TABLE becomes one editable
//! table frame with a header, a rule, and one row per column. Column-level
//! REFERENCES and table-level FOREIGN KEY clauses become bound connectors, so
//! relationships move with the tables they join.

use super::{Builder, Generated, INK, MUTED, Report, grid_positions, rect_at};
use crate::geom::{Rect, V2, v2};
use crate::model::Head;

#[derive(Debug, Default, Clone)]
struct Column {
    name: String,
    ty: String,
    pk: bool,
    fk: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct Table {
    name: String,
    columns: Vec<Column>,
    /// (local column, referenced table, referenced column)
    fks: Vec<(String, String, String)>,
}

pub fn parse(text: &str) -> Result<Generated, String> {
    let clean = strip_comments(text);
    let statements = split_statements(&clean);
    let mut tables: Vec<Table> = Vec::new();
    let mut alters: Vec<String> = Vec::new();
    let mut report = Report::default();

    for statement in statements {
        let trimmed = statement.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("create table") {
            let table = parse_create(trimmed)?;
            if tables.iter().any(|t| same_name(&t.name, &table.name)) {
                report.warn(format!(
                    "table \"{}\" is defined twice; the first one is kept",
                    table.name
                ));
                continue;
            }
            tables.push(table);
        } else if lower.starts_with("alter table") {
            alters.push(trimmed.to_string());
        }
    }
    if tables.is_empty() {
        return Err(
            "no CREATE TABLE statements were found; paste a schema such as CREATE TABLE users (id INTEGER)"
                .into(),
        );
    }

    for alter in &alters {
        apply_alter(alter, &mut tables, &mut report);
    }

    let index: std::collections::HashMap<String, usize> = tables
        .iter()
        .enumerate()
        .map(|(i, t)| (normalize(&t.name), i))
        .collect();

    let sizes: Vec<(f64, f64)> = tables
        .iter()
        .map(|t| {
            let chars = t
                .columns
                .iter()
                .map(|c| column_label(c).chars().count())
                .chain(std::iter::once(t.name.chars().count()))
                .max()
                .unwrap_or(8);
            let w = (chars as f64 * 7.2 + 32.0).clamp(190.0, 380.0);
            let h = (44.0 + t.columns.len() as f64 * 24.0).max(64.0);
            (w, h)
        })
        .collect();
    let pos = grid_positions(&sizes);
    let mut frames = Vec::new();

    let mut b = Builder::new("SQL schema");
    for (i, table) in tables.iter().enumerate() {
        let (w, h) = sizes[i];
        let rect = rect_at(pos[i], (w, h));
        let id = b.frame(rect, &table.name);
        b.text(v2(12.0, 10.0), &table.name, 14.0, INK, Some(id));
        b.line(v2(1.0, 38.0), v2(w - 1.0, 38.0), MUTED, 1.0, Some(id));
        for (j, column) in table.columns.iter().enumerate() {
            b.text(
                v2(12.0, 44.0 + j as f64 * 24.0),
                &column_label(column),
                12.0,
                INK,
                Some(id),
            );
        }
        if table.columns.is_empty() {
            report.warn(format!("table \"{}\" has no columns", table.name));
        }
        let _ = h;
        frames.push(id);
    }

    for (i, table) in tables.iter().enumerate() {
        for (column, target, _target_column) in &table.fks {
            let Some(&to) = index.get(&normalize(target)) else {
                report.warn(format!(
                    "\"{}.{column}\" references \"{target}\", which is not in the schema",
                    table.name
                ));
                continue;
            };
            if to == i {
                continue;
            }
            let a = b.world_rect(frames[i]).unwrap_or_default();
            let z = b.world_rect(frames[to]).unwrap_or_default();
            let (hf, ht) = facing(a, z);
            b.link(
                (frames[i], hf),
                (frames[to], ht),
                column,
                false,
                (Head::None, Head::Arrow),
            );
        }
    }
    b.finish("SQL schema", report)
}

fn column_label(column: &Column) -> String {
    let mut label = format!("{}  {}", column.name, column.ty);
    if column.pk {
        label.push_str("  PK");
    }
    if let Some(target) = &column.fk {
        label.push_str("  FK -> ");
        label.push_str(target);
    }
    label
}

/// The two normalized side points that face each other, picked from the
/// relative position of the tables.
fn facing(a: Rect, b: Rect) -> (V2, V2) {
    let dx = b.center().x - a.center().x;
    let dy = b.center().y - a.center().y;
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 {
            (v2(1.0, 0.5), v2(0.0, 0.5))
        } else {
            (v2(0.0, 0.5), v2(1.0, 0.5))
        }
    } else if dy >= 0.0 {
        (v2(0.5, 1.0), v2(0.5, 0.0))
    } else {
        (v2(0.5, 0.0), v2(0.5, 1.0))
    }
}

fn parse_create(statement: &str) -> Result<Table, String> {
    let rest = statement[12..].trim_start();
    let rest = strip_prefix_ci(rest, "if not exists")
        .map(str::trim_start)
        .unwrap_or(rest);
    let open = find_outside(rest, '(').ok_or_else(|| {
        format!(
            "CREATE TABLE is missing its column list: \"{}\"",
            head(statement)
        )
    })?;
    let name = clean_name(&rest[..open]);
    if name.is_empty() {
        return Err("CREATE TABLE is missing a table name".into());
    }
    let close = matching_paren(rest, open)
        .ok_or_else(|| format!("unbalanced parentheses in CREATE TABLE \"{name}\""))?;
    let body = &rest[open + 1..close];
    let mut table = Table {
        name,
        ..Default::default()
    };
    for part in split_top(body, ',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let upper = part.to_ascii_uppercase();
        if upper.starts_with("CONSTRAINT")
            || upper.starts_with("FOREIGN KEY")
            || upper.starts_with("PRIMARY KEY")
            || upper.starts_with("UNIQUE")
            || upper.starts_with("CHECK")
            || upper.starts_with("KEY ")
            || upper.starts_with("INDEX")
        {
            if let Some((column, target, target_column)) = parse_foreign_key(part) {
                table.fks.push((column, target, target_column));
            }
            continue;
        }
        let Some((column_name, after)) = take_identifier(part) else {
            continue;
        };
        let column_name = last_part(&column_name);
        let ty = column_type(after);
        let pk = upper.contains("PRIMARY KEY");
        let fk = parse_references(after).map(|(target, _)| target);
        let referenced = fk.clone();
        if let Some(target) = referenced {
            let target_column = parse_references(after)
                .and_then(|(_, col)| col)
                .unwrap_or_default();
            table.fks.push((column_name.clone(), target, target_column));
        }
        table.columns.push(Column {
            name: column_name,
            ty,
            pk,
            fk,
        });
    }
    Ok(table)
}

/// `FOREIGN KEY (col) REFERENCES other (col)`.
fn parse_foreign_key(part: &str) -> Option<(String, String, String)> {
    let upper = part.to_ascii_uppercase();
    let at = upper.find("FOREIGN KEY")?;
    let after = &part[at + "FOREIGN KEY".len()..];
    let open = after.find('(')?;
    let close = after[open..].find(')')? + open;
    let column = last_part(after[open + 1..close].trim());
    let (target, target_column) = parse_references(&after[close + 1..])?;
    Some((column, target, target_column.unwrap_or_default()))
}

fn parse_references(s: &str) -> Option<(String, Option<String>)> {
    let upper = s.to_ascii_uppercase();
    let at = upper.find("REFERENCES")?;
    let after = s[at + "REFERENCES".len()..].trim_start();
    let (qualified, rest) = take_identifier(after)?;
    let target = last_part(&qualified);
    let rest = rest.trim_start();
    let column = rest.strip_prefix('(').and_then(|r| {
        let close = r.find(')')?;
        Some(last_part(r[..close].trim()))
    });
    Some((target, column))
}

fn column_type(s: &str) -> String {
    let upper = s.to_ascii_uppercase();
    let stop = [
        "PRIMARY KEY",
        "NOT NULL",
        "REFERENCES",
        "UNIQUE",
        "DEFAULT",
        "CHECK",
        "CONSTRAINT",
        "COLLATE",
        "GENERATED",
        "AUTOINCREMENT",
    ]
    .iter()
    .filter_map(|k| upper.find(k))
    .min()
    .unwrap_or(s.len());
    let ty = s[..stop].trim();
    if ty.is_empty() {
        "TEXT".into()
    } else {
        ty.to_string()
    }
}

fn apply_alter(statement: &str, tables: &mut [Table], report: &mut Report) {
    let rest = statement[11..].trim_start();
    let Some((name, after)) = take_identifier(rest) else {
        return;
    };
    let name = last_part(&name);
    let Some(table) = tables.iter_mut().find(|t| same_name(&t.name, &name)) else {
        report.warn(format!("ALTER TABLE names unknown table \"{name}\""));
        return;
    };
    if let Some((column, target, target_column)) = parse_foreign_key(after) {
        if let Some(c) = table
            .columns
            .iter_mut()
            .find(|c| same_name(&c.name, &column))
        {
            c.fk = Some(target.clone());
        }
        table.fks.push((column, target, target_column));
    }
}

fn head(statement: &str) -> String {
    let line = statement.lines().next().unwrap_or("").trim();
    line.chars().take(60).collect()
}

fn same_name(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase()
}

fn last_part(name: &str) -> String {
    name.rsplit('.')
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches(['"', '`', '[', ']'])
        .to_string()
}

fn clean_name(s: &str) -> String {
    let s = s.trim();
    let name = match take_identifier(s) {
        Some((name, _)) => name,
        None => s.to_string(),
    };
    last_part(&name)
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&s[prefix.len()..])
    } else {
        None
    }
}

/// Read a possibly quoted, possibly schema-qualified identifier.
fn take_identifier(s: &str) -> Option<(String, &str)> {
    let mut rest = s.trim_start();
    let mut parts = Vec::new();
    loop {
        let (part, after) = take_one(rest)?;
        parts.push(part);
        rest = after.trim_start();
        match rest.strip_prefix('.') {
            Some(next) => rest = next.trim_start(),
            None => break,
        }
    }
    Some((parts.join("."), rest))
}

fn take_one(s: &str) -> Option<(String, &str)> {
    let indexed: Vec<(usize, char)> = s.char_indices().collect();
    let (_, first) = *indexed.first()?;
    if let Some(close) = closer(first) {
        let mut out = String::new();
        let mut k = 1usize;
        while let Some(&(at, c)) = indexed.get(k) {
            if c == close {
                if close != ']' && indexed.get(k + 1).map(|x| x.1) == Some(close) {
                    out.push(close);
                    k += 2;
                    continue;
                }
                return Some((out, &s[at + c.len_utf8()..]));
            }
            out.push(c);
            k += 1;
        }
        return None;
    }
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || matches!(c, '_' | '$' | '#')))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    Some((s[..end].to_string(), &s[end..]))
}

fn closer(open: char) -> Option<char> {
    match open {
        '"' => Some('"'),
        '\'' => Some('\''),
        '`' => Some('`'),
        '[' => Some(']'),
        _ => None,
    }
}

fn find_outside(s: &str, needle: char) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => {
                if let Some(close) = closer(c) {
                    quote = Some(close);
                } else if c == needle {
                    return Some(i);
                }
            }
        }
    }
    None
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => {
                if let Some(close) = closer(c) {
                    quote = Some(close);
                } else {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth = depth.saturating_sub(1);
                            if depth == 0 {
                                return Some(i);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    None
}

fn split_top(s: &str, separator: char) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                current.push(c);
            }
            Some(_) => current.push(c),
            None => {
                if let Some(close) = closer(c) {
                    quote = Some(close);
                    current.push(c);
                } else {
                    match c {
                        '(' => {
                            depth += 1;
                            current.push(c);
                        }
                        ')' => {
                            depth = depth.saturating_sub(1);
                            current.push(c);
                        }
                        c if c == separator && depth == 0 => {
                            out.push(std::mem::take(&mut current));
                        }
                        _ => current.push(c),
                    }
                }
            }
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '-' && chars.peek() == Some(&'-') {
            while let Some(&n) = chars.peek() {
                if n == '\n' {
                    break;
                }
                chars.next();
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            while let Some(n) = chars.next() {
                if n == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    break;
                }
                if n == '\n' {
                    out.push('\n');
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn split_statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for c in text.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                current.push(c);
            }
            Some(_) => current.push(c),
            None => {
                if let Some(close) = closer(c) {
                    quote = Some(close);
                    current.push(c);
                } else if c == ';' {
                    out.push(std::mem::take(&mut current));
                } else {
                    current.push(c);
                }
            }
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    const SCHEMA: &str = "\
-- a tiny schema
CREATE TABLE teams (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL
);
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  display_name VARCHAR(120),
  team_id INTEGER REFERENCES teams(id),
  created_at TIMESTAMP DEFAULT now()
);
";

    #[test]
    fn a_schema_becomes_tables_with_columns_and_one_relationship() {
        let g = parse(SCHEMA).unwrap();
        let frames = g
            .doc
            .root
            .iter()
            .filter(|id| matches!(g.doc.get(**id).unwrap().kind, NodeKind::Frame { .. }))
            .count();
        assert_eq!(frames, 2);
        assert!(g.report.warnings.is_empty(), "{:?}", g.report.warnings);
        let text: Vec<String> = g
            .doc
            .nodes
            .values()
            .filter_map(|n| n.kind.text().cloned())
            .collect();
        assert!(
            text.iter()
                .any(|t| t.contains("display_name") && t.contains("VARCHAR(120)"))
        );
        assert!(text.iter().any(|t| t.contains("id") && t.contains("PK")));
        let connectors: Vec<_> = g
            .doc
            .nodes
            .values()
            .filter_map(|n| match &n.kind {
                NodeKind::Connector { start, end, .. } => Some((n, start, end)),
                _ => None,
            })
            .collect();
        assert_eq!(connectors.len(), 1);
        assert_eq!(connectors[0].0.label, "team_id");
        assert!(connectors[0].1.is_some() && connectors[0].2.is_some());
        assert!(g.doc.validate().is_ok());
    }

    #[test]
    fn a_table_level_foreign_key_and_alter_add_edges() {
        let text = "\
CREATE TABLE a (id INTEGER PRIMARY KEY, b_id INTEGER);
CREATE TABLE b (id INTEGER PRIMARY KEY);
ALTER TABLE a ADD CONSTRAINT fk FOREIGN KEY (b_id) REFERENCES b(id);
";
        let g = parse(text).unwrap();
        let labels: Vec<String> = g
            .doc
            .nodes
            .values()
            .filter(|n| n.is_connector())
            .map(|n| n.label.clone())
            .collect();
        assert_eq!(labels, vec!["b_id".to_string()]);
    }

    #[test]
    fn a_missing_reference_is_reported_and_dropped() {
        let g = parse("CREATE TABLE a (id INTEGER, gone_id INTEGER REFERENCES gone(id));").unwrap();
        assert_eq!(g.report.warnings.len(), 1);
        assert!(g.report.summary().contains("not in the schema"));
        assert_eq!(g.doc.nodes.values().filter(|n| n.is_connector()).count(), 0);
    }

    #[test]
    fn non_sql_text_and_unbalanced_parens_fail_with_a_clear_message() {
        let err = parse("hello world;").unwrap_err();
        assert!(err.contains("no CREATE TABLE"), "{err}");
        let err = parse("CREATE TABLE broken (id INTEGER;").unwrap_err();
        assert!(err.contains("unbalanced parentheses"), "{err}");
    }

    #[test]
    fn the_same_schema_lays_out_identically() {
        let a = parse(SCHEMA).unwrap();
        let b = parse(SCHEMA).unwrap();
        let mut ra: Vec<_> = a.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        let mut rb: Vec<_> = b.doc.nodes.iter().map(|(id, n)| (*id, n.rect)).collect();
        ra.sort_by_key(|(id, _)| *id);
        rb.sort_by_key(|(id, _)| *id);
        assert_eq!(ra, rb);
    }
}
