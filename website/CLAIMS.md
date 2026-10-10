# BarePDF Claims Ledger & Verifiability Audit

This document records the verification source and truth status of every public claim, specification, metric, and feature statement published across the BarePDF website (`website/`) and documentation (`docs/`).

Per the BarePDF Engineering Contract and Evidence Policy:
- No speculative, fabricated, or mock numbers are permitted.
- If a metric cannot be traced to an immutable commit hash, an explicit code definition, or a live authenticated API response, it must be removed.
- In fallback states (e.g. GitHub API rate limits), metrics are hidden completely rather than presenting stale or estimated baselines.

---

## 1. Performance & Benchmark Metrics

| Claim / Metric | Public Location | Verification Source | Status | Exact Evidence |
| :--- | :--- | :--- | :---: | :--- |
| **Idle Memory Footprint**<br/>~2.5 MB Private Bytes, ~13.6 MB Working Set | docs/BENCHMARKS.md, docs/benchmarks/*.json | `docs/benchmarks/2026-10-10-5526791.json` | **VERIFIED** | Real execution of `scripts/benchmark-memory-and-startup.ps1` on Windows 11 Pro, AMD Ryzen 5 5500. Measured: Private Bytes = 2.51 MB, Working Set = 13.64 MB. |
| **Document Loaded Memory**<br/>~2.5 MB Private Bytes, ~13.6 MB Working Set | docs/BENCHMARKS.md, docs/benchmarks/*.json | `docs/benchmarks/2026-10-10-5526791.json` | **VERIFIED** | Loaded `assets/barepdf-welcome.pdf`. Measured: Private Bytes = 2.54 MB, Working Set = 13.62 MB. |
| **Idle Background CPU: 0.0%** | docs/BENCHMARKS.md, docs/benchmarks/*.json | `docs/benchmarks/2026-10-10-5526791.json` | **VERIFIED** | Sampled over 5s idle duration: Measured 0.00% CPU. Verified no background thread spin. |
| **Memory Budget Partitions**<br/>(Raw RGBA, Slint Textures, Thumbnails) | docs/BENCHMARKS.md, docs/developer/architecture.md | `crates/barepdf-render/src/memory_budget.rs` | **VERIFIED** | Explicit constants: `calculate_adaptive_memory_budget()`, hardware RAM inspect: <= 4GB -> 256MB, 4-8GB -> 384MB, >8GB -> 1024MB. Partitions defined in code. |
| **Unsubstantiated Memory Ranges**<br/>(e.g. "38-52 MB standard", "85-128 MB visual") | Legacy docs/BENCHMARKS.md | None committed | **REPLACED** | Legacy unproven ranges replaced with measured data and clear hardware spec. |

---

## 2. Release & Download Metrics

| Claim / Metric | Public Location | Verification Source | Status | Exact Evidence |
| :--- | :--- | :--- | :---: | :--- |
| **Total Download Counts** | website Hero, DownloadMetricCard, download.astro | Live GitHub Releases API (`/repos/Woffluon/BarePDF/releases`) | **VERIFIED (LIVE)** | Fetched build-time from GitHub API asset `download_count`. Explicit footnote: counted per asset retrieved, not unique devices. |
| **Download Baseline Fallback**<br/>(149 / 104 / 45 / 5) | `website/src/lib/github.ts` | Hardcoded `VERIFIED_DOWNLOAD_BASELINE` | **REMOVED** | Hardcoded numbers deleted. If GitHub API is unavailable, the metric card gracefully hides. No mock data. |
| **Zero Telemetry Pings (0)** | DownloadMetricCard | `crates/barepdf-platform-windows`, `apps/barepdf` | **VERIFIED** | Code contains 0 telemetry libraries, 0 analytics endpoints, 0 background pings. Opt-in updater is the only network component. |

---

## 3. Architecture & Functional Guarantees

| Claim / Feature | Public Location | Verification Source | Status | Exact Evidence |
| :--- | :--- | :--- | :---: | :--- |
| **Demand-Driven Rendering** | Homepage, docs/developer/rendering-pipeline.md | `crates/barepdf-render/src/scheduler.rs` | **VERIFIED** | Visible viewport page queue with LRU cache eviction and cancellation of superseded tokens. |
| **Generation-Token Cancellation** | docs/BENCHMARKS.md, docs/developer/rendering-pipeline.md | `crates/barepdf-render/src/scheduler.rs` | **VERIFIED** | `bump_generation()`, atomic token increments reject in-flight stale renders on user scroll. |
| **100% Offline by Default** | Homepage Philosophy, docs/user/getting-started.md | `apps/barepdf/src/infrastructure/update/` & `AGENTS.md §8` | **VERIFIED** | `update_checks_enabled` defaults to `false`. No socket or HTTP requests occur until user opts in via settings. |
| **Local CPU Processing Only** | Homepage FeatureGrid, docs/user/reading.md | `crates/barepdf-pdf/src/operations.rs` | **VERIFIED** | Merge, split, rotate, delete, crop, and reorder run directly inside local process via PDFium C-FFI. |
| **Native Win32 Printing** | Homepage FeatureGrid, docs/user/interface.md | `crates/barepdf-platform-windows/src/ffi/printing.rs` | **VERIFIED** | Direct Win32 `PrintDlgExW` / `StartDocW` / GDI printing pipeline. |
| **Windows Shell Thumbnail Provider** | Homepage FeatureGrid, docs/developer/packaging.md | `crates/barepdf-thumbnail/src/lib.rs` | **VERIFIED** | Windows Shell COM extension implementing `IThumbnailProvider` and `IInitializeWithStream`. |
| **Command Palette HUD (Ctrl+K)** | Homepage FeatureGrid, docs/user/keyboard-shortcuts.md | `crates/barepdf-ui/ui/main_window.slint`, `apps/barepdf` | **VERIFIED** | Slint HUD popup handling page navigation, search, and tool activation via shortcut. |
| **Vector Annotations & Signatures** | Homepage FeatureGrid, docs/user/reading.md | `crates/barepdf-core/src/annotations.rs`, `barepdf-pdf/src/operations.rs` | **VERIFIED** | Glyph highlight, freehand ink with undo/redo stack, and flattened signature stamping. |
