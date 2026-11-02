//! Offline OCR with `ocrs` (pure Rust, rten models). Needs `text-detection.rten` and
//! `text-recognition.rten` in `runtime/models` (run scripts/fetch-assets once on a machine with internet).
//! Models are Latin-script (English and most Western European languages).
use crate::pdfops::*;
use crate::render::*;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;

const DET: &str = "text-detection.rten";
const REC: &str = "text-recognition.rten";

pub fn model_paths() -> Result<(PathBuf, PathBuf)> {
    for d in runtime_dirs() {
        for base in [d.join("models"), d.clone()] {
            let (a, b) = (base.join(DET), base.join(REC));
            if a.is_file() && b.is_file() {
                return Ok((a, b));
            }
        }
    }
    bail!("OCR models not found. Run scripts/fetch-assets once (needs internet) and keep the \"runtime/models\" folder next to the app.")
}

/// `Ok(())` when OCR can run, otherwise the reason (shown in the UI).
#[cfg(not(feature = "ocr"))]
pub fn status() -> std::result::Result<(), String> {
    Err("This build was compiled without OCR support (feature \"ocr\").".into())
}
#[cfg(feature = "ocr")]
pub fn status() -> std::result::Result<(), String> {
    model_paths().map(|_| ()).map_err(|e| e.to_string())?;
    pdfium_status()
}

/// One recognised word with its pixel box (x0, y0, x1, y1) in the rendered image.
pub struct Word {
    pub text: String,
    pub bbox: (f32, f32, f32, f32),
}

#[cfg(feature = "ocr")]
mod engine {
    use super::*;
    use image::RgbaImage;
    use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
    use rten::Model;

    pub struct Engine(OcrEngine);
    pub fn load() -> Result<Engine> {
        let (d, r) = model_paths()?;
        let det = Model::load_file(&d).map_err(|e| anyhow!("Could not load the detection model: {e}"))?;
        let rec = Model::load_file(&r).map_err(|e| anyhow!("Could not load the recognition model: {e}"))?;
        let e = OcrEngine::new(OcrEngineParams { detection_model: Some(det), recognition_model: Some(rec), ..Default::default() })
            .map_err(|e| anyhow!("Could not start the OCR engine: {e}"))?;
        Ok(Engine(e))
    }
    impl Engine {
        pub fn words(&self, img: &RgbaImage) -> Result<(Vec<Word>, String)> {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let src = ImageSource::from_bytes(rgb.as_raw(), rgb.dimensions()).map_err(|e| anyhow!("{e}"))?;
            let input = self.0.prepare_input(src).map_err(|e| anyhow!("{e}"))?;
            let words = self.0.detect_words(&input).map_err(|e| anyhow!("{e}"))?;
            let lines = self.0.find_text_lines(&input, &words);
            let texts = self.0.recognize_text(&input, &lines).map_err(|e| anyhow!("{e}"))?;
            let mut out = Vec::new();
            let mut full = Vec::new();
            for line in texts.into_iter().flatten() {
                full.push(line.to_string());
                for w in line.words() {
                    let r = w.bounding_rect();
                    out.push(Word { text: w.to_string(), bbox: (r.left() as f32, r.top() as f32, r.right() as f32, r.bottom() as f32) });
                }
            }
            Ok((out, full.join("\n")))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Searchable,
    Text,
}

pub struct OcrOpts<'a> {
    pub mode: Mode,
    pub pages: &'a str,
    pub dpi: f32,
    pub skip_text: bool,
}

#[cfg(not(feature = "ocr"))]
pub fn run(_bytes: &[u8], _file: &str, _o: &OcrOpts, _ctx: &Ctx) -> Result<Vec<Output>> {
    bail!("This build was compiled without OCR support.")
}

#[cfg(feature = "ocr")]
pub fn run(bytes: &[u8], file: &str, o: &OcrOpts, ctx: &Ctx) -> Result<Vec<Output>> {
    status().map_err(|e| anyhow!(e))?;
    ctx.progress(0, 1, "Starting OCR engine");
    let eng = engine::load()?;

    // Phase 1: render + recognise (PDFium lock held), collect per-page results.
    struct PageResult {
        index: usize,
        words: Vec<Word>,
        text: String,
        img_w: f32,
        img_h: f32,
        page_w: f32,
        page_h: f32,
    }
    let (results, skipped, total) = with_doc(bytes, |doc| {
        let pages = doc.pages();
        let total = pages.len() as usize;
        let idx = parse_pages(o.pages, total)?;
        let mut res: Vec<PageResult> = Vec::new();
        let mut skipped = Vec::new();
        for (k, &i) in idx.iter().enumerate() {
            ctx.check()?;
            ctx.progress(k, idx.len(), &format!("OCR page {} of {}", i + 1, total));
            let page = pages.get(i as u16).map_err(|e| anyhow!("{e}"))?;
            if o.skip_text && page_text_len(&page) > 25 {
                skipped.push(i);
                continue;
            }
            let img = render_dpi(&page, o.dpi)?;
            let (words, text) = eng.words(&img)?;
            res.push(PageResult { index: i, words, text, img_w: img.width() as f32, img_h: img.height() as f32, page_w: page.width().value, page_h: page.height().value });
        }
        ctx.progress(idx.len(), idx.len(), "Finishing");
        Ok((res, skipped, total))
    })?;

    let mut notes = Vec::new();
    if !skipped.is_empty() {
        notes.push(format!("{} page(s) already had text and were skipped", skipped.len()));
    }
    let _ = total;

    if o.mode == Mode::Text {
        let mut parts: Vec<(usize, String)> = results.iter().map(|r| (r.index, format!("===== Page {} =====\n{}", r.index + 1, r.text.trim()))).collect();
        parts.extend(skipped.iter().map(|&i| (i, format!("===== Page {} (already has text) =====", i + 1))));
        parts.sort_by_key(|p| p.0);
        let txt = parts.into_iter().map(|p| p.1).collect::<Vec<_>>().join("\n\n");
        let note = if notes.is_empty() { format!("{} page(s) recognised", results.len()) } else { notes.join(". ") };
        return Ok(vec![Output::text(format!("{}-ocr.txt", base_name(file)), txt).note(note)]);
    }

    // Phase 2: write the invisible text layer with lopdf.
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    let font = font_object(&mut d, Font::Helv);
    let mut rotated = 0;
    for r in &results {
        let Some(&pid) = ids.get(r.index) else { continue };
        if page_rotation(&d, pid) != 0 {
            rotated += 1;
            continue;
        }
        let b = page_box(&d, pid);
        let (sx, sy) = (r.page_w / r.img_w, r.page_h / r.img_h);
        let mut ov = Overlay::new();
        ov.fonts.push((Font::Helv.res_name().into(), font));
        for w in &r.words {
            let t = safe_text(&w.text);
            let t = t.trim();
            if t.is_empty() {
                continue;
            }
            let (x0, y0, x1, y1) = w.bbox;
            let (bw, bh) = ((x1 - x0) * sx, (y1 - y0) * sy);
            let natural = text_width(Font::Helv, t, 1.0).max(0.01);
            let size = (bw / natural).max(1.0).min(bh * 2.5);
            ov.content.text(Font::Helv, size, b[0] + x0 * sx, b[1] + (r.img_h - y1) * sy + bh * 0.2, 0.0, 3, t);
        }
        ov.apply(&mut d, pid)?;
    }
    if rotated > 0 {
        notes.push(format!("{rotated} rotated page(s) skipped (rotate them to 0 first)"));
    }
    let mut note = vec!["Invisible text layer added".to_string()];
    note.extend(notes);
    Ok(vec![Output::new(format!("{}-searchable.pdf", base_name(file)), save(&mut d)?).note(note.join(". "))])
}
