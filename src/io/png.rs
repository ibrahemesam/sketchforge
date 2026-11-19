//! PNG export through the offscreen GPU renderer.

use super::svg::export_bounds_for_page;
use crate::geom::{Camera, V2, v2};
use crate::model::{Color, ColorMode, Document, NodeId};
use crate::render::gpu::Headless;
use crate::render::scene::{self, View};
use std::borrow::Cow;

/// Largest exported edge, in pixels.
pub const MAX_EDGE: f64 = 16384.0;

/// Render `roots` (or all top-level nodes) at `scale` px per unit in light mode.
pub fn render(
    gpu: &Headless,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
) -> Result<image::RgbaImage, String> {
    render_with_mode(
        gpu,
        doc,
        roots,
        scale,
        background,
        ColorMode::light(doc.settings.adaptive_colors),
    )
}

/// Render one baked appearance, resolving the document's dark colors first.
pub fn render_with_mode(
    gpu: &Headless,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
    mode: ColorMode,
) -> Result<image::RgbaImage, String> {
    let baked: Cow<'_, Document> = if mode.is_dark() {
        Cow::Owned(doc.with_appearance(mode))
    } else {
        Cow::Borrowed(doc)
    };
    let doc = baked.as_ref();
    let roots: Vec<NodeId> = roots
        .map(|r| r.to_vec())
        .unwrap_or_else(|| doc.layered_root());
    let b = export_bounds_for_page(doc, &roots).ok_or("nothing to export")?;
    let scale = scale.min(MAX_EDGE / b.w.max(b.h)).max(1e-3);
    let size = [
        (b.w * scale).ceil().max(1.0) as u32,
        (b.h * scale).ceil().max(1.0) as u32,
    ];
    // Tessellate at physical resolution: the camera zoom carries the scale.
    let v = View {
        doc,
        camera: Camera {
            pan: b.min(),
            zoom: scale,
        },
        viewport: v2(size[0] as f64, size[1] as f64),
        offset: V2::ZERO,
    };
    let frame = scene::export_frame(doc, Some(&roots), v.camera, v.viewport, None);
    let clear = match background {
        Some(c) => {
            let f = c.to_f32(1.0);
            [f[0] as f64, f[1] as f64, f[2] as f64, f[3] as f64]
        }
        None => [0.0; 4],
    };
    let mut img = gpu.render_frame(&frame, size, 1.0, clear);
    if background.is_none() {
        unpremultiply_edges(&mut img);
    }
    Ok(img)
}

/// The GPU blends into a transparent target with straight-alpha blending, which leaves color
/// channels multiplied by coverage at anti-aliased edges. Undo that so PNGs composite cleanly.
fn unpremultiply_edges(img: &mut image::RgbaImage) {
    for p in img.pixels_mut() {
        let a = p.0[3];
        if a > 0 && a < 255 {
            for c in 0..3 {
                p.0[c] = ((p.0[c] as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            }
        }
    }
}

pub fn export(
    path: &std::path::Path,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
) -> Result<(u32, u32), String> {
    export_with_data(path, doc, roots, scale, background, None)
}

/// Write a PNG, optionally carrying a native page bundle in a tEXt chunk.
pub fn export_with_data(
    path: &std::path::Path,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
    data: Option<&str>,
) -> Result<(u32, u32), String> {
    export_with_data_mode(
        path,
        doc,
        roots,
        scale,
        background,
        data,
        ColorMode::light(doc.settings.adaptive_colors),
    )
}

/// Write a PNG in one baked appearance, optionally carrying native page data.
pub fn export_with_data_mode(
    path: &std::path::Path,
    doc: &Document,
    roots: Option<&[NodeId]>,
    scale: f64,
    background: Option<Color>,
    data: Option<&str>,
    mode: ColorMode,
) -> Result<(u32, u32), String> {
    let gpu = Headless::new()?;
    let img = render_with_mode(&gpu, doc, roots, scale, background, mode)?;
    let bytes = match data {
        Some(json) => super::embed::png_with_data(&img, json)?,
        None => {
            let mut buf = std::io::Cursor::new(Vec::new());
            img.write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| e.to_string())?;
            buf.into_inner()
        }
    };
    super::atomic::write(path, &bytes).map_err(|e| e.to_string())?;
    Ok(img.dimensions())
}
