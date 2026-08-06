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

# 2. Build release target & stage binaries
powershell -File packaging/windows/scripts/stage-release.ps1

# 3. Create Portable ZIP package
powershell -File packaging/windows/scripts/build-portable.ps1

# 4. Compile Windows Setup Installer (requires Inno Setup 6)
powershell -File packaging/windows/scripts/build-installer.ps1

# 5. Run non-interactive installation validation test
powershell -File packaging/windows/scripts/validate-installer.ps1

# 6. Generate SHA-256 checksum file
powershell -File packaging/windows/scripts/generate-checksums.ps1
```

## Windows Registration Summary

BarePDF registers standard per-user registry entries (`HKCU\Software\Classes\BarePDF.Document.1` & `HKCU\Software\RegisteredApplications`). During installation, users are asked if they wish to set BarePDF as default, which opens `ms-settings:defaultapps?registeredAppUser=BarePDF` upon completion to comply with Windows Default Apps standards.
