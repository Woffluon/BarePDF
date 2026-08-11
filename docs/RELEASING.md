# BarePDF Clean-Windows Release Verification Checklist

This document provides a manual test verification checklist for BarePDF releases on clean Windows 10 and 11 hardware or disposable virtual machines.

## Pre-Release Verification

- [ ] Choose the exact Conventional Commit message, prepare the product version, and verify the working tree with that same message:
  ```powershell
  $CommitMessage = "feat(scope): describe the releasable change"
  powershell -File scripts/prepare-version.ps1 -Message $CommitMessage
  powershell -File packaging/windows/scripts/validate-version.ps1 -Message $CommitMessage
  ```
- [ ] Run formatting, clippy, unit tests, and release build:
  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo test --workspace --all-features --locked
  cargo build --workspace --release --locked
  ```
- [ ] Execute release packaging pipeline:
  ```powershell
  powershell -File packaging/windows/scripts/stage-release.ps1
  powershell -File packaging/windows/scripts/build-portable.ps1
  powershell -File packaging/windows/scripts/build-installer.ps1
  powershell -File packaging/windows/scripts/validate-installer.ps1
  powershell -File packaging/windows/scripts/generate-checksums.ps1
  ```

---

## Clean Windows Installation Verification

### Interactive Setup Test
1. [Open the latest GitHub Release](https://github.com/Woffluon/BarePDF/releases/latest) and download its versioned `BarePDF-Setup-x64-v<version>.exe` asset onto a clean Windows 10 or 11 system.
2. Verify its SHA-256 checksum against `BarePDF-v<version>-SHA256SUMS.txt` from the same release.
3. Launch the downloaded installer as a normal non-administrator user.
4. Confirm destination defaults to `%LOCALAPPDATA%\Programs\BarePDF`.
5. On the **Default PDF Reader** page, select *"Yes, open Windows Default Apps settings after installation"*.
6. Complete setup. Verify Windows **Default Apps** settings window opens automatically (`ms-settings:defaultapps?registeredAppUser=BarePDF`).
7. Select BarePDF as default `.pdf` reader in Windows UI.

### File Handler & Command Line Tests
1. Double-click a PDF file containing spaces and Unicode characters in its path (e.g. `C:\Docs\Sample PDF Document (日本語).pdf`).
2. Verify BarePDF opens instantly and displays the first page.
3. Test command-line file opening:
   ```cmd
   "%LOCALAPPDATA%\Programs\BarePDF\BarePDF.exe" "C:\Docs\Test Document.pdf"
   ```
4. Right-click any `.pdf` file -> **Open with** -> Confirm `BarePDF` is listed.

---

## Portable Package Verification

1. [Open the latest GitHub Release](https://github.com/Woffluon/BarePDF/releases/latest) and download its versioned `BarePDF-Portable-x64-v<version>.zip` asset.
2. Extract to desktop or USB drive (`C:\Users\Public\BarePDF-Portable`).
3. Run `BarePDF.exe` directly without installation.
4. Verify application runs fully offline without writing installer registry keys.

---

## Uninstallation Verification

1. Go to Windows **Settings** -> **Apps** -> **Installed apps** -> **BarePDF** -> **Uninstall**.
2. Run uninstaller.
3. Verify `%LOCALAPPDATA%\Programs\BarePDF` is removed.
4. Verify registry key `HKCU\Software\Classes\BarePDF.Document.1` is deleted.
5. Verify user documents remain untouched.
