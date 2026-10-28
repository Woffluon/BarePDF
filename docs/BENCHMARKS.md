# BarePDF Performance Benchmarks

This report documents the performance characteristics, memory bounds, and resource utilization of BarePDF on Windows (x64). All published figures are derived from automated benchmark scripts executed against release binaries, with raw outputs committed to the repository for reproducible verification.

## Design Philosophy

Traditional PDF readers often load full document structures into memory or maintain unbounded rasterization caches as the reader advances through pages. BarePDF takes a different approach:

1. **Demand-driven rendering:** The rendering pipeline rasters only the pages intersecting the active viewport plus a single-page prefetch margin (`crates/barepdf-render/src/scheduler.rs`).
2. **Generation-token cancellation:** Scrolling rapidly increments the viewport generation token. Queued rendering jobs with older generation tokens cancel immediately, saving CPU cycles.
3. **Byte-budgeted LRU caches:** Bitmap caches have hard byte ceilings. When the cache limit is reached, least-recently-viewed page bitmaps are evicted immediately (`crates/barepdf-render/src/memory_budget.rs`).
4. **Zero background telemetry:** The application performs 0 background network calls during document viewing, preventing background polling threads or network latency from impacting UI responsiveness. Update checks are strictly opt-in and disabled by default.

---

## Measured Startup Latency & Memory Footprint

The following metrics were captured using the automated repository benchmark harness (`scripts/benchmark-memory-and-startup.ps1`) against a release build of BarePDF. The harness was fixed to report true document-open timing instead of a deadline fallback (the earlier harness fell back to a 15 s estimate when a sample could not be read; those fallback figures are obsolete).

### Metric Definition

- **`first_bitmap_ms` (startup latency):** elapsed time from issuing a document open request to the first decoded page bitmap being presented in the window. This includes PDFium engine initialization and window creation, but excludes process boot time (time spent before the application is ready to accept the open request) and image loading. The metric is recorded via `BAREPDF_PROFILE_FILE`.
- Metric provenance is kept consistent per run in the raw evidence archive (`docs/benchmarks/*.json`) so values stay comparable across commits.

### Test Environment
- **Operating System:** Microsoft Windows 11 Pro (64-bit)
- **Processor:** AMD Ryzen 5 5500 (12 logical cores)
- **Binary:** `target/release/barepdf.exe` (Release Profile, LTO enabled)
- **PDFium:** pinned `chromium/7988` build, SHA-256 verified at fetch time
- **Fixture:** `assets/barepdf-welcome.pdf`
- **Raw Evidence Archive:** [`docs/benchmarks/2026-10-28-r5500-fixed-harness.json`](./benchmarks/2026-10-28-r5500-fixed-harness.json) / [Markdown summary](./benchmarks/2026-10-28-r5500-fixed-harness.md)

### Measured Results (2026-10-28 run)

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency (first_bitmap_ms)** | **302.8 ms** | ≤ 350 ms | PASS |
| **Hot Startup Latency, p50 (n=5)** | **299.02 ms** | ≤ 150 ms | WARN |
| **Hot Startup Latency, p95 (n=5)** | **304.67 ms** | ≤ 250 ms | WARN |
| **Idle Private Memory** | **125.0 MB** (≈125 MB) | ≤ 60 MB | WARN |
| **Idle Working Set** | **113.7 MB** (≈114 MB) | ≤ 150 MB | PASS |
| **Document Loaded Private Memory** | **122.0 MB** (≈122 MB) | ≤ 80 MB | WARN |
| **Document Loaded Working Set** | **109.1 MB** (≈109 MB) | ≤ 180 MB | PASS |
| **Idle Background CPU** | **0.00%** | ≤ 0.1% | PASS |

Detailed hot run samples: min 293.22 ms, average 299.11 ms, max 305.2 ms.

*Note: The earlier published figures (2.51 MB / 13.64 MB) came from a benchmark process that was blocked in the fatal "PDF engine could not be loaded" dialog (missing sibling `pdfium.dll` during that run), not real idle/loaded application state; the fabricated evidence file has been removed. Individual working set numbers may vary based on OS memory paging conditions, screen resolution, and graphics adapter drivers.*

---

## Memory Budget Architecture

BarePDF enforces bounded memory consumption using both hardware-adaptive global budgets and strict internal component partitions defined in `crates/barepdf-render/src/memory_budget.rs` and `apps/barepdf/src/presentation/ui.rs`.

### Adaptive Hardware Budgeting

On application startup, BarePDF inspects physical RAM and sets the maximum aggregate render cache budget:

| Total System RAM | Adaptive Render Budget | Target Workload Profile |
| :--- | :---: | :--- |
| **1 GB or less** | **64 MB** | Entry-level netbooks and lightweight virtual machines |
| **1 GB to 2 GB** | **128 MB** | Low-end laptops |
| **2 GB to 4 GB** | **256 MB** | Standard laptops and virtual machines |
| **4 GB to 8 GB** | **384 MB** | Standard workstations and laptops |
| **Over 8 GB** | **1024 MB** | High-resolution multi-monitor desktop environments |

### Internal Cache Partitions

Within the active working set, memory is strictly partitioned:

| Cache Subsystem | Allocation Budget | Eviction Policy |
| :--- | :---: | :--- |
| **Raw RGBA Cache** | 128 MB | LRU eviction by uncompressed bitmap byte size (`RAW_BITMAP_BUDGET`) |
| **Slint UI Image Cache** | 128 MB, zoom-adaptive in 32/64/128 MB tiers | Released on viewport repositioning; downsized to the active zoom tier (`UI_IMAGE_CACHE_BUDGET`) |
| **Single Page Image** | 64 MB cap | Any decoded page bitmap is clamped to this byte budget (`PAGE_IMAGE_BUDGET`) |
| **Sidebar Page Thumbnails** | 4 MB | Demand-driven rendering on sidebar scroll (`THUMB_IMAGE_BUDGET`) |

---

## How to Reproduce Benchmarks Locally

You can reproduce and verify these exact figures on your own Windows system using the automated PowerShell benchmark harness:

```powershell
# 1. Build the release binary
cargo build --release --bin barepdf

# 2. Execute the benchmark harness
powershell -ExecutionPolicy Bypass -File scripts/benchmark-memory-and-startup.ps1 `
  -ExecutablePath target/release/barepdf.exe `
  -FixturePath assets/barepdf-welcome.pdf `
  -Runs 5 `
  -IdleSeconds 5 `
  -JsonOutputPath docs/benchmarks/local-run.json
```
