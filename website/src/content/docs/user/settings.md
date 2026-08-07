---
title: "Settings & Configuration"
description: "How to configure theme, application language, LRU memory budget, and user preferences."
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
- **Viewing Mode**: Default startup mode (`SinglePage`, `ContinuousVertical`, `TwoPageSpread`, `BookMode`).
- **Reading Direction**: `LeftToRight` or `RightToLeft`.
- **Zoom Mode**: Default zoom strategy (`FitWidth`, `FitPage`, `ActualSize`, or `Custom`).
- **Memory Budget**: Configures LRU bitmap cache limit (default: 96MB / 256MB).
- **Recent Files**: Remembers up to 10 recently opened documents.
- **Window Geometry**: Remembers last window size and sidebar visibility state.

## Modifying Preferences

Preferences can be modified directly within the BarePDF **Settings** dialog or edited manually in `config.json`. Changes take effect immediately without requiring application restart.
