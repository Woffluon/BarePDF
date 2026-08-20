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

Build the baseline in a clean worktree at `HEAD`, then retain that release executable outside the
repository so a candidate build cannot overwrite it. The baseline and candidate must use the same
toolchain, `Cargo.lock`, target triple, release profile, and effective `RUSTFLAGS`/
`CARGO_ENCODED_RUSTFLAGS`. Keep the fixture and result JSON files outside the repository:

```powershell
# Run this in a clean HEAD worktree, then retain the resulting executable.
$baselineExecutable = 'C:\benchmarks\barepdf-clean-head.exe'
cargo build --release --locked
Copy-Item .\target\release\barepdf.exe $baselineExecutable
powershell -File scripts/profile-barepdf.ps1 `
  -ExecutablePath $baselineExecutable `
  -FixturePath C:\benchmarks\barepdf-10-page.pdf `
  -FixtureName text-10-v1 `
  -FixtureClass 10-page `
  -ExpectedSha256 <64-character-sha256> `
  -Runs 5 `
  -DurationSeconds 60 `
  -ResultPath C:\benchmarks\barepdf-10-page-baseline.json
```

Keep fixture PDFs outside the repository. Record their source or deterministic generation method,
license, byte size, and SHA-256 beside benchmark results. Use these stable fixture classes:

| Class | Required fixture property |
| --- | --- |
| `10-page` | Exactly 10 pages; normal text/document startup case. |
| `500-page` | Exactly 500 pages; long-document navigation and memory case. |
| `visual-heavy` | Image-heavy pages representative of costly raster work. |
| `boundary` | Document at a tested page-count, file-size, dimension, or outline limit; record the exact boundary. |

Build the candidate with those same build inputs, retain it at a different path, then compare it
against the baseline with the same fixture and measurement duration:

```powershell
$candidateExecutable = 'C:\benchmarks\barepdf-candidate.exe'
cargo build --release --locked
Copy-Item .\target\release\barepdf.exe $candidateExecutable
powershell -File scripts/profile-barepdf.ps1 `
  -ExecutablePath $candidateExecutable `
  -FixturePath C:\benchmarks\barepdf-10-page.pdf `
  -FixtureName text-10-v1 `
  -FixtureClass 10-page `
  -ExpectedSha256 <64-character-sha256> `
  -Runs 5 `
  -DurationSeconds 60 `
  -BaselinePath C:\benchmarks\barepdf-10-page-baseline.json `
  -ResultPath C:\benchmarks\barepdf-10-page-candidate.json
```

The saved schema-v2 JSON deliberately records fixture identity, release-profile measurement settings,
and a machine description without local PDF, executable, repository, or computer paths/names. It also
records the executable SHA-256/size, Cargo.lock SHA-256, full `rustc -Vv` identity, `cargo -V`, target
triple, effective flags, and source commit/dirty state. Executable identity and source state are
evidence only: they may differ between baseline and candidate. A baseline is comparable only when the
fixture hash and byte size, release profile, duration, sample interval, OS/CPU/core and memory
description, display, power plan, requested run count, toolchain, target triple, effective flags, and
Cargo.lock SHA-256 match. Missing provenance and legacy schema-v1 results produce no gate result.

For supported metrics, the script reports p50/p95 and compares p95 against these gates. A comparable
failure exits with code `2`; unavailable metrics are `NOT EVALUATED`, not passes.
Process measurements require at least five samples on both sides of a comparison.

| Metric | Gate |
| --- | --- |
| First low-resolution bitmap | Baseline +5% |
| Idle CPU | Baseline +0.2 percentage points |
| Idle working set | Baseline +2 MiB |
| Peak working set | Baseline +5% |
| Installer package | Baseline +2% |
| Portable package | Baseline +2% |

Installer and portable size gates apply only when exactly one versioned package is present in each
release package directory; otherwise their metric is unsupported. Cold start, native-DPI bitmap, and
UI callback timing remain unsupported until the application emits their distinct signals. Timer
wake-up counts still require a WPR/WPA CPU Usage and Thread Activity trace. Process private-memory
figures are reported for diagnosis but do not stand in for unsupported timing or UI metrics.

## Version Consistency Check

Verify version synchronization across `apps/barepdf/Cargo.toml`, `BarePDF.iss`, and repository scripts:

```powershell
powershell -File packaging/windows/scripts/validate-version.ps1
```
