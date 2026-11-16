# Faz 2: Envanter (Ekranlar, Diyaloglar, Bileşen Haritası, Görev Adımları ve Sabit Değerler)

> **Doğrulama Durumu:** Bu envanter `@deps-ci`, `@update-log` ve `@pdf-tools` ajanlarının tüm güncellemeleri tamamlandıktan sonraki güncel kod tabanı (`app_window.slint`: `3.115` satır, `tools_dialog.slint`: `2.007` satır, `toast.slint`: `310` satır; toplam `26` `.slint` dosyası, `8.833` satır) üzerinden [slint_ui_audit.py](./araclar/slint_ui_audit.py), [task_steps.py](./araclar/task_steps.py) ve [i18n_audit.py](./araclar/i18n_audit.py) betikleriyle doğrulanmıştır.

---

## 1. Ekran ve Görünüm Modları Envanteri

| ID | Ekran / Görünüm | Konum ve Tetikleyici | İçerdiği Ana Öğeler |
| --- | --- | --- | --- |
| **E1** | Ana Kabuk — Çerçevesiz Üst Krom (`window-mode == 0`) | [app_window.slint:774-1082](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L774-L1082), [title_bar.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/title_bar.slint) | `TitleBar` (`40px`: marka ikonu, yatay sekme şeridi `DocumentTab`, `+` butonu, Küçült/Ekranı Kapla/Kapat); `command-bar` (`40px`: Aç, Kenar Çubuğu, Önceki/Sayfa No `LineEdit`/Sonraki, `-`/`Zoom%`/`+`/`Fit ▾`, Ara, Çiz, Kaydır, İmzala, Yazdır, PDF Araçları, `⋯`, Ayarlar). `width < 900px` altında 4 eylem `⋯` menüsüne taşınır |
| **E2** | Boş Durum (`!has-document && !password-required`) | [app_window.slint:1227-1363](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L1227-L1363) | Marka başlığı, `96px` sürükle-bırak/tıkla aç kartı, birincil `TextButton` ("Open PDF"), en fazla `MAX_RECENT_FILES = 10` son dosya listesi |
| **E3** | Okuma Alanı — Kenar Çubuğu + Tuval | [app_window.slint:1087-1919](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L1087-L1919) | `260px` kenar çubuğu (`Thumbnails` / `Outline` / `Bookmarks` sekmeleri); sağda `ContinuousVertical` (`view-mode == 0`) veya `SinglePage` / `TwoPageSpread` / `BookMode` (`view-mode == 1..3`) tuvali, metin seçimi, arama vurguları, imza yerleştirme kutusu ve serbest metin katmanı |
| **E4** | Tam Ekran Modu (`window-mode == 1`, `F11`) | [app_window.slint:2704-2722](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L2704-L2722), [floating_capsule.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/floating_capsule.slint) | Üst krom/kenar çubuğu gizlenir; sağ üstte çıkış hapı, altta `FloatingCapsule` (`Önceki`, tıklanabilir `Sayfa/Toplam` numpad tetikleyicisi, `Sonraki`, `Geri Al`, `Zoom -/+`, `Sol/Merkez/Sağ` hizalama, `Çık`) |
| **E5** | Sunum Modu (`window-mode == 2`, `F5`) | [app_window.slint:703-772](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L703-L772) | `#08090b` arka plan üzerinde tek merkezlenmiş sayfa, sol tık ileri / sağ tık çıkış, klavye ok/Space/Backspace, `show-zoom: false` `FloatingCapsule` |
| **E6** | Kayan Çizim & Açıklama Araç Çubuğu | [app_window.slint:1922-2152](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L1922-L2152) | `52px` yükseklikli üst/alt hizalanabilir kayan çubuk: konum değiştirme, `Pan`, `Kalem`, `Silgi`, `Yazı (Type)`, 4 renk örneği, 3 kalınlık / 4 silgi boyutu, `Undo`/`Redo`/`Clear`, `Save`/`Save As`/`Discard` |

---

## 2. Diyalog, Popover ve Bildirim Envanteri

| ID | Diyalog / Overlay | Dosya ve Boyut | Temel İşlev ve Durum |
| --- | --- | --- | --- |
| **D1** | `ToolsModal` (Master-Detail PDF Araçları) | [tools_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/tools_dialog.slint) (`840×560px` maks.) | `current-tool: 0` ile açılır; solda `220px` 8 araç listesi (`Merge`, `Split`, `Delete`, `Rotate`, `Convert`, `Crop`, `Reorder`, `Compress`); sağda `SegmentedTrack` kontrolleri, çok sütunlu `PageSelectionGrid` (`96×126px` kartlar), `Crop` şablon + canlı önizleme, sürükle-bırak + ok tuşlu `Reorder` ve 3 modlu vektör-korumalı `Compress` özet kartı |
| **D2** | `PrintPreviewDialog` (Yazdırma Önizleme) | [print_preview_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/print_preview_dialog.slint) (`880×640px` maks.) | Sol sütunda 1-up / 2-up / 4-up / 6-up canlı sayfa önizleme ve sayfalama; sağ `320px` sütunda özel yazıcı açılır kutusu, kopya sayısı, sayfa aralığı (`Tümü/Geçerli/Özel`), "Daha Fazla Ayar" akordeonu (kağıt boyutu, çift taraflı, yönlendirme, ölçekleme, N-up) |
| **D3** | `PasswordDialog` (x2: Belge ve Araçlar) | [password_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/password_dialog.slint) (`420×236/284px`) | Parola girişi (`LineEdit`), `assertive` canlı hata bandı, Esc desteği ve kapanışta parola tamponu temizliği |
| **D4** | `SignatureDialog` (İmza Oluşturma) | [signature_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/signature_dialog.slint) (`480×380px`) | `Draw` / `Upload Image` sekmeleri, `ZoomSlider` kalınlık ayarı (`0.5–6.0 pt`), temizle/yerleştir akışı |
| **D5** | `TextAnnotationDialog` (Metin Notu) | [text_annotation_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/text_annotation_dialog.slint) (`440×280px`) | Metin girişi, `12/14/18/24pt` boyut seçimi, 4 renk örneği, `Esc` desteği |
| **D6** | Kaydedilmemiş Çizimler Onay Modalı (Satır içi) | [app_window.slint:2794-2923](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L2794-L2923) (`400×220px`) | `Dialog` bileşenini kullanmak yerine satır içi yazılmış; Türkçe sabit metinler (`"Kaydedilmemiş Çizimler Var"`, `"Kaydetmeden Kapat"`, `"İptal"`) |
| **D7** | `PreferencesDialog` (**Ölü Bileşen**) | [preferences_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/preferences_dialog.slint) (`480×360px`) | Hiçbir yerden içe aktarılmıyor. İçinde `Theme`, `Paper Tint`, `Invert colors` ve `Language` seçicileri sabit İngilizce metinle tanımlı |
| **P1** | Ayarlar Flyout'u (Aktif Ayarlar Paneli) | [app_window.slint:2174-2272](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L2174-L2272) (`360px` sağ çekmece) | Tema (`System/Light/Dark`), Renk ters çevirme (`"Off"`/`"On"`), Dil, Güncelleme izin/kontrolü ve Hakkında linkleri. **Not:** `paper-tint` seçeneği burada bulunmuyor |
| **P2** | `CommandPalette` (`Ctrl+K`) | [command_palette.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/command_palette.slint) (`520×360px`) | `hud_commands.rs` içindeki `14` sabit İngilizce komut ve sayfa numarası atlama girişi |
| **P3** | `SearchBar` (`Ctrl+F`) | [search_bar.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/search_bar.slint) (`480×44px`) | Arama kutusu, `current / total` sayacı, `↑`/`↓`, `Aa` (büyük/küçük harf), `W` (tam sözcük), kapat |
| **P4** | `ContextMenu` (Sağ Tık) | [context_menu.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/context_menu.slint) (`210px`) | Seçim varsa `Kopyala`/`Belgede Ara`/`Vurgula`; genel `Tümünü Seç`/`Saat Yönünde Döndür`/`Sayfaya Sığdır`/`Önceki`/`Sonraki` |
| **P5** | `Flyout` (`Fit ▾` ve `⋯ More`) | [app_window.slint:2517-2672](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L2517-L2672), [flyout.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/flyout.slint) | `Fit ▾` (`190×112px`) ve `⋯ More` (`240×182px` veya `<900px` iken `240×286px`) |
| **P6** | `NumericKeypadPopover` | [numpad_popover.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/numpad_popover.slint) (`280×360px`) | Tam Ekran / Sunum kapsülü sayfa numarası göstergesinden açılan 3×4 dokunmatik tuş takımı |
| **P7** | `FloatingPrintCard` & `FloatingToastCard` | [toast.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/toast.slint) (`340px`), [app_window.slint:2957-3006](file:///D:/BarePDF/crates/barepdf-ui/ui/app_window.slint#L2957-L3006) | Sağ üst köşede (`y: 48px` ve `132px`) yazdırma durumu ve güncelleme indirme `4px` ilerleme çubuğu + hata/bilgi bildirimleri |
| **P8** | `Scrubber` | [scrubber.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/scrubber.slint) (`90×32px`) | Küçük resim sürükleme sayfası önizleme balonu |

---

## 3. Bileşen Kullanım Haritası (`slint_ui_audit.py` Çıktısı)

| Bileşen | Tanım Dosyası | Referans Sayısı | Kullanan Dosyalar |
| --- | --- | ---: | --- |
| `TextButton` | [button.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/button.slint) | 10 | `app_window`, `context_menu`, `search_bar`, `toast`, `password_dialog`, `preferences_dialog`, `print_preview_dialog`, `signature_dialog`, `text_annotation_dialog`, `tools_dialog` |
| `DialogFooter` | [dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/dialog.slint) | 5 | `password_dialog`, `print_preview_dialog`, `signature_dialog`, `text_annotation_dialog`, `tools_dialog` |
| `Dialog` | [dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/dialog.slint) | 4 | `app_window`, `preferences_dialog`, `signature_dialog`, `text_annotation_dialog` |
| `IconButton` | [icon_button.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/icon_button.slint) | 3 | `app_window`, `floating_capsule`, `search_bar` |
| `MenuItem` | [menu_item.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/menu_item.slint) | 2 | `app_window`, `context_menu` |
| `TextAnnotationDialog` | [text_annotation_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/text_annotation_dialog.slint) | 2 | `app_window`, `crates/barepdf-ui/src/lib.rs` (test) |
| `TitleBar`, `DocumentTab`, `CommandPalette`, `ContextMenu`, `FloatingCapsule`, `Flyout`, `NumericKeypadPopover`, `Scrubber`, `SearchBar`, `ZoomSlider`, `FloatingToastCard`, `FloatingPrintCard`, `TouchButton`, `PasswordDialog`, `PrintPreviewDialog`, `SignatureDialog`, `ToolsModal` | İlgili modüller | 1'er | Ana pencere veya üst bileşeni (`TitleBar` -> `DocumentTab`, `SignatureDialog` -> `ZoomSlider`, `NumericKeypadPopover` -> `TouchButton`) |
| `ToolSidebarItem`, `SegmentedTrack`, `SegmentedItem`, `ToolDropZone`, `PageSelectionCard`, `PageSelectionGrid`, `PageRangeBar` | [tools_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/tools_dialog.slint) | Yalnızca yerel | `tools_dialog.slint` içinde yerel (dışa aktarılmıyor; başka ekranlarda tekrar kullanılamıyor) |
| **`FilterPill`** | [filter_pill.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/filter_pill.slint) | **0** | **KULLANILMIYOR (Ölü Bileşen)** |
| **`PreferencesDialog`** | [preferences_dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/dialogs/preferences_dialog.slint) | **0** | **KULLANILMIYOR (Ölü Bileşen)** |

---

## 4. Ana Görevlerin Tıklama / Adım Sayıları (`task_steps.py` Çıktısı)

| Görev | Fare Adımı | Klavye Adımı | Kısayol | Modal Geçişi | Sözdizimi Gerekli | Gözlenen Sürtünme |
| --- | ---: | ---: | --- | ---: | :---: | --- |
| PDF Dosyası Açma | 2 | 2 | `Ctrl+O` | 1 | Hayır | Hızlı ve doğrudan |
| Sayfaya Git (Masaüstü) | 3 | 3 | `Ctrl+G / Ctrl+K` | 0 | Hayır | `LineEdit` erişilebilir etiketi eksik |
| Sayfaya Git (Numpad) | 4 | **N/A (eksik)** | Yok | 1 | Hayır | `FocusScope` ve kapatma (`X`) butonu yok |
| Belge İçi Arama + Sonraki | 3 | 3 | `Ctrl+F` | 0 | Hayır | Çalışıyor; açılışta otomatik odak tetikleme belirsiz |
| Tema / Kağıt Tonu / Dil | 2 | **N/A (eksik)** | Yok (yalnız `Ctrl+K` kağıt tonu) | 1 | Hayır | `paper-tint` Ayarlar panelinde yok; `"Off"`/`"On"` sabit İngilizce |
| Çizim Yapma ve Kaydetme | 4 | **N/A (eksik)** | `Ctrl+Z / Ctrl+Y` | 1 | Hayır | Renk/kalınlık/silgi butonlarında `accessible-*` ve `FocusScope` yok |
| İmza Oluşturma ve Yerleştirme | 5 | **N/A (eksik)** | Yok | 2 | Hayır | `SignatureDialog` üzerinde `Esc` / `FocusScope` yok; `14×14px` yeniden boyut tutamacı |
| Yazdırma (Önizleme + Yazdır) | 3 | 2 | `Ctrl+P` | 1 | Hayır | Özel yazıcı açılır kutusu ve "Daha Fazla Ayar" akordeonu klavyesiz |
| PDF Araçları (8 Araç) | 3 | 4 | Yok | 1 | Hayır | Master-Detail + görsel `Reorder` iyileşmesi sağlandı; modal açılışında `forward-focus` ve `DropZone` gözat düğmesi eksik |
| Komut Paleti Eylemi | 3 | 3 | `Ctrl+K` | 1 | Hayır | Ok tuşlarıyla listede gezinme yok; `More` menüsünden açılışta yanlış callback (`request-search-query("")`) çağrılıyor |

---

## 5. Tema Dışı Sabit (Hardcoded) Değerler Özeti

Detaylı dosya dökümü [04-olcumler.md](./04-olcumler.md) içindedir; öne çıkan toplamlar:
- **Tema Dışı Sabit Hex Renkler:** `120` adet (`#00000000` saydam hariç; `43` adet `#00000000` mevcut). En yoğun kaynaklar: `app_window.slint` (`36`), `print_preview_dialog.slint` (`28`), `tools_dialog.slint` (`19`), `toast.slint` (`14`), `touch_button.slint` (`6`), `text_annotation_dialog.slint` (`4`).
- **Tema Dışı Sabit UI Metinleri:** `52` adet bileşen gövdesi sabit metni (`preferences_dialog.slint`: `17`, `app_window.slint`: `12`, `tools_dialog.slint`: `6`, `toast.slint`: `5`, `print_preview_dialog.slint`: `3`, `text_annotation_dialog.slint`: `3`, `numpad_popover.slint`: `2`, `command_palette.slint`: `1`, `floating_capsule.slint`: `1`, `search_bar.slint`: `1`, `signature_dialog.slint`: `1`) + `27` varsayılan özellik metni. Ayrıca `hud_commands.rs` içinde `14` sabit İngilizce komut yer alır.
- **Tema Dışı Sabit Animasyon Süresi:** `0` (`animate` bildirimlerinin tamamı `ThemeTokens.motion-fast` `80ms` veya `ThemeTokens.motion-medium` `120ms` kullanır).
- **24×24px Altı Hedefler:** `8` konum (`app_window.slint:1518-1519,1679-1680,1878-1879` `14×14px` imza boyut tutamacı; `tab_bar.slint:71-72` `22×22px` masaüstü sekme kapatma butonu).
- **i18n Eksik Anahtarlar:** `PRINT` kategorisinde `2` TR anahtarı (`print.preview.cancel_tooltip`, `print.preview.range_accessible`), `TOOLS` kategorisinde `9` TR anahtarı ve `1` EN anahtarı eksik ([i18n_audit.py](./araclar/i18n_audit.py)).
