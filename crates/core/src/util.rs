//! Shared helpers: progress/cancel context, results, page-range parsing.
use anyhow::{anyhow, bail, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Progress + cancellation handle passed to every long-running operation.
#[derive(Clone)]
pub struct Ctx {
    cancel: Arc<AtomicBool>,
    cb: Arc<dyn Fn(f32, &str) + Send + Sync>,
}

impl Ctx {
    pub fn new(cancel: Arc<AtomicBool>, cb: Arc<dyn Fn(f32, &str) + Send + Sync>) -> Self {
        Self { cancel, cb }
    }
    pub fn silent() -> Self {
        Self { cancel: Arc::new(AtomicBool::new(false)), cb: Arc::new(|_, _| {}) }
    }
    pub fn progress(&self, done: usize, total: usize, label: &str) {
        let f = if total == 0 { 0.0 } else { done as f32 / total as f32 };
        (self.cb)(f.clamp(0.0, 1.0), label);
    }
    pub fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            bail!("Cancelled");
        }
        Ok(())
    }
}

/// One produced file.
#[derive(Clone, Debug)]
pub struct Output {
    pub name: String,
    pub data: Vec<u8>,
    pub note: String,
    /// Text shown inside the app (reports, extracted text).
    pub preview: Option<String>,
}

impl Output {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self { name: name.into(), data, note: String::new(), preview: None }
    }
    pub fn note(mut self, n: impl Into<String>) -> Self {
        self.note = n.into();
        self
    }
    pub fn text(name: impl Into<String>, text: String) -> Self {
        let preview: String = text.chars().take(6000).collect();
        Self { name: name.into(), data: text.into_bytes(), note: String::new(), preview: Some(preview) }
    }
}

/// An input file picked by the user.
#[derive(Clone, Debug)]
pub struct Input {
    pub path: PathBuf,
    pub name: String,
}

impl Input {
    pub fn from_path(path: PathBuf) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        Self { path, name }
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        std::fs::read(&self.path).map_err(|e| anyhow!("Could not read \"{}\": {e}", self.name))
    }
}

/// Option values from the form (everything is kept as text, like HTML inputs).
#[derive(Clone, Default, Debug)]
pub struct Values(pub HashMap<String, String>);

impl Values {
    pub fn set(&mut self, k: &str, v: impl Into<String>) {
        self.0.insert(k.to_string(), v.into());
    }
    pub fn s(&self, k: &str) -> &str {
        self.0.get(k).map(|s| s.as_str()).unwrap_or("")
    }
    pub fn f(&self, k: &str) -> f32 {
        self.s(k).trim().parse().unwrap_or(0.0)
    }
    pub fn i(&self, k: &str) -> i64 {
        self.s(k).trim().parse::<f64>().map(|v| v as i64).unwrap_or(0)
    }
    pub fn b(&self, k: &str) -> bool {
        matches!(self.s(k), "true" | "1" | "yes")
    }
}

pub fn base_name(n: &str) -> String {
    match n.rfind('.') {
        Some(i) if i > 0 => n[..i].to_string(),
        _ => n.to_string(),
    }
}
pub fn ext_of(n: &str) -> String {
    match n.rfind('.') {
        Some(i) => n[i + 1..].to_lowercase(),
        None => String::new(),
    }
}
pub fn fmt_size(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1_048_576 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    }
}
pub fn unique_sorted(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v.dedup();
    v
}

/// "#rrggbb" -> (r, g, b) in 0..1
pub fn hex_rgb(h: &str) -> (f32, f32, f32) {
    let s = h.trim().trim_start_matches('#');
    let n = u32::from_str_radix(s, 16).unwrap_or(0);
    (((n >> 16) & 255) as f32 / 255.0, ((n >> 8) & 255) as f32 / 255.0, (n & 255) as f32 / 255.0)
}

/// Page selection: "1-3, 5, 8-" -> 0-based indexes in the order written.
/// Accepts "all", "last", open ranges ("8-", "-3") and reverse ranges ("5-2").
pub fn parse_pages(spec: &str, max: usize) -> Result<Vec<usize>> {
    let s = spec.trim().to_lowercase();
    if s.is_empty() || s == "all" {
        return Ok((0..max).collect());
    }
    let num = |t: &str| -> Result<usize> {
        if t == "last" {
            return Ok(max);
        }
        let n: usize = t.parse().map_err(|_| anyhow!("\"{t}\" is not a page number."))?;
        if n < 1 || n > max {
            bail!("Page {n} is outside this document (1-{max}).");
        }
        Ok(n)
    };
    let mut out = Vec::new();
    for part in s.split(|c: char| c == ',' || c.is_whitespace()).filter(|p| !p.is_empty()) {
        if let Some((a, b)) = part.split_once('-') {
            let from = if a.is_empty() { 1 } else { num(a)? };
            let to = if b.is_empty() { max } else { num(b)? };
            if from <= to {
                out.extend((from..=to).map(|p| p - 1));
            } else {
                out.extend((to..=from).rev().map(|p| p - 1));
            }
        } else {
            out.push(num(part)? - 1);
        }
    }
    if out.is_empty() {
        bail!("No pages selected.");
    }
    Ok(out)
}

/// Groups for splitting: "1-3; 4-6; 7-"
pub fn parse_groups(spec: &str, max: usize) -> Result<Vec<Vec<usize>>> {
    let groups: Vec<&str> = spec.split(';').map(|g| g.trim()).filter(|g| !g.is_empty()).collect();
    if groups.is_empty() {
        bail!("Enter at least one page range, e.g. 1-3; 4-6");
    }
    groups.into_iter().map(|g| parse_pages(g, max)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges() {
        assert_eq!(parse_pages("1-3, 5, 8-", 9).unwrap(), vec![0, 1, 2, 4, 7, 8]);
        assert_eq!(parse_pages("5-2", 9).unwrap(), vec![4, 3, 2, 1]);
        assert_eq!(parse_pages("last", 4).unwrap(), vec![3]);
        assert!(parse_pages("12", 4).is_err());
        assert_eq!(parse_pages("all", 3).unwrap(), vec![0, 1, 2]);
    }
}
