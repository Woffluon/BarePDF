---
title: "Rendering Pipeline & Cache"
description: "Demand-driven LRU bitmap memory cache, generation tokens, and worker pools."
category: "developer"
order: 5
---

# Rendering Pipeline & Cache

BarePDF's rendering engine (`crates/barepdf-render`) is engineered for smooth 60fps continuous scrolling over multi-thousand page documents without excessive memory usage.

## Demand-Driven Architecture

1. **Viewport Intersection**: The layout engine (`layout.rs`) computes which page indices overlap the visible window viewport plus a 1-page prefetch margin.
2. **LRU Cache Query**: The render scheduler checks the LRU bitmap cache for pre-rendered RGBA buffers matching the requested `(PageIndex, RenderDimensions, ZoomFactor)`.
3. **Cache Hit**: Bitmaps are immediately transferred to the Slint frame buffer for instant display.
4. **Cache Miss & Work Dispatch**: Unrendered pages spawn render tasks onto the background thread worker pool.

## Generation Token Cancellation

When a user scrolls rapidly:
- The UI increments the global `ViewportGeneration` token.
- Pending background worker tasks check their task generation token before executing heavy PDFium rasterization.
- Stale render tasks (pages already scrolled past) cancel immediately, freeing CPU cores for newly visible pages.

## Memory Budgeting

The LRU cache tracks total allocated RGBA bitmap byte size. When total memory exceeds `MemoryBudget` (default 96MB / 256MB), off-screen page bitmaps are evicted automatically.
