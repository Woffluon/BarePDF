# PowerShell Release Staging Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
Set-Location $RepoRoot

Write-Host "Building release binaries..." -ForegroundColor Cyan
$CargoPath = Get-Command "cargo" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path
if (-not $CargoPath) {
    $CargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
}
& $CargoPath build --workspace --release --locked

$StagedDir = Join-Path $RepoRoot "target\release\staged"
if (Test-Path $StagedDir) {
    Remove-Item $StagedDir -Recurse -Force
}
New-Item -ItemType Directory -Path $StagedDir | Out-Null

$ExePath = Join-Path $RepoRoot "target\release\barepdf.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "Release executable not found at $ExePath"
}

Copy-Item $ExePath -Destination (Join-Path $StagedDir "BarePDF.exe")
Copy-Item (Join-Path $RepoRoot "README.md") -Destination $StagedDir

$PdfiumDll = Join-Path $RepoRoot "target\release\pdfium.dll"
if (-not (Test-Path $PdfiumDll)) {
    Write-Host "pdfium.dll missing in target\release, fetching PDFium binary package..." -ForegroundColor Yellow
    $TempDir = Join-Path $RepoRoot "target\release\pdfium_temp"
    $TgzPath = Join-Path $RepoRoot "target\release\pdfium-win-x64.tgz"
    if (-not (Test-Path $TempDir)) { New-Item -ItemType Directory -Path $TempDir | Out-Null }
    
    Invoke-WebRequest -Uri "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/7988/pdfium-win-x64.tgz" -OutFile $TgzPath
    tar -xzf $TgzPath -C $TempDir
    Copy-Item (Join-Path $TempDir "bin\pdfium.dll") $PdfiumDll -Force
    Remove-Item $TgzPath, $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not (Test-Path $PdfiumDll)) {
    Write-Error "pdfium.dll is missing and could not be downloaded."
}

Copy-Item $PdfiumDll -Destination $StagedDir -Force

# Copy LICENSE file if present or create standard MIT notice
$LicensePath = Join-Path $RepoRoot "LICENSE"
if (Test-Path $LicensePath) {
    Copy-Item $LicensePath -Destination $StagedDir
} else {
    Set-Content -Path (Join-Path $StagedDir "LICENSE") -Value "MIT License - BarePDF Contributors"
}

Write-Host "Release staging complete at $StagedDir" -ForegroundColor Green
