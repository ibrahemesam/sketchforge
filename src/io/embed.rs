//! Native diagram data carried inside SVG and PNG exports.
//!
//! The payload is the same JSON bundle used for a native project: every page, its camera and
//! its image assets. SVG carries it as base64 text in a `<metadata id="sketch-rs-data">`
//! element; PNG carries it in a tEXt chunk. Both formats still render normally in other apps,
//! and Sketch can reopen the exported file to recover the editable pages.

use std::io::Cursor;

/// Marker read by the importer and by tests.
pub const SVG_MARKER: &str = "sketch-rs-data";
/// Latin-1 tEXt keyword, kept short enough for the PNG spec.
pub const PNG_KEYWORD: &str = "Sketch-rs data";

fn encode(json: &str) -> String {
    crate::assets::base64(json.as_bytes())
}

fn decode(text: &str) -> Option<String> {
    let bytes = crate::assets::base64_decode(text.trim())?;
    String::from_utf8(bytes).ok()
}

/// Insert the bundle into an exported SVG, just after the root element opens.
pub fn svg_with_data(svg: &str, json: &str) -> String {
    let Some(end) = svg.find('>') else {
        return svg.to_string();
    };
    let mut out = String::with_capacity(svg.len() + json.len() * 2 + 64);
    out.push_str(&svg[..=end]);
    out.push_str("<metadata id=\"");
    out.push_str(SVG_MARKER);
    out.push_str("\">");
    out.push_str(&encode(json));
    out.push_str("</metadata>");
    out.push_str(&svg[end + 1..]);
    out
}

/// Read a bundle from an SVG, returning `None` for files exported without one.
pub fn svg_data(svg: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(svg).ok()?;
    let node = doc
        .descendants()
        .find(|n| n.tag_name().name() == "metadata" && n.attribute("id") == Some(SVG_MARKER))?;
    decode(node.text()?)
}

/// Encode RGBA pixels as PNG with a native bundle in a tEXt chunk.
pub fn png_with_data(img: &image::RgbaImage, json: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, img.width(), img.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk(PNG_KEYWORD.to_string(), encode(json))
            .map_err(|e| format!("could not add PNG data: {e}"))?;
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("could not write PNG header: {e}"))?;
        writer
            .write_image_data(img.as_raw())
            .map_err(|e| format!("could not write PNG pixels: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("could not finish PNG: {e}"))?;
    }
    Ok(out)
}

/// Read a bundle from PNG bytes, returning `None` for ordinary images.
pub fn png_data(bytes: &[u8]) -> Option<String> {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let reader = decoder.read_info().ok()?;
    let info = reader.info();
    let text = info
        .uncompressed_latin1_text
        .iter()
        .find(|c| c.keyword == PNG_KEYWORD)?;
    decode(&text.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_bundle_survives_metadata_roundtrip() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10"/></svg>"#;
        let json = r#"{"format":"sketch-rs-bundle","pages":["one"]}"#;
        let out = svg_with_data(svg, json);
        assert!(out.contains(SVG_MARKER));
        assert!(out.contains("<rect"));
        assert_eq!(svg_data(&out).as_deref(), Some(json));
        assert_eq!(svg_data(svg), None);
    }

    #[test]
    fn png_bundle_survives_text_chunk_roundtrip() {
        let img = image::RgbaImage::from_fn(3, 2, |x, y| {
            image::Rgba([(x * 80) as u8, (y * 100) as u8, 40, 255])
        });
        let json = r#"{"format":"sketch-rs-bundle","pages":["one","two"]}"#;
        let bytes = png_with_data(&img, json).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), img.as_raw());
        assert_eq!(png_data(&bytes).as_deref(), Some(json));

        let mut plain = std::io::Cursor::new(Vec::new());
        img.write_to(&mut plain, image::ImageFormat::Png).unwrap();
        assert_eq!(png_data(&plain.into_inner()), None);
    }
}
