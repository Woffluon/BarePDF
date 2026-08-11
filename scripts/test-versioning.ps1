Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "versioning.ps1")

function Assert-Equal($Expected, $Actual, [string]$Case) {
    if ($Expected -cne $Actual) {
        throw "$Case failed: expected '$Expected', got '$Actual'"
    }
}

$Cases = @(
    @{ Message = "feat: add updater"; Bump = "minor"; Version = "1.3.0" },
    @{ Message = "fix(ui): prevent crash"; Bump = "patch"; Version = "1.2.4" },
    @{ Message = "perf(render): reduce copies"; Bump = "patch"; Version = "1.2.4" },
    @{ Message = "feat!: replace settings format"; Bump = "major"; Version = "2.0.0" },
    @{ Message = "fix: migrate`n`nBREAKING CHANGE: settings reset"; Bump = "major"; Version = "2.0.0" },
    @{ Message = "docs: update guide"; Bump = "none"; Version = "1.2.3" }
)

foreach ($Case in $Cases) {
    $Bump = Get-VersionBump $Case.Message
    Assert-Equal $Case.Bump $Bump $Case.Message
    Assert-Equal $Case.Version (Get-NextProductVersion -Version "1.2.3" -Bump $Bump) $Case.Message
}

$Failed = $false
try { Get-VersionBump "updated the app" | Out-Null } catch { $Failed = $true }
if (-not $Failed) { throw "Invalid commit message was accepted" }

$Toml = "[workspace]`nmembers = []`n`n[workspace.package]`nversion = `"1.2.3`"`nedition = `"2021`"`n"
Assert-Equal "1.2.3" (Get-ProductVersionFromToml $Toml) "read Cargo version"
$Updated = Set-ProductVersionInToml -Content $Toml -Version "1.3.0"
Assert-Equal "1.3.0" (Get-ProductVersionFromToml $Updated) "write Cargo version"

Assert-ReleaseBaseContract -Tag "v1.0.0" -HasVersionContract $false
foreach ($LegacyTag in "v0.9.0", "v1.0.1", "v2.0.0") {
    $LegacyFailed = $false
    try { Assert-ReleaseBaseContract -Tag $LegacyTag -HasVersionContract $false } catch { $LegacyFailed = $true }
    if (-not $LegacyFailed) { throw "pre-contract release base was accepted: $LegacyTag" }
}
Assert-ReleaseBaseContract -Tag "v2.0.0" -HasVersionContract $true

$ReleaseCandidates = @(
    [pscustomobject]@{ version = "1.1.0"; tag = "v1.1.0" },
    [pscustomobject]@{ version = "1.1.1"; tag = "v1.1.1" }
)
Assert-Equal "1.1.1" (Select-LatestUnreleasedVersion -Candidates $ReleaseCandidates).version "release backlog compaction"
Assert-Equal 0 @(Select-LatestUnreleasedVersion -Candidates @()).Count "empty release backlog"

$PreparePath = Join-Path $PSScriptRoot "prepare-version.ps1"
$TestRoot = Join-Path ([IO.Path]::GetTempPath()) ("barepdf-versioning-" + [Guid]::NewGuid().ToString("N"))
$Utf8NoBom = [Text.UTF8Encoding]::new($false)
function Write-TestFile([string]$Path, [string]$Content) {
    $Parent = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $Parent)) { New-Item -ItemType Directory -Path $Parent | Out-Null }
    [IO.File]::WriteAllText($Path, $Content, $Utf8NoBom)
}
function Initialize-TestRepo([string]$Path) {
    & git -C $Path init --quiet
    if ($LASTEXITCODE -ne 0) { throw "Failed to initialize test repository." }
    & git -C $Path add --all
    & git -C $Path -c user.name=BarePDF-Test -c user.email=test@example.invalid commit --quiet --no-gpg-sign -m "chore: initialize test repository"
    if ($LASTEXITCODE -ne 0) { throw "Failed to commit test repository." }
}

try {
    $SuccessRepo = Join-Path $TestRoot "success"
    New-Item -ItemType Directory -Path $SuccessRepo | Out-Null
    Write-TestFile (Join-Path $SuccessRepo "Cargo.toml") "[workspace]`nmembers = [`"app`"]`nresolver = `"2`"`n`n[workspace.package]`nversion = `"1.0.0`"`nedition = `"2021`"`n"
    Write-TestFile (Join-Path $SuccessRepo "app\Cargo.toml") "[package]`nname = `"version-test-app`"`nversion.workspace = true`nedition.workspace = true`n"
    Write-TestFile (Join-Path $SuccessRepo "app\src\lib.rs") "pub fn version_test() {}`n"
    & cargo generate-lockfile --quiet --manifest-path (Join-Path $SuccessRepo "Cargo.toml")
    if ($LASTEXITCODE -ne 0) { throw "Failed to create test Cargo.lock." }
    Initialize-TestRepo $SuccessRepo

    & $PreparePath -Message "feat: add updater" -RepoRoot $SuccessRepo
    Assert-Equal "1.1.0" (Get-ProductVersionFromToml ([IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.toml")))) "prepare version"
    if ([IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.lock")) -notmatch '(?ms)name = "version-test-app"\s+version = "1\.1\.0"') {
        throw "prepare version failed to refresh Cargo.lock"
    }
    $PreparedCargo = [IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.toml"))
    $PreparedLock = [IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.lock"))
    & $PreparePath -Message "feat: add updater" -RepoRoot $SuccessRepo
    Assert-Equal $PreparedCargo ([IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.toml"))) "prepare idempotency Cargo.toml"
    Assert-Equal $PreparedLock ([IO.File]::ReadAllText((Join-Path $SuccessRepo "Cargo.lock"))) "prepare idempotency Cargo.lock"

    Write-TestFile (Join-Path $SuccessRepo "Cargo.toml") (Set-ProductVersionInToml -Content $PreparedCargo -Version "1.2.0")
    $ConflictFailed = $false
    try { & $PreparePath -Message "fix: conflicting version" -RepoRoot $SuccessRepo } catch { $ConflictFailed = $true }
    if (-not $ConflictFailed) { throw "prepare version accepted a conflicting product version" }
    Write-TestFile (Join-Path $SuccessRepo "Cargo.toml") $PreparedCargo
    Write-TestFile (Join-Path $SuccessRepo "Cargo.lock") $PreparedLock
    Write-TestFile (Join-Path $SuccessRepo "scripts\versioning.ps1") ([IO.File]::ReadAllText((Join-Path $PSScriptRoot "versioning.ps1")))
    & git -C $SuccessRepo add Cargo.toml Cargo.lock scripts/versioning.ps1
    & git -C $SuccessRepo -c user.name=BarePDF-Test -c user.email=test@example.invalid commit --quiet --no-gpg-sign -m "feat: add updater"
    if ($LASTEXITCODE -ne 0) { throw "Failed to commit prepared test version." }
    $History = @(Get-ValidatedVersionHistory -RepoRoot $SuccessRepo -TargetSha HEAD)
    Assert-Equal 1 $History.Count "version history count"
    Assert-Equal "1.1.0" $History[0].version "version history release"

    Write-TestFile (Join-Path $SuccessRepo "Cargo.toml") (Set-ProductVersionInToml -Content $PreparedCargo -Version "1.2.0")
    & git -C $SuccessRepo add Cargo.toml
    & git -C $SuccessRepo -c user.name=BarePDF-Test -c user.email=test@example.invalid commit --quiet --no-gpg-sign -m "docs: invalid version bump"
    if ($LASTEXITCODE -ne 0) { throw "Failed to commit invalid history fixture." }
    $HistoryFailed = $false
    try { Get-ValidatedVersionHistory -RepoRoot $SuccessRepo -TargetSha HEAD | Out-Null } catch { $HistoryFailed = $true }
    if (-not $HistoryFailed) { throw "version history accepted an invalid transition" }

    $RollbackRepo = Join-Path $TestRoot "rollback"
    New-Item -ItemType Directory -Path $RollbackRepo | Out-Null
    Write-TestFile (Join-Path $RollbackRepo "Cargo.toml") "[workspace]`nmembers = [`"missing`"]`n`n[workspace.package]`nversion = `"1.0.0`"`nedition = `"2021`"`n"
    Write-TestFile (Join-Path $RollbackRepo "Cargo.lock") "rollback sentinel`n"
    Initialize-TestRepo $RollbackRepo
    $OriginalCargo = [IO.File]::ReadAllText((Join-Path $RollbackRepo "Cargo.toml"))
    $OriginalLock = [IO.File]::ReadAllText((Join-Path $RollbackRepo "Cargo.lock"))
    $RollbackFailed = $false
    try { & $PreparePath -Message "fix: trigger cargo failure" -RepoRoot $RollbackRepo 2>$null } catch { $RollbackFailed = $true }
    if (-not $RollbackFailed) { throw "prepare version did not propagate cargo metadata failure" }
    Assert-Equal $OriginalCargo ([IO.File]::ReadAllText((Join-Path $RollbackRepo "Cargo.toml"))) "rollback Cargo.toml"
    Assert-Equal $OriginalLock ([IO.File]::ReadAllText((Join-Path $RollbackRepo "Cargo.lock"))) "rollback Cargo.lock"
} finally {
    $ResolvedTemp = [IO.Path]::GetFullPath($TestRoot)
    if ($ResolvedTemp.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $ResolvedTemp)) {
        Remove-Item -LiteralPath $ResolvedTemp -Recurse -Force
    }
}

Write-Host "Versioning tests passed." -ForegroundColor Green
