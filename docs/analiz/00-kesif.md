# Faz 0: Keşif (Mimari, Platformlar, Tema Token'ları ve CodeGraph Bulguları)

> **Kapsam ve Tarih:** `D:\BarePDF` deposu, 6. İş Paketi UI/UX denetimi başlangıç keşfi. Kaynak kodda ürün değişikliği yapılmamıştır. Paralel çalışan `@deps-ci`, `@pdf-tools` ve `@update-log` ajanlarının `.slint` ve bağımlılık güncellemeleri devam ettiği için `app_window.slint`, `tools_dialog.slint`, `toast.slint` ve `Cargo.toml` üzerindeki son satır numaraları Faz 2–4 aşamasında yeniden doğrulanacaktır.

---

## 1. CodeGraph İndeks ve Çağrı Yolu Bulguları

Swarm worker oturumunda `call_mcp_tool` aracı tanımlı olmadığı ve `list_resources(ServerName: "codegraph")` çağrısı `"server codegraph is not allowed in this context"` döndürdüğü için koordinatör onayı (`#14`) doğrultusunda aynı `.codegraph/` veritabanını okuyan yerel `codegraph` CLI sorguları çalıştırıldı.

### 1.1 İndeks Özeti (`codegraph status D:\BarePDF`)
- **Dosya sayısı:** `154` (`rust: 112`, `astro: 23`, `yaml: 11`, `typescript: 7`, `javascript: 1`). Not: `.slint` dosyaları CodeGraph dil ayrıştırıcısı tarafından doğrudan indekslenmez; `.slint` katmanı `crates/barepdf-ui/src/lib.rs` içindeki `slint::slint!` makrosu ve `rg` taramaları ile çapraz doğrulandı.
- **Graf boyutu:** `3.653` düğüm (`function: 1.025`, `import: 786`, `method: 742`, `enum_member: 334`, `struct: 191`, `variable: 148`, `file: 143`, `enum: 73`, `property: 73`, `constant: 63`, `component: 23`, `interface: 18`, `type_alias: 18`, `trait: 9`, `route: 7`) ve `11.331` kenar.

### 1.2 UI/UX Etki Alanı ve Çağrı Yolu Sorguları (`codegraph explore`)
- **`AppWindow presentation ui callbacks slint bridge` (`46` sembol / `4` dosya):**
  - `connect_window_chrome_callbacks` (`apps/barepdf/src/presentation/window_chrome.rs:53`) → tek çağırıcı `apps/barepdf/src/presentation/callbacks/mod.rs`; 3 adım içinde test kapsamı yok.
  - `presentation_render_size` (`apps/barepdf/src/presentation/ui.rs:1065`) → 2 çağırıcı (`apps/barepdf/src/presentation/ui.rs`); 3 adım içinde test kapsamı yok.
  - `WindowHit` (`crates/barepdf-platform-windows/src/window_chrome.rs:17` ve `non_windows.rs:310`) → Windows özel çerçevesiz başlık çubuğu (`no-frame: true`) sürükleme ve kenar boyutlandırma hit-test köprüsü.
- **`main run event_pump hud_commands barepdf-ui` (`41` sembol / `5` dosya):**
  - `run` (`apps/barepdf/src/presentation/ui.rs:80`) → `apps/barepdf/src/presentation/mod.rs:13` üzerinden `apps/barepdf/src/main.rs:24` tarafından çağrılıyor.
  - `ALL_HUD_COMMANDS` (`apps/barepdf/src/presentation/hud_commands.rs:15`) ve `filter_hud_commands` (`hud_commands.rs:93`) → `apps/barepdf/src/presentation/callbacks/features.rs` ve `ui.rs` içinde Komut Paleti (`Ctrl+K`) veri kaynağını oluşturuyor.
- **`ToolsModal FloatingToastCard update_ui tools callbacks` (`codegraph explore`):**
  - `connect_tools_callbacks` (`apps/barepdf/src/presentation/callbacks/tools.rs:285`) → `AppWindow` üzerindeki araç seçim, önizleme ve worker olaylarını bağlar.
  - `UpdateConsent` / `UpdateUiState` / `UpdateController` (`apps/barepdf/src/application/update_controller.rs`) → `render_update_ui` (`presentation/update_ui.rs`) üzerinden banner ve toast durumunu günceller.
- **`Preferences theme language shortcuts` (`35` sembol / `4` dosya):**
  - `UserPreferences` (`crates/barepdf-core/src/preferences.rs:37`) → `18` çağırıcı (`preferences_store.rs`, `update_ui.rs`, `ui.rs` vb.) ile pencere boyutu, tema, dil, kağıt tonu, renk ters çevirme, açık sekmeler ve son dosyaları kalıcı JSON olarak saklar (`MAX_OPEN_TABS = 16`, `MAX_RECENT_FILES = 10`).

---

## 2. Workspace ve Crate Mimarisi

Root [Cargo.toml](file:///D:/BarePDF/Cargo.toml) (`[workspace.package].version = "1.25.3"`, `edition = "2021"`, `rust-version = "1.92"`, `license = "MIT"`) altında `10` crate yer alır:

| Crate | Yol | Sorumluluk |
| --- | --- | --- |
| `barepdf-core` | [crates/barepdf-core](file:///D:/BarePDF/crates/barepdf-core/Cargo.toml) | Domain tipleri (`ViewingMode`, `ZoomMode`, `ReadingDirection`), sayfa düzeni (`layout.rs`), kaynak limitleri (`limits.rs`), metin seçimi/arama, tercih modeli (`preferences.rs`) |
| `barepdf-pdf` | [crates/barepdf-pdf](file:///D:/BarePDF/crates/barepdf-pdf/Cargo.toml) | `PdfBackend` trait'i ve `pdfium-render` (`pdfium_7881`) tabanlı PDFium adaptörü, PDF operasyonları (`operations.rs`) ve raster/görüntü dönüşümü |
| `barepdf-render` | [crates/barepdf-render](file:///D:/BarePDF/crates/barepdf-render/Cargo.toml) | Talep odaklı `RenderScheduler`, arka plan PDFium aktörü, üretim token'ı (generation-token) iptali ve bayt bütçeli LRU bitmap önbelleği |
| `barepdf-platform` | [crates/barepdf-platform](file:///D:/BarePDF/crates/barepdf-platform/Cargo.toml) | Soyut platform arayüzleri (`printing`, `watcher`, `PlatformError`) |
| `barepdf-platform-windows` | [crates/barepdf-platform-windows](file:///D:/BarePDF/crates/barepdf-platform-windows/Cargo.toml) | Win32 dosya diyalogları, pano (`arboard`), Win32/XPS yazdırma, çerçevesiz pencere DWM/hit-test (`window_chrome.rs`), `SystemParametersInfoW` erişilebilirlik efekt algılama (`reduce_visual_effects`), dosya bırakma (`drop_target.rs`) |
| `barepdf-platform-linux` | [crates/barepdf-platform-linux](file:///D:/BarePDF/crates/barepdf-platform-linux/Cargo.toml) | Linux dosya diyalogları, `arboard` pano, CUPS/lpr yazdırma, `xdg-open` kabuk entegrasyonu |
| `barepdf-i18n` | [crates/barepdf-i18n](file:///D:/BarePDF/crates/barepdf-i18n/Cargo.toml) | `Language` (`System`, `English`, `Turkish`) çözümlemesi ve statik İngilizce/Türkçe sözlük tabloları (`t(lang, key)`) |
| `barepdf-ui` | [crates/barepdf-ui](file:///D:/BarePDF/crates/barepdf-ui/Cargo.toml) | Tüm `.slint` görünümleri, `ThemeTokens`, diyaloglar ve Rust'a ihraç edilen `AppWindow` + veri yapıları |
| `barepdf-thumbnail` | [crates/barepdf-thumbnail](file:///D:/BarePDF/crates/barepdf-thumbnail/Cargo.toml) | Windows Explorer PDF küçük resim COM işleyicisi (`cdylib` + `rlib`) |
| `barepdf` | [apps/barepdf](file:///D:/BarePDF/apps/barepdf/Cargo.toml) | Masaüstü ikilisi; `application` (`DocumentController`, `PrintController`, `UpdateController`, `Tabs`), `infrastructure`, `controllers` (`search`, `bookmark`), `presentation` katmanları |

---

## 3. Slint–Rust Köprüsü ve UI Yaşam Döngüsü

1. **Derleme Zamanı Makro Bağlantısı:** [crates/barepdf-ui/src/lib.rs](file:///D:/BarePDF/crates/barepdf-ui/src/lib.rs#L1-L33) dosyası `slint::slint!` makrosu ile [app_window.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint) bileşenlerini derler ve `AppWindow`, `ThemeTokens`, `TabItem`, `SelectionBox`, `OverlayRectData`, `FreeTextItem`, `BookmarkItem`, `PageItem`, `ThumbnailItem`, `OutlineItem`, `RecentFileItem` tiplerini Rust'a açar (`Cargo.toml` üzerinde `slint` özellikleri: `std`, `backend-winit`, `renderer-femtovg`, `renderer-software`, `accessibility`, `compat-1-2`).
2. **Başlatma ve Tercih Yükleme:** `apps/barepdf/src/presentation/ui.rs:80` (`run`) PDFium motorunu başlatır; hata varsa yerel iletişim kutusu (`show_fatal_error`) açar. İlk çalıştırmada `preferences.update_checks_enabled == None` ise Slint penceresi açılmadan önce yerel `ask_yes_no` penceresiyle güncelleme izni sorar (`ui.rs:94-101`).
3. **Pencere Oluşturma ve Başlangıç Durumu:** `AppWindow::new()?` sonrası `LogicalSize` en az `760×520` (`preferences.last_window_width/height`, varsayılan `1100×800`) olarak ayarlanır. `initialize_window` (`ui.rs:253`) tema (`ThemeTokens.theme_mode`), `paper-tint`, `invert-page-colors`, `system-reduce-effects` (`reduce_visual_effects()`), akıllı tahta ölçeği (`update_smartboard_scale`), i18n metinleri (`update_ui_strings`, `ui.rs:1740`) ve başlangıç Komut Paleti öğelerini atar.
4. **Callback Bağlama:** `wire_callbacks` (`apps/barepdf/src/presentation/callbacks/mod.rs:73`) dosya açma, gezinme, yakınlaştırma, görünüm, metin seçimi, sekmeler, yazdırma, araçlar, niş özellikler (arama, yer imi, içindekiler, komut paleti), çizim/imza ve pencere kromu callback'lerini bağlar.
5. **16 ms / 250 ms Olay Pompası:** [event_pump.rs](file:///D:/BarePDF/apps/barepdf/src/presentation/event_pump.rs#L21-L222) `slint::Timer` üzerinden aktifken `16 ms` (`ACTIVE_INTERVAL`), boştayken `250 ms` (`IDLE_INTERVAL`) aralıkla; sürükle-bırak yolları, güncelleme olayları, yazdırma olayları, PDF araçları olayları, arka plan G/Ç olayları, dosya değişiklikleri, görünüm kaydırma değişiklikleri (`process_view_changes`) ve render olaylarını en fazla `EVENTS_PER_TICK = 4` partiyle UI thread'ine aktarır.
6. **Kapanış:** `window.run()` sonrasında pencere mantıksal boyutu, maximize durumu ve açık sekme/sayfa/kaydırma/yer imi oturumu `UserPreferences` içine alınıp atomik olarak kaydedilir (`ui.rs:212-222`).

---

## 4. Hedef Platformlar ve Çapraz Platform Davranışı

- **Windows 10 / 11 (x64):**
  - Birincil platformdur (`packaging/windows/installer/BarePDF.iss`, Inno Setup kullanıcı bazlı kurulum `%LOCALAPPDATA%\Programs\BarePDF`, taşınabilir `.zip`, WinGet).
  - Çerçevesiz pencere (`no-frame: true`) üzerinde `crates/barepdf-platform-windows/src/window_chrome.rs` ile Win32 `WM_NCLBUTTONDOWN` (`HTCAPTION`, kenar hit-test) ve `DwmSetWindowAttribute` (`DWMWCP_ROUND`) uygulanır.
  - `reduce_visual_effects()` Win32 `SystemParametersInfoW` (`SPI_GETHIGHCONTRAST`, `SPI_GETCLIENTAREAANIMATION`) okuyarak `ThemeTokens.system-reduce-effects` değerini besler.
  - Windows Explorer önizleme DLL'i (`BarePDF.Thumbnail.dll`) ve `windows_subsystem = "windows"` manifesti (`apps/barepdf/build.rs`) mevcuttur.
- **Linux (Pardus 23/25, Debian 12+, Ubuntu 22.04+, x86_64):**
  - `.github/workflows/build-linux.yml` `ubuntu-22.04` (glibc 2.35) üzerinde `cargo deb` (`.deb`) ve `AppImage` üretir; [barepdf.desktop](file:///D:/BarePDF/packaging/linux/barepdf.desktop) içinde Türkçe (`PDF Görüntüleyici`) ve İngilizce masaüstü girişi yer alır.
  - `apps/barepdf/Cargo.toml` koşulsuz olarak `barepdf-platform-windows` crate'ine bağlıdır; Linux derlemelerinde [non_windows.rs](file:///D:/BarePDF/crates/barepdf-platform-windows/src/non_windows.rs) yedek (`rfd`, `arboard`) implementasyonu devreye girer. Ancak `non_windows.rs:383` içinde `reduce_visual_effects()` ve `window_chrome.rs` içinde `#[cfg(not(windows))]` `winit` üzerinden `drag_window()` / `drag_resize_window()` kullanılır.
  - **Yapısal Not:** [preferences_store.rs](file:///D:/BarePDF/apps/barepdf/src/infrastructure/preferences_store.rs#L19-L24) `default_config_path()` için yalnızca `APPDATA` ortam değişkenine bakar; `APPDATA` tanımlı olmayan Linux sistemlerinde göreli `"config.json"` yoluna düşer (`XDG_CONFIG_HOME` kullanılmıyor).

---

## 5. Marka, Tema Token'ları ve Tasarım Dili

[tokens.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint) (`ThemeTokens` global) Windows 11 nötr yüzeylerini Warm Amber marka kimliğiyle birleştirir:

| Kategori | Token | Açık Mod | Koyu Mod | Not / Kaynak |
| --- | --- | --- | --- | --- |
| Marka Vurgusu | `accent` / `accent-hover` / `accent-pressed` | `#f69423` / `#faad52` / `#df7d13` | Aynı | [tokens.slint:40-42](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L40-L42); [logo.svg](file:///D:/BarePDF/assets/logo.svg) turuncu `#f69423` ile uyumlu |
| Vurgu Üzeri Metin | `accent-content` | `#181105` | `#181105` | [tokens.slint:43](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L43) (Ancak `TouchButton` `#ffffff` kullanıyor — Faz 3–4'te ölçülecek) |
| Yüzeyler | `window` / `layer` / `layer-elevated` | `#f3f3f3` / `#fbfbfb` / `#ffffff` | `#202020` / `#2b2b2b` / `#323232` | [tokens.slint:18-20](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L18-L20) |
| Tuval & Kağıt Tonu | `canvas` / `paper-tint` (`0..3`) | Normal `#e6e6e6` | Normal `#181818` | `1: Sepia (#f4ecd8 / #2c2218)`, `2: Night (#111111 / #dcdcdc)`, `3: Amber (#1a1600 / #f69423)` ([tokens.slint:21-24,50-53](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L21-L24)) |
| Kontrol Durumları | `control` / `subtle` / `hover` / `pressed` | `#ffffff` / `#00000008` / `#eaeaea` / `#dfdfdf` | `#2d2d2d` / `#ffffff0d` / `#383838` / `#242424` | [tokens.slint:25-28](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L25-L28) |
| Metin & Sınırlar | `text` / `text-muted` / `stroke` / `divider` | `#1b1b1b` / `#333333` / `#dcdcdc` / `#e2e2e2` | `#f5f5f5` / `#d0d0d0` / `#3a3a3a` / `#333333` | [tokens.slint:36-39](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L36-L39) |
| Durum & Odak | `danger` / `focus` / `selection` | `#c42b1c` / `#9a4100` / `#f6942326` | `#ff99a4` / `#f6a14a` / `#f6942333` | [tokens.slint:44-47](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L44-L47) |
| Akıllı Tahta Ölçeği | `smartboard-mode` / `scale-multiplier` | `1.0` (`touch-min: 32px`) | `width >= 3840` → `1.5` (`touch-min: 48px`) | [tokens.slint:29-34](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L29-L34); [ui.rs:1988-2003](file:///D:/BarePDF/apps/barepdf/src/presentation/ui.rs#L1988-L2003) |
| Hareket & Geometri | `motion-fast` / `motion-medium` / `bar-height` | `80ms` / `120ms` (`system-reduce-effects` aktifse `0ms`) | `bar-height: 40px`, `control-height: 32px`, `radius: 4/8px` | [tokens.slint:7-9,56-69](file:///D:/BarePDF/crates/barepdf-ui/ui/tokens.slint#L7-L9) |

---

## 6. Çalıştırma, Test ve Ekran Görüntüsü Durumu

- **Derleme ve Test Altyapısı:** Depoda `target/debug` dizini mevcuttur; `target/release/barepdf.exe` henüz derlenmemiştir (`@ram-bench` tarafından derlenecek). [headless_smoke_test.rs](file:///D:/BarePDF/crates/barepdf-ui/tests/headless_smoke_test.rs) pencere açmadan `AppWindow` model bağlamalarını ve callback'leri test eder; [crates/barepdf-ui/src/lib.rs](file:///D:/BarePDF/crates/barepdf-ui/src/lib.rs#L35-L274) dışa açılan tipleri ve diyalog sözleşmelerini kontrol eder.
- **Mevcut Ekran Görüntüsü Kanıtı:** [website/public/images/app-interface.webp](file:///D:/BarePDF/website/public/images/app-interface.webp) dosyası görsel olarak incelendi:
  - Koyu modda (`#202020` / `#181818`) özel başlık çubuğu (`barepdf-welcome.pdf` aktif sekmesi, `+` butonu, sağda Windows tarzı küçült/ekranı kapla/kapat butonları) görülüyor.
  - İkinci satırda araç çubuğu (`Open PDF`, kenar çubuğu düğmesi, `< 1 / 2 >`, `- 109% +`, sığdır açılır oku, ara, `Draw`, `Sign`, `Print`, `PDF Tools`, `...`, ayarlar dişlisi) yer alıyor.
  - Solda 3 sekmeli (`Thumbnails`, `Outline`, `Bookmarks`) kenar çubuğu; seçili 1. sayfa küçük resminin etrafında turuncu `#f69423` çerçeve, sağda beyaz harfli iki sayfalık manifesto kapağı render'ı görülüyor.
  - Boş durum, Ayarlar flyout'u, Komut Paleti, Araçlar Modalı, Yazdırma Önizleme ve Açık Mod ekran görüntüleri henüz mevcut değil (`@website-swiss` yeni release ikilisinden ekran görüntüleri alacak).

---

## 7. Varsayımlar ve Doğrulanamayan Sınırlar

1. **Çalışma Zamanı ve Ekran Okuyucu (AccessKit) Doğrulaması (`doğrulanmadı`):** Canlı uygulama penceresi üzerinde Windows Narrator / NVDA veya Linux Orca (AT-SPI) ile gerçek odak sırası, canlı bölge duyuruları (`accessible-live-region`) ve High Contrast teması bu fazda çalıştırılmamıştır; bulgular statik `.slint` ve Rust kaynak kod incelemesine dayanacaktır.
2. **Paralel Değişiklikler (`varsayım` / beklemede):** `Cargo.toml` (`slint 1.9` → `1.18.1`), `tools_dialog.slint`, `app_window.slint` ve `toast.slint` dosyaları eşzamanlı olarak `@deps-ci`, `@pdf-tools` ve `@update-log` tarafından güncellenmektedir. `00-kesif.md` içindeki sürüm ve pencere kabuğu gözlemleri mevcut `HEAD` (`666603f`) durumunu yansıtır; final bileşen envanteri ve metrikler bu ajanların `[done]` sinyali sonrasında güncellenmiş dosyalar üzerinden alınacaktır.
3. **Linux Pencere ve DPI Ölçekleme Davranışı (`doğrulanmadı`):** `winit` altında `no-frame: true` pencere sürükleme/kenar boyutlandırma ve X11/Wayland DPI ölçekleme davranışı Windows ortamında olduğumuz için yalnızca kaynak koddan (`window_chrome.rs`) okunmuştur; canlı Linux/Pardus testi yapılmamıştır.
4. **Görsel Yalnızca Koyu Mod Örneği:** Mevcut `app-interface.webp` görseli yalnızca ana okuyucu ekranının koyu temasını gösterir; alt diyalogların piksel hizalaması ve taşma davranışı canlı pencere yakalaması olmadan kod hesaplamalarıyla (`min-width`, `preferred-width`, `height`) analiz edilecektir.
