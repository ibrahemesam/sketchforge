//! Text layout and glyph outlines.
//!
//! Text is rendered as vector glyph outlines, so it stays crisp at every zoom level and
//! exports to SVG without font dependencies. Latin text uses Inter; Arabic text falls back
//! to Noto Sans Arabic and is shaped into Presentation Forms before it is laid out right to
//! left. Both fonts are SIL OFL and bundled in assets/fonts, not anything taken from Paper.

mod arabic;
mod fonts;

pub use fonts::library_epoch;

use crate::geom::{Rect, V2, v2};
use crate::model::{CharStyle, Color, ListKind, TextAlign, TextStyle};
use crate::vector::VPath;
use lyon::path::Path;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub static FONT_BYTES: &[u8] = crate::theme::INTER_REGULAR;
static BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
static ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-Italic.ttf");
static BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-BoldItalic.ttf");
static ARABIC: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf");
static ARABIC_BOLD: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Bold.ttf");

/// Number of built-in faces. Faces 0-3 are Inter, 4-7 are Noto Sans Arabic.
/// System font families load into the dynamic registry at ids 8 and up.
pub const FACE_COUNT: u8 = 8;

/// Face index: bits 0-1 pick Inter's weight/slant, bit 2 picks the Arabic
/// family; ids ≥ FACE_COUNT index the dynamic system-font registry.
pub type FaceId = u16;

/// Faces registered at runtime from system font files, in registration order.
/// Each face's bytes and the face itself are leaked once; they live for the
/// process lifetime and are never mutated afterwards.
static DYN_FACES: Mutex<Vec<&'static ttf_parser::Face<'static>>> = Mutex::new(Vec::new());

/// Cached per-family fragmentation: [regular, bold, italic, bold italic].
static FAMILY_CACHE: std::sync::LazyLock<Mutex<HashMap<String, Option<[FaceId; 4]>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

thread_local! {
    /// Faces of the family currently being laid out. Set for the duration of a
    /// layout call, consulted by `face_for_char` and the metric helpers.
    static FAMILY_FACES: std::cell::RefCell<Option<[FaceId; 4]>> =
        const { std::cell::RefCell::new(None) };
}

/// Restore the previous family override when a layout call ends.
struct FamilyScope(Option<[FaceId; 4]>);

impl Drop for FamilyScope {
    fn drop(&mut self) {
        FAMILY_FACES.with(|c| *c.borrow_mut() = self.0);
    }
}

/// Activate `ts`'s font family for the duration of a layout call.
fn scope_family(ts: &TextStyle) -> FamilyScope {
    let resolved = ts.family.as_deref().and_then(resolve_family);
    let prev = FAMILY_FACES.with(|c| c.borrow_mut().take());
    FAMILY_FACES.with(|c| *c.borrow_mut() = resolved);
    FamilyScope(prev)
}
/// Register one face from file bytes; returns its dynamic FaceId.
fn dyn_face(bytes: &[u8], index: u32) -> Option<FaceId> {
    let leaked: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());
    let face = ttf_parser::Face::parse(leaked, index).ok()?;
    let face: &'static ttf_parser::Face<'static> = Box::leak(Box::new(face));
    let mut v = DYN_FACES.lock().unwrap();
    v.push(face);
    Some(FACE_COUNT as FaceId + (v.len() - 1) as FaceId)
}

/// Faces of a font family: regular, bold, italic and bold italic. Missing
/// styles fall back to whatever face the family ships (as in most editors).
///
/// A miss is not cached: the background scan may simply not have finished, and
/// caching that None would pin the fallback font for the whole session (the
/// restart bug where the panel showed the picked family but the canvas kept
/// rendering Inter). A miss after the scan costs one linear lookup per layout
/// call, which is nothing next to shaping.
pub fn resolve_family(name: &str) -> Option<[FaceId; 4]> {
    let key = name.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Some(hit) = FAMILY_CACHE.lock().unwrap().get(&key) {
        return *hit;
    }
    let res = compute_family(name);
    if res.is_some() {
        FAMILY_CACHE.lock().unwrap().insert(key, res);
    }
    res
}

fn compute_family(name: &str) -> Option<[FaceId; 4]> {
    if name.eq_ignore_ascii_case("Inter") {
        return Some([0, 1, 2, 3]);
    }
    fonts::scan_once();
    let lib = fonts::library()?;
    let fam = lib.find(name)?;
    let mut slots: [Option<FaceId>; 4] = [None; 4];
    for f in &fam.faces {
        let s = (f.bold as usize) | ((f.italic as usize) << 1);
        if slots[s].is_some() {
            continue;
        }
        let Ok(bytes) = std::fs::read(&f.file) else {
            continue;
        };
        if let Some(id) = dyn_face(&bytes, f.index) {
            slots[s] = Some(id);
        }
    }
    // No face parsed, or the family carries nothing usable.
    let reg = slots[0].or(slots[1]).or(slots[2]).or(slots[3]);
    let reg = reg?;
    for s in &mut slots {
        if s.is_none() {
            *s = Some(reg);
        }
    }
    Some([
        slots[0].unwrap(),
        slots[1].unwrap(),
        slots[2].unwrap(),
        slots[3].unwrap(),
    ])
}

#[cfg(test)]
pub fn install_test_library(dirs: &[std::path::PathBuf]) {
    fonts::install_test_library(dirs);
}

/// Families offered by the inspector's font dropdown. Inter is always first;
/// system families appear as the background scan finds them.
pub fn font_families() -> Vec<String> {
    fonts::scan_once();
    let mut names = vec!["Inter".to_string()];
    if let Some(lib) = fonts::library() {
        for f in &lib.families {
            if !f.name.eq_ignore_ascii_case("Inter") {
                names.push(f.name.clone());
            }
        }
    }
    names
}

/// Font availability and faces, as reported by MCP's `get_font_family_info`.
pub struct FamilyInfo {
    pub available: bool,
    /// `(weight, style)` pairs in draw.io's order: regular, bold, italic, then
    /// bold italic. A family with one face reports just that face's style.
    pub styles: Vec<(u16, &'static str)>,
}

/// Report whether `name` can render and which of its faces the app found.
/// Inter is bundled with all four styles. Other families come from the same
/// background scan that fills the inspector dropdown, so a call before the
/// scan lands reports them unavailable rather than claiming a false fallback.
pub fn family_info(name: &str) -> FamilyInfo {
    fonts::scan_once();
    let styles = styles_from(fonts::library(), name);
    FamilyInfo {
        available: !styles.is_empty(),
        styles,
    }
}

fn styles_from(lib: Option<&fonts::Library>, name: &str) -> Vec<(u16, &'static str)> {
    if name.trim().eq_ignore_ascii_case("Inter") {
        return vec![
            (400, "normal"),
            (700, "normal"),
            (400, "italic"),
            (700, "italic"),
        ];
    }
    let Some(family) = lib.and_then(|l| l.find(name.trim())) else {
        return Vec::new();
    };
    let mut found = [[false; 2]; 2]; // [italic][bold]
    for face in &family.faces {
        found[face.italic as usize][face.bold as usize] = true;
    }
    [
        (false, false, 400, "normal"),
        (true, false, 700, "normal"),
        (false, true, 400, "italic"),
        (true, true, 700, "italic"),
    ]
    .into_iter()
    .filter(|(bold, italic, _, _)| found[*italic as usize][*bold as usize])
    .map(|(_, _, weight, style)| (weight, style))
    .collect()
}

pub fn face_id(bold: bool, italic: bool) -> FaceId {
    bold as FaceId | (italic as FaceId) << 1
}

/// Arabic has no italic face; both bold and italic requests pick regular or bold.
pub fn arabic_face_id(bold: bool) -> FaceId {
    4 | bold as FaceId
}

fn face_for_char(c: char, bold: bool, italic: bool) -> FaceId {
    if arabic::is_arabic(c) {
        return arabic_face_id(bold);
    }
    let i = (bold as usize) | ((italic as usize) << 1);
    FAMILY_FACES
        .with(|c| *c.borrow())
        .and_then(|f| f.get(i).copied())
        .unwrap_or_else(|| face_id(bold, italic))
}

pub fn face_bytes(id: FaceId) -> &'static [u8] {
    debug_assert!(
        (id as usize) < FACE_COUNT as usize,
        "bundled faces only serve ids 0..{FACE_COUNT}"
    );
    match id {
        0 => FONT_BYTES,
        1 => BOLD,
        2 => ITALIC,
        3 => BOLD_ITALIC,
        4 | 6 => ARABIC,
        _ => ARABIC_BOLD,
    }
}

pub fn face_of(id: FaceId) -> &'static ttf_parser::Face<'static> {
    if (id as usize) >= FACE_COUNT as usize {
        // A registered system face; fall back to Inter if the id is stale.
        return DYN_FACES
            .lock()
            .unwrap()
            .get(id as usize - FACE_COUNT as usize)
            .copied()
            .unwrap_or(face());
    }
    static FACES: OnceLock<[ttf_parser::Face<'static>; FACE_COUNT as usize]> = OnceLock::new();
    &FACES.get_or_init(|| {
        (0..FACE_COUNT)
            .map(|i| {
                ttf_parser::Face::parse(face_bytes(i as FaceId), 0).expect("bundled font parses")
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("one face per id")
    })[id as usize]
}

pub fn face() -> &'static ttf_parser::Face<'static> {
    face_of(0)
}

fn units_per_em() -> f64 {
    face().units_per_em() as f64
}

/// The face metrics come from: the family currently being laid out when one
/// is active, otherwise Inter. Glyph advances already use per-char faces;
/// these helpers place lines and baselines.
fn metrics_face() -> &'static ttf_parser::Face<'static> {
    FAMILY_FACES
        .with(|c| *c.borrow())
        .map(|f| face_of(f[0]))
        .unwrap_or(face())
}

pub fn line_height(font_size: f64) -> f64 {
    let f = metrics_face();
    let h = (f.ascender() as f64 - f.descender() as f64 + f.line_gap() as f64)
        / f.units_per_em() as f64;
    font_size * h.max(1.1)
}

pub fn ascent(font_size: f64) -> f64 {
    let f = metrics_face();
    f.ascender() as f64 / f.units_per_em() as f64 * font_size
}

#[derive(Debug, Clone, Copy)]
pub struct PlacedGlyph {
    pub gid: u16,
    pub face: FaceId,
    pub size: f64,
    pub color: Color,
    /// Pen position of the glyph origin (baseline), relative to the text box.
    pub x: f64,
    pub baseline: f64,
    pub advance: f64,
    /// Char index into the source string this glyph comes from.
    pub char_index: usize,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub top: f64,
    pub height: f64,
    pub baseline: f64,
    pub width: f64,
    /// Char index range `[start, end)`; `end` excludes the line break.
    pub start: usize,
    pub end: usize,
    /// Left x of each char boundary in the line (len = end - start + 1).
    pub carets: Vec<f64>,
}

#[derive(Debug, Clone)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub lines: Vec<Line>,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
}

fn advance_in(face: FaceId, c: char, font_size: f64) -> (u16, f64) {
    let f = face_of(face);
    let gid = f
        .glyph_index(c)
        .or_else(|| f.glyph_index('?'))
        .map(|g| g.0)
        .unwrap_or(0);
    let adv = f.glyph_hor_advance(ttf_parser::GlyphId(gid)).unwrap_or(0) as f64;
    (gid, adv / f.units_per_em() as f64 * font_size)
}

#[cfg(test)]
fn advance(c: char, font_size: f64) -> (u16, f64) {
    advance_in(0, c, font_size)
}

/// Lay out plain text with optional wrapping at `max_width`.
pub fn layout(text: &str, font_size: f64, max_width: Option<f64>, align: TextAlign) -> TextLayout {
    layout_rich(
        text,
        font_size,
        Color::INK,
        &TextStyle::default(),
        max_width,
        align,
    )
}

/// Lay out text whose characters may each have their own weight, slant, size and color.
pub fn layout_rich(
    text: &str,
    font_size: f64,
    color: Color,
    ts: &TextStyle,
    max_width: Option<f64>,
    align: TextAlign,
) -> TextLayout {
    let _fam = scope_family(ts);
    if arabic::has_arabic(text) {
        return layout_bidi(text, font_size, color, ts, max_width, align, None);
    }
    let chars: Vec<char> = text.chars().collect();
    let styles: Vec<CharStyle> = (0..chars.len())
        .map(|i| ts.char_style(i, font_size, color))
        .collect();
    let ls = ts.letter_spacing;
    let adv: Vec<(u16, f64)> = chars
        .iter()
        .zip(&styles)
        .map(|(c, s)| {
            let (g, a) = advance_in(face_for_char(*c, s.bold, s.italic), *c, s.draw_size());
            (g, a + ls)
        })
        .collect();
    let lh_of = |size: f64| match ts.line_height {
        Some(m) => size * m,
        None => line_height(size),
    };
    // Break into lines: hard breaks on '\n', soft breaks at spaces when wrapping.
    let mut ranges: Vec<(usize, usize, usize, bool)> = Vec::new();
    let mut start = 0;
    let mut para_idx = 0usize;
    while start <= chars.len() {
        let hard_end = chars[start..]
            .iter()
            .position(|c| *c == '\n')
            .map_or(chars.len(), |p| start + p);
        let mut line_start = start;
        let mut first_of_para = true;
        if let Some(maxw) = max_width {
            let mut x = 0.0;
            let mut last_space: Option<usize> = None;
            let mut i = line_start;
            while i < hard_end {
                let a = adv[i].1;
                if chars[i] == ' ' {
                    last_space = Some(i);
                }
                if x + a > maxw && i > line_start {
                    let brk = match last_space {
                        Some(s) if s > line_start => s,
                        _ => i,
                    };
                    let skip_space = chars.get(brk) == Some(&' ');
                    ranges.push((line_start, brk, para_idx, first_of_para));
                    first_of_para = false;
                    line_start = if skip_space { brk + 1 } else { brk };
                    i = line_start;
                    x = 0.0;
                    last_space = None;
                    continue;
                }
                x += a;
                i += 1;
            }
        }
        ranges.push((line_start, hard_end, para_idx, first_of_para));
        start = hard_end + 1;
        para_idx += 1;
    }

    let mut lines = Vec::new();
    let mut glyphs = Vec::new();
    let mut max_line_w: f64 = 0.0;
    let mut top = 0.0;
    let mut number = 0usize;
    for (s, e, para_idx, first_of_para) in ranges.iter().copied() {
        // Line metrics from the largest size on the line (or the char before an empty line).
        let sizes = styles[s..e].iter().map(|c| c.size);
        let max_size = sizes
            .clone()
            .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.max(z))))
            .unwrap_or_else(|| {
                styles
                    .get(s.saturating_sub(1))
                    .map_or(font_size, |c| c.size)
            });
        let lh = lh_of(max_size);
        let natural = line_height(max_size);
        let baseline = top + ascent(max_size) + (lh - natural) * 0.5;
        let para = ts.paragraphs.get(para_idx);
        let indent = para.map_or(0.0, |p| p.indent as f64 * font_size * 1.5);
        let list = para.map_or(ListKind::None, |p| p.list);
        if first_of_para && list == ListKind::Number {
            number += 1;
        }
        let marker = if first_of_para {
            match list {
                ListKind::Bullet => Some("•".to_string()),
                ListKind::Number => Some(format!("{number}.")),
                ListKind::None => None,
            }
        } else {
            None
        };
        let mut x = indent;
        if let Some(marker) = marker {
            for ch in marker.chars() {
                let face = face_for_char(ch, ts.bold, ts.italic);
                let (gid, advance) = advance_in(face, ch, max_size);
                glyphs.push(PlacedGlyph {
                    gid,
                    face,
                    size: max_size,
                    color,
                    x,
                    baseline,
                    advance,
                    char_index: s,
                    underline: false,
                    strike: false,
                });
                x += advance;
            }
            x += font_size * 0.35;
        }
        let mut carets = vec![x];
        for i in s..e {
            let st = styles[i];
            let (gid, a) = adv[i];
            glyphs.push(PlacedGlyph {
                gid,
                face: face_for_char(chars[i], st.bold, st.italic),
                size: st.draw_size(),
                color: st.color,
                x,
                baseline: baseline + st.baseline_shift(),
                advance: a,
                char_index: i,
                underline: st.underline,
                strike: st.strike,
            });
            x += a;
            carets.push(x);
        }
        max_line_w = max_line_w.max(x);
        lines.push(Line {
            top,
            height: lh,
            baseline,
            width: x,
            start: s,
            end: e,
            carets,
        });
        top += lh;
    }
    let box_w = max_width.unwrap_or(max_line_w);
    if align != TextAlign::Left {
        let k = if align == TextAlign::Center { 0.5 } else { 1.0 };
        for line in &mut lines {
            let dx = (box_w - line.width) * k;
            for c in &mut line.carets {
                *c += dx;
            }
            for g in glyphs
                .iter_mut()
                .filter(|g| g.char_index >= line.start && g.char_index < line.end)
            {
                g.x += dx;
            }
        }
    }
    TextLayout {
        glyphs,
        height: top,
        lines,
        width: max_line_w,
        font_size,
    }
}

struct LayoutCell {
    ch: char,
    gid: u16,
    face: FaceId,
    size: f64,
    draw_size: f64,
    baseline_shift: f64,
    color: Color,
    advance: f64,
    underline: bool,
    strike: bool,
    char_start: usize,
    char_end: usize,
    newline: bool,
}

/// Layout a line of mixed Arabic and Latin text. Arabic is shaped first, then
/// unicode-bidi puts each line's cells in visual order and the caret table is
/// built from the visual cells instead of logical order.
fn layout_bidi(
    text: &str,
    font_size: f64,
    color: Color,
    ts: &TextStyle,
    max_width: Option<f64>,
    align: TextAlign,
    para_dir: Option<unicode_bidi::Level>,
) -> TextLayout {
    let _fam = scope_family(ts);
    let chars: Vec<char> = text.chars().collect();
    let styles: Vec<CharStyle> = (0..chars.len())
        .map(|i| ts.char_style(i, font_size, color))
        .collect();
    let shaped = arabic::shape(&chars);
    let mut cells = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            cells.push(LayoutCell {
                ch: c,
                gid: 0,
                face: face_id(false, false),
                size: styles[i].size,
                draw_size: styles[i].draw_size(),
                baseline_shift: 0.0,
                color: styles[i].color,
                advance: 0.0,
                underline: false,
                strike: false,
                char_start: i,
                char_end: i + 1,
                newline: true,
            });
            i += 1;
            continue;
        }
        let Some(shaped) = shaped.get(i).copied().flatten() else {
            i += 1;
            continue;
        };
        let st = styles[i];
        let face = face_for_char(shaped.ch, st.bold, st.italic);
        let (gid, adv) = advance_in(face, shaped.ch, st.draw_size());
        // Arabic letters join, so letter spacing would tear the strokes apart.
        let spacing = if arabic::is_arabic(shaped.ch) {
            0.0
        } else {
            ts.letter_spacing
        };
        let end = (i + shaped.len).min(chars.len());
        cells.push(LayoutCell {
            ch: shaped.ch,
            gid,
            face,
            size: st.size,
            draw_size: st.draw_size(),
            baseline_shift: st.baseline_shift(),
            color: st.color,
            advance: adv + spacing,
            underline: st.underline,
            strike: st.strike,
            char_start: i,
            char_end: end,
            newline: false,
        });
        i = end;
    }

    let lh_of = |size: f64| match ts.line_height {
        Some(m) => size * m,
        None => line_height(size),
    };
    // Break cells into lines: hard breaks on newlines, soft breaks at spaces.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    while start <= cells.len() {
        let hard_end = cells[start..]
            .iter()
            .position(|c| c.newline)
            .map_or(cells.len(), |p| start + p);
        let mut line_start = start;
        if let Some(maxw) = max_width {
            let mut x = 0.0;
            let mut last_space: Option<usize> = None;
            let mut i = line_start;
            while i < hard_end {
                let a = cells[i].advance;
                if cells[i].ch == ' ' {
                    last_space = Some(i);
                }
                if x + a > maxw && i > line_start {
                    let brk = match last_space {
                        Some(s) if s > line_start => s,
                        _ => i,
                    };
                    let skip_space = cells.get(brk).is_some_and(|c| c.ch == ' ');
                    ranges.push((line_start, brk));
                    line_start = if skip_space { brk + 1 } else { brk };
                    i = line_start;
                    x = 0.0;
                    last_space = None;
                    continue;
                }
                x += a;
                i += 1;
            }
        }
        ranges.push((line_start, hard_end));
        start = hard_end + 1;
    }

    let mut lines = Vec::new();
    let mut line_glyphs = Vec::new();
    let mut max_line_w: f64 = 0.0;
    let mut top = 0.0;
    for (s, e) in ranges.iter().copied() {
        let (char_start, char_end) = if s < e {
            (cells[s].char_start, cells[e - 1].char_end)
        } else if s < cells.len() {
            (cells[s].char_start, cells[s].char_start)
        } else {
            (chars.len(), chars.len())
        };
        let max_size = cells[s..e]
            .iter()
            .map(|c| c.size)
            .fold(None, |m: Option<f64>, z| Some(m.map_or(z, |m| m.max(z))))
            .unwrap_or_else(|| cells.get(s.saturating_sub(1)).map_or(font_size, |c| c.size));
        let lh = lh_of(max_size);
        let natural = line_height(max_size);
        let baseline = top + ascent(max_size) + (lh - natural) * 0.5;

        let line_text0: String = chars[char_start..char_end].iter().collect();
        let mut visual: Vec<(usize, bool)> = Vec::new();
        if !line_text0.is_empty() {
            // An explicit Writing Direction must steer every bidi rule, not
            // just the paragraph default (which only applies without a strong
            // character). A zero-width strong prefix makes the first strong
            // character, and therefore the whole resolution, follow it.
            let prefix = match para_dir {
                Some(l) if l.is_rtl() => "\u{200F}",
                Some(l) if l.is_ltr() => "\u{200E}",
                _ => "",
            };
            let pre = prefix.chars().count();
            let line_text = if pre > 0 {
                format!("{prefix}{line_text0}")
            } else {
                line_text0.clone()
            };
            let bidi = unicode_bidi::BidiInfo::new(&line_text, None);
            if let Some(para) = bidi.paragraphs.first() {
                let (levels, runs) = bidi.visual_runs(para, 0..bidi.text.len());
                let mut byte_starts = Vec::with_capacity(line_text.chars().count() + 1);
                let mut b = 0;
                for ch in line_text.chars() {
                    byte_starts.push(b);
                    b += ch.len_utf8();
                }
                byte_starts.push(b);
                let char_at = |b: usize| {
                    byte_starts
                        .iter()
                        .position(|&x| x == b)
                        .unwrap_or(byte_starts.len() - 1)
                };
                let mut used = vec![false; e - s];
                for run in runs {
                    let level = levels
                        .get(run.start)
                        .copied()
                        .unwrap_or(unicode_bidi::Level::ltr());
                    // Bidi indices include the zero-width prefix; drop it
                    // when mapping to real cells.
                    let a = char_at(run.start).saturating_sub(pre);
                    let b = char_at(run.end).saturating_sub(pre).max(a);
                    let mut run_cells = Vec::new();
                    for (k, ci) in (s..e).enumerate() {
                        if used[k] {
                            continue;
                        }
                        let c0 = cells[ci].char_start - char_start;
                        let c1 = cells[ci].char_end - char_start;
                        if c0 >= a && c1 <= b {
                            used[k] = true;
                            run_cells.push((ci, level.is_rtl()));
                        }
                    }
                    if level.is_rtl() {
                        run_cells.reverse();
                    }
                    visual.extend(run_cells);
                }
                for (k, ci) in (s..e).enumerate() {
                    if !used[k] {
                        visual.push((ci, false));
                    }
                }
            }
        }
        if visual.is_empty() {
            for ci in s..e {
                visual.push((ci, false));
            }
        }

        let mut carets = vec![f64::NAN; char_end - char_start + 1];
        let mut glyphs = Vec::new();
        let mut x = 0.0;
        for (ci, rtl) in visual {
            let cell = &cells[ci];
            let c0 = cell.char_start - char_start;
            let c1 = cell.char_end - char_start;
            let mut set = |k: usize, v: f64| {
                if carets[k].is_nan() {
                    carets[k] = v;
                }
            };
            if rtl {
                set(c1, x);
                set(c0, x + cell.advance);
                for k in (c0 + 1)..c1 {
                    set(k, x);
                }
            } else {
                set(c0, x);
                set(c1, x + cell.advance);
                for k in (c0 + 1)..c1 {
                    set(k, x + cell.advance);
                }
            }
            if !cell.newline {
                glyphs.push(PlacedGlyph {
                    gid: cell.gid,
                    face: cell.face,
                    size: cell.draw_size,
                    color: cell.color,
                    x,
                    baseline: baseline + cell.baseline_shift,
                    advance: cell.advance,
                    char_index: cell.char_start,
                    underline: cell.underline,
                    strike: cell.strike,
                });
            }
            x += cell.advance;
        }
        let mut last = 0.0;
        for c in &mut carets {
            if c.is_nan() {
                *c = last;
            } else {
                last = *c;
            }
        }
        max_line_w = max_line_w.max(x);
        lines.push(Line {
            top,
            height: lh,
            baseline,
            width: x,
            start: char_start,
            end: char_end,
            carets,
        });
        line_glyphs.push(glyphs);
        top += lh;
    }
    let box_w = max_width.unwrap_or(max_line_w);
    if align != TextAlign::Left {
        let k = if align == TextAlign::Center { 0.5 } else { 1.0 };
        for (line, glyphs) in lines.iter_mut().zip(line_glyphs.iter_mut()) {
            let dx = (box_w - line.width) * k;
            for c in &mut line.carets {
                *c += dx;
            }
            for g in glyphs {
                g.x += dx;
            }
        }
    }
    TextLayout {
        glyphs: line_glyphs.into_iter().flatten().collect(),
        height: top,
        lines,
        width: max_line_w,
        font_size,
    }
}

impl TextLayout {
    /// Caret (x, top, height) for a char index into the source string.
    pub fn caret(&self, index: usize) -> (f64, f64, f64) {
        let i = self.line_of(index);
        let l = &self.lines[i];
        let k = index.clamp(l.start, l.end) - l.start;
        (l.carets[k], l.top, l.height)
    }

    /// Index of the line holding char boundary `index`.
    pub fn line_of(&self, index: usize) -> usize {
        for (i, l) in self.lines.iter().enumerate() {
            let next_start = self.lines.get(i + 1).map(|n| n.start);
            if index >= l.start && index <= l.end && next_start.is_none_or(|ns| index < ns) {
                return i;
            }
        }
        self.lines.len() - 1
    }

    /// Highlight rectangles (x, top, w, h) for the char range `[a, b)`.
    pub fn selection_rects(&self, a: usize, b: usize) -> Vec<(f64, f64, f64, f64)> {
        let (a, b) = (a.min(b), a.max(b));
        let mut out = Vec::new();
        for l in &self.lines {
            let s = a.max(l.start);
            let e = b.min(l.end);
            if s > e || (s == e && !(a < l.start && b > l.end)) {
                continue;
            }
            let x0 = l.carets[s - l.start];
            let x1 = l.carets[e - l.start];
            let extra = if b > l.end && l.end < usize::MAX {
                4.0
            } else {
                0.0
            };
            out.push((
                x0.min(x1),
                l.top,
                (x1 - x0).abs().max(1.0) + extra,
                l.height,
            ));
        }
        out
    }

    /// Char index nearest to a local point.
    pub fn hit(&self, p: V2) -> usize {
        let li = self
            .lines
            .iter()
            .position(|l| p.y < l.top + l.height)
            .unwrap_or(self.lines.len() - 1);
        let l = &self.lines[li];
        let mut best = 0;
        for (i, x) in l.carets.iter().enumerate() {
            if (x - p.x).abs() < (l.carets[best] - p.x).abs() {
                best = i;
            }
        }
        l.start + best
    }
}

/// A glyph tessellated once in font units (y up).
pub struct GlyphMesh {
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

pub fn glyph_mesh(face: FaceId, gid: u16) -> Arc<GlyphMesh> {
    type Cache = Mutex<HashMap<(FaceId, u16), Arc<GlyphMesh>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(m) = cache.lock().unwrap().get(&(face, gid)) {
        return m.clone();
    }
    let mesh = Arc::new(tessellate_glyph(face, gid));
    cache.lock().unwrap().insert((face, gid), mesh.clone());
    mesh
}

fn tessellate_glyph(face: FaceId, gid: u16) -> GlyphMesh {
    let mut b = LyonOutline {
        b: Path::builder(),
        open: false,
    };
    face_of(face).outline_glyph(ttf_parser::GlyphId(gid), &mut b);
    if b.open {
        b.b.end(true);
    }
    let path = b.b.build();
    let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let tol = (units_per_em() / 2000.0) as f32;
    let _ = FillTessellator::new().tessellate_path(
        &path,
        &FillOptions::tolerance(tol).with_fill_rule(lyon::tessellation::FillRule::NonZero),
        &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| v.position().to_array()),
    );
    GlyphMesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

struct LyonOutline {
    b: lyon::path::path::Builder,
    open: bool,
}

impl ttf_parser::OutlineBuilder for LyonOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        if self.open {
            self.b.end(true);
        }
        self.b.begin(lyon::math::point(x, y));
        self.open = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.b.line_to(lyon::math::point(x, y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.b
            .quadratic_bezier_to(lyon::math::point(x1, y1), lyon::math::point(x, y));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.b.cubic_bezier_to(
            lyon::math::point(x1, y1),
            lyon::math::point(x2, y2),
            lyon::math::point(x, y),
        );
    }
    fn close(&mut self) {
        self.b.end(true);
        self.open = false;
    }
}

/// Outline of a glyph placed at `origin` (baseline) with `font_size`, y down.
pub fn glyph_path(face: FaceId, gid: u16, origin: V2, font_size: f64) -> VPath {
    struct B {
        p: VPath,
        o: V2,
        s: f64,
    }
    impl B {
        fn t(&self, x: f32, y: f32) -> V2 {
            self.o + v2(x as f64 * self.s, -(y as f64) * self.s)
        }
    }
    impl ttf_parser::OutlineBuilder for B {
        fn move_to(&mut self, x: f32, y: f32) {
            let q = self.t(x, y);
            self.p.move_to(q);
        }
        fn line_to(&mut self, x: f32, y: f32) {
            let q = self.t(x, y);
            self.p.line_to(q);
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            let (a, b) = (self.t(x1, y1), self.t(x, y));
            self.p.quad_to(a, b);
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            let (a, b, c) = (self.t(x1, y1), self.t(x2, y2), self.t(x, y));
            self.p.cubic_to(a, b, c);
        }
        fn close(&mut self) {
            self.p.close();
        }
    }
    let f = face_of(face);
    let mut b = B {
        p: VPath::new(),
        o: origin,
        s: font_size / f.units_per_em() as f64,
    };
    f.outline_glyph(ttf_parser::GlyphId(gid), &mut b);
    b.p
}

/// SVG path data for a glyph (regular face).
pub fn glyph_svg_path(gid: u16, origin: V2, font_size: f64) -> String {
    glyph_path(0, gid, origin, font_size).to_svg()
}

/// Inner padding of sticky notes.
pub const STICKY_PAD: f64 = 16.0;

/// Font size of draw.io-style shape labels.
pub const LABEL_FONT_SIZE: f64 = 16.0;

/// Font size and gap above the frame for its editable title.
pub const FRAME_TITLE_SIZE: f64 = 12.0;
pub const FRAME_TITLE_GAP: f64 = 20.0;

/// Horizontal/vertical padding kept between a shape label and its shape.
const LABEL_PAD: f64 = 8.0;

/// Width of a wrapped connector label column, as in draw.io's edge labels.
pub const CONNECTOR_LABEL_WIDTH: f64 = 80.0;

/// Color of a label: an explicit override, black/white against the shape fill,
/// or the line color for connectors.
pub fn label_color(n: &crate::model::Node) -> Color {
    n.label_color.unwrap_or_else(|| {
        if n.is_connector() {
            n.style.stroke.unwrap_or(Color::INK)
        } else {
            n.style.fill().map_or(Color::INK, |c| c.contrasting_text())
        }
    })
}

/// Where a node's text box starts inside the node.
pub fn text_offset(n: &crate::model::Node) -> V2 {
    match n.kind {
        crate::model::NodeKind::Sticky { .. } => v2(STICKY_PAD, STICKY_PAD),
        _ => V2::ZERO,
    }
}

/// Top-left of a node's laid-out text, including label centering.
pub fn text_origin(n: &crate::model::Node, l: &TextLayout) -> V2 {
    text_origin_label(n, l, 0)
}

/// Top-left of label `idx`'s laid-out text. Index 0 is the main label; higher
/// indices are a connector's extra labels placed along the same route.
pub fn text_origin_label(n: &crate::model::Node, l: &TextLayout, idx: usize) -> V2 {
    if idx > 0 {
        // Extra labels only exist on connectors and always follow the route
        // placement math, with the extra's own position and offset.
        if let crate::model::NodeKind::Connector { points, route, .. } = &n.kind {
            let (pos, offset) = n
                .labels
                .get(idx - 1)
                .map(|e| (e.pos, e.offset))
                .unwrap_or((None, None));
            let t = pos.unwrap_or(0.5).clamp(0.0, 1.0);
            let p = crate::connector::path_point_at(points, *route, t) + offset.unwrap_or(V2::ZERO);
            let column = if n.label_wrap && n.dir != crate::model::WritingDir::Vertical {
                CONNECTOR_LABEL_WIDTH
            } else {
                l.width
            };
            return v2(p.x - column * 0.5, p.y - l.height * 0.5);
        }
    }
    if n.is_frame() {
        // Frame titles sit centered above the frame, like a layer name.
        v2(((n.rect.w - l.width) * 0.5).max(0.0), -FRAME_TITLE_GAP)
    } else if matches!(n.kind, crate::model::NodeKind::Sticky { .. }) {
        text_offset(n)
    } else if let crate::model::NodeKind::Connector { points, route, .. } = &n.kind {
        // Edge labels start at the middle of the routed line and move along it
        // or off it with the stored position and offset. A wrapped label is
        // laid out in a fixed column, so center that column rather than the
        // widest line and let the alignment work inside it.
        let t = n.label_pos.unwrap_or(0.5).clamp(0.0, 1.0);
        let p =
            crate::connector::path_point_at(points, *route, t) + n.label_offset.unwrap_or(V2::ZERO);
        let column = if n.label_wrap && n.dir != crate::model::WritingDir::Vertical {
            CONNECTOR_LABEL_WIDTH
        } else {
            l.width
        };
        v2(p.x - column * 0.5, p.y - l.height * 0.5)
    } else if n.kind.text().is_some() {
        // Vertical position inside the box; no effect while the height
        // auto-fits the content.
        let avail = (n.rect.h - n.pad.top - n.pad.bottom).max(0.0);
        let extra = (avail - l.height).max(0.0);
        let dy = match n.valign {
            crate::model::VAlign::Top => 0.0,
            crate::model::VAlign::Middle => extra * 0.5,
            crate::model::VAlign::Bottom => extra,
        };
        v2(n.pad.left, n.pad.top + dy)
    } else {
        let align = n.text_style.align.unwrap_or(TextAlign::Center);
        let avail_w = (n.rect.w - n.pad.left - n.pad.right - 2.0 * LABEL_PAD).max(0.0);
        // A wrapped label is laid out in a column as wide as the shape, so the
        // layout already applied the alignment. An unwrapped one is only as
        // wide as its longest line and is aligned here. Vertical text always
        // needs the column itself placed in the shape.
        let x = if n.label_wrap && n.dir != crate::model::WritingDir::Vertical {
            n.pad.left + LABEL_PAD
        } else {
            match align {
                TextAlign::Left => n.pad.left + LABEL_PAD,
                TextAlign::Center => n.pad.left + LABEL_PAD + ((avail_w - l.width) * 0.5).max(0.0),
                TextAlign::Right => n.pad.left + LABEL_PAD + (avail_w - l.width).max(0.0),
            }
        };
        let avail_h = (n.rect.h - n.pad.top - n.pad.bottom).max(0.0);
        let dy = n.pad.top
            + match n.label_valign.unwrap_or(crate::model::VAlign::Middle) {
                crate::model::VAlign::Top => LABEL_PAD,
                crate::model::VAlign::Middle => ((avail_h - l.height) * 0.5).max(0.0),
                crate::model::VAlign::Bottom => (avail_h - LABEL_PAD - l.height).max(0.0),
            };
        v2(x, dy)
    }
}

/// Padded box around a layout's glyphs, relative to the text origin. A wrapped
/// connector label is aligned inside a fixed column but its glyphs can be
/// narrower than that column, so the plate follows the glyphs. Shared by the
/// label background plate and the transparent-label line gap.
pub fn glyph_plate(l: &TextLayout) -> Rect {
    let mut x0 = f64::MAX;
    let mut x1 = f64::MIN;
    for g in &l.glyphs {
        x0 = x0.min(g.x);
        x1 = x1.max(g.x + g.advance);
    }
    if x0 > x1 {
        x0 = 0.0;
        x1 = l.width;
    }
    let (y0, y1) = l.lines.iter().fold((f64::MAX, f64::MIN), |(a, b), line| {
        (a.min(line.top), b.max(line.top + line.height))
    });
    let (y0, y1) = if y0 > y1 { (0.0, l.height) } else { (y0, y1) };
    Rect::new(x0 - 3.0, y0 - 2.0, x1 - x0 + 6.0, y1 - y0 + 4.0)
}

/// World-space rectangles a connector's line must avoid behind transparent
/// labels: one per non-empty label without a solid background. A plate paints
/// over the line, so labeled boxes with a background need no cut.
pub fn label_gap_rects(n: &crate::model::Node) -> Vec<Rect> {
    let mut out = Vec::new();
    if !n.is_connector() || n.text_bg.is_some() {
        return out;
    }
    for idx in 0..n.label_count() {
        if n.label_text(idx).is_none_or(str::is_empty) {
            continue;
        }
        let Some(l) = layout_node_label(n, idx) else {
            continue;
        };
        let r = glyph_plate(&l).translate(text_origin_label(n, &l, idx));
        out.push(r.translate(n.rect.min()));
    }
    out
}

/// World-space rectangle a connector's line must avoid behind its main
/// transparent label. `None` when the label is empty or has a solid
/// background: a plate paints over the line, so there is nothing to cut.
pub fn label_gap_rect(n: &crate::model::Node) -> Option<Rect> {
    if !n.is_connector() || n.text_bg.is_some() || n.label.is_empty() {
        return None;
    }
    let l = layout_node(n)?;
    let r = glyph_plate(&l).translate(text_origin(n, &l));
    Some(r.translate(n.rect.min()))
}

/// Box of label `idx` in node-local space, using the laid-out column for a
/// wrapped connector label. Used by hit testing and drag handling.
pub fn label_box_label(n: &crate::model::Node, idx: usize) -> Option<Rect> {
    if !n.supports_label() {
        return None;
    }
    if n.label_text(idx).is_none_or(str::is_empty) {
        return None;
    }
    let l = layout_node_label(n, idx)?;
    let o = text_origin_label(n, &l, idx);
    let w = if n.is_connector() && n.label_wrap && n.dir != crate::model::WritingDir::Vertical {
        CONNECTOR_LABEL_WIDTH
    } else {
        l.width.max(1.0)
    };
    Some(Rect::new(o.x, o.y, w, l.height.max(1.0)))
}

/// Box of the node's main label in node-local space.
pub fn label_box(n: &crate::model::Node) -> Option<Rect> {
    label_box_label(n, 0)
}

/// Rich-text spans can be hidden without deleting them (the Formatted Text
/// switch). Everything else about the style still applies.
fn effective_text_style(n: &crate::model::Node) -> Cow<'_, TextStyle> {
    if n.text_formatted {
        Cow::Borrowed(&n.text_style)
    } else {
        let mut ts = n.text_style.clone();
        ts.spans.clear();
        Cow::Owned(ts)
    }
}

/// Lift the middle of a straight run into an arc. Glyphs keep their upright
/// orientation; their baselines follow a quadratic curve across the run. The
/// layout grows downward by the curve height so the arc stays inside the box.
fn apply_curve(mut l: TextLayout, font_size: f64, curve: Option<f64>) -> TextLayout {
    let Some(curve) = curve.filter(|c| c.is_finite() && *c > 0.0) else {
        return l;
    };
    let lift = curve.min(1.0) * font_size * 0.9;
    let span = l.width.max(1.0);
    for g in &mut l.glyphs {
        let t = ((g.x + g.advance * 0.5) / span).clamp(0.0, 1.0);
        g.baseline += lift * (1.0 - 4.0 * t * (1.0 - t));
    }
    for line in &mut l.lines {
        line.baseline += lift;
    }
    l.height += lift;
    l
}

/// Stack characters top to bottom, one per line (draw.io's vertical Writing
/// Direction). Each character keeps its own weight, slant, size and color.
fn layout_vertical(
    text: &str,
    font_size: f64,
    color: Color,
    ts: &TextStyle,
    align: TextAlign,
) -> TextLayout {
    let mut glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut top = 0.0;
    let mut max_w: f64 = 0.0;
    for (idx, ch) in text.chars().enumerate() {
        if ch == '\n' {
            let lh = line_height(font_size);
            lines.push(Line {
                top,
                height: lh,
                baseline: top + ascent(font_size),
                width: 0.0,
                start: idx,
                end: idx,
                carets: vec![0.0],
            });
            top += lh;
            continue;
        }
        let st = ts.char_style(idx, font_size, color);
        let one = TextStyle {
            family: ts.family.clone(),
            bold: st.bold,
            italic: st.italic,
            line_height: ts.line_height,
            letter_spacing: ts.letter_spacing,
            align: None,
            script: st.script,
            curve: None,
            spans: Vec::new(),
            paragraphs: Vec::new(),
        };
        let mut l = layout_rich(
            &ch.to_string(),
            st.size,
            st.color,
            &one,
            None,
            TextAlign::Left,
        );
        let width = l.width;
        let height = l.height;
        let (baseline, carets) = l.lines.first().map_or((0.0, vec![0.0, width]), |line| {
            (line.baseline, line.carets.clone())
        });
        for g in &mut l.glyphs {
            g.char_index = idx;
            g.baseline += top;
            g.underline = st.underline;
            g.strike = st.strike;
        }
        glyphs.extend(l.glyphs);
        lines.push(Line {
            top,
            height,
            baseline: top + baseline,
            width,
            start: idx,
            end: idx + 1,
            carets,
        });
        top += height;
        max_w = max_w.max(width);
    }
    // Center or right-align each stacked character in the widest column.
    let mut shift = |line: &mut Line| {
        let dx = match align {
            TextAlign::Left => 0.0,
            TextAlign::Center => (max_w - line.width) * 0.5,
            TextAlign::Right => max_w - line.width,
        };
        for c in &mut line.carets {
            *c += dx;
        }
        for g in glyphs
            .iter_mut()
            .filter(|g| g.char_index >= line.start && g.char_index < line.end)
        {
            g.x += dx;
        }
    };
    for line in &mut lines {
        shift(line);
    }
    TextLayout {
        glyphs,
        lines,
        width: max_w,
        height: top,
        font_size,
    }
}

/// Layout of a text node's (or sticky note's, or shape label's) content in its box.
pub fn layout_node(n: &crate::model::Node) -> Option<TextLayout> {
    let ts = effective_text_style(n);
    if let crate::model::NodeKind::Sticky { text, font_size } = &n.kind {
        // Shrink the font (down to 12) until the text fits, like Excalidraw's sticky notes.
        let w = (n.rect.w - 2.0 * STICKY_PAD).max(10.0);
        let h = (n.rect.h - 2.0 * STICKY_PAD).max(10.0);
        let color = n
            .label_color
            .unwrap_or_else(|| n.style.fill().map_or(Color::INK, |c| c.contrasting_text()));
        let mut size = *font_size;
        loop {
            let ratio = size / font_size;
            let mut ts = ts.as_ref().clone();
            for sp in &mut ts.spans {
                sp.size = sp.size.map(|z| z * ratio);
            }
            let l = layout_rich(text, size, color, &ts, Some(w), TextAlign::Left);
            if l.height <= h || size <= 12.0 {
                return Some(l);
            }
            size = (size - 2.0).max(12.0);
        }
    }
    if let crate::model::NodeKind::Text {
        text,
        font_size,
        auto_width,
        align,
    } = &n.kind
    {
        let color = n
            .style
            .fills
            .last()
            .map_or(Color::INK, |p| p.preview_color());
        if n.dir == crate::model::WritingDir::Vertical {
            return Some(layout_vertical(text, *font_size, color, &ts, *align));
        }
        // The wrapped line column insets into the box by the spacing pad.
        let wide = (n.rect.w - n.pad.left - n.pad.right).max(1.0);
        // Writing direction: automatic follows the content, otherwise the
        // Writing Direction choice overrides the bidi paragraph direction.
        let rtl = match n.dir {
            crate::model::WritingDir::Auto => arabic::has_arabic(text),
            crate::model::WritingDir::Ltr => false,
            crate::model::WritingDir::Rtl => true,
            crate::model::WritingDir::Vertical => false,
        };
        let para = match n.dir {
            crate::model::WritingDir::Auto => None,
            crate::model::WritingDir::Ltr => Some(unicode_bidi::Level::ltr()),
            crate::model::WritingDir::Rtl => Some(unicode_bidi::Level::rtl()),
            crate::model::WritingDir::Vertical => None,
        };
        let wide = (!auto_width).then_some(wide);
        // Any bidi content, or an explicit direction override, goes through
        // the bidi layout; the prefix below steers its paragraph direction.
        let l = if rtl
            || arabic::has_arabic(text)
            || !matches!(n.dir, crate::model::WritingDir::Auto)
        {
            layout_bidi(text, *font_size, color, &ts, wide, *align, para)
        } else {
            layout_rich(text, *font_size, color, &ts, wide, *align)
        };
        return Some(apply_curve(l, *font_size, ts.curve));
    }
    if n.is_frame() {
        return Some(layout_rich(
            &n.name,
            FRAME_TITLE_SIZE,
            label_color(n),
            &ts,
            None,
            TextAlign::Center,
        ));
    }
    if !n.supports_label() {
        return None;
    }
    layout_shape_label(n, &n.label)
}

/// Layout of one label's text with the node's label style. Shared by the main
/// label and a connector's extra labels, which inherit the same style.
fn layout_shape_label(n: &crate::model::Node, text: &str) -> Option<TextLayout> {
    let ts = effective_text_style(n);
    let size = n.label_size.unwrap_or(LABEL_FONT_SIZE);
    let color = label_color(n);
    if n.dir == crate::model::WritingDir::Vertical {
        return Some(layout_vertical(
            text,
            size,
            color,
            &ts,
            n.text_style.align.unwrap_or(TextAlign::Center),
        ));
    }
    let width = if n.label_wrap {
        if n.is_connector() {
            Some(CONNECTOR_LABEL_WIDTH)
        } else {
            Some((n.rect.w - n.pad.left - n.pad.right - 2.0 * LABEL_PAD).max(1.0))
        }
    } else {
        None
    };
    Some(apply_curve(
        layout_rich(
            text,
            size,
            color,
            &ts,
            width,
            n.text_style.align.unwrap_or(TextAlign::Center),
        ),
        size,
        ts.curve,
    ))
}

/// Layout of label `idx`: 0 is the node's main label, higher indices are a
/// connector's extra labels. `None` when the index is out of range, the text
/// is empty, or the node cannot carry it.
pub fn layout_node_label(n: &crate::model::Node, idx: usize) -> Option<TextLayout> {
    if idx == 0 {
        return layout_node(n);
    }
    if !n.is_connector() {
        return None;
    }
    let text = n.label_text(idx)?;
    if text.is_empty() {
        return None;
    }
    layout_shape_label(n, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TextScript;

    #[test]
    fn layout_basic_and_newlines() {
        let l = layout("Hello\nWorld!", 20.0, None, TextAlign::Left);
        assert_eq!(l.lines.len(), 2);
        assert!(l.width > 40.0 && l.width < 120.0, "{}", l.width);
        assert!((l.height - 2.0 * line_height(20.0)).abs() < 1e-9);
        assert_eq!(l.glyphs.len(), 11);
        assert_eq!(l.caret(0).0, 0.0);
        assert_eq!(l.caret(6).1, line_height(20.0));
        let empty = layout("", 20.0, None, TextAlign::Left);
        assert_eq!(empty.lines.len(), 1);
        assert_eq!(empty.caret(0), (0.0, 0.0, line_height(20.0)));
    }

    #[test]
    fn list_markers_indent_and_number_paragraphs() {
        use crate::model::{ListKind, ParagraphStyle};
        let text = "one\ntwo\nthree";
        let ts = TextStyle {
            paragraphs: vec![
                ParagraphStyle {
                    list: ListKind::Bullet,
                    indent: 0,
                },
                ParagraphStyle {
                    list: ListKind::Bullet,
                    indent: 1,
                },
                ParagraphStyle {
                    list: ListKind::Number,
                    indent: 0,
                },
            ],
            ..TextStyle::default()
        };
        let plain = layout_rich(
            text,
            16.0,
            Color::INK,
            &TextStyle::default(),
            None,
            TextAlign::Left,
        );
        let list = layout_rich(text, 16.0, Color::INK, &ts, None, TextAlign::Left);
        assert_eq!(list.lines.len(), 3);
        assert!(list.glyphs.len() > plain.glyphs.len(), "markers add glyphs");
        let first_x = list.glyphs.iter().rfind(|g| g.char_index == 0).unwrap().x;
        let second_x = list.glyphs.iter().rfind(|g| g.char_index == 4).unwrap().x;
        let third_x = list.glyphs.iter().rfind(|g| g.char_index == 8).unwrap().x;
        assert!(first_x > 8.0, "bullet offsets its text: {first_x}");
        assert!(
            second_x > first_x + 16.0,
            "indented bullet moves right: {first_x} {second_x}"
        );
        assert!(
            third_x > 8.0,
            "numbered marker also offsets text: {third_x}"
        );
        assert!(list.width > plain.width, "markers grow the measured width");
    }

    #[test]
    fn wrapping_respects_width() {
        let l = layout(
            "the quick brown fox jumps over the lazy dog",
            16.0,
            Some(80.0),
            TextAlign::Left,
        );
        assert!(l.lines.len() >= 3);
        for line in &l.lines {
            assert!(
                line.width <= 80.0 + 1e-6 || line.end - line.start == 1,
                "{line:?}"
            );
        }
    }

    #[test]
    fn hit_and_caret_agree() {
        let l = layout("abc def", 30.0, None, TextAlign::Left);
        for i in 0..=7 {
            let (x, top, h) = l.caret(i);
            assert_eq!(l.hit(v2(x, top + h * 0.5)), i);
        }
    }

    #[test]
    fn superscript_raises_and_shrinks_the_glyph() {
        let mut ts = TextStyle::default();
        ts.apply_range(1, 2, |s| s.script = Some(TextScript::Super));
        let l = layout_rich("x2", 20.0, Color::BLACK, &ts, None, TextAlign::Left);
        let normal = &l.glyphs[0];
        let sup = &l.glyphs[1];
        assert!(sup.size < normal.size);
        assert!(sup.baseline < normal.baseline);
        assert!(sup.advance < normal.advance);
        // The line box still follows the base size.
        assert!((l.height - line_height(20.0)).abs() < 1e-9);
        assert!(l.width < layout("x2", 20.0, None, TextAlign::Left).width);
    }

    #[test]
    fn subscript_lowers_and_shrinks_the_glyph() {
        let ts = TextStyle {
            script: TextScript::Sub,
            ..Default::default()
        };
        let l = layout_rich("2", 20.0, Color::BLACK, &ts, None, TextAlign::Left);
        let g = &l.glyphs[0];
        let plain = layout("2", 20.0, None, TextAlign::Left);
        assert!(g.size < plain.glyphs[0].size);
        assert!(g.baseline > plain.glyphs[0].baseline);
        assert!(g.advance < plain.glyphs[0].advance);
    }

    #[test]
    fn glyphs_tessellate() {
        let (gid, _) = advance('A', 10.0);
        let m = glyph_mesh(0, gid);
        assert!(!m.indices.is_empty());
        assert!(glyph_svg_path(gid, V2::ZERO, 10.0).starts_with('M'));
    }

    #[test]
    fn arabic_uses_arabic_face_and_glyphs() {
        let l = layout("مرحبا", 24.0, None, TextAlign::Left);
        assert!(!l.glyphs.is_empty());
        assert!(l.width > 20.0);
        for g in &l.glyphs {
            assert!(g.face >= 4, "face {}", g.face);
            assert!(!glyph_mesh(g.face, g.gid).indices.is_empty());
        }
    }

    #[test]
    fn arabic_runs_are_visual_right_to_left() {
        let l = layout("بب", 24.0, None, TextAlign::Left);
        assert_eq!(l.glyphs.len(), 2);
        // The second logical letter is drawn first (leftmost), the first is on its right.
        assert_eq!(l.glyphs[0].char_index, 1);
        assert_eq!(l.glyphs[1].char_index, 0);
        assert!(l.glyphs[0].x < l.glyphs[1].x);
        let line = &l.lines[0];
        assert!(line.carets[0] > line.carets[2]);
        assert_eq!(line.carets[2], 0.0);
    }

    #[test]
    fn arabic_lam_alef_is_one_glyph() {
        let l = layout("لا", 24.0, None, TextAlign::Left);
        assert_eq!(l.glyphs.len(), 1, "{:?}", l.glyphs);
        assert_eq!(l.glyphs[0].char_index, 0);
        assert!(l.glyphs[0].gid != 0);
    }

    #[test]
    fn mixed_arabic_and_latin_keeps_latin_ltr() {
        let l = layout("abc مرحبا", 20.0, None, TextAlign::Left);
        assert!(l.width > 0.0);
        let abc: Vec<_> = l.glyphs.iter().filter(|g| g.char_index < 3).collect();
        assert_eq!(abc.len(), 3);
        assert!(abc[0].x < abc[1].x && abc[1].x < abc[2].x);
        let arabic_start = l
            .glyphs
            .iter()
            .position(|g| g.char_index > 3)
            .expect("arabic glyphs");
        assert!(l.glyphs[arabic_start].x > abc[2].x);
    }

    #[test]
    fn arabic_selection_rects_are_positive() {
        let l = layout("مرحبا", 24.0, None, TextAlign::Left);
        for (x, _, w, _) in l.selection_rects(0, 5) {
            assert!(w >= 1.0, "x {x} w {w}");
        }
        assert!(l.caret(0).0 > l.caret(5).0);
    }

    #[test]
    fn font_scanner_reads_bundled_families() {
        let dirs = [std::path::PathBuf::from("assets/fonts")];
        let lib = fonts::scan_dirs(&dirs);
        let inter = lib.find("Inter").expect("Inter family");
        let styles: Vec<(bool, bool)> = inter.faces.iter().map(|f| (f.bold, f.italic)).collect();
        assert!(styles.contains(&(false, false)), "{styles:?}");
        assert!(styles.contains(&(true, false)), "{styles:?}");
        assert!(styles.contains(&(false, true)), "{styles:?}");
        assert!(styles.contains(&(true, true)), "{styles:?}");
        assert!(lib.find("Noto Sans Arabic").is_some());
        // Case-insensitive lookup.
        assert_eq!(lib.find("INTER").unwrap().name, "Inter");
    }

    #[test]
    fn family_info_reports_discovered_styles() {
        let dirs = [std::path::PathBuf::from("assets/fonts")];
        let lib = fonts::scan_dirs(&dirs);
        assert_eq!(
            styles_from(Some(&lib), "inter"),
            vec![
                (400, "normal"),
                (700, "normal"),
                (400, "italic"),
                (700, "italic")
            ]
        );
        let arabic = styles_from(Some(&lib), "Noto Sans Arabic");
        assert!(!arabic.is_empty(), "bundled Arabic family should be found");
        assert!(arabic.contains(&(400, "normal")), "{arabic:?}");
        assert!(styles_from(Some(&lib), "No Such Family 4E7").is_empty());
        // Before the system scan lands there is no library to inspect.
        assert!(styles_from(None, "Noto Sans Arabic").is_empty());
    }

    #[test]
    fn inter_resolves_to_builtin_faces() {
        assert_eq!(resolve_family("Inter"), Some([0, 1, 2, 3]));
        assert_eq!(resolve_family("inter"), Some([0, 1, 2, 3]));
        assert_eq!(resolve_family("  "), None);
    }

    /// Cache keys visible to tests, to assert what resolution remembered.
    #[cfg(test)]
    fn family_cache_keys() -> Vec<String> {
        FAMILY_CACHE.lock().unwrap().keys().cloned().collect()
    }

    #[test]
    fn a_miss_is_not_cached_so_a_late_scan_can_still_resolve() {
        // The restart bug: the first paint happened before the background font
        // scan finished, the miss was cached, and the document kept rendering
        // the fallback font even though the panel showed the picked family.
        let name = "Not Cached Missing Family 91A";
        assert_eq!(resolve_family(name), None);
        assert_eq!(resolve_family(name), None);
        assert!(
            !family_cache_keys()
                .iter()
                .any(|k| k.contains("not cached missing")),
            "a miss while the scan runs must not be cached: {:?}",
            family_cache_keys()
        );
        // Hits are still cached.
        assert_eq!(resolve_family("Inter"), Some([0, 1, 2, 3]));
        assert!(family_cache_keys().iter().any(|k| k == "inter"));
    }

    #[test]
    fn missing_family_falls_back_to_inter() {
        install_test_library(&[std::path::PathBuf::from("assets/fonts")]);
        // A name nobody ships: resolution misses, layout matches the default.
        let name = "Definitely Missing Family 4E7";
        assert_eq!(resolve_family(name), None);
        let base = layout("Hi 字 مرحبا", 22.0, Some(180.0), TextAlign::Left);
        let ts = TextStyle {
            family: Some(name.into()),
            ..Default::default()
        };
        let owned = layout_rich(
            "Hi 字 مرحبا",
            22.0,
            Color::INK,
            &ts,
            Some(180.0),
            TextAlign::Left,
        );
        assert_eq!(base.glyphs.len(), owned.glyphs.len());
        for (a, b) in base.glyphs.iter().zip(&owned.glyphs) {
            assert_eq!(a.gid, b.gid);
            assert_eq!(a.face, b.face);
        }
    }
}
