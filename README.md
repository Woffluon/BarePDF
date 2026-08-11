# BarePDF

> **Bare, Fast, Yours.**

BarePDF is an open-source, ultra-lightweight PDF reader for Windows 10 and 11.

It provides the speed and low resource usage of SumatraPDF paired with a clean desktop interface built with Rust and Slint.

## Core Principles

- **Low Overhead**: Demand-driven rendering, bounded bitmap caches, and negligible settled idle CPU usage.
- **Offline & Private**: Zero telemetry, zero analytics, zero AI clutter, zero account requirements.
- **Demand-Driven Rendering**: Automatically bounded raw/UI bitmap caches and generation-token cancellation for fast, smooth scrolling.
- **Modern Native UI**: Slint UI framework delivering dark/light theme, high-DPI scaling, and responsive controls.
- **Modular Workspace**: Clean architectural boundaries isolating platform code, render pipeline, PDF engine, and domain logic.

## Downloads & Installation

### Windows Setup Installer
[Open the latest GitHub Release](https://github.com/Woffluon/BarePDF/releases/latest) and download its versioned `BarePDF-Setup-x64-v<version>.exe` asset.
- Installs to `%LOCALAPPDATA%\Programs\BarePDF` (no administrator rights required).
- Registers BarePDF in Windows "Open with" and Default Apps.
- Offers option during setup to launch Windows Default Apps settings to select BarePDF as your default `.pdf` reader.

### Portable Package
[Open the latest GitHub Release](https://github.com/Woffluon/BarePDF/releases/latest) and download its versioned `BarePDF-Portable-x64-v<version>.zip` asset.
- Extract anywhere and run `BarePDF.exe` directly.
- Requires no installation or registry modifications.

### Verifying Release Checksums
```powershell
$Installer = Get-Item .\BarePDF-Setup-x64-v*.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $Installer.FullName
# Compare with BarePDF-v<version>-SHA256SUMS.txt from the same GitHub Release
```

## Update authenticity policy

BarePDF stable releases include an Ed25519-signed update manifest. The corresponding public key is pinned in the application, while the private key is restricted to the release workflow's encrypted GitHub Actions secret.

- Installers are checked against the signed manifest's immutable GitHub URL, size, SHA-256, and embedded product version before launch.
- Release automation fails closed when the manifest signing key is unavailable or does not match the pinned public key.
- The installer is not Authenticode-signed, so Windows may display an unknown-publisher warning.
- Privacy: This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. Update checks remain disabled until the user explicitly opts in.

## Architecture Workspace

- `crates/barepdf-core`: Engine-agnostic domain models, page pairing calculations, viewport math, and user preferences.
- `crates/barepdf-pdf`: Abstract `PdfBackend` / `PdfDocument` traits and PDFium engine adapter.
- `crates/barepdf-render`: Priority-queued single PDFium actor and LRU bitmap memory cache.
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
# Use this exact message for preparation, validation, and the eventual commit.
$CommitMessage = "feat(scope): describe the releasable change"
powershell -File scripts/prepare-version.ps1 -Message $CommitMessage

# Run workspace checks and unit tests
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked

# Run PowerShell release scripts
powershell -File packaging/windows/scripts/validate-version.ps1 -Message $CommitMessage
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
| Next Page | `PageDown` / Right Arrow |
| Previous Page | `PageUp` / Left Arrow |
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
