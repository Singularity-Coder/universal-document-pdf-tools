# Builds the release exe and packages it as:
#   dist\UniversalDocumentPDFTools-Portable.zip        (portable: unzip anywhere and run)
#   dist\UniversalDocumentPDFTools-Setup-1.0.0.exe     (installer, only if Inno Setup is installed)
# Run from the project folder:  powershell -ExecutionPolicy Bypass -File .\scripts\package-windows.ps1
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$exeName = "universal-document-pdf-tools.exe"

# 1. Make sure runtime assets exist
foreach ($f in @("runtime\pdfium.dll","runtime\models\text-detection.rten","runtime\models\text-recognition.rten")) {
    if (-not (Test-Path $f)) { throw "Missing $f - run scripts\fetch-assets.ps1 first." }
}

# 2. Build
Write-Host "Building release..."
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

# 3. Assemble portable folder
$dist = "dist"; $pf = "$dist\portable"
if (Test-Path $pf) { Remove-Item -Recurse -Force $pf }
New-Item -ItemType Directory -Force -Path "$pf\runtime\models" | Out-Null
Copy-Item "target\release\$exeName" $pf
Copy-Item "runtime\pdfium.dll" "$pf\runtime\"
Copy-Item "runtime\models\*.rten" "$pf\runtime\models\"
if (Test-Path "README.md") { Copy-Item "README.md" $pf }

# 4. Portable zip
$zip = "$dist\UniversalDocumentPDFTools-Portable.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path "$pf\*" -DestinationPath $zip
Write-Host "Portable package: $zip"

# 5. Installer (optional)
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe","${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe","$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($iscc) {
    & $iscc "installer\setup.iss"
    Write-Host "Installer created in $dist"
} else {
    Write-Host "Inno Setup not found - skipped installer. Install it from https://jrsoftware.org/isdl.php and run this script again." -ForegroundColor Yellow
}
