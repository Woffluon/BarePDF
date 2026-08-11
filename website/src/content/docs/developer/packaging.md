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

The `.github/workflows/release.yml` workflow starts after a successful `CI` push run on `main`, or through a manual dispatch. `RELEASE_AUTOMATION_ENABLED=true` is the fail-closed gate; signing secrets and the pinned signer fingerprint must pass preflight before release discovery begins.

Discovery walks the first-parent backlog since the latest stable tag and selects every commit whose Conventional Commit message changes the product version. Eligible commits are validated and published in source order with one immutable `vMAJOR.MINOR.PATCH` release per version. Existing matching releases are accepted idempotently; conflicting or incomplete releases fail. After publishing the backlog, the newest stable version becomes GitHub's latest release and the Pages workflow is explicitly dispatched.

Public documentation links to the latest release page until `v1.1.0` has been published with stable alias assets. Switching public links to `releases/latest/download/BarePDF-Setup-x64.exe` and the matching portable/checksum aliases is a separate documentation-only commit after that bootstrap release exists.
