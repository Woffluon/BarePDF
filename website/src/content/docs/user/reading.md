---
title: "Reading & Viewing Modes"
description: "Learn about continuous reading, single-page mode, presentation mode, and text selection."
category: "user"
order: 4
---

# Reading & Viewing Modes

BarePDF supports multiple viewing modes tailored for different reading scenarios and document layouts.

## Viewing Modes

- **Continuous Vertical (Default)**: Smooth, unbroken vertical scrolling through all pages in the document.
- **Single Page**: Displays one page at a time. Ideal for focused reading.

## Window & Presentation Modes

- **Full Screen (`F11`)**: Hides window borders, title bar, and taskbar for distraction-free reading. Press `F11` or `Esc` to exit.
- **Presentation Mode (`F5`)**: Displays slides centered on a clean dark background with arrow key slide controls. Press `Esc` to exit.

## Text Selection & Copying

BarePDF includes accurate text selection backed by PDFium font metrics:

1. **Select Text**: Click and drag your mouse over document text.
2. **Word or Line Selection**: Double-click a word or triple-click a line.
3. **Copy Text**: Press `Ctrl+C` to copy selected text to the system clipboard.

## Fast Scrolling Architecture

BarePDF uses a priority-queued rendering pipeline with generation-token cancellation. Stale queued work is rejected before rasterization, keeping new viewport work responsive and memory bounded.
