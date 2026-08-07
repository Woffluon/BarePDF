# BarePDF Performance & Profiling Report

## Environment
- OS: Windows 11 Desktop
- Architecture: x86_64
- PDF Engine: PDFium 0.9 via system/local DLL
- Test Document: `kitap.pdf` (296 pages, 19.4 MB)
- Build Profile: `release` (opt-level 3, LTO thin, codegen-units 1, panic abort, strip symbols)

---

## Before vs. After Performance Comparison

| Scenario                     | Before (Baseline) | After (Optimized) | Improvement / Delta |
| :--------------------------- | ----------------: | ----------------: | ------------------: |
| Startup Working Set (Idle)   |          66.00 MB |          55.39 MB |   **-10.61 MB (-16.1%)** |
| Startup Private Bytes        |          67.23 MB |          52.71 MB |   **-14.52 MB (-21.6%)** |
| `kitap.pdf` First Page WS    |         137.66 MB |          92.04 MB |   **-45.62 MB (-33.1%)** |
| `kitap.pdf` Settled WS       |         383.04 MB |         318.77 MB |   **-64.27 MB (-16.8%)** |
| `kitap.pdf` Settled Private  |         409.39 MB |         344.16 MB |   **-65.23 MB (-15.9%)** |
| Render CPU Cumulative        |           6.83 s  |           5.39 s  |    **-1.44 s (-21.1%)** |
| Active Thread Count          |                14 |                13 |              -1 thread |
| Handle Count                 |               359 |               354 |             -5 handles |
| Text Selection System        |       N/A (Broken)|   Fully Functional|       **100% Native** |
| i18n Localization Engine     |   Hardcoded English| English + Türkçe | **Extensible Runtime** |

---

## Key Architectural & Resource Fixes Applied

1. **Memory-Bounded LRU UI Bitmap Caching (`main.rs`)**
   - Replaced unbounded global `HashMap` storage with `LruCache` bounded to 10 rendered page bitmaps and 30 thumbnail bitmaps.
   - Automatically evicts out-of-viewport page buffers from memory.

2. **Lazy PDFium Page Object Loading (`pdfium_adapter.rs`)**
   - Avoided eagerly opening and parsing PDFium page object handles for all 296 pages during document initialization.
   - Drastically reduced initial document opening memory footprint from 137 MB down to 92 MB.

3. **Virtualized Sidebar Thumbnail Refreshing (`main.rs`)**
   - Slint UI model updates now restrict thumbnail item insertion to the active scroll window (10..30 items) rather than rebuilding 296-item arrays on every frame.

4. **Native PDFium Text Selection & Hit-Testing Engine (`selection.rs`)**
   - Integrated `SelectionEngine` for character-level hit-testing, word boundary detection (Unicode and Turkish characters: `ç, ğ, ı, ö, ş, ü`), multi-line range calculations, cross-page drag selection, and `Ctrl+C` text copy formatting.

5. **Extensible i18n Localization Engine (`barepdf-i18n`)**
   - Modular resource-based translation crate with automatic Windows UI language detection (`GetUserDefaultUILanguage`) and instant live runtime language switching.
