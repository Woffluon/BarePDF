# BarePDF Complete Build & Packaging Script
$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path "$ScriptDir\.."

Set-Location $RepoRoot

Write-Host "1/5 Validating version..." -ForegroundColor Cyan
powershell -File packaging/windows/scripts/validate-version.ps1

Write-Host "2/5 Staging release build..." -ForegroundColor Cyan
powershell -File packaging/windows/scripts/stage-release.ps1

Write-Host "3/5 Building portable ZIP..." -ForegroundColor Cyan
powershell -File packaging/windows/scripts/build-portable.ps1

Write-Host "4/5 Compiling Inno Setup installer..." -ForegroundColor Cyan
powershell -File packaging/windows/scripts/build-installer.ps1

Write-Host "5/5 Generating SHA-256 checksums..." -ForegroundColor Cyan
powershell -File packaging/windows/scripts/generate-checksums.ps1

Write-Host "Build & installer packaging completed successfully! Artifacts in target/release/artifacts/" -ForegroundColor Green
