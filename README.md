# BarePDF

BarePDF is an open-source, fast, modern, ultra-lightweight PDF reader designed for Windows 10/11.

It provides the raw speed and low resource usage of SumatraPDF while delivering a clean, modern desktop visual interface built with Rust and Slint.

## Core Principles

- **Ultra Low Overhead**: <60MB idle memory, ~0% idle CPU, fast document opening.
- **Offline & Private**: Zero telemetry, zero analytics, zero AI clutter, zero account requirements.
- **Demand-Driven Rendering**: LRU byte-budgeted bitmap cache (default 256MB) and generation token cancellation for fast smooth scrolling.
- **Modern Native UI**: Slint UI framework delivering dark/light theme, high-DPI scaling, and responsive controls.
- **Modular Workspace**: Clean architectural boundaries isolating platform code, render pipeline, PDF engine, and domain logic.

## Downloads & Installation

### Windows Setup Installer
Download `BarePDF-Setup-x64-v1.0.0.exe` from GitHub Releases.
- Installs to `%LOCALAPPDATA%\Programs\BarePDF` (no administrator rights required).
- Registers BarePDF in Windows "Open with" and Default Apps.
- Offers option during setup to launch Windows Default Apps settings to select BarePDF as your default `.pdf` reader.

### Portable Package
Download `BarePDF-Portable-x64-v1.0.0.zip` from GitHub Releases.
- Extract anywhere and run `BarePDF.exe` directly.
- Requires no installation or registry modifications.

### Verifying Release Checksums
```powershell
Get-FileHash -Algorithm SHA256 .\BarePDF-Setup-x64-v1.0.0.exe
# Compare with SHA256SUMS.txt
```

## Architecture Workspace

- `crates/barepdf-core`: Engine-agnostic domain models, page pairing calculations, viewport math, and user preferences.
- `crates/barepdf-pdf`: Abstract `PdfBackend` / `PdfDocument` traits and PDFium engine adapter.
- `crates/barepdf-render`: Priority-queued render scheduler, worker pool, and LRU bitmap memory cache.
- `crates/barepdf-platform`: Abstract traits for OS file dialogs, clipboard, and printing.
- `crates/barepdf-platform-windows`: Native Win32 file dialogs (`rfd`), clipboard (`arboard`), and print spooling.
- `crates/barepdf-ui`: Modern Slint desktop views, theme integration, view models, and keyboard shortcuts.
- `apps/barepdf`: Main binary entry point, logging bootstrap, and event loop wiring.

## Windows Default PDF Reader Integration

BarePDF adheres strictly to Windows Default Apps guidelines:
1. Registers ProgID `BarePDF.Document.1` and application capabilities under `HKCU`.
2. Registers BarePDF in Windows "Open with" and `RegisteredApplications`.
3. Asks during setup if you wish to set BarePDF as default. If "Yes", setup launches `ms-settings:defaultapps?registeredAppUser=BarePDF` upon exit.
4. **No Protected Registry Manipulation**: BarePDF never forces default associations or modifies protected `UserChoice` registry keys.

## Build & Release Automation

```powershell
# 1. Run workspace checks and unit tests
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked

# 2. Run PowerShell release scripts
powershell -File packaging/windows/scripts/validate-version.ps1
powershell -File packaging/windows/scripts/stage-release.ps1
powershell -File packaging/windows/scripts/build-portable.ps1
powershell -File packaging/windows/scripts/build-installer.ps1
powershell -File packaging/windows/scripts/validate-installer.ps1
powershell -File packaging/windows/scripts/generate-checksums.ps1
```

## Shortcuts

| Action | Shortcut |
| --- | --- |
| Open Document | `Ctrl+O` |
| Next Page | `PageDown` / `>` |
| Previous Page | `PageUp` / `<` |
| Zoom In | `+` / `Ctrl++` |
| Zoom Out | `-` / `Ctrl+-` |
| Fit Width | `Fit Width` button |
| Fit Page | `Fit Page` button |
| Full Screen | `F11` |
| Presentation Mode | `F5` |
| Unlock Password | Enter password in modal |

## Website

The official BarePDF website and documentation are built with Astro and located under [`./website`](./website).

## License

BarePDF is open source software licensed under the MIT License.
