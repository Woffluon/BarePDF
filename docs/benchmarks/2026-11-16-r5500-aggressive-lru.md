# BarePDF Performance Benchmarks

**Benchmark Run:** 2026-11-16T20:00:54.4470925Z  
**Environment:** Microsoft Windows 11 Pro | AMD Ryzen 5 5500 (16176 MB RAM)  
**Fixture:** barepdf-welcome.pdf | Multi-Page Fixture (60 pages)  

### Benchmark Summary

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency** | 187.56 ms | <= 350 ms | PASS |
| **Hot Startup Latency (p50)** | 186.64 ms | <= 150 ms | WARN |
| **Hot Startup Latency (p95)** | 192.09 ms | <= 250 ms | PASS |
| **Idle Private Memory (Empty)** | 85.44 MB | <= 60 MB | WARN |
| **Idle Working Set (Empty)** | 75.74 MB | <= 150 MB | PASS |
| **Loaded PDF Private Memory** | 138.07 MB | <= 80 MB | WARN |
| **Loaded PDF Working Set** | 115.99 MB | <= 180 MB | PASS |
| **Multi-Page PDF (60 p) Private Memory** | 151.58 MB | <= 95 MB | WARN |
| **Multi-Page PDF (60 p) Working Set** | 123.95 MB | <= 180 MB | PASS |
| **After Rapid Scroll (30 steps) Private Memory** | 160.11 MB | <= 110 MB | WARN |
| **After Rapid Scroll (30 steps) Working Set** | 135.93 MB | <= 190 MB | PASS |
| **Idle Background CPU** | 0% | <= 0.1% | PASS |

### Detailed Hot Startup Latency & Scroll Telemetry
- **Hot Startup (5 samples):** min 173.13 ms | avg 185.03 ms | max 193.38 ms | p50 186.64 ms | p95 192.09 ms
- **Rapid Scroll Peak:** 174.98 MB Private Bytes / 147.09 MB Working Set (pages 1 -> 31, scroll CPU delta: 953.12 ms, settled CPU: 0%)
