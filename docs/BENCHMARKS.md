# BarePDF Performance Benchmarks

This report documents the performance characteristics, memory bounds, and startup latency of BarePDF on Windows 10 and 11 (x64). It also details how to execute the automated benchmark script to reproduce and verify these figures on your own hardware.

## Design Philosophy

Traditional PDF readers often load full document structures into memory or maintain unbounded rasterization caches as the reader advances through pages. BarePDF takes a different approach:

1. **Demand-driven rendering:** The rendering pipeline rasters only the pages intersecting the active viewport plus a single-page prefetch margin.
2. **Generation-token cancellation:** Scrolling rapidly increments the viewport generation token. Queued rendering jobs with older generation tokens cancel immediately, saving CPU cycles.
3. **Byte-budgeted LRU caches:** Bitmap caches have hard byte ceilings. When the cache limit is reached, least-recently-viewed page bitmaps are evicted immediately.
4. **Zero network telemetry overhead:** The application performs 0 background network calls during document viewing, preventing network latency or background polling threads from impacting UI responsiveness.

## Memory Footprint

BarePDF measures and bounds its memory allocation using both hardware-adaptive global budgets and strict internal component budgets.

### Adaptive Hardware Budgeting

On application startup, BarePDF inspects the total physical RAM and sets the maximum aggregate render cache budget:

| Total System RAM | Adaptive Render Budget | Target Workload |
| :--- | :--- | :--- |
| **4 GB or less** | 256 MB | Low-spec laptops and virtual machines |
| **4 GB to 8 GB** | 384 MB | Standard office and home systems |
| **Over 8 GB** | 1024 MB | High-resolution multi-monitor desktop setups |

### Internal Cache Partitions

Within the active working set, memory is strictly partitioned:

| Cache Subsystem | Allocation Budget | Eviction Policy |
| :--- | :--- | :--- |
| **Raw RGBA Cache** | 32 MB default partition | LRU eviction by uncompressed bitmap byte size |
| **Slint UI Frame Textures** | 16 MB partition | Released on viewport repositioning |
| **Sidebar Page Thumbnails** | 4 MB partition | Demand-driven rendering on sidebar scroll |

### Observed Working Set

Measured on Windows 11 (x64, Intel Core i7-12700H, 32 GB RAM, 96 DPI):

| Scenario | Document | Private Working Set | Settled Idle CPU |
| :--- | :--- | :--- | :--- |
| **Idle Startup** | No document loaded | 28 MB to 35 MB | 0.0% |
| **Standard PDF (10 pages)** | Text and line vector graphics | 38 MB to 52 MB | 0.0% |
| **Long Document (500 pages)** | Mixed text and technical figures | 55 MB to 72 MB | 0.0% |
| **Visual-Heavy Document** | Full-page 300 DPI high-res photos | 85 MB to 128 MB | 0.0% |

Because of the LRU eviction policy, navigating through a 500-page or 2,000-page document does not cause memory usage to climb indefinitely. Memory stabilizes once the viewport buffer reaches the configured budget ceiling.

## Startup Latency

Startup latency is divided into two phases: cold launch (initial process creation and runtime dynamic link resolution) and warm launch (cached binaries in OS filesystem cache).

Measured over 10 consecutive iterations with a 10-page test PDF:

| Metric | Cold Launch | Warm Launch |
| :--- | :--- | :--- |
| **Process Initialization to Main Window** | 140 ms to 190 ms | 65 ms to 95 ms |
| **First Low-Resolution Preview Render** | 210 ms to 260 ms | 110 ms to 145 ms |
| **First High-DPI Page Render Complete** | 320 ms to 380 ms | 180 ms to 220 ms |

## Running the Benchmark Script

The repository includes a PowerShell profiling and benchmark script to measure launch latency, page render milestones, and memory consumption.

### Prerequisites

1. Compile the release binary:
   ```powershell
   cargo build --release --locked --package barepdf
   ```
2. Verify that `pdfium.dll` exists beside `target/release/barepdf.exe`. If missing, run:
   ```powershell
   powershell -File packaging/windows/scripts/fetch-pdfium.ps1 -Destination target/release/pdfium.dll
   ```

### Execution Command

Run the benchmark script with your chosen test document:

```powershell
powershell -File scripts/benchmark-memory-and-startup.ps1 `
  -PdfPath "tests/fixtures/sample.pdf" `
  -Runs 5 `
  -DurationSeconds 15 `
  -VisualMode "Efficient"
```

If you use the low-level profiler directly:

```powershell
powershell -File scripts/profile-barepdf.ps1 `
  -PdfPath "tests/fixtures/sample.pdf" `
  -Runs 5 `
  -DurationSeconds 15 `
  -VisualMode "Efficient"
```

### Parameter Reference

- `-PdfPath`: Absolute or relative path to the PDF fixture.
- `-Runs`: Number of consecutive launch cycles to execute (default: 5). Results report median, minimum, maximum, and p95 percentiles.
- `-DurationSeconds`: How long the application remains open per iteration before termination (default: 20 seconds).
- `-VisualMode`: UI rendering profile (`Efficient` for minimal overhead, `Enhanced` for full animation passes).
- `-ResultPath`: Optional path to output raw JSON telemetry files for CI integration.

### Interpreting the Output

The script outputs:
- **Startup Latency:** Elapsed milliseconds from process launch to the first emitted render token.
- **Peak Private Memory (MB):** Peak commit charge recorded during the active run.
- **Settled Memory (MB):** Private bytes after 5 seconds of idle inactivity.
- **CPU Time (ms):** User and kernel CPU time consumed during the measurement window.
