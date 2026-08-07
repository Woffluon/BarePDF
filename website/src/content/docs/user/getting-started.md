---
title: "Getting Started with BarePDF"
description: "Learn what BarePDF is, system requirements, and how to launch your first document."
category: "user"
order: 1
---

# Getting Started with BarePDF

BarePDF is an open-source, ultra-lightweight, fast PDF reader built specifically for Windows 10 and 11. It delivers the speed and efficiency of classic minimalist readers while providing a modern desktop user interface powered by Rust and Slint.

## Key Features

- **Ultra-Low Memory Footprint**: Less than 60MB idle RAM usage.
- **Fast Document Opening**: Instant startup and rendering via native PDFium integration.
- **Offline & Private**: Zero telemetry, zero analytics, zero AI clutter, and no user accounts.
- **Modern Desktop UX**: Native dark and light theme integration, high-DPI scaling, and responsive controls.
- **Demand-Driven Rendering**: LRU byte-budgeted memory cache ensuring smooth continuous scrolling.

## System Requirements

- **Operating System**: Windows 10 or Windows 11 (64-bit).
- **Architecture**: x86_64 (AMD64 / Intel 64-bit).
- **RAM**: 512 MB minimum (1 GB recommended).
- **Storage**: ~50 MB available disk space.

## Opening Your First PDF

You can open documents in BarePDF in multiple ways:

1. **Toolbar Button**: Click **Open PDF** or press `Ctrl+O` to open the file selection dialog.
2. **File Explorer Double-Click**: If configured as your default PDF reader, double-clicking any `.pdf` file opens it instantly.
3. **Command Line**: Pass the file path directly to the executable:
   ```cmd
   BarePDF.exe "C:\Users\You\Documents\sample.pdf"
   ```
