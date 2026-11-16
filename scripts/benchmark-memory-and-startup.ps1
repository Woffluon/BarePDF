<#
.SYNOPSIS
    Automated benchmark measuring BarePDF startup latency, memory footprint across multiple states,
    and idle background CPU.

.DESCRIPTION
    Measures:
    1. Cold and hot startup latency (using BAREPDF_PROFILE_FILE first_bitmap_ms).
    2. True idle memory footprint: Private Bytes and Working Set (MB) + background CPU (%) with no open PDF.
    3. Welcome/single-document loaded memory footprint and idle CPU.
    4. Multi-page PDF loaded footprint, rapid-scroll peak, and post-scroll settled Private Bytes / Working Set (MB)
       after aggressive out-of-window LRU eviction.
    5. Outputs formatted results in Markdown and JSON suitable for docs/BENCHMARKS.md and docs/benchmarks/.

.PARAMETER ExecutablePath
    Path to barepdf.exe. Defaults to target/release/barepdf.exe or target/debug/barepdf.exe.

.PARAMETER PdfPath
    Path to the startup/single-document PDF fixture. Defaults to assets/barepdf-welcome.pdf.

.PARAMETER MultiPagePdfPath
    Optional path to a multi-page PDF fixture. When omitted, generates a deterministic 60-page PDF
    in %TEMP% and removes it on exit.

.PARAMETER MultiPageCount
    Number of pages to generate in the temporary multi-page fixture (when MultiPagePdfPath is omitted). Defaults to 60.

.PARAMETER ScrollSteps
    Number of rapid PageDown / scroll steps to execute during the rapid-scroll benchmark. Defaults to 30.

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
    [string]$MultiPagePdfPath = "",
    [ValidateRange(10, 500)]
    [int]$MultiPageCount = 60,
    [ValidateRange(5, 200)]
    [int]$ScrollSteps = 30,
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
Add-Type -AssemblyName UIAutomationClient
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
            Write-Error "Failed to build target/release/barepdf.exe. Run 'cargo build --release --locked -p barepdf' first."
            exit 1
        }
        $ExecutablePath = $releaseExe
    }
}
$ExePath = (Resolve-Path -LiteralPath $ExecutablePath).Path

# Guard: a release binary requires a sibling pdfium.dll (no search-path fallback in release builds).
$PdfiumSibling = Join-Path (Split-Path -Parent $ExePath) "pdfium.dll"
if (-not (Test-Path -LiteralPath $PdfiumSibling -PathType Leaf)) {
    Write-Error "Missing sibling pdfium.dll next to '$ExePath'. Run 'packaging/windows/scripts/fetch-pdfium.ps1' to stage the pinned PDFium runtime."
    exit 1
}

# 2. Resolve Sample PDF
if (-not $PdfPath) {
    $PdfPath = Join-Path $repoRoot "assets\barepdf-welcome.pdf"
}
if (-not (Test-Path -LiteralPath $PdfPath -PathType Leaf)) {
    Write-Error "Sample PDF fixture not found at '$PdfPath'."
    exit 1
}
$ResolvedPdfPath = (Resolve-Path -LiteralPath $PdfPath).Path

Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;

public static class BarePdfBenchWin32 {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct POINT {
        public int X;
        public int Y;
    }

    public const uint WM_MOUSEMOVE = 0x0200;
    public const uint WM_LBUTTONDOWN = 0x0201;
    public const uint WM_LBUTTONUP = 0x0202;
    public const uint WM_MOUSEWHEEL = 0x020A;

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

    [DllImport("user32.dll")]
    public static extern bool ScreenToClient(IntPtr hWnd, ref POINT lpPoint);
}
"@ -ErrorAction Stop

function New-BenchmarkAppData {
    param(
        [bool]$WelcomeSeen = $true,
        [bool]$SidebarVisible = $true
    )

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
        sidebar_visible = $SidebarVisible
        welcome_manifesto_seen = $WelcomeSeen
        window_maximized = $false
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

function New-MultiPageBenchmarkPdf {
    param(
        [string]$OutputPath,
        [int]$PageCount = 60
    )

    $ms = New-Object System.IO.MemoryStream
    $ascii = [System.Text.Encoding]::ASCII
    $offsets = New-Object 'System.Collections.Generic.List[long]'

    $writeAscii = {
        param([string]$Text)
        $bytes = $ascii.GetBytes($Text)
        $ms.Write($bytes, 0, $bytes.Length)
    }

    & $writeAscii "%PDF-1.4`n"

    # Object 1: Catalog, Object 2: Pages, Object 3: Helvetica, Object 4: Helvetica-Bold
    $pageObjectIds = New-Object 'System.Collections.Generic.List[int]'
    for ($i = 0; $i -lt $PageCount; $i++) {
        $pageObjectIds.Add(5 + ($i * 2))
    }
    $kidsList = ($pageObjectIds | ForEach-Object { "$_ 0 R" }) -join " "

    $offsets.Add($ms.Position)
    & $writeAscii "1 0 obj`n<< /Type /Catalog /Pages 2 0 R >>`nendobj`n"

    $offsets.Add($ms.Position)
    & $writeAscii "2 0 obj`n<< /Type /Pages /Kids [ $kidsList ] /Count $PageCount >>`nendobj`n"

    $offsets.Add($ms.Position)
    & $writeAscii "3 0 obj`n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>`nendobj`n"

    $offsets.Add($ms.Position)
    & $writeAscii "4 0 obj`n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold >>`nendobj`n"

    for ($i = 0; $i -lt $PageCount; $i++) {
        $pageNum = $i + 1
        $pageObjId = 5 + ($i * 2)
        $streamObjId = $pageObjId + 1

        $sb = New-Object System.Text.StringBuilder
        [void]$sb.AppendLine("0.11 0.13 0.16 rg 42 752 511 42 re f")
        [void]$sb.AppendLine("0.80 0.83 0.87 RG 0.75 w 42 54 511 740 re S")
        [void]$sb.AppendLine("0.92 0.94 0.97 RG 0.50 w 42 690 m 553 690 l S")
        [void]$sb.AppendLine("BT")
        [void]$sb.AppendLine("1 1 1 rg /F2 13 Tf 56 768 Td (BarePDF Multi-Page Benchmark Fixture - Page $pageNum of $PageCount) Tj")
        [void]$sb.AppendLine("0.12 0.14 0.18 rg /F2 11 Tf 0 -46 Td (Section $($pageNum).1 Memory & LRU Eviction Telemetry Verification) Tj")
        [void]$sb.AppendLine("/F1 9.5 Tf 0 -22 Td")
        for ($line = 1; $line -le 24; $line++) {
            $yStep = if ($line -eq 1) { 0 } else { -22 }
            [void]$sb.AppendLine("0 $yStep Td (Page $pageNum row ${line}: Deterministic vector and glyph rasterization payload for 60fps scroll testing.) Tj")
        }
        [void]$sb.AppendLine("0 -30 Td (Footer Index: P$pageNum / $PageCount | BarePDF Release Harness) Tj")
        [void]$sb.AppendLine("ET")

        $streamText = $sb.ToString()
        $streamLen = $ascii.GetByteCount($streamText)

        $offsets.Add($ms.Position)
        & $writeAscii "$pageObjId 0 obj`n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> /Contents $streamObjId 0 R >>`nendobj`n"

        $offsets.Add($ms.Position)
        & $writeAscii "$streamObjId 0 obj`n<< /Length $streamLen >>`nstream`n$streamText`nendstream`nendobj`n"
    }

    $xrefOffset = $ms.Position
    $totalObjects = $offsets.Count + 1
    & $writeAscii "xref`n0 $totalObjects`n0000000000 65535 f `n"
    foreach ($offset in $offsets) {
        $line = "{0:D10} 00000 n `n" -f $offset
        & $writeAscii $line
    }
    & $writeAscii "trailer`n<< /Size $totalObjects /Root 1 0 R >>`nstartxref`n$xrefOffset`n%%EOF`n"

    [System.IO.File]::WriteAllBytes($OutputPath, $ms.ToArray())
    $ms.Dispose()
}

function Wait-MainWindowHandle {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$TimeoutMs = 8000
    )

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMs)
    while ([DateTime]::UtcNow -lt $deadline) {
        $Process.Refresh()
        if ($Process.HasExited) {
            return [IntPtr]::Zero
        }
        if ($Process.MainWindowHandle -ne [IntPtr]::Zero -and -not [string]::IsNullOrEmpty($Process.MainWindowTitle)) {
            return $Process.MainWindowHandle
        }
        Start-Sleep -Milliseconds 50
    }
    $Process.Refresh()
    return $Process.MainWindowHandle
}

function Measure-SingleStartup {
    param (
        [string]$BinaryPath,
        [string]$ArgumentPath
    )

    $tempProfile = [System.IO.Path]::GetTempFileName()
    $benchAppData = New-BenchmarkAppData -WelcomeSeen $true
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
                } catch {}
            }
            if ($process.HasExited) {
                break
            }
        }

        if ($null -eq $recordedMs) {
            throw "Startup profile signal ('first_bitmap_ms') was not written to '$tempProfile' within 15 seconds."
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

function Measure-ProcessResourceWindow {
    param(
        [System.Diagnostics.Process]$Process,
        [int]$SampleSeconds
    )

    $Process.Refresh()
    $startCpu = $Process.TotalProcessorTime.TotalMilliseconds
    $startStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()
    Start-Sleep -Seconds $SampleSeconds
    $Process.Refresh()
    $endCpu = $Process.TotalProcessorTime.TotalMilliseconds
    $endStamp = [System.Diagnostics.Stopwatch]::GetTimestamp()

    $elapsedMs = ($endStamp - $startStamp) / [System.Diagnostics.Stopwatch]::Frequency * 1000.0
    $cpuDeltaMs = [math]::Max(0.0, ($endCpu - $startCpu))
    $cpuPercent = [math]::Round(($cpuDeltaMs / ($elapsedMs * [Environment]::ProcessorCount)) * 100.0, 2)
    return [PSCustomObject]@{
        PrivateBytesMB = [math]::Round($Process.PrivateMemorySize64 / 1MB, 2)
        WorkingSetMB = [math]::Round($Process.WorkingSet64 / 1MB, 2)
        CpuPercent = $cpuPercent
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

$generatedTempPdf = $null
if (-not $MultiPagePdfPath) {
    $generatedTempPdf = Join-Path ([System.IO.Path]::GetTempPath()) ("barepdf-bench-multipage-" + [guid]::NewGuid().ToString("N") + ".pdf")
    New-MultiPageBenchmarkPdf -OutputPath $generatedTempPdf -PageCount $MultiPageCount
    $ResolvedMultiPagePdfPath = $generatedTempPdf
    $MeasuredMultiPageCount = $MultiPageCount
} else {
    if (-not (Test-Path -LiteralPath $MultiPagePdfPath -PathType Leaf)) {
        Write-Error "Multi-page PDF fixture not found at '$MultiPagePdfPath'."
        exit 1
    }
    $ResolvedMultiPagePdfPath = (Resolve-Path -LiteralPath $MultiPagePdfPath).Path
    $MeasuredMultiPageCount = $MultiPageCount
}

try {
    Write-Host "==========================================================" -ForegroundColor Cyan
    Write-Host " BarePDF Performance & Resource Benchmark" -ForegroundColor Cyan
    Write-Host " Executable  : $ExePath" -ForegroundColor Gray
    Write-Host " Sample PDF  : $ResolvedPdfPath" -ForegroundColor Gray
    Write-Host " Multi-Page  : $ResolvedMultiPagePdfPath ($MeasuredMultiPageCount pages)" -ForegroundColor Gray
    Write-Host " Iterations  : $Runs hot runs, ${IdleSeconds}s idle, $ScrollSteps scroll steps" -ForegroundColor Gray
    Write-Host "==========================================================" -ForegroundColor Cyan

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

    # --- 2. True Idle Memory & Background CPU Benchmark (no document opened) ---
    Write-Host "`nMeasuring True Idle Resource Utilization (${IdleSeconds}s duration)..." -ForegroundColor Green
    $idleAppData = New-BenchmarkAppData -WelcomeSeen $true
    $prevAppData = [Environment]::GetEnvironmentVariable("APPDATA", "Process")
    [Environment]::SetEnvironmentVariable("APPDATA", $idleAppData.Root, "Process")

    $idleProc = $null
    try {
        $idleProc = Start-Process -FilePath $ExePath -PassThru -WindowStyle Normal
        $null = Wait-MainWindowHandle -Process $idleProc
        Start-Sleep -Milliseconds 2000
        $idleSample = Measure-ProcessResourceWindow -Process $idleProc -SampleSeconds $IdleSeconds
        $idlePrivateBytesMB = $idleSample.PrivateBytesMB
        $idleWorkingSetMB = $idleSample.WorkingSetMB
        $idleCpuPercent = $idleSample.CpuPercent
    } finally {
        Stop-BarePdfProcess $idleProc
        [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
        Remove-BenchmarkAppData $idleAppData.Root
    }

    Write-Host ("  Idle Private Bytes : {0} MB" -f $idlePrivateBytesMB) -ForegroundColor White
    Write-Host ("  Idle Working Set   : {0} MB" -f $idleWorkingSetMB) -ForegroundColor White
    Write-Host ("  Idle Background CPU: {0:N2}%" -f $idleCpuPercent) -ForegroundColor White

    # --- 3. Welcome / Single Document Loaded Memory & CPU Benchmark ---
    Write-Host "`nMeasuring Document Loaded Resource Utilization ($([System.IO.Path]::GetFileName($ResolvedPdfPath)))..." -ForegroundColor Green
    $docAppData = New-BenchmarkAppData -WelcomeSeen $true
    [Environment]::SetEnvironmentVariable("APPDATA", $docAppData.Root, "Process")

    $docProc = $null
    try {
        $docProc = Start-Process -FilePath $ExePath -ArgumentList "`"$ResolvedPdfPath`"" -PassThru -WindowStyle Normal
        $null = Wait-MainWindowHandle -Process $docProc
        Start-Sleep -Milliseconds 2500
        $sampleDuration = [int]([math]::Max(2, [int]($IdleSeconds / 2)))
        $docSample = Measure-ProcessResourceWindow -Process $docProc -SampleSeconds $sampleDuration
        $docPrivateBytesMB = $docSample.PrivateBytesMB
        $docWorkingSetMB = $docSample.WorkingSetMB
        $docCpuPercent = $docSample.CpuPercent
    } finally {
        Stop-BarePdfProcess $docProc
        [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
        Remove-BenchmarkAppData $docAppData.Root
    }

    Write-Host ("  Loaded Private Bytes : {0} MB" -f $docPrivateBytesMB) -ForegroundColor White
    Write-Host ("  Loaded Working Set   : {0} MB" -f $docWorkingSetMB) -ForegroundColor White
    Write-Host ("  Loaded Idle CPU      : {0:N2}%" -f $docCpuPercent) -ForegroundColor White

    # --- 4. Multi-Page PDF Loaded & Rapid Scroll Post-Eviction Benchmark ---
    Write-Host "`nMeasuring Multi-Page PDF & Rapid Scroll Resource Utilization ($MeasuredMultiPageCount pages, $ScrollSteps steps)..." -ForegroundColor Green
    $multiAppData = New-BenchmarkAppData -WelcomeSeen $true
    [Environment]::SetEnvironmentVariable("APPDATA", $multiAppData.Root, "Process")

    $multiProc = $null
    try {
        $multiProc = Start-Process -FilePath $ExePath -ArgumentList "`"$ResolvedMultiPagePdfPath`"" -PassThru -WindowStyle Normal
        $hwnd = Wait-MainWindowHandle -Process $multiProc
        Start-Sleep -Milliseconds 2500

        $multiSampleDuration = [int]([math]::Max(2, [int]($IdleSeconds / 2)))
        $multiLoadedSample = Measure-ProcessResourceWindow -Process $multiProc -SampleSeconds $multiSampleDuration
        $multiPrivateBytesMB = $multiLoadedSample.PrivateBytesMB
        $multiWorkingSetMB = $multiLoadedSample.WorkingSetMB
        $multiCpuPercent = $multiLoadedSample.CpuPercent

        # Rapid scroll phase: drive real page navigation + viewport wheel scrolling via UIA coordinates & PostMessageW
        $peakPrivateBytesMB = $multiPrivateBytesMB
        $peakWorkingSetMB = $multiWorkingSetMB
        $multiProc.Refresh()
        $hwnd = $multiProc.MainWindowHandle
        $scrollStartCpuMs = $multiProc.TotalProcessorTime.TotalMilliseconds
        $scrollStartPage = $null
        $scrollEndPage = $null

        if ($hwnd -ne [IntPtr]::Zero) {
            $uiaRoot = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
            $uiaItems = $uiaRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
            if ($uiaItems.Count -lt 5) {
                Start-Sleep -Milliseconds 250
                $uiaItems = $uiaRoot.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
            }
            $pageEdit = $null
            $nextBtn = $null
            $seenPageEdit = $false
            foreach ($item in $uiaItems) {
                $ct = $item.Current.ControlType
                if ($null -eq $pageEdit -and $ct -eq [System.Windows.Automation.ControlType]::Edit) {
                    $pageEdit = $item
                    $seenPageEdit = $true
                    continue
                }
                if ($ct -eq [System.Windows.Automation.ControlType]::Button) {
                    $name = $item.Current.Name
                    if ($name -eq "Sonraki sayfa" -or $name -eq "Next page" -or ($seenPageEdit -and $null -eq $nextBtn)) {
                        $nextBtn = $item
                    }
                }
            }
            if ($null -ne $pageEdit) {
                try { $scrollStartPage = [int]($pageEdit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value) } catch {}
            }

            $btnLParam = [IntPtr]::Zero
            if ($null -ne $nextBtn) {
                $r = $nextBtn.Current.BoundingRectangle
                if ($r.Width -gt 0 -and $r.Height -gt 0) {
                    $pt = New-Object BarePdfBenchWin32+POINT
                    $pt.X = [int]($r.Left + $r.Width / 2.0)
                    $pt.Y = [int]($r.Top + $r.Height / 2.0)
                    [void][BarePdfBenchWin32]::ScreenToClient($hwnd, [ref]$pt)
                    $btnLParam = [IntPtr]((($pt.Y -band 0xFFFF) -shl 16) -bor ($pt.X -band 0xFFFF))
                }
            }

            $rect = New-Object BarePdfBenchWin32+RECT
            [void][BarePdfBenchWin32]::GetWindowRect($hwnd, [ref]$rect)
            $centerX = [int](($rect.Left + $rect.Right) / 2)
            $centerY = [int](($rect.Top + $rect.Bottom) / 2)
            $cpt = New-Object BarePdfBenchWin32+POINT
            $cpt.X = $centerX
            $cpt.Y = $centerY
            [void][BarePdfBenchWin32]::ScreenToClient($hwnd, [ref]$cpt)
            $clientCenterLParam = [IntPtr]((($cpt.Y -band 0xFFFF) -shl 16) -bor ($cpt.X -band 0xFFFF))
            $wheelCoords = [IntPtr]((($centerY -band 0xFFFF) -shl 16) -bor ($centerX -band 0xFFFF))
            $wheelWParam = [IntPtr](((-360 -band 0xFFFF) -shl 16))

            for ($step = 0; $step -lt $ScrollSteps; $step++) {
                if ($btnLParam -ne [IntPtr]::Zero) {
                    [void][BarePdfBenchWin32]::PostMessageW($hwnd, [BarePdfBenchWin32]::WM_MOUSEMOVE, [IntPtr]::Zero, $btnLParam)
                    [void][BarePdfBenchWin32]::PostMessageW($hwnd, [BarePdfBenchWin32]::WM_LBUTTONDOWN, [IntPtr]1, $btnLParam)
                    [void][BarePdfBenchWin32]::PostMessageW($hwnd, [BarePdfBenchWin32]::WM_LBUTTONUP, [IntPtr]::Zero, $btnLParam)
                }
                [void][BarePdfBenchWin32]::PostMessageW($hwnd, [BarePdfBenchWin32]::WM_MOUSEMOVE, [IntPtr]::Zero, $clientCenterLParam)
                [void][BarePdfBenchWin32]::PostMessageW($hwnd, [BarePdfBenchWin32]::WM_MOUSEWHEEL, $wheelWParam, $wheelCoords)
                Start-Sleep -Milliseconds 45
                if (($step % 5) -eq 4) {
                    $multiProc.Refresh()
                    $curPriv = [math]::Round($multiProc.PrivateMemorySize64 / 1MB, 2)
                    $curWs = [math]::Round($multiProc.WorkingSet64 / 1MB, 2)
                    if ($curPriv -gt $peakPrivateBytesMB) { $peakPrivateBytesMB = $curPriv }
                    if ($curWs -gt $peakWorkingSetMB) { $peakWorkingSetMB = $curWs }
                }
            }

            Start-Sleep -Milliseconds 250
            if ($null -ne $pageEdit) {
                try { $scrollEndPage = [int]($pageEdit.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern).Current.Value) } catch {}
            }
        }

        $multiProc.Refresh()
        $scrollCpuDeltaMs = [math]::Round([math]::Max(0.0, ($multiProc.TotalProcessorTime.TotalMilliseconds - $scrollStartCpuMs)), 2)

        # Wait past SCROLL_IDLE_DELAY (180ms) + worker TrimCache settle before measuring post-scroll memory
        Start-Sleep -Milliseconds 1500
        $postScrollSample = Measure-ProcessResourceWindow -Process $multiProc -SampleSeconds $multiSampleDuration
        $afterScrollPrivateBytesMB = $postScrollSample.PrivateBytesMB
        $afterScrollWorkingSetMB = $postScrollSample.WorkingSetMB
        $afterScrollCpuPercent = $postScrollSample.CpuPercent
        $multiProc.Refresh()
        $curPrivAfter = [math]::Round($multiProc.PrivateMemorySize64 / 1MB, 2)
        if ($curPrivAfter -gt $peakPrivateBytesMB) { $peakPrivateBytesMB = $curPrivAfter }
    } finally {
        Stop-BarePdfProcess $multiProc
        [Environment]::SetEnvironmentVariable("APPDATA", $prevAppData, "Process")
        Remove-BenchmarkAppData $multiAppData.Root
    }

    Write-Host ("  Multi-Page Loaded Private Bytes : {0} MB" -f $multiPrivateBytesMB) -ForegroundColor White
    Write-Host ("  Multi-Page Loaded Working Set   : {0} MB" -f $multiWorkingSetMB) -ForegroundColor White
    Write-Host ("  Rapid Scroll Pages              : {0} -> {1} (CPU delta: {2} ms)" -f $scrollStartPage, $scrollEndPage, $scrollCpuDeltaMs) -ForegroundColor White
    Write-Host ("  Rapid Scroll Peak Private Bytes : {0} MB (Peak WS: {1} MB)" -f $peakPrivateBytesMB, $peakWorkingSetMB) -ForegroundColor White
    Write-Host ("  After Rapid Scroll Private Bytes: {0} MB" -f $afterScrollPrivateBytesMB) -ForegroundColor White
    Write-Host ("  After Rapid Scroll Working Set  : {0} MB" -f $afterScrollWorkingSetMB) -ForegroundColor White
    Write-Host ("  After Rapid Scroll Idle CPU     : {0:N2}%" -f $afterScrollCpuPercent) -ForegroundColor White

    # --- 5. Structure Output Data ---
    $timestamp = [DateTime]::UtcNow.ToString("o")
    $osInfo = (Get-CimInstance Win32_OperatingSystem).Caption
    $cpuModel = ((Get-CimInstance Win32_Processor | Select-Object -First 1).Name).Trim()
    $ramBytes = (Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory
    $totalRamMB = [math]::Round($ramBytes / 1MB, 0)

    $benchmarkData = [ordered]@{
        timestamp = $timestamp
        environment = [ordered]@{
            os = $osInfo
            processor = $cpuModel
            processorCount = [Environment]::ProcessorCount
            totalPhysicalRamMB = $totalRamMB
            executable = $ExePath
            fixture = $ResolvedPdfPath
            multiPageFixturePages = $MeasuredMultiPageCount
            scrollSteps = $ScrollSteps
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
            multiPageLoaded = [ordered]@{
                pages = $MeasuredMultiPageCount
                privateBytesMB = $multiPrivateBytesMB
                workingSetMB = $multiWorkingSetMB
                idleCpuPercent = $multiCpuPercent
            }
            afterRapidScroll = [ordered]@{
                scrollSteps = $ScrollSteps
                startPage = $scrollStartPage
                endPage = $scrollEndPage
                scrollCpuMs = $scrollCpuDeltaMs
                peakPrivateBytesMB = $peakPrivateBytesMB
                peakWorkingSetMB = $peakWorkingSetMB
                privateBytesMB = $afterScrollPrivateBytesMB
                workingSetMB = $afterScrollWorkingSetMB
                idleCpuPercent = $afterScrollCpuPercent
            }
        }
    }

    $jsonOutput = $benchmarkData | ConvertTo-Json -Depth 6

    $statusCold = if ($coldStartupMs -le 350) { "PASS" } else { "WARN" }
    $statusHotP50 = if ($hotP50 -le 150) { "PASS" } else { "WARN" }
    $statusHotP95 = if ($hotP95 -le 250) { "PASS" } else { "WARN" }
    $statusIdlePrivate = if ($idlePrivateBytesMB -le 60) { "PASS" } else { "WARN" }
    $statusIdleWS = if ($idleWorkingSetMB -le 150) { "PASS" } else { "WARN" }
    $statusLoadedPrivate = if ($docPrivateBytesMB -le 80) { "PASS" } else { "WARN" }
    $statusLoadedWS = if ($docWorkingSetMB -le 180) { "PASS" } else { "WARN" }
    $statusMultiPrivate = if ($multiPrivateBytesMB -le 95) { "PASS" } else { "WARN" }
    $statusMultiWS = if ($multiWorkingSetMB -le 180) { "PASS" } else { "WARN" }
    $statusScrollPrivate = if ($afterScrollPrivateBytesMB -le 110) { "PASS" } else { "WARN" }
    $statusScrollWS = if ($afterScrollWorkingSetMB -le 190) { "PASS" } else { "WARN" }
    $statusIdleCpu = if ($idleCpuPercent -le 0.1) { "PASS" } else { "WARN" }

    $markdownReport = @"
# BarePDF Performance Benchmarks

**Benchmark Run:** $timestamp  
**Environment:** $osInfo | $cpuModel (${totalRamMB} MB RAM)  
**Fixture:** $([System.IO.Path]::GetFileName($ResolvedPdfPath)) | Multi-Page Fixture ($MeasuredMultiPageCount pages)  

### Benchmark Summary

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency** | $coldStartupMs ms | <= 350 ms | $statusCold |
| **Hot Startup Latency (p50)** | $hotP50 ms | <= 150 ms | $statusHotP50 |
| **Hot Startup Latency (p95)** | $hotP95 ms | <= 250 ms | $statusHotP95 |
| **Idle Private Memory (Empty)** | $idlePrivateBytesMB MB | <= 60 MB | $statusIdlePrivate |
| **Idle Working Set (Empty)** | $idleWorkingSetMB MB | <= 150 MB | $statusIdleWS |
| **Loaded PDF Private Memory** | $docPrivateBytesMB MB | <= 80 MB | $statusLoadedPrivate |
| **Loaded PDF Working Set** | $docWorkingSetMB MB | <= 180 MB | $statusLoadedWS |
| **Multi-Page PDF ($MeasuredMultiPageCount p) Private Memory** | $multiPrivateBytesMB MB | <= 95 MB | $statusMultiPrivate |
| **Multi-Page PDF ($MeasuredMultiPageCount p) Working Set** | $multiWorkingSetMB MB | <= 180 MB | $statusMultiWS |
| **After Rapid Scroll ($ScrollSteps steps) Private Memory** | $afterScrollPrivateBytesMB MB | <= 110 MB | $statusScrollPrivate |
| **After Rapid Scroll ($ScrollSteps steps) Working Set** | $afterScrollWorkingSetMB MB | <= 190 MB | $statusScrollWS |
| **Idle Background CPU** | $idleCpuPercent% | <= 0.1% | $statusIdleCpu |

### Detailed Hot Startup Latency & Scroll Telemetry
- **Hot Startup ($Runs samples):** min $hotMin ms | avg $hotAvg ms | max $hotMax ms | p50 $hotP50 ms | p95 $hotP95 ms
- **Rapid Scroll Peak:** $peakPrivateBytesMB MB Private Bytes / $peakWorkingSetMB MB Working Set (pages $scrollStartPage -> $scrollEndPage, scroll CPU delta: $scrollCpuDeltaMs ms, settled CPU: $afterScrollCpuPercent%)
"@

    Write-Host "`n==========================================================" -ForegroundColor Cyan
    Write-Host " BENCHMARK RESULTS (Markdown)" -ForegroundColor Cyan
    Write-Host "==========================================================" -ForegroundColor Cyan
    Write-Host $markdownReport -ForegroundColor White

    if ($MarkdownOutputPath) {
        $mdParent = Split-Path -Parent $MarkdownOutputPath
        if ($mdParent) { New-Item -ItemType Directory -Path $mdParent -Force | Out-Null }
        Set-Content -LiteralPath $MarkdownOutputPath -Value $markdownReport -Encoding utf8
        Write-Host "`nMarkdown report saved to: $MarkdownOutputPath" -ForegroundColor Green
    }

    if ($JsonOutputPath) {
        $jsonParent = Split-Path -Parent $JsonOutputPath
        if ($jsonParent) { New-Item -ItemType Directory -Path $jsonParent -Force | Out-Null }
        Set-Content -LiteralPath $JsonOutputPath -Value $jsonOutput -Encoding utf8
        Write-Host "JSON report saved to: $JsonOutputPath" -ForegroundColor Green
    }

    return $benchmarkData
} finally {
    if ($generatedTempPdf -and (Test-Path -LiteralPath $generatedTempPdf -PathType Leaf)) {
        Remove-Item -LiteralPath $generatedTempPdf -Force -ErrorAction SilentlyContinue
    }
}
