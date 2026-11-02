//! Page-level PDF editing on top of `lopdf`: assemble/merge/split, rotate, crop, overlays
//! (watermark, numbers, header/footer, stamp), metadata, cleaning, image embedding.
//!
//! Only the stable core object API of lopdf is used (Document/Object/Dictionary/Stream).
//! The few version-sensitive spots (`real`, `num`) are isolated at the top of this file.
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use flate2::{write::ZlibEncoder, Compression};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use std::collections::HashMap;
use std::io::Write;

// ---------------------------------------------------------------- tiny object helpers
pub fn name(s: &str) -> Object {
    Object::Name(s.as_bytes().to_vec())
}
/// If a newer lopdf stores reals as f64, change only this function.
pub fn real(x: f32) -> Object {
    Object::Real(x)
}
pub fn num(o: &Object) -> Option<f32> {
    match o {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r as f32),
        _ => None,
    }
}
static NULL: Object = Object::Null;
pub fn resolve<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    let mut cur = o;
    for _ in 0..24 {
        match cur {
            Object::Reference(id) => match doc.objects.get(id) {
                Some(n) => cur = n,
                None => return &NULL,
            },
            _ => return cur,
        }
    }
    cur
}
fn dict_of(doc: &Document, o: &Object) -> Option<Dictionary> {
    match resolve(doc, o) {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Stream(s) => Some(s.dict.clone()),
        _ => None,
    }
}

// ---------------------------------------------------------------- load / save
pub fn load(bytes: &[u8]) -> Result<Document> {
    let mut d = Document::load_mem(bytes).map_err(|e| anyhow!("Could not read this PDF: {e}"))?;
    if d.is_encrypted() {
        d.decrypt("").map_err(|_| anyhow!("This PDF is password-protected. Remove the password first."))?;
    }
    Ok(d)
}
pub fn save(doc: &mut Document) -> Result<Vec<u8>> {
    doc.prune_objects();
    doc.compress();
    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(|e| anyhow!("Could not write the PDF: {e}"))?;
    Ok(buf)
}
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().values().copied().collect()
}
pub fn page_count(bytes: &[u8]) -> Result<usize> {
    Ok(page_ids(&load(bytes)?).len())
}

pub fn inherited(doc: &Document, page: ObjectId, key: &[u8]) -> Option<Object> {
    let mut id = page;
    for _ in 0..32 {
        let d = doc.get_dictionary(id).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(resolve(doc, v).clone());
        }
        id = match d.get(b"Parent").ok()? {
            Object::Reference(r) => *r,
            _ => return None,
        };
    }
    None
}
fn box_of(doc: &Document, page: ObjectId, key: &[u8]) -> Option<[f32; 4]> {
    if let Some(Object::Array(a)) = inherited(doc, page, key) {
        let v: Vec<f32> = a.iter().filter_map(|o| num(resolve(doc, o))).collect();
        if v.len() == 4 {
            return Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]);
        }
    }
    None
}
/// Visible page box [x0, y0, x1, y1] (CropBox, else MediaBox, else A4).
pub fn page_box(doc: &Document, page: ObjectId) -> [f32; 4] {
    box_of(doc, page, b"CropBox").or_else(|| box_of(doc, page, b"MediaBox")).unwrap_or([0.0, 0.0, 595.28, 841.89])
}
pub fn page_size(doc: &Document, page: ObjectId) -> (f32, f32) {
    let b = page_box(doc, page);
    (b[2] - b[0], b[3] - b[1])
}
pub fn page_rotation(doc: &Document, page: ObjectId) -> i32 {
    match inherited(doc, page, b"Rotate") {
        Some(o) => (num(&o).unwrap_or(0.0) as i32).rem_euclid(360),
        None => 0,
    }
}

// ---------------------------------------------------------------- fonts & text encoding
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    Helv,
    Bold,
    Cour,
}
impl Font {
    pub fn res_name(self) -> &'static str {
        match self {
            Font::Helv => "OmniHelv",
            Font::Bold => "OmniBold",
            Font::Cour => "OmniCour",
        }
    }
    fn base_font(self) -> &'static str {
        match self {
            Font::Helv => "Helvetica",
            Font::Bold => "Helvetica-Bold",
            Font::Cour => "Courier",
        }
    }
}
// Adobe AFM advance widths for ASCII 32..=126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278,
    278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722,
    667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556,
    556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333,
    333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722,
    667, 944, 667, 667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611,
    611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];
pub fn char_w(f: Font, c: char) -> f32 {
    if f == Font::Cour {
        return 0.6;
    }
    let t = if f == Font::Bold { &BOLD } else { &HELV };
    let cp = c as u32;
    if (32..=126).contains(&cp) {
        t[(cp - 32) as usize] as f32 / 1000.0
    } else {
        0.556 // Latin-1 supplement: close enough for layout
    }
}
pub fn text_width(f: Font, s: &str, size: f32) -> f32 {
    s.chars().map(|c| char_w(f, c)).sum::<f32>() * size
}
/// Text -> WinAnsi bytes. Characters outside WinAnsi become '?'.
pub fn winansi(s: &str) -> Vec<u8> {
    s.chars()
        .map(|c| {
            let cp = c as u32;
            match c {
                '\t' => b' ',
                '\u{00a0}' => b' ',
                '€' => 0x80,
                '…' => 0x85,
                '‘' => 0x91,
                '’' => 0x92,
                '“' => 0x93,
                '”' => 0x94,
                '•' => 0x95,
                '–' => 0x96,
                '—' => 0x97,
                '™' => 0x99,
                _ if (32..=126).contains(&cp) || (0xa1..=0xff).contains(&cp) => cp as u8,
                _ => b'?',
            }
        })
        .collect()
}
/// What the PDF will really show for this text (used for wrapping widths).
pub fn safe_text(s: &str) -> String {
    winansi(s)
        .into_iter()
        .map(|b| match b {
            0x80 => '€',
            0x85 => '…',
            0x91 => '‘',
            0x92 => '’',
            0x93 => '“',
            0x94 => '”',
            0x95 => '•',
            0x96 => '–',
            0x97 => '—',
            0x99 => '™',
            b => b as char,
        })
        .collect()
}

// ---------------------------------------------------------------- content stream builder
#[derive(Default)]
pub struct Content(pub Vec<u8>);
fn n(x: f32) -> String {
    let s = format!("{:.3}", x);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
impl Content {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn op(&mut self, s: &str) -> &mut Self {
        self.0.extend_from_slice(s.as_bytes());
        self.0.push(b'\n');
        self
    }
    pub fn fill_rgb(&mut self, c: (f32, f32, f32)) -> &mut Self {
        self.op(&format!("{} {} {} rg", n(c.0), n(c.1), n(c.2)))
    }
    pub fn stroke_rgb(&mut self, c: (f32, f32, f32)) -> &mut Self {
        self.op(&format!("{} {} {} RG", n(c.0), n(c.1), n(c.2)))
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, fill: bool) -> &mut Self {
        self.op(&format!("{} {} {} {} re {}", n(x), n(y), n(w), n(h), if fill { "f" } else { "S" }))
    }
    pub fn line_width(&mut self, w: f32) -> &mut Self {
        self.op(&format!("{} w", n(w)))
    }
    pub fn gs(&mut self, res: &str) -> &mut Self {
        self.op(&format!("/{res} gs"))
    }
    /// Draw text. `rot` in degrees, `mode` is the text render mode (0 = fill, 3 = invisible).
    pub fn text(&mut self, font: Font, size: f32, x: f32, y: f32, rot: f32, mode: u8, s: &str) -> &mut Self {
        let a = rot.to_radians();
        let (c, sn) = (a.cos(), a.sin());
        self.op("BT");
        self.op(&format!("{mode} Tr"));
        self.op(&format!("/{} {} Tf", font.res_name(), n(size)));
        self.op(&format!("{} {} {} {} {} {} Tm", n(c), n(sn), n(-sn), n(c), n(x), n(y)));
        self.0.push(b'(');
        for b in winansi(s) {
            if b == b'(' || b == b')' || b == b'\\' {
                self.0.push(b'\\');
            }
            self.0.push(b);
        }
        self.0.extend_from_slice(b") Tj\n");
        self.op("ET")
    }
    pub fn image(&mut self, res: &str, x: f32, y: f32, w: f32, h: f32) -> &mut Self {
        self.op(&format!("q {} 0 0 {} {} {} cm /{res} Do Q", n(w), n(h), n(x), n(y)))
    }
}

/// Everything an overlay needs registered in the page's /Resources.
#[derive(Default)]
pub struct Overlay {
    pub content: Content,
    pub fonts: Vec<(String, ObjectId)>,
    pub gs: Vec<(String, ObjectId)>,
    pub xobj: Vec<(String, ObjectId)>,
}

pub fn font_object(doc: &mut Document, f: Font) -> ObjectId {
    let mut d = Dictionary::new();
    d.set("Type", name("Font"));
    d.set("Subtype", name("Type1"));
    d.set("BaseFont", name(f.base_font()));
    d.set("Encoding", name("WinAnsiEncoding"));
    doc.add_object(Object::Dictionary(d))
}
pub fn gs_object(doc: &mut Document, alpha: f32, multiply: bool) -> ObjectId {
    let mut d = Dictionary::new();
    d.set("Type", name("ExtGState"));
    d.set("ca", real(alpha));
    d.set("CA", real(alpha));
    if multiply {
        d.set("BM", name("Multiply"));
    }
    doc.add_object(Object::Dictionary(d))
}

fn merge_res(doc: &Document, res: &mut Dictionary, key: &str, items: &[(String, ObjectId)]) {
    if items.is_empty() {
        return;
    }
    let mut sub = res.get(key.as_bytes()).ok().and_then(|o| dict_of(doc, o)).unwrap_or_default();
    for (nm, id) in items {
        sub.set(nm.as_str(), Object::Reference(*id));
    }
    res.set(key, Object::Dictionary(sub));
}

impl Overlay {
    pub fn new() -> Self {
        Self::default()
    }
    /// Append this overlay on top of the page (wrapped in q/Q so it cannot disturb existing content).
    pub fn apply(self, doc: &mut Document, page: ObjectId) -> Result<()> {
        let mut res = inherited(doc, page, b"Resources").and_then(|o| dict_of(doc, &o)).unwrap_or_default();
        merge_res(doc, &mut res, "Font", &self.fonts);
        merge_res(doc, &mut res, "ExtGState", &self.gs);
        merge_res(doc, &mut res, "XObject", &self.xobj);

        let existing: Vec<Object> = match doc.get_dictionary(page)?.get(b"Contents") {
            Ok(Object::Reference(r)) => match doc.objects.get(r) {
                Some(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*r)],
            },
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![],
        };
        let open = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let mut body = b"Q\nq\n".to_vec();
        body.extend_from_slice(&self.content.0);
        body.extend_from_slice(b"Q\n");
        let mine = doc.add_object(Stream::new(Dictionary::new(), body));
        let mut arr = vec![Object::Reference(open)];
        arr.extend(existing);
        arr.push(Object::Reference(mine));
        let p = doc.get_object_mut(page)?.as_dict_mut()?;
        p.set("Contents", Object::Array(arr));
        p.set("Resources", Object::Dictionary(res));
        Ok(())
    }
}

// ---------------------------------------------------------------- image embedding
pub struct Img {
    pub id: ObjectId,
    pub w: u32,
    pub h: u32,
}
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}
fn image_stream(w: u32, h: u32, cs: &str, filter: &str, data: Vec<u8>, smask: Option<ObjectId>) -> Stream {
    let mut d = Dictionary::new();
    d.set("Type", name("XObject"));
    d.set("Subtype", name("Image"));
    d.set("Width", Object::Integer(w as i64));
    d.set("Height", Object::Integer(h as i64));
    d.set("ColorSpace", name(cs));
    d.set("BitsPerComponent", Object::Integer(8));
    d.set("Filter", name(filter));
    if let Some(m) = smask {
        d.set("SMask", Object::Reference(m));
    }
    let mut s = Stream::new(d, data);
    s.allows_compression = false;
    s
}
/// (width, height, components) from the first SOF marker.
pub fn jpeg_info(b: &[u8]) -> Option<(u32, u32, u8)> {
    let mut i = 2;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if (0xC0..=0xCF).contains(&m) && m != 0xC4 && m != 0xC8 && m != 0xCC {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h, b[i + 9]));
        }
        if m == 0xD8 || (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        i += 2 + len;
    }
    None
}
pub fn embed_jpeg(doc: &mut Document, bytes: &[u8], w: u32, h: u32, comps: u8) -> Img {
    let cs = if comps == 1 { "DeviceGray" } else { "DeviceRGB" };
    let id = doc.add_object(image_stream(w, h, cs, "DCTDecode", bytes.to_vec(), None));
    Img { id, w, h }
}
/// Embed any picture: JPEG is copied as-is; everything else is decoded and stored losslessly (with alpha as SMask).
pub fn embed_image(doc: &mut Document, bytes: &[u8]) -> Result<Img> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        if let Some((w, h, c)) = jpeg_info(bytes) {
            if c == 1 || c == 3 {
                return Ok(embed_jpeg(doc, bytes, w, h, c));
            }
        }
    }
    let img = image::load_from_memory(bytes).map_err(|e| anyhow!("Could not decode the picture: {e}"))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let mut alpha = Vec::with_capacity((w * h) as usize);
    let mut has_alpha = false;
    for p in rgba.pixels() {
        rgb.extend_from_slice(&p.0[..3]);
        alpha.push(p.0[3]);
        has_alpha |= p.0[3] < 255;
    }
    let smask = if has_alpha { Some(doc.add_object(image_stream(w, h, "DeviceGray", "FlateDecode", deflate(&alpha), None))) } else { None };
    let id = doc.add_object(image_stream(w, h, "DeviceRGB", "FlateDecode", deflate(&rgb), smask));
    Ok(Img { id, w, h })
}

// ---------------------------------------------------------------- copying pages between documents
const SKIP_KEYS: [&[u8]; 8] = [b"Parent", b"P", b"Dest", b"A", b"AA", b"B", b"Thumb", b"StructParents"];

fn copy_obj(dst: &mut Document, src: &Document, obj: &Object, map: &mut HashMap<ObjectId, ObjectId>) -> Object {
    match obj {
        Object::Reference(id) => {
            if let Some(n) = map.get(id) {
                return Object::Reference(*n);
            }
            let new_id = dst.new_object_id();
            map.insert(*id, new_id);
            let o = src.objects.get(id).cloned().unwrap_or(Object::Null);
            let c = copy_obj(dst, src, &o, map);
            dst.objects.insert(new_id, c);
            Object::Reference(new_id)
        }
        Object::Array(a) => Object::Array(a.iter().map(|x| copy_obj(dst, src, x, map)).collect()),
        Object::Dictionary(d) => Object::Dictionary(copy_dict(dst, src, d, map)),
        Object::Stream(s) => {
            let mut ns = s.clone();
            ns.dict = copy_dict(dst, src, &s.dict, map);
            Object::Stream(ns)
        }
        o => o.clone(),
    }
}
fn copy_dict(dst: &mut Document, src: &Document, d: &Dictionary, map: &mut HashMap<ObjectId, ObjectId>) -> Dictionary {
    let mut nd = Dictionary::new();
    for (k, v) in d.iter() {
        if SKIP_KEYS.iter().any(|s| *s == k.as_slice()) {
            continue;
        }
        nd.set(k.clone(), copy_obj(dst, src, v, map));
    }
    nd
}

/// One page of the output document.
pub enum Item {
    /// A page of source document `doc`, optionally rotated by `rot` degrees on top of its own rotation.
    Page { doc: usize, index: usize, rot: i32 },
    Blank { w: f32, h: f32 },
    /// A full-page JPEG (used by compress / grayscale / redaction).
    Raster { jpeg: Vec<u8>, pw: f32, ph: f32 },
}

/// Build a new PDF from pages of one or more source documents, in the given order (duplicates allowed).
/// Interactive form fields and internal links are not carried over.
pub fn assemble(srcs: &[Document], items: &[Item], ctx: &Ctx) -> Result<Vec<u8>> {
    let mut out = Document::with_version("1.7");
    let pages_id = out.new_object_id();
    let mut maps: Vec<HashMap<ObjectId, ObjectId>> = vec![HashMap::new(); srcs.len()];
    let src_pages: Vec<Vec<ObjectId>> = srcs.iter().map(page_ids).collect();
    let mut kids: Vec<Object> = Vec::new();

    for (k, it) in items.iter().enumerate() {
        ctx.check()?;
        if k % 8 == 0 {
            ctx.progress(k, items.len(), "Building pages");
        }
        let page_obj = match it {
            Item::Page { doc, index, rot } => {
                let src = &srcs[*doc];
                let pid = *src_pages[*doc].get(*index).ok_or_else(|| anyhow!("Page {} does not exist.", index + 1))?;
                let mut d = src.get_dictionary(pid)?.clone();
                for key in ["Resources", "MediaBox", "CropBox", "Rotate"] {
                    if !d.has(key.as_bytes()) {
                        if let Some(v) = inherited(src, pid, key.as_bytes()) {
                            d.set(key, v);
                        }
                    }
                }
                if let Ok(a) = d.get(b"Annots") {
                    if let Object::Array(arr) = resolve(src, a) {
                        let keep: Vec<Object> = arr
                            .iter()
                            .filter(|o| match dict_of(src, o).and_then(|a| a.get(b"Subtype").ok().cloned()) {
                                Some(Object::Name(n)) => n.as_slice() != b"Link",
                                _ => true,
                            })
                            .cloned()
                            .collect();
                        d.set("Annots", Object::Array(keep));
                    }
                }
                let mut nd = copy_dict(&mut out, src, &d, &mut maps[*doc]);
                if *rot != 0 {
                    let cur = page_rotation(src, pid);
                    nd.set("Rotate", Object::Integer(((cur + rot) % 360 + 360) as i64 % 360));
                }
                nd.set("Type", name("Page"));
                nd.set("Parent", Object::Reference(pages_id));
                Object::Dictionary(nd)
            }
            Item::Blank { w, h } => {
                let mut d = Dictionary::new();
                d.set("Type", name("Page"));
                d.set("Parent", Object::Reference(pages_id));
                d.set("MediaBox", Object::Array(vec![real(0.0), real(0.0), real(*w), real(*h)]));
                d.set("Resources", Object::Dictionary(Dictionary::new()));
                d.set("Contents", Object::Reference(out.add_object(Stream::new(Dictionary::new(), Vec::new()))));
                Object::Dictionary(d)
            }
            Item::Raster { jpeg, pw, ph } => {
                let (w, h, c) = jpeg_info(jpeg).ok_or_else(|| anyhow!("Internal error: bad JPEG."))?;
                let img = embed_jpeg(&mut out, jpeg, w, h, c);
                let mut c = Content::new();
                c.image("Im0", 0.0, 0.0, *pw, *ph);
                let cid = out.add_object(Stream::new(Dictionary::new(), c.0));
                let mut x = Dictionary::new();
                x.set("Im0", Object::Reference(img.id));
                let mut r = Dictionary::new();
                r.set("XObject", Object::Dictionary(x));
                let mut d = Dictionary::new();
                d.set("Type", name("Page"));
                d.set("Parent", Object::Reference(pages_id));
                d.set("MediaBox", Object::Array(vec![real(0.0), real(0.0), real(*pw), real(*ph)]));
                d.set("Resources", Object::Dictionary(r));
                d.set("Contents", Object::Reference(cid));
                Object::Dictionary(d)
            }
        };
        kids.push(Object::Reference(out.add_object(page_obj)));
    }
    if kids.is_empty() {
        bail!("There are no pages to write.");
    }
    let mut pages = Dictionary::new();
    pages.set("Type", name("Pages"));
    pages.set("Count", Object::Integer(kids.len() as i64));
    pages.set("Kids", Object::Array(kids));
    out.objects.insert(pages_id, Object::Dictionary(pages));
    let mut cat = Dictionary::new();
    cat.set("Type", name("Catalog"));
    cat.set("Pages", Object::Reference(pages_id));
    let cat_id = out.add_object(Object::Dictionary(cat));
    out.trailer.set("Root", Object::Reference(cat_id));
    ctx.progress(items.len(), items.len(), "Writing file");
    save(&mut out)
}

fn items_for(doc_idx: usize, idx: &[usize]) -> Vec<Item> {
    idx.iter().map(|&i| Item::Page { doc: doc_idx, index: i, rot: 0 }).collect()
}

// ---------------------------------------------------------------- page operations
pub fn merge(list: &[Vec<u8>], ctx: &Ctx) -> Result<Vec<u8>> {
    let docs: Vec<Document> = list.iter().map(|b| load(b)).collect::<Result<_>>()?;
    let mut items = Vec::new();
    for (di, d) in docs.iter().enumerate() {
        items.extend(items_for(di, &(0..page_ids(d).len()).collect::<Vec<_>>()));
    }
    assemble(&docs, &items, ctx)
}
pub fn pick_pages(bytes: &[u8], idx: &[usize]) -> Result<Vec<u8>> {
    let d = load(bytes)?;
    assemble(std::slice::from_ref(&d), &items_for(0, idx), &Ctx::silent())
}
pub fn split_groups(bytes: &[u8], spec: &str, file: &str) -> Result<Vec<Output>> {
    let total = page_count(bytes)?;
    let groups = parse_groups(spec, total)?;
    groups
        .iter()
        .enumerate()
        .map(|(i, g)| Ok(Output::new(format!("{}-part{}.pdf", base_name(file), i + 1), pick_pages(bytes, g)?).note(format!("{} page(s)", g.len()))))
        .collect()
}
pub fn split_every(bytes: &[u8], file: &str, step: usize) -> Result<Vec<Output>> {
    let total = page_count(bytes)?;
    let step = step.max(1);
    let mut out = Vec::new();
    let mut s = 0;
    while s < total {
        let idx: Vec<usize> = (s..(s + step).min(total)).collect();
        out.push(Output::new(
            format!("{}-{}{:03}.pdf", base_name(file), if step == 1 { "page" } else { "part" }, out.len() + 1),
            pick_pages(bytes, &idx)?,
        ));
        s += step;
    }
    Ok(out)
}
pub fn reverse(bytes: &[u8]) -> Result<Vec<u8>> {
    let n = page_count(bytes)?;
    pick_pages(bytes, &(0..n).rev().collect::<Vec<_>>())
}
pub fn insert_blank(bytes: &[u8], after: usize, count: usize) -> Result<Vec<u8>> {
    let d = load(bytes)?;
    let ids = page_ids(&d);
    let n = ids.len();
    if n == 0 {
        bail!("This PDF has no pages.");
    }
    let at = after.min(n);
    let (w, h) = page_size(&d, ids[at.saturating_sub(1).min(n - 1)]);
    let mut items = Vec::new();
    for i in 0..n {
        if i == at {
            items.extend((0..count).map(|_| Item::Blank { w, h }));
        }
        items.push(Item::Page { doc: 0, index: i, rot: 0 });
    }
    if at >= n {
        items.extend((0..count).map(|_| Item::Blank { w, h }));
    }
    assemble(std::slice::from_ref(&d), &items, &Ctx::silent())
}
/// Rotate in place (keeps forms and links).
pub fn rotate(bytes: &[u8], spec: &str, angle: i32) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    for i in unique_sorted(parse_pages(spec, ids.len())?) {
        let cur = page_rotation(&d, ids[i]);
        d.get_object_mut(ids[i])?.as_dict_mut()?.set("Rotate", Object::Integer(((cur + angle) % 360 + 360) as i64 % 360));
    }
    save(&mut d)
}
pub fn crop(bytes: &[u8], spec: &str, top: f32, bottom: f32, left: f32, right: f32) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    for i in unique_sorted(parse_pages(spec, ids.len())?) {
        let b = page_box(&d, ids[i]);
        let (w, h) = (b[2] - b[0] - left - right, b[3] - b[1] - top - bottom);
        if w < 20.0 || h < 20.0 {
            bail!("Margins are larger than the page.");
        }
        let arr = vec![real(b[0] + left), real(b[1] + bottom), real(b[0] + left + w), real(b[1] + bottom + h)];
        d.get_object_mut(ids[i])?.as_dict_mut()?.set("CropBox", Object::Array(arr));
    }
    save(&mut d)
}

// ---------------------------------------------------------------- overlays
fn anchor_x(pos: &str, bx: f32, page_w: f32, text_w: f32, margin: f32) -> f32 {
    if pos.ends_with("left") {
        bx + margin
    } else if pos.ends_with("right") {
        bx + page_w - margin - text_w
    } else {
        bx + (page_w - text_w) / 2.0
    }
}
fn tokens(t: &str, n: i64, total: i64, file: &str) -> String {
    t.replace("{n}", &n.to_string()).replace("{total}", &total.to_string()).replace("{date}", &today()).replace("{file}", file)
}
pub fn today() -> String {
    // civil-from-days (Howard Hinnant) so no date crate is needed
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let z = secs / 86400 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

pub struct WatermarkOpts<'a> {
    pub text: &'a str,
    pub size: f32,
    pub color: &'a str,
    pub opacity: f32,
    pub angle: f32,
    pub pages: &'a str,
}
pub fn watermark(bytes: &[u8], o: &WatermarkOpts) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    let font = font_object(&mut d, Font::Bold);
    let gs = gs_object(&mut d, o.opacity.clamp(0.0, 1.0), false);
    let a = o.angle.to_radians();
    let w = text_width(Font::Bold, o.text, o.size);
    let half = o.size * 0.3;
    for i in unique_sorted(parse_pages(o.pages, ids.len())?) {
        let b = page_box(&d, ids[i]);
        let (pw, ph) = (b[2] - b[0], b[3] - b[1]);
        let x = b[0] + pw / 2.0 - (w / 2.0) * a.cos() + half * a.sin();
        let y = b[1] + ph / 2.0 - (w / 2.0) * a.sin() - half * a.cos();
        let mut ov = Overlay::new();
        ov.fonts.push((Font::Bold.res_name().into(), font));
        ov.gs.push(("OmniGs".into(), gs));
        ov.content.gs("OmniGs").fill_rgb(hex_rgb(o.color)).text(Font::Bold, o.size, x, y, o.angle, 0, o.text);
        ov.apply(&mut d, ids[i])?;
    }
    save(&mut d)
}
pub fn page_numbers(bytes: &[u8], format: &str, position: &str, start: i64, size: f32, pages: &str) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    let total = ids.len() as i64;
    let font = font_object(&mut d, Font::Helv);
    for i in unique_sorted(parse_pages(pages, ids.len())?) {
        let b = page_box(&d, ids[i]);
        let t = tokens(format, i as i64 + start, total + start - 1, "");
        let w = text_width(Font::Helv, &safe_text(&t), size);
        let x = anchor_x(position, b[0], b[2] - b[0], w, 36.0);
        let y = if position.starts_with("top") { b[3] - 28.0 } else { b[1] + 22.0 };
        let mut ov = Overlay::new();
        ov.fonts.push((Font::Helv.res_name().into(), font));
        ov.content.fill_rgb((0.2, 0.2, 0.25)).text(Font::Helv, size, x, y, 0.0, 0, &t);
        ov.apply(&mut d, ids[i])?;
    }
    save(&mut d)
}
pub fn header_footer(bytes: &[u8], slots: &[(&str, &str, String)], size: f32, file: &str) -> Result<Vec<u8>> {
    // slots: (row "header"/"footer", column "left"/"center"/"right", raw text)
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    let total = ids.len() as i64;
    let font = font_object(&mut d, Font::Helv);
    for (i, &pid) in ids.iter().enumerate() {
        let b = page_box(&d, pid);
        let mut ov = Overlay::new();
        ov.fonts.push((Font::Helv.res_name().into(), font));
        ov.content.fill_rgb((0.25, 0.25, 0.3));
        let mut any = false;
        for (row, col, raw) in slots {
            if raw.is_empty() {
                continue;
            }
            any = true;
            let t = tokens(raw, i as i64 + 1, total, file);
            let w = text_width(Font::Helv, &safe_text(&t), size);
            let pos = if *row == "header" { format!("top{col}") } else { format!("bottom{col}") };
            let x = anchor_x(&pos, b[0], b[2] - b[0], w, 36.0);
            let y = if *row == "header" { b[3] - 28.0 } else { b[1] + 22.0 };
            ov.content.text(Font::Helv, size, x, y, 0.0, 0, &t);
        }
        if any {
            ov.apply(&mut d, pid)?;
        }
    }
    save(&mut d)
}
pub fn stamp(bytes: &[u8], text: &str, size: f32, color: &str, position: &str, pages: &str) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let ids = page_ids(&d);
    let font = font_object(&mut d, Font::Bold);
    let col = hex_rgb(color);
    for i in unique_sorted(parse_pages(pages, ids.len())?) {
        let b = page_box(&d, ids[i]);
        let (pw, ph) = (b[2] - b[0], b[3] - b[1]);
        let w = text_width(Font::Bold, &safe_text(text), size);
        let pad = 8.0;
        let (bw, bh) = (w + pad * 2.0, size + pad * 2.0);
        let x = b[0] + if position.ends_with("left") { 36.0 } else if position.ends_with("right") { pw - 36.0 - bw } else { (pw - bw) / 2.0 };
        let y = b[1] + if position.starts_with("top") { ph - 36.0 - bh } else if position.starts_with("middle") { (ph - bh) / 2.0 } else { 36.0 };
        let mut ov = Overlay::new();
        ov.fonts.push((Font::Bold.res_name().into(), font));
        ov.content.stroke_rgb(col).line_width(2.0).rect(x, y, bw, bh, false).fill_rgb(col).text(
            Font::Bold,
            size,
            x + pad,
            y + pad + size * 0.2,
            0.0,
            0,
            text,
        );
        ov.apply(&mut d, ids[i])?;
    }
    save(&mut d)
}
/// "Light" compression: drop unused objects and deflate streams. Text stays selectable.
pub fn optimize(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    d.delete_zero_length_streams();
    save(&mut d)
}

// ---------------------------------------------------------------- metadata / cleaning
#[derive(Clone, Default, Debug)]
pub struct Meta {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
    pub creator: String,
    pub producer: String,
    pub created: String,
    pub modified: String,
    pub pages: usize,
    pub first_page: String,
    pub encrypted: bool,
}
fn decode_pdf_string(b: &[u8]) -> String {
    if b.starts_with(&[0xFE, 0xFF]) {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        b.iter().map(|&c| c as char).collect()
    }
}
fn encode_pdf_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut v = vec![0xFE, 0xFF];
        for u in s.encode_utf16() {
            v.extend_from_slice(&u.to_be_bytes());
        }
        Object::String(v, StringFormat::Literal)
    }
}
fn info_id(doc: &Document) -> Option<ObjectId> {
    match doc.trailer.get(b"Info") {
        Ok(Object::Reference(r)) => Some(*r),
        _ => None,
    }
}
pub fn read_meta(bytes: &[u8]) -> Result<Meta> {
    let raw = Document::load_mem(bytes).map_err(|e| anyhow!("Could not read this PDF: {e}"))?;
    let encrypted = raw.is_encrypted();
    let d = load(bytes)?;
    let mut m = Meta { encrypted, ..Default::default() };
    if let Some(id) = info_id(&d) {
        if let Ok(info) = d.get_dictionary(id) {
            let get = |k: &[u8]| -> String {
                match info.get(k).ok().map(|o| resolve(&d, o)) {
                    Some(Object::String(b, _)) => decode_pdf_string(b),
                    _ => String::new(),
                }
            };
            m.title = get(b"Title");
            m.author = get(b"Author");
            m.subject = get(b"Subject");
            m.keywords = get(b"Keywords");
            m.creator = get(b"Creator");
            m.producer = get(b"Producer");
            m.created = get(b"CreationDate");
            m.modified = get(b"ModDate");
        }
    }
    let ids = page_ids(&d);
    m.pages = ids.len();
    if let Some(&p) = ids.first() {
        let (w, h) = page_size(&d, p);
        m.first_page = format!("{:.0} x {:.0} pt", w, h);
    }
    Ok(m)
}
pub fn write_meta(bytes: &[u8], m: &Meta) -> Result<Vec<u8>> {
    let mut d = load(bytes)?;
    let id = match info_id(&d) {
        Some(id) if d.get_dictionary(id).is_ok() => id,
        _ => {
            let id = d.add_object(Object::Dictionary(Dictionary::new()));
            d.trailer.set("Info", Object::Reference(id));
            id
        }
    };
    let info = d.get_object_mut(id)?.as_dict_mut()?;
    for (k, v) in [("Title", &m.title), ("Author", &m.author), ("Subject", &m.subject), ("Keywords", &m.keywords), ("Creator", &m.creator), ("Producer", &m.producer)] {
        if v.trim().is_empty() {
            info.remove(k.as_bytes());
        } else {
            info.set(k, encode_pdf_string(v));
        }
    }
    save(&mut d)
}

pub struct SanitizeOpts {
    pub metadata: bool,
    pub scripts: bool,
    pub attachments: bool,
}
pub fn sanitize(bytes: &[u8], o: &SanitizeOpts) -> Result<(Vec<u8>, Vec<String>)> {
    let mut d = load(bytes)?;
    let mut found = Vec::new();
    if o.metadata {
        if d.trailer.has(b"Info") {
            d.trailer.remove(b"Info");
            found.push("document info".to_string());
        }
    }
    let root = match d.trailer.get(b"Root") {
        Ok(Object::Reference(r)) => *r,
        _ => bail!("This PDF has no catalog."),
    };
    let names_dict = {
        let cat = d.get_dictionary(root)?;
        cat.get(b"Names").ok().and_then(|o| match o {
            Object::Reference(r) => Some(*r),
            _ => None,
        })
    };
    {
        let cat = d.get_object_mut(root)?.as_dict_mut()?;
        if o.metadata && cat.has(b"Metadata") {
            cat.remove(b"Metadata");
            found.push("XMP metadata".into());
        }
        if o.scripts {
            for k in ["OpenAction", "AA"] {
                if cat.has(k.as_bytes()) {
                    cat.remove(k.as_bytes());
                    found.push(format!("catalog {k}"));
                }
            }
        }
        if o.attachments && cat.has(b"AF") {
            cat.remove(b"AF");
        }
        // inline /Names dictionary
        if let Ok(Object::Dictionary(names)) = cat.get_mut(b"Names") {
            if o.scripts && names.has(b"JavaScript") {
                names.remove(b"JavaScript");
                found.push("JavaScript".into());
            }
            if o.attachments && names.has(b"EmbeddedFiles") {
                names.remove(b"EmbeddedFiles");
                found.push("embedded files".into());
            }
        }
    }
    if let Some(nid) = names_dict {
        if let Ok(Object::Dictionary(names)) = d.get_object_mut(nid) {
            if o.scripts && names.has(b"JavaScript") {
                names.remove(b"JavaScript");
                found.push("JavaScript".into());
            }
            if o.attachments && names.has(b"EmbeddedFiles") {
                names.remove(b"EmbeddedFiles");
                found.push("embedded files".into());
            }
        }
    }
    Ok((save(&mut d)?, found))
}
/// Remove comments, highlights, stamps (optionally links). Form widgets are kept.
pub fn remove_annotations(bytes: &[u8], keep_links: bool) -> Result<(Vec<u8>, usize)> {
    let mut d = load(bytes)?;
    let mut removed = 0usize;
    for pid in page_ids(&d) {
        let annots: Option<Vec<Object>> = d.get_dictionary(pid)?.get(b"Annots").ok().and_then(|o| match resolve(&d, o) {
            Object::Array(a) => Some(a.clone()),
            _ => None,
        });
        let Some(arr) = annots else { continue };
        let mut keep = Vec::new();
        for o in arr {
            let st = dict_of(&d, &o).and_then(|a| a.get(b"Subtype").ok().cloned());
            let is = |s: &[u8]| matches!(&st, Some(Object::Name(n)) if n.as_slice() == s);
            if st.is_none() || is(b"Widget") || (keep_links && is(b"Link")) {
                keep.push(o);
            } else {
                removed += 1;
            }
        }
        d.get_object_mut(pid)?.as_dict_mut()?.set("Annots", Object::Array(keep));
    }
    Ok((save(&mut d)?, removed))
}
pub fn annotation_count(doc: &Document, page: ObjectId) -> usize {
    doc.get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Annots").ok())
        .map(|o| match resolve(doc, o) {
            Object::Array(a) => a.len(),
            _ => 0,
        })
        .unwrap_or(0)
}
/// Embedded JPEG pictures, byte-for-byte. Returns (jpegs, count_of_other_encodings).
pub fn extract_jpegs(bytes: &[u8], min_kb: usize) -> Result<(Vec<Vec<u8>>, usize)> {
    let d = load(bytes)?;
    let mut out = Vec::new();
    let mut other = 0;
    for (_, obj) in d.objects.iter() {
        let Object::Stream(s) = obj else { continue };
        let is_img = matches!(s.dict.get(b"Subtype"), Ok(Object::Name(n)) if n.as_slice() == b"Image");
        if !is_img {
            continue;
        }
        let dct = match s.dict.get(b"Filter") {
            Ok(Object::Name(n)) => n.as_slice() == b"DCTDecode",
            Ok(Object::Array(a)) => a.iter().any(|o| matches!(o, Object::Name(n) if n.as_slice() == b"DCTDecode")),
            _ => false,
        };
        if dct {
            if s.content.len() >= min_kb * 1024 {
                out.push(s.content.clone());
            }
        } else {
            other += 1;
        }
    }
    Ok((out, other))
}
