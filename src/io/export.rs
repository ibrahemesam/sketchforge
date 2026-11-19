//! Export of selections, frames and pages to PNG, SVG and PDF (shared by the app, the CLI and
//! tests).

use crate::model::{Color, ColorMode, Document, NodeId};
use crate::render::gpu::Headless;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Png,
    Svg,
    Pdf,
    /// A draw.io `mxfile` containing every page of the project.
    Drawio,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Svg => "svg",
            Format::Pdf => "pdf",
            Format::Drawio => "drawio",
        }
    }
    pub const ALL: [Format; 4] = [Format::Png, Format::Svg, Format::Pdf, Format::Drawio];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// The selected layers (or the whole page when nothing is selected).
    #[default]
    Selection,
    /// Each top-level frame (or each selected frame) separately.
    Frames,
    /// Everything on the current page.
    Page,
    /// Every page of the project.
    AllPages,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Selection => "Selection",
            Scope::Frames => "Each frame",
            Scope::Page => "Page",
            Scope::AllPages => "All pages",
        }
    }
    pub const ALL: [Scope; 4] = [
        Scope::Selection,
        Scope::Frames,
        Scope::Page,
        Scope::AllPages,
    ];
}

/// Which appearance a raster/vector export bakes into the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        self == Appearance::Dark
    }

    pub fn label(self) -> &'static str {
        match self {
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    pub format: Format,
    pub scope: Scope,
    /// PNG pixel scale.
    pub scale: f64,
    pub transparent: bool,
    /// Which appearance baked colors use. draw.io XML keeps both values instead.
    pub appearance: Appearance,
    /// Carry every page inside PNG/SVG metadata so the file can be reopened natively.
    pub embed_data: bool,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            format: Format::Png,
            scope: Scope::Selection,
            scale: 2.0,
            transparent: false,
            appearance: Appearance::Light,
            embed_data: false,
        }
    }
}

impl Format {
    /// Which formats can carry a native page bundle.
    pub fn can_embed(self) -> bool {
        matches!(self, Format::Png | Format::Svg)
    }
}

impl Spec {
    /// One output file, or a folder of files?
    pub fn single_file(&self, items: usize) -> bool {
        self.format == Format::Pdf || self.format == Format::Drawio || items <= 1
    }
}

/// The document with baked colors, its effective page mode and the page background.
fn appearance_doc<'a>(
    doc: &'a Document,
    spec: &Spec,
) -> (Cow<'a, Document>, ColorMode, Option<Color>) {
    let mode = ColorMode::new(doc.settings.adaptive_colors, spec.appearance.is_dark());
    let draw_doc = if mode.is_dark() {
        Cow::Owned(doc.with_appearance(mode))
    } else {
        Cow::Borrowed(doc)
    };
    let authored_bg = if doc.settings.background_enabled {
        doc.settings.background_color
    } else {
        Color::WHITE
    };
    let bg =
        (!spec.transparent).then(|| mode.resolve(authored_bg, doc.settings.background_color_dark));
    (draw_doc, mode, bg)
}

/// One thing to export: a document and the roots to draw, with a file-friendly name.
pub struct Item<'a> {
    pub name: String,
    pub doc: &'a Document,
    pub roots: Vec<NodeId>,
}

/// Resolve what a spec exports, given the pages (`(name, doc)`), the active page index and
/// the selection on it.
pub fn items<'a>(
    spec: &Spec,
    pages: &[(&'a str, &'a Document)],
    active: usize,
    selection: &[NodeId],
) -> Vec<Item<'a>> {
    let (pname, doc) = pages[active];
    let frames_of = |doc: &'a Document, among: &[NodeId]| -> Vec<Item<'a>> {
        among
            .iter()
            .filter(|id| doc.get(**id).is_some_and(|n| n.is_frame() && n.visible))
            .map(|id| Item {
                name: doc.get(*id).unwrap().name.clone(),
                doc,
                roots: vec![*id],
            })
            .collect()
    };
    match spec.scope {
        Scope::Selection if !selection.is_empty() => vec![Item {
            name: if selection.len() == 1 {
                doc.get(selection[0])
                    .map_or(pname.to_string(), |n| n.name.clone())
            } else {
                pname.to_string()
            },
            doc,
            roots: selection.to_vec(),
        }],
        Scope::Selection | Scope::Page => vec![Item {
            name: pname.to_string(),
            doc,
            roots: doc.layered_root(),
        }],
        Scope::Frames => {
            let sel_frames = frames_of(doc, selection);
            if !sel_frames.is_empty() {
                sel_frames
            } else {
                frames_of(doc, &doc.layered_root())
            }
        }
        Scope::AllPages => pages
            .iter()
            .map(|(n, d)| Item {
                name: n.to_string(),
                doc: d,
                roots: d.layered_root(),
            })
            .collect(),
    }
}

fn safe(name: &str) -> String {
    let s = crate::project::folder_name(name);
    if s.is_empty() { "export".into() } else { s }
}

/// Write the export. `out` is a file (single output) or a folder (several). Returns the
/// files written.
pub fn write(
    spec: &Spec,
    items: &[Item],
    out: &Path,
    gpu: Option<&Headless>,
) -> Result<Vec<PathBuf>, String> {
    write_with_data(spec, items, out, gpu, None)
}

/// Write an export, embedding `data` in PNG/SVG files when the spec asks for it.
pub fn write_with_data(
    spec: &Spec,
    items: &[Item],
    out: &Path,
    gpu: Option<&Headless>,
    data: Option<&str>,
) -> Result<Vec<PathBuf>, String> {
    if spec.embed_data && spec.format.can_embed() && data.is_none() {
        return Err("no page data to embed".into());
    }
    if items.is_empty() {
        return Err("nothing to export".into());
    }
    if spec.format == Format::Drawio {
        let pages: Vec<(&str, &Document)> =
            items.iter().map(|it| (it.name.as_str(), it.doc)).collect();
        let xml = crate::io::drawio::export(&pages);
        crate::io::atomic::write(out, xml.as_bytes()).map_err(|e| e.to_string())?;
        return Ok(vec![out.to_path_buf()]);
    }
    if spec.format == Format::Pdf {
        let resolved: Vec<(Cow<'_, Document>, Option<Color>)> = items
            .iter()
            .map(|it| {
                let (doc, _, bg) = appearance_doc(it.doc, spec);
                (doc, bg)
            })
            .collect();
        let mut pages = Vec::new();
        for (it, (doc, bg)) in items.iter().zip(&resolved) {
            let frames = spec.scope == Scope::Frames;
            pages.extend(crate::io::pdf::pages_for(
                doc.as_ref(),
                Some(&it.roots),
                frames,
                *bg,
            ));
        }
        if pages.is_empty() {
            return Err("nothing to export".into());
        }
        let bytes = crate::io::pdf::export(&pages);
        crate::io::atomic::write(out, &bytes).map_err(|e| e.to_string())?;
        return Ok(vec![out.to_path_buf()]);
    }
    let single = spec.single_file(items.len());
    let mut written = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for it in items {
        let path = if single {
            out.to_path_buf()
        } else {
            let mut name = safe(&it.name);
            let mut i = 2;
            while used.contains(&name) {
                name = format!("{} {i}", safe(&it.name));
                i += 1;
            }
            used.push(name.clone());
            std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
            out.join(format!("{name}.{}", spec.format.ext()))
        };
        match spec.format {
            Format::Svg => {
                let (draw_doc, _, bg) = appearance_doc(it.doc, spec);
                let mut s = crate::io::svg::export(draw_doc.as_ref(), Some(&it.roots), bg)
                    .ok_or("nothing to export")?;
                if spec.embed_data {
                    s = crate::io::embed::svg_with_data(&s, data.unwrap_or_default());
                }
                crate::io::atomic::write(&path, s.as_bytes()).map_err(|e| e.to_string())?;
            }
            Format::Png => {
                let owned;
                let g = match gpu {
                    Some(g) => g,
                    None => {
                        owned = Headless::new()?;
                        &owned
                    }
                };
                let (_, mode, bg) = appearance_doc(it.doc, spec);
                let img = crate::io::png::render_with_mode(
                    g,
                    it.doc,
                    Some(&it.roots),
                    spec.scale,
                    bg,
                    mode,
                )?;
                let bytes = if spec.embed_data {
                    crate::io::embed::png_with_data(&img, data.unwrap_or_default())?
                } else {
                    let mut buf = std::io::Cursor::new(Vec::new());
                    img.write_to(&mut buf, image::ImageFormat::Png)
                        .map_err(|e| e.to_string())?;
                    buf.into_inner()
                };
                crate::io::atomic::write(&path, &bytes).map_err(|e| e.to_string())?;
            }
            Format::Pdf | Format::Drawio => unreachable!(),
        }
        written.push(path);
    }
    Ok(written)
}
