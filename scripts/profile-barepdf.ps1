# BarePDF release profiling. First-page time is emitted by the application when the
# first page bitmap is attached to the Slint model; Start-Process duration is not used.
param (
    [string]$PdfPath = "$env:USERPROFILE\Desktop\kitap.pdf",
    [int]$DurationSeconds = 20,
    [int]$Runs = 5
)

$ExePath = Resolve-Path (Join-Path $PSScriptRoot "..\target\release\barepdf.exe") -ErrorAction SilentlyContinue
if (-not $ExePath) {
    Write-Error "Release executable not found. Run 'cargo build --release --locked' first."
    exit 1
}
if (-not (Test-Path -LiteralPath $PdfPath -PathType Leaf)) {
    Write-Error "PDF file not found: $PdfPath"
    exit 1
}

function Get-Median([double[]]$Values) {
    $sorted = $Values | Sort-Object
    $count = $sorted.Count
    if ($count -eq 0) { return 0 }
    if ($count % 2 -eq 1) { return $sorted[[int]($count / 2)] }
    return ($sorted[$count / 2 - 1] + $sorted[$count / 2]) / 2
}

function Stop-BarePdfProcess($Process) {
    if ($Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        $Process.WaitForExit()
    }
}

$idleWorkingSets = @()
for ($run = 1; $run -le $Runs; $run++) {
    $process = Start-Process -FilePath $ExePath -PassThru -WindowStyle Hidden
    Start-Sleep -Seconds 3
    $sample = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
    if ($sample) { $idleWorkingSets += $sample.WorkingSet64 / 1MB }
    Stop-BarePdfProcess $process
}

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $profilePath = Join-Path $env:TEMP "barepdf-profile-$([guid]::NewGuid().ToString('N')).json"
    $previousProfilePath = $env:BAREPDF_PROFILE_FILE
    $env:BAREPDF_PROFILE_FILE = $profilePath
    $process = Start-Process -FilePath $ExePath -ArgumentList "`"$PdfPath`"" -PassThru -WindowStyle Hidden
    if ($null -eq $previousProfilePath) {
        Remove-Item Env:BAREPDF_PROFILE_FILE
    } else {
        $env:BAREPDF_PROFILE_FILE = $previousProfilePath
    }

    $peakPrivate = 0.0
    $settledWorkingSet = 0.0
    $settledPrivate = 0.0
    for ($sampleIndex = 0; $sampleIndex -lt ($DurationSeconds * 4); $sampleIndex++) {
        Start-Sleep -Milliseconds 250
        $sample = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
        if (-not $sample) { break }
        $privateMb = $sample.PrivateMemorySize64 / 1MB
        $peakPrivate = [math]::Max($peakPrivate, $privateMb)
        $settledWorkingSet = $sample.WorkingSet64 / 1MB
        $settledPrivate = $privateMb
    }

    $firstBitmapMs = 0.0
    if (Test-Path -LiteralPath $profilePath) {
        $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json
        $firstBitmapMs = [double]$profile.first_bitmap_ms
        Remove-Item -LiteralPath $profilePath -Force
    }

    $results += [PSCustomObject]@{
        Run = $run
        FirstBitmapMs = [math]::Round($firstBitmapMs, 2)
        SettledWorkingSetMB = [math]::Round($settledWorkingSet, 2)
        SettledPrivateMB = [math]::Round($settledPrivate, 2)
        PeakPrivateMB = [math]::Round($peakPrivate, 2)
    }
    Stop-BarePdfProcess $process
}

$firstPageValues = @($results | ForEach-Object { $_.FirstBitmapMs } | Where-Object { $_ -gt 0 })
$settledValues = @($results | ForEach-Object { $_.SettledWorkingSetMB })
$peakValues = @($results | ForEach-Object { $_.PeakPrivateMB })

Write-Host "BarePDF release profile ($Runs runs)" -ForegroundColor Cyan
Write-Host "PDF: $PdfPath"
Write-Host "Idle working set median:       $([math]::Round((Get-Median $idleWorkingSets), 2)) MB"
Write-Host "First bitmap median:           $([math]::Round((Get-Median $firstPageValues), 2)) ms"
Write-Host "Settled working set median:    $([math]::Round((Get-Median $settledValues), 2)) MB"
Write-Host "Peak private memory median:    $([math]::Round((Get-Median $peakValues), 2)) MB"
Write-Host ""
$results | Format-Table -AutoSize
