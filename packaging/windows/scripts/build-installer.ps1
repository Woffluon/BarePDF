# PowerShell Inno Setup Installer Compiler Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

# Locate ISCC.exe
$IsccPath = Get-Command "iscc.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path
if (-not $IsccPath) {
    $CandidatePaths = @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )
    foreach ($path in $CandidatePaths) {
        if (Test-Path $path) {
            $IsccPath = $path
            break
        }
    }
}

if (-not $IsccPath) {
    Write-Warning "Inno Setup Compiler (ISCC.exe) not found on PATH or standard directories. Installer compilation skipped."
    exit 0
}

$IssPath = Join-Path $RepoRoot "packaging\windows\installer\BarePDF.iss"
Write-Host "Compiling Inno Setup script with ISCC: $IssPath" -ForegroundColor Cyan
& "$IsccPath" "$IssPath"

$AppCargoContent = Get-Content (Join-Path $RepoRoot "apps\barepdf\Cargo.toml") -Raw
if ($AppCargoContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "1.0.0"
}

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
if (-not (Test-Path $InstallerPath)) {
    Write-Error "Expected installer binary not found at $InstallerPath"
}

Write-Host "Installer created successfully at $InstallerPath" -ForegroundColor Green
