---
title: "Building from Source"
description: "Step-by-step instructions for compiling BarePDF locally on Windows."
category: "developer"
order: 2
---

# Building from Source

Follow this guide to set up your build environment and compile BarePDF from source.

## Prerequisites

1. **Rust Toolchain**: Install stable Rust (Edition 2021+) via [rustup.rs](https://rustup.rs/).
   ```powershell
   rustup update stable
   ```
2. **C++ Build Tools**: Visual Studio 2022 C++ build tools or Windows SDK (required for native Win32 linking).
3. **Inno Setup** (Optional, for installer generation):
   ```powershell
   choco install innosetup --no-progress -y
   ```

## Development Build

To compile and launch the debug build:

```powershell
cargo run --package barepdf
```

To run checks across all workspace crates:

```powershell
cargo check --workspace --all-targets --all-features
```

## Release Build

To build the optimized production binary with LTO, symbol stripping, and `opt-level = 3`:

```powershell
cargo build --workspace --release --locked
```

The compiled executable will be output to:

```text
target/release/BarePDF.exe
```

## Running Full Packaging Pipeline

To execute the full release verification pipeline locally:

```powershell
powershell -File packaging/windows/scripts/validate-version.ps1
powershell -File packaging/windows/scripts/stage-release.ps1
powershell -File packaging/windows/scripts/build-portable.ps1
powershell -File packaging/windows/scripts/build-installer.ps1
powershell -File packaging/windows/scripts/validate-installer.ps1
powershell -File packaging/windows/scripts/generate-checksums.ps1
```
