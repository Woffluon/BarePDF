Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$ManifestValidator = Join-Path $PSScriptRoot "test-release-manifest.ps1"
$FixtureDirectory = Join-Path ([IO.Path]::GetTempPath()) ("barepdf-release-manifest-" + [Guid]::NewGuid().ToString("N"))
$Version = "1.2.3"
$InstallerName = "BarePDF-Setup-x64-v$Version.exe"
$InstallerBytes = [byte[]](1, 2, 3, 4)
$InstallerHash = (Get-FileHash -InputStream ([IO.MemoryStream]::new($InstallerBytes)) -Algorithm SHA256).Hash.ToLowerInvariant()
$InstallerUrl = "https://github.com/Woffluon/BarePDF/releases/download/v$Version/$InstallerName"
$ReleaseUrl = "https://github.com/Woffluon/BarePDF/releases/tag/v$Version"
$MaxMetadataBytes = 64 * 1024
$MaxInstallerBytes = 256 * 1024 * 1024

function Write-Fixture([string]$Url, [string]$Hash, [long]$Size, [string]$FixtureReleaseUrl = $ReleaseUrl, [int]$Padding = 0) {
    New-Item -ItemType Directory -Path $FixtureDirectory -Force | Out-Null
    [IO.File]::WriteAllBytes((Join-Path $FixtureDirectory $InstallerName), $InstallerBytes)
    [IO.File]::WriteAllBytes((Join-Path $FixtureDirectory "BarePDF-Portable-x64-v$Version.zip"), [byte[]](5))
    [IO.File]::WriteAllText((Join-Path $FixtureDirectory "BarePDF-v$Version-SHA256SUMS.txt"), "fixture`n")
    $Manifest = [ordered]@{
        schemaVersion = 1
        version = $Version
        releaseUrl = $FixtureReleaseUrl
        releaseNotes = "Fixture"
        installer = [ordered]@{ url = $Url; sha256 = $Hash; size = $Size }
    }
    if ($Padding -gt 0) { $Manifest.padding = "x" * $Padding }
    $Manifest = $Manifest | ConvertTo-Json -Depth 3
    [IO.File]::WriteAllText((Join-Path $FixtureDirectory "latest.json"), "$Manifest`n", [Text.UTF8Encoding]::new($false))
}

function Write-MetadataSizedFixture([int]$TargetLength) {
    Write-Fixture $InstallerUrl $InstallerHash $InstallerBytes.Length $ReleaseUrl 1
    $ManifestPath = Join-Path $FixtureDirectory "latest.json"
    $Padding = $TargetLength - (([IO.File]::ReadAllBytes($ManifestPath).Length) - 1)
    if ($Padding -le 0) { throw "Fixture metadata target is too small" }
    Write-Fixture $InstallerUrl $InstallerHash $InstallerBytes.Length $ReleaseUrl $Padding
    if ([IO.File]::ReadAllBytes($ManifestPath).Length -ne $TargetLength) {
        throw "Fixture metadata length does not match its target"
    }
}

function Assert-Rejected([string]$ExpectedMessage) {
    try {
        & $ManifestValidator -ManifestPath (Join-Path $FixtureDirectory "latest.json")
    } catch {
        if ($_.Exception.Message -notlike "*$ExpectedMessage*") { throw }
        return
    }
    throw "Expected release manifest validation to fail: $ExpectedMessage"
}

try {
    Write-Fixture $InstallerUrl $InstallerHash $InstallerBytes.Length
    & $ManifestValidator -ManifestPath (Join-Path $FixtureDirectory "latest.json")

    Write-Fixture "https://example.com/releases/download/v$Version/$InstallerName" $InstallerHash $InstallerBytes.Length
    Assert-Rejected "installer URL"

    Write-Fixture $InstallerUrl $InstallerHash $InstallerBytes.Length "https://github.com/Woffluon/BarePDF/releases/tag/v$Version-old"
    Assert-Rejected "release URL"

    Write-Fixture $InstallerUrl ("0" * 64) $InstallerBytes.Length
    Assert-Rejected "does not match the staged installer"

    Write-Fixture $InstallerUrl $InstallerHash ($InstallerBytes.Length + 1)
    Assert-Rejected "does not match the staged installer"

    Write-Fixture $InstallerUrl $InstallerHash $InstallerBytes.Length
    $InstallerStream = [IO.File]::Open((Join-Path $FixtureDirectory $InstallerName), [IO.FileMode]::Open, [IO.FileAccess]::Write)
    try { $InstallerStream.SetLength($MaxInstallerBytes + 1) } finally { $InstallerStream.Dispose() }
    Assert-Rejected "installer size limit"

    Write-MetadataSizedFixture ($MaxMetadataBytes - 1)
    & $ManifestValidator -ManifestPath (Join-Path $FixtureDirectory "latest.json")

    Write-MetadataSizedFixture $MaxMetadataBytes
    & $ManifestValidator -ManifestPath (Join-Path $FixtureDirectory "latest.json")

    Write-MetadataSizedFixture ($MaxMetadataBytes + 1)
    Assert-Rejected "metadata size limit"

    Write-Host "Release manifest validator negative tests passed." -ForegroundColor Green
} finally {
    Remove-Item -LiteralPath $FixtureDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
