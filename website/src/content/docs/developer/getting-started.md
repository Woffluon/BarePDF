---
title: "Developer Overview"
description: "Overview of the BarePDF codebase, Rust toolchain, Slint UI, and repository layout."
category: "developer"
order: 1
---

# Developer Overview

BarePDF is written entirely in **Rust** (Edition 2021) and uses a modular Cargo workspace architecture. It combines the high-performance PDFium rendering engine with Slint UI framework for native Windows presentation.

## Technology Stack

- **Language**: Rust (Stable Edition 2021+).
- **UI Framework**: [Slint UI](https://slint.dev/) (v1.9) with native Win32 platform integration.
- **PDF Engine**: Google PDFium via `pdfium-render` Rust bindings.
- **Background Work / Logging**: Crossbeam channels with a single PDFium actor, `tracing`, and `tracing-subscriber`.
- **Packaging**: Inno Setup (installer) & PowerShell automation.

## Workspace Layout Overview

```text
BarePDF/
├── Cargo.toml                  # Root workspace definition
├── apps/
│   └── barepdf/                # Binary application entry point
├── crates/
│   ├── barepdf-core/           # Domain models, viewport math, preferences
│   ├── barepdf-pdf/            # PdfBackend trait & PDFium integration
│   ├── barepdf-render/         # Render scheduler, LRU cache, PDFium actor
│   ├── barepdf-platform/       # Abstract platform traits (dialogs, print, clipboard)
│   ├── barepdf-platform-windows/ # Win32 native implementations (rfd, arboard)
│   ├── barepdf-i18n/           # Multi-language string tables (en, tr)
│   └── barepdf-ui/             # Slint desktop views & keyboard shortcuts
├── docs/                       # Project documentation & release checklists
├── packaging/                  # Inno Setup scripts & PowerShell release pipeline
└── scripts/                    # Helper build & profiling scripts
```
