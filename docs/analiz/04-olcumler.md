# BarePDF UI/UX Denetimi — Faz 4: Tekrarlanabilir Ölçüm ve Nicel Sonuçlar

> **Durum:** Tamamlandı (`Python 3.11.3`, tüm betikler çıkış kodu `0`)
> **Hedef Depo:** `D:\BarePDF` (yalnızca `crates/barepdf-ui/ui/`, `crates/barepdf-i18n/src/lib.rs`, `apps/barepdf/src/presentation/hud_commands.rs` kaynakları üzerinde statik okuma)

---

## 1. Ölçüm Araçları ve Yeniden Çalıştırma Komutları

`docs/analiz/araclar/` altında dört bağımsız, standart kütüphane (`re`, `json`, `pathlib`, `dataclasses`) dışında harici bağımlılık gerektirmeyen Python 3 betiği oluşturulmuştur. Her betik insan okunabilir Markdown tabloları üretir ve `--json` parametresi ile makine okunabilir çıktı verir:

| Betik | Amaç | Çalıştırma Komutu (Çıkış Kodu) |
| --- | --- | --- |
| [`wcag_contrast.py`](file:///D:/BarePDF/docs/analiz/araclar/wcag_contrast.py) | `tokens.slint` renk token'larını ayrıştırır; `ThemeTokens`, Paper Tint ve bileşen sabit hex geçersiz kılmaları için WCAG 2.2 sRGB bağıl parlaklık + alfa kompozitleme kontrast oranlarını hesaplar. | `python docs/analiz/araclar/wcag_contrast.py` (exit=`0`) / `--json` |
| [`slint_ui_audit.py`](file:///D:/BarePDF/docs/analiz/araclar/slint_ui_audit.py) | `crates/barepdf-ui/ui/**/*.slint` dosyalarını tarar; `accessible-role`, `accessible-label`, `accessible-live-region`, `FocusScope` kapsamını, sabit hex/süre/metin sayılarını, `<24px` küçük hedefleri ve ölü bileşenleri ölçer. | `python docs/analiz/araclar/slint_ui_audit.py` (exit=`0`) / `--json` |
| [`i18n_audit.py`](file:///D:/BarePDF/docs/analiz/araclar/i18n_audit.py) | `crates/barepdf-i18n/src/lib.rs` içinde `GENERAL`, `PRINT`, `TOOLS`, `ANNOTATIONS` tablolarının EN/TR anahtar eşitliğini ve `hud_commands.rs` yerelleştirme durumunu denetler. | `python docs/analiz/araclar/i18n_audit.py` (exit=`0`) / `--json` |
| [`task_steps.py`](file:///D:/BarePDF/docs/analiz/araclar/task_steps.py) | Temel kullanıcı akışlarını kod çapaları (`tools_dialog.slint`, `numpad_popover.slint`, `command_palette.slint`, `app_window.slint`) üzerinden doğrulayıp fare/klavye adım sayılarını ve sözdizimi gereksinimini raporlar. | `python docs/analiz/araclar/task_steps.py` (exit=`0`) / `--json` |

---

## 2. WCAG 2.2 AA Kontrast Ölçümü (`wcag_contrast.py`)

### 2.1 Metodoloji ve Sınırlar
- `tokens.slint` dosyası doğrudan okunarak Light ve Dark mod hex değerleri çıkarılır; `selection` gibi saydam yüzeyler arka plan katmanı (`layer`) ile alfa karıştırılarak (`blend_over`) değerlendirilir.
- WCAG 2.2 eşikleri: normal metin için **4.5:1** (`SC 1.4.3`), UI bileşeni kenarlığı/odak/göstergesi ve büyük metin için **3.0:1** (`SC 1.4.11`).
- **Standart istisnası notu:** `Disabled text-muted (0.55 alpha)` (`#3333338c` / `#ffffff` → `3.23:1`), betikte eşik karşılaştırması gereği `FAIL` olarak işaretlenmiştir; ancak WCAG 2.2 SC 1.4.3 pasif (disabled/inactive) bileşenleri zorunlu kontrast kapsamı dışında bırakır. Buna karşın aktif `TouchButton` (primary ve dark danger), `ThemeTokens.stroke`, Light mod `#f69423` göstergeleri, `PrintPreviewDialog` sabit renkleri ve Dark `Toast` başarı rengi gerçek **aktif UI/metin kontrast ihlalleridir**.

### 2.2 Gerçek Çıktı: **37 PASS / 16 FAIL (toplam 53 çift)**

| Grup | Çift | Ön / Arka | Oran | Eşik | Durum | Kaynak |
| --- | --- | --- | ---: | ---: | :---: | --- |
| Light | text on window | `#1b1b1b` / `#f3f3f3` | **15.52:1** | 4.5:1 | PASS | `tokens.slint:18,38` |
| Light | text on layer | `#1b1b1b` / `#fbfbfb` | **16.65:1** | 4.5:1 | PASS | `tokens.slint:19,38` |
| Light | text on control | `#1b1b1b` / `#ffffff` | **17.22:1** | 4.5:1 | PASS | `tokens.slint:25,38` |
| Light | text-muted on window | `#333333` / `#f3f3f3` | **11.39:1** | 4.5:1 | PASS | `tokens.slint:18,39` |
| Light | text-muted on layer | `#333333` / `#fbfbfb` | **12.21:1** | 4.5:1 | PASS | `tokens.slint:19,39` |
| Light | text-muted on control | `#333333` / `#ffffff` | **12.63:1** | 4.5:1 | PASS | `tokens.slint:25,39` |
| Light | text on selection (active button/tab) | `#1b1b1b` / `#faecdb` | **14.83:1** | 4.5:1 | PASS | `tokens.slint:38,44 (over layer)` |
| Light | accent-content (#181105) on accent (#f69423) [TextButton/IconButton] | `#181105` / `#f69423` | **8.19:1** | 4.5:1 | PASS | `tokens.slint:40,43; button.slint:47` |
| Light | accent-content on accent-hover (#faad52) | `#181105` / `#faad52` | **9.96:1** | 4.5:1 | PASS | `tokens.slint:41,43` |
| Light | accent-content on accent-pressed (#df7d13) | `#181105` / `#df7d13` | **6.32:1** | 4.5:1 | PASS | `tokens.slint:42,43` |
| Light | danger on layer | `#c42b1c` / `#fbfbfb` | **5.47:1** | 4.5:1 | PASS | `tokens.slint:19,45` |
| Light | focus ring on window | `#9a4100` / `#f3f3f3` | **6.05:1** | 3.0:1 | PASS | `tokens.slint:18,47` |
| Light | focus ring on layer | `#9a4100` / `#fbfbfb` | **6.48:1** | 3.0:1 | PASS | `tokens.slint:19,47` |
| Light | focus ring on control | `#9a4100` / `#ffffff` | **6.71:1** | 3.0:1 | PASS | `tokens.slint:25,47` |
| Light | stroke border on layer | `#dcdcdc` / `#fbfbfb` | **1.33:1** | 3.0:1 | FAIL | `tokens.slint:19,36` |
| Light | stroke border on control | `#dcdcdc` / `#ffffff` | **1.37:1** | 3.0:1 | FAIL | `tokens.slint:25,36` |
| Dark | text on window | `#f5f5f5` / `#202020` | **14.94:1** | 4.5:1 | PASS | `tokens.slint:18,38` |
| Dark | text on layer | `#f5f5f5` / `#2b2b2b` | **12.99:1** | 4.5:1 | PASS | `tokens.slint:19,38` |
| Dark | text on control | `#f5f5f5` / `#2d2d2d` | **12.63:1** | 4.5:1 | PASS | `tokens.slint:25,38` |
| Dark | text-muted on window | `#d0d0d0` / `#202020` | **10.56:1** | 4.5:1 | PASS | `tokens.slint:18,39` |
| Dark | text-muted on layer | `#d0d0d0` / `#2b2b2b` | **9.18:1** | 4.5:1 | PASS | `tokens.slint:19,39` |
| Dark | text-muted on control | `#d0d0d0` / `#2d2d2d` | **8.93:1** | 4.5:1 | PASS | `tokens.slint:25,39` |
| Dark | text on selection (active button/tab) | `#f5f5f5` / `#544029` | **8.99:1** | 4.5:1 | PASS | `tokens.slint:38,44 (over layer)` |
| Dark | accent-content (#181105) on accent (#f69423) [TextButton/IconButton] | `#181105` / `#f69423` | **8.19:1** | 4.5:1 | PASS | `tokens.slint:40,43; button.slint:47` |
| Dark | accent-content on accent-hover (#faad52) | `#181105` / `#faad52` | **9.96:1** | 4.5:1 | PASS | `tokens.slint:41,43` |
| Dark | accent-content on accent-pressed (#df7d13) | `#181105` / `#df7d13` | **6.32:1** | 4.5:1 | PASS | `tokens.slint:42,43` |
| Dark | danger on layer | `#ff99a4` / `#2b2b2b` | **6.97:1** | 4.5:1 | PASS | `tokens.slint:19,45` |
| Dark | focus ring on window | `#f6a14a` / `#202020` | **7.85:1** | 3.0:1 | PASS | `tokens.slint:18,47` |
| Dark | focus ring on layer | `#f6a14a` / `#2b2b2b` | **6.82:1** | 3.0:1 | PASS | `tokens.slint:19,47` |
| Dark | focus ring on control | `#f6a14a` / `#2d2d2d` | **6.63:1** | 3.0:1 | PASS | `tokens.slint:25,47` |
| Dark | stroke border on layer | `#3a3a3a` / `#2b2b2b` | **1.24:1** | 3.0:1 | FAIL | `tokens.slint:19,36` |
| Dark | stroke border on control | `#3a3a3a` / `#2d2d2d` | **1.21:1** | 3.0:1 | FAIL | `tokens.slint:25,36` |
| PaperTint | Sepia text (#2c2218) on Sepia bg (#f4ecd8) | `#2c2218` / `#f4ecd8` | **13.22:1** | 4.5:1 | PASS | `tokens.slint:50,52` |
| PaperTint | Night text (#dcdcdc) on Night bg (#111111) | `#dcdcdc` / `#111111` | **13.77:1** | 4.5:1 | PASS | `tokens.slint:51,53` |
| PaperTint | Amber text (#f69423) on Amber bg (#1a1600) | `#f69423` / `#1a1600` | **7.92:1** | 4.5:1 | PASS | `tokens.slint:51,53` |
| Override | TouchButton primary #ffffff on accent (#f69423) | `#ffffff` / `#f69423` | **2.29:1** | 4.5:1 | FAIL | `touch_button.slint:67,77` |
| Override | TouchButton primary hover #ffffff on accent-hover (#faad52) | `#ffffff` / `#faad52` | **1.88:1** | 4.5:1 | FAIL | `touch_button.slint:19,77` |
| Override | TouchButton primary pressed #ffffff on accent-pressed (#df7d13) | `#ffffff` / `#df7d13` | **2.97:1** | 4.5:1 | FAIL | `touch_button.slint:19,77` |
| Override | TouchButton danger (Light) #ffffff on #c42b1c | `#ffffff` / `#c42b1c` | **5.66:1** | 4.5:1 | PASS | `touch_button.slint:18,77` |
| Override | TouchButton danger (Dark) #ffffff on #ff99a4 | `#ffffff` / `#ff99a4` | **2.03:1** | 4.5:1 | FAIL | `touch_button.slint:18,77` |
| Override | PrintPreviewDialog #e03131 error on Dark layer (#2b2b2b) | `#e03131` / `#2b2b2b` | **3.14:1** | 4.5:1 | FAIL | `print_preview_dialog.slint:713` |
| Override | PrintPreviewDialog #e03131 error on Light layer (#fbfbfb) | `#e03131` / `#fbfbfb` | **4.36:1** | 4.5:1 | FAIL | `print_preview_dialog.slint:713` |
| Override | PrintPreviewDialog #8f96a0 page number on #ffffff | `#8f96a0` / `#ffffff` | **2.98:1** | 4.5:1 | FAIL | `print_preview_dialog.slint:220` |
| Override | PrintPreviewDialog #626972 empty label on #ffffff | `#626972` / `#ffffff` | **5.55:1** | 4.5:1 | PASS | `print_preview_dialog.slint:214` |
| Override | Disabled text-muted (0.55 alpha) on Light control (#ffffff) | `#3333338c` / `#ffffff` | **3.23:1** | 4.5:1 | FAIL | `button.slint:46 (disabled ref)` |
| Override | Accent #f69423 UI indicator on Light layer (#fbfbfb) | `#f69423` / `#fbfbfb` | **2.21:1** | 3.0:1 | FAIL | `tab_bar.slint:37; slider.slint:25` |
| Override | Accent #f69423 UI indicator on Dark layer (#2b2b2b) | `#f69423` / `#2b2b2b` | **6.19:1** | 3.0:1 | PASS | `tab_bar.slint:37; slider.slint:25` |
| Override | Toast success icon/bar #107c41 on Light toast (#ffffff) | `#107c41` / `#ffffff` | **5.27:1** | 3.0:1 | PASS | `toast.slint:27,44,74` |
| Override | Toast success icon/bar #107c41 on Dark toast (#2c2c2c) | `#107c41` / `#2c2c2c` | **2.65:1** | 3.0:1 | FAIL | `toast.slint:27,44,74` |
| Override | Toast info icon/bar #0078d4 on Light toast (#ffffff) | `#0078d4` / `#ffffff` | **4.53:1** | 3.0:1 | PASS | `toast.slint:27,45,75` |
| Override | Toast info icon/bar #60cdff on Dark toast (#2c2c2c) | `#60cdff` / `#2c2c2c` | **7.76:1** | 3.0:1 | PASS | `toast.slint:27,45,75` |
| Override | Toast progress track #e8e8e8 on Light toast (#ffffff) | `#e8e8e8` / `#ffffff` | **1.23:1** | 3.0:1 | FAIL | `toast.slint:27,152,297` |
| Override | Toast progress track #3e3e3e on Dark toast (#2c2c2c) | `#3e3e3e` / `#2c2c2c` | **1.31:1** | 3.0:1 | FAIL | `toast.slint:27,152,297` |

---

## 3. Slint UI Statik Erişilebilirlik, Token ve Hedef Boyutu Ölçümü (`slint_ui_audit.py`)

### 3.1 Genel Özet Metrikleri

| Metrik | Ölçülen Değer | Açıklama |
| --- | ---: | --- |
| Taranan `.slint` dosyası | **26** | `crates/barepdf-ui/ui/` altındaki tüm UI dosyaları |
| Toplam Slint satırı | **8.833** | Yorumlar ve boşluklar dahil fiziksel satır |
| Ham `TouchArea` / `LineEdit` | **85** / **21** (**106**) | Toplam ham etkileşimli ilkel |
| Etkileşim dışı `TouchArea` (`non_control_touch_areas`) | **13** | Boş `TouchArea {}` tıklama yutucular ve tam ekran modal arka plan karartmaları |
| Ayarlanmış etkileşimli kontrol (`interactive_controls_adjusted`) | **93** | `(85 - 13) + 21` gerçek kullanıcı denetimi |
| `FocusScope` sayısı / Kapsama | **17** (**%18.3**) | Ayarlanmış 93 kontrole oranı (`17 / 93`) |
| `accessible-role` sayısı / Kapsama | **38** (**%40.9**) | Slint yerleşik `LineEdit` rolü eklenince **%63.4** |
| `accessible-label` sayısı / Kapsama | **34** (**%36.6**) | Ayarlanmış 93 kontrole oranı (`34 / 93`) |
| `accessible-live-region` sayısı | **2** | Yalnızca `toast.slint` ve `password_dialog.slint` içinde |
| Tema dışı sabit hex (`tokens.slint` hariç) | **120** | Saydam `#00000000` (`43` adet) hariç sabit renk |
| Sabit animasyon süresi (`hardcoded_durations`) | **0** | Tüm `animate` blokları `ThemeTokens.motion-*` kullanıyor |
| Sabit iç UI metin / Varsayılan property metin | **52** / **27** | `.slint` içinde doğrudan gömülü sabit UI metni / default prop |
| Küçük hedef (`<24px` direct `TouchArea` kapsayıcı) | **8** | `app_window.slint` (6 ölçüm, 3 buton) + `tab_bar.slint` (2 ölçüm, 1 buton) |
| Ölü (hiç çağrılmayan) dışa aktarılan bileşen | **2** | `FilterPill`, `PreferencesDialog` |

### 3.2 Dosya Bazlı Dağılım

| Dosya | Satır | Kontrol (Ham / Ayarlı) | `FocusScope` | `acc-role` | `acc-label` | Sabit Hex | Sabit Metin (İç) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `crates/barepdf-ui/ui/app_window.slint` | 3115 | 38 / 37 | 3 | 7 | 6 | 36 | 12 |
| `crates/barepdf-ui/ui/components/button.slint` | 61 | 1 / 1 | 1 | 1 | 1 | 2 | 0 |
| `crates/barepdf-ui/ui/components/command_palette.slint` | 121 | 5 / 3 | 0 | 0 | 0 | 2 | 1 |
| `crates/barepdf-ui/ui/components/context_menu.slint` | 111 | 2 / 0 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/components/dialog.slint` | 103 | 3 / 1 | 0 | 2 | 2 | 2 | 0 |
| `crates/barepdf-ui/ui/components/filter_pill.slint` | 33 | 1 / 1 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/components/floating_capsule.slint` | 223 | 5 / 4 | 0 | 1 | 1 | 1 | 1 |
| `crates/barepdf-ui/ui/components/flyout.slint` | 45 | 2 / 1 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/components/icon_button.slint` | 89 | 1 / 1 | 1 | 2 | 1 | 1 | 0 |
| `crates/barepdf-ui/ui/components/menu_item.slint` | 75 | 1 / 1 | 0 | 1 | 1 | 0 | 0 |
| `crates/barepdf-ui/ui/components/numpad_popover.slint` | 134 | 1 / 0 | 0 | 0 | 0 | 0 | 2 |
| `crates/barepdf-ui/ui/components/scrubber.slint` | 38 | 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/components/search_bar.slint` | 96 | 2 / 2 | 0 | 0 | 0 | 0 | 1 |
| `crates/barepdf-ui/ui/components/slider.slint` | 51 | 1 / 1 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/components/tab_bar.slint` | 105 | 2 / 2 | 1 | 3 | 2 | 0 | 0 |
| `crates/barepdf-ui/ui/components/title_bar.slint` | 248 | 5 / 5 | 0 | 6 | 4 | 2 | 0 |
| `crates/barepdf-ui/ui/components/toast.slint` | 310 | 2 / 2 | 0 | 2 | 2 | 14 | 5 |
| `crates/barepdf-ui/ui/components/touch_button.slint` | 98 | 1 / 1 | 1 | 2 | 1 | 6 | 0 |
| `crates/barepdf-ui/ui/dialogs/password_dialog.slint` | 197 | 5 / 4 | 1 | 3 | 4 | 2 | 0 |
| `crates/barepdf-ui/ui/dialogs/preferences_dialog.slint` | 77 | 0 / 0 | 0 | 0 | 0 | 0 | 17 |
| `crates/barepdf-ui/ui/dialogs/print_preview_dialog.slint` | 1008 | 9 / 8 | 1 | 2 | 3 | 28 | 3 |
| `crates/barepdf-ui/ui/dialogs/signature_dialog.slint` | 173 | 1 / 1 | 0 | 0 | 0 | 1 | 1 |
| `crates/barepdf-ui/ui/dialogs/text_annotation_dialog.slint` | 181 | 3 / 3 | 1 | 1 | 1 | 4 | 3 |
| `crates/barepdf-ui/ui/dialogs/tools_dialog.slint` | 2007 | 15 / 14 | 7 | 5 | 5 | 19 | 6 |
| `crates/barepdf-ui/ui/tokens.slint` | 70 | 0 / 0 | 0 | 0 | 0 | 0 | 0 |
| `crates/barepdf-ui/ui/types.slint` | 64 | 0 / 0 | 0 | 0 | 0 | 0 | 0 |

### 3.3 Küçük Dokunma/Tıklama Hedefleri (`<24px` — WCAG 2.2 SC 2.5.8)

| Hedef | Satır & Ölçü | Bağlam |
| --- | --- | --- |
| İmza seçili kutu silme düğmesi | `app_window.slint:1518-1519` (`14×14px`) | Doğrudan dokunmatik/fare kaldırma ikonu |
| İmza boyutlandırma tutamacı | `app_window.slint:1679-1680` (`14×14px`) | Sağ-alt köşe tutamacı |
| Metin notu boyutlandırma tutamacı | `app_window.slint:1878-1879` (`14×14px`) | Sağ-alt köşe tutamacı |
| Sekme kapatma (`×`) düğmesi | `tab_bar.slint:71-72` (`22×22px`, masaüstü modu) | `smartboard-mode` dışındayken `24×24px` altında |

### 3.4 Bileşen Kullanım Haritası ve Ölü Bileşenler

| Bileşen | Tanım Dosyası | Referans Sayısı | Kullanan Dosyalar |
| --- | --- | ---: | --- |
| `TextButton` | `components/button.slint` | 10 | `app_window.slint`, `context_menu.slint`, `search_bar.slint`, `toast.slint`, `password_dialog.slint`, `preferences_dialog.slint`, `print_preview_dialog.slint`, `signature_dialog.slint`, `text_annotation_dialog.slint`, `tools_dialog.slint` |
| `DialogFooter` | `components/dialog.slint` | 5 | `password_dialog.slint`, `print_preview_dialog.slint`, `signature_dialog.slint`, `text_annotation_dialog.slint`, `tools_dialog.slint` |
| `Dialog` | `components/dialog.slint` | 4 | `app_window.slint`, `preferences_dialog.slint`, `signature_dialog.slint`, `text_annotation_dialog.slint` |
| `IconButton` | `components/icon_button.slint` | 3 | `app_window.slint`, `floating_capsule.slint`, `search_bar.slint` |
| `MenuItem` | `components/menu_item.slint` | 2 | `app_window.slint`, `context_menu.slint` |
| `TextAnnotationDialog` | `dialogs/text_annotation_dialog.slint` | 2 | `app_window.slint`, `crates/barepdf-ui/src/lib.rs` |
| `TitleBar`, `DocumentTab`, `CommandPalette`, `ContextMenu`, `FloatingCapsule`, `Flyout`, `NumericKeypadPopover`, `Scrubber`, `SearchBar`, `ZoomSlider`, `FloatingToastCard`, `FloatingPrintCard`, `TouchButton`, `PasswordDialog`, `PrintPreviewDialog`, `SignatureDialog`, `ToolsModal` | İlgili tekil dosyalar | 1 | Çoğunlukla `app_window.slint` (alt bileşenlerde `title_bar.slint`, `numpad_popover.slint`, `signature_dialog.slint`) |
| `FilterPill` | `components/filter_pill.slint` | **0** | **KULLANILMIYOR (Ölü Bileşen)** |
| `PreferencesDialog` | `dialogs/preferences_dialog.slint` | **0** | **KULLANILMIYOR (Ölü Bileşen)** |

---

## 4. Yerelleştirme (i18n) Eşitlik Ölçümü (`i18n_audit.py`)

`crates/barepdf-i18n/src/lib.rs` içindeki dört statik sözlük tablosu ve `apps/barepdf/src/presentation/hud_commands.rs` çıktısı:

| Kategori | EN Anahtar | TR Anahtar | TR'de Eksik | EN'de Eksik |
| --- | ---: | ---: | --- | --- |
| `GENERAL` | 92 | 92 | — | — |
| `PRINT` | 45 | 43 | `print.preview.cancel_tooltip`, `print.preview.range_accessible` | — |
| `TOOLS` | 90 | 82 | `tools.compress.result_unchanged`, `tools.crop.placeholder`, `tools.drop.merge`, `tools.drop.reorder`, `tools.drop.rotate`, `tools.error.invalid_range`, `tools.error.no_pages`, `tools.merge.dragged_hint`, `tools.range.rotate_placeholder` | `tools.password.incorrect` |
| `ANNOTATIONS` | 34 | 34 | — | — |

- **Komut Paleti (`hud_commands.rs`) sabit öğe sayısı:** **14** (`barepdf_i18n` kullanımı: `False`)
- **Yorum:** `TOOLS` ve `PRINT` kategorilerinde `@pdf-tools` ile yeni eklenen anahtarların Türkçe çevirileri henüz eşlenmemiştir (`9 + 2 = 11` eksik TR anahtarı; ayrıca `tools.password.incorrect` yalnız TR'de tanımlıdır).

---

## 5. Temel Görev Adım ve Sürtünme Analizi (`task_steps.py`)

| Görev | Fare Adımı | Klavye Adımı | Kısayol | Modal Geçişi | Sözdizimi Gerekli | Not |
| --- | ---: | ---: | --- | ---: | :---: | --- |
| PDF Dosyası Açma (Boş durum / Toolbar) | 2 | 2 | `Ctrl+O` | 1 | Hayır | Açık dosya yokken boş ekrandaki birincil CTA veya araç çubuğu Open PDF ile tetiklenir. |
| Sayfaya Git (Masaüstü Giriş Kutusu) | 3 | 3 | `Ctrl+G / Ctrl+K` | 0 | Hayır | Doğrudan sayfa numarası girişi veya Komut Paleti üzerinden sayısal atlama. |
| Sayfaya Git (Akıllı Tahta / Numpad) | 4 | N/A (eksik) | `Yok (yalnızca dokunma)` | 1 | Hayır | Numpad üzerinde FocusScope ve kapatma (X) butonu yok; Türkçe sabit etiketler ('Sil', 'Git'). |
| Belge İçi Arama ve Sonraki Eşleşme | 3 | 3 | `Ctrl+F` | 0 | Hayır | Arama çubuğu açıkken Enter sonraki eşleşmeye gider. |
| Tema / Kağıt Tonu / Dil Değiştirme | 2 | N/A (eksik) | `Komut Paleti (sadece Kağıt Tonu/Ters Renk)` | 1 | Hayır | Tema/Dil değişimi Ayarlar flyout'unda; Komut Paleti tema veya dil değiştirmeyi desteklemiyor. |
| Çizim Yapma ve Kalıcı Kaydetme | 4 | N/A (eksik) | `Ctrl+Z / Ctrl+Y` | 1 | Hayır | Çizim araç çubuğu açıldığında tuval üzerinde doğrudan kalem/silgi/metin notu modu aktifleşir. |
| İmza Oluşturma ve Sayfaya Yerleştirme | 5 | N/A (eksik) | `Yok` | 2 | Hayır | SignatureDialog Escape ile kapanmıyor (Dialog bileşeninde FocusScope yok). |
| Yazdırma (Önizleme + Yazdır) | 3 | 2 | `Ctrl+P` | 1 | Hayır | Özel aralık seçilirse '1-3, 5' biçimi gerekir; yazıcı seçimi ve 'Daha Fazla Ayar' klavye ile açılamaz. |
| PDF Araçları — Sayfa Döndürme / Ayırma / Kırpma / Sıkıştırma / Sıralama | 3 | 4 | `Yok` | 1 | Hayır | Master-Detail (840x560) tek pencerede doğrudan açılıyor; görsel kart seçimi, Crop şablonları ve sürükle-bırak/ok tuşlu Reorder sözdizimi zorunluluğunu kaldırdı (klavye için modal giriş odağı ve DropZone dosya seçme butonu eksik). |
| Komut Paleti ile Hızlı Eylem | 3 | 3 | `Ctrl+K` | 1 | Hayır | Komut listesinde ok tuşları ile seçim ve Enter yürütme yok; satıra fare tıklaması veya tam metin/komut ID yazımı gerekiyor. |

---

## 6. Ölçümlerin Çapraz Özeti

1. **En büyük erişilebilirlik açığı klavye odağı yönetimindedir:** 93 etkileşimli kontrol başına yalnızca `17` `FocusScope` (`%18.3`) ve `34` `accessible-label` (`%36.6`) tanımlıdır; `Dialog`, `Flyout`, `CommandPalette`, `SearchBar`, `ZoomSlider` ve `SignatureDialog` klavye odağı döngüsünü kendi düzeyinde yönetmez.
2. **Kontrast ihlalleri merkezi token setinin dışına taşan sabit renklerde yoğunlaşır:** `ThemeTokens.text` ve `accent-content` (`8.19:1–17.22:1`) son derece güçlüdür; buna karşın `ThemeTokens.stroke` (`1.21:1–1.37:1`), `TouchButton` beyaz metni (`1.88:1–2.97:1`) ve `PrintPreviewDialog`/`Toast` sabit renkleri toplam `16` WCAG başarısızlığının ana kaynağıdır (`1` adedi pasif disabled istisnası olmak üzere `15` gerçek aktif ihlal).
3. **Kod içi metin ve hex sertleşmesi teknik borç yaratır:** `ThemeTokens` dışında `120` hex değeri ve `.slint`/`hud_commands.rs` katmanında `52` inline + `27` default metin + `14` HUD komutu + `11` eksik TR i18n anahtarı birikmiştir.
