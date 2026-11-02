//! Annotations (highlight, text, signature picture, redaction) and AcroForm filling.
use crate::gen::wrap;
use crate::pdfops::*;
use crate::render::*;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use lopdf::{Document, Object, ObjectId, StringFormat};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnnotKind {
    Highlight,
    Text,
    Image,
    Redact,
}

/// Coordinates are fractions of the page (0..1) with the origin at the top-left corner.
#[derive(Clone, Debug)]
pub struct Annot {
    pub page: usize,
    pub kind: AnnotKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub text: String,
    pub size: f32,
    pub color: String,
    pub image: Option<Vec<u8>>,
}

/// Returns (pdf bytes, number of pages that were redacted).
pub fn apply(bytes: &[u8], annots: &[Annot], ctx: &Ctx) -> Result<(Vec<u8>, usize)> {
    let mut doc = load(bytes)?;
    let ids = page_ids(&doc);
    let font = font_object(&mut doc, Font::Helv);
    let multiply = gs_object(&mut doc, 0.4, true);
    let mut overlays: HashMap<usize, Overlay> = HashMap::new();
    let mut n_img = 0;

    for a in annots.iter().filter(|a| a.kind != AnnotKind::Redact) {
        let pid = *ids.get(a.page).ok_or_else(|| anyhow!("Page {} does not exist.", a.page + 1))?;
        if page_rotation(&doc, pid) != 0 {
            bail!("Page {} is rotated. Rotate it back to 0 degrees in \"Organize pages\" before annotating it.", a.page + 1);
        }
        let b = page_box(&doc, pid);
        let (pw, ph) = (b[2] - b[0], b[3] - b[1]);
        let (x, w, h) = (b[0] + a.x * pw, a.w * pw, a.h * ph);
        let y_top = b[3] - a.y * ph;
        let ov = overlays.entry(a.page).or_default();
        if ov.fonts.is_empty() {
            ov.fonts.push((Font::Helv.res_name().into(), font));
            ov.gs.push(("OmniMul".into(), multiply));
        }
        match a.kind {
            AnnotKind::Highlight => {
                ov.content.gs("OmniMul").fill_rgb(hex_rgb(if a.color.is_empty() { "#ffe600" } else { &a.color })).rect(x, y_top - h, w, h, true);
            }
            AnnotKind::Text => {
                let size = if a.size > 0.0 { a.size } else { 12.0 };
                ov.content.fill_rgb(hex_rgb(if a.color.is_empty() { "#111111" } else { &a.color }));
                for (i, line) in wrap(&a.text, Font::Helv, size, w.max(20.0)).iter().enumerate() {
                    if !line.is_empty() {
                        ov.content.text(Font::Helv, size, x, y_top - size - i as f32 * size * 1.2, 0.0, 0, line);
                    }
                }
            }
            AnnotKind::Image => {
                let Some(bytes) = &a.image else { continue };
                let img = embed_image(&mut doc, bytes)?;
                n_img += 1;
                let nm = format!("OmniImg{n_img}");
                let s = (w / img.w as f32).min(h / img.h as f32);
                let (iw, ih) = (img.w as f32 * s, img.h as f32 * s);
                let ov = overlays.get_mut(&a.page).expect("overlay");
                ov.xobj.push((nm.clone(), img.id));
                ov.content.image(&nm, x + (w - iw) / 2.0, y_top - h + (h - ih) / 2.0, iw, ih);
            }
            AnnotKind::Redact => {}
        }
    }
    for (page, ov) in overlays {
        ov.apply(&mut doc, ids[page])?;
    }
    let annotated = save(&mut doc)?;

    let reds: Vec<&Annot> = annots.iter().filter(|a| a.kind == AnnotKind::Redact).collect();
    if reds.is_empty() {
        return Ok((annotated, 0));
    }
    // Redaction = flatten the affected pages to pictures with black boxes burned in, so the text is really gone.
    let mut by_page: HashMap<usize, Vec<&Annot>> = HashMap::new();
    for r in &reds {
        by_page.entry(r.page).or_default().push(r);
    }
    let total = page_ids(&load(&annotated)?).len();
    for p in by_page.keys() {
        if *p >= total {
            bail!("Page {} does not exist.", p + 1);
        }
    }
    let rasters: HashMap<usize, (Vec<u8>, f32, f32)> = with_doc(&annotated, |pd| {
        let pages = pd.pages();
        let mut out = HashMap::new();
        let mut k = 0;
        for (&pi, list) in &by_page {
            ctx.check()?;
            ctx.progress(k, by_page.len(), &format!("Redacting page {}", pi + 1));
            k += 1;
            let page = pages.get(pi as u16).map_err(|e| anyhow!("{e}"))?;
            let (pw, ph) = (page.width().value, page.height().value);
            let mut img = render_dpi(&page, 200.0)?;
            let (iw, ih) = (img.width() as f32, img.height() as f32);
            for r in list {
                let (x0, y0) = ((r.x * iw).floor().max(0.0) as u32, (r.y * ih).floor().max(0.0) as u32);
                let (x1, y1) = (((r.x + r.w) * iw).ceil().min(iw) as u32, ((r.y + r.h) * ih).ceil().min(ih) as u32);
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        img.put_pixel(xx, yy, image::Rgba([0, 0, 0, 255]));
                    }
                }
            }
            out.insert(pi, (encode_jpeg(&img, 92)?, pw, ph));
        }
        Ok(out)
    })?;
    let src = load(&annotated)?;
    let items: Vec<Item> = (0..total)
        .map(|i| match rasters.get(&i) {
            Some((j, pw, ph)) => Item::Raster { jpeg: j.clone(), pw: *pw, ph: *ph },
            None => Item::Page { doc: 0, index: i, rot: 0 },
        })
        .collect();
    Ok((assemble(std::slice::from_ref(&src), &items, ctx)?, by_page.len()))
}

// ---------------------------------------------------------------- forms
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldType {
    Text,
    Check,
    Choice,
}
#[derive(Clone, Debug)]
pub struct FormField {
    pub name: String,
    pub kind: FieldType,
    /// Text value, "true"/"false" for check boxes, or the selected option.
    pub value: String,
    pub options: Vec<String>,
}

struct Widget {
    page: ObjectId,
    id: ObjectId,
    owner: ObjectId,
    name: String,
    kind: FieldType,
    radio: bool,
    rect: [f32; 4],
    on_state: Option<String>,
    options: Vec<String>,
    value: Option<Object>,
}

fn pdf_text(o: &Object) -> Option<String> {
    match o {
        Object::String(b, _) => {
            if b.starts_with(&[0xFE, 0xFF]) {
                let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                Some(String::from_utf16_lossy(&u))
            } else {
                Some(b.iter().map(|&c| c as char).collect())
            }
        }
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    }
}

fn collect_widgets(doc: &Document) -> Vec<Widget> {
    let mut out = Vec::new();
    for page in page_ids(doc) {
        let Ok(pd) = doc.get_dictionary(page) else { continue };
        let Some(Object::Array(arr)) = pd.get(b"Annots").ok().map(|o| resolve(doc, o).clone()) else { continue };
        for a in arr {
            let Object::Reference(wid) = a else { continue };
            let Ok(wd) = doc.get_dictionary(wid) else { continue };
            if !matches!(wd.get(b"Subtype"), Ok(Object::Name(n)) if n.as_slice() == b"Widget") {
                continue;
            }
            // walk up the field tree
            let (mut id, mut parts, mut owner, mut ft, mut ff, mut value, mut opt) = (wid, Vec::<String>::new(), None, None::<Vec<u8>>, None::<i64>, None::<Object>, None::<Vec<Object>>);
            for _ in 0..16 {
                let Ok(d) = doc.get_dictionary(id) else { break };
                if let Some(t) = d.get(b"T").ok().and_then(|o| pdf_text(resolve(doc, o))) {
                    parts.push(t);
                    owner.get_or_insert(id);
                }
                if ft.is_none() {
                    if let Ok(Object::Name(n)) = d.get(b"FT") {
                        ft = Some(n.clone());
                    }
                }
                if ff.is_none() {
                    ff = d.get(b"Ff").ok().and_then(|o| num(resolve(doc, o))).map(|v| v as i64);
                }
                if value.is_none() {
                    value = d.get(b"V").ok().map(|o| resolve(doc, o).clone());
                }
                if opt.is_none() {
                    if let Some(Object::Array(a)) = d.get(b"Opt").ok().map(|o| resolve(doc, o).clone()) {
                        opt = Some(a);
                    }
                }
                match d.get(b"Parent") {
                    Ok(Object::Reference(p)) => id = *p,
                    _ => break,
                }
            }
            parts.reverse();
            let name = parts.join(".");
            let (Some(owner), Some(ft)) = (owner, ft) else { continue };
            let ff = ff.unwrap_or(0);
            let radio = ft == b"Btn" && ff & 32768 != 0;
            let kind = match ft.as_slice() {
                b"Tx" => FieldType::Text,
                b"Btn" if ff & 65536 != 0 => continue, // push button
                b"Btn" => {
                    if radio {
                        FieldType::Choice
                    } else {
                        FieldType::Check
                    }
                }
                b"Ch" => FieldType::Choice,
                _ => continue,
            };
            let rect = match wd.get(b"Rect").ok().map(|o| resolve(doc, o)) {
                Some(Object::Array(r)) => {
                    let v: Vec<f32> = r.iter().filter_map(|o| num(resolve(doc, o))).collect();
                    if v.len() == 4 {
                        [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])]
                    } else {
                        [0.0; 4]
                    }
                }
                _ => [0.0; 4],
            };
            // on-state of a check box / radio button comes from its normal appearance dictionary
            let on_state = wd
                .get(b"AP")
                .ok()
                .and_then(|o| dict_get(doc, o, b"N"))
                .and_then(|n| match n {
                    Object::Dictionary(d) => d.iter().map(|(k, _)| String::from_utf8_lossy(k).to_string()).find(|k| k != "Off"),
                    _ => None,
                });
            let options: Vec<String> = opt
                .unwrap_or_default()
                .iter()
                .filter_map(|o| match resolve(doc, o) {
                    Object::Array(p) => p.get(1).or(p.first()).and_then(|x| pdf_text(resolve(doc, x))),
                    other => pdf_text(other),
                })
                .collect();
            out.push(Widget { page, id: wid, owner, name, kind, radio, rect, on_state, options, value });
        }
    }
    out
}
fn dict_get(doc: &Document, o: &Object, key: &[u8]) -> Option<Object> {
    match resolve(doc, o) {
        Object::Dictionary(d) => d.get(key).ok().map(|v| resolve(doc, v).clone()),
        _ => None,
    }
}

pub fn list_fields(bytes: &[u8]) -> Result<Vec<FormField>> {
    let doc = load(bytes)?;
    let ws = collect_widgets(&doc);
    let mut fields: Vec<FormField> = Vec::new();
    for w in &ws {
        let current = match (&w.kind, &w.value) {
            (FieldType::Check, Some(Object::Name(n))) => (n.as_slice() != b"Off").to_string(),
            (FieldType::Check, _) => "false".to_string(),
            (_, Some(v)) => pdf_text(v).unwrap_or_default(),
            _ => String::new(),
        };
        if let Some(f) = fields.iter_mut().find(|f| f.name == w.name) {
            if w.radio {
                if let Some(s) = &w.on_state {
                    if !f.options.contains(s) {
                        f.options.push(s.clone());
                    }
                }
            }
            continue;
        }
        let mut options = w.options.clone();
        if w.radio {
            options = w.on_state.iter().cloned().collect();
        }
        fields.push(FormField { name: w.name.clone(), kind: w.kind, value: current, options });
    }
    Ok(fields)
}

fn set_key(doc: &mut Document, id: ObjectId, key: &str, v: Object) -> Result<()> {
    doc.get_object_mut(id)?.as_dict_mut()?.set(key, v);
    Ok(())
}

/// Fill form fields. With `flatten` the values are drawn onto the page and the fields are removed.
pub fn fill(bytes: &[u8], values: &HashMap<String, String>, flatten: bool) -> Result<Vec<u8>> {
    let mut doc = load(bytes)?;
    let ws = collect_widgets(&doc);
    if ws.is_empty() {
        bail!("This PDF has no fillable form fields.");
    }
    // 1) write values
    let mut shown: HashMap<ObjectId, String> = HashMap::new(); // widget -> text to draw when flattening
    for w in &ws {
        let Some(v) = values.get(&w.name) else {
            if let Some(cur) = &w.value {
                if flatten && w.kind != FieldType::Check {
                    shown.insert(w.id, pdf_text(cur).unwrap_or_default());
                }
            }
            continue;
        };
        match w.kind {
            FieldType::Text | FieldType::Choice if !w.radio => {
                set_key(&mut doc, w.owner, "V", Object::String(winansi(v), StringFormat::Literal))?;
                shown.insert(w.id, safe_text(v));
            }
            FieldType::Check => {
                let on = matches!(v.as_str(), "true" | "1" | "yes");
                let state = w.on_state.clone().unwrap_or_else(|| "Yes".into());
                let val = if on { state.as_str() } else { "Off" };
                set_key(&mut doc, w.owner, "V", name(val))?;
                set_key(&mut doc, w.id, "AS", name(val))?;
                if on {
                    shown.insert(w.id, "X".into());
                }
            }
            FieldType::Choice => {
                // radio group
                set_key(&mut doc, w.owner, "V", name(if v.is_empty() { "Off" } else { v.as_str() }))?;
                let mine = w.on_state.clone().unwrap_or_default();
                set_key(&mut doc, w.id, "AS", name(if &mine == v { mine.as_str() } else { "Off" }))?;
                if &mine == v {
                    shown.insert(w.id, "X".into());
                }
            }
            _ => {}
        }
    }
    // 2) ask viewers to regenerate appearances
    let root = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(r)) => *r,
        _ => bail!("This PDF has no catalog."),
    };
    let acro_ref = match doc.get_dictionary(root)?.get(b"AcroForm") {
        Ok(Object::Reference(r)) => Some(*r),
        _ => None,
    };
    match acro_ref {
        Some(r) => set_key(&mut doc, r, "NeedAppearances", Object::Boolean(true))?,
        None => {
            if let Ok(Object::Dictionary(d)) = doc.get_object_mut(root)?.as_dict_mut()?.get_mut(b"AcroForm") {
                d.set("NeedAppearances", Object::Boolean(true));
            }
        }
    }
    // 3) flatten
    if flatten {
        let font = font_object(&mut doc, Font::Helv);
        let mut per_page: HashMap<ObjectId, Overlay> = HashMap::new();
        for w in &ws {
            let Some(text) = shown.get(&w.id) else { continue };
            if text.is_empty() {
                continue;
            }
            let h = (w.rect[3] - w.rect[1]).max(1.0);
            let size = (h * 0.7).clamp(6.0, 12.0);
            let ov = per_page.entry(w.page).or_default();
            if ov.fonts.is_empty() {
                ov.fonts.push((Font::Helv.res_name().into(), font));
            }
            ov.content.fill_rgb((0.05, 0.05, 0.1)).text(Font::Helv, size, w.rect[0] + 2.0, w.rect[1] + (h - size) / 2.0 + size * 0.22, 0.0, 0, text);
        }
        for (page, ov) in per_page {
            ov.apply(&mut doc, page)?;
        }
        // remove widgets and the AcroForm
        let widget_ids: Vec<ObjectId> = ws.iter().map(|w| w.id).collect();
        for pid in page_ids(&doc) {
            let arr: Option<Vec<Object>> = doc.get_dictionary(pid)?.get(b"Annots").ok().and_then(|o| match resolve(&doc, o) {
                Object::Array(a) => Some(a.clone()),
                _ => None,
            });
            if let Some(arr) = arr {
                let keep: Vec<Object> = arr.into_iter().filter(|o| !matches!(o, Object::Reference(r) if widget_ids.contains(r))).collect();
                set_key(&mut doc, pid, "Annots", Object::Array(keep))?;
            }
        }
        doc.get_object_mut(root)?.as_dict_mut()?.remove(b"AcroForm");
    }
    save(&mut doc)
}
