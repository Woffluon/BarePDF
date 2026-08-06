# PowerShell Version Validation Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")

# Extract version from apps/barepdf/Cargo.toml
$AppCargoPath = Join-Path $RepoRoot "apps\barepdf\Cargo.toml"
$AppCargoContent = Get-Content $AppCargoPath -Raw
if ($AppCargoContent -match 'version\s*=\s*"([^"]+)"') {
    $CargoVersion = $Matches[1]
} else {
    Write-Error "Failed to extract version from apps/barepdf/Cargo.toml"
}

# Extract version from BarePDF.iss
$IssPath = Join-Path $RepoRoot "packaging\windows\installer\BarePDF.iss"
$IssContent = Get-Content $IssPath -Raw
if ($IssContent -match '#define MyAppVersion "([^"]+)"') {
    $IssVersion = $Matches[1]
} else {
    Write-Error "Failed to extract version from BarePDF.iss"
}

Write-Host "Cargo Version: $CargoVersion"
Write-Host "Inno Setup Version: $IssVersion"

if ($CargoVersion -ne $IssVersion) {
    Write-Error "Version mismatch! Cargo.toml ($CargoVersion) does not match BarePDF.iss ($IssVersion)"
}

# Validate Git tag if specified
$GitTag = $env:GITHUB_REF_NAME
if ($GitTag -and $GitTag -like "v*") {
    $TagVersion = $GitTag.TrimStart("v")
    Write-Host "Git Tag Version: $TagVersion"
    if ($TagVersion -ne $CargoVersion) {
        Write-Error "Version mismatch! Git tag ($TagVersion) does not match Cargo version ($CargoVersion)"
    }
}

Write-Host "Version validation PASSED successfully!" -ForegroundColor Green
