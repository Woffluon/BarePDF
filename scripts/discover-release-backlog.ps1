param(
    [Parameter(Mandatory)][string]$TargetSha,
    [string]$RepoRoot,
    [string]$Repository = $env:GITHUB_REPOSITORY,
    [string]$Token = $env:GH_TOKEN
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$RepoRoot = if ($RepoRoot) { (Resolve-Path $RepoRoot).Path } else { (Resolve-Path (Join-Path $PSScriptRoot "..")).Path }
. (Join-Path $PSScriptRoot "versioning.ps1")

if ([string]::IsNullOrWhiteSpace($Repository)) {
    throw "Repository is required for release discovery."
}

$Target = (& git -C $RepoRoot rev-parse "$TargetSha^{commit}" 2>$null).Trim()
if ($LASTEXITCODE -ne 0) { throw "Cannot resolve release target '$TargetSha'." }
$History = @(Get-ValidatedVersionHistory -RepoRoot $RepoRoot -TargetSha $Target)

$Tags = @(& git -C $RepoRoot tag --merged $Target --list "v*" --sort=-version:refname | Where-Object { $_ -match '^v\d+\.\d+\.\d+$' })
if ($LASTEXITCODE -ne 0) { throw "Cannot read semantic-version tags merged into $Target." }
$BaseCommit = $null
if ($Tags.Count -gt 0) {
    $BaseTag = $Tags[0]
    $BaseVersion = $BaseTag.Substring(1)
    $BaseCommit = (& git -C $RepoRoot rev-list -n 1 $BaseTag 2>$null).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Cannot resolve release base tag '$BaseTag'." }

    $VersionContract = @(& git -C $RepoRoot ls-tree -r --name-only $BaseCommit -- scripts/versioning.ps1)
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect version contract at $BaseTag." }
    $HasVersionContract = $VersionContract -contains "scripts/versioning.ps1"
    Assert-ReleaseBaseContract -Tag $BaseTag -HasVersionContract $HasVersionContract
    if ($HasVersionContract) {
        $TagCargo = (& git -C $RepoRoot show "${BaseCommit}:Cargo.toml") -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo.toml at release base '$BaseTag'." }
        if ((Get-ProductVersionFromToml $TagCargo) -cne $BaseVersion) {
            throw "Release base tag $BaseTag does not match Cargo version at $BaseCommit."
        }
    }

    $Headers = @{
        Accept = "application/vnd.github+json"
        "User-Agent" = "BarePDF-release-workflow"
        "X-GitHub-Api-Version" = "2022-11-28"
    }
    if (-not [string]::IsNullOrWhiteSpace($Token)) { $Headers.Authorization = "Bearer $Token" }
    try {
        $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/tags/$BaseTag" -Headers $Headers
    } catch {
        throw "Highest reachable semantic-version tag $BaseTag has no readable published GitHub Release: $($_.Exception.Message)"
    }
    if ($Release.tag_name -cne $BaseTag -or $Release.draft -or $Release.prerelease -or [string]::IsNullOrWhiteSpace($Release.published_at)) {
        throw "Release base $BaseTag must be a published, non-draft, non-prerelease GitHub Release."
    }
    if ($HasVersionContract) {
        $ReleaseTarget = (& git -C $RepoRoot rev-parse "$($Release.target_commitish)^{commit}" 2>$null).Trim()
        if ($LASTEXITCODE -ne 0 -or $ReleaseTarget -cne $BaseCommit) {
            throw "Release base $BaseTag target does not match tag commit $BaseCommit."
        }
    }
    $IntegrityContract = @(& git -C $RepoRoot ls-tree -r --name-only $BaseCommit -- scripts/test-existing-release.ps1)
    if ($LASTEXITCODE -ne 0) { throw "Cannot inspect release integrity contract at $BaseTag." }
    if ($IntegrityContract -contains "scripts/test-existing-release.ps1") {
        & (Join-Path $PSScriptRoot "test-existing-release.ps1") -ReleaseSha $BaseCommit -ReleaseTag $BaseTag -Version $BaseVersion -RepoRoot $RepoRoot -Repository $Repository -Token $Token -NoOutput
    }
}

$Items = @($History | Where-Object {
    if ($_.bump -eq "none") { return $false }
    if (-not $BaseCommit) { return $true }
    & git -C $RepoRoot merge-base --is-ancestor $BaseCommit $_.sha
    if ($LASTEXITCODE -gt 1) { throw "Cannot compare release base $BaseCommit with $($_.sha)." }
    return $LASTEXITCODE -eq 0 -and $_.sha -cne $BaseCommit
} | ForEach-Object {
    [ordered]@{ sha = $_.sha; version = $_.version; tag = $_.tag }
})

$Json = ConvertTo-Json -InputObject @($Items) -Compress
if ($env:GITHUB_OUTPUT) {
    "releases=$Json" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
    "count=$($Items.Count)" | Out-File -FilePath $env:GITHUB_OUTPUT -Encoding utf8 -Append
}
if ($env:GITHUB_STEP_SUMMARY) {
    "### Release discovery" | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Encoding utf8 -Append
    "Found $($Items.Count) unreleased product version(s) through $Target." | Out-File -FilePath $env:GITHUB_STEP_SUMMARY -Encoding utf8 -Append
}
if (-not $env:GITHUB_OUTPUT) { $Json }
