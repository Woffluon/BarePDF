---
title: "Reading & Viewing Modes"
description: "Learn about continuous reading, page spreads, presentation mode, and text selection."
category: "user"
order: 4
---

# Reading & Viewing Modes

BarePDF supports multiple viewing modes tailored for different reading scenarios and document layouts.

## Viewing Modes

- **Continuous Vertical (Default)**: Smooth, unbroken vertical scrolling through all pages in the document.
- **Single Page**: Displays one page at a time. Ideal for focused reading.
- **Two Page Spread**: Side-by-side display of consecutive pages.
- **Book Mode**: Two-page layout with cover page offset.

## Window & Presentation Modes

- **Full Screen (`F11`)**: Hides window borders, title bar, and taskbar for distraction-free reading. Press `F11` or `Esc` to exit.
- **Presentation Mode (`F5`)**: Displays slides centered on a clean dark background with arrow key slide controls. Press `Esc` to exit.

## Text Selection & Copying

BarePDF includes accurate text selection backed by PDFium font metrics:

1. **Select Text**: Click and drag your mouse over document text.
2. **Multi-Page Selection**: Drag continuously across page boundaries.
3. **Copy Text**: Press `Ctrl+C` or right-click to copy selected text to the system clipboard.

## Fast Scrolling Architecture

BarePDF uses a priority-queued rendering pipeline with generation token cancellation. When scrolling rapidly through large PDFs, cancelled viewport pages abort rendering immediately, eliminating scroll stutter and keeping memory usage bounded.
