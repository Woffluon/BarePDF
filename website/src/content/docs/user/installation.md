---
title: "Installation & Setup"
description: "How to install BarePDF using the Windows Setup executable or run the portable ZIP package."
category: "user"
order: 2
---

# Installation & Setup

BarePDF provides release packages for both Windows (Interactive Setup & Portable ZIP) and Linux (Debian / Pardus `.deb` & Portable `AppImage`).

## Option 1: Windows Setup Installer (Recommended for Windows)

[Open the BarePDF download page](/BarePDF/download/) and download the single Windows installer for the latest stable release.

### Setup Behavior
- **No Administrator Rights Required**: Installs directly to `%LOCALAPPDATA%\Programs\BarePDF`.
- **File Association**: Registers ProgID `BarePDF.Document.1` and application capabilities under `HKCU`.
- **Open With**: Registers BarePDF in the Windows "Open with" menu and `RegisteredApplications`.
- **Default App Prompt**: Setup offers an option upon completion: *"Launch Windows Default Apps settings to select BarePDF as your default .pdf reader"*.

> [!NOTE]
> BarePDF strictly adheres to Windows guidelines. It never forcibly overwrites protected `UserChoice` registry keys. If you choose to set BarePDF as default, Windows opens the native `ms-settings:defaultapps?registeredAppUser=BarePDF` page for your confirmation.

---

## Option 2: Portable Windows ZIP Package

[Open the BarePDF download page](/BarePDF/download/) and download the single portable package for the latest stable release.

1. Extract the ZIP archive anywhere on your disk or portable USB drive (e.g. `C:\Tools\BarePDF-Portable`).
2. Double-click `BarePDF.exe` to run immediately.
3. The portable build requires no installation, leaves no registry traces, and operates completely self-contained.

---

## Option 3: Debian / Pardus (.deb) (Recommended for Linux)

[Open the BarePDF download page](/BarePDF/download/) and download the `.deb` package (`barepdf_<version>_amd64.deb`) for Pardus 23 "Yirmiüç", Pardus 25 "Bilge", Debian 12+, or Ubuntu 22.04+.

### Installation on Pardus
1. **Via GUI (Pardus Paket Kurucu):** Double-click the downloaded `.deb` file. Pardus Paket Kurucu will prompt to install BarePDF with full desktop and MIME associations.
2. **Via Terminal:**
   ```bash
   sudo apt install ./barepdf_*_amd64.deb
   ```

### Desktop Integration
- Installs the binary to `/usr/bin/barepdf` and bundled PDFium runtime to `/usr/lib/barepdf/`.
- Integrates into your application menu under **Office** / **Ofis** with native desktop icons.
- Associates MIME type `application/pdf` for seamless double-click opening in File Managers (Thunar, Dolphin, Nautilus).

---

## Option 4: Portable Linux AppImage

[Open the BarePDF download page](/BarePDF/download/) and download the universal standalone Linux executable (`BarePDF-<version>-x86_64.AppImage`).

1. Open your terminal or file manager and grant execution permissions:
   ```bash
   chmod +x BarePDF-*-x86_64.AppImage
   ```
2. Run BarePDF directly:
   ```bash
   ./BarePDF-*-x86_64.AppImage
   ```
3. The AppImage bundles all required runtime libraries including PDFium and runs on any modern 64-bit Linux distribution without system changes.

---

## Verifying Release Checksums

You can compare downloaded release integrity with the published checksum manifest using PowerShell:

```powershell
$Installer = Get-Item .\BarePDF-Setup-x64-v*.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $Installer.FullName
```

Compare the calculated hash with the versioned checksum manifest linked on the download page.

---

## Uninstallation

To remove the installed version:
1. Open Windows **Settings** → **Apps** → **Installed apps**.
2. Locate **BarePDF** and click **Uninstall**.
3. The uninstaller removes `%LOCALAPPDATA%\Programs\BarePDF` and clean-up registry keys. Your personal documents are never touched.
