# Downloads the pinned PDFium runtime used by Windows packages and verifies its integrity.
param(
    [string]$Destination
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..\..")
if ([string]::IsNullOrWhiteSpace($Destination)) {
    $Destination = Join-Path $RepoRoot "target\release\pdfium.dll"
}

$PdfiumVersion = "chromium/7988"
$PdfiumUrl = "https://github.com/bblanchon/pdfium-binaries/releases/download/$PdfiumVersion/pdfium-win-x64.tgz"
$ExpectedSha256 = "654daf488d9357d2787cec439d0dbdb93d7e180e2cb28e5f1d41c66d2645daec"
$TempDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("barepdf-pdfium-" + [System.Guid]::NewGuid())
$ArchivePath = Join-Path $TempDirectory "pdfium-win-x64.tgz"
$ExtractDirectory = Join-Path $TempDirectory "extracted"

try {
    New-Item -ItemType Directory -Path $ExtractDirectory -Force | Out-Null
    Invoke-WebRequest -Uri $PdfiumUrl -OutFile $ArchivePath -MaximumRedirection 5

    $ActualSha256 = (Get-FileHash -Path $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualSha256 -ne $ExpectedSha256) {
        throw "PDFium archive SHA-256 mismatch. Expected $ExpectedSha256, got $ActualSha256."
    }

    & tar.exe -xzf $ArchivePath -C $ExtractDirectory
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract the verified PDFium archive."
    }

    $SourceDll = Join-Path $ExtractDirectory "bin\pdfium.dll"
    if (-not (Test-Path $SourceDll -PathType Leaf)) {
        throw "Verified PDFium archive does not contain bin\\pdfium.dll."
    }

    $DestinationDirectory = Split-Path -Parent $Destination
    New-Item -ItemType Directory -Path $DestinationDirectory -Force | Out-Null
    Copy-Item $SourceDll -Destination $Destination -Force
    Write-Host "Verified PDFium $PdfiumVersion at $Destination" -ForegroundColor Green
}
finally {
    if (Test-Path $TempDirectory) {
        Remove-Item $TempDirectory -Recurse -Force
    }
}
