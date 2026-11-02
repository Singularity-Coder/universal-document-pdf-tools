//! Declarative description of every module/action (the UI renders forms from this) and the dispatcher.
use crate::inspect::Tone;
use crate::util::*;
use crate::{convert, inspect, ocr, office, pdfops};
use anyhow::{anyhow, bail, Result};

#[derive(Clone, Debug)]
pub enum FieldKind {
    Text,
    Number { min: f64, max: f64 },
    Select(Vec<(&'static str, &'static str)>),
    Check,
    Color,
    Range { min: f64, max: f64, step: f64 },
}
#[derive(Clone, Debug)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub default: &'static str,
    pub help: &'static str,
    /// Only show when another field has this value.
    pub when: Option<(&'static str, &'static str)>,
}
impl Field {
    fn new(key: &'static str, label: &'static str, kind: FieldKind, default: &'static str) -> Self {
        Self { key, label, kind, default, help: "", when: None }
    }
    pub fn help(mut self, h: &'static str) -> Self {
        self.help = h;
        self
    }
    pub fn when(mut self, key: &'static str, value: &'static str) -> Self {
        self.when = Some((key, value));
        self
    }
}
fn text(k: &'static str, l: &'static str, d: &'static str) -> Field {
    Field::new(k, l, FieldKind::Text, d)
}
fn number(k: &'static str, l: &'static str, d: &'static str, min: f64, max: f64) -> Field {
    Field::new(k, l, FieldKind::Number { min, max }, d)
}
fn select(k: &'static str, l: &'static str, d: &'static str, o: &[(&'static str, &'static str)]) -> Field {
    Field::new(k, l, FieldKind::Select(o.to_vec()), d)
}
fn check(k: &'static str, l: &'static str, d: bool) -> Field {
    Field::new(k, l, FieldKind::Check, if d { "true" } else { "false" })
}
fn color(k: &'static str, l: &'static str, d: &'static str) -> Field {
    Field::new(k, l, FieldKind::Color, d)
}
fn range(k: &'static str, l: &'static str, d: &'static str, min: f64, max: f64, step: f64) -> Field {
    Field::new(k, l, FieldKind::Range { min, max, step }, d)
}
fn pages() -> Field {
    text("pages", "Pages", "all").help("all, or e.g. 1-3, 5, 8-")
}
fn dpi(def: &'static str) -> Field {
    select("dpi", "Resolution", def, &[("72", "72 dpi"), ("96", "96 dpi"), ("110", "110 dpi"), ("150", "150 dpi"), ("200", "200 dpi"), ("300", "300 dpi")])
}
const POS: [(&str, &str); 6] = [
    ("bottom-center", "Bottom center"),
    ("bottom-right", "Bottom right"),
    ("bottom-left", "Bottom left"),
    ("top-center", "Top center"),
    ("top-right", "Top right"),
    ("top-left", "Top left"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    /// Any number of supported files.
    Any,
    /// Two or more PDFs.
    Pdfs,
    /// One PDF.
    Pdf,
    /// Exactly two PDFs.
    Pdf2,
}
/// Actions with their own screen instead of the generic form.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Special {
    None,
    Organizer,
    Properties,
    Sign,
}
#[derive(Clone, Debug)]
pub struct ActionDef {
    pub id: &'static str,
    pub label: &'static str,
    pub desc: &'static str,
    pub cta: &'static str,
    pub input: InputKind,
    pub fields: Vec<Field>,
    pub special: Special,
    pub needs_render: bool,
    pub needs_ocr: bool,
}
impl ActionDef {
    fn new(id: &'static str, label: &'static str, desc: &'static str, cta: &'static str, input: InputKind) -> Self {
        Self { id, label, desc, cta, input, fields: vec![], special: Special::None, needs_render: false, needs_ocr: false }
    }
    fn f(mut self, f: Vec<Field>) -> Self {
        self.fields = f;
        self
    }
    fn render(mut self) -> Self {
        self.needs_render = true;
        self
    }
    fn ocr(mut self) -> Self {
        self.needs_ocr = true;
        self.needs_render = true;
        self
    }
    fn special(mut self, s: Special) -> Self {
        self.special = s;
        self
    }
}
#[derive(Clone, Debug)]
pub struct ModuleDef {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub tagline: &'static str,
    pub actions: Vec<ActionDef>,
}

pub fn modules() -> Vec<ModuleDef> {
    use InputKind::*;
    let a = ActionDef::new;
    vec![
        ModuleDef {
            id: "create",
            label: "Create PDF",
            icon: "create",
            tagline: "Turn images, text, HTML, Word, Excel and PowerPoint files into PDF.",
            actions: vec![a("make", "Create PDF", "Add images, text, CSV, HTML, Word, Excel, PowerPoint or PDF files. Use the arrows to set the order.", "Create PDF", Any).f(vec![
                select("output", "Output", "one", &[("one", "Combine everything into one PDF"), ("separate", "One PDF per file")]),
                select("imageFit", "Images", "a4", &[("a4", "Fit on A4 pages"), ("native", "Page size = image size")]),
            ])],
        },
        ModuleDef {
            id: "convert",
            label: "Convert",
            icon: "convert",
            tagline: "PDF to text, Word, Excel, PowerPoint and images.",
            actions: vec![
                a("text", "PDF to text", "Extracts the text layer (scanned PDFs need OCR first).", "Convert", Pdf).f(vec![check("markers", "Add \"Page N\" separators", true)]),
                a("docx", "PDF to Word", "Text and headings go to an editable .docx. Images and exact layout are not carried over.", "Convert", Pdf).f(vec![check("pageBreaks", "Keep page breaks", true)]),
                a("xlsx", "PDF to Excel", "Detects columns from gaps between text and writes one sheet per page (or a single sheet).", "Convert", Pdf).f(vec![select("layout", "Sheets", "pages", &[("pages", "One sheet per page"), ("single", "Everything on one sheet")])]),
                a("pptx", "PDF to PowerPoint", "Every page becomes a slide picture; the page text is kept as the picture's alt text.", "Convert", Pdf).f(vec![dpi("150")]).render(),
                a("images", "PDF to images", "Renders pages as PNG or JPEG pictures.", "Convert", Pdf).f(vec![select("format", "Format", "png", &[("png", "PNG"), ("jpeg", "JPEG")]), dpi("150"), pages()]).render(),
            ],
        },
        ModuleDef {
            id: "organize",
            label: "Organize pages",
            icon: "organize",
            tagline: "Merge, split, reorder, delete, rotate, crop and extract pages.",
            actions: vec![
                a("visual", "Visual organizer", "See every page as a thumbnail: reorder, rotate, duplicate or delete, then export.", "Export", Pdf).special(Special::Organizer).render(),
                a("merge", "Merge", "Combine several PDFs into one, in the order listed.", "Merge PDFs", Pdfs),
                a("split", "Split", "Cut a PDF into several files.", "Split PDF", Pdf).f(vec![
                    select("mode", "Split by", "ranges", &[("ranges", "Page ranges"), ("every", "Every N pages")]),
                    text("ranges", "Ranges (one file per group)", "1-3; 4-6; 7-").help("Separate groups with a semicolon").when("mode", "ranges"),
                    number("n", "Pages per file", "1", 1.0, 1000.0).when("mode", "every"),
                ]),
                a("pages", "Extract / delete", "Keep only some pages (in the order you type them) or remove pages.", "Apply", Pdf).f(vec![
                    select("mode", "Action", "keep", &[("keep", "Keep these pages"), ("delete", "Delete these pages")]),
                    text("pages", "Pages", "1").help("e.g. 3, 1, 2 reorders; 2-5 selects a range"),
                ]),
                a("rotate", "Rotate", "Turn pages clockwise or counter-clockwise.", "Rotate", Pdf).f(vec![pages(), select("angle", "Rotate", "90", &[("90", "90\u{b0} clockwise"), ("180", "180\u{b0}"), ("-90", "90\u{b0} counter-clockwise")])]),
                a("reverse", "Reverse", "Flip the page order (last page becomes first).", "Reverse", Pdf),
                a("blank", "Insert blank pages", "Add empty pages the same size as their neighbour.", "Insert", Pdf).f(vec![number("after", "After page (0 = at the start)", "1", 0.0, 100000.0), number("count", "How many", "1", 1.0, 50.0)]),
                a("crop", "Crop", "Trim margins (in points, 72 pt = 1 inch). The content stays in the file but is hidden.", "Crop", Pdf).f(vec![
                    pages(),
                    number("top", "Top margin (pt)", "0", 0.0, 2000.0),
                    number("bottom", "Bottom margin (pt)", "0", 0.0, 2000.0),
                    number("left", "Left margin (pt)", "0", 0.0, 2000.0),
                    number("right", "Right margin (pt)", "0", 0.0, 2000.0),
                ]),
            ],
        },
        ModuleDef {
            id: "finish",
            label: "Finish & optimize",
            icon: "finish",
            tagline: "Watermarks, page numbers, headers, stamps, compression.",
            actions: vec![
                a("watermark", "Watermark", "Write diagonal text across pages.", "Add watermark", Pdf).f(vec![
                    text("text", "Text", "CONFIDENTIAL"),
                    number("size", "Font size", "64", 8.0, 400.0),
                    color("color", "Colour", "#8a8a8a"),
                    range("opacity", "Opacity", "0.25", 0.05, 1.0, 0.05),
                    range("angle", "Angle", "45", 0.0, 90.0, 5.0),
                    pages(),
                ]),
                a("numbers", "Page numbers", "Use {n} for the page number and {total} for the page count.", "Add numbers", Pdf).f(vec![
                    text("format", "Format", "Page {n} of {total}"),
                    select("position", "Position", "bottom-center", &POS),
                    number("start", "First number", "1", -1000.0, 100000.0),
                    number("size", "Size", "10", 6.0, 72.0),
                    pages(),
                ]),
                a("headfoot", "Header & footer", "Tokens: {n} page, {total} count, {date} today, {file} file name.", "Apply", Pdf).f(vec![
                    text("headerLeft", "Header left", "{file}"),
                    text("headerCenter", "Header center", ""),
                    text("headerRight", "Header right", "{date}"),
                    text("footerLeft", "Footer left", ""),
                    text("footerCenter", "Footer center", "Page {n} of {total}"),
                    text("footerRight", "Footer right", ""),
                    number("size", "Size", "9", 6.0, 72.0),
                ]),
                a("stamp", "Stamp", "A bordered label such as APPROVED or DRAFT.", "Stamp", Pdf).f(vec![
                    text("text", "Text", "APPROVED"),
                    number("size", "Size", "22", 8.0, 200.0),
                    color("color", "Colour", "#c1272d"),
                    select("position", "Position", "top-right", &[POS[0], POS[1], POS[2], POS[3], POS[4], POS[5], ("middle-center", "Center")]),
                    text("pages", "Pages", "all").help("all, or e.g. 1"),
                ]),
                a("compress", "Compress", "Light keeps text selectable. Strong re-renders pages as JPEG pictures - best for scans and photo-heavy files.", "Compress", Pdf).f(vec![
                    select("level", "Level", "light", &[("light", "Light (keeps text)"), ("strong", "Strong (pages become pictures)")]),
                    dpi("110").when("level", "strong"),
                    range("quality", "JPEG quality", "70", 30.0, 95.0, 1.0).when("level", "strong"),
                ]),
                a("tone", "Grey / B&W / invert", "Re-renders pages in grayscale, pure black-and-white (clean scans) or inverted colours. Text becomes non-selectable.", "Convert", Pdf)
                    .f(vec![select("mode", "Mode", "gray", &[("gray", "Grayscale"), ("bw", "Black and white"), ("invert", "Inverted grayscale")]), dpi("150"), range("quality", "JPEG quality", "80", 30.0, 95.0, 1.0), pages()])
                    .render(),
            ],
        },
        ModuleDef {
            id: "ocr",
            label: "OCR",
            icon: "ocr",
            tagline: "Make scanned PDFs searchable or pull the text out - fully offline.",
            actions: {
                let common = || {
                    vec![
                        pages(),
                        select("dpi", "Scan quality", "200", &[("150", "Fast (150 dpi)"), ("200", "Balanced (200 dpi)"), ("300", "Best (300 dpi)")]),
                        check("skipText", "Skip pages that already contain text", true),
                    ]
                };
                vec![
                    a("searchable", "Make searchable", "Adds an invisible text layer so scanned pages can be searched, selected and copied. The pages look exactly the same.", "Run OCR", Pdf).f(common()).ocr(),
                    a("text", "Extract text", "Recognizes the text on each page and saves it as a .txt file.", "Run OCR", Pdf).f(common()).ocr(),
                ]
            },
        },
        ModuleDef {
            id: "sign",
            label: "Sign & annotate",
            icon: "sign",
            tagline: "Highlight, redact, sign, add text and fill forms.",
            actions: vec![a("sign", "Sign & annotate", "Place highlights, text, signatures and redactions on the page, or fill in form fields.", "Save", Pdf).special(Special::Sign).render()],
        },
        ModuleDef {
            id: "inspect",
            label: "Inspect & clean",
            icon: "inspect",
            tagline: "Properties, hidden data, blank pages, compare and image export.",
            actions: vec![
                a("props", "Properties", "Read and edit title, author, subject and keywords.", "Save", Pdf).special(Special::Properties),
                a("report", "Page report", "Lists page sizes, rotation, amount of text and annotations - and flags probable scans.", "Build report", Pdf),
                a("sanitize", "Remove hidden data", "Strips metadata, scripts and attachments before you share a file.", "Clean", Pdf).f(vec![
                    check("metadata", "Metadata (title, author, XMP...)", true),
                    check("scripts", "JavaScript and open-actions", true),
                    check("attachments", "Embedded files", true),
                ]),
                a("annots", "Remove annotations", "Deletes comments, highlights, stamps and sticky notes. Form fields are kept.", "Remove", Pdf).f(vec![check("keepLinks", "Keep clickable links", true)]),
                a("blankpages", "Remove blank pages", "Finds pages with almost no ink (e.g. from double-sided scanning) and drops them.", "Find & remove", Pdf)
                    .f(vec![range("threshold", "Sensitivity (% of page that may be inked)", "0.1", 0.02, 2.0, 0.02)])
                    .render(),
                a("images", "Extract images", "Saves the JPEG pictures embedded in the PDF at their original quality.", "Extract", Pdf).f(vec![number("minKb", "Ignore images smaller than (KB)", "4", 0.0, 100000.0)]),
                a("compare", "Compare", "Shows which text lines were added or removed between two versions.", "Compare", Pdf2),
            ],
        },
    ]
}

pub fn default_values(a: &ActionDef) -> Values {
    let mut v = Values::default();
    for f in &a.fields {
        v.set(f.key, f.default);
    }
    v
}

fn first(inputs: &[Input]) -> Result<(Vec<u8>, String)> {
    let f = inputs.first().ok_or_else(|| anyhow!("Add a PDF first."))?;
    Ok((inputs[0].bytes()?, f.name.clone()))
}

/// Run a generic (form-driven) action.
pub fn run(module: &str, action: &str, inputs: &[Input], v: &Values, ctx: &Ctx) -> Result<Vec<Output>> {
    use pdfops as p;
    let one = |name: String, data: Vec<u8>, note: String| vec![Output::new(name, data).note(note)];
    match (module, action) {
        ("create", "make") => convert::create_pdfs(inputs, v.s("output") == "separate", v.s("imageFit") != "native", ctx),
        ("convert", "text") => {
            let (b, n) = first(inputs)?;
            convert::pdf_to_text(&b, &n, v.b("markers"), ctx)
        }
        ("convert", "docx") => {
            let (b, n) = first(inputs)?;
            convert::pdf_to_docx(&b, &n, v.b("pageBreaks"), ctx)
        }
        ("convert", "xlsx") => {
            let (b, n) = first(inputs)?;
            convert::pdf_to_xlsx(&b, &n, v.s("layout") == "single", ctx)
        }
        ("convert", "pptx") => {
            let (b, n) = first(inputs)?;
            convert::pdf_to_pptx(&b, &n, v.f("dpi"), ctx)
        }
        ("convert", "images") => {
            let (b, n) = first(inputs)?;
            convert::pdf_to_images(&b, &n, v.s("format") == "jpeg", v.f("dpi"), v.s("pages"), ctx)
        }
        ("organize", "merge") => {
            if inputs.len() < 2 {
                bail!("Add at least two PDFs to merge.");
            }
            let list: Vec<Vec<u8>> = inputs.iter().map(|i| i.bytes()).collect::<Result<_>>()?;
            let out = p::merge(&list, ctx)?;
            Ok(one("merged.pdf".into(), out, format!("{} files", inputs.len())))
        }
        ("organize", "split") => {
            let (b, n) = first(inputs)?;
            if v.s("mode") == "every" {
                p::split_every(&b, &n, v.i("n").max(1) as usize)
            } else {
                p::split_groups(&b, v.s("ranges"), &n)
            }
        }
        ("organize", "pages") => {
            let (b, n) = first(inputs)?;
            let total = p::page_count(&b)?;
            let mut idx = parse_pages(v.s("pages"), total)?;
            if v.s("mode") == "delete" {
                let drop: std::collections::HashSet<usize> = idx.iter().copied().collect();
                idx = (0..total).filter(|i| !drop.contains(i)).collect();
                if idx.is_empty() {
                    bail!("That would delete every page.");
                }
            }
            Ok(one(format!("{}-pages.pdf", base_name(&n)), p::pick_pages(&b, &idx)?, format!("{} page(s)", idx.len())))
        }
        ("organize", "rotate") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-rotated.pdf", base_name(&n)), p::rotate(&b, v.s("pages"), v.i("angle") as i32)?, String::new()))
        }
        ("organize", "reverse") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-reversed.pdf", base_name(&n)), p::reverse(&b)?, String::new()))
        }
        ("organize", "blank") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-blank.pdf", base_name(&n)), p::insert_blank(&b, v.i("after").max(0) as usize, v.i("count").max(1) as usize)?, String::new()))
        }
        ("organize", "crop") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-cropped.pdf", base_name(&n)), p::crop(&b, v.s("pages"), v.f("top"), v.f("bottom"), v.f("left"), v.f("right"))?, String::new()))
        }
        ("finish", "watermark") => {
            let (b, n) = first(inputs)?;
            let o = p::WatermarkOpts { text: v.s("text"), size: v.f("size"), color: v.s("color"), opacity: v.f("opacity"), angle: v.f("angle"), pages: v.s("pages") };
            Ok(one(format!("{}-watermarked.pdf", base_name(&n)), p::watermark(&b, &o)?, String::new()))
        }
        ("finish", "numbers") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-numbered.pdf", base_name(&n)), p::page_numbers(&b, v.s("format"), v.s("position"), v.i("start"), v.f("size"), v.s("pages"))?, String::new()))
        }
        ("finish", "headfoot") => {
            let (b, n) = first(inputs)?;
            let slots = [
                ("header", "left", v.s("headerLeft").to_string()),
                ("header", "center", v.s("headerCenter").to_string()),
                ("header", "right", v.s("headerRight").to_string()),
                ("footer", "left", v.s("footerLeft").to_string()),
                ("footer", "center", v.s("footerCenter").to_string()),
                ("footer", "right", v.s("footerRight").to_string()),
            ];
            Ok(one(format!("{}-headers.pdf", base_name(&n)), p::header_footer(&b, &slots, v.f("size"), &n)?, String::new()))
        }
        ("finish", "stamp") => {
            let (b, n) = first(inputs)?;
            Ok(one(format!("{}-stamped.pdf", base_name(&n)), p::stamp(&b, v.s("text"), v.f("size"), v.s("color"), v.s("position"), v.s("pages"))?, String::new()))
        }
        ("finish", "compress") => {
            let (b, n) = first(inputs)?;
            if v.s("level") == "light" {
                let out = p::optimize(&b)?;
                let note = format!("{} KB -> {} KB", b.len() / 1024, out.len() / 1024);
                Ok(one(format!("{}-compressed.pdf", base_name(&n)), out, note))
            } else {
                inspect::rasterize(&b, &n, Tone::Color, v.f("dpi"), v.f("quality") as u8, "all", ctx)
            }
        }
        ("finish", "tone") => {
            let (b, n) = first(inputs)?;
            let tone = match v.s("mode") {
                "bw" => Tone::Bw,
                "invert" => Tone::Invert,
                _ => Tone::Gray,
            };
            inspect::rasterize(&b, &n, tone, v.f("dpi"), v.f("quality") as u8, v.s("pages"), ctx)
        }
        ("ocr", mode @ ("searchable" | "text")) => {
            let (b, n) = first(inputs)?;
            let o = ocr::OcrOpts { mode: if mode == "text" { ocr::Mode::Text } else { ocr::Mode::Searchable }, pages: v.s("pages"), dpi: v.f("dpi"), skip_text: v.b("skipText") };
            ocr::run(&b, &n, &o, ctx)
        }
        ("inspect", "report") => {
            let (b, n) = first(inputs)?;
            inspect::page_report(&b, &n)
        }
        ("inspect", "sanitize") => {
            let (b, n) = first(inputs)?;
            let (out, found) = p::sanitize(&b, &p::SanitizeOpts { metadata: v.b("metadata"), scripts: v.b("scripts"), attachments: v.b("attachments") })?;
            Ok(one(format!("{}-clean.pdf", base_name(&n)), out, format!("Removed: {}", if found.is_empty() { "nothing found".to_string() } else { found.join(", ") })))
        }
        ("inspect", "annots") => {
            let (b, n) = first(inputs)?;
            let (out, removed) = p::remove_annotations(&b, v.b("keepLinks"))?;
            Ok(one(format!("{}-no-annotations.pdf", base_name(&n)), out, format!("{removed} annotation(s) removed")))
        }
        ("inspect", "blankpages") => {
            let (b, n) = first(inputs)?;
            inspect::remove_blank_pages(&b, &n, v.f("threshold"), ctx)
        }
        ("inspect", "images") => {
            let (b, n) = first(inputs)?;
            inspect::extract_images(&b, &n, v.i("minKb").max(0) as usize)
        }
        ("inspect", "compare") => {
            if inputs.len() != 2 {
                bail!("Add exactly two PDFs to compare.");
            }
            inspect::compare(&inputs[0].bytes()?, &inputs[0].name, &inputs[1].bytes()?, &inputs[1].name, ctx)
        }
        _ => bail!("Unknown action: {module}/{action}"),
    }
}

/// File extensions the file dialog should offer for this input kind.
pub fn dialog_extensions(kind: InputKind) -> Vec<&'static str> {
    match kind {
        InputKind::Any => office::all_extensions(),
        _ => vec!["pdf"],
    }
}
