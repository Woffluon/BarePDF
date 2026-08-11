Set-StrictMode -Version 3.0

function Get-ProductVersionFromToml {
    param([Parameter(Mandatory)][string]$Content)

    $Match = [regex]::Match(
        $Content,
        '(?ms)(?<=^\[workspace\.package\]\s*\r?\n)(?:(?!^\[).)*?^version\s*=\s*"(?<version>\d+\.\d+\.\d+)"'
    )
    if (-not $Match.Success) {
        throw "[workspace.package].version was not found in Cargo.toml"
    }

    return $Match.Groups["version"].Value
}

function Get-VersionBump {
    param([Parameter(Mandatory)][string]$Message)

    $Header = ($Message -split '\r?\n', 2)[0]
    $Match = [regex]::Match(
        $Header,
        '^(?<type>feat|fix|perf|refactor|build|security|docs|ci|test|chore)(?:\([a-z0-9][a-z0-9._/-]*\))?(?<breaking>!)?: (?<description>\S.*)$'
    )
    if (-not $Match.Success) {
        throw "Invalid Conventional Commit message: '$Header'"
    }

    if ($Match.Groups["breaking"].Success -or $Message -match '(?m)^BREAKING CHANGE:\s+\S') {
        return "major"
    }

    switch ($Match.Groups["type"].Value) {
        "feat" { return "minor" }
        { $_ -in "fix", "perf", "refactor", "build", "security" } { return "patch" }
        default { return "none" }
    }
}

function Get-NextProductVersion {
    param(
        [Parameter(Mandatory)][string]$Version,
        [Parameter(Mandatory)][ValidateSet("major", "minor", "patch", "none")][string]$Bump
    )

    if ($Version -notmatch '^(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)$') {
        throw "Unsupported product version: '$Version'"
    }

    $Major = [int]$Matches["major"]
    $Minor = [int]$Matches["minor"]
    $Patch = [int]$Matches["patch"]
    switch ($Bump) {
        "major" { return "$($Major + 1).0.0" }
        "minor" { return "$Major.$($Minor + 1).0" }
        "patch" { return "$Major.$Minor.$($Patch + 1)" }
        default { return $Version }
    }
}

function Assert-ReleaseBaseContract {
    param(
        [Parameter(Mandatory)][string]$Tag,
        [Parameter(Mandatory)][bool]$HasVersionContract
    )

    if (-not $HasVersionContract -and $Tag -cne "v1.0.0") {
        throw "Only the v1.0.0 release may predate the product-version contract."
    }
}

function Get-LatestReleaseSelection {
    param(
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$Candidates,
        [Parameter(Mandatory)][string]$TargetSha
    )

    if ($Candidates.Count -eq 0) { return }
    $Latest = $Candidates[-1]
    return [pscustomobject]@{
        sha = $TargetSha
        version = $Latest.version
        tag = $Latest.tag
    }
}

function Set-ProductVersionInToml {
    param(
        [Parameter(Mandatory)][string]$Content,
        [Parameter(Mandatory)][string]$Version
    )

    $Pattern = '(?ms)(^\[workspace\.package\]\s*\r?\n(?:(?!^\[).)*?^version\s*=\s*")[^"]+(".*$)'
    $Updated = [regex]::Replace($Content, $Pattern, "`${1}$Version`${2}", 1)
    if ($Updated -eq $Content -and (Get-ProductVersionFromToml $Content) -ne $Version) {
        throw "Failed to update [workspace.package].version"
    }
    return $Updated
}

function Get-ValidatedVersionHistory {
    param(
        [Parameter(Mandatory)][string]$RepoRoot,
        [Parameter(Mandatory)][string]$TargetSha
    )

    $Target = (& git -C $RepoRoot rev-parse "$TargetSha^{commit}" 2>$null).Trim()
    if ($LASTEXITCODE -ne 0) { throw "Cannot resolve version-history target '$TargetSha'." }

    $Commits = @(& git -C $RepoRoot rev-list --reverse --first-parent $Target)
    if ($LASTEXITCODE -ne 0) { throw "Cannot read first-parent history for '$Target'." }
    $ContractStart = $Commits | Where-Object {
        $Path = @(& git -C $RepoRoot ls-tree -r --name-only $_ -- scripts/versioning.ps1)
        $LASTEXITCODE -eq 0 -and $Path -contains "scripts/versioning.ps1"
    } | Select-Object -First 1
    if (-not $ContractStart) { return }

    $ContractCommits = @(& git -C $RepoRoot rev-list --reverse --first-parent "$ContractStart^..$Target")
    if ($LASTEXITCODE -ne 0) { throw "Cannot read version-contract history through '$Target'." }
    $SeenVersions = @{}
    foreach ($Commit in $ContractCommits) {
        $Parent = (& git -C $RepoRoot rev-parse "$Commit^" 2>$null).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Version-contract commit $Commit has no parent." }
        $CurrentToml = (& git -C $RepoRoot show "${Commit}:Cargo.toml") -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo.toml at $Commit." }
        $ParentToml = (& git -C $RepoRoot show "${Parent}:Cargo.toml") -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read Cargo.toml at parent $Parent." }
        $CurrentVersion = Get-ProductVersionFromToml $CurrentToml
        $ParentVersion = Get-ProductVersionFromToml $ParentToml
        $Message = (& git -C $RepoRoot show -s --format=%B $Commit) -join "`n"
        if ($LASTEXITCODE -ne 0) { throw "Cannot read commit message at $Commit." }
        $Bump = Get-VersionBump $Message
        $ExpectedVersion = Get-NextProductVersion -Version $ParentVersion -Bump $Bump
        if ($CurrentVersion -ne $ExpectedVersion) {
            throw "Commit $Commit requires product version $ExpectedVersion ($Bump), but contains $CurrentVersion."
        }
        if ($Bump -ne "none") {
            if ($SeenVersions.ContainsKey($CurrentVersion)) {
                throw "Product version is reused in version-contract history: $CurrentVersion"
            }
            $SeenVersions[$CurrentVersion] = $true
        }
        [pscustomobject]@{
            sha = $Commit
            version = $CurrentVersion
            tag = "v$CurrentVersion"
            bump = $Bump
        }
    }
}
