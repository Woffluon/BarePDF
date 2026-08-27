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
& $CargoPath build --workspace --exclude barepdf-thumbnail --release --locked
if ($LASTEXITCODE -ne 0) {
    throw "Release workspace build failed with exit code $LASTEXITCODE"
}
& $CargoPath build --package barepdf-thumbnail --profile release-unwind --locked
if ($LASTEXITCODE -ne 0) {
    throw "Thumbnail provider build failed with exit code $LASTEXITCODE"
}

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

$ThumbnailDllPath = Join-Path $RepoRoot "target\release-unwind\barepdf_thumbnail.dll"
if (-not (Test-Path $ThumbnailDllPath)) {
    throw "Thumbnail DLL not found: $ThumbnailDllPath"
}
Copy-Item $ThumbnailDllPath -Destination (Join-Path $StagedDir "BarePDF.Thumbnail.dll")

Copy-Item (Join-Path $RepoRoot "README.md") -Destination $StagedDir

$PdfiumDll = Join-Path $RepoRoot "target\release\pdfium.dll"
if (-not (Test-Path $PdfiumDll)) {
    throw "pdfium.dll is missing. Supply a separately verified PDFium binary at $PdfiumDll; staging never downloads unsigned native code."
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
