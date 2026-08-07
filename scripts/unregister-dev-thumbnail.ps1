# PowerShell script to unregister BarePDF Thumbnail Provider COM DLL
$ErrorActionPreference = "Continue"

$Clsid = "{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}"
$ProgID = "BarePDF.Document.1"
$ShellExGuid = "{E357FCCD-A995-4576-B01F-234630154E96}"

Remove-Item -Path "HKCU:\Software\Classes\CLSID\$Clsid" -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item -Path "HKCU:\Software\Classes\$ProgID\ShellEx\$ShellExGuid" -Recurse -Force -ErrorAction SilentlyContinue
Remove-ItemProperty -Path "HKCU:\Software\Classes\$ProgID" -Name "TypeOverlay" -ErrorAction SilentlyContinue
Remove-Item -Path "HKCU:\Software\Classes\.pdf\ShellEx\$ShellExGuid" -Recurse -Force -ErrorAction SilentlyContinue

$Signature = @"
[DllImport("shell32.dll", CharSet = CharSet.Auto, SetLastError = true)]
public static extern void SHChangeNotify(uint wEventId, uint uFlags, IntPtr dwItem1, IntPtr dwItem2);
"@
$Shell32 = Add-Type -MemberDefinition $Signature -Name "Win32ShellUnreg" -Namespace "Win32Functions" -PassThru
$Shell32::SHChangeNotify(0x08000000, 0, [IntPtr]::Zero, [IntPtr]::Zero)

Write-Host "Unregistered BarePDF Thumbnail Provider." -ForegroundColor Green
