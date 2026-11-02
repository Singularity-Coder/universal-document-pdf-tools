//! PDF -> text / Word / Excel / PowerPoint / images, and anything -> PDF ("Create PDF").
use crate::gen::{blocks_to_pdf, image_to_pdf};
use crate::office::{file_to_blocks, kind_of, Kind};
use crate::pdfops;
use crate::render::*;
use crate::util::*;
use anyhow::{anyhow, bail, Result};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

// ---------------------------------------------------------------- create PDF
fn one_file_to_pdf(inp: &Input, fit_a4: bool) -> Result<Vec<u8>> {
    let kind = kind_of(&ext_of(&inp.name)).ok_or_else(|| anyhow!("\"{}\" is not a supported file type.", inp.name))?;
    let bytes = inp.bytes()?;
    match kind {
        Kind::Pdf => Ok(bytes),
        Kind::Image => image_to_pdf(&bytes, fit_a4),
        _ => blocks_to_pdf(&file_to_blocks(&inp.name, kind, &bytes)?),
    }
}

pub fn create_pdfs(files: &[Input], separate: bool, fit_a4: bool, ctx: &Ctx) -> Result<Vec<Output>> {
    if files.is_empty() {
        bail!("Add at least one file.");
    }
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, f) in files.iter().enumerate() {
        ctx.check()?;
        ctx.progress(i, files.len(), &f.name);
        parts.push((f.name.clone(), one_file_to_pdf(f, fit_a4)?));
    }
    ctx.progress(files.len(), files.len(), "Finishing");
    if separate {
        return parts
            .into_iter()
            .map(|(n, b)| {
                let pages = pdfops::page_count(&b)?;
                Ok(Output::new(format!("{}.pdf", base_name(&n)), b).note(format!("{pages} page(s)")))
            })
            .collect();
    }
    if parts.len() == 1 {
        let (n, b) = parts.remove(0);
        let pages = pdfops::page_count(&b)?;
        return Ok(vec![Output::new(format!("{}.pdf", base_name(&n)), b).note(format!("{pages} page(s)"))]);
    }
    let list: Vec<Vec<u8>> = parts.into_iter().map(|p| p.1).collect();
    let merged = pdfops::merge(&list, ctx)?;
    let pages = pdfops::page_count(&merged)?;
    Ok(vec![Output::new("combined.pdf", merged).note(format!("{pages} page(s) from {} file(s)", files.len()))])
}

// ---------------------------------------------------------------- zip writing helper
fn zip_write(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in entries {
        z.start_file(name.as_str(), opts.clone())?;
        z.write_all(data)?;
    }
    Ok(z.finish()?.into_inner())
}
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn xml_clean(s: &str) -> String {
    s.chars().filter(|c| matches!(*c, '\t' | '\n' | '\r') || (*c as u32) >= 0x20).collect()
}

// ---------------------------------------------------------------- PDF -> text
pub fn pdf_to_text(bytes: &[u8], file: &str, markers: bool, ctx: &Ctx) -> Result<Vec<Output>> {
    let pages = all_lines(bytes, ctx)?;
    require_text(&pages)?;
    let txt = pages
        .iter()
        .enumerate()
        .map(|(i, p)| format!("{}{}", if markers { format!("===== Page {} =====\n", i + 1) } else { String::new() }, p.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n")))
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(vec![Output::text(format!("{}.txt", base_name(file)), txt).note(format!("{} page(s)", pages.len()))])
}

// ---------------------------------------------------------------- PDF -> Word
pub fn pdf_to_docx(bytes: &[u8], file: &str, page_breaks: bool, ctx: &Ctx) -> Result<Vec<Output>> {
    let pages = all_lines(bytes, ctx)?;
    require_text(&pages)?;
    let body = body_height(&pages);
    let mut xml = String::new();
    let para = |style: Option<&str>, text: &str, page_break_before: bool| -> String {
        let mut ppr = String::new();
        if let Some(s) = style {
            ppr.push_str(&format!("<w:pStyle w:val=\"{s}\"/>"));
        }
        if page_break_before {
            ppr.push_str("<w:pageBreakBefore/>");
        }
        format!("<w:p><w:pPr>{ppr}</w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", esc(&xml_clean(text)))
    };
    for (pi, lines) in pages.iter().enumerate() {
        let mut cur: Option<String> = None;
        let mut prev: Option<(f32, f32)> = None; // (y, h)
        let mut first = true;
        let flush = |cur: &mut Option<String>, xml: &mut String, brk: &mut bool| {
            if let Some(t) = cur.take() {
                xml.push_str(&para(None, &t, *brk));
                *brk = false;
            }
        };
        let mut pending_break = page_breaks && pi > 0;
        for l in lines {
            let is_head = l.h > body * 1.35 && l.text.chars().count() < 140;
            if is_head {
                flush(&mut cur, &mut xml, &mut pending_break);
                let style = if l.h > body * 1.8 { "Heading1" } else { "Heading2" };
                xml.push_str(&para(Some(style), &l.text, pending_break));
                pending_break = false;
                prev = None;
                first = false;
                continue;
            }
            let joins = cur.is_some() && prev.map(|(py, _)| py - l.y < l.h * 1.7).unwrap_or(false);
            if joins {
                let c = cur.as_mut().expect("cur");
                if !c.ends_with('-') {
                    c.push(' ');
                }
                c.push_str(&l.text);
            } else {
                flush(&mut cur, &mut xml, &mut pending_break);
                cur = Some(l.text.clone());
            }
            prev = Some((l.y, l.h));
            first = false;
        }
        let _ = first;
        flush(&mut cur, &mut xml, &mut pending_break);
        if pending_break {
            xml.push_str(&para(None, "", true));
        }
    }
    let ns = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
    let document = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document {ns}><w:body>{xml}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>");
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles {ns}><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults>\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"120\"/><w:outlineLvl w:val=\"0\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"36\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:basedOn w:val=\"Normal\"/><w:next w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"200\" w:after=\"100\"/><w:outlineLvl w:val=\"1\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr></w:style></w:styles>"
    );
    let entries = vec![
        ("[Content_Types].xml".to_string(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".to_string(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
        ("word/_rels/document.xml.rels".to_string(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec()),
        ("word/document.xml".to_string(), document.into_bytes()),
        ("word/styles.xml".to_string(), styles.into_bytes()),
    ];
    Ok(vec![Output::new(format!("{}.docx", base_name(file)), zip_write(&entries)?).note("Text and headings only - images and exact layout are not converted")])
}

// ---------------------------------------------------------------- PDF -> Excel
pub fn pdf_to_xlsx(bytes: &[u8], file: &str, single_sheet: bool, ctx: &Ctx) -> Result<Vec<Output>> {
    use rust_xlsxwriter::Workbook;
    let pages = all_lines(bytes, ctx)?;
    require_text(&pages)?;
    let mut wb = Workbook::new();
    let mut write_rows = |name: &str, rows: &[&Vec<String>]| -> Result<()> {
        let ws = wb.add_worksheet();
        ws.set_name(name).map_err(|e| anyhow!("{e}"))?;
        for (r, row) in rows.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let v = xml_clean(cell);
                ws.write_string(r as u32, c as u16, &v).map_err(|e| anyhow!("{e}"))?;
            }
        }
        Ok(())
    };
    if single_sheet {
        let rows: Vec<&Vec<String>> = pages.iter().flatten().map(|l| &l.cells).collect();
        write_rows("PDF", &rows)?;
    } else {
        for (i, p) in pages.iter().enumerate() {
            let rows: Vec<&Vec<String>> = p.iter().map(|l| &l.cells).collect();
            write_rows(&format!("Page {}", i + 1), &rows)?;
        }
    }
    let data = wb.save_to_buffer().map_err(|e| anyhow!("{e}"))?;
    Ok(vec![Output::new(format!("{}.xlsx", base_name(file)), data).note("Columns are detected from gaps between text")])
}

// ---------------------------------------------------------------- PDF -> images
pub fn pdf_to_images(bytes: &[u8], file: &str, jpeg: bool, dpi: f32, pages_spec: &str, ctx: &Ctx) -> Result<Vec<Output>> {
    with_doc(bytes, |doc| {
        let pages = doc.pages();
        let total = pages.len() as usize;
        let idx = parse_pages(pages_spec, total)?;
        let mut out = Vec::new();
        for (k, &i) in idx.iter().enumerate() {
            ctx.check()?;
            ctx.progress(k, idx.len(), &format!("Rendering page {}", i + 1));
            let page = pages.get(i as u16).map_err(|e| anyhow!("{e}"))?;
            let img = render_dpi(&page, dpi)?;
            let data = if jpeg { encode_jpeg(&img, 92)? } else { encode_png(&img)? };
            out.push(Output::new(format!("{}-page{:03}.{}", base_name(file), i + 1, if jpeg { "jpg" } else { "png" }), data).note(format!("{} x {} px", img.width(), img.height())));
        }
        ctx.progress(idx.len(), idx.len(), "");
        Ok(out)
    })
}

// ---------------------------------------------------------------- PDF -> PowerPoint
const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Omni"><a:themeElements><a:clrScheme name="Omni"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F2937"/></a:dk2><a:lt2><a:srgbClr val="F3F4F6"/></a:lt2><a:accent1><a:srgbClr val="4F5BE8"/></a:accent1><a:accent2><a:srgbClr val="8B5CF6"/></a:accent2><a:accent3><a:srgbClr val="10B981"/></a:accent3><a:accent4><a:srgbClr val="F59E0B"/></a:accent4><a:accent5><a:srgbClr val="EF4444"/></a:accent5><a:accent6><a:srgbClr val="06B6D4"/></a:accent6><a:hlink><a:srgbClr val="4F5BE8"/></a:hlink><a:folHlink><a:srgbClr val="8B5CF6"/></a:folHlink></a:clrScheme><a:fontScheme name="Omni"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Omni"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#;

/// Every page becomes a full-slide picture. The page text is stored as the picture's alt text.
pub fn pdf_to_pptx(bytes: &[u8], file: &str, dpi: f32, ctx: &Ctx) -> Result<Vec<Output>> {
    let text_pages = all_lines(bytes, &Ctx::silent()).unwrap_or_default();
    let (slides, w_emu, h_emu) = with_doc(bytes, |doc| {
        let pages = doc.pages();
        let total = pages.len() as usize;
        let mut out: Vec<Vec<u8>> = Vec::new();
        let (mut w, mut h) = (0.0f32, 0.0f32);
        for i in 0..total {
            ctx.check()?;
            ctx.progress(i, total, &format!("Slide {}", i + 1));
            let page = pages.get(i as u16).map_err(|e| anyhow!("{e}"))?;
            if i == 0 {
                w = page.width().value;
                h = page.height().value;
            }
            out.push(encode_jpeg(&render_dpi(&page, dpi)?, 88)?);
        }
        Ok((out, (w * 12700.0) as i64, (h * 12700.0) as i64))
    })?;
    let n = slides.len();
    let ns_a = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"";
    let ns_r = "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
    let ns_p = "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
    let hdr = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
    let rel_ns = "http://schemas.openxmlformats.org/package/2006/relationships";
    let rel_t = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    let mut ct = format!("{hdr}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>");
    let mut pres_rels = format!("{hdr}<Relationships xmlns=\"{rel_ns}\"><Relationship Id=\"rId1\" Type=\"{rel_t}/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>");
    let mut sld_ids = String::new();
    for i in 1..=n {
        ct.push_str(&format!("<Override PartName=\"/ppt/slides/slide{i}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
        pres_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"{rel_t}/slide\" Target=\"slides/slide{i}.xml\"/>", i + 1));
        sld_ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 255 + i, i + 1));
    }
    ct.push_str("</Types>");
    pres_rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"{rel_t}/theme\" Target=\"theme/theme1.xml\"/></Relationships>", n + 2));

    let presentation = format!("{hdr}<p:presentation {ns_a} {ns_r} {ns_p}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{sld_ids}</p:sldIdLst><p:sldSz cx=\"{w_emu}\" cy=\"{h_emu}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>");
    let empty_tree = "<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld>";
    let master = format!("{hdr}<p:sldMaster {ns_a} {ns_r} {ns_p}>{empty_tree}<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst></p:sldMaster>");
    let master_rels = format!("{hdr}<Relationships xmlns=\"{rel_ns}\"><Relationship Id=\"rId1\" Type=\"{rel_t}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"{rel_t}/theme\" Target=\"../theme/theme1.xml\"/></Relationships>");
    let layout = format!("{hdr}<p:sldLayout {ns_a} {ns_r} {ns_p} type=\"blank\" preserve=\"1\">{empty_tree}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>");
    let layout_rels = format!("{hdr}<Relationships xmlns=\"{rel_ns}\"><Relationship Id=\"rId1\" Type=\"{rel_t}/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>");

    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), ct.into_bytes()),
        ("_rels/.rels".into(), format!("{hdr}<Relationships xmlns=\"{rel_ns}\"><Relationship Id=\"rId1\" Type=\"{rel_t}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>").into_bytes()),
        ("ppt/presentation.xml".into(), presentation.into_bytes()),
        ("ppt/_rels/presentation.xml.rels".into(), pres_rels.into_bytes()),
        ("ppt/slideMasters/slideMaster1.xml".into(), master.into_bytes()),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels".into(), master_rels.into_bytes()),
        ("ppt/slideLayouts/slideLayout1.xml".into(), layout.into_bytes()),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(), layout_rels.into_bytes()),
        ("ppt/theme/theme1.xml".into(), THEME.as_bytes().to_vec()),
    ];
    for (i, jpg) in slides.into_iter().enumerate() {
        let k = i + 1;
        let alt: String = text_pages.get(i).map(|p| p.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join(" ")).unwrap_or_default().chars().take(1500).collect();
        let slide = format!("{hdr}<p:sld {ns_a} {ns_r} {ns_p}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr><p:pic><p:nvPicPr><p:cNvPr id=\"2\" name=\"Page {k}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{w_emu}\" cy=\"{h_emu}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>", esc(&xml_clean(&alt)));
        let srels = format!("{hdr}<Relationships xmlns=\"{rel_ns}\"><Relationship Id=\"rId1\" Type=\"{rel_t}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rId2\" Type=\"{rel_t}/image\" Target=\"../media/image{k}.jpeg\"/></Relationships>");
        entries.push((format!("ppt/slides/slide{k}.xml"), slide.into_bytes()));
        entries.push((format!("ppt/slides/_rels/slide{k}.xml.rels"), srels.into_bytes()));
        entries.push((format!("ppt/media/image{k}.jpeg"), jpg));
    }
    Ok(vec![Output::new(format!("{}.pptx", base_name(file)), zip_write(&entries)?).note("Each page is a picture slide; the page text is stored as the picture's alt text")])
}
