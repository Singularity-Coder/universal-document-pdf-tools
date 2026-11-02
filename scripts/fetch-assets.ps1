# One-time download of the offline engines into .\runtime (internet needed ONCE; the app itself never uses the network).
$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$ProgressPreference = "SilentlyContinue"
Set-Location (Join-Path $PSScriptRoot "..")
New-Item -ItemType Directory -Force -Path "runtime\models" | Out-Null
$ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"

function Get-First($urls, $out) {
    foreach ($u in $urls) {
        try {
            Write-Host "  trying $u"
            Invoke-WebRequest $u -OutFile $out -UserAgent $ua
            if ((Get-Item $out).Length -gt 1MB) { return $true }
        } catch { Write-Host "  failed: $($_.Exception.Message)" }
    }
    return $false
}

# --- PDFium ---
if (-not (Test-Path "runtime\pdfium.dll")) {
    $tmp = Join-Path $env:TEMP ("udpt-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    $arch = if ($env:PROCESSOR_ARCHITECTURE -eq "ARM64") { "arm64" } else { "x64" }
    Write-Host "Downloading PDFium (win-$arch)..."
    Invoke-WebRequest "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-$arch.tgz" -OutFile "$tmp\pdfium.tgz" -UserAgent $ua
    tar -xzf "$tmp\pdfium.tgz" -C $tmp
    Copy-Item "$tmp\bin\pdfium.dll" "runtime\pdfium.dll" -Force
    Remove-Item -Recurse -Force $tmp
} else { Write-Host "PDFium already present, skipping." }

# --- OCR models (several mirrors, first one that works wins) ---
Write-Host "Downloading OCR models..."
$ok = $true
$models = @{
    "text-detection.rten"   = "f15cfb56bd02c4bf478a20343986504a1f01e1665c2b3a0ad66340f054b1b5ca"
    "text-recognition.rten" = "e484866d4cce403175bd8d00b128feb08ab42e208de30e42cd9889d8f1735a6e"
}
foreach ($m in $models.Keys) {
    $dest = "runtime\models\$m"
    $want = $models[$m]
    if ((Test-Path $dest) -and ((Get-FileHash $dest -Algorithm SHA256).Hash.ToLower() -eq $want)) {
        Write-Host "$m already present and verified, skipping."; continue
    }
    $urls = @(
        "https://github.com/deeplabua/deepocr/releases/download/models-v1/$m",
        "https://ocrs-models.s3-accelerate.amazonaws.com/$m",
        "https://ocrs-models.s3.amazonaws.com/$m"
    )
    $got = $false
    foreach ($u in $urls) {
        try {
            Write-Host "  trying $u"
            Invoke-WebRequest $u -OutFile $dest -UserAgent $ua
            $hash = (Get-FileHash $dest -Algorithm SHA256).Hash.ToLower()
            if ($hash -eq $want) { Write-Host "  OK (checksum verified)"; $got = $true; break }
            Write-Host "  checksum mismatch, discarding"; Remove-Item $dest -Force
        } catch { Write-Host "  failed: $($_.Exception.Message)" }
    }
    if (-not $got) { $ok = $false; Write-Host "COULD NOT DOWNLOAD $m" -ForegroundColor Red }
}
if ($ok) { Write-Host "Done. runtime\ now holds everything needed offline." }
else {
    Write-Host ""
    Write-Host "OCR models failed. Download these 2 files manually in your browser (try another network/VPN if blocked)" -ForegroundColor Yellow
    Write-Host "and put them in: $((Get-Location).Path)\runtime\models\"
    Write-Host "  https://github.com/deeplabua/deepocr/releases/download/models-v1/text-detection.rten"
    Write-Host "  https://github.com/deeplabua/deepocr/releases/download/models-v1/text-recognition.rten"
}
