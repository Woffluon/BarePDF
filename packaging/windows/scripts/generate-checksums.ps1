# PowerShell SHA-256 Checksum Generator Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

$AppCargoContent = Get-Content (Join-Path $RepoRoot "apps\barepdf\Cargo.toml") -Raw
if ($AppCargoContent -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "1.0.0"
}

$ArtifactsDir = Join-Path $RepoRoot "target\release\artifacts"
if (-not (Test-Path $ArtifactsDir)) {
    New-Item -ItemType Directory -Path $ArtifactsDir | Out-Null
}

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
$PortablePath = Join-Path $RepoRoot "target\release\portable\BarePDF-Portable-x64-v$Version.zip"

$ChecksumLines = @()

if (Test-Path $InstallerPath) {
    $InstallerHash = (Get-FileHash -Path $InstallerPath -Algorithm SHA256).Hash.ToLower()
    $InstallerName = [System.IO.Path]::GetFileName($InstallerPath)
    $ChecksumLines += "$InstallerHash  $InstallerName"
    Copy-Item $InstallerPath -Destination $ArtifactsDir
}

if (Test-Path $PortablePath) {
    $PortableHash = (Get-FileHash -Path $PortablePath -Algorithm SHA256).Hash.ToLower()
    $PortableName = [System.IO.Path]::GetFileName($PortablePath)
    $ChecksumLines += "$PortableHash  $PortableName"
    Copy-Item $PortablePath -Destination $ArtifactsDir
}

$ChecksumFile = Join-Path $ArtifactsDir "BarePDF-v$Version-SHA256SUMS.txt"
$ChecksumLines | Set-Content -Path $ChecksumFile

Write-Host "Generated SHA-256 Checksums:" -ForegroundColor Cyan
Get-Content $ChecksumFile
