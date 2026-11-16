#!/usr/bin/env python3
"""
WCAG 2.2 AA/AAA contrast calculator for BarePDF ThemeTokens and component overrides.
Parses crates/barepdf-ui/ui/tokens.slint directly and checks critical UI pairs.
"""

from __future__ import annotations

import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
TOKENS_PATH = ROOT / "crates" / "barepdf-ui" / "ui" / "tokens.slint"


def parse_hex_color(value: str) -> tuple[int, int, int, float]:
    h = value.strip().lstrip("#")
    if len(h) == 6:
        r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
        return r, g, b, 1.0
    if len(h) == 8:
        r, g, b, a = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), int(h[6:8], 16)
        return r, g, b, a / 255.0
    raise ValueError(f"Unsupported hex color: {value}")


def blend_over(fg: tuple[int, int, int, float], bg: tuple[int, int, int, float]) -> tuple[float, float, float]:
    fr, fg_c, fb, fa = fg
    br, bg_c, bb, _ = bg
    return (
        fr * fa + br * (1.0 - fa),
        fg_c * fa + bg_c * (1.0 - fa),
        fb * fa + bb * (1.0 - fa),
    )


def channel_to_linear(c: float) -> float:
    s = c / 255.0
    return s / 12.92 if s <= 0.04045 else ((s + 0.055) / 1.055) ** 2.4


def relative_luminance(rgb: tuple[float, float, float]) -> float:
    r, g, b = rgb
    return 0.2126 * channel_to_linear(r) + 0.7152 * channel_to_linear(g) + 0.0722 * channel_to_linear(b)


def contrast_ratio(fg_hex: str, bg_hex: str) -> float:
    bg = parse_hex_color(bg_hex)
    fg = parse_hex_color(fg_hex)
    fg_rgb = blend_over(fg, bg)
    bg_rgb = (float(bg[0]), float(bg[1]), float(bg[2]))
    l1 = relative_luminance(fg_rgb)
    l2 = relative_luminance(bg_rgb)
    hi, lo = max(l1, l2), min(l1, l2)
    return (hi + 0.05) / (lo + 0.05)


def extract_tokens(tokens_text: str) -> dict[str, dict[str, str]]:
    """Extract Light and Dark hex colors from ThemeTokens properties."""
    light: dict[str, str] = {}
    dark: dict[str, str] = {}

    # Match simple properties: out property <color> name: dark ? #... : #...;
    ternary_re = re.compile(
        r"out\s+property\s+<color>\s+([a-z0-9-]+)\s*:\s*dark\s*\?\s*(#[0-9a-fA-F]{6,8})\s*:\s*(#[0-9a-fA-F]{6,8})\s*;"
    )
    single_re = re.compile(
        r"out\s+property\s+<color>\s+([a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6,8})\s*;"
    )

    for name, d_hex, l_hex in ternary_re.findall(tokens_text):
        dark[name] = d_hex.lower()
        light[name] = l_hex.lower()

    for name, hex_val in single_re.findall(tokens_text):
        dark.setdefault(name, hex_val.lower())
        light.setdefault(name, hex_val.lower())

    # Explicitly capture multi-line canvas & paper-tint values verified against tokens.slint
    light["canvas"] = "#e6e6e6"
    dark["canvas"] = "#181818"
    return {"light": light, "dark": dark}


@dataclass
class CheckCase:
    group: str
    label: str
    fg: str
    bg: str
    threshold: float
    kind: str  # "normal_text" | "ui_or_large"
    source: str


def build_cases(tokens: dict[str, dict[str, str]]) -> list[CheckCase]:
    l, d = tokens["light"], tokens["dark"]
    cases: list[CheckCase] = []

    for mode_name, pal in [("Light", l), ("Dark", d)]:
        cases.extend([
            CheckCase(mode_name, "text on window", pal["text"], pal["window"], 4.5, "normal_text", "tokens.slint:18,38"),
            CheckCase(mode_name, "text on layer", pal["text"], pal["layer"], 4.5, "normal_text", "tokens.slint:19,38"),
            CheckCase(mode_name, "text on control", pal["text"], pal["control"], 4.5, "normal_text", "tokens.slint:25,38"),
            CheckCase(mode_name, "text-muted on window", pal["text-muted"], pal["window"], 4.5, "normal_text", "tokens.slint:18,39"),
            CheckCase(mode_name, "text-muted on layer", pal["text-muted"], pal["layer"], 4.5, "normal_text", "tokens.slint:19,39"),
            CheckCase(mode_name, "text-muted on control", pal["text-muted"], pal["control"], 4.5, "normal_text", "tokens.slint:25,39"),
            CheckCase(mode_name, "text on selection (active button/tab)", pal["text"], pal["selection"], 4.5, "normal_text", "tokens.slint:38,44 (over layer)"),
            CheckCase(mode_name, "accent-content (#181105) on accent (#f69423) [TextButton/IconButton]", pal["accent-content"], pal["accent"], 4.5, "normal_text", "tokens.slint:40,43; button.slint:47"),
            CheckCase(mode_name, "accent-content on accent-hover (#faad52)", pal["accent-content"], pal["accent-hover"], 4.5, "normal_text", "tokens.slint:41,43"),
            CheckCase(mode_name, "accent-content on accent-pressed (#df7d13)", pal["accent-content"], pal["accent-pressed"], 4.5, "normal_text", "tokens.slint:42,43"),
            CheckCase(mode_name, "danger on layer", pal["danger"], pal["layer"], 4.5, "normal_text", "tokens.slint:19,45"),
            CheckCase(mode_name, "focus ring on window", pal["focus"], pal["window"], 3.0, "ui_or_large", "tokens.slint:18,47"),
            CheckCase(mode_name, "focus ring on layer", pal["focus"], pal["layer"], 3.0, "ui_or_large", "tokens.slint:19,47"),
            CheckCase(mode_name, "focus ring on control", pal["focus"], pal["control"], 3.0, "ui_or_large", "tokens.slint:25,47"),
            CheckCase(mode_name, "stroke border on layer", pal["stroke"], pal["layer"], 3.0, "ui_or_large", "tokens.slint:19,36"),
            CheckCase(mode_name, "stroke border on control", pal["stroke"], pal["control"], 3.0, "ui_or_large", "tokens.slint:25,36"),
        ])

    # Paper tints (tokens.slint:50-53)
    cases.extend([
        CheckCase("PaperTint", "Sepia text (#2c2218) on Sepia bg (#f4ecd8)", "#2c2218", "#f4ecd8", 4.5, "normal_text", "tokens.slint:50,52"),
        CheckCase("PaperTint", "Night text (#dcdcdc) on Night bg (#111111)", "#dcdcdc", "#111111", 4.5, "normal_text", "tokens.slint:51,53"),
        CheckCase("PaperTint", "Amber text (#f69423) on Amber bg (#1a1600)", "#f69423", "#1a1600", 4.5, "normal_text", "tokens.slint:51,53"),
    ])

    # Hardcoded component overrides observed in .slint files
    cases.extend([
        CheckCase("Override", "TouchButton primary #ffffff on accent (#f69423)", "#ffffff", "#f69423", 4.5, "normal_text", "touch_button.slint:67,77"),
        CheckCase("Override", "TouchButton primary hover #ffffff on accent-hover (#faad52)", "#ffffff", "#faad52", 4.5, "normal_text", "touch_button.slint:19,77"),
        CheckCase("Override", "TouchButton primary pressed #ffffff on accent-pressed (#df7d13)", "#ffffff", "#df7d13", 4.5, "normal_text", "touch_button.slint:19,77"),
        CheckCase("Override", "TouchButton danger (Light) #ffffff on #c42b1c", "#ffffff", "#c42b1c", 4.5, "normal_text", "touch_button.slint:18,77"),
        CheckCase("Override", "TouchButton danger (Dark) #ffffff on #ff99a4", "#ffffff", "#ff99a4", 4.5, "normal_text", "touch_button.slint:18,77"),
        CheckCase("Override", "PrintPreviewDialog #e03131 error on Dark layer (#2b2b2b)", "#e03131", "#2b2b2b", 4.5, "normal_text", "print_preview_dialog.slint:713"),
        CheckCase("Override", "PrintPreviewDialog #e03131 error on Light layer (#fbfbfb)", "#e03131", "#fbfbfb", 4.5, "normal_text", "print_preview_dialog.slint:713"),
        CheckCase("Override", "PrintPreviewDialog #8f96a0 page number on #ffffff", "#8f96a0", "#ffffff", 4.5, "normal_text", "print_preview_dialog.slint:220"),
        CheckCase("Override", "PrintPreviewDialog #626972 empty label on #ffffff", "#626972", "#ffffff", 4.5, "normal_text", "print_preview_dialog.slint:214"),
        CheckCase("Override", "Disabled text-muted (0.55 alpha) on Light control (#ffffff)", l["text-muted"] + "8c", l["control"], 4.5, "normal_text", "button.slint:46 (disabled ref)"),
        CheckCase("Override", "Accent #f69423 UI indicator on Light layer (#fbfbfb)", "#f69423", "#fbfbfb", 3.0, "ui_or_large", "tab_bar.slint:37; slider.slint:25"),
        CheckCase("Override", "Accent #f69423 UI indicator on Dark layer (#2b2b2b)", "#f69423", "#2b2b2b", 3.0, "ui_or_large", "tab_bar.slint:37; slider.slint:25"),
        CheckCase("Override", "Toast success icon/bar #107c41 on Light toast (#ffffff)", "#107c41", "#ffffff", 3.0, "ui_or_large", "toast.slint:27,44,74"),
        CheckCase("Override", "Toast success icon/bar #107c41 on Dark toast (#2c2c2c)", "#107c41", "#2c2c2c", 3.0, "ui_or_large", "toast.slint:27,44,74"),
        CheckCase("Override", "Toast info icon/bar #0078d4 on Light toast (#ffffff)", "#0078d4", "#ffffff", 3.0, "ui_or_large", "toast.slint:27,45,75"),
        CheckCase("Override", "Toast info icon/bar #60cdff on Dark toast (#2c2c2c)", "#60cdff", "#2c2c2c", 3.0, "ui_or_large", "toast.slint:27,45,75"),
        CheckCase("Override", "Toast progress track #e8e8e8 on Light toast (#ffffff)", "#e8e8e8", "#ffffff", 3.0, "ui_or_large", "toast.slint:27,152,297"),
        CheckCase("Override", "Toast progress track #3e3e3e on Dark toast (#2c2c2c)", "#3e3e3e", "#2c2c2c", 3.0, "ui_or_large", "toast.slint:27,152,297"),
    ])

    return cases


def main() -> int:
    tokens_text = TOKENS_PATH.read_text(encoding="utf-8")
    tokens = extract_tokens(tokens_text)
    cases = build_cases(tokens)

    # For selection over layer, composite selection over layer first
    results = []
    passed = 0
    failed = 0
    for c in cases:
        bg = c.bg
        if "selection" in c.label:
            layer_hex = tokens[c.group.lower()]["layer"]
            sel_rgb = blend_over(parse_hex_color(c.bg), parse_hex_color(layer_hex))
            bg = "#{:02x}{:02x}{:02x}".format(round(sel_rgb[0]), round(sel_rgb[1]), round(sel_rgb[2]))
        ratio = contrast_ratio(c.fg, bg)
        ok = ratio >= c.threshold
        if ok:
            passed += 1
        else:
            failed += 1
        results.append({
            "group": c.group,
            "label": c.label,
            "fg": c.fg,
            "bg": bg,
            "ratio": round(ratio, 2),
            "threshold": c.threshold,
            "pass": ok,
            "source": c.source,
        })

    if "--json" in sys.argv:
        print(json.dumps({"passed": passed, "failed": failed, "results": results}, indent=2, ensure_ascii=False))
    else:
        print(f"WCAG 2.2 AA Contrast Report ({passed} PASS / {failed} FAIL out of {len(results)} pairs)\n")
        print("| Grup | Çift | Ön / Arka | Oran | Eşik | Durum | Kaynak |")
        print("| --- | --- | --- | ---: | ---: | :---: | --- |")
        for r in results:
            status = "PASS" if r["pass"] else "FAIL"
            print(f"| {r['group']} | {r['label']} | `{r['fg']}` / `{r['bg']}` | **{r['ratio']:.2f}:1** | {r['threshold']:.1f}:1 | {status} | `{r['source']}` |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
