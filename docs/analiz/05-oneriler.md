# BarePDF UI/UX Denetimi — Faz 5: Mimari Öneriler, Tasarım Sistemi ve Yol Haritası

> **Durum:** Tamamlandı
> **Bağlı Prototipler:** [accessible_controls.slint](./prototipler/accessible_controls.slint) · [unified_preferences_panel.slint](./prototipler/unified_preferences_panel.slint) · [modal_a11y_shell.slint](./prototipler/modal_a11y_shell.slint)

---

## 1. Önce / Sonra Bilgi Mimarisi (IA) ve Akış İyileştirmeleri

### 1.1 Dağınık Ayarlar ve Ölü Bileşenlerin Birleştirilmesi

```text
ÖNCE (Parçalı ve Gizli Ayarlar)
├─ Başlık/Araç Çubuğu [⚙ Ayarlar] ──► Flyout: Tema, Dil, Güncelleme, Hakkında (Paper Tint YOK)
├─ Ctrl+K (Komut Paleti)          ──► Paper Tint (Sepia/Night/Amber) + Invert (Yalnızca kısayol bilenler)
└─ Ölü Bileşen: PreferencesDialog  ──► Tema + Paper Tint (Hiçbir yerde çağrılmıyor, 17 sabit metin)

SONRA (Tek Kanonik Tercihler ve Görünüm Akışı)
├─ Başlık/Araç Çubuğu [⚙ Ayarlar] ─┬► UnifiedPreferencesPanel (i18n + FocusScope + Esc):
└─ Ctrl+K -> "Preferences"         ─┘  ├─ Görünüm: Tema (System/Light/Dark) + Kağıt Tonu + Ters Renk
                                       ├─ Dil: EN / TR + (Opsiyonel: Dokunmatik / Tahta Modu anahtarı)
                                       └─ Çevrimdışı Öncelikli Güncelleme İzni + Sürüm & Hakkında
```

- **Neden:** `PreferencesDialog` teknik borcunu sıfırlar, göz yorgunluğu / düşük görme kullanıcılarının aradığı `Paper Tint` seçeneklerini keşfedilebilir kılar (NN/g H6) ve `17` ölü sabit metni temizler.
- **Ek not:** `paper-tint` özelliği şu anda yalnızca tuval çerçevesini (`ThemeTokens.canvas`) boyar, PDF sayfası arka planını (`background: white`) etkilemez. İsimlendirme "Tuval Tonu" olarak netleştirilmeli veya sayfa katmanında gerçek kağıt tonu renk eşlemesi uygulanmalıdır.

### 1.2 `⋯ More` Taşma Menüsü, Komut Paleti ve Okuma Kenar Çubuğu

| Alan | Önce (Mevcut Durum) | Sonra (Önerilen Yapı) |
| --- | --- | --- |
| **`⋯ More` Menüsü → Komut Paleti** | `app_window.slint:2621` yanlışlıkla `request-search-query("")` çağırarak belge içi aramayı siliyor; `<900px` pencerede `286px` sabit yükseklik `Yazdır` öğesini `33px` kesiyor. | Doğrudan `request-toggle-command-palette()` + `focus-input()` çağrılır; `flyout-height` değeri `preferred-height` üzerinden dinamik hesaplanır. |
| **Komut Paleti (`hud_commands.rs`)** | `14` sabit İngilizce öğe; ok tuşu ile satır seçimi yok; tema/dil/Tools/Draw/Sign komutları eksik. | Tüm öğeler `barepdf-i18n` üzerinden beslenir; `UpArrow`/`DownArrow`/`Enter`/`Esc` tam klavye döngüsü ve Tema/Dil/`PDF Tools` komutları eklenir. |
| **`ToolsModal` (`Split`..`Compress`)** | Açık belge yokken `ToolDropZone` yalnızca sürükle-bırak kabul ediyor; klavye giriş odağı ve canlı hata duyurusu eksik; `760×520px` pencerede `Crop` grid `12px`'e sıkışıyor. | `ToolDropZone` yanına klavye odaklı `Browse PDF…` düğmesi (`AccessibleDropZone`), gövdeye `ScrollView` sarmalayıcısı (`ResponsiveToolsModalShell`) ve hata bandına `accessible-live-region: assertive` eklenir. |
| **`Outline` / `Bookmarks` Kenar Çubuğu** | Yer imi ekleme yalnız `Ctrl+D`; silme butonu yok; `Outline` satırı hem dalı açıp kapatıyor hem sayfaya atlıyor. | `Bookmarks` başlığına `+ Ekle` ve satırlara klavye odaklı silme ikonu; `Outline` ok ikonuna ayrı genişletme `TouchArea`'sı verilir. |
| **Bildirim (`Toast`) × `SearchBar` Yerleşimi** | Her ikisi de sağ üst köşede (`y: 48px–88px`) çakışıyor. | `FloatingPrintCard` ve `FloatingToastCard` sağ alt köşeye (veya `search-open ? 140px : 48px` konuma) alınır. |

---

## 2. Tasarım Sistemi ve WCAG 2.2 AA Token Önerisi

### 2.1 `ThemeTokens` İçin Yeni / Güncellenmiş Semantik Token'lar

Mevcut token tablosunun güçlü metin kontrastını (`8.19:1`–`17.22:1`) koruyarak, [`wcag_contrast.py`](./araclar/wcag_contrast.py) ile doğrulanan `15` aktif kontrast ihlalini gidermek için aşağıdaki token seti önerilir (örnek uygulama: [`accessible_controls.slint`](./prototipler/accessible_controls.slint)):

| Token Adı | Light Önerisi (Kontrast) | Dark Önerisi (Kontrast) | Mevcut Hatalı Değer | Hedef Bileşenler |
| --- | --- | --- | --- | --- |
| `stroke-control` *(yeni)* | `#858585` (`3.69:1` on `#ffffff`) | `#787878` (`3.12:1` on `#2d2d2d`) | `stroke` (`#dcdcdc` `1.37:1` / `#3a3a3a` `1.21:1`) | Tüm etkileşimli buton, girdi ve kart kenarlıkları (`divider` yalnız dekoratif kalır) |
| `accent-indicator` *(yeni)* | `#b85600` (`4.64:1` on `#fbfbfb`) | `#f69423` (`6.19:1` on `#2b2b2b`) | `#f69423` (`2.21:1` Light) | Aktif sekme alt çizgisi, `ZoomSlider` dolgu çizgisi, aktif pill kenarı |
| `danger-fill` *(yeni)* | `#c42b1c` + `#ffffff` (`5.66:1`) | `#a81c24` + `#ffffff` (`7.35:1`) | `TouchButton` `#ff99a4` + `#ffffff` (`2.03:1`) | Dolgulu tehlike butonları (`Sil`, `Vazgeç` vb.) |
| `success` *(yeni)* | `#107c41` (`5.27:1` on `#ffffff`) | `#4ec97d` (`6.63:1` on `#2c2c2c`) | Sabit `#107c41` (`2.65:1` Dark) | `FloatingToastCard`, `ToolsModal` başarı banner'ı |
| `track-bg` *(yeni)* | `#8a8a8a` (`3.45:1` on `#ffffff`) | `#787878` (`3.16:1` on `#2c2c2c`) | Sabit `#e8e8e8` (`1.23:1`) / `#3e3e3e` (`1.31:1`) | İlerleme çubukları (`Toast`, `FloatingPrintCard`) ve `ZoomSlider` yolu |
| `text-subtle` *(yeni)* | `#5e6570` (`5.88:1` on `#ffffff`) | `#d0d0d0` (`8.93:1` on `#2d2d2d`) | Sabit `#8f96a0` (`2.98:1`) | `PrintPreviewDialog` sayfa numarası, ikincil ipucu etiketleri |
| `scrim-modal` *(yeni)* | `#00000080` | `#00000080` | Her diyalogda farklı sabit alpha | Tüm modal arka plan karartmaları |

### 2.2 Mühendislik Kalitesi ve Etkileşim İncelemesi (`emil-design-eng` Formatı)

| Before | After | Why |
| --- | --- | --- |
| `TextButton` (`#181105`) ve `TouchButton` (`#ffffff`) ayrı iki bileşen; dokunmatik `TouchButton` birincil turuncu (`2.29:1`) ve koyu tehlike (`2.03:1`) butonlarında WCAG AA'yı bozuyor. | Tek `ActionButton` bileşeni (`touch-mode: ThemeTokens.smartboard-mode`); birincil yazı daima `accent-content` (`#181105`, `8.19:1`), tehlike dolgusu `danger-fill` (`7.35:1` / `5.66:1`). | Tek gerçeklik kaynağı (Single Source of Truth) sağlar; masaüstü ve Akıllı Tahta arasında hem kontrastı hem klavye/odak halkası davranışını eşitler. |
| `Dialog`, `SignatureDialog`, `CommandPalette`, `NumericKeypadPopover` ve `ToolsModal` açıldığında odak taşınmıyor veya `Esc` işleyicisi eksik. | Her modal kabuğunda `forward-focus` + `FocusScope { key-pressed(Esc) => close() }` ve canlı hata şeritlerinde `accessible-live-region: assertive`. | Klavye kullanıcılarının modal arkasındaki `main-focus` kısayollarına yanlışlıkla basmasını (`Ctrl` dışı `+`/`-`/ok tuşları) engeller ve ekran okuyucu hatalarını anında duyurur. |
| Masaüstü sekme kapatma (`22×22px`) ve imza/metin notu köşe tutamaçları (`14×14px`) WCAG 2.2 SC 2.5.8 `24×24px` alt sınırının altında. | Görsel boyut (`14×14px` kutu / `16×16px` ikon) korunurken çevreleyen şeffaf tıklama kapsayıcısı `24×24px` (`TabCloseHitTarget`: `26×26px`) yapılır. | İkonları görsel olarak şişirmeden dokunmatik, kalem ve motor hassasiyet kullanıcılarının yanlış tıklama oranını düşürür. |
| Sayfa henüz render edilmemişken yalnızca `"Loading 1"` düz metni; `ToolsModal` çalışırken yalnızca soluk/devre dışı buton. | Sabit en-boy oranlı `layer` yüzeyli sayfa iskeleti ve `is-working` sırasında footer yanında belirgin durum metni / ince ilerleme çubuğu. | Uzun PDF veya sıkıştırma işlemlerinde arayüzün donduğu algısını ("perceived latency") ortadan kaldırır. |

---

## 3. Bağımsız Tasarım Taslakları (`docs/analiz/prototipler/`)

`docs/analiz/prototipler/` altında üç bağımsız Slint 1.x taslağı üretilmiştir. **Doğrulama sınırı:** Ortamda `slint-viewer` veya `slint-lsp` kurulu olmadığı için bu dosyalar üzerinde canlı derleme/çalıştırma doğrulaması **yapılmamıştır** (`bağımsız tasarım taslağı`).

1. **[accessible_controls.slint](./prototipler/accessible_controls.slint):**
   - `PrototypeTokens` (yukarıdaki tüm WCAG AA token değerleri).
   - `ActionButton`: `TextButton` + `TouchButton` birleşimi, `FocusScope`, `2px` odak halkası, Enter/Space etkinleştirme.
   - `TabCloseHitTarget` (`26×26px`), `CanvasCornerHandle` (`24×24px` hit alanı + `14×14px` görsel), `AccessibleFilterPill` (ölü `FilterPill`'in klavye + AccessKit güncellemesi) ve `AccessibleZoomSlider` (`accessible-role: slider`, Left/Right/Up/Down ok desteği).
2. **[unified_preferences_panel.slint](./prototipler/unified_preferences_panel.slint):**
   - Ölü `PreferencesDialog` ve Ayarlar Flyout'unu tek i18n uyumlu kartta birleştirir; Tema, Kağıt Tonu, Ters Renk, Dil (`EN`/`TR`), Çevrimdışı Öncelikli Güncelleme izni ve sürüm bilgisini klavye (`Esc`) destekli olarak sunar.
3. **[modal_a11y_shell.slint](./prototipler/modal_a11y_shell.slint):**
   - `forward-focus` ve `Esc` kapanışlı `ResponsiveToolsModalShell`, `accessible-live-region: assertive` hata kutusu, sürükle-bırak yanında odaklanabilir `Browse PDF…` düğmeli `AccessibleDropZone` ve `760×520px` pencere altında `Crop` panelini kırpılmadan kaydırılabilir tutan `ScrollView` kabuğu.

### 3.1 Dar Pencere (`760×520px`) Kayan Kart & Araç Çubuğu Yerleşim Taslağı

```text
+-----------------------------------------------------------------------+
| [Tab 1 (x 26px)] [+]                                   [ _ ] [ [] ] [X]|  TitleBar (38px)
+-----------------------------------------------------------------------+
| [Open] | [< 1/12 >] | [- 100% + v] | [Search] | ... | [Tools] | [⚙]  |  Toolbar (<900px: [More] preferred-height)
+------------------+----------------------------------------------------+
| [Thumb][Out][Bm] |  +-----------------------+                         |
| +--------------+ |  | [x] Search: "bare"    | (y: 44px sağ üst)      |
| | + Ekle (Bm)  | |  +-----------------------+                         |
| | Sayfa 1      | |                                                    |
| | Sayfa 2      | |         PDF Sayfa Tuvali                           |
| +--------------+ |         (Draw bar: 2 satır sarmalanır)             |
|                  |                                                    |
|                  |                      +--------------------------+  |
|                  |                      | Toast / Print Card       |  |  <- Sağ alt (SearchBar ile çakışmaz)
|                  |                      | [======----] %60         |  |
+------------------+----------------------+--------------------------+--+
```

---

## 4. Önceliklendirilmiş Yol Haritası (`S / M / L`)

> Efor ölçeği: **S** (1–4 saat, tek dosya/düşük risk), **M** (0.5–2 gün, çoklu dosya/test), **L** (2–5 gün, mimari/platform çaprazı).

### Faz A — Hızlı Kazanımlar (S Efor · Anında Etki, Sıfır Mimari Risk)

| ID | Adım & Kapsam | Efor | İlgili Dosyalar | Doğrulama Ölçütü |
| --- | --- | :---: | --- | --- |
| **Q1** | **`More` menüsü `Search` silme hatası ve `286px` taşma düzeltmesi:** `request-search-query("")` yerine `request-toggle-command-palette()`, `flyout-height` yerine `preferred-height` kullan. | **S** | `app_window.slint` | `<900px` pencerede `Yazdır` öğesi tam görünür; mevcut arama silinmez |
| **Q2** | **`TouchButton` WCAG AA düzeltmesi:** Primary metni `ThemeTokens.accent-content` (`#181105`), dark danger dolgusunu `#a81c24` + `#ffffff` yap (`8.19:1` / `7.35:1`). | **S** | `touch_button.slint`, `tokens.slint` | `wcag_contrast.py`: TouchButton çiftleri `PASS` (`0 FAIL`) |
| **Q3** | **Yeni semantik renk token'ları & sabit hex temizliği:** `stroke-control`, `accent-indicator`, `success`, `track-bg`, `text-subtle` token'larını ekle; `PrintPreviewDialog` (`#e03131`, `#8f96a0`) ve `Toast` (`#107c41`, `#e8e8e8`) sabitlerini değiştir. | **S** | `tokens.slint`, `print_preview_dialog.slint`, `toast.slint`, `tab_bar.slint`, `slider.slint` | `wcag_contrast.py`: Aktif çiftlerde `0 FAIL` |
| **Q4** | **`24×24px`+ hedef boyutu genişletmesi:** Sekme kapatma butonunu `26×26px`, imza/metin notu köşe tutamaçlarını `24×24px` şeffaf kapsayıcıya al. | **S** | `tab_bar.slint`, `app_window.slint` | `slint_ui_audit.py`: `small_targets_count == 0` |
| **Q5** | **`ToolsModal` `Esc`, canlı hata bölgesi & `ToolDropZone` Gözat butonu:** `forward-focus`, `accessible-live-region: assertive` ve `Browse PDF…` düğmesini ekle. | **S** | `tools_dialog.slint`, `callbacks/tools.rs` | `Esc` ile modal kapanır; sürükle-bırak olmadan dosya seçilebilir |
| **Q6** | **Linux `config.json` yolu ve yazıcı yedeği:** `preferences_store.rs` içinde `#[cfg(not(windows))]` için `XDG_CONFIG_HOME`/`HOME/.config/barepdf/config.json`; `print.rs` platform-koşullu yazıcı yedeği. | **S** | `preferences_store.rs`, `print.rs` | `cargo test --workspace` + Linux ortamında `$HOME/.config/barepdf/` |

### Faz B — Orta Vade (M Efor · Klavye, i18n ve Duyarlılık Tamamlama)

| ID | Adım & Kapsam | Efor | İlgili Dosyalar | Doğrulama Ölçütü |
| --- | --- | :---: | --- | --- |
| **M1** | **Tam i18n temizliği (`52` iç metin + `14` HUD komutu + `11` eksik TR anahtar):** `Numpad`, `UnsavedDrawings`, `Toast`, `Settings`, `PrintPreview`, `ToolsModal` ve `hud_commands.rs` metinlerini `barepdf-i18n`'e taşı. | **M** | `barepdf-i18n/src/lib.rs`, `hud_commands.rs`, `numpad_popover.slint`, `toast.slint`, `app_window.slint`, `tools_dialog.slint` | `i18n_audit.py`: `0` eksik anahtar, `hud_uses_i18n == True`; `slint_ui_audit.py`: `0` sabit UI metin |
| **M2** | **Modal & Overlay Klavye Odak Yönetimi:** `Dialog` tabanına `FocusScope` + `Esc`; `CommandPalette`'e açılış `focus-input()`, `UpArrow`/`DownArrow` seçimi ve `Esc`; `PrintPreviewDialog` yazıcı açılır listesine `combobox` klavye desteği. | **M** | `dialog.slint`, `command_palette.slint`, `signature_dialog.slint`, `print_preview_dialog.slint`, `numpad_popover.slint` | Tüm modallar fare değmeden `Tab`/`Shift+Tab`/`Esc`/`Enter` ile tamamlanabilir |
| **M3** | **Etkileşimli kontrollerde `%100` AccessKit semantiği & `FocusScope` boyutu:** `SidebarListItem`, `ZoomSlider`, çizim renk/kalınlık butonları ve tüm `TouchArea` kontrollerine `accessible-role`/`accessible-label` ekle; `0px` genişlikli `FocusScope` yerleşimini Narrator/NVDA ile test et. | **M** | `app_window.slint`, `slider.slint`, `search_bar.slint`, `button.slint`, `icon_button.slint` | `slint_ui_audit.py`: `accessible-role` ve `accessible-label` kapsamı `>= %95` |
| **M4** | **Dar pencere duyarlılığı & kart yerleşimi:** `ToolsModal -> Crop` gövdesini `ScrollView` içine al; çizim araç çubuğunu `500px` altında sarmala/grupla; `FloatingToastCard`/`FloatingPrintCard` ile `SearchBar` çakışmasını kaldır. | **M** | `tools_dialog.slint`, `app_window.slint` | `760×520px` pencerede sıfır yatay/dikey taşma |

### Faz C — Yapısal Değişiklikler (L Efor · Tasarım Sistemi Konsolidasyonu ve Ölçüm Otomasyonu)

| ID | Adım & Kapsam | Efor | İlgili Dosyalar | Doğrulama Ölçütü |
| --- | --- | :---: | --- | --- |
| **L1** | **Tek Buton Sistemi + Ölü Bileşen Konsolidasyonu:** `TouchButton` + `TextButton` → `ActionButton`; `PreferencesDialog` + Ayarlar `Flyout` → `UnifiedPreferencesPanel` (`Paper Tint` dahil); `FilterPill` ya `ToolsModal`/Arama filtrelerinde kullanılır ya da temizlenir. | **L** | `button.slint`, `touch_button.slint`, `preferences_dialog.slint`, `filter_pill.slint`, `app_window.slint` | `slint_ui_audit.py`: `orphaned_components == []` |
| **L2** | **Tam `smartboard-mode` Ölçekleme ve Manuel Dokunmatik Anahtarı:** Ayarlarda manuel Akıllı Tahta/Dokunmatik modu; `scale-multiplier` değerinin tüm araç çubuğu/diyalog butonlarına tutarlı uygulanması. | **L** | `tokens.slint`, `preferences.rs`, `app_window.slint`, `ui.rs` | `smartboard-mode: true` iken tüm butonlar `>= 48px` |
| **L3** | **Gerçek `Paper Tint` Sayfa Katmanı ve Çizim/İmza A11y Altyapısı:** PDF render çıktısına kağıt tonu renk eşlemesi (veya net "Tuval Tonu" ayrımı); sayfa yükleme iskeleti ve `tools-working` ilerleme göstergesi. | **L** | `barepdf-render`, `app_window.slint`, `tools_dialog.slint` | Görsel doğrulama + `headless_smoke_test.rs` |
| **L4** | **CI UI Kalite Kapısı ve Görsel Regresyon Testi:** `wcag_contrast.py`, `slint_ui_audit.py` ve `i18n_audit.py` betiklerinin CI içine eşik kontrollü entegrasyonu + canlı Windows Narrator / Linux Orca kabul testi. | **L** | `.github/workflows/ci.yml`, `docs/analiz/araclar/` | CI üzerinde otomatik WCAG/i18n/A11y geçerliği |
