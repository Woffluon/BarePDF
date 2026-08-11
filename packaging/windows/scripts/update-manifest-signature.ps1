param(
    [Parameter(Mandatory)][ValidateSet("CheckKey", "Sign", "Verify")][string]$Action,
    [string]$ManifestPath,
    [string]$SignaturePath,
    [string]$PrivateKeyPath,
    [string]$PublicKeyPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
if ([string]::IsNullOrWhiteSpace($PublicKeyPath)) {
    $PublicKeyPath = Join-Path $RepoRoot "assets\update-public-key.hex"
}

function Resolve-OpenSsl {
    $Command = Get-Command openssl.exe -ErrorAction SilentlyContinue
    if ($Command) { return $Command.Source }
    foreach ($Candidate in "C:\Program Files\Git\usr\bin\openssl.exe", "C:\Program Files\Git\mingw64\bin\openssl.exe") {
        if (Test-Path -LiteralPath $Candidate) { return $Candidate }
    }
    throw "OpenSSL 3 is required for Ed25519 release manifest signing."
}

function Convert-HexToBytes([string]$Value) {
    $Value = $Value.Trim()
    if ($Value -notmatch '^[a-fA-F0-9]{64}$') { throw "Pinned update public key must be 32-byte hexadecimal." }
    $Bytes = [byte[]]::new(32)
    for ($Index = 0; $Index -lt $Bytes.Length; $Index++) {
        $Bytes[$Index] = [Convert]::ToByte($Value.Substring($Index * 2, 2), 16)
    }
    return $Bytes
}

$OpenSsl = Resolve-OpenSsl
$PublicKey = Convert-HexToBytes ([IO.File]::ReadAllText($PublicKeyPath))
$SpkiPrefix = [byte[]](0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00)
$ExpectedDer = [byte[]]::new($SpkiPrefix.Length + $PublicKey.Length)
[Array]::Copy($SpkiPrefix, 0, $ExpectedDer, 0, $SpkiPrefix.Length)
[Array]::Copy($PublicKey, 0, $ExpectedDer, $SpkiPrefix.Length, $PublicKey.Length)
$TempDirectory = Join-Path ([IO.Path]::GetTempPath()) ("barepdf-update-signing-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $TempDirectory | Out-Null

try {
    $PublicDerPath = Join-Path $TempDirectory "public.der"
    $PublicPemPath = Join-Path $TempDirectory "public.pem"
    [IO.File]::WriteAllBytes($PublicDerPath, $ExpectedDer)
    & $OpenSsl pkey -pubin -inform DER -in $PublicDerPath -out $PublicPemPath
    if ($LASTEXITCODE -ne 0) { throw "Could not load the pinned Ed25519 public key." }

    if ($Action -in "CheckKey", "Sign") {
        if ([string]::IsNullOrWhiteSpace($PrivateKeyPath) -or -not (Test-Path -LiteralPath $PrivateKeyPath)) {
            throw "An Ed25519 private key is required for $Action."
        }
        $ActualDerPath = Join-Path $TempDirectory "derived-public.der"
        & $OpenSsl pkey -in $PrivateKeyPath -pubout -outform DER -out $ActualDerPath
        if ($LASTEXITCODE -ne 0) { throw "The update signing private key is invalid." }
        $ActualDer = [IO.File]::ReadAllBytes($ActualDerPath)
        if (-not [Linq.Enumerable]::SequenceEqual([byte[]]$ExpectedDer, [byte[]]$ActualDer)) {
            throw "The update signing private key does not match the pinned public key."
        }
    }

    if ($Action -eq "CheckKey") {
        Write-Host "Update signing key validation passed." -ForegroundColor Green
        return
    }
    if ([string]::IsNullOrWhiteSpace($ManifestPath) -or -not (Test-Path -LiteralPath $ManifestPath)) {
        throw "A release manifest is required for $Action."
    }
    if ([string]::IsNullOrWhiteSpace($SignaturePath)) { $SignaturePath = "$ManifestPath.sig" }

    if ($Action -eq "Sign") {
        & $OpenSsl pkeyutl -sign -rawin -inkey $PrivateKeyPath -in $ManifestPath -out $SignaturePath
        if ($LASTEXITCODE -ne 0) { throw "Release manifest signing failed." }
    }
    if (-not (Test-Path -LiteralPath $SignaturePath) -or (Get-Item -LiteralPath $SignaturePath).Length -ne 64) {
        throw "Release manifest signature must be exactly 64 bytes."
    }
    & $OpenSsl pkeyutl -verify -pubin -rawin -inkey $PublicPemPath -in $ManifestPath -sigfile $SignaturePath
    if ($LASTEXITCODE -ne 0) { throw "Release manifest signature verification failed." }
    Write-Host "Release manifest signature validation passed." -ForegroundColor Green
} finally {
    Remove-Item -LiteralPath $TempDirectory -Recurse -Force -ErrorAction SilentlyContinue
}
