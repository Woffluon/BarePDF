param(
    [Parameter(Mandatory, HelpMessage = "Exact full commit message, including any BREAKING CHANGE body")][ValidateNotNullOrEmpty()][string]$Message,
    [string]$RepoRoot
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "versioning.ps1")

$RepoRoot = if ($RepoRoot) { $RepoRoot } else { Join-Path $PSScriptRoot ".." }
$RepoRoot = (Resolve-Path $RepoRoot).Path
$CargoPath = Join-Path $RepoRoot "Cargo.toml"
$LockPath = Join-Path $RepoRoot "Cargo.lock"
$CurrentCargo = [System.IO.File]::ReadAllText($CargoPath)
$CurrentVersion = Get-ProductVersionFromToml $CurrentCargo

$HeadCargoLines = & git -C $RepoRoot show "HEAD:Cargo.toml"
if ($LASTEXITCODE -ne 0) {
    throw "Cannot read Cargo.toml from HEAD"
}
$HeadVersion = Get-ProductVersionFromToml ($HeadCargoLines -join "`n")
$Bump = Get-VersionBump $Message
$ExpectedVersion = Get-NextProductVersion -Version $HeadVersion -Bump $Bump

if ($CurrentVersion -eq $ExpectedVersion) {
    Write-Host "Product version is already prepared: $CurrentVersion"
    return
}
if ($CurrentVersion -ne $HeadVersion) {
    throw "Product version is '$CurrentVersion'; expected HEAD version '$HeadVersion' or prepared version '$ExpectedVersion'."
}
if ($Bump -eq "none") {
    Write-Host "Commit type does not change the product version ($CurrentVersion)."
    return
}

$OriginalLock = if (Test-Path -LiteralPath $LockPath) { [System.IO.File]::ReadAllText($LockPath) } else { $null }
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
try {
    $UpdatedCargo = Set-ProductVersionInToml -Content $CurrentCargo -Version $ExpectedVersion
    [System.IO.File]::WriteAllText($CargoPath, $UpdatedCargo, $Utf8NoBom)

    & cargo metadata --manifest-path $CargoPath --format-version 1 | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed while refreshing Cargo.lock"
    }
} catch {
    [System.IO.File]::WriteAllText($CargoPath, $CurrentCargo, $Utf8NoBom)
    if ($null -ne $OriginalLock) {
        [System.IO.File]::WriteAllText($LockPath, $OriginalLock, $Utf8NoBom)
    }
    throw
}

Write-Host "Prepared product version: $HeadVersion -> $ExpectedVersion ($Bump)" -ForegroundColor Green
