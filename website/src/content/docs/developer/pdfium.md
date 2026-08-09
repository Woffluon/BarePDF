---
title: "PDFium Backend Integration"
description: "How BarePDF integrates Google PDFium for fast document rendering and memory safety."
category: "developer"
order: 4
---

# PDFium Backend Integration

BarePDF uses Google PDFium as its underlying PDF rendering engine due to its industry-proven rendering fidelity, performance, and security record.

## Binding Layer

PDFium C++ runtime binaries are interfaced via the `pdfium-render` crate inside `crates/barepdf-pdf`.

### Key Lifetime Considerations
- PDFium page instances rely on the parent `FPDF_DOCUMENT` lifetime.
- `barepdf-pdf` encapsulates PDFium pointers inside thread-safe Rust smart pointers, preventing premature deallocation or use-after-free conditions.
- A document handle remains isolated on one background actor thread. BarePDF does not parallelize access to a PDFium document across a worker pool.

## Text Extraction & Font Metrics

PDFium's text page API (`FPDFText_LoadPage`) is queried to extract glyph bounding boxes (`PageTextGeometry`, `GlyphRect`). This data powers BarePDF's mouse text drag selection, double-click word selection, and multi-page text copy capabilities.
