---
title: "Troubleshooting Guide"
description: "Solutions to common issues with PDF opening, password protection, rendering, and default apps."
category: "user"
order: 7
---

# Troubleshooting Guide

Find answers and verified solutions to common operational issues.

## 1. Password-Protected PDFs

If a PDF document is encrypted with an owner or user password:
- BarePDF opens a secure password prompt modal.
- Type the password and press `Enter`.
- If the password is correct, the document unlocks and renders immediately.
- If incorrect, an inline error message appears allowing you to retry.

## 2. Document Fails to Open or Appears Blank

- **Corrupted PDF**: Verify the file opens in another application. BarePDF relies on strict PDFium parsing standards and rejects malformed headers.
- **File Permissions**: Ensure the file is not locked exclusively by another application.
- **Path Length Limit**: Ensure the file path does not exceed standard Windows path limits unless long paths are enabled in Windows settings. BarePDF fully supports paths with spaces and Unicode characters (e.g. `Japanese (日本語)`).

## 3. PDF File Associations / Default Reader Setup

If double-clicking a `.pdf` file opens another program:
1. Open Windows **Settings** (`Win+I`) → **Apps** → **Default apps**.
2. Type `.pdf` in the file type search bar.
3. Select **BarePDF** from the list of registered applications.
4. Click **Set default**.

Alternatively, right-click any `.pdf` file in File Explorer → **Open with** → **Choose another app** → Select **BarePDF** and check *"Always use this app"*.

## 4. High Memory Usage on Very Large PDFs

BarePDF automatically bounds both raw PDFium bitmaps and the copies attached to Slint using byte-budgeted Least Recently Used (LRU) caches. Pages outside the current viewport are evicted automatically; there is no user-facing memory-budget setting.
