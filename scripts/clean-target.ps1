# PowerShell Target Directory Cleanup Script for BarePDF
[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [int]$Days = 3,

    [switch]$IncrementalOnly,

    [switch]$DryRun,

    [string]$TargetDir = "target"
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ResolvedTarget = Join-Path $RepoRoot $TargetDir

if (-not (Test-Path -LiteralPath $ResolvedTarget)) {
    Write-Host "Target directory not found at: $ResolvedTarget" -ForegroundColor Yellow
    return
}

$CutoffDate = (Get-Date).AddDays(-$Days)
Write-Host "BarePDF Target Pruner" -ForegroundColor Cyan
Write-Host "Repo Root: $RepoRoot"
Write-Host "Target:    $ResolvedTarget"
if ($IncrementalOnly) {
    Write-Host "Scope:     Incremental Cache Only" -ForegroundColor Yellow
} else {
    Write-Host "Scope:     Incremental Cache + Build Artifacts older than $Days day(s) ($($CutoffDate.ToString('yyyy-MM-dd HH:mm')))"
}
if ($DryRun) {
    Write-Host "Mode:      DRY RUN (no files will be deleted)" -ForegroundColor Magenta
}

$TotalBytes = 0
$DeletedCount = 0
$FailedCount = 0

function Remove-TargetFileSafely {
    param(
        [System.IO.FileInfo]$File
    )
    $script:TotalBytes += $File.Length
    $script:DeletedCount++

    if ($DryRun) {
        Write-Verbose "Would delete: $($File.FullName) ($([math]::Round($File.Length / 1MB, 2)) MB)"
    } else {
        try {
            Remove-Item -LiteralPath $File.FullName -Force -ErrorAction Stop
        } catch {
            $script:FailedCount++
            Write-Warning "Could not delete $($File.FullName): $_"
        }
    }
}

# 1. Clean incremental cache
$IncrementalDir = Join-Path $ResolvedTarget "debug\incremental"
if (Test-Path -LiteralPath $IncrementalDir) {
    Write-Host "`nScanning incremental cache..." -ForegroundColor Gray
    $IncrementalFiles = Get-ChildItem -LiteralPath $IncrementalDir -Recurse -File -ErrorAction SilentlyContinue |
        Where-Object { $_.LastWriteTime -lt $CutoffDate }

    foreach ($file in $IncrementalFiles) {
        Remove-TargetFileSafely -File $file
    }

    # Clean empty directories in incremental
    if (-not $DryRun) {
        Get-ChildItem -LiteralPath $IncrementalDir -Recurse -Directory -ErrorAction SilentlyContinue |
            Where-Object { @(Get-ChildItem -LiteralPath $_.FullName -Recurse -File -ErrorAction SilentlyContinue).Length -eq 0 } |
            Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    }
}

# 2. Clean stale debug artifacts (deps, tmp) if not IncrementalOnly
if (-not $IncrementalOnly) {
    Write-Host "Scanning build artifacts (deps, tmp)..." -ForegroundColor Gray
    
    $TmpDir = Join-Path $ResolvedTarget "tmp"
    if (Test-Path -LiteralPath $TmpDir) {
        $TmpFiles = Get-ChildItem -LiteralPath $TmpDir -Recurse -File -ErrorAction SilentlyContinue
        foreach ($file in $TmpFiles) {
            Remove-TargetFileSafely -File $file
        }
    }

    $DepsDir = Join-Path $ResolvedTarget "debug\deps"
    if (Test-Path -LiteralPath $DepsDir) {
        # Target stale PDBs, old test binaries, and intermediate d files older than cutoff
        $CandidateFiles = Get-ChildItem -LiteralPath $DepsDir -File -ErrorAction SilentlyContinue |
            Where-Object {
                $_.LastWriteTime -lt $CutoffDate -and
                ($_.Extension -in @(".pdb", ".exe", ".d", ".o") -or $_.Name -like "*test*")
            }

        foreach ($file in $CandidateFiles) {
            Remove-TargetFileSafely -File $file
        }
    }
}

$TotalMB = [math]::Round($TotalBytes / 1MB, 2)
$TotalGB = [math]::Round($TotalBytes / 1GB, 2)

Write-Host "`nSummary:" -ForegroundColor Cyan
Write-Host "  Files Processed: $DeletedCount"
if ($TotalGB -ge 1.0) {
    Write-Host "  Space Reclaimed: $TotalGB GB ($TotalMB MB)" -ForegroundColor Green
} else {
    Write-Host "  Space Reclaimed: $TotalMB MB" -ForegroundColor Green
}

if ($FailedCount -gt 0) {
    Write-Host "  Locked / Skipped Files: $FailedCount" -ForegroundColor Yellow
}
