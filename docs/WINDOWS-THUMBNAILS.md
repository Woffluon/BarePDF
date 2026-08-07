# BarePDF Windows Explorer PDF Thumbnail Provider

This document describes the design, architecture, COM registration, and testing procedures for BarePDF's Windows Shell PDF Thumbnail Provider (`BarePDF.Thumbnail.dll`).

## Architecture

The thumbnail provider is a lightweight Windows COM DLL crate (`crates/barepdf-thumbnail`) isolated from the main BarePDF application UI and async runtime.

- **COM Server Interface**: Implements `IThumbnailProvider` and `IInitializeWithStream`.
- **Stream Initialization**: Prefers `IInitializeWithStream` to preserve native Windows Explorer process isolation (surrogate host process `dllhost.exe`).
- **PDF Engine**: Uses `pdfium-render` to load page 1 and render it into a 32-bit Win32 DIB Section (`HBITMAP`, `WTSAT_ARGB`).
- **Aspect Ratio**: Preserves document page aspect ratio for requested Explorer dimensions (`cx`).

## Windows Registration Architecture

### COM CLSID
- **Class Identifier**: `{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}`
- **Threading Model**: `Apartment` (STA)

### Registry Keys
1. **ProgID Shell Extension**:
   `HKCU\Software\Classes\BarePDF.Document.1\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` -> `{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}`
2. **Native TypeOverlay Branding**:
   `HKCU\Software\Classes\BarePDF.Document.1\TypeOverlay` -> `"{app}\BarePDF.exe,0"`
3. **COM Class Registration**:
   `HKCU\Software\Classes\CLSID\{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}\InprocServer32` -> `"{app}\BarePDF.Thumbnail.dll"`

## Native TypeOverlay Mechanism

Windows Shell automatically overlays the BarePDF application icon in the lower-right corner of the thumbnail preview. The icon is **not** manually composited or painted into the PDF page bitmap by BarePDF.

## Safety & Crash Isolation

- All COM vtable calls and exports wrap execution in `std::panic::catch_unwind`.
- Invalid, missing, or password-protected PDFs return `E_FAIL` without UI popups, allowing Windows Explorer to fallback gracefully to the standard document icon.
- `pdfium.dll` path is deterministically resolved relative to `BarePDF.Thumbnail.dll` module directory, avoiding DLL search path vulnerabilities.

## Verification & Testing

### Development Verification
1. Run `cargo test --workspace --all-features`.
2. Build release workspace: `cargo build --workspace --release`.
3. Run release staging script: `powershell -File packaging/windows/scripts/stage-release.ps1`.
4. Compile Inno Setup installer: `powershell -File packaging/windows/scripts/build-installer.ps1`.
5. Install and verify PDF thumbnails on Windows Desktop and File Explorer.
