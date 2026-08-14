# BarePDF release profiling. The application currently emits only the first low-resolution
# page bitmap milestone. Metrics without an application or Windows trace signal are reported
# as unsupported instead of being inferred from process launch timing.
[CmdletBinding()]
param (
    [Alias("FixturePath")]
    [string]$PdfPath = "$env:USERPROFILE\Desktop\kitap.pdf",
    [ValidateSet("custom", "10-page", "500-page", "visual-heavy", "boundary")]
    [string]$FixtureClass = "custom",
    [string]$FixtureName = "",
    [string]$ExpectedSha256 = "",
    [ValidateRange(1, 3600)]
    [int]$DurationSeconds = 20,
    [ValidateRange(1, 100)]
    [int]$Runs = 5
)

$ExePath = Resolve-Path (Join-Path $PSScriptRoot "..\target\release\barepdf.exe") -ErrorAction SilentlyContinue
if (-not $ExePath) {
    Write-Error "Release executable not found. Run 'cargo build --release --locked' first."
    exit 1
}
$ResolvedPdfPath = Resolve-Path -LiteralPath $PdfPath -ErrorAction SilentlyContinue
if (-not $ResolvedPdfPath -or -not (Test-Path -LiteralPath $ResolvedPdfPath.Path -PathType Leaf)) {
    Write-Error "PDF file not found: $PdfPath"
    exit 1
}
if ($ExpectedSha256 -and $ExpectedSha256 -notmatch '^[0-9a-fA-F]{64}$') {
    Write-Error "ExpectedSha256 must contain exactly 64 hexadecimal characters."
    exit 1
}

$PdfPath = $ResolvedPdfPath.Path
$fixture = Get-Item -LiteralPath $PdfPath
$fixtureSha256 = (Get-FileHash -LiteralPath $PdfPath -Algorithm SHA256).Hash.ToLowerInvariant()
if ($ExpectedSha256 -and $fixtureSha256 -ne $ExpectedSha256.ToLowerInvariant()) {
    Write-Error "Fixture SHA-256 mismatch. Expected $($ExpectedSha256.ToLowerInvariant()), got $fixtureSha256."
    exit 1
}
if (-not $FixtureName) {
    $FixtureName = $fixture.BaseName
}

function Get-Percentile([double[]]$Values, [double]$Percentile) {
    if ($Values.Count -eq 0) { return $null }
    $sorted = @($Values | Sort-Object)
    $position = ($sorted.Count - 1) * ($Percentile / 100.0)
    $lower = [math]::Floor($position)
    $upper = [math]::Ceiling($position)
    if ($lower -eq $upper) { return [double]$sorted[$lower] }
    $weight = $position - $lower
    return [double]$sorted[$lower] * (1.0 - $weight) + [double]$sorted[$upper] * $weight
}

function Write-Percentiles([string]$Name, [double[]]$Values, [string]$Unit) {
    if ($Values.Count -eq 0) {
        Write-Host ("{0,-36} UNSUPPORTED" -f $Name)
        return
    }
    $p50 = [math]::Round((Get-Percentile $Values 50), 2)
    $p95 = [math]::Round((Get-Percentile $Values 95), 2)
    Write-Host ("{0,-36} p50={1} {3}; p95={2} {3}" -f $Name, $p50, $p95, $Unit)
}

function Stop-BarePdfProcess($Process) {
    if ($Process -and -not $Process.HasExited) {
        Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
        $Process.WaitForExit()
    }
}

function Get-ReferenceMachine {
    $os = Get-CimInstance Win32_OperatingSystem -ErrorAction SilentlyContinue
    $computer = Get-CimInstance Win32_ComputerSystem -ErrorAction SilentlyContinue
    $processor = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
    $display = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue |
        Where-Object { $_.CurrentHorizontalResolution -and $_.CurrentVerticalResolution } |
        Select-Object -First 1
    $powerPlan = (& powercfg.exe /getactivescheme 2>$null | Out-String).Trim()

    [PSCustomObject]@{
        Computer = $env:COMPUTERNAME
        OS = if ($os) { "$($os.Caption) $($os.Version) (build $($os.BuildNumber)) $($os.OSArchitecture)" } else { "Unavailable" }
        CPU = if ($processor) { $processor.Name.Trim() } else { "Unavailable" }
        PhysicalCores = if ($processor) { $processor.NumberOfCores } else { "Unavailable" }
        LogicalProcessors = [Environment]::ProcessorCount
        MemoryGiB = if ($computer) { [math]::Round($computer.TotalPhysicalMemory / 1GB, 2) } else { "Unavailable" }
        Display = if ($display) { "$($display.CurrentHorizontalResolution)x$($display.CurrentVerticalResolution)" } else { "Unavailable" }
        PowerPlan = if ($powerPlan) { $powerPlan } else { "Unavailable" }
    }
}

$machine = Get-ReferenceMachine
$idleResults = @()
for ($run = 1; $run -le $Runs; $run++) {
    $process = $null
    try {
        $process = Start-Process -FilePath $ExePath -PassThru -WindowStyle Hidden
        Start-Sleep -Seconds 3
        $startSample = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
        if (-not $startSample) { continue }

        $startCpu = $startSample.TotalProcessorTime.TotalSeconds
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $peakPrivate = $startSample.PrivateMemorySize64 / 1MB
        $lastSample = $startSample
        for ($sampleIndex = 0; $sampleIndex -lt ($DurationSeconds * 4); $sampleIndex++) {
            Start-Sleep -Milliseconds 250
            $sample = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
            if (-not $sample) { break }
            $lastSample = $sample
            $peakPrivate = [math]::Max($peakPrivate, $sample.PrivateMemorySize64 / 1MB)
        }
        $stopwatch.Stop()

        $cpuPercent = if ($stopwatch.Elapsed.TotalSeconds -gt 0) {
            (($lastSample.TotalProcessorTime.TotalSeconds - $startCpu) / $stopwatch.Elapsed.TotalSeconds) /
                [Environment]::ProcessorCount * 100.0
        } else {
            0.0
        }
        $idleResults += [PSCustomObject]@{
            Run = $run
            CpuPercent = [math]::Round($cpuPercent, 3)
            WorkingSetMB = [math]::Round($lastSample.WorkingSet64 / 1MB, 2)
            PrivateMB = [math]::Round($lastSample.PrivateMemorySize64 / 1MB, 2)
            PeakPrivateMB = [math]::Round($peakPrivate, 2)
        }
    } finally {
        Stop-BarePdfProcess $process
    }
}

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $profilePath = Join-Path $env:TEMP "barepdf-profile-$([guid]::NewGuid().ToString('N')).json"
    $process = $null
    try {
        $previousProfilePath = $env:BAREPDF_PROFILE_FILE
        $env:BAREPDF_PROFILE_FILE = $profilePath
        try {
            $process = Start-Process -FilePath $ExePath -ArgumentList "`"$PdfPath`"" -PassThru -WindowStyle Hidden
        } finally {
            if ($null -eq $previousProfilePath) {
                Remove-Item Env:BAREPDF_PROFILE_FILE -ErrorAction SilentlyContinue
            } else {
                $env:BAREPDF_PROFILE_FILE = $previousProfilePath
            }
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

        $firstBitmapMs = $null
        if (Test-Path -LiteralPath $profilePath) {
            try {
                $profile = Get-Content -LiteralPath $profilePath -Raw | ConvertFrom-Json
                if ($null -ne $profile.first_bitmap_ms) {
                    $firstBitmapMs = [double]$profile.first_bitmap_ms
                }
            } catch {
                Write-Warning "Run $run profile signal could not be read: $($_.Exception.Message)"
            }
        }

        $results += [PSCustomObject]@{
            Run = $run
            FirstBitmapMs = if ($null -ne $firstBitmapMs) { [math]::Round($firstBitmapMs, 2) } else { $null }
            SettledWorkingSetMB = [math]::Round($settledWorkingSet, 2)
            SettledPrivateMB = [math]::Round($settledPrivate, 2)
            PeakPrivateMB = [math]::Round($peakPrivate, 2)
        }
    } finally {
        Stop-BarePdfProcess $process
        Remove-Item -LiteralPath $profilePath -Force -ErrorAction SilentlyContinue
    }
}

$firstPageValues = @($results | ForEach-Object { $_.FirstBitmapMs } | Where-Object { $null -ne $_ })
$settledWorkingSetValues = @($results | ForEach-Object { $_.SettledWorkingSetMB })
$settledPrivateValues = @($results | ForEach-Object { $_.SettledPrivateMB })
$peakValues = @($results | ForEach-Object { $_.PeakPrivateMB })
$idleCpuValues = @($idleResults | ForEach-Object { $_.CpuPercent })
$idleWorkingSetValues = @($idleResults | ForEach-Object { $_.WorkingSetMB })
$idlePrivateValues = @($idleResults | ForEach-Object { $_.PrivateMB })

Write-Host "BarePDF release profile ($Runs runs)" -ForegroundColor Cyan
Write-Host "Fixture: $FixtureName [$FixtureClass]"
Write-Host "PDF: $PdfPath"
Write-Host "Fixture bytes: $($fixture.Length)"
Write-Host "Fixture SHA-256: $fixtureSha256"
Write-Host "Duration per idle/document run: $DurationSeconds s"
Write-Host "Machine: $($machine.Computer)"
Write-Host "OS: $($machine.OS)"
Write-Host "CPU: $($machine.CPU)"
Write-Host "Cores: $($machine.PhysicalCores) physical / $($machine.LogicalProcessors) logical"
Write-Host "Memory: $($machine.MemoryGiB) GiB"
Write-Host "Display: $($machine.Display)"
Write-Host "Power plan: $($machine.PowerPlan)"
Write-Host ""
Write-Percentiles "Cold start" @() "ms"
Write-Host "  Requires an application-ready signal; process launch time is not a cold-start milestone."
Write-Percentiles "First low-resolution bitmap" $firstPageValues "ms"
Write-Percentiles "First native-DPI bitmap" @() "ms"
Write-Host "  Requires a distinct native-DPI bitmap signal from the application."
Write-Percentiles "Idle CPU" $idleCpuValues "%"
Write-Percentiles "Timer wake-ups" @() "wake-ups/s"
Write-Host "  Requires a WPR/WPA CPU Usage + Thread Activity trace."
Write-Percentiles "Idle working set" $idleWorkingSetValues "MiB"
Write-Percentiles "Idle private memory" $idlePrivateValues "MiB"
Write-Percentiles "Settled working set" $settledWorkingSetValues "MiB"
Write-Percentiles "Settled private memory" $settledPrivateValues "MiB"
Write-Percentiles "Peak private memory" $peakValues "MiB"
Write-Host ""
Write-Host "Idle runs"
$idleResults | Format-Table -AutoSize
Write-Host "Document runs"
$results | Format-Table -AutoSize
