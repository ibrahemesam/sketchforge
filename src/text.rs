//! Text layout and glyph outlines.
//!
//! Text is rendered as vector glyph outlines, so it stays crisp at every zoom level and
//! exports to SVG without font dependencies. Latin text uses Inter; Arabic text falls back
//! to Noto Sans Arabic and is shaped into Presentation Forms before it is laid out right to
//! left. Both fonts are SIL OFL and bundled in assets/fonts, not anything taken from Paper.

mod arabic;

use crate::geom::{V2, v2};
use crate::model::{CharStyle, Color, TextAlign, TextStyle};
use crate::vector::VPath;
use lyon::path::Path;
use lyon::tessellation::{BuffersBuilder, FillOptions, FillTessellator, FillVertex, VertexBuffers};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

pub static FONT_BYTES: &[u8] = crate::theme::INTER_REGULAR;
static BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
static ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-Italic.ttf");
static BOLD_ITALIC: &[u8] = include_bytes!("../assets/fonts/Inter-BoldItalic.ttf");
static ARABIC: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Regular.ttf");
static ARABIC_BOLD: &[u8] = include_bytes!("../assets/fonts/NotoSansArabic-Bold.ttf");

/// Number of built-in faces. Faces 0-3 are Inter, 4-7 are Noto Sans Arabic.
pub const FACE_COUNT: u8 = 8;

/// Face index: bits 0-1 pick Inter's weight/slant, bit 2 picks the Arabic family.
pub type FaceId = u8;

pub fn face_id(bold: bool, italic: bool) -> FaceId {
    bold as u8 | (italic as u8) << 1
}

/// Arabic has no italic face; both bold and italic requests pick regular or bold.
pub fn arabic_face_id(bold: bool) -> FaceId {
    4 | bold as u8
}

fn face_for_char(c: char, bold: bool, italic: bool) -> FaceId {
    if arabic::is_arabic(c) {
        arabic_face_id(bold)
    } else {
        face_id(bold, italic)
    }
}

pub fn face_bytes(id: FaceId) -> &'static [u8] {
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
    static FACES: OnceLock<[ttf_parser::Face<'static>; FACE_COUNT as usize]> = OnceLock::new();
    &FACES.get_or_init(|| {
        (0..FACE_COUNT)
            .map(|i| ttf_parser::Face::parse(face_bytes(i), 0).expect("bundled font parses"))
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

pub fn line_height(font_size: f64) -> f64 {
    let f = face();
    let h = (f.ascender() as f64 - f.descender() as f64 + f.line_gap() as f64) / units_per_em();
    font_size * h.max(1.1)
}

pub fn ascent(font_size: f64) -> f64 {
    face().ascender() as f64 / units_per_em() * font_size
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
    if arabic::has_arabic(text) {
        return layout_bidi(text, font_size, color, ts, max_width, align);
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
            let (g, a) = advance_in(face_id(s.bold, s.italic), *c, s.size);
            (g, a + ls)
        })
        .collect();
    let lh_of = |size: f64| match ts.line_height {
        Some(m) => size * m,
        None => line_height(size),
    };
    // Break into lines: hard breaks on '\n', soft breaks at spaces when wrapping.
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    while start <= chars.len() {
        let hard_end = chars[start..]
            .iter()
            .position(|c| *c == '\n')
            .map_or(chars.len(), |p| start + p);
        let mut line_start = start;
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
    let mut glyphs = Vec::new();
    let mut max_line_w: f64 = 0.0;
    let mut top = 0.0;
    for (s, e) in ranges.iter().copied() {
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
        let mut x = 0.0;
        let mut carets = vec![0.0];
        for i in s..e {
            let st = styles[i];
            let (gid, a) = adv[i];
            glyphs.push(PlacedGlyph {
                gid,
                face: face_id(st.bold, st.italic),
                size: st.size,
                color: st.color,
                x,
                baseline,
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
) -> TextLayout {
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
        let (gid, adv) = advance_in(face, shaped.ch, st.size);
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

        let line_text: String = chars[char_start..char_end].iter().collect();
        let mut visual: Vec<(usize, bool)> = Vec::new();
        if !line_text.is_empty() {
            let bidi = unicode_bidi::BidiInfo::new(&line_text, None);
            if let Some(para) = bidi.paragraphs.first() {
                let (levels, runs) = bidi.visual_runs(para, 0..line_text.len());
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
                    let a = char_at(run.start);
                    let b = char_at(run.end);
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
                    size: cell.size,
                    color: cell.color,
                    x,
                    baseline,
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

/// Color of a shape label: an explicit override, or black/white against the fill.
pub fn label_color(n: &crate::model::Node) -> Color {
    n.label_color
        .unwrap_or_else(|| n.style.fill().map_or(Color::INK, |c| c.contrasting_text()))
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
    if n.is_frame() {
        // Frame titles sit centered above the frame, like a layer name.
        v2(((n.rect.w - l.width) * 0.5).max(0.0), -FRAME_TITLE_GAP)
    } else if n.kind.text().is_some() {
        text_offset(n)
    } else {
        v2(LABEL_PAD, ((n.rect.h - l.height) * 0.5).max(0.0))
    }
}

/// Layout of a text node's (or sticky note's, or shape label's) content in its box.
pub fn layout_node(n: &crate::model::Node) -> Option<TextLayout> {
    if let crate::model::NodeKind::Sticky { text, font_size } = &n.kind {
        // Shrink the font (down to 12) until the text fits, like Excalidraw's sticky notes.
        let w = (n.rect.w - 2.0 * STICKY_PAD).max(10.0);
        let h = (n.rect.h - 2.0 * STICKY_PAD).max(10.0);
        let color = n.style.fill().map_or(Color::INK, |c| c.contrasting_text());
        let mut size = *font_size;
        loop {
            let ratio = size / font_size;
            let mut ts = n.text_style.clone();
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
        return Some(layout_rich(
            text,
            *font_size,
            color,
            &n.text_style,
            (!auto_width).then_some(n.rect.w),
            *align,
        ));
    }
    if n.is_frame() {
        return Some(layout_rich(
            &n.name,
            FRAME_TITLE_SIZE,
            label_color(n),
            &n.text_style,
            None,
            TextAlign::Center,
        ));
    }
    if !n.supports_label() {
        return None;
    }
    Some(layout_rich(
        &n.label,
        LABEL_FONT_SIZE,
        label_color(n),
        &n.text_style,
        Some((n.rect.w - 2.0 * LABEL_PAD).max(1.0)),
        TextAlign::Center,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
