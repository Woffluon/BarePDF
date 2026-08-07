# PowerShell Portable ZIP Builder Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

# Get Version
$AppCargoContent = Get-Content (Join-Path $RepoRoot "apps\barepdf\Cargo.toml") -Raw
if ($AppCargoContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    Write-Error "Could not parse version from Cargo.toml"
}

$StagedDir = Join-Path $RepoRoot "target\release\staged"
if (-not (Test-Path $StagedDir)) {
    Write-Error "Staged directory missing. Run stage-release.ps1 first."
}

$PortableDir = Join-Path $RepoRoot "target\release\portable"
if (-not (Test-Path $PortableDir)) {
    New-Item -ItemType Directory -Path $PortableDir | Out-Null
}

$ZipName = "BarePDF-Portable-x64-v$Version.zip"
$ZipPath = Join-Path $PortableDir $ZipName

if (Test-Path $ZipPath) {
    try {
        Remove-Item $ZipPath -Force -ErrorAction Stop
    } catch {
        Start-Sleep -Milliseconds 500
        Remove-Item $ZipPath -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "Creating Portable ZIP: $ZipPath" -ForegroundColor Cyan
Compress-Archive -Path "$StagedDir\*" -DestinationPath $ZipPath

Write-Host "Portable package created successfully!" -ForegroundColor Green
