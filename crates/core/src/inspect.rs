//! Page report, compare, blank-page removal, rasterise (compress / grey / B&W / invert), image extraction.
use crate::pdfops::*;
use crate::render::*;
use crate::util::*;
use anyhow::{anyhow, bail, Result};

pub fn page_report(bytes: &[u8], file: &str) -> Result<Vec<Output>> {
    let d = load(bytes)?;
    let ids = page_ids(&d);
    let lines = all_lines(bytes, &Ctx::silent()).unwrap_or_default();
    let mut rows = vec![
        format!("File: {file}"),
        format!("Pages: {}", ids.len()),
        format!("Encrypted: {}", if Document_is_encrypted(bytes) { "yes" } else { "no" }),
        String::new(),
        "Page | Size (pt)       | Rot | Text chars | Annots".to_string(),
    ];
    let mut scans = 0;
    for (i, &pid) in ids.iter().enumerate() {
        let (w, h) = page_size(&d, pid);
        let chars: usize = lines.get(i).map(|p| p.iter().map(|l| l.text.chars().count()).sum()).unwrap_or(0);
        if chars == 0 {
            scans += 1;
        }
        rows.push(format!("{:>4} | {:<15} | {:>3} | {:>10} | {}", i + 1, format!("{:.0} x {:.0}", w, h), page_rotation(&d, pid), chars, annotation_count(&d, pid)));
    }
    if scans > 0 {
        rows.push(String::new());
        rows.push(format!("{scans} page(s) have no text layer - they are probably scans (use OCR)."));
    }
    Ok(vec![Output::text(format!("{}-report.txt", base_name(file)), rows.join("\n"))])
}
#[allow(non_snake_case)]
fn Document_is_encrypted(bytes: &[u8]) -> bool {
    lopdf::Document::load_mem(bytes).map(|d| d.is_encrypted()).unwrap_or(false)
}

/// Line diff (LCS) limited to 1500 lines per page.
pub fn diff_lines(a: &[String], b: &[String]) -> Vec<String> {
    let a = &a[..a.len().min(1500)];
    let b = &b[..b.len().min(1500)];
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push(format!("- {}", a[i]));
            i += 1;
        } else {
            out.push(format!("+ {}", b[j]));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|x| format!("- {x}")));
    out.extend(b[j..].iter().map(|x| format!("+ {x}")));
    out
}

pub fn compare(a: &[u8], name_a: &str, b: &[u8], name_b: &str, ctx: &Ctx) -> Result<Vec<Output>> {
    let pa = all_lines(a, &Ctx::silent())?;
    let pb = all_lines(b, &Ctx::silent())?;
    let max = pa.len().max(pb.len());
    let mut rep = vec![format!("A: {name_a} ({} pages)", pa.len()), format!("B: {name_b} ({} pages)", pb.len()), String::new()];
    let mut changed = 0;
    for i in 0..max {
        ctx.check()?;
        ctx.progress(i, max, &format!("Comparing page {}", i + 1));
        match (pa.get(i), pb.get(i)) {
            (Some(x), Some(y)) => {
                let xs: Vec<String> = x.iter().map(|l| l.text.clone()).collect();
                let ys: Vec<String> = y.iter().map(|l| l.text.clone()).collect();
                let d = diff_lines(&xs, &ys);
                if !d.is_empty() {
                    changed += 1;
                    rep.push(format!("Page {}: {} changed line(s)", i + 1, d.len()));
                    rep.extend(d.iter().take(60).map(|x| format!("   {x}")));
                    if d.len() > 60 {
                        rep.push(format!("   ... {} more", d.len() - 60));
                    }
                }
            }
            (x, _) => {
                changed += 1;
                rep.push(format!("Page {}: only in {}", i + 1, if x.is_some() { "A" } else { "B" }));
            }
        }
    }
    rep.insert(3, if changed > 0 { format!("{changed} of {max} page(s) differ.") } else { "The text content of both files is identical.".into() });
    rep.insert(4, String::new());
    Ok(vec![Output::text("comparison.txt", rep.join("\n"))])
}

pub fn extract_images(bytes: &[u8], file: &str, min_kb: usize) -> Result<Vec<Output>> {
    let (jpegs, other) = extract_jpegs(bytes, min_kb)?;
    if jpegs.is_empty() {
        bail!("No embedded JPEG images found{}", if other > 0 { format!(" ({other} image(s) use other encodings - use Convert > PDF to images instead).") } else { ".".to_string() });
    }
    let mut out: Vec<Output> = jpegs.into_iter().enumerate().map(|(i, d)| Output::new(format!("{}-image{:03}.jpg", base_name(file), i + 1), d)).collect();
    if other > 0 {
        out[0].note = format!("{other} image(s) with other encodings were skipped");
    }
    Ok(out)
}

pub fn remove_blank_pages(bytes: &[u8], file: &str, threshold_pct: f32, ctx: &Ctx) -> Result<Vec<Output>> {
    let (blank, total) = with_doc(bytes, |doc| {
        let pages = doc.pages();
        let total = pages.len() as usize;
        let mut blank = Vec::new();
        for i in 0..total {
            ctx.check()?;
            ctx.progress(i, total, &format!("Checking page {}", i + 1));
            let page = pages.get(i as u16).map_err(|e| anyhow!("{e}"))?;
            let img = render_page(&page, (page.width().value * 0.5) as i32)?;
            let ink = img.pixels().filter(|p| (p[0] as u32 + p[1] as u32 + p[2] as u32) < 3 * 225).count();
            if (ink as f32 / (img.width() * img.height()) as f32) < threshold_pct / 100.0 {
                blank.push(i);
            }
        }
        Ok((blank, total))
    })?;
    if blank.is_empty() {
        return Ok(vec![Output::new(format!("{}.pdf", base_name(file)), bytes.to_vec()).note("No blank pages found - file unchanged")]);
    }
    if blank.len() == total {
        bail!("Every page looks blank.");
    }
    let keep: Vec<usize> = (0..total).filter(|i| !blank.contains(i)).collect();
    let list = blank.iter().map(|i| (i + 1).to_string()).collect::<Vec<_>>().join(", ");
    Ok(vec![Output::new(format!("{}-no-blanks.pdf", base_name(file)), pick_pages(bytes, &keep)?).note(format!("Removed page(s): {list}"))])
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Color,
    Gray,
    Bw,
    Invert,
}

/// Re-render pages as pictures: shrinks scans, or turns pages grey / black-and-white / inverted. Text becomes non-selectable.
pub fn rasterize(bytes: &[u8], file: &str, tone: Tone, dpi: f32, quality: u8, pages_spec: &str, ctx: &Ctx) -> Result<Vec<Output>> {
    let (rasters, total) = with_doc(bytes, |doc| {
        let pages = doc.pages();
        let total = pages.len() as usize;
        let sel = parse_pages(pages_spec, total)?;
        let mut out: Vec<Option<(Vec<u8>, f32, f32)>> = vec![None; total];
        for (k, &i) in sel.iter().enumerate() {
            ctx.check()?;
            ctx.progress(k, sel.len(), &format!("Page {}", i + 1));
            let page = pages.get(i as u16).map_err(|e| anyhow!("{e}"))?;
            let (pw, ph) = (page.width().value, page.height().value);
            let mut img = render_dpi(&page, dpi)?;
            if tone != Tone::Color {
                for p in img.pixels_mut() {
                    let y = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                    let v = match tone {
                        Tone::Bw => {
                            if y > 150.0 {
                                255.0
                            } else {
                                0.0
                            }
                        }
                        Tone::Invert => 255.0 - y,
                        _ => y,
                    } as u8;
                    p[0] = v;
                    p[1] = v;
                    p[2] = v;
                }
            }
            out[i] = Some((encode_jpeg(&img, quality)?, pw, ph));
        }
        Ok((out, total))
    })?;
    let src = load(bytes)?;
    let items: Vec<Item> = (0..total)
        .map(|i| match &rasters[i] {
            Some((j, pw, ph)) => Item::Raster { jpeg: j.clone(), pw: *pw, ph: *ph },
            None => Item::Page { doc: 0, index: i, rot: 0 },
        })
        .collect();
    let data = assemble(std::slice::from_ref(&src), &items, ctx)?;
    let suffix = match tone {
        Tone::Color => "compressed",
        Tone::Gray => "gray",
        Tone::Bw => "bw",
        Tone::Invert => "inverted",
    };
    let note = format!("{} KB -> {} KB", bytes.len() / 1024, data.len() / 1024);
    Ok(vec![Output::new(format!("{}-{suffix}.pdf", base_name(file)), data).note(note)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diff() {
        let a: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let b: Vec<String> = ["a", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(diff_lines(&a, &b), vec!["- b".to_string(), "+ d".to_string()]);
    }
}
