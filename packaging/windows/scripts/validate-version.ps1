param(
    [string]$CommitSha = "HEAD",
    [string]$Message
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
. (Join-Path $RepoRoot "scripts\versioning.ps1")

$CargoContent = [System.IO.File]::ReadAllText((Join-Path $RepoRoot "Cargo.toml"))
$CargoVersion = Get-ProductVersionFromToml $CargoContent
$Metadata = cargo metadata --manifest-path (Join-Path $RepoRoot "Cargo.toml") --locked --no-deps --format-version 1 | ConvertFrom-Json
$WorkspaceIds = @($Metadata.workspace_members)
$WorkspacePackages = @($Metadata.packages | Where-Object { $_.id -in $WorkspaceIds })
$Mismatches = @($WorkspacePackages | Where-Object { $_.version -ne $CargoVersion })
if ($Mismatches.Count -gt 0) {
    $Details = ($Mismatches | ForEach-Object { "$($_.name)=$($_.version)" }) -join ", "
    throw "Workspace packages must inherit product version $CargoVersion. Mismatches: $Details"
}

$IssPath = Join-Path $RepoRoot "packaging\windows\installer\BarePDF.iss"
$IssContent = [System.IO.File]::ReadAllText($IssPath)
if ($IssContent -match '#define\s+MyAppVersion\s+"') {
    throw "BarePDF.iss must not hardcode MyAppVersion"
}
if ($IssContent -notmatch '(?m)^#ifndef\s+MyAppVersion\s*$') {
    throw "BarePDF.iss must require MyAppVersion from build-installer.ps1"
}
if ($IssContent -notmatch '(?m)^VersionInfoVersion=\{#MyAppVersion\}\s*$') {
    throw "BarePDF.iss must bind installer file metadata to MyAppVersion"
}

$ResolvedCommit = (& git -C $RepoRoot rev-parse "$CommitSha^{commit}" 2>$null)
if ($LASTEXITCODE -ne 0) { throw "Cannot resolve commit '$CommitSha'" }
$ResolvedCommit = $ResolvedCommit.Trim()
$CommitCargoLines = & git -C $RepoRoot show "${ResolvedCommit}:Cargo.toml"
if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo.toml at '$ResolvedCommit'" }
$CommitVersion = Get-ProductVersionFromToml ($CommitCargoLines -join "`n")

if ($Message) {
    $Bump = Get-VersionBump $Message
    $ExpectedVersion = Get-NextProductVersion -Version $CommitVersion -Bump $Bump
    if ($CargoVersion -ne $ExpectedVersion) {
        throw "Working tree requires product version $ExpectedVersion ($Bump), but contains $CargoVersion."
    }
    $ValidatedVersion = $CargoVersion
} else {
    if ($CargoVersion -ne $CommitVersion) {
        throw "Working tree version $CargoVersion differs from $CommitSha version $CommitVersion. Pass -Message with the proposed full commit message before committing."
    }
    $Parent = (& git -C $RepoRoot rev-parse "$ResolvedCommit^" 2>$null)
    if ($LASTEXITCODE -eq 0) {
        $ParentCargoLines = & git -C $RepoRoot show "${Parent}:Cargo.toml"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo.toml at parent '$Parent'" }
        $ParentVersion = Get-ProductVersionFromToml ($ParentCargoLines -join "`n")
        $CommitMessage = (& git -C $RepoRoot show -s --format=%B $ResolvedCommit) -join "`n"
        $Bump = Get-VersionBump $CommitMessage
        $ExpectedVersion = Get-NextProductVersion -Version $ParentVersion -Bump $Bump
        if ($CommitVersion -ne $ExpectedVersion) {
            throw "Commit $ResolvedCommit requires product version $ExpectedVersion ($Bump), but contains $CommitVersion."
        }
    }
    $ValidatedVersion = $CommitVersion
    Get-ValidatedVersionHistory -RepoRoot $RepoRoot -TargetSha $ResolvedCommit | Out-Null
}

if ($env:GITHUB_REF_TYPE -eq "tag") {
    $ExpectedTag = "v$ValidatedVersion"
    if ($env:GITHUB_REF_NAME -cne $ExpectedTag) {
        throw "Git tag '$env:GITHUB_REF_NAME' must match product version '$ExpectedTag'."
    }
}

Write-Host "Product version validation passed: $ValidatedVersion" -ForegroundColor Green
