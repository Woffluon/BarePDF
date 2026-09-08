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
    [int]$Runs = 5,
    [ValidateSet("Efficient", "Enhanced")]
    [string]$VisualMode = "Efficient",
    [string]$ExecutablePath = "",
    [string]$BaselinePath = "",
    [Alias("OutputPath")]
    [string]$ResultPath = ""
)

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $ExecutablePath) {
    $ExecutablePath = Join-Path $repoRoot "target\release\barepdf.exe"
}
$ExePath = Resolve-Path -LiteralPath $ExecutablePath -ErrorAction SilentlyContinue
if (-not $ExePath) {
    Write-Error "Release executable not found: $ExecutablePath. Run 'cargo build --release --locked' first."
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

function Get-MetricSummary([double[]]$Values) {
    if ($Values.Count -eq 0) { return $null }
    return [PSCustomObject][ordered]@{
        samples = $Values.Count
        p50 = [math]::Round((Get-Percentile $Values 50), 2)
        p95 = [math]::Round((Get-Percentile $Values 95), 2)
    }
}

function Write-MetricSummary([string]$Name, $Summary, [string]$Unit) {
    if ($null -eq $Summary) {
        Write-Host ("{0,-36} UNSUPPORTED" -f $Name)
        return
    }
    Write-Host ("{0,-36} p50={1} {3}; p95={2} {3} ({4} runs)" -f $Name, $Summary.p50, $Summary.p95, $Unit, $Summary.samples)
}

function Get-PackageSize([string]$Label, [string]$Directory, [string]$Filter) {
    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
        return [PSCustomObject]@{ Label = $Label; Bytes = $null; Reason = "package directory is absent" }
    }
    $matches = @(Get-ChildItem -LiteralPath $Directory -Filter $Filter -File -ErrorAction SilentlyContinue)
    if ($matches.Count -ne 1) {
        return [PSCustomObject]@{ Label = $Label; Bytes = $null; Reason = "expected exactly one matching package, found $($matches.Count)" }
    }
    return [PSCustomObject]@{ Label = $Label; Bytes = [int64]$matches[0].Length; Reason = $null }
}

function Get-CommandText([string]$Command, [string[]]$Arguments, [switch]$AllowEmpty) {
    try {
        $output = & $Command @Arguments 2>$null
        if ($LASTEXITCODE -ne 0) { return "Unavailable" }
        $text = ($output | Out-String).Trim()
        if (-not $AllowEmpty -and [string]::IsNullOrWhiteSpace($text)) { return "Unavailable" }
        return $text
    } catch {
        return "Unavailable"
    }
}

function Get-RustcField([string]$Metadata, [string]$Field) {
    if ($Metadata -eq "Unavailable") { return "Unavailable" }
    $match = [regex]::Match($Metadata, "(?m)^$([regex]::Escape($Field)):\s*(.+?)\s*$")
    if ($match.Success) { return $match.Groups[1].Value }
    return "Unavailable"
}

function Get-FileSha256([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return "Unavailable" }
    try {
        return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash.ToLowerInvariant()
    } catch {
        return "Unavailable"
    }
}

function Get-EnvironmentValue([string]$Name) {
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ($null -eq $value) { return "<unset>" }
    return $value
}

function New-ProfileAppData([string]$Mode) {
    $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
    $directory = Join-Path $tempRoot ("barepdf-profile-appdata-" + [guid]::NewGuid().ToString("N"))
    $configDirectory = Join-Path $directory "BarePDF"
    New-Item -ItemType Directory -Path $configDirectory -Force -ErrorAction Stop | Out-Null
    $configPath = Join-Path $configDirectory "config.json"
    $config = [ordered]@{
        theme = "Dark"
        update_checks_enabled = $false
        last_window_width = 1260
        last_window_height = 926
        enhanced_ui = ($Mode -eq "Enhanced")
    }
    [IO.File]::WriteAllText($configPath, ($config | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
    [PSCustomObject]@{ Root = $directory; Config = $configPath }
}

function Test-VerifiedProfileAppData([string]$Directory) {
    try {
        $fullDirectory = [IO.Path]::GetFullPath($Directory).TrimEnd('\')
        $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
        $parent = [IO.Directory]::GetParent($fullDirectory)
        $item = Get-Item -LiteralPath $fullDirectory -Force -ErrorAction Stop
        return $null -ne $parent -and
            $parent.FullName.TrimEnd('\') -eq $tempRoot -and
            [IO.Path]::GetFileName($fullDirectory) -match '^barepdf-profile-appdata-[0-9a-f]{32}$' -and
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0
    } catch {
        return $false
    }
}

function Remove-ProfileAppData([string]$Directory) {
    if (Test-VerifiedProfileAppData $Directory) {
        Remove-Item -LiteralPath $Directory -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Get-GpuCountersForProcess([int]$ProcessId) {
    $engineValues = @()
    $enginePath = '\GPU Engine(*)\Utilization Percentage'
    try {
        $engineSamples = @(Get-Counter -Counter $enginePath -ErrorAction Stop).CounterSamples |
            Where-Object { $_.InstanceName -match "(?i)(^|_)pid_$ProcessId(?:_|$)" -or $_.Path -match "(?i)(^|_)pid_$ProcessId(?:_|$)" }
        $engineValues = @($engineSamples | ForEach-Object { [double]$_.CookedValue } |
            Where-Object { -not [double]::IsNaN($_) -and -not [double]::IsInfinity($_) })
    } catch {
        $engineValues = @()
    }

    $memoryValues = @()
    $memoryPaths = @(
        '\GPU Process Memory(*)\Dedicated Usage',
        '\GPU Process Memory(*)\Shared Usage'
    )
    $usableMemoryPaths = @()
    foreach ($memoryPath in $memoryPaths) {
        try {
            $memorySamples = @(Get-Counter -Counter $memoryPath -ErrorAction Stop).CounterSamples |
                Where-Object { $_.InstanceName -match "(?i)(^|_)pid_$ProcessId(?:_|$)" -or $_.Path -match "(?i)(^|_)pid_$ProcessId(?:_|$)" }
            $values = @($memorySamples | ForEach-Object { [double]$_.CookedValue } |
                Where-Object { -not [double]::IsNaN($_) -and -not [double]::IsInfinity($_) })
            if ($values.Count -gt 0) {
                $memoryValues += $values
                $usableMemoryPaths += $memoryPath
            }
        } catch {
        }
    }

    [PSCustomObject]@{
        EngineUtilizationPercent = if ($engineValues.Count -gt 0) { ($engineValues | Measure-Object -Sum).Sum } else { $null }
        ProcessMemoryMiB = if ($memoryValues.Count -gt 0) { (($memoryValues | Measure-Object -Sum).Sum / 1MB) } else { $null }
        EngineSupported = ($engineValues.Count -gt 0)
        ProcessMemorySupported = ($memoryValues.Count -gt 0)
        EnginePath = if ($engineValues.Count -gt 0) { $enginePath } else { $null }
        MemoryPaths = $usableMemoryPaths
    }
}

function Get-BuildProvenance([string]$Executable) {
    $rustcVv = Get-CommandText -Command "rustc" -Arguments @("-Vv")
    $cargoVersion = Get-CommandText -Command "cargo" -Arguments @("-V")
    $sourceCommit = Get-CommandText -Command "git" -Arguments @("rev-parse", "HEAD")
    $sourceStatus = Get-CommandText -Command "git" -Arguments @("status", "--porcelain") -AllowEmpty
    $rustcHost = Get-RustcField $rustcVv "host"
    $configuredTarget = [Environment]::GetEnvironmentVariable("CARGO_BUILD_TARGET")
    $targetTriple = if ([string]::IsNullOrWhiteSpace($configuredTarget)) { $rustcHost } else { $configuredTarget }
    $executableItem = Get-Item -LiteralPath $Executable -ErrorAction SilentlyContinue

    [PSCustomObject][ordered]@{
        executable = [PSCustomObject][ordered]@{
            sha256 = Get-FileSha256 $Executable
            bytes = if ($executableItem) { [int64]$executableItem.Length } else { "Unavailable" }
        }
        cargo_lock_sha256 = Get-FileSha256 (Join-Path $repoRoot "Cargo.lock")
        toolchain = [PSCustomObject][ordered]@{
            rustc_vv = $rustcVv
            rustc_binary = Get-RustcField $rustcVv "binary"
            rustc_commit_hash = Get-RustcField $rustcVv "commit-hash"
            rustc_host = $rustcHost
            rustc_release = Get-RustcField $rustcVv "release"
            cargo_version = $cargoVersion
            target_triple = $targetTriple
        }
        flags = [PSCustomObject][ordered]@{
            rustflags = Get-EnvironmentValue "RUSTFLAGS"
            cargo_encoded_rustflags = Get-EnvironmentValue "CARGO_ENCODED_RUSTFLAGS"
        }
        source = [PSCustomObject][ordered]@{
            commit = $sourceCommit
            dirty = if ($sourceStatus -eq "Unavailable") { "Unavailable" } elseif ([string]::IsNullOrWhiteSpace($sourceStatus)) { "clean" } else { "dirty" }
        }
    }
}

function Get-RecordValue($Record, [string]$Path) {
    $value = $Record
    foreach ($segment in $Path.Split(".")) {
        if ($null -eq $value) { return $null }
        $property = $value.PSObject.Properties[$segment]
        if ($null -eq $property) { return $null }
        $value = $property.Value
    }
    return $value
}

function Get-BaselineCompatibility($Baseline, $Current) {
    $reasons = @()
    if ($null -eq $Baseline -or $Baseline.schema_version -ne 2) {
        return [PSCustomObject]@{ Comparable = $false; Reasons = @("baseline schema_version must be 2; legacy results are not comparable") }
    }
    if ($Baseline.fixture.sha256 -ne $Current.fixture.sha256) { $reasons += "fixture SHA-256 differs" }
    if ($Baseline.fixture.bytes -ne $Current.fixture.bytes) { $reasons += "fixture byte size differs" }
    if ($Baseline.measurement.release_profile -ne $Current.measurement.release_profile) { $reasons += "release profile differs" }
    if ($Baseline.measurement.duration_seconds -ne $Current.measurement.duration_seconds) { $reasons += "duration differs" }
    if ($Baseline.measurement.sample_interval_ms -ne $Current.measurement.sample_interval_ms) { $reasons += "sample interval differs" }
    if ($Baseline.measurement.requested_runs -ne $Current.measurement.requested_runs) { $reasons += "requested run count differs" }
    foreach ($field in @(
        "provenance.cargo_lock_sha256",
        "provenance.toolchain.rustc_vv",
        "provenance.toolchain.rustc_binary",
        "provenance.toolchain.rustc_commit_hash",
        "provenance.toolchain.rustc_host",
        "provenance.toolchain.rustc_release",
        "provenance.toolchain.cargo_version",
        "provenance.toolchain.target_triple",
        "provenance.flags.rustflags",
        "provenance.flags.cargo_encoded_rustflags"
    )) {
        $baselineValue = Get-RecordValue $Baseline $field
        $currentValue = Get-RecordValue $Current $field
        if ($null -eq $baselineValue -or $null -eq $currentValue -or "$baselineValue" -eq "Unavailable" -or "$currentValue" -eq "Unavailable") {
            $reasons += "$field is unavailable"
        } elseif ("$baselineValue" -ne "$currentValue") {
            $reasons += "$field differs"
        }
    }
    foreach ($field in "OS", "CPU", "PhysicalCores", "LogicalProcessors", "MemoryGiB", "Display", "PowerPlan") {
        $baselineValue = "$($Baseline.machine.$field)"
        $currentValue = "$($Current.machine.$field)"
        if (-not $baselineValue -or -not $currentValue -or $baselineValue -eq "Unavailable" -or $currentValue -eq "Unavailable") {
            $reasons += "machine $field is unavailable"
        } elseif ($baselineValue -ne $currentValue) {
            $reasons += "machine $field differs"
        }
    }
    return [PSCustomObject]@{ Comparable = ($reasons.Count -eq 0); Reasons = $reasons }
}

function Write-GateComparison([string]$Name, $Current, $Baseline, [string]$Unit, [double]$Limit, [ValidateSet("Percent", "Absolute")] [string]$LimitType) {
    if ($null -eq $Current -or $null -eq $Baseline -or $null -eq $Baseline.p95) {
        Write-Host ("{0,-36} NOT EVALUATED (metric unavailable in current result or baseline)" -f $Name)
        return $null
    }
    if ($Unit -ne "bytes" -and ([int]$Current.samples -lt 5 -or [int]$Baseline.samples -lt 5)) {
        Write-Host ("{0,-36} NOT EVALUATED (requires at least five samples in current result and baseline)" -f $Name)
        return $null
    }

    $p50Delta = [double]$Current.p50 - [double]$Baseline.p50
    $p95Delta = [double]$Current.p95 - [double]$Baseline.p95
    if ($LimitType -eq "Percent") {
        if ([double]$Baseline.p95 -le 0) {
            Write-Host ("{0,-36} NOT EVALUATED (baseline p95 must be greater than zero)" -f $Name)
            return $null
        }
        $gateDelta = ($p95Delta / [double]$Baseline.p95) * 100.0
        $gateLabel = "+$Limit% p95"
        $gateValue = "{0:N2}%" -f $gateDelta
    } else {
        $gateDelta = $p95Delta
        $gateLabel = "+$Limit $Unit p95"
        $gateValue = "{0:N2} $Unit" -f $gateDelta
    }
    $passed = $gateDelta -le $Limit
    $status = if ($passed) { "PASS" } else { "FAIL" }
    Write-Host ("{0,-36} baseline p50/p95={1}/{2} {6}; current={3}/{4} {6}; p95 delta={5}; gate {7}: {8}" -f $Name, $Baseline.p50, $Baseline.p95, $Current.p50, $Current.p95, $gateValue, $Unit, $gateLabel, $status)
    return [PSCustomObject][ordered]@{
        metric = $Name
        unit = $Unit
        baseline_p50 = $Baseline.p50
        baseline_p95 = $Baseline.p95
        current_p50 = $Current.p50
        current_p95 = $Current.p95
        p50_delta = [math]::Round($p50Delta, 2)
        p95_delta = [math]::Round($p95Delta, 2)
        gate = $gateLabel
        status = $status
    }
}

function Write-UnsupportedGate([string]$Name, [string]$Reason) {
    Write-Host ("{0,-36} UNSUPPORTED ({1})" -f $Name, $Reason)
    return [PSCustomObject][ordered]@{
        metric = $Name
        status = "UNSUPPORTED"
        reason = $Reason
    }
}

function Write-MaxGate([string]$Name, $Current, [string]$Unit, [double]$Limit) {
    if ($null -eq $Current -or $null -eq $Current.p95) {
        Write-Host ("{0,-36} NOT EVALUATED (metric unavailable in current result)" -f $Name)
        return [PSCustomObject][ordered]@{
            metric = $Name
            status = "NOT_EVALUATED"
            reason = "metric unavailable in current result"
        }
    }
    $passed = [double]$Current.p95 -le $Limit
    $status = if ($passed) { "PASS" } else { "FAIL" }
    Write-Host ("{0,-36} current p95={1} {2}; ceiling <= {3} {2}: {4}" -f $Name, $Current.p95, $Unit, $Limit, $status)
    return [PSCustomObject][ordered]@{
        metric = $Name
        unit = $Unit
        current_p95 = $Current.p95
        gate = "<= $Limit $Unit p95"
        status = $status
    }
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
$buildProvenance = Get-BuildProvenance $ExePath.Path
$installerPackage = Get-PackageSize "Installer package" (Join-Path $repoRoot "target\release\installer") "BarePDF-Setup-x64-v*.exe"
$portablePackage = Get-PackageSize "Portable package" (Join-Path $repoRoot "target\release\portable") "BarePDF-Portable-x64-v*.zip"
$idleResults = @()
$gpuEngineValues = @()
$gpuProcessMemoryValues = @()
$gpuEngineCounterPaths = @()
$gpuProcessMemoryCounterPaths = @()
for ($run = 1; $run -le $Runs; $run++) {
    $process = $null
    $profileAppData = $null
    $previousAppData = [Environment]::GetEnvironmentVariable("APPDATA", "Process")
    try {
        $profileAppData = New-ProfileAppData $VisualMode
        [Environment]::SetEnvironmentVariable("APPDATA", $profileAppData.Root, "Process")
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
        $gpu = Get-GpuCountersForProcess $process.Id
        if ($gpu.EngineSupported) {
            $gpuEngineValues += [double]$gpu.EngineUtilizationPercent
            $gpuEngineCounterPaths += $gpu.EnginePath
        }
        if ($gpu.ProcessMemorySupported) {
            $gpuProcessMemoryValues += [double]$gpu.ProcessMemoryMiB
            $gpuProcessMemoryCounterPaths += $gpu.MemoryPaths
        }

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
        if ($null -eq $previousAppData) {
            Remove-Item Env:APPDATA -ErrorAction SilentlyContinue
        } else {
            [Environment]::SetEnvironmentVariable("APPDATA", $previousAppData, "Process")
        }
        if ($profileAppData) { Remove-ProfileAppData $profileAppData.Root }
    }
}

$results = @()
for ($run = 1; $run -le $Runs; $run++) {
    $profilePath = Join-Path $env:TEMP "barepdf-profile-$([guid]::NewGuid().ToString('N')).json"
    $process = $null
    $profileAppData = $null
    $previousAppData = [Environment]::GetEnvironmentVariable("APPDATA", "Process")
    try {
        $profileAppData = New-ProfileAppData $VisualMode
        [Environment]::SetEnvironmentVariable("APPDATA", $profileAppData.Root, "Process")
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

        $peakWorkingSet = $null
        $peakPrivate = $null
        $settledWorkingSet = $null
        $settledPrivate = $null
        for ($sampleIndex = 0; $sampleIndex -lt ($DurationSeconds * 4); $sampleIndex++) {
            Start-Sleep -Milliseconds 250
            $sample = Get-Process -Id $process.Id -ErrorAction SilentlyContinue
            if (-not $sample) { break }
            $privateMb = $sample.PrivateMemorySize64 / 1MB
            $workingSetMb = $sample.WorkingSet64 / 1MB
            $peakWorkingSet = if ($null -eq $peakWorkingSet) { $workingSetMb } else { [math]::Max($peakWorkingSet, $workingSetMb) }
            $peakPrivate = if ($null -eq $peakPrivate) { $privateMb } else { [math]::Max($peakPrivate, $privateMb) }
            $settledWorkingSet = $workingSetMb
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
            SettledWorkingSetMB = if ($null -ne $settledWorkingSet) { [math]::Round($settledWorkingSet, 2) } else { $null }
            SettledPrivateMB = if ($null -ne $settledPrivate) { [math]::Round($settledPrivate, 2) } else { $null }
            PeakWorkingSetMB = if ($null -ne $peakWorkingSet) { [math]::Round($peakWorkingSet, 2) } else { $null }
            PeakPrivateMB = if ($null -ne $peakPrivate) { [math]::Round($peakPrivate, 2) } else { $null }
        }
    } finally {
        Stop-BarePdfProcess $process
        Remove-Item -LiteralPath $profilePath -Force -ErrorAction SilentlyContinue
        if ($null -eq $previousAppData) {
            Remove-Item Env:APPDATA -ErrorAction SilentlyContinue
        } else {
            [Environment]::SetEnvironmentVariable("APPDATA", $previousAppData, "Process")
        }
        if ($profileAppData) { Remove-ProfileAppData $profileAppData.Root }
    }
}

$firstPageValues = @($results | ForEach-Object { $_.FirstBitmapMs } | Where-Object { $null -ne $_ })
$settledWorkingSetValues = @($results | ForEach-Object { $_.SettledWorkingSetMB } | Where-Object { $null -ne $_ })
$settledPrivateValues = @($results | ForEach-Object { $_.SettledPrivateMB } | Where-Object { $null -ne $_ })
$peakWorkingSetValues = @($results | ForEach-Object { $_.PeakWorkingSetMB } | Where-Object { $null -ne $_ })
$peakValues = @($results | ForEach-Object { $_.PeakPrivateMB } | Where-Object { $null -ne $_ })
$idleCpuValues = @($idleResults | ForEach-Object { $_.CpuPercent })
$idleWorkingSetValues = @($idleResults | ForEach-Object { $_.WorkingSetMB })
$idlePrivateValues = @($idleResults | ForEach-Object { $_.PrivateMB })
$gpuEngineCounterPaths = @($gpuEngineCounterPaths | Select-Object -Unique)
$gpuProcessMemoryCounterPaths = @($gpuProcessMemoryCounterPaths | Select-Object -Unique)
$gpuCounterProvenance = [PSCustomObject][ordered]@{
    provider = "Windows Performance Counters via Get-Counter"
    process_scope = "target process pid per sample"
    sample_after_settle_seconds = 3
    gpu_engine_utilization = [PSCustomObject][ordered]@{
        supported = ($gpuEngineValues.Count -gt 0)
        counter_paths = $gpuEngineCounterPaths
        samples = $gpuEngineValues.Count
        reason = if ($gpuEngineValues.Count -gt 0) { $null } else { "GPU Engine counters absent or unusable" }
    }
    gpu_process_memory = [PSCustomObject][ordered]@{
        supported = ($gpuProcessMemoryValues.Count -gt 0)
        counter_paths = $gpuProcessMemoryCounterPaths
        samples = $gpuProcessMemoryValues.Count
        reason = if ($gpuProcessMemoryValues.Count -gt 0) { $null } else { "GPU Process Memory counters absent or unusable" }
    }
}

Write-Host "BarePDF release profile ($Runs runs)" -ForegroundColor Cyan
Write-Host "Fixture: $FixtureName [$FixtureClass]"
Write-Host "Visual mode: $VisualMode"
Write-Host "PDF: $PdfPath"
Write-Host "Fixture bytes: $($fixture.Length)"
Write-Host "Fixture SHA-256: $fixtureSha256"
Write-Host "Executable SHA-256: $($buildProvenance.executable.sha256)"
Write-Host "Cargo.lock SHA-256: $($buildProvenance.cargo_lock_sha256)"
Write-Host "Target triple: $($buildProvenance.toolchain.target_triple)"
Write-Host "Duration per idle/document run: $DurationSeconds s"
Write-Host "Machine: $($machine.Computer)"
Write-Host "OS: $($machine.OS)"
Write-Host "CPU: $($machine.CPU)"
Write-Host "Cores: $($machine.PhysicalCores) physical / $($machine.LogicalProcessors) logical"
Write-Host "Memory: $($machine.MemoryGiB) GiB"
Write-Host "Display: $($machine.Display)"
Write-Host "Power plan: $($machine.PowerPlan)"
Write-Host ""

$metricSummaries = [PSCustomObject][ordered]@{
    first_low_resolution_bitmap_ms = Get-MetricSummary $firstPageValues
    idle_cpu_percent = Get-MetricSummary $idleCpuValues
    idle_gpu_engine_utilization_percent = Get-MetricSummary $gpuEngineValues
    gpu_process_memory_mib = Get-MetricSummary $gpuProcessMemoryValues
    idle_working_set_mib = Get-MetricSummary $idleWorkingSetValues
    idle_private_memory_mib = Get-MetricSummary $idlePrivateValues
    settled_working_set_mib = Get-MetricSummary $settledWorkingSetValues
    settled_private_memory_mib = Get-MetricSummary $settledPrivateValues
    peak_working_set_mib = Get-MetricSummary $peakWorkingSetValues
    peak_private_memory_mib = Get-MetricSummary $peakValues
}
$installerSizeValues = @()
if ($null -ne $installerPackage.Bytes) { $installerSizeValues += [double]$installerPackage.Bytes }
$portableSizeValues = @()
if ($null -ne $portablePackage.Bytes) { $portableSizeValues += [double]$portablePackage.Bytes }
$artifactSummaries = [PSCustomObject][ordered]@{
    installer_bytes = Get-MetricSummary $installerSizeValues
    portable_bytes = Get-MetricSummary $portableSizeValues
}

Write-MetricSummary "Cold start" $null "ms"
Write-Host "  Requires an application-ready signal; process launch time is not a cold-start milestone."
Write-MetricSummary "First low-resolution bitmap" $metricSummaries.first_low_resolution_bitmap_ms "ms"
Write-MetricSummary "First native-DPI bitmap" $null "ms"
Write-Host "  Requires a distinct native-DPI bitmap signal from the application."
Write-MetricSummary "UI callback p95" $null "ms"
Write-Host "  Requires an application callback-duration signal or trace."
Write-MetricSummary "Idle CPU" $metricSummaries.idle_cpu_percent "%"
Write-MetricSummary "Idle GPU engine utilization" $metricSummaries.idle_gpu_engine_utilization_percent "%"
Write-MetricSummary "GPU process memory" $metricSummaries.gpu_process_memory_mib "MiB"
Write-MetricSummary "Timer wake-ups" $null "wake-ups/s"
Write-Host "  Requires a WPR/WPA CPU Usage + Thread Activity trace."
Write-MetricSummary "Idle working set" $metricSummaries.idle_working_set_mib "MiB"
Write-MetricSummary "Idle private memory" $metricSummaries.idle_private_memory_mib "MiB"
Write-MetricSummary "Settled working set" $metricSummaries.settled_working_set_mib "MiB"
Write-MetricSummary "Settled private memory" $metricSummaries.settled_private_memory_mib "MiB"
Write-MetricSummary "Peak working set" $metricSummaries.peak_working_set_mib "MiB"
Write-MetricSummary "Peak private memory" $metricSummaries.peak_private_memory_mib "MiB"
Write-MetricSummary "Installer package" $artifactSummaries.installer_bytes "bytes"
if ($null -eq $artifactSummaries.installer_bytes) { Write-Host "  $($installerPackage.Reason)." }
Write-MetricSummary "Portable package" $artifactSummaries.portable_bytes "bytes"
if ($null -eq $artifactSummaries.portable_bytes) { Write-Host "  $($portablePackage.Reason)." }

$currentResult = [PSCustomObject][ordered]@{
    schema_version = 2
    captured_at_utc = (Get-Date).ToUniversalTime().ToString("o")
    visual_mode = $VisualMode
    fixture = [PSCustomObject][ordered]@{
        name = $FixtureName
        class = $FixtureClass
        bytes = [int64]$fixture.Length
        sha256 = $fixtureSha256
    }
    machine = [PSCustomObject][ordered]@{
        OS = $machine.OS
        CPU = $machine.CPU
        PhysicalCores = $machine.PhysicalCores
        LogicalProcessors = $machine.LogicalProcessors
        MemoryGiB = $machine.MemoryGiB
        Display = $machine.Display
        PowerPlan = $machine.PowerPlan
    }
    measurement = [PSCustomObject][ordered]@{
        release_profile = "release"
        visual_mode = $VisualMode
        duration_seconds = $DurationSeconds
        sample_interval_ms = 250
        settle_seconds = 3
        requested_runs = $Runs
        idle_runs = $idleResults.Count
        document_runs = $results.Count
    }
    provenance = $buildProvenance
    counter_support = $gpuCounterProvenance
    metrics = $metricSummaries
    artifacts = $artifactSummaries
}

$comparison = $null
$failedGates = @()
if ($BaselinePath) {
    $resolvedBaseline = Resolve-Path -LiteralPath $BaselinePath -ErrorAction SilentlyContinue
    if (-not $resolvedBaseline -or -not (Test-Path -LiteralPath $resolvedBaseline.Path -PathType Leaf)) {
        Write-Error "Baseline result file not found: $BaselinePath"
        exit 1
    }
    try {
        $baseline = Get-Content -LiteralPath $resolvedBaseline.Path -Raw | ConvertFrom-Json -ErrorAction Stop
    } catch {
        Write-Error "Baseline result could not be read: $($_.Exception.Message)"
        exit 1
    }
    $comparison = Get-BaselineCompatibility $baseline $currentResult
    Write-Host ""
    if (-not $comparison.Comparable) {
        Write-Host "Baseline is not comparable; gates were not evaluated." -ForegroundColor Yellow
        $comparison.Reasons | ForEach-Object { Write-Host "  - $_" }
    } else {
        Write-Host "Baseline comparison (p95 gates)" -ForegroundColor Cyan
        $comparisons = @(
            Write-GateComparison "First low-resolution bitmap" $currentResult.metrics.first_low_resolution_bitmap_ms $baseline.metrics.first_low_resolution_bitmap_ms "ms" 5 "Percent"
        )
        if ($VisualMode -eq "Efficient") {
            $comparisons += @(
                Write-GateComparison "Idle CPU" $currentResult.metrics.idle_cpu_percent $baseline.metrics.idle_cpu_percent "%" 0.2 "Absolute"
                Write-GateComparison "Idle working set" $currentResult.metrics.idle_working_set_mib $baseline.metrics.idle_working_set_mib "MiB" 2 "Absolute"
                Write-GateComparison "Peak working set" $currentResult.metrics.peak_working_set_mib $baseline.metrics.peak_working_set_mib "MiB" 5 "Percent"
            )
        } else {
            $enhancedGpuGate = if ($null -eq $currentResult.metrics.idle_gpu_engine_utilization_percent -or
                $null -eq $baseline.metrics.idle_gpu_engine_utilization_percent) {
                Write-UnsupportedGate "Enhanced idle GPU" "GPU Engine counter support is unavailable in current result or baseline"
            } else {
                Write-GateComparison "Enhanced idle GPU" $currentResult.metrics.idle_gpu_engine_utilization_percent $baseline.metrics.idle_gpu_engine_utilization_percent "%" 0.5 "Absolute"
            }
            $enhancedGpuMaxGate = if ($null -eq $currentResult.metrics.idle_gpu_engine_utilization_percent) {
                Write-UnsupportedGate "Enhanced idle GPU ceiling" "GPU Engine counter support is unavailable in current result"
            } else {
                Write-MaxGate "Enhanced idle GPU ceiling" $currentResult.metrics.idle_gpu_engine_utilization_percent "%" 1.0
            }
            $comparisons += @(
                Write-GateComparison "Enhanced idle CPU" $currentResult.metrics.idle_cpu_percent $baseline.metrics.idle_cpu_percent "%" 0.5 "Absolute"
                Write-MaxGate "Enhanced idle CPU ceiling" $currentResult.metrics.idle_cpu_percent "%" 1.0
                $enhancedGpuGate
                $enhancedGpuMaxGate
                Write-GateComparison "Enhanced idle working set" $currentResult.metrics.idle_working_set_mib $baseline.metrics.idle_working_set_mib "MiB" 8 "Absolute"
                Write-GateComparison "Enhanced peak working set" $currentResult.metrics.peak_working_set_mib $baseline.metrics.peak_working_set_mib "MiB" 10 "Percent"
                Write-GateComparison "Enhanced peak private memory" $currentResult.metrics.peak_private_memory_mib $baseline.metrics.peak_private_memory_mib "MiB" 10 "Percent"
            )
        }
        $comparisons += @(
            Write-GateComparison "Installer package" $currentResult.artifacts.installer_bytes $baseline.artifacts.installer_bytes "bytes" 2 "Percent"
            Write-GateComparison "Portable package" $currentResult.artifacts.portable_bytes $baseline.artifacts.portable_bytes "bytes" 2 "Percent"
        )
        $comparisons = @($comparisons | Where-Object { $null -ne $_ })
        $failedGates = @($comparisons | Where-Object { $_.status -eq "FAIL" })
        $unsupportedGates = @($comparisons | Where-Object { $_.status -eq "UNSUPPORTED" })
        $comparison | Add-Member -NotePropertyName comparisons -NotePropertyValue $comparisons
        $comparison | Add-Member -NotePropertyName failed_gates -NotePropertyValue $failedGates.Count
        $comparison | Add-Member -NotePropertyName unsupported_gates -NotePropertyValue $unsupportedGates.Count
    }
}
$currentResult | Add-Member -NotePropertyName baseline_comparison -NotePropertyValue $comparison

if ($ResultPath) {
    $resultDirectory = Split-Path -Parent $ResultPath
    if (-not $resultDirectory -or -not (Test-Path -LiteralPath $resultDirectory -PathType Container)) {
        Write-Error "Result directory does not exist: $resultDirectory"
        exit 1
    }
    $currentResult | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $ResultPath -Encoding utf8
    Write-Host "Saved machine- and fixture-scoped result: $ResultPath"
}
Write-Host ""
Write-Host "Idle runs"
$idleResults | Format-Table -AutoSize
Write-Host "Document runs"
$results | Format-Table -AutoSize
if ($failedGates.Count -gt 0) {
    Write-Error "$($failedGates.Count) comparable baseline gate(s) failed."
    exit 2
}
