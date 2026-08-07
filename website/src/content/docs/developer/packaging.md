---
title: "Packaging & Release Pipeline"
description: "How BarePDF compiles Windows Setup installers, portable packages, and release manifests."
category: "developer"
order: 7
---

# Packaging & Release Pipeline

BarePDF automates Windows binary packaging, installer compilation, portable ZIP creation, and SHA256 checksum manifest generation using PowerShell scripts in `packaging/windows/scripts/`.

## Automated Packaging Pipeline

The release pipeline executes in the following sequence:

1. **Version Validation**:
   `validate-version.ps1` verifies matching version strings in `apps/barepdf/Cargo.toml` and Inno Setup `BarePDF.iss`.
2. **Staging Executable & Dependencies**:
   `stage-release.ps1` builds the release executable (`cargo build --release`) and copies `BarePDF.exe`, PDFium DLLs, licenses, and assets into `target/release/staged/`.
3. **Building Portable Package**:
   `build-portable.ps1` compresses staged files into `target/release/artifacts/BarePDF-Portable-x64-v1.0.0.zip`.
4. **Compiling Windows Installer**:
   `build-installer.ps1` compiles `packaging/windows/installer/BarePDF.iss` via Inno Setup (`ISCC.exe`) into `target/release/artifacts/BarePDF-Setup-x64-v1.0.0.exe`.
5. **Validating Installer & Registry Config**:
   `validate-installer.ps1` verifies the generated setup executable and registry key entries.
6. **Generating Checksums**:
   `generate-checksums.ps1` calculates SHA256 hashes for all release artifacts and outputs `target/release/artifacts/BarePDF-v1.0.0-SHA256SUMS.txt`.

## GitHub Actions Release Workflow

The `.github/workflows/release.yml` workflow triggers on pushes to `main` or manual dispatch. It executes the full packaging pipeline on `windows-latest`, uploads build artifacts, and publishes the release on GitHub.
