//! Readers that turn Word / Excel / PowerPoint / CSV / HTML / text into `Block`s for the PDF generator.
//! Everything is parsed locally. Fidelity is "good enough": text, headings, lists, tables and pictures.
use crate::gen::Block;
use anyhow::{anyhow, bail, Result};
use base64::Engine;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};

pub const IMAGE_EXT: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];
pub const SHEET_EXT: [&str; 4] = ["xlsx", "xls", "xlsm", "ods"];
pub const TEXT_EXT: [&str; 18] =
    ["txt", "md", "log", "json", "xml", "yml", "yaml", "ini", "rtf", "js", "css", "py", "rs", "c", "cpp", "java", "sql", "sh"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Pdf,
    Image,
    Docx,
    Pptx,
    Sheet,
    Csv,
    Html,
    Text,
}
pub fn kind_of(ext: &str) -> Option<Kind> {
    let e = ext.to_lowercase();
    let e = e.as_str();
    Some(match e {
        "pdf" => Kind::Pdf,
        "docx" => Kind::Docx,
        "pptx" => Kind::Pptx,
        "csv" | "tsv" => Kind::Csv,
        "html" | "htm" | "xhtml" => Kind::Html,
        _ if IMAGE_EXT.contains(&e) => Kind::Image,
        _ if SHEET_EXT.contains(&e) => Kind::Sheet,
        _ if TEXT_EXT.contains(&e) => Kind::Text,
        _ => return None,
    })
}
/// Every extension the "Create PDF" module accepts (for file dialogs).
pub fn all_extensions() -> Vec<&'static str> {
    let mut v = vec!["pdf", "docx", "pptx", "csv", "tsv", "html", "htm", "xhtml"];
    v.extend(IMAGE_EXT);
    v.extend(SHEET_EXT);
    v.extend(TEXT_EXT);
    v
}

fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ------------------------------------------------------------------ HTML
fn heading_size(tag: &str) -> Option<f32> {
    Some(match tag {
        "h1" => 20.0,
        "h2" => 17.0,
        "h3" => 15.0,
        "h4" => 13.0,
        "h5" => 12.0,
        "h6" => 11.0,
        _ => return None,
    })
}
fn data_uri_image(src: &str) -> Option<Vec<u8>> {
    let rest = src.strip_prefix("data:image")?;
    let (_, b64) = rest.split_once("base64,")?;
    base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()
}

pub fn html_to_blocks(html: &str) -> Vec<Block> {
    use scraper::{ElementRef, Html};
    let doc = Html::parse_document(html);
    let mut blocks = Vec::new();
    fn push_imgs(el: ElementRef, blocks: &mut Vec<Block>) {
        for d in el.descendants() {
            if let Some(e) = ElementRef::wrap(d) {
                if e.value().name() == "img" {
                    if let Some(b) = e.value().attr("src").and_then(data_uri_image) {
                        blocks.push(Block::Image(b));
                    }
                }
            }
        }
    }
    fn walk(el: ElementRef, blocks: &mut Vec<Block>) {
        for child in el.children() {
            if let Some(t) = child.value().as_text() {
                let t = clean(t);
                if !t.is_empty() {
                    blocks.push(Block::Para(t));
                }
                continue;
            }
            let Some(e) = ElementRef::wrap(child) else { continue };
            let tag = e.value().name().to_lowercase();
            let text = || clean(&e.text().collect::<String>());
            if matches!(tag.as_str(), "script" | "style" | "head" | "noscript" | "template") {
                continue;
            }
            if let Some(sz) = heading_size(&tag) {
                let t = text();
                if !t.is_empty() {
                    blocks.push(Block::Heading(t, sz));
                }
            } else if matches!(tag.as_str(), "p" | "blockquote" | "figcaption") {
                let t = text();
                if !t.is_empty() {
                    blocks.push(Block::Para(t));
                }
                push_imgs(e, blocks);
            } else if tag == "ul" || tag == "ol" {
                let mut n = 0;
                for li in e.children().filter_map(ElementRef::wrap) {
                    n += 1;
                    let t = clean(&li.text().collect::<String>());
                    if !t.is_empty() {
                        blocks.push(Block::Para(if tag == "ol" { format!("{n}. {t}") } else { format!("\u{2022} {t}") }));
                    }
                }
            } else if tag == "table" {
                let mut rows = Vec::new();
                for tr in e.descendants().filter_map(ElementRef::wrap).filter(|x| x.value().name() == "tr") {
                    let cells: Vec<String> = tr
                        .children()
                        .filter_map(ElementRef::wrap)
                        .filter(|c| matches!(c.value().name(), "td" | "th"))
                        .map(|c| clean(&c.text().collect::<String>()))
                        .collect();
                    rows.push(cells);
                }
                blocks.push(Block::Table(rows, None));
            } else if tag == "pre" {
                blocks.push(Block::Mono(e.text().collect::<String>().trim_end().to_string(), 9.5));
            } else if tag == "img" {
                if let Some(b) = e.value().attr("src").and_then(data_uri_image) {
                    blocks.push(Block::Image(b));
                }
            } else if tag == "hr" {
                blocks.push(Block::Para(String::new()));
            } else {
                walk(e, blocks);
            }
        }
    }
    walk(doc.root_element(), &mut blocks);
    blocks
}

// ------------------------------------------------------------------ zip + xml helpers
type Zip<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;
fn open_zip(bytes: &[u8]) -> Result<Zip<'_>> {
    zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| anyhow!("This file is not a valid Office document: {e}"))
}
fn zip_text(z: &mut Zip, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}
fn zip_bytes(z: &mut Zip, name: &str) -> Option<Vec<u8>> {
    let mut f = z.by_name(name).ok()?;
    let mut v = Vec::new();
    f.read_to_end(&mut v).ok()?;
    Some(v)
}
fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == key).map(|a| String::from_utf8_lossy(&a.value).to_string())
}
/// Relationship id -> target path (relative to the part's folder).
fn rels(xml: &str) -> HashMap<String, String> {
    let mut m = HashMap::new();
    let mut r = Reader::from_str(xml);
    loop {
        match r.read_event() {
            Ok(Event::Empty(e)) | Ok(Event::Start(e)) if e.name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(t)) = (attr(&e, b"Id"), attr(&e, b"Target")) {
                    m.insert(id, t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    m
}
fn media_path(base: &str, target: &str) -> String {
    // base like "word/" or "ppt/slides/"; target like "media/x.png" or "../media/x.png"
    let t = target.trim_start_matches('/');
    let mut parts: Vec<&str> = base.trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    for seg in t.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}
fn is_picture(path: &str) -> bool {
    let e = path.rsplit('.').next().unwrap_or("").to_lowercase();
    IMAGE_EXT.contains(&e.as_str())
}

// ------------------------------------------------------------------ DOCX
pub fn docx_blocks(bytes: &[u8]) -> Result<Vec<Block>> {
    let mut z = open_zip(bytes)?;
    let xml = zip_text(&mut z, "word/document.xml").ok_or_else(|| anyhow!("No word/document.xml - is this a .docx file?"))?;
    let rel = zip_text(&mut z, "word/_rels/document.xml.rels").map(|s| rels(&s)).unwrap_or_default();

    let mut blocks: Vec<Block> = Vec::new();
    let mut r = Reader::from_str(&xml);
    let (mut in_t, mut in_p) = (false, false);
    let mut text = String::new();
    let mut style = String::new();
    let mut list = false;
    let mut pending_imgs: Vec<Vec<u8>> = Vec::new();
    // tables
    let mut tdepth = 0;
    let mut table: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();

    loop {
        match r.read_event() {
            Ok(ev @ (Event::Start(_) | Event::Empty(_))) => {
                let start = matches!(ev, Event::Start(_));
                let e = match &ev {
                    Event::Start(e) | Event::Empty(e) => e,
                    _ => unreachable!(),
                };
                match e.name().as_ref() {
                    b"w:p" if start => {
                        in_p = true;
                        text.clear();
                        style.clear();
                        list = false;
                    }
                    b"w:pStyle" if in_p => style = attr(e, b"w:val").unwrap_or_default(),
                    b"w:numPr" if in_p => list = true,
                    b"w:t" if start => in_t = true,
                    b"w:tab" if in_p => text.push(' '),
                    b"w:br" if in_p => text.push(' '),
                    b"w:tbl" if start => {
                        tdepth += 1;
                        if tdepth == 1 {
                            table.clear();
                        }
                    }
                    b"w:tr" if start && tdepth == 1 => row.clear(),
                    b"w:tc" if start && tdepth == 1 => cell.clear(),
                    b"a:blip" => {
                        if let Some(id) = attr(e, b"r:embed") {
                            if let Some(t) = rel.get(&id) {
                                let p = media_path("word/", t);
                                if is_picture(&p) {
                                    if let Some(b) = zip_bytes(&mut z, &p) {
                                        pending_imgs.push(b);
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_t => {
                if let Ok(s) = t.unescape() {
                    text.push_str(&s);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_t = false,
                b"w:p" => {
                    in_p = false;
                    let t = clean(&text);
                    if tdepth > 0 {
                        if !cell.is_empty() && !t.is_empty() {
                            cell.push(' ');
                        }
                        cell.push_str(&t);
                    } else {
                        if !t.is_empty() {
                            let lower = style.to_lowercase();
                            if lower == "title" {
                                blocks.push(Block::Heading(t, 22.0));
                            } else if let Some(n) = lower.strip_prefix("heading").and_then(|n| n.trim().parse::<usize>().ok()) {
                                blocks.push(Block::Heading(t, [20.0, 17.0, 15.0, 13.0, 12.0, 11.0][n.clamp(1, 6) - 1]));
                            } else if list || lower.contains("list") {
                                blocks.push(Block::Para(format!("\u{2022} {t}")));
                            } else {
                                blocks.push(Block::Para(t));
                            }
                        }
                        for b in pending_imgs.drain(..) {
                            blocks.push(Block::Image(b));
                        }
                    }
                }
                b"w:tc" if tdepth == 1 => row.push(std::mem::take(&mut cell)),
                b"w:tr" if tdepth == 1 => table.push(std::mem::take(&mut row)),
                b"w:tbl" => {
                    tdepth -= 1;
                    if tdepth == 0 {
                        blocks.push(Block::Table(std::mem::take(&mut table), None));
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => bail!("The Word file could not be read: {e}"),
            _ => {}
        }
    }
    if blocks.is_empty() {
        blocks.push(Block::Para("(empty document)".into()));
    }
    Ok(blocks)
}

// ------------------------------------------------------------------ PPTX
pub fn pptx_blocks(bytes: &[u8]) -> Result<Vec<Block>> {
    let mut z = open_zip(bytes)?;
    let mut slides: Vec<(usize, String)> = z
        .file_names()
        .filter_map(|n| {
            let rest = n.strip_prefix("ppt/slides/slide")?.strip_suffix(".xml")?;
            Some((rest.parse::<usize>().ok()?, n.to_string()))
        })
        .collect();
    slides.sort();
    if slides.is_empty() {
        bail!("No slides found - is this a .pptx file?");
    }
    let total = slides.len();
    let mut blocks = Vec::new();
    for (i, (num, path)) in slides.iter().enumerate() {
        let xml = zip_text(&mut z, path).unwrap_or_default();
        if i > 0 {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Mono(format!("Slide {} of {}", i + 1, total), 8.0));
        let (mut paras, mut cur, mut in_t) = (Vec::<String>::new(), String::new(), false);
        let mut r = Reader::from_str(&xml);
        loop {
            match r.read_event() {
                Ok(Event::Start(e)) if e.name().as_ref() == b"a:t" => in_t = true,
                Ok(Event::Text(t)) if in_t => {
                    if let Ok(s) = t.unescape() {
                        cur.push_str(&s)
                    }
                }
                Ok(Event::End(e)) => match e.name().as_ref() {
                    b"a:t" => in_t = false,
                    b"a:p" => {
                        let t = cur.trim().to_string();
                        if !t.is_empty() {
                            paras.push(t);
                        }
                        cur.clear();
                    }
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        for (k, p) in paras.into_iter().enumerate() {
            blocks.push(if k == 0 { Block::Heading(p, 18.0) } else { Block::Para(format!("\u{2022} {p}")) });
        }
        let rel_xml = zip_text(&mut z, &format!("ppt/slides/_rels/slide{num}.xml.rels")).unwrap_or_default();
        let mut targets: Vec<String> = rels(&rel_xml).into_values().filter(|t| t.contains("media/")).collect();
        targets.sort();
        for t in targets {
            let p = media_path("ppt/slides/", &t);
            if is_picture(&p) {
                if let Some(b) = zip_bytes(&mut z, &p) {
                    blocks.push(Block::Image(b));
                }
            }
        }
    }
    Ok(blocks)
}

// ------------------------------------------------------------------ spreadsheets & CSV
pub fn sheet_blocks(bytes: &[u8]) -> Result<Vec<Block>> {
    use calamine::{open_workbook_auto_from_rs, Reader as _};
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec())).map_err(|e| anyhow!("Could not open the spreadsheet: {e}"))?;
    let names = wb.sheet_names().to_vec();
    let mut blocks = Vec::new();
    for (i, nm) in names.iter().enumerate() {
        let Ok(range) = wb.worksheet_range(nm) else { continue };
        let rows: Vec<Vec<String>> = range.rows().map(|r| r.iter().map(|c| c.to_string()).collect()).filter(|r: &Vec<String>| r.iter().any(|c| !c.is_empty())).collect();
        if rows.is_empty() {
            continue;
        }
        if i > 0 && !blocks.is_empty() {
            blocks.push(Block::PageBreak);
        }
        blocks.push(Block::Table(rows, if names.len() > 1 { Some(nm.clone()) } else { None }));
    }
    if blocks.is_empty() {
        blocks.push(Block::Para("(empty workbook)".into()));
    }
    Ok(blocks)
}

pub fn parse_csv(text: &str, delim: Option<char>) -> Vec<Vec<String>> {
    let text = text.trim_start_matches('\u{feff}');
    let delim = delim.unwrap_or_else(|| {
        let first = text.lines().next().unwrap_or("");
        [',', ';', '\t', '|'].into_iter().max_by_key(|d| first.matches(*d).count()).unwrap_or(',')
    });
    let chars: Vec<char> = text.chars().collect();
    let (mut rows, mut row, mut cell, mut q) = (Vec::<Vec<String>>::new(), Vec::<String>::new(), String::new(), false);
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if q {
            if ch == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    cell.push('"');
                    i += 1;
                } else {
                    q = false;
                }
            } else {
                cell.push(ch);
            }
        } else if ch == '"' {
            q = true;
        } else if ch == delim {
            row.push(std::mem::take(&mut cell));
        } else if ch == '\n' || ch == '\r' {
            if ch == '\r' && chars.get(i + 1) == Some(&'\n') {
                i += 1;
            }
            row.push(std::mem::take(&mut cell));
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            cell.push(ch);
        }
        i += 1;
    }
    row.push(cell);
    if row.iter().any(|c| !c.is_empty()) {
        rows.push(row);
    }
    rows
}

pub fn file_to_blocks(file_name: &str, kind: Kind, bytes: &[u8]) -> Result<Vec<Block>> {
    Ok(match kind {
        Kind::Docx => docx_blocks(bytes)?,
        Kind::Pptx => pptx_blocks(bytes)?,
        Kind::Sheet => sheet_blocks(bytes)?,
        Kind::Csv => {
            let t = String::from_utf8_lossy(bytes).to_string();
            let d = if file_name.to_lowercase().ends_with(".tsv") { Some('\t') } else { None };
            vec![Block::Table(parse_csv(&t, d), None)]
        }
        Kind::Html => html_to_blocks(&String::from_utf8_lossy(bytes)),
        Kind::Text => vec![Block::Mono(String::from_utf8_lossy(bytes).to_string(), 9.5)],
        _ => bail!("Unsupported file type: {file_name}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quotes_and_delims() {
        let r = parse_csv("a;b;c\n1;\"x;y\";3\n", None);
        assert_eq!(r, vec![vec!["a", "b", "c"], vec!["1", "x;y", "3"]]);
    }
    #[test]
    fn html_blocks() {
        let b = html_to_blocks("<h1>Hi</h1><p>one  two</p><ul><li>a</li><li>b</li></ul>");
        assert!(matches!(&b[0], Block::Heading(t, _) if t == "Hi"));
        assert!(matches!(&b[1], Block::Para(t) if t == "one two"));
        assert_eq!(b.len(), 4);
    }
}
