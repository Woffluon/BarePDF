# PowerShell Release Staging Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
Set-Location $RepoRoot

Write-Host "Building release binaries..." -ForegroundColor Cyan
& "C:\Users\Efe\.cargo\bin\cargo.exe" build --workspace --release --locked

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

# Copy LICENSE file if present or create standard MIT notice
$LicensePath = Join-Path $RepoRoot "LICENSE"
if (Test-Path $LicensePath) {
    Copy-Item $LicensePath -Destination $StagedDir
} else {
    Set-Content -Path (Join-Path $StagedDir "LICENSE") -Value "MIT License - BarePDF Contributors"
}

Write-Host "Release staging complete at $StagedDir" -ForegroundColor Green
