param(
    [string]$ManifestPath = (Join-Path (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")) "target\release\artifacts\latest.json"),
    [switch]$RequireSignature
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$Bytes = [System.IO.File]::ReadAllBytes($ManifestPath)
$MaxMetadataBytes = 64 * 1024
$MaxInstallerBytes = 256 * 1024 * 1024
if ($Bytes.Length -gt $MaxMetadataBytes) {
    throw "latest.json exceeds the updater metadata size limit"
}
if ($Bytes.Length -ge 3 -and $Bytes[0] -eq 0xEF -and $Bytes[1] -eq 0xBB -and $Bytes[2] -eq 0xBF) {
    throw "latest.json must be UTF-8 without a BOM"
}

$Manifest = [System.Text.Encoding]::UTF8.GetString($Bytes) | ConvertFrom-Json
if ($Manifest.schemaVersion -ne 1 -or $Manifest.version -notmatch '^\d+\.\d+\.\d+$') {
    throw "latest.json has an invalid schema or product version"
}
$ArtifactsDirectory = Split-Path -Parent $ManifestPath
$ExpectedAssets = @(
    "BarePDF-Setup-x64-v$($Manifest.version).exe",
    "BarePDF-Portable-x64-v$($Manifest.version).zip",
    "BarePDF-v$($Manifest.version)-SHA256SUMS.txt",
    "latest.json"
)
if ($RequireSignature) { $ExpectedAssets += "latest.json.sig" }
$ActualAssets = @(Get-ChildItem -LiteralPath $ArtifactsDirectory -File | ForEach-Object Name)
$UnexpectedAssets = @($ActualAssets | Where-Object { $_ -notin $ExpectedAssets })
$MissingAssets = @($ExpectedAssets | Where-Object { $_ -notin $ActualAssets })
if ($UnexpectedAssets.Count -gt 0 -or $MissingAssets.Count -gt 0) {
    throw "Release asset set is not canonical. Missing: $($MissingAssets -join ', '); unexpected: $($UnexpectedAssets -join ', ')"
}
$InstallerName = "BarePDF-Setup-x64-v$($Manifest.version).exe"
$ExpectedReleaseUrl = "https://github.com/Woffluon/BarePDF/releases/tag/v$($Manifest.version)"
$ExpectedUrl = "https://github.com/Woffluon/BarePDF/releases/download/v$($Manifest.version)/$InstallerName"
if (-not [string]::Equals($Manifest.releaseUrl, $ExpectedReleaseUrl, [StringComparison]::Ordinal)) {
    throw "latest.json release URL must target the immutable versioned release"
}
if (-not [string]::Equals($Manifest.installer.url, $ExpectedUrl, [StringComparison]::Ordinal)) {
    throw "latest.json installer URL must target the immutable versioned installer"
}
if ($Manifest.installer.sha256 -notmatch '^[a-f0-9]{64}$' -or $Manifest.installer.size -le 0) {
    throw "latest.json installer integrity metadata is invalid"
}
$InstallerPath = Join-Path $ArtifactsDirectory $InstallerName
$InstallerSize = (Get-Item -LiteralPath $InstallerPath).Length
if ($InstallerSize -gt $MaxInstallerBytes) {
    throw "staged installer exceeds the updater installer size limit"
}
$InstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Manifest.installer.sha256 -cne $InstallerHash -or $Manifest.installer.size -ne $InstallerSize) {
    throw "latest.json installer integrity metadata does not match the staged installer"
}
if ([string]::IsNullOrWhiteSpace($Manifest.releaseNotes)) {
    throw "latest.json releaseNotes must not be empty"
}

$SignaturePath = "$ManifestPath.sig"
if (Test-Path -LiteralPath $SignaturePath) {
    & (Join-Path $PSScriptRoot "update-manifest-signature.ps1") -Action Verify -ManifestPath $ManifestPath -SignaturePath $SignaturePath
    if ($LASTEXITCODE -ne 0) { throw "latest.json signature validation failed" }
} elseif ($RequireSignature) {
    throw "latest.json.sig is required"
}

Write-Host "Release manifest validation passed." -ForegroundColor Green
