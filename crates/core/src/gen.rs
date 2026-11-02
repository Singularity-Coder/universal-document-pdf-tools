//! Block model -> PDF. Standard Helvetica/Courier fonts (WinAnsi), so text outside Latin-1 becomes "?".
use crate::pdfops::*;
use anyhow::{anyhow, Result};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

pub const A4: (f32, f32) = (595.28, 841.89);
const MARGIN: f32 = 54.0;

#[derive(Clone, Debug)]
pub enum Block {
    Para(String),
    Heading(String, f32),
    Mono(String, f32),
    Table(Vec<Vec<String>>, Option<String>),
    Image(Vec<u8>),
    PageBreak,
}

/// Greedy word wrap; very long words are split.
pub fn wrap(text: &str, font: Font, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.replace('\r', "").split('\n') {
        let para = safe_text(raw);
        if para.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        let mut tokens: Vec<String> = Vec::new();
        let mut cur = String::new();
        let mut in_ws = false;
        for ch in para.chars() {
            if ch.is_whitespace() != in_ws && !cur.is_empty() {
                tokens.push(std::mem::take(&mut cur));
            }
            in_ws = ch.is_whitespace();
            cur.push(ch);
        }
        if !cur.is_empty() {
            tokens.push(cur);
        }
        for word in tokens {
            let test = format!("{line}{word}");
            if text_width(font, &test, size) <= max_w || line.is_empty() {
                line = test;
                while text_width(font, &line, size) > max_w && line.chars().count() > 1 {
                    let chars: Vec<char> = line.chars().collect();
                    let mut k = chars.len() - 1;
                    while k > 1 && text_width(font, &chars[..k].iter().collect::<String>(), size) > max_w {
                        k -= 1;
                    }
                    lines.push(chars[..k].iter().collect());
                    line = chars[k..].iter().collect();
                }
            } else {
                lines.push(line.trim_end().to_string());
                line = word.trim_start().to_string();
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

struct PageBuf {
    content: Content,
    xobj: Vec<(String, ObjectId)>,
}

struct Writer {
    doc: Document,
    pages: Vec<PageBuf>,
    cur: Option<PageBuf>,
    y: f32,
    fonts: [ObjectId; 3],
    n_img: usize,
}

impl Writer {
    fn new() -> Self {
        let mut doc = Document::with_version("1.7");
        let fonts = [font_object(&mut doc, Font::Helv), font_object(&mut doc, Font::Bold), font_object(&mut doc, Font::Cour)];
        Self { doc, pages: vec![], cur: None, y: 0.0, fonts, n_img: 0 }
    }
    fn new_page(&mut self) {
        if let Some(p) = self.cur.take() {
            self.pages.push(p);
        }
        self.cur = Some(PageBuf { content: Content::new(), xobj: vec![] });
        self.y = A4.1 - MARGIN;
    }
    fn need(&mut self, h: f32) {
        if self.cur.is_none() || self.y - h < MARGIN {
            self.new_page();
        }
    }
    fn page(&mut self) -> &mut PageBuf {
        self.cur.as_mut().expect("page exists")
    }

    fn block(&mut self, b: &Block) {
        let max_w = A4.0 - MARGIN * 2.0;
        match b {
            Block::PageBreak => {
                if let Some(p) = self.cur.take() {
                    self.pages.push(p);
                }
            }
            Block::Para(t) | Block::Heading(t, _) | Block::Mono(t, _) => {
                let (font, size) = match b {
                    Block::Heading(_, s) => (Font::Bold, *s),
                    Block::Mono(_, s) => (Font::Cour, *s),
                    _ => (Font::Helv, 11.0),
                };
                let heading = matches!(b, Block::Heading(..));
                let mono = matches!(b, Block::Mono(..));
                let lh = size * 1.3;
                let lines = wrap(t, font, size, max_w);
                if heading {
                    self.need(lh * 2.0);
                    self.y -= size * 0.6;
                }
                for ln in lines {
                    self.need(lh);
                    self.y -= lh;
                    if !ln.is_empty() {
                        let y = self.y + size * 0.25;
                        self.page().content.fill_rgb((0.08, 0.08, 0.1)).text(font, size, MARGIN, y, 0.0, 0, &ln);
                    }
                }
                self.y -= if heading { size * 0.3 } else if mono { 4.0 } else { size * 0.55 };
            }
            Block::Table(rows, title) => {
                let rows: Vec<&Vec<String>> = rows.iter().filter(|r| !r.is_empty()).collect();
                if rows.is_empty() {
                    return;
                }
                if let Some(t) = title {
                    self.need(30.0);
                    self.y -= 8.0;
                    let y = self.y - 12.0;
                    self.page().content.fill_rgb((0.1, 0.1, 0.12)).text(Font::Bold, 13.0, MARGIN, y, 0.0, 0, t);
                    self.y -= 22.0;
                }
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(1);
                let size = if cols > 10 { 6.0 } else if cols > 7 { 7.0 } else if cols > 5 { 8.0 } else { 9.0 };
                let w: Vec<f32> = (0..cols)
                    .map(|c| rows.iter().take(200).map(|r| r.get(c).map(|s| s.chars().count()).unwrap_or(0)).max().unwrap_or(0).clamp(4, 40) as f32)
                    .collect();
                let tot: f32 = w.iter().sum();
                let cw: Vec<f32> = w.iter().map(|x| x / tot * max_w).collect();
                for (ri, row) in rows.iter().enumerate() {
                    let font = if ri == 0 { Font::Bold } else { Font::Helv };
                    let cells: Vec<Vec<String>> = (0..cols).map(|c| wrap(row.get(c).map(|s| s.as_str()).unwrap_or(""), font, size, (cw[c] - 6.0).max(8.0))).collect();
                    let nlines = cells.iter().map(|c| c.len()).max().unwrap_or(1);
                    let rh = nlines as f32 * size * 1.25 + 6.0;
                    self.need(rh);
                    let top = self.y;
                    let mut x = MARGIN;
                    if ri == 0 {
                        self.page().content.fill_rgb((0.93, 0.94, 0.98)).rect(MARGIN, top - rh, max_w, rh, true);
                    }
                    for (c, cl) in cells.iter().enumerate() {
                        for (li, t) in cl.iter().enumerate() {
                            if !t.is_empty() {
                                let ty = top - 3.0 - size - li as f32 * size * 1.25 + 1.0;
                                self.page().content.fill_rgb((0.1, 0.1, 0.12)).text(font, size, x + 3.0, ty, 0.0, 0, t);
                            }
                        }
                        self.page().content.stroke_rgb((0.8, 0.82, 0.88)).line_width(0.5).rect(x, top - rh, cw[c], rh, false);
                        x += cw[c];
                    }
                    self.y -= rh;
                }
                self.y -= 12.0;
            }
            Block::Image(bytes) => {
                let Ok(img) = embed_image(&mut self.doc, bytes) else { return };
                let s = (max_w / img.w as f32).min((A4.1 - MARGIN * 2.0) / img.h as f32).min(1.0);
                let (w, h) = (img.w as f32 * s, img.h as f32 * s);
                self.need(h + 8.0);
                self.y -= h;
                self.n_img += 1;
                let nm = format!("Im{}", self.n_img);
                let y = self.y;
                let p = self.page();
                p.xobj.push((nm.clone(), img.id));
                p.content.image(&nm, MARGIN, y, w, h);
                self.y -= 10.0;
            }
        }
    }

    fn finish(mut self) -> Result<Vec<u8>> {
        if let Some(p) = self.cur.take() {
            self.pages.push(p);
        }
        if self.pages.is_empty() {
            self.pages.push(PageBuf { content: Content::new(), xobj: vec![] });
        }
        let pages_id = self.doc.new_object_id();
        let mut kids = Vec::new();
        for p in std::mem::take(&mut self.pages) {
            let cid = self.doc.add_object(Stream::new(Dictionary::new(), p.content.0));
            let mut fonts = Dictionary::new();
            for (i, f) in [Font::Helv, Font::Bold, Font::Cour].iter().enumerate() {
                fonts.set(f.res_name(), Object::Reference(self.fonts[i]));
            }
            let mut res = Dictionary::new();
            res.set("Font", Object::Dictionary(fonts));
            if !p.xobj.is_empty() {
                let mut x = Dictionary::new();
                for (n, id) in &p.xobj {
                    x.set(n.as_str(), Object::Reference(*id));
                }
                res.set("XObject", Object::Dictionary(x));
            }
            let mut d = Dictionary::new();
            d.set("Type", name("Page"));
            d.set("Parent", Object::Reference(pages_id));
            d.set("MediaBox", Object::Array(vec![real(0.0), real(0.0), real(A4.0), real(A4.1)]));
            d.set("Resources", Object::Dictionary(res));
            d.set("Contents", Object::Reference(cid));
            kids.push(Object::Reference(self.doc.add_object(Object::Dictionary(d))));
        }
        let mut pages = Dictionary::new();
        pages.set("Type", name("Pages"));
        pages.set("Count", Object::Integer(kids.len() as i64));
        pages.set("Kids", Object::Array(kids));
        self.doc.objects.insert(pages_id, Object::Dictionary(pages));
        let mut cat = Dictionary::new();
        cat.set("Type", name("Catalog"));
        cat.set("Pages", Object::Reference(pages_id));
        let cat_id = self.doc.add_object(Object::Dictionary(cat));
        self.doc.trailer.set("Root", Object::Reference(cat_id));
        save(&mut self.doc)
    }
}

/// Blocks -> PDF bytes (A4).
pub fn blocks_to_pdf(blocks: &[Block]) -> Result<Vec<u8>> {
    let mut w = Writer::new();
    for b in blocks {
        w.block(b);
    }
    w.finish()
}

/// One picture -> one page. `fit_a4`: centred on A4 (landscape picture -> landscape page); otherwise the page is the picture's size.
pub fn image_to_pdf(bytes: &[u8], fit_a4: bool) -> Result<Vec<u8>> {
    let mut doc = Document::with_version("1.7");
    let img = embed_image(&mut doc, bytes)?;
    let (iw, ih) = (img.w as f32, img.h as f32);
    let (pw, ph, x, y, w, h) = if fit_a4 {
        let (pw, ph) = if iw > ih { (A4.1, A4.0) } else { A4 };
        let m = 24.0;
        let s = ((pw - m * 2.0) / iw).min((ph - m * 2.0) / ih).min(1.5);
        let (w, h) = (iw * s, ih * s);
        (pw, ph, (pw - w) / 2.0, (ph - h) / 2.0, w, h)
    } else {
        let s = (1400.0 / iw.max(ih)).min(1.0);
        (iw * s, ih * s, 0.0, 0.0, iw * s, ih * s)
    };
    let mut c = Content::new();
    c.image("Im0", x, y, w, h);
    let cid = doc.add_object(Stream::new(Dictionary::new(), c.0));
    let mut xo = Dictionary::new();
    xo.set("Im0", Object::Reference(img.id));
    let mut res = Dictionary::new();
    res.set("XObject", Object::Dictionary(xo));
    let pages_id = doc.new_object_id();
    let mut d = Dictionary::new();
    d.set("Type", name("Page"));
    d.set("Parent", Object::Reference(pages_id));
    d.set("MediaBox", Object::Array(vec![real(0.0), real(0.0), real(pw), real(ph)]));
    d.set("Resources", Object::Dictionary(res));
    d.set("Contents", Object::Reference(cid));
    let page_id = doc.add_object(Object::Dictionary(d));
    let mut pages = Dictionary::new();
    pages.set("Type", name("Pages"));
    pages.set("Count", Object::Integer(1));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let mut cat = Dictionary::new();
    cat.set("Type", name("Catalog"));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = doc.add_object(Object::Dictionary(cat));
    doc.trailer.set("Root", Object::Reference(cat_id));
    save(&mut doc).map_err(|e| anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wraps_long_text() {
        let l = wrap("the quick brown fox jumps over the lazy dog ".repeat(10).as_str(), Font::Helv, 11.0, 200.0);
        assert!(l.len() > 3);
        assert!(l.iter().all(|s| text_width(Font::Helv, s, 11.0) <= 201.0));
    }
}
