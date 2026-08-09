---
title: "Rendering Pipeline & Cache"
description: "Demand-driven LRU bitmap caches, generation tokens, and the PDFium actor."
category: "developer"
order: 5
---

# Rendering Pipeline & Cache

BarePDF's rendering engine (`crates/barepdf-render`) uses demand-driven work and bounded caches to keep continuous scrolling responsive on large documents without allowing bitmap usage to grow with page count.

## Demand-Driven Architecture

1. **Viewport Intersection**: The layout engine (`layout.rs`) computes which page indices overlap the visible window viewport plus a 1-page prefetch margin.
2. **LRU Cache Query**: The render scheduler checks the LRU bitmap cache for pre-rendered RGBA buffers matching the requested `(PageIndex, RenderDimensions, ZoomFactor)`.
3. **Cache Hit**: Bitmaps are immediately transferred to the Slint frame buffer for instant display.
4. **Cache Miss & Work Dispatch**: Unrendered pages are sent to the single background PDFium actor. Control/visible work uses the high-priority channel; bounded background work never blocks the UI thread.

## Generation Token Cancellation

When a user scrolls rapidly:
- The UI increments the global `ViewportGeneration` token.
- All queued render tasks, including visible work, check their document and generation identity before rasterization.
- Stale render tasks (pages already scrolled past) cancel immediately, freeing CPU cores for newly visible pages.

## Memory Budgeting

The raw RGBA cache uses a fixed 32MB byte budget. The application separately budgets Slint page images (16MB) and thumbnails (4MB), preventing duplicated raw/UI copies from growing with page count.
