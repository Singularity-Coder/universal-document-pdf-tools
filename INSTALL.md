# Install guide - Universal Document PDF Tools

## A. Install with the installer
1. Double-click `UniversalDocumentPDFTools-Setup-1.0.0.exe`.
2. If you see "Windows protected your PC": click **More info**, then **Run anyway**.
3. Pick install mode (just me / all users) > **Next** > tick *Create a desktop shortcut* > **Install** > **Finish**.
4. Open the app from the desktop icon or Start Menu.
5. Uninstall: Settings > Apps > Universal Document PDF Tools > Uninstall.

## B. Portable
1. Right-click `UniversalDocumentPDFTools-Portable.zip` > **Extract All...**
2. Open the extracted folder and run `universal-document-pdf-tools.exe`.
3. Do not move the `.exe` away from its `runtime` folder.

## C. Check it works
Open any PDF. If thumbnails/preview show, PDFium is working. Run OCR on a scanned page to confirm the OCR models are found.
If a message says PDFium or the OCR models are missing, see README > Troubleshooting.

## D. Build it yourself (developers)
1. Install Rust (https://rustup.rs) + Visual Studio C++ Build Tools. Optional: Inno Setup 6.
2. `cd` into the project folder.
3. `powershell -ExecutionPolicy Bypass -File .\scripts\fetch-assets.ps1`
4. `cargo build --release`
5. `powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1` -> results in `dist\`.
