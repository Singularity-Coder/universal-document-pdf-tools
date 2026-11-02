# Universal Document PDF Tools

Free, **fully offline** all-in-one document tool written in Rust (egui). No accounts, no profile, no telemetry, no network access:
create PDFs from almost anything, convert, organize pages, OCR, sign, annotate, redact, fill forms and clean files.
Windows, Linux and macOS.

## What changed from the web version

| Request | Result |
|---|---|
| Remove log in section and profile | Login page, accounts, guest mode, profile picture, plan badge, sign-out and the Usage page are gone. Nothing identifies a user. |
| Rebuild everything in Rust | Cargo workspace: `crates/core` (all document logic, no UI) + `crates/app` (egui desktop UI). No JavaScript, no Node, no server. |
| Make it wider | Opens at 1560 x 940 (minimum 1200 x 720). Every tool uses a wide two-column layout: files + options on the left, results/preview on the right. |
| Icons | Vector icon set for every module, button and status, a painted app logo, a window/taskbar icon (`assets/icon.png`) and an embedded Windows `.exe` icon (`assets/icon.ico`). |
| Loading screen + animation | Animated splash: pulsing logo, orbiting arcs, progress bar with sheen, status text, fade-out. It runs a real check of the offline engines while it plays. In-app work shows a spinner + progress bar. "Reduce motion" is in Settings. |
| Consistent design | One theme engine (`theme.rs`) and one widget kit (`kit.rs`): the same buttons, cards, chips, spacing, radii and colours on every screen. Six presets + custom theme + accent override are kept. |
| Offline | No network code anywhere. Heavy engines (PDFium, OCR models) are local files in `runtime/`. |

## Modules (same seven as before)

Create PDF - Convert - Organize pages (visual organizer, merge, split, extract/delete, rotate, reverse, blank pages, crop) -
Finish & optimize (watermark, page numbers, header/footer, stamp, compress, grey/B&W/invert) - OCR - Sign & annotate
(highlight, text, drawn/uploaded signature, true redaction, form filling + flatten) - Inspect & clean (properties, page report,
remove hidden data, remove annotations, blank pages, extract images, compare).

## Install and run (for users)

Pick ONE option. You do not need Rust or an internet connection to use the app.

### Option A - Installer (recommended)
1. Get `UniversalDocumentPDFTools-Setup-1.0.0.exe`.
2. Double-click it. If Windows SmartScreen says "Windows protected your PC", click **More info** then **Run anyway** (the app is not code-signed yet).
3. Choose "Install for me only" (no admin needed) or "for all users", click **Next**, tick **Create a desktop shortcut**, click **Install**, then **Finish**.
4. Start it from the desktop shortcut or Start Menu. Uninstall via Settings > Apps.

### Option B - Portable (no install)
1. Get `UniversalDocumentPDFTools-Portable.zip`.
2. Right-click > **Extract All...** to any folder (a USB stick works).
3. Open the folder and run `universal-document-pdf-tools.exe`.
4. Keep the `runtime` folder **next to the .exe**. It contains PDFium and the OCR models; without it only the basic tools work (see below).

## Build from source (for developers, Windows)

Requirements: Windows 10/11 x64, internet for the first build, and:
1. **Rust** from https://rustup.rs (choose the default; it will ask for the *Visual Studio C++ Build Tools* - install the "Desktop development with C++" workload).
2. **Inno Setup 6** (only for the installer): https://jrsoftware.org/isdl.php

Steps (Command Prompt, in the project folder):

```bat
:: 1. Offline engines (internet needed once) -> creates runtime\pdfium.dll and runtime\models\*.rten
powershell -ExecutionPolicy Bypass -File .\scripts\fetch-assets.ps1

:: 2. Build + run
cargo build --release
target\release\universal-document-pdf-tools.exe

:: 3. Make the portable zip and installer (runs the build for you) -> dist\
powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
```

Linux / macOS: `./scripts/fetch-assets.sh` then `cargo build --release`.
The first build takes about 5 minutes. Rust warnings are normal and do not stop the build.

### Where the app looks for `runtime/`
`./`, `./runtime`, the executable's folder and its `runtime` subfolder (and `../Resources/runtime` inside a macOS `.app`).

### Without `runtime/`
The app still starts. Create PDF, merge/split/rotate/crop/reverse/blank pages, watermark/numbers/headers/stamp,
light compress, properties, sanitize, annotations, image extraction, PDF to text/Word/Excel (lopdf text extraction)
and form filling work. Anything that must *draw* a page (thumbnails, page preview, PDF to images/PowerPoint, OCR, strong compress,
grey/B&W, blank-page detection, redaction) needs PDFium, and OCR also needs the models; the UI tells you exactly what is missing.

Run the unit tests with `cargo test -p udpt-core`.

## Feature flags

`udpt-core` has a default feature `ocr` (ocrs + rten). To build without OCR, set `default-features = false` on the `udpt-core` dependency in `crates/app/Cargo.toml`; the OCR tools then report that OCR is not compiled in.

## Known limits

- Office to PDF is "good enough": text, headings, lists, tables and pictures; no fonts, colours, charts or exact layout.
- PDF to Word/Excel/PowerPoint is text/picture based. **Change from the web version:** in PowerPoint the page text is stored as the picture's alt text (not speaker notes).
- Text written into PDFs uses Helvetica/Courier (Latin-1 plus common punctuation); other characters become "?".
- Merging / organizing / splitting rebuilds the file: interactive form fields and internal links are not carried over (rotate, crop, watermark etc. edit in place and keep them).
- Overlays (watermark, numbers, stamp...) assume pages with no /Rotate; annotating rotated pages is refused with a clear message.
- Strong compress, grey/B&W and redaction turn pages into pictures. Redacted text is truly removed.
- Image extraction exports JPEG-encoded images only. Signatures are visual only (no certificate signing). Encrypted PDFs open only if they have no user password.
- OCR models are Latin-script (English and most Western European languages). Weak on handwriting and rotated pages.

## Status

- The Windows release build compiles (verified on Windows 11, Rust release profile).
- Fixed since the first draft: `icons.rs` now passes `svg_of(name).as_bytes()` to `egui::Image::from_bytes`; `Stroke::new` literals are `f32`; `fetch-assets` uses a checksum-verified GitHub mirror for the OCR models (the original S3 bucket now returns 403).
- Still to confirm on your machine: OCR accuracy on real scans and every tool end to end. Report any bug with a screenshot.

## Troubleshooting

| Symptom | Fix |
|---|---|
| `E0277 Bytes: From<&str>` in `icons.rs` | Use the fixed source: `svg_of(name).as_bytes()`. |
| `fetch-assets.ps1` "Access is denied" | Run it as `powershell -ExecutionPolicy Bypass -File .\scripts\fetch-assets.ps1`. |
| OCR model download `403 Forbidden` | The old S3 address is dead. Use the updated script, or download the two files from the links in `runtime\README.txt` into `runtime\models\`. |
| App says PDFium / OCR models not found | `runtime\` must be next to the `.exe` and hold `pdfium.dll` and `models\*.rten`. |
| SmartScreen warning | Click **More info > Run anyway**. Signing the exe needs a paid certificate. |
| `cargo` not found | Install Rust (rustup.rs), then open a new Command Prompt. |
| Linker / `link.exe` error | Install Visual Studio C++ Build Tools ("Desktop development with C++"). |

Known-good pins (do not loosen without checking):
- `zip = "=2.2.3"` - calamine 0.26 needs the zip 2.2 API (zip 2.3+ gives E0107 inside calamine).
- `rten = "0.21"` - must match the rten used by `ocrs 0.10.x`; check with `cargo tree -p ocrs | findstr rten`.
- `eframe`/`egui` 0.29 (upgrading to 0.30+ needs the `Rounding`/`Margin`/`Frame` renames).
