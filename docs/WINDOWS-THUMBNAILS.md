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

## Safety, Build Policy & Crash Isolation

- **COM FFI `catch_unwind` Guard**: Every exported COM entry point (`DllGetClassObject`, `DllCanUnloadNow`) and vtable method (`IClassFactory::CreateInstance`, `IClassFactory::LockServer`, `IInitializeWithStream::Initialize`, `IThumbnailProvider::GetThumbnail`) wraps its body in `std::panic::catch_unwind` so Rust panics are converted into `E_UNEXPECTED` (or `S_FALSE` in `DllCanUnloadNow`) instead of unwinding across the `extern "system"` COM FFI boundary (which would be Undefined Behavior).
- **Release Profile & Panic Strategy (`panic = "abort"` vs. `panic = "unwind"`)**: Standard unified release builds (`cargo build --release -p barepdf -p barepdf-thumbnail --locked`) compile with `[profile.release]` (`panic = "abort"`). Under `panic = "abort"`, any panic terminates the process immediately rather than unwinding—because Windows Explorer hosts `IInitializeWithStream` thumbnail providers out-of-process inside `dllhost.exe` (`DllSurrogate`), an abort or native PDFium fault is isolated to the surrogate process without crashing `explorer.exe`. When unwind recovery inside the COM server is explicitly needed, Cargo's `[profile.release-unwind]` (`panic = "unwind"`) enables `catch_unwind` to intercept unwinding Rust panics at the FFI boundary; native access violations and OOM aborts remain process-fatal in all profiles.
- **PE `VERSIONINFO` Metadata**: `crates/barepdf-thumbnail/build.rs` embeds Windows PE `VERSIONINFO` metadata into `barepdf_thumbnail.dll` (`BarePDF.Thumbnail.dll`) via `winres` and watches `CARGO_PKG_VERSION` so file and product versions always match `[workspace.package].version`.
- Invalid, missing, or password-protected PDFs return `E_FAIL` without UI popups, allowing Windows Explorer to fallback gracefully to the standard document icon.
- `pdfium.dll` path is deterministically resolved relative to `BarePDF.Thumbnail.dll` module directory, avoiding DLL search path vulnerabilities.

## Verification & Testing

### Development Verification
1. Run `cargo test --workspace --all-features --locked`.
2. Build release binaries: `cargo build --release -p barepdf -p barepdf-thumbnail --locked`.
3. Run release staging script: `powershell -File packaging/windows/scripts/stage-release.ps1`.
4. Compile Inno Setup installer: `powershell -File packaging/windows/scripts/build-installer.ps1`.
5. On an account with existing BarePDF registration, install-directory, or shortcut state, compile the isolated test package without changing user installation state: `powershell -File packaging/windows/scripts/validate-installer.ps1 -CompileOnly`.
6. On a clean disposable Windows account or CI runner, run `powershell -File packaging/windows/scripts/validate-installer.ps1` to verify the native 64-bit `InprocServer32` path, `ThreadingModel=Apartment`, ShellEx mappings, installed thumbnail/PDFium DLLs, legacy 32-bit CLSID removal, and uninstall cleanup.
7. Confirm Explorer's **Always show icons, never thumbnails** option is off (`IconsOnly=0`), then install and inspect PDF file thumbnails on Windows Desktop and File Explorer.

Explorer's Alt+P preview pane uses `IPreviewHandler` and is outside this thumbnail provider's scope.
