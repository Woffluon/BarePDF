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
   `validate-version.ps1` verifies the root Cargo product version, inherited workspace package versions, Conventional Commit bump, Inno Setup configuration, and release tag when present.
2. **Staging Executable & Dependencies**:
   `stage-release.ps1` builds the release executable (`cargo build --release`) and copies `BarePDF.exe`, PDFium DLLs, licenses, and assets into `target/release/staged/`.
3. **Building Portable Package**:
   `build-portable.ps1` compresses staged files into `target/release/portable/BarePDF-Portable-x64-v<version>.zip`.
4. **Compiling Windows Installer**:
   `build-installer.ps1` compiles `packaging/windows/installer/BarePDF.iss` via Inno Setup (`ISCC.exe`) into `target/release/installer/BarePDF-Setup-x64-v<version>.exe`.
5. **Validating Installer & Registry Config**:
   `validate-installer.ps1` verifies the generated setup executable and registry key entries.
6. **Generating Checksums**:
   `generate-checksums.ps1` hashes the versioned installer and portable package, then copies them into `target/release/artifacts/` together with stable `BarePDF-Setup-x64.exe` and `BarePDF-Portable-x64.zip` aliases. It writes versioned and stable checksum manifests plus `latest.json` in the same directory.

## GitHub Actions Release Workflow

The `.github/workflows/release.yml` workflow starts automatically after a successful `CI` push run on `main`, or through a manual dispatch. The Ed25519 manifest signing key must match the public key pinned in the repository before release discovery begins; a missing or mismatched key fails closed.

Discovery validates every first-parent commit since the latest stable tag, selects the newest unreleased product version, and tags the exact `main` snapshot whose CI run triggered publication. Superseded intermediate versions are intentionally skipped instead of shipping obsolete binaries. An existing matching release is accepted idempotently; conflicting or incomplete releases fail. After publication, that stable version becomes GitHub's latest release and the Pages workflow is explicitly dispatched.

Public documentation uses the stable `releases/latest/download/BarePDF-Setup-x64.exe`, `BarePDF-Portable-x64.zip`, and `BarePDF-SHA256SUMS.txt` aliases published with every release. Version bumps therefore require no repetitive download-link edits.
