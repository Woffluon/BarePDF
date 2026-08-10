# PowerShell SHA-256 Checksum Generator Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

$Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$Version = ($Metadata.packages | Where-Object { $_.name -eq "barepdf" } | Select-Object -First 1).version
if (-not $Version) { throw "barepdf package version not found" }

$ArtifactsDir = Join-Path $RepoRoot "target\release\artifacts"
if (-not (Test-Path $ArtifactsDir)) {
    New-Item -ItemType Directory -Path $ArtifactsDir | Out-Null
}

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
$PortablePath = Join-Path $RepoRoot "target\release\portable\BarePDF-Portable-x64-v$Version.zip"

if (-not (Test-Path $InstallerPath)) { throw "Missing installer: $InstallerPath" }
if (-not (Test-Path $PortablePath)) { throw "Missing portable package: $PortablePath" }

$InstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLower()
$InstallerName = [System.IO.Path]::GetFileName($InstallerPath)
$PortableHash = (Get-FileHash -LiteralPath $PortablePath -Algorithm SHA256).Hash.ToLower()
$PortableName = [System.IO.Path]::GetFileName($PortablePath)
$ChecksumLines = @("$InstallerHash  $InstallerName", "$PortableHash  $PortableName")
Copy-Item $InstallerPath -Destination $ArtifactsDir
Copy-Item $PortablePath -Destination $ArtifactsDir

$ChecksumFile = Join-Path $ArtifactsDir "BarePDF-v$Version-SHA256SUMS.txt"
$ChecksumLines | Set-Content -Path $ChecksumFile

Write-Host "Generated SHA-256 Checksums:" -ForegroundColor Cyan
Get-Content $ChecksumFile
