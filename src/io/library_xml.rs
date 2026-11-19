//! CUS-05: draw.io custom shape library XML (`.xml`) adapter.
//!
//! A draw.io library is an `<mxlibrary>` element whose text is a JSON array of
//! `{ "xml": "&lt;mxGraphModel&gt;...&lt;/mxGraphModel&gt;", "w": "80",
//! "h": "50", "title": "Decision" }` entries. Each `xml` value is one shape's
//! mxGraph model, so this module reuses the ordinary draw.io importer and
//! exporter instead of growing a second shape parser. Imported shapes become
//! `LibItem`s in the same personal library the Library tab already uses.

use crate::library::LibItem;
use serde::{Deserialize, Serialize};

/// One entry in an `<mxlibrary>` JSON array. Draw.io writes sizes as strings;
/// accepting numbers too keeps hand-written libraries usable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    xml: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    w: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tags: Option<String>,
}

/// Deserialize draw.io's size fields whether they are JSON strings or numbers.
fn size_field(v: &serde_json::Value, key: &str) -> Option<String> {
    let v = v.get(key)?;
    match v {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn parse_entries(text: &str) -> Result<Vec<Entry>, String> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(text.trim()).map_err(|e| format!("library is not JSON: {e}"))?;
    arr.iter()
        .map(|v| {
            let mut e = Entry {
                xml: v
                    .get("xml")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                data: v
                    .get("data")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                w: size_field(v, "w"),
                h: size_field(v, "h"),
                title: v
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                tags: v
                    .get("tags")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            };
            // An entry may carry its model in `data` instead of `xml`.
            if e.xml.is_none()
                && e.data
                    .as_deref()
                    .is_some_and(|d| d.trim_start().starts_with('<'))
            {
                e.xml = e.data.take();
            }
            Ok(e)
        })
        .collect()
}

/// Import a draw.io `<mxlibrary>` file into personal library items.
pub fn import_library(xml: &str) -> Result<Vec<LibItem>, String> {
    let parsed =
        roxmltree::Document::parse(xml).map_err(|e| format!("not valid library XML: {e}"))?;
    let root = parsed.root_element();
    if !root.has_tag_name("mxlibrary") {
        return Err(format!(
            "unsupported library root <{}>; expected <mxlibrary>",
            root.tag_name().name()
        ));
    }
    let body = root.children().filter_map(|n| n.text()).collect::<String>();
    let entries = parse_entries(&body)?;
    if entries.is_empty() {
        return Err("the library has no shapes".into());
    }
    let mut items = Vec::with_capacity(entries.len());
    for (i, e) in entries.iter().enumerate() {
        let model = e.xml.as_deref().ok_or_else(|| {
            format!(
                "library entry {} has no uncompressed XML; Sketch cannot import compressed or encrypted shapes",
                i + 1
            )
        })?;
        let (pages, _) = crate::io::drawio::import_str(model)
            .map_err(|err| format!("library entry {}: {err}", i + 1))?;
        let page = pages
            .first()
            .ok_or_else(|| format!("library entry {} has no shape", i + 1))?;
        let name = e
            .title
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| page.doc.name.clone());
        let item = LibItem::from_selection(&page.doc, &page.doc.root, &name)
            .ok_or_else(|| format!("library entry {} has no geometry", i + 1))?;
        items.push(item);
    }
    Ok(items)
}

/// Escape text for the body of `<mxlibrary>`. Quotes are JSON escapes there and
/// must stay literal, so this only handles XML's text-node special characters.
fn escape_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

fn number(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        "0".into()
    }
}

/// Export personal library items as a draw.io `<mxlibrary>` XML file.
pub fn export_library(items: &[LibItem]) -> String {
    let mut entries = Vec::with_capacity(items.len());
    for item in items {
        let doc = item.doc();
        let mxfile = crate::io::drawio::export(&[(&item.name, &doc)]);
        let model = mxfile
            .find("<mxGraphModel")
            .zip(mxfile.find("</mxGraphModel>"))
            .map(|(start, end)| &mxfile[start..end + "</mxGraphModel>".len()])
            .unwrap_or("");
        let bounds = item.bounds().unwrap_or_default();
        entries.push(Entry {
            xml: Some(model.into()),
            data: None,
            w: Some(number(bounds.w)),
            h: Some(number(bounds.h)),
            title: Some(item.name.clone()),
            tags: None,
        });
    }
    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".into());
    let mut out = String::from("<mxlibrary>");
    escape_text(&mut out, &json);
    out.push_str("</mxlibrary>");
    out
}
