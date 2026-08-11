---
title: "Installation & Setup"
description: "How to install BarePDF using the Windows Setup executable or run the portable ZIP package."
category: "user"
order: 2
---

# Installation & Setup

BarePDF provides two distribution channels: an interactive **Windows Setup Installer** and a zero-install **Portable ZIP Package**.

## Option 1: Windows Setup Installer (Recommended)

[Download the latest official Windows installer](https://github.com/Woffluon/BarePDF/releases/latest/download/BarePDF-Setup-x64.exe).

### Setup Behavior
- **No Administrator Rights Required**: Installs directly to `%LOCALAPPDATA%\Programs\BarePDF`.
- **File Association**: Registers ProgID `BarePDF.Document.1` and application capabilities under `HKCU`.
- **Open With**: Registers BarePDF in the Windows "Open with" menu and `RegisteredApplications`.
- **Default App Prompt**: Setup offers an option upon completion: *"Launch Windows Default Apps settings to select BarePDF as your default .pdf reader"*.

> [!NOTE]
> BarePDF strictly adheres to Windows guidelines. It never forcibly overwrites protected `UserChoice` registry keys. If you choose to set BarePDF as default, Windows opens the native `ms-settings:defaultapps?registeredAppUser=BarePDF` page for your confirmation.

---

## Option 2: Portable ZIP Package

[Download the latest official portable package](https://github.com/Woffluon/BarePDF/releases/latest/download/BarePDF-Portable-x64.zip).

1. Extract the ZIP archive anywhere on your disk or portable USB drive (e.g. `C:\Tools\BarePDF-Portable`).
2. Double-click `BarePDF.exe` to run immediately.
3. The portable build requires no installation, leaves no registry traces, and operates completely self-contained.

---

## Verifying Release Checksums

You can compare downloaded release integrity with the published checksum manifest using PowerShell:

```powershell
$Installer = Get-Item .\BarePDF-Setup-x64.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $Installer.FullName
```

Compare the calculated hash with the latest [`BarePDF-SHA256SUMS.txt`](https://github.com/Woffluon/BarePDF/releases/latest/download/BarePDF-SHA256SUMS.txt).

---

## Uninstallation

To remove the installed version:
1. Open Windows **Settings** → **Apps** → **Installed apps**.
2. Locate **BarePDF** and click **Uninstall**.
3. The uninstaller removes `%LOCALAPPDATA%\Programs\BarePDF` and clean-up registry keys. Your personal documents are never touched.
