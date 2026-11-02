This folder holds the offline engines. It must sit NEXT TO the program (.exe).

  pdfium.dll / libpdfium.so / libpdfium.dylib        PDF renderer
  models\text-detection.rten, models\text-recognition.rten   OCR (Latin script)

Missing? Run scripts\fetch-assets.ps1 (Windows) or scripts/fetch-assets.sh (Linux/macOS) once, with internet.
Manual download of the two OCR models (save into models\):
  https://github.com/deeplabua/deepocr/releases/download/models-v1/text-detection.rten
  https://github.com/deeplabua/deepocr/releases/download/models-v1/text-recognition.rten
PDFium: https://github.com/bblanchon/pdfium-binaries/releases (pdfium-win-x64.tgz -> bin\pdfium.dll)
