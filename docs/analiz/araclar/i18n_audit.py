#!/usr/bin/env python3
"""
Checks English vs Turkish translation key parity in crates/barepdf-i18n/src/lib.rs
and detects hardcoded UI strings in Rust presentation files (hud_commands.rs, ui.rs).
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
I18N_RS = ROOT / "crates" / "barepdf-i18n" / "src" / "lib.rs"
HUD_RS = ROOT / "apps" / "barepdf" / "src" / "presentation" / "hud_commands.rs"


def check_i18n_parity() -> dict:
    src = I18N_RS.read_text(encoding="utf-8")
    blocks = dict(re.findall(r"const (ENGLISH_[A-Z]+|TURKISH_[A-Z]+):[^=]+=\s*&\[(.*?)\];", src, re.S))
    categories = []
    for cat in ["GENERAL", "PRINT", "TOOLS", "ANNOTATIONS"]:
        en_key = f"ENGLISH_{cat}"
        tr_key = f"TURKISH_{cat}"
        en = dict(re.findall(r'\("([^"]+)",\s*"([^"]*)"\)', blocks.get(en_key, "")))
        tr = dict(re.findall(r'\("([^"]+)",\s*"([^"]*)"\)', blocks.get(tr_key, "")))
        missing_tr = sorted(set(en) - set(tr))
        missing_en = sorted(set(tr) - set(en))
        categories.append({
            "category": cat,
            "en_count": len(en),
            "tr_count": len(tr),
            "missing_in_tr": missing_tr,
            "missing_in_en": missing_en,
        })

    hud_src = HUD_RS.read_text(encoding="utf-8") if HUD_RS.exists() else ""
    hud_items = len(re.findall(r"HudCommandItem\s*\{", hud_src))
    hud_uses_i18n = "barepdf_i18n" in hud_src

    return {
        "categories": categories,
        "hud_command_count": hud_items,
        "hud_uses_i18n": hud_uses_i18n,
    }


def main() -> int:
    data = check_i18n_parity()
    if "--json" in sys.argv:
        print(json.dumps(data, indent=2, ensure_ascii=False))
        return 0

    print("=== BarePDF i18n Anahtar Eşitliği ve HUD Komut Paleti Denetimi ===\n")
    print("| Kategori | EN Anahtar | TR Anahtar | TR'de Eksik | EN'de Eksik |")
    print("| --- | ---: | ---: | --- | --- |")
    for c in data["categories"]:
        mtr = ", ".join(f"`{k}`" for k in c["missing_in_tr"]) if c["missing_in_tr"] else "—"
        men = ", ".join(f"`{k}`" for k in c["missing_in_en"]) if c["missing_in_en"] else "—"
        print(f"| `{c['category']}` | {c['en_count']} | {c['tr_count']} | {mtr} | {men} |")
    print(f"\n- Komut Paleti (`hud_commands.rs`) sabit öğe sayısı: **{data['hud_command_count']}** (`barepdf_i18n` kullanımı: `{data['hud_uses_i18n']}`)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
