#!/usr/bin/env python3
"""
Static UI audit script for BarePDF Slint files (crates/barepdf-ui/ui/).
Measures:
1. Accessibility coverage (accessible-role, accessible-label, accessible-live-region, FocusScope)
   both against raw (TouchArea + LineEdit) and adjusted interactive controls (excluding modal backdrops & empty TouchArea blockers).
2. Hardcoded non-token hex colors, hardcoded UI text literals, and hardcoded animation durations.
3. Sub-24px interactive targets (WCAG 2.2 SC 2.5.8).
4. Exported component usage & orphaned UI components.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
UI_ROOT = ROOT / "crates" / "barepdf-ui" / "ui"
UI_RS = ROOT / "crates" / "barepdf-ui" / "src" / "lib.rs"

HEX_RE = re.compile(r"#[0-9a-fA-F]{6,8}\b")
EXPORT_COMP_RE = re.compile(r"export\s+component\s+([A-Za-z0-9_]+)")
ANIMATE_RE = re.compile(r"animate\s+[^{]+\{([^}]+)\}")
TOUCH_AREA_RE = re.compile(r"\bTouchArea\b")
LINE_EDIT_RE = re.compile(r"\bLineEdit\b")
FOCUS_SCOPE_RE = re.compile(r"\bFocusScope\b")
ACC_ROLE_RE = re.compile(r"\baccessible-role\s*:")
ACC_LABEL_RE = re.compile(r"\baccessible-label\s*:")
ACC_LIVE_RE = re.compile(r"\baccessible-live-region\s*:")
UI_STR_PROP_RE = re.compile(
    r'\b(text|tooltip|placeholder-text|accessible-label|title|close-label|label)\s*:\s*"([^"]+)"'
)
PROP_DECL_RE = re.compile(r"^\s*(?:in|out|in-out|private)?\s*property\s+<")


def strip_comments(line: str) -> str:
    idx = line.find("//")
    return line[:idx] if idx != -1 else line


def is_meaningful_ui_literal(lit: str) -> bool:
    s = lit.strip()
    if len(s) <= 1:
        return False
    if re.fullmatch(r"[0-9]+(?:\.[0-9]+)?(?:pt|px|%)?|[0-9/.\-\s]+|[-+%<>↑↓…×✕]+", s):
        return False
    # Ignore paper standard codes or tiny technical units
    if s in {"A4", "Letter", "Legal", "PNG", "JPG", "JPEG", "PDF", "pt", "DPI", "Aa", "12pt", "14pt", "18pt", "24pt"}:
        return False
    return True


def count_non_control_touch_areas(lines: list[str]) -> int:
    """Count TouchArea {} event swallowers and full-screen modal scrim TouchAreas."""
    non_control = 0
    n = len(lines)
    for i, raw in enumerate(lines):
        code = strip_comments(raw)
        if not TOUCH_AREA_RE.search(code):
            continue
        # Single-line empty swallower: TouchArea {}
        if re.search(r"\bTouchArea\s*\{\s*\}", code):
            non_control += 1
            continue
        # Multi-line TouchArea with no logic or backdrop scrim (width: 100% / top-level close)
        block = "\n".join(strip_comments(l) for l in lines[i : min(n, i + 6)])
        prev_block = "\n".join(strip_comments(l) for l in lines[max(0, i - 4) : i + 1])
        if re.search(r"\bTouchArea\s*\{\s*(?:\n\s*)?\}", block):
            non_control += 1
        elif ("width: 100%;" in prev_block and "height: 100%;" in prev_block) and (
            "root.close()" in block or "root.dismissed()" in block or "root.is-open = false" in block or "cancel-and-clear" in block
        ):
            non_control += 1
    return non_control


def scan_slint_file(path: Path) -> dict:
    rel = path.relative_to(ROOT).as_posix()
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()

    is_tokens = path.name == "tokens.slint"
    non_transparent_hex: list[tuple[int, str]] = []
    transparent_hex = 0
    hardcoded_durations: list[tuple[int, str]] = []
    hardcoded_inline_strings: list[tuple[int, str]] = []
    hardcoded_default_strings: list[tuple[int, str]] = []
    small_targets: list[tuple[int, str]] = []

    touch_areas = 0
    line_edits = 0
    focus_scopes = 0
    acc_roles = 0
    acc_labels = 0
    acc_live = 0

    for lineno, raw in enumerate(lines, start=1):
        code = strip_comments(raw)
        if not is_tokens:
            for hx in HEX_RE.findall(code):
                if hx.lower() == "#00000000":
                    transparent_hex += 1
                else:
                    non_transparent_hex.append((lineno, hx))

        for m in ANIMATE_RE.finditer(code):
            body = m.group(1)
            if "ThemeTokens.motion" not in body and re.search(r"\d+\s*m?s\b", body):
                hardcoded_durations.append((lineno, body.strip()))

        for prop_name, lit in UI_STR_PROP_RE.findall(code):
            if not is_meaningful_ui_literal(lit):
                continue
            if PROP_DECL_RE.match(code):
                hardcoded_default_strings.append((lineno, f"{prop_name}: \"{lit}\""))
            else:
                hardcoded_inline_strings.append((lineno, f"{prop_name}: \"{lit}\""))

        touch_areas += len(TOUCH_AREA_RE.findall(code))
        line_edits += len(LINE_EDIT_RE.findall(code))
        focus_scopes += len(FOCUS_SCOPE_RE.findall(code))
        acc_roles += len(ACC_ROLE_RE.findall(code))
        acc_labels += len(ACC_LABEL_RE.findall(code))
        acc_live += len(ACC_LIVE_RE.findall(code))

        # small_targets computed below via block-level brace matching

    # Accurate brace-scoped small target check (<24px direct TouchArea container, excluding window resize strips)
    stack: list[dict] = []
    for lineno, raw in enumerate(lines, start=1):
        code = strip_comments(raw)
        opens = code.count("{")
        closes = code.count("}")
        if re.search(r"\bRectangle\s*\{", code):
            stack.append({"start": lineno, "dims": [], "direct_touch": False, "is_resize_strip": False, "depth": 1})
            # adjust extra opens beyond the Rectangle { itself
            for frame in stack[:-1]:
                frame["depth"] += opens
            if opens > 1:
                stack[-1]["depth"] += opens - 1
        else:
            for frame in stack:
                frame["depth"] += opens

        if stack:
            top = stack[-1]
            if top["depth"] == 1:
                dm = re.match(r"^\s*(?:min-)?(width|height)\s*:\s*(.+);", code)
                if dm:
                    rhs = dm.group(2).strip()
                    px_vals = [float(x) for x in re.findall(r"\b([0-9]+(?:\.[0-9]+)?)px\b", rhs)]
                    small_px = [v for v in px_vals if 0 < v < 24]
                    if small_px and "resize-border" not in rhs and "1px" != rhs:
                        top["dims"].append((lineno, dm.group(1), min(small_px), rhs))
            if TOUCH_AREA_RE.search(code) and top["depth"] == 2:
                # Direct TouchArea child inside this Rectangle
                top["direct_touch"] = True
            if "begin-resize" in code:
                for frame in stack:
                    frame["is_resize_strip"] = True

        for _ in range(closes):
            for frame in stack:
                frame["depth"] -= 1
            while stack and stack[-1]["depth"] <= 0:
                finished = stack.pop()
                if finished["direct_touch"] and not finished["is_resize_strip"] and finished["dims"]:
                    for d_line, d_prop, d_val, d_rhs in finished["dims"]:
                        small_targets.append((d_line, f"{d_prop}: {d_val:g}px ({d_rhs})"))

    non_control_ta = count_non_control_touch_areas(lines)
    interactive_controls = max(0, (touch_areas - non_control_ta) + line_edits)
    raw_primitives = touch_areas + line_edits

    return {
        "file": rel,
        "lines": len(lines),
        "exports": EXPORT_COMP_RE.findall(text),
        "touch_areas": touch_areas,
        "non_control_touch_areas": non_control_ta,
        "line_edits": line_edits,
        "raw_primitives": raw_primitives,
        "interactive_controls": interactive_controls,
        "focus_scopes": focus_scopes,
        "accessible_role": acc_roles,
        "accessible_label": acc_labels,
        "accessible_live_region": acc_live,
        "hardcoded_hex_count": len(non_transparent_hex),
        "hardcoded_hex_samples": non_transparent_hex[:12],
        "transparent_hex_count": transparent_hex,
        "hardcoded_durations": hardcoded_durations,
        "hardcoded_inline_strings_count": len(hardcoded_inline_strings),
        "hardcoded_inline_strings_samples": hardcoded_inline_strings[:12],
        "hardcoded_default_strings_count": len(hardcoded_default_strings),
        "small_targets": small_targets,
    }


def analyze_component_usage(slint_files: list[Path]) -> list[dict]:
    all_texts = {p.relative_to(ROOT).as_posix(): p.read_text(encoding="utf-8") for p in slint_files}
    ui_rs_text = UI_RS.read_text(encoding="utf-8") if UI_RS.exists() else ""
    usage: list[dict] = []

    for p in slint_files:
        rel = p.relative_to(ROOT).as_posix()
        if p.name in ("app_window.slint", "tokens.slint", "types.slint"):
            continue
        text = all_texts[rel]
        for comp in EXPORT_COMP_RE.findall(text):
            refs = []
            for other_rel, other_text in all_texts.items():
                if other_rel == rel:
                    continue
                if re.search(rf"\b{re.escape(comp)}\b", other_text):
                    refs.append(other_rel)
            if re.search(rf"\b{re.escape(comp)}\b", ui_rs_text):
                refs.append(UI_RS.relative_to(ROOT).as_posix())
            usage.append({
                "component": comp,
                "defined_in": rel,
                "referenced_in": refs,
                "reference_count": len(refs),
            })
    return usage


def main() -> int:
    slint_files = sorted(UI_ROOT.rglob("*.slint"))
    per_file = [scan_slint_file(p) for p in slint_files]
    usage = analyze_component_usage(slint_files)

    raw_prim = sum(f["raw_primitives"] for f in per_file)
    adj_ctrl = sum(f["interactive_controls"] for f in per_file)
    roles = sum(f["accessible_role"] for f in per_file)
    labels = sum(f["accessible_label"] for f in per_file)
    focus = sum(f["focus_scopes"] for f in per_file)

    # Note: TextButton/IconButton/TouchButton/MenuItem/DocumentTab carry accessible-role/label on their component definition;
    # LineEdit has built-in textbox role in Slint std-widgets, so role coverage is computed both raw and with LineEdit included.
    line_edits = sum(f["line_edits"] for f in per_file)

    totals = {
        "files": len(per_file),
        "total_lines": sum(f["lines"] for f in per_file),
        "touch_areas": sum(f["touch_areas"] for f in per_file),
        "non_control_touch_areas": sum(f["non_control_touch_areas"] for f in per_file),
        "line_edits": line_edits,
        "raw_primitives": raw_prim,
        "interactive_controls_adjusted": adj_ctrl,
        "focus_scopes": focus,
        "accessible_role": roles,
        "accessible_label": labels,
        "accessible_live_region": sum(f["accessible_live_region"] for f in per_file),
        "role_coverage_vs_adjusted_controls_pct": round(100.0 * roles / adj_ctrl, 1) if adj_ctrl else 0.0,
        "role_plus_std_lineedit_coverage_pct": round(100.0 * min(adj_ctrl, roles + line_edits) / adj_ctrl, 1) if adj_ctrl else 0.0,
        "label_coverage_vs_adjusted_controls_pct": round(100.0 * labels / adj_ctrl, 1) if adj_ctrl else 0.0,
        "focus_scope_coverage_vs_adjusted_controls_pct": round(100.0 * focus / adj_ctrl, 1) if adj_ctrl else 0.0,
        "hardcoded_hex": sum(f["hardcoded_hex_count"] for f in per_file),
        "transparent_hex": sum(f["transparent_hex_count"] for f in per_file),
        "hardcoded_durations": sum(len(f["hardcoded_durations"]) for f in per_file),
        "hardcoded_inline_strings": sum(f["hardcoded_inline_strings_count"] for f in per_file),
        "hardcoded_default_strings": sum(f["hardcoded_default_strings_count"] for f in per_file),
        "small_targets_count": sum(len(f["small_targets"]) for f in per_file),
        "orphaned_components": [u["component"] for u in usage if u["reference_count"] == 0],
    }

    if "--json" in sys.argv:
        print(json.dumps({"totals": totals, "files": per_file, "component_usage": usage}, indent=2, ensure_ascii=False))
        return 0

    print("=== BarePDF Slint UI Statik Ölçüm Özeti ===")
    for k, v in totals.items():
        print(f"- {k}: {v}")

    print("\n=== Dosya Bazlı Metrikler ===")
    print("| Dosya | Satır | Kontrol (Touch+Edit / Ayarlı) | FocusScope | acc-role | acc-label | Sabit Hex | Sabit Metin (İç) |")
    print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
    for f in per_file:
        print(
            f"| `{f['file']}` | {f['lines']} | {f['raw_primitives']} / {f['interactive_controls']} | {f['focus_scopes']} | "
            f"{f['accessible_role']} | {f['accessible_label']} | {f['hardcoded_hex_count']} | {f['hardcoded_inline_strings_count']} |"
        )

    print("\n=== Bileşen Kullanım Haritası ===")
    print("| Bileşen | Tanım Dosyası | Referans Sayısı | Kullanan Dosyalar |")
    print("| --- | --- | ---: | --- |")
    for u in usage:
        refs = ", ".join(f"`{r}`" for r in u["referenced_in"]) if u["referenced_in"] else "**KULLANILMIYOR (Ölü Bileşen)**"
        print(f"| `{u['component']}` | `{u['defined_in']}` | {u['reference_count']} | {refs} |")

    print("\n=== Küçük Hedefler (<24px) ===")
    for f in per_file:
        for lineno, detail in f["small_targets"]:
            print(f"- `{f['file']}:{lineno}` -> {detail}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
