---
title: "Settings & Configuration"
description: "How to configure theme, application language, and saved user preferences."
category: "user"
order: 6
---

# Settings & Configuration

BarePDF stores user preferences in a plain JSON configuration file located at:

```text
%APPDATA%\BarePDF\config.json
```

## Available Preferences

- **Language**:
  - `System` (Default): Automatically matches your OS display language.
  - `English`: Forces English UI strings.
  - `Turkish`: Forces Türkçe UI strings.
- **Theme Mode**:
  - `System` (Default): Follows Windows Dark/Light mode preference.
  - `Light`: Forces bright theme background and controls.
  - `Dark`: Forces dark slint theme.
- **Viewing Mode**: Default startup mode (`SinglePage` or `ContinuousVertical`).
- **Reading Direction**: `LeftToRight` or `RightToLeft`.
- **Zoom Mode**: Default zoom strategy (`FitWidth`, `FitPage`, `ActualSize`, or `Custom`).
- **Recent Files**: Remembers up to 10 recently opened documents.
- **Window Geometry**: Remembers last window size and sidebar visibility state.

## Modifying Preferences

Language and theme can be modified from the compact **Settings** popover. BarePDF manages bitmap memory automatically; there is no memory-budget control in the UI. Changes take effect immediately without requiring an application restart.
