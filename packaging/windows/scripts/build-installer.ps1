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
    throw "Inno Setup Compiler (ISCC.exe) is required for release packaging."
}

$IssPath = Join-Path $RepoRoot "packaging\windows\installer\BarePDF.iss"
Write-Host "Compiling Inno Setup script with ISCC: $IssPath" -ForegroundColor Cyan
& "$IsccPath" "$IssPath"
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup compilation failed with exit code $LASTEXITCODE"
}

$Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$Version = ($Metadata.packages | Where-Object { $_.name -eq "barepdf" } | Select-Object -First 1).version
if (-not $Version) { throw "barepdf package version not found" }

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
if (-not (Test-Path $InstallerPath)) {
    throw "Expected installer binary not found at $InstallerPath"
}

Write-Host "Installer created successfully at $InstallerPath" -ForegroundColor Green
