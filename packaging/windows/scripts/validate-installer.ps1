param(
    [switch]$CompileOnly
)

# PowerShell Installer Test & Validation Script for BarePDF
Set-StrictMode -Version 3.0
$ErrorActionPreference = "Stop"

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
$ThumbnailClsid = "{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}"
$ThumbnailHandler = "{E357FCCD-A995-4576-B01F-234630154E96}"
$ProgId = "BarePDF.Document.1"
$ProductionAppId = "B3A82379-88F4-4D4D-A815-998A4476B66C"

function Get-IsccPath {
    $Command = Get-Command "iscc.exe" -ErrorAction SilentlyContinue
    if ($Command) {
        return $Command.Path
    }

    foreach ($Candidate in @(
        "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
        "C:\Program Files\Inno Setup 6\ISCC.exe"
    )) {
        if (Test-Path -LiteralPath $Candidate -PathType Leaf) {
            return $Candidate
        }
    }

    throw "Inno Setup Compiler (ISCC.exe) is required for installer validation."
}

function Open-CurrentUserKey {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey
    )

    $BaseKey = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::CurrentUser,
        $View
    )
    try {
        return $BaseKey.OpenSubKey($Subkey)
    }
    finally {
        $BaseKey.Dispose()
    }
}

function Test-CurrentUserKey {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey
    )

    $Key = Open-CurrentUserKey -View $View -Subkey $Subkey
    if ($null -eq $Key) {
        return $false
    }

    $Key.Dispose()
    return $true
}

function Test-CurrentUserValue {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey,
        [Parameter(Mandatory)]
        [string]$Name
    )

    $Key = Open-CurrentUserKey -View $View -Subkey $Subkey
    if ($null -eq $Key) {
        return $false
    }
    try {
        $Value = $Key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        return $null -ne $Value
    }
    finally {
        $Key.Dispose()
    }
}

function Set-CurrentUserStringValue {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey,
        [Parameter(Mandatory)]
        [string]$Name,
        [Parameter(Mandatory)]
        [string]$Value
    )

    $BaseKey = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::CurrentUser,
        $View
    )
    try {
        $Key = $BaseKey.CreateSubKey($Subkey)
        if ($null -eq $Key) {
            throw "Could not create validation registry key in $View view: HKCU\$Subkey"
        }
        try {
            $Key.SetValue($Name, $Value, [Microsoft.Win32.RegistryValueKind]::String)
        }
        finally {
            $Key.Dispose()
        }
    }
    finally {
        $BaseKey.Dispose()
    }
}

function Remove-CurrentUserKey {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey
    )

    $BaseKey = [Microsoft.Win32.RegistryKey]::OpenBaseKey(
        [Microsoft.Win32.RegistryHive]::CurrentUser,
        $View
    )
    try {
        $BaseKey.DeleteSubKeyTree($Subkey, $false)
    }
    finally {
        $BaseKey.Dispose()
    }
}

function Get-BarePdfUserStateEvidence {
    $Evidence = [System.Collections.Generic.List[string]]::new()
    $ProductKeys = @(
        "Software\Classes\CLSID\$ThumbnailClsid",
        "Software\Classes\$ProgId",
        "Software\Classes\Applications\BarePDF.exe",
        "Software\BarePDF\Capabilities"
    )

    foreach ($View in @(
        [Microsoft.Win32.RegistryView]::Registry32,
        [Microsoft.Win32.RegistryView]::Registry64
    )) {
        foreach ($Subkey in $ProductKeys) {
            if (Test-CurrentUserKey -View $View -Subkey $Subkey) {
                $Evidence.Add("$View\HKCU\$Subkey")
            }
        }

        foreach ($ValueEntry in @(
            @{ Subkey = "Software\RegisteredApplications"; Name = "BarePDF" },
            @{ Subkey = "Software\Classes\.pdf\OpenWithProgids"; Name = $ProgId }
        )) {
            if (Test-CurrentUserValue -View $View -Subkey $ValueEntry.Subkey -Name $ValueEntry.Name) {
                $Evidence.Add("$View\HKCU\$($ValueEntry.Subkey) [$($ValueEntry.Name)]")
            }
        }

        $UninstallKey = Open-CurrentUserKey -View $View -Subkey "Software\Microsoft\Windows\CurrentVersion\Uninstall"
        if ($null -ne $UninstallKey) {
            try {
                foreach ($Name in $UninstallKey.GetSubKeyNames()) {
                    $Entry = $UninstallKey.OpenSubKey($Name)
                    if ($null -eq $Entry) {
                        continue
                    }
                    try {
                        if ($Name.Contains($ProductionAppId) -or $Entry.GetValue("DisplayName") -eq "BarePDF") {
                            $Evidence.Add("$View\HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\$Name")
                        }
                    }
                    finally {
                        $Entry.Dispose()
                    }
                }
            }
            finally {
                $UninstallKey.Dispose()
            }
        }
    }

    foreach ($Path in @(
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)) "Programs\BarePDF"),
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)) "BarePDF"),
        (Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)) "BarePDF.lnk")
    )) {
        if (Test-Path -LiteralPath $Path) {
            $Evidence.Add($Path)
        }
    }

    return @($Evidence | Sort-Object -Unique)
}

function Assert-NoExistingBarePdfRegistration {
    $Evidence = @(Get-BarePdfUserStateEvidence)
    if ($Evidence.Count -gt 0) {
        $Details = $Evidence -join "; "
        throw "Installer validation refuses to modify an account with an existing BarePDF registration. Use -CompileOnly here, or run the full validation in a clean disposable Windows account/CI environment. Found: $Details"
    }
}

function Assert-RegistryValue {
    param(
        [Parameter(Mandatory)]
        [Microsoft.Win32.RegistryView]$View,
        [Parameter(Mandatory)]
        [string]$Subkey,
        [AllowEmptyString()]
        [string]$Name = "",
        [Parameter(Mandatory)]
        [string]$Expected
    )

    $Key = Open-CurrentUserKey -View $View -Subkey $Subkey
    if ($null -eq $Key) {
        throw "Registry key missing in $View view: HKCU\$Subkey"
    }
    try {
        $Actual = [string]$Key.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
        if ($Actual -cne $Expected) {
            throw "Registry value mismatch in $View view at HKCU\$Subkey [$Name]: expected '$Expected', got '$Actual'"
        }
    }
    finally {
        $Key.Dispose()
    }
}

function Assert-ValidationDirectory {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    $ResolvedPath = [System.IO.Path]::GetFullPath($Path)
    $TempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if (-not $ResolvedPath.StartsWith($TempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not ([System.IO.Path]::GetFileName($ResolvedPath)).StartsWith("BarePDF-InstallerValidation-", [System.StringComparison]::Ordinal)) {
        throw "Refusing to remove unexpected validation directory: $ResolvedPath"
    }
}

$Metadata = cargo metadata --manifest-path (Join-Path $RepoRoot "Cargo.toml") --locked --no-deps --format-version 1 | ConvertFrom-Json
$Version = ($Metadata.packages | Where-Object { $_.name -eq "barepdf" } | Select-Object -First 1).version
if (-not $Version) {
    throw "barepdf package version not found"
}

if (-not $CompileOnly) {
    Assert-NoExistingBarePdfRegistration
}

$RequiredStagedFiles = @(
    "BarePDF.exe",
    "BarePDF.Thumbnail.dll",
    "pdfium.dll",
    "README.md",
    "LICENSE"
)
$StagedDirectory = Join-Path $RepoRoot "target\release\staged"
foreach ($Name in $RequiredStagedFiles) {
    $Path = Join-Path $StagedDirectory $Name
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf) -or (Get-Item -LiteralPath $Path).Length -eq 0) {
        throw "Required staged installer input is missing or empty: $Path"
    }
}

$ValidationToken = [System.Guid]::NewGuid().ToString("N")
$ValidationRoot = Join-Path ([System.IO.Path]::GetTempPath()) "BarePDF-InstallerValidation-$ValidationToken"
$ValidationAppId = "BarePDF.Validation.$ValidationToken"
$InstallerPath = Join-Path $ValidationRoot "BarePDF-Validation.exe"
$TestInstallDir = Join-Path $ValidationRoot "app"
$IssPath = Join-Path $RepoRoot "packaging\windows\installer\BarePDF.iss"
$IsccPath = Get-IsccPath
$InstallAttempted = $false
$LegacyClsidOwnedByValidation = $false

try {
    New-Item -ItemType Directory -Path $ValidationRoot | Out-Null
    Write-Host "Compiling isolated validation installer..." -ForegroundColor Cyan
    & $IsccPath "/DMyAppVersion=$Version" "/DMyAppId=$ValidationAppId" "/O$ValidationRoot" "/FBarePDF-Validation" $IssPath
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup validation compilation failed with exit code $LASTEXITCODE"
    }
    if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
        throw "Validation installer binary not found: $InstallerPath"
    }

    if ($CompileOnly) {
        Write-Host "Installer compile-only validation passed without changing the current installation." -ForegroundColor Green
        return
    }

    Assert-NoExistingBarePdfRegistration

    $LegacyClsidKey = "Software\Classes\CLSID\$ThumbnailClsid"
    $LegacyClsidOwnedByValidation = $true
    Set-CurrentUserStringValue -View ([Microsoft.Win32.RegistryView]::Registry32) `
        -Subkey $LegacyClsidKey `
        -Name "BarePDFInstallerValidation" `
        -Value $ValidationToken

    Write-Host "Running isolated silent installation test in $TestInstallDir..." -ForegroundColor Cyan
    $InstallAttempted = $true
    $InstallProc = Start-Process -FilePath $InstallerPath -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR=`"$TestInstallDir`"" -WindowStyle Hidden -Wait -PassThru
    if ($InstallProc.ExitCode -ne 0) {
        throw "Installer exited with non-zero error code: $($InstallProc.ExitCode)"
    }

    foreach ($Name in @("BarePDF.exe", "BarePDF.Thumbnail.dll", "pdfium.dll")) {
        $InstalledPath = Join-Path $TestInstallDir $Name
        if (-not (Test-Path -LiteralPath $InstalledPath -PathType Leaf) -or (Get-Item -LiteralPath $InstalledPath).Length -eq 0) {
            throw "Installed runtime file is missing or empty: $InstalledPath"
        }
    }

    $NativeView = [Microsoft.Win32.RegistryView]::Registry64
    $InprocKey = "Software\Classes\CLSID\$ThumbnailClsid\InprocServer32"
    $ExpectedThumbnailDll = Join-Path $TestInstallDir "BarePDF.Thumbnail.dll"
    Assert-RegistryValue -View $NativeView -Subkey $InprocKey -Expected $ExpectedThumbnailDll
    Assert-RegistryValue -View $NativeView -Subkey $InprocKey -Name "ThreadingModel" -Expected "Apartment"
    Assert-RegistryValue -View $NativeView -Subkey "Software\Classes\$ProgId\ShellEx\$ThumbnailHandler" -Expected $ThumbnailClsid
    Assert-RegistryValue -View $NativeView -Subkey "Software\Classes\Applications\BarePDF.exe\ShellEx\$ThumbnailHandler" -Expected $ThumbnailClsid

    if (Test-CurrentUserKey -View ([Microsoft.Win32.RegistryView]::Registry32) -Subkey $LegacyClsidKey) {
        throw "Legacy private thumbnail CLSID remains in the 32-bit registry view after installation"
    }
    $LegacyClsidOwnedByValidation = $false

    $ExplorerAdvanced = Open-CurrentUserKey -View $NativeView -Subkey "Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced"
    if ($null -ne $ExplorerAdvanced) {
        try {
            $IconsOnly = $ExplorerAdvanced.GetValue("IconsOnly", 0)
            if ([int]$IconsOnly -ne 0) {
                throw "Explorer IconsOnly is enabled for this validation account; set 'Always show icons, never thumbnails' off before end-to-end thumbnail inspection"
            }
        }
        finally {
            $ExplorerAdvanced.Dispose()
        }
    }

    $Uninstaller = Join-Path $TestInstallDir "unins000.exe"
    if (-not (Test-Path -LiteralPath $Uninstaller -PathType Leaf)) {
        throw "Uninstaller not found: $Uninstaller"
    }

    Write-Host "Running isolated uninstaller test..." -ForegroundColor Cyan
    $UninstallProc = Start-Process -FilePath $Uninstaller -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART" -WindowStyle Hidden -Wait -PassThru
    if ($UninstallProc.ExitCode -ne 0) {
        throw "Uninstaller exited with non-zero code: $($UninstallProc.ExitCode)"
    }

    foreach ($Subkey in @(
        "Software\Classes\CLSID\$ThumbnailClsid",
        "Software\Classes\$ProgId\ShellEx\$ThumbnailHandler",
        "Software\Classes\Applications\BarePDF.exe\ShellEx\$ThumbnailHandler"
    )) {
        if (Test-CurrentUserKey -View $NativeView -Subkey $Subkey) {
            throw "Uninstaller left native thumbnail registration behind: HKCU\$Subkey"
        }
    }
    foreach ($Name in @("BarePDF.exe", "BarePDF.Thumbnail.dll", "pdfium.dll")) {
        if (Test-Path -LiteralPath (Join-Path $TestInstallDir $Name)) {
            throw "Uninstaller left installed runtime file behind: $Name"
        }
    }
    $RemainingState = @(Get-BarePdfUserStateEvidence)
    if ($RemainingState.Count -gt 0) {
        throw "Uninstaller left BarePDF user state behind: $($RemainingState -join '; ')"
    }
    $InstallAttempted = $false

    Write-Host "Installer validation completed successfully." -ForegroundColor Green
}
finally {
    $CanRemoveValidationRoot = $true
    if ($LegacyClsidOwnedByValidation) {
        Remove-CurrentUserKey -View ([Microsoft.Win32.RegistryView]::Registry32) -Subkey "Software\Classes\CLSID\$ThumbnailClsid"
        if (Test-CurrentUserKey -View ([Microsoft.Win32.RegistryView]::Registry32) -Subkey "Software\Classes\CLSID\$ThumbnailClsid") {
            $CanRemoveValidationRoot = $false
            Write-Warning "Emergency cleanup could not remove the validation-owned legacy private thumbnail CLSID."
        }
    }
    if ($InstallAttempted) {
        $Uninstaller = Join-Path $TestInstallDir "unins000.exe"
        if (Test-Path -LiteralPath $Uninstaller -PathType Leaf) {
            $CleanupProc = Start-Process -FilePath $Uninstaller -ArgumentList "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART" -WindowStyle Hidden -Wait -PassThru
            if ($CleanupProc.ExitCode -ne 0) {
                Write-Warning "Emergency validation uninstall exited with code $($CleanupProc.ExitCode)."
            }
        }

        $RemainingState = [System.Collections.Generic.List[string]]::new()
        foreach ($Item in @(Get-BarePdfUserStateEvidence)) {
            $RemainingState.Add($Item)
        }
        foreach ($Name in @("BarePDF.exe", "BarePDF.Thumbnail.dll", "pdfium.dll")) {
            $Path = Join-Path $TestInstallDir $Name
            if (Test-Path -LiteralPath $Path) {
                $RemainingState.Add($Path)
            }
        }
        if ($RemainingState.Count -gt 0) {
            $CanRemoveValidationRoot = $false
            Write-Warning "Emergency cleanup left validation state behind: $($RemainingState -join '; ')"
        }
    }

    if (Test-Path -LiteralPath $ValidationRoot) {
        if ($CanRemoveValidationRoot) {
            Assert-ValidationDirectory -Path $ValidationRoot
            Remove-Item -LiteralPath $ValidationRoot -Recurse -Force
        }
        else {
            Write-Warning "Preserving validation directory for manual recovery: $ValidationRoot"
        }
    }
}
