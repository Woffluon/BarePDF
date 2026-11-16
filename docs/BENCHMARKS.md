# BarePDF Performance Benchmarks

This report documents the performance characteristics, memory bounds, and resource utilization of BarePDF on Windows (x64). All published figures are derived from automated benchmark scripts executed against release binaries, with raw outputs committed to the repository for reproducible verification.

## Design Philosophy

Traditional PDF readers often load full document structures into memory or maintain unbounded rasterization caches as the reader advances through pages. BarePDF takes a different approach:

1. **Demand-driven rendering with ±2 lookahead:** The rendering pipeline rasters pages intersecting the active viewport plus a `±2` neighbor prefetch window (`first.saturating_sub(2)..=last.saturating_add(2)` at zoom `< 3.0`, and viewport-only at zoom `>= 3.0`) in `apps/barepdf/src/presentation/ui.rs`.
2. **Generation-token cancellation:** Scrolling rapidly increments the viewport generation token. Queued rendering jobs with older generation tokens cancel immediately, saving CPU cycles.
3. **Aggressive window-trimmed LRU caches:** Both `UiImageCache` (`apps/barepdf/src/presentation/state.rs`) and `BitmapCache` (`crates/barepdf-render/src/cache.rs`, driven via `RenderCommand::TrimCache` in `crates/barepdf-render/src/worker.rs`) immediately evict pages outside the active `±2` window on navigation/scroll and purge inactive-document bitmaps when scroll settles.
4. **Native Windows RAM detection & tight byte budgets:** Physical system RAM is detected natively on Windows via Win32 `GlobalMemoryStatusEx` (`crates/barepdf-platform-windows/src/lib.rs`, honoring `BAREPDF_TOTAL_RAM_MB` overrides) and mapped to strict cache ceilings (`crates/barepdf-render/src/memory_budget.rs`).
5. **Zero background telemetry:** The application performs 0 background network calls during document viewing, preventing background polling threads or network latency from impacting UI responsiveness. Update checks are strictly opt-in and disabled by default.

---

## Measured Startup Latency & Memory Footprint

The latest metrics were captured using the expanded automated benchmark harness (`scripts/benchmark-memory-and-startup.ps1`) against a release build of BarePDF (`target/release/barepdf.exe` + pinned sibling `pdfium.dll`).

### Metric Definition

- **`first_bitmap_ms` (startup latency):** elapsed time from issuing a document open request to the first decoded page bitmap being presented in the window (`BAREPDF_PROFILE_FILE`).
- **True Idle (Empty):** process launched with `welcome_manifesto_seen = true` and no document argument, measuring baseline UI/GPU/PDFium footprint without auto-opening `barepdf-welcome.pdf`. (The earlier 2026-10-28 run did not set `welcome_manifesto_seen = true`, so its idle phase opened `barepdf-welcome.pdf`.)
- **Multi-Page & Rapid Scroll:** opens a deterministic 60-page A4 fixture, measures settled memory, drives 30 rapid navigation/scroll steps across pages `1 -> 31`, records peak memory during scroll, and measures settled Private Bytes / Working Set after `TrimCache` + window eviction complete.

### Test Environment
- **Operating System:** Microsoft Windows 11 Pro (64-bit)
- **Processor:** AMD Ryzen 5 5500 (12 logical cores)
- **System RAM:** 16,176 MB physical RAM (detected natively via `GlobalMemoryStatusEx`)
- **Binary:** `target/release/barepdf.exe` (Release Profile, ThinLTO enabled, `codegen-units = 1`)
- **PDFium:** pinned `chromium/7988` build, SHA-256 verified (`03cc8de22238ea9ffbbf41703f8ef8aae77faeab735583815481f5c2c70a63c7`)
- **Fixtures:** `assets/barepdf-welcome.pdf` (2 pages) and generated 60-page A4 benchmark fixture
- **Raw Evidence Archive (Latest):** [`docs/benchmarks/2026-11-16-r5500-aggressive-lru.json`](./benchmarks/2026-11-16-r5500-aggressive-lru.json) / [Markdown summary](./benchmarks/2026-11-16-r5500-aggressive-lru.md)
- **Previous Baseline Archive (2026-10-28):** [`docs/benchmarks/2026-10-28-r5500-fixed-harness.json`](./benchmarks/2026-10-28-r5500-fixed-harness.json) / [Markdown summary](./benchmarks/2026-10-28-r5500-fixed-harness.md)

### Measured Results (2026-11-16 run)

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency (`first_bitmap_ms`)** | **187.56 ms** | <= 350 ms | PASS |
| **Hot Startup Latency, p50 (n=5)** | **186.64 ms** | <= 150 ms | WARN |
| **Hot Startup Latency, p95 (n=5)** | **192.09 ms** | <= 250 ms | PASS |
| **Idle Private Memory (Empty)** | **85.44 MB** | <= 60 MB | WARN |
| **Idle Working Set (Empty)** | **75.74 MB** | <= 150 MB | PASS |
| **Loaded PDF Private Memory (`barepdf-welcome.pdf`)** | **138.07 MB** | <= 80 MB | WARN |
| **Loaded PDF Working Set (`barepdf-welcome.pdf`)** | **115.99 MB** | <= 180 MB | PASS |
| **Multi-Page PDF (60 pages) Private Memory** | **151.58 MB** | <= 95 MB | WARN |
| **Multi-Page PDF (60 pages) Working Set** | **123.95 MB** | <= 180 MB | PASS |
| **Rapid Scroll Peak (pages 1 -> 31, 30 steps)** | **174.98 MB** Private / **147.09 MB** WS | — | — |
| **After Rapid Scroll Private Memory (settled)** | **160.11 MB** | <= 110 MB | WARN |
| **After Rapid Scroll Working Set (settled)** | **135.93 MB** | <= 190 MB | PASS |
| **Idle Background CPU (Empty / Loaded / Post-Scroll)** | **0.00%** | <= 0.1% | PASS |

Detailed hot run samples: min `173.13 ms`, average `185.03 ms`, max `193.38 ms`, p50 `186.64 ms`, p95 `192.09 ms`.
Scroll phase verification: pages `1 -> 31` traversed (`953.12 ms` CPU delta during active rasterization); `14.87 MB` Private Bytes reclaimed between rapid-scroll peak (`174.98 MB`) and post-scroll settle (`160.11 MB`), bounding steady-state growth after scrolling 30 pages to `+8.53 MB` over initial load.

---

## Memory Budget Architecture

BarePDF enforces bounded memory consumption using both hardware-adaptive global budgets and strict internal component partitions defined in `crates/barepdf-render/src/memory_budget.rs`, `apps/barepdf/src/presentation/state.rs`, and `apps/barepdf/src/presentation/ui.rs`.

### Adaptive Hardware Budgeting

On application startup, BarePDF inspects physical RAM via Win32 `GlobalMemoryStatusEx` on Windows (or `/proc/meminfo` on Linux, with `BAREPDF_TOTAL_RAM_MB` override) and sets the worker `BitmapCache` ceiling:

| Total System RAM | Adaptive Render Budget (`BitmapCache`) | Target Workload Profile |
| :--- | :---: | :--- |
| **1 GB or less** | **32 MB** | Entry-level netbooks and lightweight virtual machines |
| **1 GB to 2 GB** | **48 MB** | Low-end laptops |
| **2 GB to 4 GB** | **64 MB** | Standard laptops and virtual machines |
| **4 GB to 8 GB** | **96 MB** | Standard workstations and laptops |
| **Over 8 GB** | **128 MB** | High-resolution multi-monitor desktop environments |

### Internal Cache Partitions

Within the active working set, memory is strictly partitioned and window-trimmed:

| Cache Subsystem | Allocation Budget | Eviction Policy |
| :--- | :---: | :--- |
| **Worker Raw RGBA Cache** | 32–128 MB (RAM-adaptive) | LRU eviction + `RenderCommand::TrimCache` outside active `±2` window & inactive-doc purge on idle |
| **Slint UI Image Cache** | 24 / 48 / 64 MB (zoom-adaptive) | `evict_outside_window` outside `±2` window (`UI_IMAGE_CACHE_BUDGET = 64 MB`); inactive tabs purged on idle |
| **Single Page Image** | 32 MB cap | Any decoded page bitmap is clamped to this byte budget (`PAGE_IMAGE_BUDGET`) |
| **Sidebar Page Thumbnails** | 4 MB | Demand-driven rendering + `evict_outside_window` on sidebar scroll (`THUMB_IMAGE_BUDGET`) |

---

## How to Reproduce Benchmarks Locally

You can reproduce and verify these exact figures on your own Windows system using the automated PowerShell benchmark harness:

```powershell
# 1. Build the release binary
cargo build --release --locked -p barepdf

# 2. Ensure sibling pdfium.dll is staged next to target/release/barepdf.exe
powershell -ExecutionPolicy Bypass -File packaging/windows/scripts/fetch-pdfium.ps1

# 3. Execute the benchmark harness
powershell -ExecutionPolicy Bypass -File scripts/benchmark-memory-and-startup.ps1 `
  -ExecutablePath target/release/barepdf.exe `
  -FixturePath assets/barepdf-welcome.pdf `
  -Runs 5 `
  -IdleSeconds 5 `
  -JsonOutputPath docs/benchmarks/local-run.json `
  -MarkdownOutputPath docs/benchmarks/local-run.md
```
