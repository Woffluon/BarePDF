# BarePDF Complete Build & Packaging Script
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path "$ScriptDir\.."

Set-Location $RepoRoot

function Invoke-PackagingScript([string]$Path) {
    & powershell.exe -NoProfile -File $Path
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Write-Host "1/6 Validating version..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/validate-version.ps1"

Write-Host "2/6 Staging release build..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/stage-release.ps1"

Write-Host "3/6 Building portable ZIP..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/build-portable.ps1"

Write-Host "4/6 Compiling Inno Setup installer..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/build-installer.ps1"

Write-Host "5/6 Validating installer..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/validate-installer.ps1"

Write-Host "6/6 Generating SHA-256 checksums..." -ForegroundColor Cyan
Invoke-PackagingScript "packaging/windows/scripts/generate-checksums.ps1"

Write-Host "Build & installer packaging completed successfully! Artifacts in target/release/artifacts/" -ForegroundColor Green
