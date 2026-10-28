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
| **Idle Memory Footprint**<br/>~125 MB Private Bytes, ~114 MB Working Set | README.md / README.tr.md ("Settled Memory") | Re-measurement with `scripts/benchmark-memory-and-startup.ps1` (2026-10-28) | **REPLACED** | The previously cited `docs/benchmarks/2026-10-10-5526791.json` values (2.51 MB Private Bytes / 13.64 MB Working Set) belong to a process blocked on the fatal "PDF engine could not be loaded" dialog (missing sibling `pdfium.dll` during the benchmark run), not to the idle app state. Re-measured with the pinned PDFium (chromium/7988) staged, Windows 11 Pro / Ryzen 5 5500, release build: Private Bytes = 124.98 MB, Working Set = 113.74 MB. |
| **Document Loaded Memory**<br/>~122 MB Private Bytes, ~109 MB Working Set | README.md / README.tr.md ("Settled Memory") | Re-measurement with `scripts/benchmark-memory-and-startup.ps1` (2026-10-28) | **REPLACED** | Loaded `assets/barepdf-welcome.pdf` after staging pinned PDFium. Measured: Private Bytes = 121.98 MB, Working Set = 109.06 MB. |
| **Startup Latency**<br/>Document open → first page bitmap ready ≈ 300–330 ms warm | README.md / README.tr.md ("Startup Latency") | `BAREPDF_PROFILE_FILE` `first_bitmap_ms` via `scripts/benchmark-memory-and-startup.ps1` (2026-10-28) | **REPLACED** | Earlier "process initialization to active window in 65–95 ms (warm) / 140–190 ms (cold)" no current instrument measures and no current signal could verify (different metric definition). Re-measured: hot p50 = 299.02 ms, p95 = 304.67 ms (5 samples), cold = 302.8 ms (Windows 11 Pro, AMD Ryzen 5 5500, release build, pinned PDFium chromium/7988). |
| **Idle Background CPU: 0.0%** | docs/BENCHMARKS.md, docs/benchmarks/*.json | `docs/benchmarks/2026-10-28-r5500-fixed-harness.json` | **VERIFIED** | Sampled over 5s idle duration: Measured 0.00% CPU. Verified no background thread spin. |
| **Memory Budget Partitions**<br/>(Raw RGBA, Slint UI images, page image cap, thumbnails) | docs/BENCHMARKS.md, docs/developer/rendering-pipeline.md | `apps/barepdf/src/presentation/ui.rs`, `crates/barepdf-render/src/memory_budget.rs` | **VERIFIED** | Explicit constants: `RAW_BITMAP_BUDGET` = 128 MB, `UI_IMAGE_CACHE_BUDGET` = 128 MB (zoom-adaptive tiers 32/64/128 MB via `adaptive_cache_budget()`), `PAGE_IMAGE_BUDGET` = 64 MB, `THUMB_IMAGE_BUDGET` = 4 MB. Hardware-adaptive total render budget via `calculate_adaptive_memory_budget()`: <= 1 GB RAM → 64 MB, <= 2 GB → 128 MB, <= 4 GB → 256 MB, <= 8 GB → 384 MB, > 8 GB → 1024 MB. Partitions defined in code. |
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
| **Native Win32 Printing** | Homepage FeatureGrid, docs/user/interface.md | `crates/barepdf-platform-windows/src/ffi/printing.rs` | **VERIFIED** | Direct Win32 `PrintDlgW` / `StartDocW` / GDI printing pipeline. |
| **Windows Shell Thumbnail Provider** | Homepage FeatureGrid, docs/developer/packaging.md | `crates/barepdf-thumbnail/src/lib.rs` | **VERIFIED** | Windows Shell COM extension implementing `IThumbnailProvider` and `IInitializeWithStream`. |
| **Command Palette HUD (Ctrl+K)** | Homepage FeatureGrid, docs/user/keyboard-shortcuts.md | `crates/barepdf-ui/ui/main_window.slint`, `apps/barepdf` | **VERIFIED** | Slint HUD popup handling page navigation, search, and tool activation via shortcut. |
| **Vector Annotations & Signatures** | Homepage FeatureGrid, docs/user/reading.md | `crates/barepdf-core/src/types.rs`, `barepdf-pdf/src/operations.rs` | **VERIFIED** | Glyph highlight, freehand ink with undo/redo stack, and flattened signature stamping. |
