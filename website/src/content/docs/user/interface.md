---
title: "User Interface Guide"
description: "Overview of the BarePDF desktop application interface, navigation sidebar, and controls."
category: "user"
order: 3
---

# User Interface Guide

BarePDF features a clean desktop layout designed to maximize reading area while keeping core controls instantly accessible. The title bar hosts multi-document tabs: opening an already open file focuses its existing tab, and up to 16 tabs can be open at once (`MAX_OPEN_TABS` in `crates/barepdf-core/src/limits.rs`). Each tab keeps its own page, zoom, and rotation state when you switch between them.

## Main Toolbar

Located at the top of the application window:

- **Open File**: Select and open PDF documents (`Ctrl+O`).
- **Page Navigation**: Validated page number input, Previous Page (`PageUp` / Left Arrow), and Next Page (`PageDown` / Right Arrow).
- **Zoom Controls**: Zoom Out (`-`), Zoom level display, Zoom In (`+`), **Fit Width**, and **Fit Page** buttons.
- **Sidebar Toggle**: Collapse or expand the navigation sidebar.
- **Window Modes**: Full Screen (`F11`) and Presentation Mode (`F5`).
- **Draw / Sign / Print / PDF Tools**: Quick actions for ink annotations, reusable signature placement, native printing, and the 8-tool Master-Detail PDF Tools dialog.
- **Settings**: Open the compact language and theme popover.

## Navigation Sidebar

The collapsible left sidebar provides document navigation tools:

### 1. Page Thumbnails Tab
Displays visual preview thumbnails of all document pages. Clicking any thumbnail jumps instantly to that page.

### 2. Document Outline (TOC) Tab
Displays the hierarchical table of contents embedded in the PDF document. Click any bookmark node to navigate to its target section.

### 3. Bookmarks Tab
Press `Ctrl+D` to add or remove a bookmark on the current page. The Bookmarks tab lists your saved page bookmarks; clicking an entry jumps straight back to that page.

## Annotations & Signatures

BarePDF includes a built-in annotation layer that works entirely offline on your local machine:

- **Text Highlights (`Ctrl+H`)**: Highlight selected text backed by PDFium glyph vector geometry.
- **Freehand Vector Ink**: Draw ink strokes with custom brush colors, thickness, and full undo/redo history (`Ctrl+Z`).
- **Signature Stamps**: Save reusable signature drawings and place them anywhere on the page with a pixel-accurate placement preview.
- **Typewriter Text Notes**: Insert typographic text notes directly onto document coordinates.
- **Save & Export**: Save annotations in place (`Ctrl+S`) or export a clean annotated copy (`Ctrl+Shift+S`).

## PDF Tools Suite (Master-Detail)

The **PDF Tools** dialog (`ToolsModal`) uses a fixed-size Master-Detail layout with a persistent tool sidebar on the left and live controls, segmented options, and multi-column page selection cards on the right:

1. **Merge**: Combine multiple PDF documents in order.
2. **Split**: Split by page ranges, extract selected pages, or burst every page into separate files.
3. **Delete**: Remove selected pages visually from the page grid.
4. **Rotate**: Rotate selected pages or the entire document in 90° steps.
5. **Convert**: Export pages to PNG or JPEG images at selectable DPI.
6. **Crop**: Apply margin presets (**Narrow 18pt**, **Normal 36pt**, **Wide 54pt**, or **Custom**) with a live crop-frame preview overlaid on the selected page thumbnail.
7. **Reorder**: Visually reorder page cards with drag or directional controls without typing page sequences manually.
8. **Compress**: Vector-preserving 3-mode hybrid PDF compressor (**LosslessLight** `220 DPI / 85% JPEG`, **Balanced** `150 DPI / 75% JPEG`, **Maximum** `110 DPI / 60% JPEG`). Text, fonts, and vector drawings stay 100% lossless while eligible opaque raster images above the target DPI are recompressed locally; original image or document bytes are preserved whenever compression does not yield smaller output.

## High-DPI Scaling

BarePDF automatically queries system DPI settings and renders crisp text and bitmaps on 4K, high-density laptop displays, and multi-monitor setups with non-uniform scaling factors.
