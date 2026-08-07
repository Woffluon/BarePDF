# BarePDF Profiling Script
param (
    [string]$PdfPath = "$env:USERPROFILE\Desktop\kitap.pdf",
    [int]$DurationSeconds = 5
)

$ExePath = Join-Path $PSScriptRoot "..\target\release\barepdf.exe"
if (-not (Test-Path $ExePath)) {
    Write-Error "Release executable not found at $ExePath. Run 'cargo build --release' first."
    exit 1
}

Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "  BarePDF Baseline Profiling Report" -ForegroundColor Cyan
Write-Host "==========================================" -ForegroundColor Cyan
Write-Host "Target PDF: $PdfPath"
Write-Host "Executable: $ExePath"
Write-Host ""

# 1. Idle Application (No PDF)
Write-Host "[1/2] Launching BarePDF Idle (No PDF)..." -ForegroundColor Yellow
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$procIdle = Start-Process -FilePath $ExePath -PassThru
$sw.Stop()

Start-Sleep -Seconds 2
$idleProcStats = Get-Process -Id $procIdle.Id
$idleWs = [math]::Round($idleProcStats.WorkingSet64 / 1MB, 2)
$idlePriv = [math]::Round($idleProcStats.PrivateMemorySize64 / 1MB, 2)
$idleThreads = $idleProcStats.Threads.Count
$idleHandles = $idleProcStats.HandleCount

Write-Host "   Startup Time: $($sw.ElapsedMilliseconds) ms"
Write-Host "   Working Set:  $idleWs MB"
Write-Host "   Private Bytes:$idlePriv MB"
Write-Host "   Threads:      $idleThreads"
Write-Host "   Handles:      $idleHandles"

Stop-Process -Id $procIdle.Id -Force
Start-Sleep -Seconds 1

# 2. Open PDF (kitap.pdf)
if (-not (Test-Path $PdfPath)) {
    Write-Error "PDF file not found: $PdfPath"
    exit 1
}

Write-Host ""
Write-Host "[2/2] Launching BarePDF with kitap.pdf..." -ForegroundColor Yellow
$swPdf = [System.Diagnostics.Stopwatch]::StartNew()
$procPdf = Start-Process -FilePath $ExePath -ArgumentList "`"$PdfPath`"" -PassThru
$swPdf.Stop()

# Sample metrics over duration
$samples = @()
for ($i = 0; $i -lt $DurationSeconds; $i++) {
    Start-Sleep -Seconds 1
    $p = Get-Process -Id $procPdf.Id -ErrorAction SilentlyContinue
    if ($p) {
        $samples += [PSCustomObject]@{
            Second = $i + 1
            WorkingSetMB = [math]::Round($p.WorkingSet64 / 1MB, 2)
            PrivateBytesMB = [math]::Round($p.PrivateMemorySize64 / 1MB, 2)
            CPU = $p.CPU
            Threads = $p.Threads.Count
            Handles = $p.HandleCount
        }
    }
}

$settled = Get-Process -Id $procPdf.Id
$settledWs = [math]::Round($settled.WorkingSet64 / 1MB, 2)
$settledPriv = [math]::Round($settled.PrivateMemorySize64 / 1MB, 2)
$settledThreads = $settled.Threads.Count
$settledHandles = $settled.HandleCount

Write-Host ""
Write-Host "--- Summary Results ---" -ForegroundColor Green
Write-Host "Application Idle Working Set:  $idleWs MB"
Write-Host "Application Idle Private Bytes: $idlePriv MB"
Write-Host "kitap.pdf First Page Time:      $($swPdf.ElapsedMilliseconds) ms"
Write-Host "kitap.pdf Settled Working Set:  $settledWs MB"
Write-Host "kitap.pdf Settled Private Bytes:$settledPriv MB"
Write-Host "kitap.pdf Settled Threads:      $settledThreads"
Write-Host "kitap.pdf Settled Handles:      $settledHandles"
Write-Host ""

$samples | Format-Table -AutoSize

Stop-Process -Id $procPdf.Id -Force
