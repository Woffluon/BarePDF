# PowerShell script to register BarePDF Thumbnail Provider COM DLL for local testing/dev
param(
    [string]$BaseDir = "$PSScriptRoot\..\target\release\staged"
)

$ErrorActionPreference = "Stop"
$BaseDir = Resolve-Path $BaseDir
$DllPath = Join-Path $BaseDir "BarePDF.Thumbnail.dll"
$ExePath = Join-Path $BaseDir "BarePDF.exe"

if (-not (Test-Path $DllPath)) {
    Write-Error "BarePDF.Thumbnail.dll not found at $DllPath"
}

$Clsid = "{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}"
$ProgID = "BarePDF.Document.1"
$ShellExGuid = "{E357FCCD-A995-4576-B01F-234630154E96}"

Write-Host "Registering BarePDF Thumbnail Provider COM Server..." -ForegroundColor Cyan

# 1. Register COM CLSID
New-Item -Path "HKCU:\Software\Classes\CLSID\$Clsid" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$Clsid" -Name "(default)" -Value "BarePDF Thumbnail Provider"

New-Item -Path "HKCU:\Software\Classes\CLSID\$Clsid\InprocServer32" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$Clsid\InprocServer32" -Name "(default)" -Value $DllPath
Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$Clsid\InprocServer32" -Name "ThreadingModel" -Value "Apartment"

# 2. Register ProgID & ShellEx
New-Item -Path "HKCU:\Software\Classes\$ProgID" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\$ProgID" -Name "(default)" -Value "PDF Document"
Set-ItemProperty -Path "HKCU:\Software\Classes\$ProgID" -Name "TypeOverlay" -Value "`"$ExePath`",0"

New-Item -Path "HKCU:\Software\Classes\$ProgID\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\$ProgID\DefaultIcon" -Name "(default)" -Value "`"$ExePath`",0"

New-Item -Path "HKCU:\Software\Classes\$ProgID\ShellEx\$ShellExGuid" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\$ProgID\ShellEx\$ShellExGuid" -Name "(default)" -Value $Clsid

# Register Applications\BarePDF.exe (for Windows 10/11 UserChoice Applications entry)
New-Item -Path "HKCU:\Software\Classes\Applications\BarePDF.exe" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Applications\BarePDF.exe" -Name "TypeOverlay" -Value "`"$ExePath`",0"
New-Item -Path "HKCU:\Software\Classes\Applications\BarePDF.exe\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Applications\BarePDF.exe\DefaultIcon" -Name "(default)" -Value "`"$ExePath`",0"
New-Item -Path "HKCU:\Software\Classes\Applications\BarePDF.exe\ShellEx\$ShellExGuid" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\Applications\BarePDF.exe\ShellEx\$ShellExGuid" -Name "(default)" -Value $Clsid

# Direct fallback under .pdf
New-Item -Path "HKCU:\Software\Classes\.pdf\ShellEx\$ShellExGuid" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.pdf\ShellEx\$ShellExGuid" -Name "(default)" -Value $Clsid

New-Item -Path "HKCU:\Software\Classes\.pdf\OpenWithProgids" -Force | Out-Null
Set-ItemProperty -Path "HKCU:\Software\Classes\.pdf\OpenWithProgids" -Name $ProgID -Value ""

Write-Host "COM registration complete. Refreshing Explorer Shell Cache..." -ForegroundColor Green

# 3. Notify Windows Shell
$Signature = @"
[DllImport("shell32.dll", CharSet = CharSet.Auto, SetLastError = true)]
public static extern void SHChangeNotify(uint wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);
"@
$Shell32 = Add-Type -MemberDefinition $Signature -Name "Win32Shell" -Namespace "Win32Functions" -PassThru
$Shell32::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host "Shell association change notified successfully." -ForegroundColor Green
