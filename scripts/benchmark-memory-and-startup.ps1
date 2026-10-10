<#
.SYNOPSIS
    Automated benchmark measuring BarePDF startup latency, memory footprint, and idle background CPU.

.DESCRIPTION
    Measures:
    1. Cold and hot startup latency (using BAREPDF_PROFILE_FILE or process start to responsive window).
    2. Memory footprint: Private bytes and Working Set (MB) at idle and after loading a sample PDF.
    3. Idle background CPU utilization (verifying <= 0.1% at idle).
    4. Outputs clean formatted results in Markdown table and JSON format suitable for docs/BENCHMARKS.md.

.PARAMETER ExecutablePath
    Path to barepdf.exe. Defaults to target/release/barepdf.exe or target/debug/barepdf.exe.

.PARAMETER PdfPath
    Path to a sample PDF fixture. Defaults to assets/barepdf-welcome.pdf.

.PARAMETER Runs
    Number of hot startup latency measurement iterations. Defaults to 5.

.PARAMETER IdleSeconds
    Seconds to sample idle background CPU and memory. Defaults to 5.

.PARAMETER MarkdownOutputPath
    Optional file path to write Markdown benchmark report.

.PARAMETER JsonOutputPath
    Optional file path to write JSON benchmark report.
#>
[CmdletBinding()]
param (
    [string]$ExecutablePath = "",
    [Alias("FixturePath")]
    [string]$PdfPath = "",
    [ValidateRange(1, 50)]
    [int]$Runs = 5,
    [Alias("DurationSeconds")]
    [ValidateRange(1, 60)]
    [int]$IdleSeconds = 5,
    [ValidateSet("Efficient", "Enhanced")]
    [string]$VisualMode = "Efficient",
    [string]$MarkdownOutputPath = "",
    [Alias("ResultPath", "OutputPath")]
    [string]$JsonOutputPath = ""
)

$ErrorActionPreference = "Stop"
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

# 1. Resolve Executable
if (-not $ExecutablePath) {
    $releaseExe = Join-Path $repoRoot "target\release\barepdf.exe"
    $debugExe = Join-Path $repoRoot "target\debug\barepdf.exe"
    if (Test-Path -LiteralPath $releaseExe -PathType Leaf) {
        $ExecutablePath = $releaseExe
    } elseif (Test-Path -LiteralPath $debugExe -PathType Leaf) {
        $ExecutablePath = $debugExe
    } else {
        Write-Host "No pre-built barepdf binary found. Building release binary..." -ForegroundColor Yellow
        & cargo build --release --locked -p barepdf
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $releaseExe -PathType Leaf)) {
            Write-Error "Failed to build target/release/barepdf.exe. Run 'cargo build --release -p barepdf' first."
            exit 1
        }
        $ExecutablePath = $releaseExe
    }
}
$ExePath = (Resolve-Path -LiteralPath $ExecutablePath).Path

# 2. Resolve Sample PDF
if (-not $PdfPath) {
    $PdfPath = Join-Path $repoRoot "assets\barepdf-welcome.pdf"
}
if (-not (Test-Path -LiteralPath $PdfPath -PathType Leaf)) {
    Write-Error "Sample PDF fixture not found at '$PdfPath'."
    exit 1
}
$ResolvedPdfPath = (Resolve-Path -LiteralPath $PdfPath).Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " BarePDF Performance & Resource Benchmark" -ForegroundColor Cyan
Write-Host " Executable : $ExePath" -ForegroundColor Gray
Write-Host " Sample PDF : $ResolvedPdfPath" -ForegroundColor Gray
Write-Host " Iterations : $Runs hot runs, ${IdleSeconds}s idle sample" -ForegroundColor Gray
Write-Host "==========================================================" -ForegroundColor Cyan

function New-BenchmarkAppData {
    $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\')
    $directory = Join-Path $tempRoot ("barepdf-bench-appdata-" + [guid]::NewGuid().ToString("N"))
    $configDirectory = Join-Path $directory "BarePDF"
    New-Item -ItemType Directory -Path $configDirectory -Force -ErrorAction Stop | Out-Null
    $configPath = Join-Path $configDirectory "config.json"
    $config = [ordered]@{
        theme = "Dark"
        update_checks_enabled = $false
        last_window_width = 1260
        last_window_height = 926
        enhanced_ui = ($VisualMode -eq "Enhanced")
    }
    [System.IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json), [System.Text.UTF8Encoding]::new($false))
    return [PSCustomObject]@{ Root = $directory; Config = $configPath }
}

function Remove-BenchmarkAppData($Directory) {
    if ($Directory -and (Test-Path -LiteralPath $Directory -PathType Container)) {
        Remove-Item -LiteralPath $Directory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Stop-BarePdfProcess($Process) {
    if ($Process -and -not $Process.HasExited) {
        try {
            Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
            $Process.WaitForExit(3000) | Out-Null
        } catch {}
    }
}

function Measure-SingleStartup {
    param (
        [string]$BinaryPath,
        [string]$ArgumentPath
    )

    $tempProfile = [System.IO.Path]::GetTempFileName()
    $benchAppData = New-BenchmarkAppData
    $prevAppData = [Environment]::GetEnvironmentVariable("APPDATA", "Process")
    $process = $null

    try {
        [Environment]::SetEnvironmentVariable("APPDATA", $benchAppData.Root, "Process")
        $prevEnv = $env:BAREPDF_PROFILE_FILE
        $env:BAREPDF_PROFILE_FILE = $tempProfile

        $startInfo = New-Object System.Diagnostics.ProcessStartInfo
        $startInfo.FileName = $BinaryPath
        if ($ArgumentPath) {
            $startInfo.Arguments = "`"$ArgumentPath`""
        }
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true

        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        $process = [System.Diagnostics.Process]::Start($startInfo)
        if ($null -eq $process) {
            throw "Failed to start process '$BinaryPath'"
        }

        $recordedMs = $null
        $deadline = [DateTime]::UtcNow.AddSeconds(15)

        while ([DateTime]::UtcNow -lt $deadline) {
            Start-Sleep -Milliseconds 20
            if (Test-Path -LiteralPath $tempProfile) {
                try {
                    $content = [System.IO.File]::ReadAllText($tempProfile)
                    if ($content -match '"first_bitmap_ms"\s*:\s*([0-9.]+)') {
                        $recordedMs = [double]$Matches[1]
                        break
                    }
                } catch {
                    # Retry on file lock
                }
            }
            if ($process.HasExited) {
                break
            }
        }
        $sw.Stop()

        if ($null -eq $recordedMs) {
            $recordedMs = [double]$sw.ElapsedMilliseconds
        }

        return $recordedMs
    } finally {
        Stop-BarePdfProcess $process
        $env:BAREPDF_PROFILE_FILE = $prevEnv
        [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
        Remove-BenchmarkAppData $benchAppData.Root
        Remove-Item -LiteralPath $tempProfile -Force -ErrorAction SilentlyContinue
    }
}

function Get-PercentileValue {
    param (
        [double[]]$Values,
        [double]$Percentile
    )
    if ($Values.Count -eq 0) { return 0.0 }
    $sorted = @($Values | Sort-Object)
    $index = ($sorted.Count - 1) * ($Percentile / 100.0)
    $lower = [math]::Floor($index)
    $upper = [math]::Ceiling($index)
    if ($lower -eq $upper) { return [double]$sorted[$lower] }
    $weight = $index - $lower
    return [double]$sorted[$lower] * (1.0 - $weight) + [double]$sorted[$upper] * $weight
}

# --- 1. Startup Latency Benchmarks ---
Write-Host "`nMeasuring Cold Startup Latency..." -ForegroundColor Green
$coldStartupMs = [math]::Round((Measure-SingleStartup -BinaryPath $ExePath -ArgumentPath $ResolvedPdfPath), 2)
Write-Host ("  Cold Startup : {0} ms" -f $coldStartupMs) -ForegroundColor White

Write-Host "Measuring Hot Startup Latency ($Runs iterations)..." -ForegroundColor Green
$hotLatencies = @()
for ($i = 1; $i -le $Runs; $i++) {
    $lat = Measure-SingleStartup -BinaryPath $ExePath -ArgumentPath $ResolvedPdfPath
    $hotLatencies += $lat
    Write-Host ("  Run #{0}: {1:N2} ms" -f $i, $lat) -ForegroundColor Gray
}

$hotP50 = [math]::Round((Get-PercentileValue -Values $hotLatencies -Percentile 50), 2)
$hotP95 = [math]::Round((Get-PercentileValue -Values $hotLatencies -Percentile 95), 2)
$hotMin = [math]::Round(($hotLatencies | Measure-Object -Minimum).Minimum, 2)
$hotMax = [math]::Round(($hotLatencies | Measure-Object -Maximum).Maximum, 2)
$hotAvg = [math]::Round(($hotLatencies | Measure-Object -Average).Average, 2)
Write-Host ("  Hot Startup  : p50={0} ms, p95={1} ms (avg={2} ms, min={3} ms, max={4} ms)" -f $hotP50, $hotP95, $hotAvg, $hotMin, $hotMax) -ForegroundColor White

# --- 2. Idle Memory & Background CPU Benchmark ---
Write-Host "`nMeasuring Idle Resource Utilization (${IdleSeconds}s duration)..." -ForegroundColor Green
$idleAppData = New-BenchmarkAppData
$prevAppData = [Environment]::GetEnvironmentVariable("APPDATA", "Process")
[Environment]::SetEnvironmentVariable("APPDATA", $idleAppData.Root, "Process")

$idleProc = $null
try {
    $idleProc = Start-Process -FilePath $ExePath -PassThru -WindowStyle Hidden
    Start-Sleep -Milliseconds 2000 # Wait for initialization

    $idleStartCpu = $idleProc.TotalProcessorTime.TotalMilliseconds
    $idleStartStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()

    Start-Sleep -Seconds $IdleSeconds

    $idleProc.Refresh()
    $idleEndCpu = $idleProc.TotalProcessorTime.TotalMilliseconds
    $idleEndStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()

    $idleElapsedMs = ($idleEndStamp - $idleStartStamp) / [System.Diagnostics.Stopwatch]::Frequency * 1000
    $idleCpuDeltaMs = [math]::Max(0.0, ($idleEndCpu - $idleStartCpu))
    $idleCpuPercent = [math]::Round(($idleCpuDeltaMs / ($idleElapsedMs * [Environment]::ProcessorCount)) * 100, 2)
    $idleWorkingSetMB = [math]::Round($idleProc.WorkingSet64 / 1MB, 2)
    $idlePrivateBytesMB = [math]::Round($idleProc.PrivateMemorySize64 / 1MB, 2)
} finally {
    Stop-BarePdfProcess $idleProc
    [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
    Remove-BenchmarkAppData $idleAppData.Root
}

Write-Host ("  Idle Private Bytes : {0} MB" -f $idlePrivateBytesMB) -ForegroundColor White
Write-Host ("  Idle Working Set   : {0} MB" -f $idleWorkingSetMB) -ForegroundColor White
Write-Host ("  Idle Background CPU: {0:N2}%" -f $idleCpuPercent) -ForegroundColor White

# --- 3. Document Loaded Memory & CPU Benchmark ---
Write-Host "`nMeasuring Document Loaded Resource Utilization ($([System.IO.Path]::GetFileName($ResolvedPdfPath)))..." -ForegroundColor Green
$docAppData = New-BenchmarkAppData
[Environment]::SetEnvironmentVariable("APPDATA", $docAppData.Root, "Process")

$docProc = $null
try {
    $docProc = Start-Process -FilePath $ExePath -ArgumentList "`"$ResolvedPdfPath`"" -PassThru -WindowStyle Hidden
    Start-Sleep -Milliseconds 2500 # Wait for page rendering

    $docStartCpu = $docProc.TotalProcessorTime.TotalMilliseconds
    $docStartStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()

    $sampleDuration = [int]([math]::Max(2, [int]($IdleSeconds / 2)))
    Start-Sleep -Seconds $sampleDuration

    $docProc.Refresh()
    $docEndCpu = $docProc.TotalProcessorTime.TotalMilliseconds
    $docEndStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()

    $docElapsedMs = ($docEndStamp - $docStartStamp) / [System.Diagnostics.Stopwatch]::Frequency * 1000
    $docCpuDeltaMs = [math]::Max(0.0, ($docEndCpu - $docStartCpu))
    $docCpuPercent = [math]::Round(($docCpuDeltaMs / ($docElapsedMs * [Environment]::ProcessorCount)) * 100, 2)
    $docWorkingSetMB = [math]::Round($docProc.WorkingSet64 / 1MB, 2)
    $docPrivateBytesMB = [math]::Round($docProc.PrivateMemorySize64 / 1MB, 2)
} finally {
    Stop-BarePdfProcess $docProc
    [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
    Remove-BenchmarkAppData $docAppData.Root
}

Write-Host ("  Loaded Private Bytes : {0} MB" -f $docPrivateBytesMB) -ForegroundColor White
Write-Host ("  Loaded Working Set   : {0} MB" -f $docWorkingSetMB) -ForegroundColor White
Write-Host ("  Loaded Idle CPU      : {0:N2}%" -f $docCpuPercent) -ForegroundColor White

# --- 4. Structure Output Data ---
$timestamp = [DateTime]::UtcNow.ToString("o")
$osInfo = (Get-CimInstance Win32_OperatingSystem).Caption
$cpuModel = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name

$benchmarkData = [ordered]@{
    timestamp = $timestamp
    environment = [ordered]@{
        os = $osInfo
        processor = $cpuModel
        processorCount = [Environment]::ProcessorCount
        executable = (Resolve-Path -LiteralPath $ExePath).Path
        fixture = (Resolve-Path -LiteralPath $ResolvedPdfPath).Path
    }
    metrics = [ordered]@{
        coldStartupMs = $coldStartupMs
        hotStartup = [ordered]@{
            p50 = $hotP50
            p95 = $hotP95
            avg = $hotAvg
            min = $hotMin
            max = $hotMax
            samples = $Runs
        }
        idle = [ordered]@{
            privateBytesMB = $idlePrivateBytesMB
            workingSetMB = $idleWorkingSetMB
            cpuPercent = $idleCpuPercent
        }
        documentLoaded = [ordered]@{
            privateBytesMB = $docPrivateBytesMB
            workingSetMB = $docWorkingSetMB
            idleCpuPercent = $docCpuPercent
        }
    }
}

$jsonOutput = $benchmarkData | ConvertTo-Json -Depth 5

$statusCold = if ($coldStartupMs -le 350) { "PASS" } else { "WARN" }
$statusHotP50 = if ($hotP50 -le 150) { "PASS" } else { "WARN" }
$statusHotP95 = if ($hotP95 -le 250) { "PASS" } else { "WARN" }
$statusIdlePrivate = if ($idlePrivateBytesMB -le 60) { "PASS" } else { "WARN" }
$statusIdleWS = if ($idleWorkingSetMB -le 150) { "PASS" } else { "WARN" }
$statusLoadedPrivate = if ($docPrivateBytesMB -le 80) { "PASS" } else { "WARN" }
$statusLoadedWS = if ($docWorkingSetMB -le 180) { "PASS" } else { "WARN" }
$statusIdleCpu = if ($idleCpuPercent -le 0.1) { "PASS" } else { "WARN" }

$markdownReport = @"
# BarePDF Performance Benchmarks

**Benchmark Run:** $timestamp  
**Environment:** $osInfo | $cpuModel  
**Fixture:** $([System.IO.Path]::GetFileName($ResolvedPdfPath))  

### Benchmark Summary

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency** | $coldStartupMs ms | <= 350 ms | $statusCold |
| **Hot Startup Latency (p50)** | $hotP50 ms | <= 150 ms | $statusHotP50 |
| **Hot Startup Latency (p95)** | $hotP95 ms | <= 250 ms | $statusHotP95 |
| **Idle Private Memory** | $idlePrivateBytesMB MB | <= 60 MB | $statusIdlePrivate |
| **Idle Working Set** | $idleWorkingSetMB MB | <= 150 MB | $statusIdleWS |
| **Loaded PDF Private Memory** | $docPrivateBytesMB MB | <= 80 MB | $statusLoadedPrivate |
| **Loaded PDF Working Set** | $docWorkingSetMB MB | <= 180 MB | $statusLoadedWS |
| **Idle Background CPU** | $idleCpuPercent% | <= 0.1% | $statusIdleCpu |

### Detailed Hot Startup Latency Runs ($Runs samples)
- **Min:** $hotMin ms
- **Average:** $hotAvg ms
- **Max:** $hotMax ms
- **p50:** $hotP50 ms
- **p95:** $hotP95 ms
"@

Write-Host "`n==========================================================" -ForegroundColor Cyan
Write-Host " BENCHMARK RESULTS (Markdown)" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host $markdownReport -ForegroundColor White

if ($MarkdownOutputPath) {
    Set-Content -LiteralPath $MarkdownOutputPath -Value $markdownReport -Encoding utf8
    Write-Host "`nMarkdown report saved to: $MarkdownOutputPath" -ForegroundColor Green
}

if ($JsonOutputPath) {
    Set-Content -LiteralPath $JsonOutputPath -Value $jsonOutput -Encoding utf8
    Write-Host "JSON report saved to: $JsonOutputPath" -ForegroundColor Green
}

return $benchmarkData
