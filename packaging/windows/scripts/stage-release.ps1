# PowerShell Release Staging Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
Set-Location $RepoRoot

Write-Host "Building release binaries..." -ForegroundColor Cyan
$CargoPath = Get-Command "cargo" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Path
if (-not $CargoPath) {
    $CargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
}
$CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
$RustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }
$RemapFlags = @(
    "--remap-path-prefix=$($env:USERPROFILE)=/user",
    "--remap-path-prefix=$RustupHome=/rustup",
    "--remap-path-prefix=$CargoHome=/cargo",
    "--remap-path-prefix=$($RepoRoot.Path)=/barepdf",
    "--remap-path-scope=object,debuginfo,macro"
)
$PreviousEncodedRustflags = $env:CARGO_ENCODED_RUSTFLAGS
$Separator = [char]0x1f
if ([string]::IsNullOrEmpty($PreviousEncodedRustflags)) {
    $env:CARGO_ENCODED_RUSTFLAGS = $RemapFlags -join $Separator
} else {
    $env:CARGO_ENCODED_RUSTFLAGS = (@($PreviousEncodedRustflags) + $RemapFlags) -join $Separator
}

try {
    & $CargoPath build --release -p barepdf --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release build failed with exit code $LASTEXITCODE"
    }
    & $CargoPath build --profile release-unwind -p barepdf-thumbnail --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo release-unwind build for barepdf-thumbnail failed with exit code $LASTEXITCODE"
    }
} finally {
    if ($null -eq $PreviousEncodedRustflags) {
        Remove-Item Env:CARGO_ENCODED_RUSTFLAGS -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_ENCODED_RUSTFLAGS = $PreviousEncodedRustflags
    }
}

$StagedDir = Join-Path $RepoRoot "target\release\staged"
if (Test-Path $StagedDir) {
    Remove-Item $StagedDir -Recurse -Force
}
New-Item -ItemType Directory -Path $StagedDir | Out-Null

$ExePath = Join-Path $RepoRoot "target\release\barepdf.exe"
if (-not (Test-Path $ExePath)) {
    throw "Release executable not found at $ExePath"
}

Copy-Item $ExePath -Destination (Join-Path $StagedDir "BarePDF.exe")

$ThumbnailDllPath = Join-Path $RepoRoot "target\release-unwind\barepdf_thumbnail.dll"
if (-not (Test-Path $ThumbnailDllPath)) {
    throw "Thumbnail DLL not found: $ThumbnailDllPath"
}
Copy-Item $ThumbnailDllPath -Destination (Join-Path $StagedDir "BarePDF.Thumbnail.dll")

Copy-Item (Join-Path $RepoRoot "README.md") -Destination $StagedDir

$ThirdPartyNoticesPath = Join-Path $RepoRoot "THIRD_PARTY_NOTICES.md"
if (-not (Test-Path -LiteralPath $ThirdPartyNoticesPath -PathType Leaf)) {
    throw "THIRD_PARTY_NOTICES.md is missing at $ThirdPartyNoticesPath"
}
Copy-Item -LiteralPath $ThirdPartyNoticesPath -Destination $StagedDir

$PdfiumDll = Join-Path $RepoRoot "target\release\pdfium.dll"
if (-not (Test-Path $PdfiumDll)) {
    throw "pdfium.dll is missing. Supply a separately verified PDFium binary at $PdfiumDll; staging never downloads unsigned native code."
}
$ExpectedPdfiumDllSha256 = "03cc8de22238ea9ffbbf41703f8ef8aae77faeab735583815481f5c2c70a63c7"
$ActualPdfiumDllSha256 = (Get-FileHash -LiteralPath $PdfiumDll -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ActualPdfiumDllSha256 -ne $ExpectedPdfiumDllSha256) {
    throw "pdfium.dll SHA-256 mismatch at $PdfiumDll. Expected $ExpectedPdfiumDllSha256, got $ActualPdfiumDllSha256."
}

Copy-Item $PdfiumDll -Destination $StagedDir -Force

# Copy LICENSE file if present or create standard MIT notice
$LicensePath = Join-Path $RepoRoot "LICENSE"
if (Test-Path $LicensePath) {
    Copy-Item $LicensePath -Destination $StagedDir
} else {
    Set-Content -Path (Join-Path $StagedDir "LICENSE") -Value "MIT License - BarePDF Contributors"
}

Write-Host "Release staging complete at $StagedDir" -ForegroundColor Green
