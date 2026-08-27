# BarePDF Windows Packaging Infrastructure

This directory contains Inno Setup configuration and PowerShell automation scripts for building and verifying Windows releases of BarePDF.

## Structure

```
packaging/windows/
├── installer/
│   └── BarePDF.iss                 # Inno Setup 6 installer script
├── scripts/
│   ├── validate-version.ps1        # Version consistency check
│   ├── stage-release.ps1           # Release build & binary staging
│   ├── build-portable.ps1          # Portable ZIP builder
│   ├── build-installer.ps1         # ISCC installer compiler
│   ├── validate-installer.ps1      # Non-interactive silent installation tester
│   └── generate-checksums.ps1      # SHA-256 manifest generator
└── README.md
```

## Running Staging & Build Scripts Locally

```powershell
# 1. Validate version consistency across Cargo & Inno Setup
powershell -File packaging/windows/scripts/validate-version.ps1

# 2. Fetch the pinned, SHA-256 verified PDFium runtime
powershell -File packaging/windows/scripts/fetch-pdfium.ps1

# 3. Build release target & stage binaries
powershell -File packaging/windows/scripts/stage-release.ps1

# 4. Create Portable ZIP package
powershell -File packaging/windows/scripts/build-portable.ps1

# 5. Compile Windows Setup Installer (requires Inno Setup 6)
powershell -File packaging/windows/scripts/build-installer.ps1

# 6a. Compile an isolated validation installer without changing the current account
powershell -File packaging/windows/scripts/validate-installer.ps1 -CompileOnly

# 6b. On a clean disposable Windows account/CI runner, validate install, native
#     thumbnail registration, required DLLs, and uninstall cleanup
powershell -File packaging/windows/scripts/validate-installer.ps1

# 7. Generate SHA-256 checksum file
powershell -File packaging/windows/scripts/generate-checksums.ps1
```

## Windows Registration Summary

BarePDF registers standard per-user registry entries (`HKCU\Software\Classes\BarePDF.Document.1` & `HKCU\Software\RegisteredApplications`). During installation, users are asked if they wish to set BarePDF as default, which opens `ms-settings:defaultapps?registeredAppUser=BarePDF` upon completion to comply with Windows Default Apps standards.

The x64 installer registers the thumbnail provider's COM class and ShellEx mappings in the native 64-bit HKCU registry view. Upgrades remove only BarePDF's legacy private 32-bit thumbnail CLSID. Full installer validation refuses to run in an account where it detects existing BarePDF registration, install-directory, or shortcut state; use `-CompileOnly` there and reserve the install/uninstall validation for a clean disposable account or CI runner.
