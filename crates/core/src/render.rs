//! Page rendering and text extraction via PDFium (loaded at run time, so the app stays one small binary
//! and works fully offline once `runtime/` contains the PDFium library).
//! Text extraction falls back to lopdf when PDFium is missing; rendering features need PDFium.
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use image::{DynamicImage, RgbaImage};
use pdfium_render::prelude::*;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOCK: Mutex<()> = Mutex::new(());

/// Folders searched for `runtime/` assets (PDFium library, OCR models).
pub fn runtime_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            v.push(dir.to_path_buf());
            v.push(dir.join("runtime"));
            // macOS .app bundle: Contents/MacOS/../Resources/runtime
            v.push(dir.join("../Resources/runtime"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        v.push(cwd.clone());
        v.push(cwd.join("runtime"));
    }
    v
}

fn bind() -> Result<Pdfium> {
    for d in runtime_dirs() {
        let p = Pdfium::pdfium_platform_library_name_at_path(&d);
        if let Ok(b) = Pdfium::bind_to_library(p) {
            return Ok(Pdfium::new(b));
        }
    }
    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|_| anyhow!("The PDF renderer (PDFium) was not found. Put the PDFium library in the app's runtime folder - see README."))
}

/// `Ok(())` when PDFium can be loaded, otherwise the reason.
pub fn pdfium_status() -> std::result::Result<(), String> {
    static S: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    S.get_or_init(|| with_pdfium(|_| Ok(())).map_err(|e| e.to_string())).clone()
}
pub fn pdfium_available() -> bool {
    pdfium_status().is_ok()
}

/// All PDFium work is serialised through one lock; a fresh binding is created per operation.
pub fn with_pdfium<T>(f: impl FnOnce(&Pdfium) -> Result<T>) -> Result<T> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = bind()?;
    f(&p)
}

fn perr(e: PdfiumError) -> anyhow::Error {
    anyhow!("{e}")
}

pub fn render_page(page: &PdfPage, width_px: i32) -> Result<RgbaImage> {
    let cfg = PdfRenderConfig::new().set_target_width(width_px.max(8)).set_format(PdfBitmapFormat::BGRA);
    let bmp = page.render_with_config(&cfg).map_err(perr)?;
    let (w, h) = (bmp.width() as u32, bmp.height() as u32);
    let mut raw = bmp.as_raw_bytes();
    for px in raw.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }
    RgbaImage::from_raw(w, h, raw).ok_or_else(|| anyhow!("The renderer returned an unexpected image size."))
}
pub fn width_for_dpi(page: &PdfPage, dpi: f32) -> i32 {
    (page.width().value * dpi / 72.0).round().max(8.0) as i32
}
pub fn render_dpi(page: &PdfPage, dpi: f32) -> Result<RgbaImage> {
    render_page(page, width_for_dpi(page, dpi))
}

/// Open a document, run `f` with it. Handy for one-off renders.
pub fn with_doc<T>(bytes: &[u8], f: impl FnOnce(&PdfDocument) -> Result<T>) -> Result<T> {
    with_pdfium(|p| {
        let doc = p.load_pdf_from_byte_slice(bytes, None).map_err(perr)?;
        f(&doc)
    })
}

pub fn render_one(bytes: &[u8], index: usize, width_px: i32) -> Result<RgbaImage> {
    with_doc(bytes, |doc| {
        let pages = doc.pages();
        let page = pages.get(index as u16).map_err(perr)?;
        render_page(&page, width_px)
    })
}
/// Render many pages; `cb` returns false to stop early. Used for thumbnails.
pub fn render_many(bytes: &[u8], indexes: &[usize], width_px: i32, mut cb: impl FnMut(usize, RgbaImage) -> bool) -> Result<()> {
    with_doc(bytes, |doc| {
        let pages = doc.pages();
        for &i in indexes {
            let page = pages.get(i as u16).map_err(perr)?;
            if !cb(i, render_page(&page, width_px)?) {
                break;
            }
        }
        Ok(())
    })
}
/// (width_pt, height_pt) of a page.
pub fn page_dims(bytes: &[u8], index: usize) -> Result<(f32, f32)> {
    with_doc(bytes, |doc| {
        let page = doc.pages().get(index as u16).map_err(perr)?;
        Ok((page.width().value, page.height().value))
    })
}

pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img.clone()).write_to(&mut buf, image::ImageFormat::Png).map_err(|e| anyhow!("PNG encoding failed: {e}"))?;
    Ok(buf.into_inner())
}
pub fn encode_jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>> {
    let rgb = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
    let mut buf = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality.clamp(10, 100));
    enc.write_image(rgb.as_raw(), rgb.width(), rgb.height(), image::ExtendedColorType::Rgb8)
        .map_err(|e| anyhow!("JPEG encoding failed: {e}"))?;
    Ok(buf)
}
use image::ImageEncoder;

// ---------------------------------------------------------------- text lines
#[derive(Clone, Debug)]
pub struct Line {
    pub y: f32,
    pub h: f32,
    pub text: String,
    pub cells: Vec<String>,
}
#[derive(Clone, Copy)]
pub struct Glyph {
    pub c: char,
    pub x: f32,
    pub y: f32,
    pub h: f32,
    pub w: f32,
}

/// Glyph boxes -> lines (top to bottom) with column cells split at wide gaps.
pub fn glyphs_to_lines(mut g: Vec<Glyph>) -> Vec<Line> {
    g.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal).then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal)));
    struct Raw {
        y: f32,
        h: f32,
        items: Vec<Glyph>,
    }
    let mut lines: Vec<Raw> = Vec::new();
    for it in g {
        let joins = matches!(lines.last(), Some(l) if (l.y - it.y).abs() < (it.h * 0.4).max(2.0));
        if joins {
            if let Some(last) = lines.last_mut() {
                last.h = last.h.max(it.h);
                last.items.push(it);
            }
        } else {
            lines.push(Raw { y: it.y, h: it.h, items: vec![it] });
        }
    }
    lines
        .into_iter()
        .map(|mut l| {
            l.items.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            let (mut cells, mut text, mut cell) = (Vec::new(), String::new(), String::new());
            let mut prev_end: Option<f32> = None;
            for it in &l.items {
                if let Some(pe) = prev_end {
                    let gap = it.x - pe;
                    if gap > (l.h * 1.6).max(10.0) {
                        cells.push(cell.trim().to_string());
                        cell.clear();
                        text.push_str("   ");
                    } else if gap > l.h * 0.12 {
                        cell.push(' ');
                        text.push(' ');
                    }
                }
                cell.push(it.c);
                text.push(it.c);
                prev_end = Some(it.x + it.w);
            }
            cells.push(cell.trim().to_string());
            Line { y: l.y, h: l.h, text: text.trim().to_string(), cells }
        })
        .filter(|l| !l.text.is_empty())
        .collect()
}

pub fn pdfium_page_lines(page: &PdfPage) -> Vec<Line> {
    let Ok(text) = page.text() else { return vec![] };
    let mut g = Vec::new();
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        if c.is_whitespace() || c.is_control() {
            continue;
        }
        let Ok(b) = ch.loose_bounds() else { continue };
        let (l, r, bt, tp) = (b.left().value, b.right().value, b.bottom().value, b.top().value);
        g.push(Glyph { c, x: l, y: bt, h: (tp - bt).abs().max(1.0), w: (r - l).max(0.0) });
    }
    glyphs_to_lines(g)
}

/// Text lines of every page. Uses PDFium when present, otherwise lopdf's simpler extractor.
pub fn all_lines(bytes: &[u8], ctx: &Ctx) -> Result<Vec<Vec<Line>>> {
    if pdfium_available() {
        return with_doc(bytes, |doc| {
            let pages = doc.pages();
            let total = pages.len() as usize;
            let mut out = Vec::new();
            for i in 0..total {
                ctx.check()?;
                ctx.progress(i, total, &format!("Reading page {}", i + 1));
                out.push(pdfium_page_lines(&pages.get(i as u16).map_err(perr)?));
            }
            ctx.progress(total, total, "");
            Ok(out)
        });
    }
    let d = crate::pdfops::load(bytes)?;
    let nums: Vec<u32> = d.get_pages().keys().copied().collect();
    let mut out = Vec::new();
    for (k, n) in nums.iter().enumerate() {
        ctx.check()?;
        ctx.progress(k, nums.len(), &format!("Reading page {n}"));
        let text = d.extract_text(&[*n]).unwrap_or_default();
        let lines: Vec<Line> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(i, l)| Line { y: -(i as f32) * 12.0, h: 10.0, text: l.to_string(), cells: l.split("  ").map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect() })
            .collect();
        out.push(lines);
    }
    Ok(out)
}

pub fn page_text_len(page: &PdfPage) -> usize {
    page.text().map(|t| t.all().trim().chars().count()).unwrap_or(0)
}

/// Median text height across all lines (body text size).
pub fn body_height(pages: &[Vec<Line>]) -> f32 {
    let mut hs: Vec<f32> = pages.iter().flatten().map(|l| l.h).collect();
    if hs.is_empty() {
        return 10.0;
    }
    hs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    hs[hs.len() / 2]
}
pub fn require_text(pages: &[Vec<Line>]) -> Result<()> {
    if pages.iter().all(|p| p.is_empty()) {
        bail!("No text found - this looks like a scan. Run OCR first.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn word(s: &str, x0: f32, y: f32) -> Vec<Glyph> {
        s.chars().enumerate().map(|(i, c)| Glyph { c, x: x0 + i as f32 * 6.0, y, h: 10.0, w: 6.0 }).collect()
    }
    #[test]
    fn lines_and_columns() {
        let mut g = word("Name", 10.0, 700.0);
        g.extend(word("Price", 200.0, 700.0));
        g.extend(word("Apple", 10.0, 680.0));
        let l = glyphs_to_lines(g);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].cells, vec!["Name".to_string(), "Price".to_string()]);
        assert_eq!(l[1].text, "Apple");
    }
}
