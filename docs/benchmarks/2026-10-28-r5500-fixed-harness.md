# BarePDF Performance Benchmarks

**Benchmark Run:** 2026-10-28T06:12:30.4769077Z  
**Environment:** Microsoft Windows 11 Pro | AMD Ryzen 5 5500                                 
**Fixture:** barepdf-welcome.pdf  

### Benchmark Summary

| Metric | Measured Value | Budget / Target | Status |
| :--- | :---: | :---: | :---: |
| **Cold Startup Latency** | 302.8 ms | <= 350 ms | PASS |
| **Hot Startup Latency (p50)** | 299.02 ms | <= 150 ms | WARN |
| **Hot Startup Latency (p95)** | 304.67 ms | <= 250 ms | WARN |
| **Idle Private Memory** | 124.98 MB | <= 60 MB | WARN |
| **Idle Working Set** | 113.74 MB | <= 150 MB | PASS |
| **Loaded PDF Private Memory** | 121.98 MB | <= 80 MB | WARN |
| **Loaded PDF Working Set** | 109.06 MB | <= 180 MB | PASS |
| **Idle Background CPU** | 0% | <= 0.1% | PASS |

### Detailed Hot Startup Latency Runs (5 samples)
- **Min:** 293.22 ms
- **Average:** 299.11 ms
- **Max:** 305.2 ms
- **p50:** 299.02 ms
- **p95:** 304.67 ms
