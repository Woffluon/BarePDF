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
1. **Native 64-bit ProgID Shell Extension**:
   `HKCU\Software\Classes\BarePDF.Document.1\ShellEx\{E357FCCD-A995-4576-B01F-234630154E96}` -> `{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}`
2. **Native TypeOverlay Branding**:
   `HKCU\Software\Classes\BarePDF.Document.1\TypeOverlay` -> `"{app}\BarePDF.exe,0"`
3. **Native 64-bit COM Class Registration**:
   `HKCU\Software\Classes\CLSID\{4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B}\InprocServer32` -> `"{app}\BarePDF.Thumbnail.dll"`

The installer runs in x64-compatible 64-bit mode so native Explorer resolves the AMD64 in-process server. During upgrade it deletes only BarePDF's legacy private CLSID from the 32-bit HKCU registry view; uninstall removes the native COM and ShellEx registrations.

## Native TypeOverlay Mechanism

Windows Shell automatically overlays the BarePDF application icon in the lower-right corner of the thumbnail preview. The icon is **not** manually composited or painted into the PDF page bitmap by BarePDF.

## Safety & Crash Isolation

- The staged thumbnail DLL uses Cargo's `release-unwind` profile. COM vtable calls and exports
  convert unwind-capable Rust panics to `E_UNEXPECTED`; native access violations and OOM aborts
  remain process-fatal and are not recoverable by `catch_unwind`.
- Invalid, missing, or password-protected PDFs return `E_FAIL` without UI popups, allowing Windows Explorer to fallback gracefully to the standard document icon.
- `pdfium.dll` path is deterministically resolved relative to `BarePDF.Thumbnail.dll` module directory, avoiding DLL search path vulnerabilities.

## Verification & Testing

### Development Verification
1. Run `cargo test --workspace --all-features`.
2. Build release workspace: `cargo build --workspace --release`.
3. Run release staging script: `powershell -File packaging/windows/scripts/stage-release.ps1`.
4. Compile Inno Setup installer: `powershell -File packaging/windows/scripts/build-installer.ps1`.
5. On an account with existing BarePDF registration, install-directory, or shortcut state, compile the isolated test package without changing user installation state: `powershell -File packaging/windows/scripts/validate-installer.ps1 -CompileOnly`.
6. On a clean disposable Windows account or CI runner, run `powershell -File packaging/windows/scripts/validate-installer.ps1` to verify the native 64-bit `InprocServer32` path, `ThreadingModel=Apartment`, ShellEx mappings, installed thumbnail/PDFium DLLs, legacy 32-bit CLSID removal, and uninstall cleanup.
7. Confirm Explorer's **Always show icons, never thumbnails** option is off (`IconsOnly=0`), then install and inspect PDF file thumbnails on Windows Desktop and File Explorer.

Explorer's Alt+P preview pane uses `IPreviewHandler` and is outside this thumbnail provider's scope.
