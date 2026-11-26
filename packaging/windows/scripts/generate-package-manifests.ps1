param(
    [string]$VersionOverride,
    [string]$Sha256Override,
    [string]$OutputDir
)

# PowerShell WinGet Package Manifest Generator for BarePDF
# Generates valid Microsoft Windows Package Manager manifests (v1.6.0 schema)
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

# Resolve version from single source of truth (cargo metadata)
$Version = $VersionOverride
if (-not $Version) {
    $Metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    $Version = ($Metadata.packages | Where-Object { $_.name -eq "barepdf" } | Select-Object -First 1).version
}
if (-not $Version) {
    throw "Unable to determine BarePDF package version."
}

# Resolve SHA-256 hash
$InstallerHash = $Sha256Override
$InstallerName = "BarePDF-Setup-x64-v$Version.exe"
$InstallerPath = Join-Path $RepoRoot "target\release\installer\$InstallerName"
$LatestJsonPath = Join-Path $RepoRoot "target\release\artifacts\latest.json"

if (-not $InstallerHash) {
    if (Test-Path -LiteralPath $LatestJsonPath) {
        $LatestData = Get-Content -LiteralPath $LatestJsonPath -Raw | ConvertFrom-Json
        if ($LatestData.version -eq $Version -and $LatestData.installer.sha256) {
            $InstallerHash = $LatestData.installer.sha256
        }
    }
}

if (-not $InstallerHash) {
    if (Test-Path -LiteralPath $InstallerPath) {
        $InstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLower()
    } else {
        # Fallback placeholder for pre-build manifest preparation
        $InstallerHash = "0000000000000000000000000000000000000000000000000000000000000000"
        Write-Warning "Installer binary not found at $InstallerPath; using placeholder hash."
    }
}

$Repository = if ($env:GITHUB_REPOSITORY) { $env:GITHUB_REPOSITORY } else { "Woffluon/BarePDF" }
$InstallerUrl = "https://github.com/$Repository/releases/download/v$Version/$InstallerName"

# Target directory: default to target/release/winget/<version>
if (-not $OutputDir) {
    $OutputDir = Join-Path $RepoRoot "target\release\winget\$Version"
}

if (-not (Test-Path -LiteralPath $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$PackageIdentifier = "Woffluon.BarePDF"

# 1. Version Manifest
$VersionManifestContent = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.version.1.6.0.schema.json
PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
"@

# 2. Installer Manifest
$InstallerManifestContent = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.installer.1.6.0.schema.json
PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
InstallerLocale: en-US
Platform:
  - Windows.Desktop
MinimumOSVersion: 10.0.17763.0
InstallerType: inno
Scope: user
InstallModes:
  - interactive
  - silent
  - silentWithProgress
InstallerSwitches:
  Silent: /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
  SilentWithProgress: /SILENT /SUPPRESSMSGBOXES /NORESTART
UpgradeBehavior: install
FileExtensions:
  - pdf
AppsAndFeaturesEntries:
  - DisplayName: BarePDF
    Publisher: BarePDF Contributors
    DisplayVersion: $Version
    ProductCode: '{B3A82379-88F4-4D4D-A815-998A4476B66C}_is1'
Installers:
  - Architecture: x64
    InstallerUrl: $InstallerUrl
    InstallerSha256: $InstallerHash
ManifestType: installer
ManifestVersion: 1.6.0
"@

# 3. Default Locale Manifest
$LocaleManifestContent = @"
# yaml-language-server: `$schema=https://aka.ms/winget-manifest.defaultLocale.1.6.0.schema.json
PackageIdentifier: $PackageIdentifier
PackageVersion: $Version
PackageLocale: en-US
Publisher: BarePDF Contributors
PublisherUrl: https://github.com/Woffluon/BarePDF
PublisherSupportUrl: https://github.com/Woffluon/BarePDF/issues
PackageName: BarePDF
PackageUrl: https://woffluon.github.io/BarePDF/
License: MIT
LicenseUrl: https://github.com/Woffluon/BarePDF/blob/main/LICENSE
ShortDescription: Offline, memory-bounded PDF reader and editor built with Rust, Slint, and PDFium.
Description: BarePDF is a local Windows and Linux PDF reader and editor built with Rust, Slint, and Google PDFium. It operates offline by default with zero telemetry, byte-bounded LRU render caches, an 8-tool PDF workspace, vector annotations, and Explorer thumbnail previews.
Moniker: barepdf
Tags:
  - pdf
  - pdf-reader
  - pdf-viewer
  - rust
  - slint
  - lightweight
  - open-source
  - privacy
ReleaseNotesUrl: https://github.com/Woffluon/BarePDF/releases/tag/v$Version
ManifestType: defaultLocale
ManifestVersion: 1.6.0
"@

$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$VersionFile = Join-Path $OutputDir "$PackageIdentifier.yaml"
$InstallerFile = Join-Path $OutputDir "$PackageIdentifier.installer.yaml"
$LocaleFile = Join-Path $OutputDir "$PackageIdentifier.locale.en-US.yaml"

[System.IO.File]::WriteAllText($VersionFile, "$VersionManifestContent`n", $Utf8NoBom)
[System.IO.File]::WriteAllText($InstallerFile, "$InstallerManifestContent`n", $Utf8NoBom)
[System.IO.File]::WriteAllText($LocaleFile, "$LocaleManifestContent`n", $Utf8NoBom)

Write-Host "Generated WinGet package manifests for BarePDF v$($Version):" -ForegroundColor Green
Write-Host "  - $VersionFile"
Write-Host "  - $InstallerFile"
Write-Host "  - $LocaleFile"
