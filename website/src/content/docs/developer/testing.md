---
title: "Testing & Code Quality"
description: "Commands and guidelines for running cargo tests, Clippy lints, and format checks."
category: "developer"
order: 6
---

# Testing & Code Quality

BarePDF enforces strict code quality, zero compiler warnings, and unit/integration test coverage across all workspace crates.

## Running Tests

Run unit and integration tests across all workspace members:

```powershell
cargo test --workspace --all-features --locked
```

## Running Clippy Lints

BarePDF code must pass strict Clippy verification without warnings (`-D warnings`):

```powershell
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

## Format Checking

Run `rustfmt` format validation across all `.rs` files:

```powershell
cargo fmt --all --check
```

## Performance Baselines

Build the release executable, then profile at least five fresh processes with one fixed,
locally stored PDF fixture:

```powershell
cargo build --release --locked
powershell -File scripts/profile-barepdf.ps1 `
  -FixturePath C:\benchmarks\barepdf-10-page.pdf `
  -FixtureName text-10-v1 `
  -FixtureClass 10-page `
  -ExpectedSha256 <64-character-sha256> `
  -Runs 5 `
  -DurationSeconds 60
```

Keep fixture PDFs outside the repository. Record their source or deterministic generation method,
license, byte size, and SHA-256 beside benchmark results. Use these stable fixture classes:

| Class | Required fixture property |
| --- | --- |
| `10-page` | Exactly 10 pages; normal text/document startup case. |
| `500-page` | Exactly 500 pages; long-document navigation and memory case. |
| `visual-heavy` | Image-heavy pages representative of costly raster work. |
| `boundary` | Document at a tested page-count, file-size, dimension, or outline limit; record the exact boundary. |

Compare before/after results only on the same release profile, fixture hash, machine, display, and
power plan. The script prints p50/p95 for available process metrics and reports unsupported metrics
explicitly. Cold start and native-DPI bitmap timing need separate application signals; timer wake-up
counts need a WPR/WPA CPU Usage and Thread Activity trace.

## Version Consistency Check

Verify version synchronization across `apps/barepdf/Cargo.toml`, `BarePDF.iss`, and repository scripts:

```powershell
powershell -File packaging/windows/scripts/validate-version.ps1
```
