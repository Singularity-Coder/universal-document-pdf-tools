//! Universal Document PDF Tools core: everything the app does with documents, with no UI and no network access.
pub mod actions;
pub mod annotate;
pub mod convert;
pub mod gen;
pub mod inspect;
pub mod ocr;
pub mod office;
pub mod pdfops;
pub mod render;
pub mod util;

pub use util::{base_name, ext_of, fmt_size, Ctx, Input, Output, Values};

use anyhow::Result;

/// Visual organizer export: `plan` is (source page index, extra rotation in degrees) in output order.
pub fn organize_export(bytes: &[u8], plan: &[(usize, i32)], ctx: &Ctx) -> Result<Vec<u8>> {
    let doc = pdfops::load(bytes)?;
    let items: Vec<pdfops::Item> = plan.iter().map(|&(index, rot)| pdfops::Item::Page { doc: 0, index, rot }).collect();
    pdfops::assemble(std::slice::from_ref(&doc), &items, ctx)
}

/// What this machine can do right now (shown on the splash screen and in Settings).
#[derive(Clone, Debug)]
pub struct Capabilities {
    pub renderer: std::result::Result<(), String>,
    pub ocr: std::result::Result<(), String>,
}
pub fn capabilities() -> Capabilities {
    Capabilities { renderer: render::pdfium_status(), ocr: ocr::status() }
}
