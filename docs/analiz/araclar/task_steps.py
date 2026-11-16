#!/usr/bin/env python3
"""
Deterministic task step & friction analyzer for BarePDF core user flows.
Verifies callback/shortcut anchors in the actual repository before reporting step counts.
"""

from __future__ import annotations

import json
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


@dataclass
class TaskFlow:
    name: str
    mouse_steps: int
    keyboard_steps: int
    shortcut: str
    modal_transitions: int
    syntax_input_required: bool
    evidence_paths: list[str]
    notes: str


def verify_anchor(rel_path: str, needle: str) -> bool:
    p = ROOT / rel_path
    if not p.exists():
        return False
    return needle in p.read_text(encoding="utf-8")


def collect_flows() -> list[TaskFlow]:
    has_tools_back = verify_anchor("crates/barepdf-ui/ui/dialogs/tools_dialog.slint", "current-tool == -1")
    has_visual_reorder = verify_anchor("crates/barepdf-ui/ui/dialogs/tools_dialog.slint", "reorder-move(")
    has_numpad = verify_anchor("crates/barepdf-ui/ui/components/numpad_popover.slint", "NumericKeypadPopover")
    has_palette = verify_anchor("crates/barepdf-ui/ui/components/command_palette.slint", "CommandPalette")
    has_prefs_flyout = verify_anchor("crates/barepdf-ui/ui/app_window.slint", "settings-open")

    flows = [
        TaskFlow(
            name="PDF Dosyası Açma (Boş durum / Toolbar)",
            mouse_steps=2,  # 1: Open PDF butonu, 2: OS dosya seçici onay
            keyboard_steps=2,  # Ctrl+O + OS dosya seçici Enter
            shortcut="Ctrl+O",
            modal_transitions=1,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/app_window.slint", "apps/barepdf/src/presentation/callbacks/mod.rs"],
            notes="Açık dosya yokken boş ekrandaki birincil CTA veya araç çubuğu Open PDF ile tetiklenir.",
        ),
        TaskFlow(
            name="Sayfaya Git (Masaüstü Giriş Kutusu)",
            mouse_steps=3,  # 1: sayfa girişine tıkla, 2: sayfa yaz, 3: Enter
            keyboard_steps=3,  # Ctrl+G / Ctrl+K -> sayı -> Enter
            shortcut="Ctrl+G / Ctrl+K",
            modal_transitions=0,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/app_window.slint", "apps/barepdf/src/presentation/callbacks/navigation.rs"],
            notes="Doğrudan sayfa numarası girişi veya Komut Paleti üzerinden sayısal atlama.",
        ),
        TaskFlow(
            name="Sayfaya Git (Akıllı Tahta / Numpad)",
            mouse_steps=4 if has_numpad else 0,  # 1: Kapsül sayfa rozeti, 2..n: rakam tuşları, 3: Git
            keyboard_steps=0,  # Numpad üzerinde klavye odak/Enter yok
            shortcut="Yok (yalnızca dokunma)",
            modal_transitions=1,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/components/floating_capsule.slint", "crates/barepdf-ui/ui/components/numpad_popover.slint"],
            notes="Numpad üzerinde FocusScope ve kapatma (X) butonu yok; Türkçe sabit etiketler ('Sil', 'Git').",
        ),
        TaskFlow(
            name="Belge İçi Arama ve Sonraki Eşleşme",
            mouse_steps=3,  # 1: Ara ikonu, 2: Sorgu yaz, 3: Sonraki ok
            keyboard_steps=3,  # Ctrl+F -> Sorgu -> Enter
            shortcut="Ctrl+F",
            modal_transitions=0,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/components/search_bar.slint", "apps/barepdf/src/presentation/callbacks/features.rs"],
            notes="Arama çubuğu açıkken Enter sonraki eşleşmeye gider.",
        ),
        TaskFlow(
            name="Tema / Kağıt Tonu / Dil Değiştirme",
            mouse_steps=2 if has_prefs_flyout else 3,  # 1: Ayarlar ikonu, 2: Seçenek tıklama
            keyboard_steps=0,  # Ayarlar ikonu ve flyout içi özel düğmeler klavye ile tam gezilebilir değilse 0
            shortcut="Komut Paleti (sadece Kağıt Tonu/Ters Renk)",
            modal_transitions=1,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/app_window.slint", "apps/barepdf/src/presentation/hud_commands.rs"],
            notes="Tema/Dil değişimi Ayarlar flyout'unda; Komut Paleti tema veya dil değiştirmeyi desteklemiyor.",
        ),
        TaskFlow(
            name="Çizim Yapma ve Kalıcı Kaydetme",
            mouse_steps=4,  # 1: Draw ikonu, 2: Çizim darbesi, 3: Save, 4: Hedef seçimi / onay
            keyboard_steps=0,  # Çizim yalnızca pointer; Ctrl+Z/Y geri alma var
            shortcut="Ctrl+Z / Ctrl+Y",
            modal_transitions=1,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/app_window.slint", "apps/barepdf/src/presentation/callbacks/drawing.rs"],
            notes="Çizim araç çubuğu açıldığında tuval üzerinde doğrudan kalem/silgi/metin notu modu aktifleşir.",
        ),
        TaskFlow(
            name="İmza Oluşturma ve Sayfaya Yerleştirme",
            mouse_steps=5,  # 1: Sign, 2: İmzayı çiz / görsel seç, 3: Place Signature, 4: Sayfaya tıkla, 5: Save
            keyboard_steps=0,  # SignatureDialog FocusScope / Esc yok
            shortcut="Yok",
            modal_transitions=2,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/dialogs/signature_dialog.slint", "apps/barepdf/src/presentation/callbacks/drawing.rs"],
            notes="SignatureDialog Escape ile kapanmıyor (Dialog bileşeninde FocusScope yok).",
        ),
        TaskFlow(
            name="Yazdırma (Önizleme + Yazdır)",
            mouse_steps=3,  # 1: Print, 2: (opsiyonel ayar), 3: Yazdır onayı
            keyboard_steps=2,  # Ctrl+P -> (Enter tetiklenmiyor çünkü yazıcı/kopya kontrolleri odaklanabilir değil)
            shortcut="Ctrl+P",
            modal_transitions=1,
            syntax_input_required=False,
            evidence_paths=["crates/barepdf-ui/ui/dialogs/print_preview_dialog.slint", "apps/barepdf/src/presentation/callbacks/print.rs"],
            notes="Özel aralık seçilirse '1-3, 5' biçimi gerekir; yazıcı seçimi ve 'Daha Fazla Ayar' klavye ile açılamaz.",
        ),
        TaskFlow(
            name="PDF Araçları — Sayfa Döndürme / Ayırma / Kırpma / Sıkıştırma / Sıralama",
            mouse_steps=(4 if has_tools_back else 3),  # Master-Detail sonrası 1: PDF Tools, 2: Araç seç, 3: Uygula (+ dosya seçimi)
            keyboard_steps=4 if not has_tools_back else 0,  # Master-Detail sidebar + SegmentedItem FocusScope mevcut; DropZone/Crop/Range alanlarında kısmi
            shortcut="Yok",
            modal_transitions=1,
            syntax_input_required=(not has_visual_reorder),
            evidence_paths=["crates/barepdf-ui/ui/dialogs/tools_dialog.slint", "apps/barepdf/src/presentation/callbacks/tools.rs"],
            notes=(
                "Eski iki adımlı liste + < Back akışı (current-tool == -1); Reorder manuel virgüllü sıra girişi gerektiriyor."
                if has_tools_back
                else "Master-Detail (840x560) tek pencerede doğrudan açılıyor; görsel kart seçimi, Crop şablonları ve sürükle-bırak/ok tuşlu Reorder sözdizimi zorunluluğunu kaldırdı (klavye için modal giriş odağı ve DropZone dosya seçme butonu eksik)."
            ),
        ),
    ]

    if has_palette:
        flows.append(
            TaskFlow(
                name="Komut Paleti ile Hızlı Eylem",
                mouse_steps=3,
                keyboard_steps=3,  # Ctrl+K -> yaz -> Enter
                shortcut="Ctrl+K",
                modal_transitions=1,
                syntax_input_required=False,
                evidence_paths=["crates/barepdf-ui/ui/components/command_palette.slint", "apps/barepdf/src/presentation/hud_commands.rs"],
                notes="Komut listesinde ok tuşları ile seçim ve Enter yürütme yok; satıra fare tıklaması veya tam metin/komut ID yazımı gerekiyor.",
            )
        )
    return flows


def main() -> int:
    flows = collect_flows()
    if "--json" in sys.argv:
        print(json.dumps([f.__dict__ for f in flows], indent=2, ensure_ascii=False))
        return 0

    print("=== BarePDF Temel Görev Adım Analizi ===\n")
    print("| Görev | Fare Adımı | Klavye Adımı | Kısayol | Modal Geçişi | Sözdizimi Gerekli | Not |")
    print("| --- | ---: | ---: | --- | ---: | :---: | --- |")
    for f in flows:
        kb = str(f.keyboard_steps) if f.keyboard_steps > 0 else "N/A (eksik)"
        syn = "Evet" if f.syntax_input_required else "Hayır"
        print(f"| {f.name} | {f.mouse_steps} | {kb} | `{f.shortcut}` | {f.modal_transitions} | {syn} | {f.notes} |")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
