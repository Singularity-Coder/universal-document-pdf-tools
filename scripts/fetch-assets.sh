#!/usr/bin/env bash
# One-time download of the offline engines into ./runtime (needs internet ONCE; the app itself never uses the network).
#   PDFium (PDF renderer)  - bblanchon/pdfium-binaries
#   OCR models (.rten)     - GitHub mirror of the ocrs models (detection + recognition, Latin script)
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p runtime/models

os="$(uname -s)"; arch="$(uname -m)"
case "$os-$arch" in
  Linux-x86_64)  pkg=pdfium-linux-x64;   lib=lib/libpdfium.so ;;
  Linux-aarch64) pkg=pdfium-linux-arm64; lib=lib/libpdfium.so ;;
  Darwin-x86_64) pkg=pdfium-mac-x64;     lib=lib/libpdfium.dylib ;;
  Darwin-arm64)  pkg=pdfium-mac-arm64;   lib=lib/libpdfium.dylib ;;
  *) echo "Unsupported platform $os-$arch - download PDFium manually from https://github.com/bblanchon/pdfium-binaries"; exit 1 ;;
esac

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
echo "Downloading PDFium ($pkg)..."
curl -fL "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/$pkg.tgz" -o "$tmp/pdfium.tgz"
tar -xzf "$tmp/pdfium.tgz" -C "$tmp"
cp "$tmp/$lib" runtime/

echo "Downloading OCR models (GitHub mirror of the ocrs models, checksum verified)..."
BASE="https://github.com/deeplabua/deepocr/releases/download/models-v1"
DET_SHA="f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca"
REC_SHA="e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e"
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
fetch() { # name expected-sha
  curl -fL "$BASE/$1" -o "runtime/models/$1"
  [ "$(sha "runtime/models/$1")" = "$2" ] || { echo "Checksum mismatch for $1"; rm -f "runtime/models/$1"; exit 1; }
}
fetch text-detection.rten "$DET_SHA"
fetch text-recognition.rten "$REC_SHA"
echo "Done. runtime/ now holds everything Universal Document PDF Tools needs offline."
