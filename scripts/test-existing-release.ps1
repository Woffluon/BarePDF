param(
    [Parameter(Mandatory)][string]$ReleaseSha,
    [Parameter(Mandatory)][string]$ReleaseTag,
    [Parameter(Mandatory)][string]$Version,
    [string]$RepoRoot,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$Token = $env:GH_TOKEN,
    [switch]$NoOutput
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$RepoRoot = if ($RepoRoot) { (Resolve-Path $RepoRoot).Path } else { (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
if ($ReleaseTag -cne "v$Version" -or $Version -notmatch '^\d+\.\d+\.\d+$') { throw "Release tag and version are inconsistent." }
if ([string]::IsNullOrWhiteSpace($Repository)) { throw "Repository is required." }

$RemoteTag = @(& git -C $RepoRoot ls-remote --tags origin "refs/tags/$ReleaseTag" "refs/tags/$ReleaseTag^{}")
if ($LASTEXITCODE -ne 0) { throw "Failed to query remote tag state for $ReleaseTag." }
$TagExists = $RemoteTag.Count -gt 0
if ($TagExists) {
    & git -C $RepoRoot fetch --force origin "refs/tags/${ReleaseTag}:refs/tags/${ReleaseTag}"
    if ($LASTEXITCODE -ne 0) { throw "Failed to fetch existing tag $ReleaseTag." }
    $TaggedCommit = (& git -C $RepoRoot rev-list -n 1 $ReleaseTag).Trim()
    if ($LASTEXITCODE -ne 0 -or $TaggedCommit -cne $ReleaseSha) {
        throw "Tag $ReleaseTag does not point to release commit $ReleaseSha."
    }
}

$Headers = @{
    Accept = "application/vnd.github+json"
    "User-Agent" = "BarePDF-release-workflow"
    "X-GitHub-Api-Version" = "2022-11-28"
}
if (-not [string]::IsNullOrWhiteSpace($Token)) { $Headers.Authorization = "Bearer $Token" }
try {
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/tags/$ReleaseTag" -Headers $Headers
} catch {
    $Status = if ($_.Exception.Response) { [int]$_.Exception.Response.StatusCode } else { 0 }
    if ($Status -ne 404) { throw }
    Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository" -Headers $Headers | Out-Null
    if ($TagExists) { throw "Tag $ReleaseTag exists without a published GitHub Release." }
    if (-not $NoOutput -and $env:GITHUB_OUTPUT) { "needed=true" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append }
    return
}

if (-not $TagExists) { throw "Release $ReleaseTag exists without a matching remote tag." }
if ($Release.tag_name -cne $ReleaseTag -or $Release.draft -or $Release.prerelease -or [string]::IsNullOrWhiteSpace($Release.published_at)) {
    throw "Existing release must be published, non-draft, and non-prerelease."
}
$ReleaseTarget = (& git -C $RepoRoot rev-parse "$($Release.target_commitish)^{commit}" 2>$null).Trim()
if ($LASTEXITCODE -ne 0 -or $ReleaseTarget -cne $ReleaseSha) { throw "Existing release target does not match $ReleaseSha." }

$VersionedInstaller = "BarePDF-Setup-x64-v$Version.exe"
$VersionedPortable = "BarePDF-Portable-x64-v$Version.zip"
$VersionedChecksums = "BarePDF-v$Version-SHA256SUMS.txt"
$AliasInstaller = "BarePDF-Setup-x64.exe"
$AliasPortable = "BarePDF-Portable-x64.zip"
$AliasChecksums = "BarePDF-SHA256SUMS.txt"
$Required = @($VersionedInstaller, $VersionedPortable, $VersionedChecksums, $AliasInstaller, $AliasPortable, $AliasChecksums, "latest.json", "latest.json.sig")
$Assets = @($Release.assets)
$Names = @($Assets | ForEach-Object { $_.name })
$Missing = @($Required | Where-Object { $_ -notin $Names })
if ($Missing.Count -gt 0) { throw "Existing immutable release is incomplete: $($Missing -join ', ')" }

$BytesByName = @{}
$HashByName = @{}
Add-Type -AssemblyName System.Net.Http
$Client = [Net.Http.HttpClient]::new()
try {
    if (-not [string]::IsNullOrWhiteSpace($Token)) {
        $Client.DefaultRequestHeaders.Authorization = [Net.Http.Headers.AuthenticationHeaderValue]::new("Bearer", $Token)
    }
    $Client.DefaultRequestHeaders.UserAgent.ParseAdd("BarePDF-release-workflow")
    foreach ($Name in $Required) {
        $Asset = @($Assets | Where-Object name -ceq $Name)
        if ($Asset.Count -ne 1) { throw "Release asset name must be unique: $Name" }
        $DigestProperty = $Asset[0].PSObject.Properties["digest"]
        $Digest = if ($DigestProperty) { $DigestProperty.Value } else { $null }
        if ($Asset[0].size -le 0 -or $Digest -notmatch '(?i)^sha256:[a-f0-9]{64}$') {
            throw "Release asset lacks required size or SHA-256 digest: $Name"
        }
        $Request = [Net.Http.HttpRequestMessage]::new([Net.Http.HttpMethod]::Get, $Asset[0].url)
        $Request.Headers.Accept.ParseAdd("application/octet-stream")
        try {
            $Response = $Client.SendAsync($Request).GetAwaiter().GetResult()
            try {
                if (-not $Response.IsSuccessStatusCode) { throw "Asset download failed for $Name with HTTP $([int]$Response.StatusCode)." }
                $Bytes = $Response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
            } finally { $Response.Dispose() }
        } finally { $Request.Dispose() }
        if ($Bytes.Length -ne $Asset[0].size) { throw "Downloaded asset size differs from GitHub metadata: $Name" }
        $Hasher = [Security.Cryptography.SHA256]::Create()
        try { $Hash = ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant() } finally { $Hasher.Dispose() }
        if ("sha256:$Hash" -cne $Digest.ToLowerInvariant()) { throw "Downloaded asset digest differs from GitHub metadata: $Name" }
        $BytesByName[$Name] = $Bytes
        $HashByName[$Name] = $Hash
    }
} finally { $Client.Dispose() }

foreach ($Pair in @(@($VersionedInstaller, $AliasInstaller), @($VersionedPortable, $AliasPortable))) {
    if ($BytesByName[$Pair[0]].Length -ne $BytesByName[$Pair[1]].Length -or $HashByName[$Pair[0]] -cne $HashByName[$Pair[1]]) {
        throw "Stable alias does not match versioned asset $($Pair[0])."
    }
}

function Read-ChecksumMap([byte[]]$Bytes, [string]$Name) {
    $Map = @{}
    foreach ($Line in ([Text.Encoding]::UTF8.GetString($Bytes) -split '\r?\n' | Where-Object { $_ })) {
        if ($Line -notmatch '^(?<hash>[a-f0-9]{64})  (?<file>[^\\/]+)$') { throw "Invalid checksum line in ${Name}: $Line" }
        if ($Map.ContainsKey($Matches.file)) { throw "Duplicate checksum entry in ${Name}: $($Matches.file)" }
        $Map[$Matches.file] = $Matches.hash
    }
    return $Map
}

$VersionedMap = Read-ChecksumMap $BytesByName[$VersionedChecksums] $VersionedChecksums
$AliasMap = Read-ChecksumMap $BytesByName[$AliasChecksums] $AliasChecksums
if ($VersionedMap.Count -ne 2 -or $VersionedMap[$VersionedInstaller] -cne $HashByName[$VersionedInstaller] -or $VersionedMap[$VersionedPortable] -cne $HashByName[$VersionedPortable]) {
    throw "Versioned checksum manifest does not match release assets."
}
if ($AliasMap.Count -ne 2 -or $AliasMap[$AliasInstaller] -cne $HashByName[$AliasInstaller] -or $AliasMap[$AliasPortable] -cne $HashByName[$AliasPortable]) {
    throw "Stable checksum manifest does not match release assets."
}

$Manifest = [Text.Encoding]::UTF8.GetString($BytesByName["latest.json"]) | ConvertFrom-Json
$ExpectedInstallerUrl = "https://github.com/$Repository/releases/download/$ReleaseTag/$VersionedInstaller"
if ($Manifest.schemaVersion -ne 1 -or $Manifest.version -cne $Version -or $Manifest.releaseUrl -cne "https://github.com/$Repository/releases/tag/$ReleaseTag") {
    throw "latest.json release identity does not match existing release."
}
if ($Manifest.installer.url -cne $ExpectedInstallerUrl -or $Manifest.installer.sha256 -cne $HashByName[$VersionedInstaller] -or $Manifest.installer.size -ne $BytesByName[$VersionedInstaller].Length) {
    throw "latest.json installer metadata does not match release assets."
}

$SignatureDirectory = Join-Path ([IO.Path]::GetTempPath()) ("barepdf-release-signature-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $SignatureDirectory | Out-Null
try {
    $ManifestPath = Join-Path $SignatureDirectory "latest.json"
    $SignaturePath = Join-Path $SignatureDirectory "latest.json.sig"
    [IO.File]::WriteAllBytes($ManifestPath, $BytesByName["latest.json"])
    [IO.File]::WriteAllBytes($SignaturePath, $BytesByName["latest.json.sig"])
    & (Join-Path $RepoRoot "packaging\windows\scripts\update-manifest-signature.ps1") -Action Verify -ManifestPath $ManifestPath -SignaturePath $SignaturePath
    if ($LASTEXITCODE -ne 0) { throw "Existing release manifest signature is invalid." }
} finally {
    Remove-Item -LiteralPath $SignatureDirectory -Recurse -Force -ErrorAction SilentlyContinue
}

if (-not $NoOutput -and $env:GITHUB_OUTPUT) { "needed=false" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append }
Write-Host "Existing release integrity validated: $ReleaseTag" -ForegroundColor Green
