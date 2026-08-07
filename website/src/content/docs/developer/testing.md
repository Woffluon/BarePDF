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

## Version Consistency Check

Verify version synchronization across `apps/barepdf/Cargo.toml`, `BarePDF.iss`, and repository scripts:

```powershell
powershell -File packaging/windows/scripts/validate-version.ps1
```
