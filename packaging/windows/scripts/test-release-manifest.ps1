param(
    [string]$ManifestPath = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")) "target\release\artifacts\latest.json")
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$Bytes = [System.IO.File]::ReadAllBytes($ManifestPath)
if ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF) {
    throw "latest.json must be UTF-8 without a BOM"
}

$Manifest = [System.Text.Encoding]::UTF8.GetString($Bytes) | ConvertFrom-Json
if ($Manifest.schemaVersion -ne 1 -or $Manifest.version -notmatch '^\d+\.\d+\.\d+$') {
    throw "latest.json has an invalid schema or product version"
}
$ExpectedUrl = "/releases/download/v$($Manifest.version)/BarePDF-Setup-x64-v$($Manifest.version).exe"
if (-not $Manifest.installer.url.EndsWith($ExpectedUrl, [StringComparison]::Ordinal)) {
    throw "latest.json installer URL must target the immutable versioned installer"
}
if ($Manifest.installer.sha256 -notmatch '^[a-f0-9]{64}$' -or $Manifest.installer.size -le 0) {
    throw "latest.json installer integrity metadata is invalid"
}
if ([string]::IsNullOrWhiteSpace($Manifest.releaseNotes)) {
    throw "latest.json releaseNotes must not be empty"
}

Write-Host "Release manifest validation passed." -ForegroundColor Green
