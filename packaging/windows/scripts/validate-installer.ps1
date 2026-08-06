# PowerShell Installer Test & Validation Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

$AppCargoContent = Get-Content (Join-Path $RepoRoot "apps\barepdf\Cargo.toml") -Raw
if ($AppCargoContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "1.0.0"
}

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
if (-not (Test-Path $InstallerPath)) {
    Write-Warning "Installer binary not found at $InstallerPath. Validation skipped."
    exit 0
}

$TestInstallDir = Join-Path $env:TEMP "BarePDF-TestInstall"
if (Test-Path $TestInstallDir) {
    Remove-Item $TestInstallDir -Recurse -Force
}

Write-Host "Running silent installation test to $TestInstallDir..." -ForegroundColor Cyan
$InstallProc = Start-Process -FilePath $InstallerPath -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /DIR=`"$TestInstallDir`"" -Wait -PassThru

if ($InstallProc.ExitCode -ne 0) {
    Write-Error "Installer exited with non-zero error code: $($InstallProc.ExitCode)"
}

$InstalledExe = Join-Path $TestInstallDir "BarePDF.exe"
if (-not (Test-Path $InstalledExe)) {
    Write-Error "Installed BarePDF.exe not found at $InstalledExe"
}

Write-Host "Checking registry keys..." -ForegroundColor Cyan
$ProgIdPath = "HKCU:\Software\Classes\BarePDF.Document.1"
if (-not (Test-Path $ProgIdPath)) {
    Write-Error "Registry key missing: $ProgIdPath"
}

$OpenWithProgId = "HKCU:\Software\Classes\.pdf\OpenWithProgids"
if (-not (Test-Path $OpenWithProgId)) {
    Write-Error "Registry key missing: $OpenWithProgId"
}

Write-Host "Running uninstaller test..." -ForegroundColor Cyan
$Uninstaller = Join-Path $TestInstallDir "unins000.exe"
if (Test-Path $Uninstaller) {
    $UninstallProc = Start-Process -FilePath $Uninstaller -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES" -Wait -PassThru
    if ($UninstallProc.ExitCode -ne 0) {
        Write-Warning "Uninstaller exited with non-zero code: $($UninstallProc.ExitCode)"
    }
}

if (Test-Path $TestInstallDir) {
    Remove-Item $TestInstallDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "Installer validation completed successfully!" -ForegroundColor Green
