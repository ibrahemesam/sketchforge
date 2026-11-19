//! Importing files into projects.

use crate::io::{drawio, embed};
use crate::project::{PageData, Project, format};
use std::path::Path;

/// The result of reading an external file: suggested project name, pages to
/// create, and an optional draw.io fidelity report.
#[derive(Debug)]
pub struct Imported {
    pub name: String,
    pub pages: Vec<PageData>,
    pub report: Option<drawio::ImportReport>,
}

/// Recover pages from an SVG metadata element or PNG tEXt chunk, if one is present.
pub fn embedded_pages(path: &Path) -> Result<Option<format::PageBundle>, String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let json = match ext.as_str() {
        "svg" => {
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            embed::svg_data(&text)
        }
        "png" => {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            embed::png_data(&bytes)
        }
        _ => None,
    };
    match json {
        Some(s) => format::pages_from_json(&s)
            .map(Some)
            .map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

/// Read a file into pages for a new project, with the draw.io report when the
/// file is a draw.io diagram.
pub fn import_detailed(path: &Path) -> Result<Imported, String> {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Imported".into());
    if path.is_dir() {
        let p = Project::open(path).map_err(|e| e.to_string())?;
        let pages = p.load_pages().map_err(|e| e.to_string())?;
        return Ok(Imported {
            name: p.manifest.name,
            pages,
            report: None,
        });
    }
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if let Some((name, pages, _)) = embedded_pages(path)? {
        let name = if name.trim().is_empty() { stem } else { name };
        return Ok(Imported {
            name,
            pages,
            report: None,
        });
    }
    if ext == "png" {
        return Err("PNG has no embedded Sketch data".into());
    }
    if matches!(ext.as_str(), "drawio" | "xml") {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let (pages, report) = drawio::import_str(&text)?;
        let name = if pages.len() == 1 {
            let p = &pages[0];
            if p.name.is_empty() || p.name == "Untitled" || p.name == "Page 1" {
                stem
            } else {
                p.name.clone()
            }
        } else {
            stem
        };
        return Ok(Imported {
            name,
            pages,
            report: Some(report),
        });
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let page = format::page_from_json(&text).map_err(|e| e.to_string())?;
    let name = if page.name.is_empty() || page.name == "Untitled" {
        stem
    } else {
        page.name.clone()
    };
    Ok(Imported {
        name,
        pages: vec![page],
        report: None,
    })
}

/// Read a file into pages for a new project. Returns a suggested project name.
pub fn import_as_pages(path: &Path) -> Result<(String, Vec<PageData>), String> {
    import_detailed(path).map(|i| (i.name, i.pages))
}

/// Load the pages of a project folder or of a single document file (CLI export).
pub fn load_any(path: &Path) -> Result<Vec<PageData>, String> {
    import_as_pages(path).map(|(_, pages)| pages)
}
