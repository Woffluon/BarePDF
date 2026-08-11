# PowerShell SHA-256 Checksum Generator Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

$Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$Version = ($Metadata.packages | Where-Object { $_.name -eq "barepdf" } | Select-Object -First 1).version
if (-not $Version) { throw "barepdf package version not found" }

$ArtifactsDir = Join-Path $RepoRoot "target\release\artifacts"
if (Test-Path -LiteralPath $ArtifactsDir) {
    Remove-Item -LiteralPath $ArtifactsDir -Recurse -Force
}
New-Item -ItemType Directory -Path $ArtifactsDir | Out-Null

$InstallerPath = Join-Path $RepoRoot "target\release\installer\BarePDF-Setup-x64-v$Version.exe"
$PortablePath = Join-Path $RepoRoot "target\release\portable\BarePDF-Portable-x64-v$Version.zip"

if (-not (Test-Path $InstallerPath)) { throw "Missing installer: $InstallerPath" }
if (-not (Test-Path $PortablePath)) { throw "Missing portable package: $PortablePath" }

$InstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLower()
$InstallerName = [System.IO.Path]::GetFileName($InstallerPath)
$PortableHash = (Get-FileHash -LiteralPath $PortablePath -Algorithm SHA256).Hash.ToLower()
$PortableName = [System.IO.Path]::GetFileName($PortablePath)
$ChecksumLines = @("$InstallerHash  $InstallerName", "$PortableHash  $PortableName")
Copy-Item $InstallerPath -Destination $ArtifactsDir -Force
Copy-Item $PortablePath -Destination $ArtifactsDir -Force

$ChecksumFile = Join-Path $ArtifactsDir "BarePDF-v$Version-SHA256SUMS.txt"
$ChecksumLines | Set-Content -Path $ChecksumFile

$Repository = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "Woffluon/BarePDF" }
$InstallerSize = (Get-Item -LiteralPath $InstallerPath).Length
$ReleaseNotes = ((& git -C $RepoRoot log -1 --format=%B) -join "`n").Trim()
if ([string]::IsNullOrWhiteSpace($ReleaseNotes)) { $ReleaseNotes = "BarePDF v$Version" }
$Manifest = [ordered]@{
    schemaVersion = 1
    version = $Version
    publishedAt = [DateTimeOffset]::UtcNow.ToString("o")
    releaseUrl = "https://github.com/$Repository/releases/tag/v$Version"
    releaseNotes = $ReleaseNotes
    installer = [ordered]@{
        url = "https://github.com/$Repository/releases/download/v$Version/$InstallerName"
        sha256 = $InstallerHash
        size = $InstallerSize
    }
}
$ManifestJson = $Manifest | ConvertTo-Json -Depth 3
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
[System.IO.File]::WriteAllText((Join-Path $ArtifactsDir "latest.json"), "$ManifestJson`n", $Utf8NoBom)

Write-Host "Generated SHA-256 Checksums:" -ForegroundColor Cyan
Get-Content $ChecksumFile
