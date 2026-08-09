---
title: "Architecture & Workspace Structure"
description: "Detailed architecture breakdown of crates, abstractions, and domain boundaries."
category: "developer"
order: 3
---

# Architecture & Workspace Structure

BarePDF is structured into decoupled crates to isolate platform API bindings, rendering pipelines, and domain logic.

## Workspace Crate Breakdown

### 1. `barepdf-core`
- **Responsibility**: Engine-agnostic domain logic and types.
- **Key Models**: `PageCount`, `PageIndex`, `ZoomFactor`, `Rotation`, `RenderDimensions`, `MemoryBudget`, `UserPreferences`.
- **Layout Engine**: `layout.rs` computes continuous page position coordinates, viewports, and page pairing.

### 2. `barepdf-pdf`
- **Responsibility**: PDF backend abstraction and engine integration.
- **Key Traits**: `PdfBackend`, `PdfDocument`, `PdfPage`.
- **Implementation**: Wraps Google PDFium runtime library (`pdfium-render`).

### 3. `barepdf-render`
- **Responsibility**: Priority-queued render scheduler and memory management.
- **Key Mechanics**: A single PDFium actor, high/low priority channels, byte-budgeted LRU caches (`lru`), deduplication, and generation-token cancellation.

### 4. `barepdf-platform` & `barepdf-platform-windows`
- **Responsibility**: System dialogs, clipboard, and printing.
- **Abstract Layer**: `barepdf-platform` defines traits for native OS dialogs.
- **Windows Impl**: `barepdf-platform-windows` implements file open dialogs via `rfd`, clipboard via `arboard`, and print spooling.

### 5. `barepdf-i18n`
- **Responsibility**: Type-safe internationalization.
- **Languages**: Supports System default detection, English, and Turkish (`Language::System`, `Language::English`, `Language::Turkish`).

### 6. `barepdf-ui`
- **Responsibility**: Presentation layer powered by Slint.
- **Components**: Toolbar view, page canvas view, sidebar thumbnails & document outline, modal dialogs, and keyboard shortcut dispatchers.

### 7. `apps/barepdf`
- **Responsibility**: Application entry point (`main.rs`).
- **Tasks**: Initializes logging (`tracing`), loads `config.json`, wires platform services, and launches the Slint event loop.
