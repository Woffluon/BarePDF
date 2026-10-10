# BarePDF Performance Benchmarks

This report documents the performance characteristics, memory bounds, and resource utilization of BarePDF on Windows (x64). All published figures are derived from automated benchmark scripts executed against release binaries, with raw outputs committed to the repository for reproducible verification.

## Design Philosophy

Traditional PDF readers often load full document structures into memory or maintain unbounded rasterization caches as the reader advances through pages. BarePDF takes a different approach:

1. **Demand-driven rendering:** The rendering pipeline rasters only the pages intersecting the active viewport plus a single-page prefetch margin (`crates/barepdf-render/src/scheduler.rs`).
2. **Generation-token cancellation:** Scrolling rapidly increments the viewport generation token. Queued rendering jobs with older generation tokens cancel immediately, saving CPU cycles.
3. **Byte-budgeted LRU caches:** Bitmap caches have hard byte ceilings. When the cache limit is reached, least-recently-viewed page bitmaps are evicted immediately (`crates/barepdf-render/src/memory_budget.rs`).
4. **Zero background telemetry:** The application performs 0 background network calls during document viewing, preventing background polling threads or network latency from impacting UI responsiveness. Update checks are strictly opt-in and disabled by default.

---

## Measured Memory Footprint & Resource Utilization

The following metrics were captured using the automated repository benchmark harness (`scripts/benchmark-memory-and-startup.ps1`) against a release build of BarePDF.

### Test Environment
- **Operating System:** Microsoft Windows 11 Pro (64-bit)
- **Processor:** AMD Ryzen 5 5500 (12 logical cores)
- **Binary:** `target/release/barepdf.exe` (Release Profile, LTO enabled)
- **Fixture:** `assets/barepdf-welcome.pdf`
- **Raw Evidence Archive:** [`docs/benchmarks/2026-10-10-5526791.json`](./benchmarks/2026-10-10-5526791.json)

### Measured Resource Results

| Workload Scenario | Private Memory (Bytes) | Working Set | Idle CPU Utilization |
| :--- | :---: | :---: | :---: |
| **Idle Startup (No Document)** | **2.51 MB** | **13.64 MB** | **0.00%** |
| **Document Loaded (`barepdf-welcome.pdf`)** | **2.54 MB** | **13.62 MB** | **0.00%** |

*Note: Individual working set numbers may vary based on OS memory paging conditions, screen resolution, and graphics adapter drivers.*

---

## Memory Budget Architecture

BarePDF enforces bounded memory consumption using both hardware-adaptive global budgets and strict internal component partitions defined in `crates/barepdf-render/src/memory_budget.rs`.

### Adaptive Hardware Budgeting

On application startup, BarePDF inspects physical RAM and sets the maximum aggregate render cache budget:

| Total System RAM | Adaptive Render Budget | Target Workload Profile |
| :--- | :---: | :--- |
| **4 GB or less** | **256 MB** | Low-spec laptops and virtual machines |
| **4 GB to 8 GB** | **384 MB** | Standard workstations and laptops |
| **Over 8 GB** | **1024 MB** | High-resolution multi-monitor desktop environments |

### Internal Cache Partitions

Within the active working set, memory is strictly partitioned:

| Cache Subsystem | Allocation Budget | Eviction Policy |
| :--- | :---: | :--- |
| **Raw RGBA Cache** | 32 MB default partition | LRU eviction by uncompressed bitmap byte size |
| **Slint UI Frame Textures** | 16 MB partition | Released on viewport repositioning |
| **Sidebar Page Thumbnails** | 4 MB partition | Demand-driven rendering on sidebar scroll |

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
