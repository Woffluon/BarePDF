# Faz 1: Araştırma (Masaüstü UX Pratikleri, Sezgisel Kurallar, WCAG 2.2 AA ve Slint Sınırları)

Bu belge, BarePDF UI/UX denetiminin (Faz 2–5) dayandığı nesnel değerlendirme çerçevesini tanımlar. Her kural doğrudan masaüstü PDF okuyucu/düzenleyici iş akışlarına ve Slint + AccessKit mimarisine uyarlanmıştır.

---

## 1. Masaüstü PDF Okuyucu/Düzenleyici UX Pratikleri

Windows 11 (Fluent / WinUI 3) ve Linux masaüstü (GNOME HIG / KDE / Pardus XFCE-GNOME) ortamlarında belge odaklı uygulamalar için temel beklentiler:

1. **İçerik Önceliği ve Sessiz Krom:** Okuma tuvali (PDF sayfası) ekran alanının baskın kısmını kaplamalıdır. Araç çubukları, kenar çubukları ve bildirimler içeriği itmek veya okuma odağını bozmak yerine sakin, hiyerarşik bir çerçeve sunmalıdır.
2. **Mod Farkındalığı ve Görsel Geri Bildirim:** Çizim (`Draw`), Kaydırma Eli (`Pan`), Metin Notu yerleştirme veya İmza yerleştirme gibi imleç/tıklama anlamını değiştiren modlar kullanıcının göz hizasında açıkça görünmeli ve tek tuşla (`Esc` veya aynı araç düğmesi) iptal edilebilmelidir.
3. **Yıkıcı İşlemlerde Güvenlik ve Geri Alınabilirlik:** PDF sayfa silme, üzerine kaydetme (`Save`), çizimleri atma (`Discard`) veya uygulama güncellemesi başlatma gibi işlemlerde veri kaybını önleyen açık geri alma (`Undo`/`Redo`), kaydedilmemiş değişiklik uyarısı ve net durum mesajları bulunmalıdır.
4. **Klavye-Öncelikli Akış ve Kısayol Keşfedilebilirliği:** Masaüstü kullanıcıları `Ctrl+O`, `Ctrl+F`, `Ctrl+P`, `Ctrl+K`, `Ctrl+Z/Y`, `Ctrl+Tab`, ok tuşları ve `Esc` bekler. Kısayollar menü öğelerinde ve araç ipuçlarında görünür olmalı; `Tab` / `Shift+Tab` ile her etkileşimli kontrole ulaşılabilmelidir.
5. **Yerel Platform Uyumluluğu:** Çerçevesiz özel pencere (`no-frame: true`) kullanıldığında bile Windows Snap Layout / sürükleme / çift tıkla büyütme / kenar boyutlandırma ve Linux `winit` pencere kuralları ile klavye sistem kısayolları (`Alt+F4`, `F11`) tutarlı çalışmalıdır.
6. **Dil ve Terminoloji Tutarlılığı (i18n):** Seçilen dil (`English` / `Türkçe`) tüm yüzeylerde (başlık çubuğu, araç çubukları, boş durumlar, diyaloglar, hata mesajları, erişilebilirlik etiketleri, komut paleti) tutarlı uygulanmalıdır; tek bir dilde kalan sabit kodlu metinler güven ve anlaşılırlığı zedeler.

---

## 2. NN/g 10 Kullanılabilirlik Sezgisel Kuralı (PDF Uygulamasına Uyarlama)

| # | Sezgisel İlke | BarePDF Denetim Ölçütü |
| --- | --- | --- |
| **H1** | Sistem durumunun görünürlüğü | Büyük PDF yüklenirken, araç worker'ı çalışırken, yazdırma/güncelleme indirilirken ilerleme yüzdesi, kalan adım veya net durum metni görünür mü? |
| **H2** | Sistem ile gerçek dünya eşleşmesi | PDF terimleri (Kırpma boşluğu pt, DPI, Birleştir/Ayır, Ön/Arka Yüz, Sayfa Sıralama) kullanıcı zihin modeline uygun mu; ham teknik kodlar/İngilizce karışımları var mı? |
| **H3** | Kullanıcı kontrolü ve özgürlüğü | Tüm diyaloglar (`ToolsModal`, `SignatureDialog`, `TextAnnotationDialog`, `PrintPreview`, `PasswordDialog`), açılır menüler ve modlar `Esc` ile veya tek tıkla kapanabiliyor mu; yanlış işlem geri alınabiliyor mu? |
| **H4** | Tutarlılık ve standartlar | Aynı işlevi gören butonlar, kapatma ikonları, etiket yerleşimleri, vurgu renkleri (`ThemeTokens.accent`) ve diyalog kalıpları (`Dialog` vs özel kaplamalar) uygulama genelinde tutarlı mı? |
| **H5** | Hata önleme | Geçersiz sayfa aralığı, boş metin notu, seçili dosya yokluğu veya kaydedilmemiş çizimlerle sekme/pencere kapatma durumlarında eylem önceden engelleniyor veya uyarılıyor mu? |
| **H6** | Hatırlama yerine tanıma | Sayfa düzenleme ve kırpmada kullanıcıdan sözdizimi (`"1-3, 5"`) veya görselsiz sıra numarası ezberlemesi mi bekleniyor, yoksa görsel kartlar/şablonlar mı sunuluyor? |
| **H7** | Esneklik ve kullanım verimliliği | İleri düzey kullanıcılar için Komut Paleti (`Ctrl+K`), klavye kısayolları ve doğrudan sayısal giriş; dokunmatik/akıllı tahta kullanıcıları için büyük hedefler ve numpad mevcut mu? |
| **H8** | Estetik ve minimalist tasarım | Araç çubukları kalabalıklaşınca taşma menüsü (`...`) ve ikincil hiyerarşi temiz çalışıyor mu; kullanılmayan/çiftlenmiş kontroller ekranı kirletiyor mu? |
| **H9** | Hataları tanıma, teşhis etme ve kurtarma | Şifre hatası, yazdırma aralığı hatası veya araç çalıştırma hatası ham sistem/İngilizce hata yerine kullanıcıya neyin yanlış olduğunu ve ne yapması gerektiğini söylüyor mu? |
| **H10** | Yardım ve dokümantasyon | Kısayol listesi, boş durum yönlendirmesi, sürükle-bırak ipuçları ve biçim açıklamaları gerektiği yerde anlaşılır şekilde sunuluyor mu? |

---

## 3. WCAG 2.2 AA ve Tasarım Mühendisliği Ölçütleri

### 3.1 WCAG 2.2 Düzey AA Teknik Eşikleri
1. **SC 1.4.3 Kontrast (Minimum):** Normal metin (`< 18pt` veya kalın `< 14pt`) için en az **4.5:1**, büyük metin (`>= 18pt` / `24px` veya kalın `>= 14pt` / `~18.67px`) için en az **3.0:1** kontrast oranı. (Slint'in mantıksal pikselinde `12px–14px` UI metinleri normal metin sınıfındadır ve `>= 4.5:1` gerektirir.)
2. **SC 1.4.11 Metin Dışı Kontrast:** Aktif buton sınırları, ikonlar, kaydırıcı başlıkları, seçim rozetleri ve klavye odak halkaları (`ThemeTokens.focus`) çevre yüzeye göre en az **3.0:1** kontrasta sahip olmalıdır.
3. **SC 2.5.8 Hedef Boyutu (Minimum):** Etkileşimli hedefler en az **24×24** mantıksal piksel olmalı (veya 24px çaplı çakışmayan daire alanı bırakmalıdır). Akıllı tahta (`smartboard-mode`) durumunda hedef en az **48×48px** (`ThemeTokens.touch-min`) olmalıdır.
4. **SC 2.1.1 Klavye & SC 2.1.2 Klavye Tuzağı Olmaması:** Yalnızca `TouchArea` ile tanımlanmış, `FocusScope` ve `key-pressed` işleyicisi olmayan özel `Rectangle` butonlar, açılır listeler, renk örnekleri veya akordeonlar klavye ile erişilemez (WCAG 2.1.1 ihlali). Modallar açıldığında odak modal içine taşınmalı ve `Esc` / kapanışta tetikleyiciye geri dönmelidir.
5. **SC 2.4.7 Odak Görünürlüğü & SC 2.4.11 Odak Örtülmemesi:** Odaklanabilen her öğe görünür bir odak göstergesine sahip olmalı; kayan araç çubukları veya yapışkan alt çubuklar odaklanan öğeyi kapatmamalıdır.
6. **SC 4.1.2 Ad, Rol, Değer & SC 4.1.3 Durum Mesajları:** Her özel kontrolün doğru `accessible-role` ve anlamlı yerelleştirilmiş `accessible-label` değeri bulunmalı; hata veya toast/durum duyuruları `accessible-live-region` ile bildirilmelidir.

### 3.2 Apple Design & Emil Design Engineering İlkeleri
Atanmış `apple-design` ve `emil-design-eng` becerilerinden bu denetime uygulanan somut kurallar:
- **Gecikmeyi öldür & anlık basma tepkisi:** Pointer-down anında anlık görsel tepki (`pressed` durumu). UI animasyonları `< 300ms` (tercihen `120–200ms`) kalmalı; BarePDF'teki `80ms` (`motion-fast`) / `120ms` (`motion-medium`) süreleri hızlıdır ancak geçiş eğrisi (`easing`) tutarlılığı ve kesintiye uğrayabilirlik önemlidir.
- **Klavye ile tetiklenen eylemlere animasyon koyma:** Komut Paleti (`Ctrl+K`) veya Arama Çubuğu (`Ctrl+F`) gibi günde onlarca/yüzlerce kez açılan klavye yüzeyleri gecikmesiz görünmelidir.
- **Eğri ve kaynak uyumu:** Popover ve flyout'lar tetikleyici konumuna göre açılmalı; modallar merkezde durmalıdır. Animasyonsuz sert belirmeler yerine gerekli durumlarda hafif geçiş veya azaltılmış hareket (`system-reduce-effects` aktifse `0ms` / cross-fade) uygulanmalıdır.
- **Bileşik zanaat detayları (Unseen details compound):** Tutarlı 4px/8px ızgara aralığı, sayfa/sonuç sayaçlarında hizalama (`tabular` okuma düzeni), tek bir paylaşılan bileşen sistemi (ör. bölünmüş `TextButton` vs `TouchButton` veya farklı modal başlıkları yerine tek kanonik yapı).

---

## 4. Slint (`accessible-*`) ve AccessKit Mimari Sınırları

BarePDF [Cargo.toml](file:///D:/BarePDF/Cargo.toml#L36) içinde Slint'in `accessibility` özelliğini etkinleştirir (arka planda **AccessKit** ile Windows UI Automation ve Linux AT-SPI köprüsünü kurar). Ancak Slint'in bu sürümlerdeki mimarisi nedeniyle şu yapısal sınırlar geçerlidir:

1. **Örtük Semantik Eksikliği (Custom Primitives):** `std-widgets.slint` içindeki `LineEdit` veya `Button` dışındaki tüm özel `Rectangle` + `TouchArea` bileşenleri (ör. `PrintPreviewDialog` yazıcı açılır kutusu, `FilterPill`, `ZoomSlider`, `TextAnnotationDialog` renk örnekleri, `AppWindow` çizim renk/kalınlık butonları, `CommandPalette` satırları) açıkça `accessible-role`, `accessible-label` ve `FocusScope` eklenmedikçe ekran okuyuculara ve klavye `Tab` zincirine **görünmezdir**.
2. **Klavye Odak ve Aktivasyon Ayrımı:** Slint'te bir `Rectangle` üzerine `accessible-role: button` koymak **otomatik olarak `Tab` odaklanabilirliği veya `Enter`/`Space` aktivasyonu sağlamaz**. Bunun için bileşen içinde `FocusScope` (`enabled`, `key-pressed` → `" "` / `"\n"` kontrolü) ve görsel odak halkası (`has-focus`) açıkça yazılmalıdır (ör. [button.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/button.slint#L30-L60) bunu yapar, ancak [menu_item.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/menu_item.slint#L18-L74) ve [dialog.slint](file:///D:/BarePDF/crates/barepdf-ui/ui/components/dialog.slint#L62-L84) kapatma butonu bunu yapmaz).
3. **`FocusScope` Üzerinde Sıfır Genişlik Hilesi ve Riskleri:** `TextButton` ve `IconButton` içinde `focus-scope := FocusScope { x: 0px; width: 0px; }` kullanılmaktadır. Bu kalıp klavye odağını yakalasa da `x: 0; width: 0` geometri sınırının AccessKit ekran okuyucu odak dikdörtgenini (bounding box — ör. Narrator çerçevesi / ekran büyüteci takibi) `0px` genişlikte raporlama riski taşır (**doğrulanmadı** — canlı Windows Narrator/Accessibility Insights ile doğrulanması gerekir).
4. **AccessKit Rol Kapsamı:** Slint `accessible-role` kümesi sınırlıdır (`none`, `button`, `checkbox`, `combobox`, `groupbox`, `image`, `list`, `listview`, `progressbar`, `radiobutton`, `slider`, `spinbox`, `tab`, `tablist`, `textbox`, `text`, `switch` vb. sürüme bağlı); karmaşık PDF tuvali metin seçimi veya sayfa içi köprüler AccessKit metin ağacına doğrudan aktarılmaz (PDF içeriği bitmap olarak render edilir; metin geometrisi yalnızca seçim/arama içindir).
5. **Canlı Bölgeler (`accessible-live-region`) ve Toast Sınırı:** `PasswordDialog` ve `ToolsModal` hata kutularında `assertive`, `AppWindow` durum çubuğunda `polite` canlı bölge kullanılmıştır; ancak `FloatingToastCard` / `FloatingPrintCard` ve `CommandPalette` sonuç sayıları canlı bölge duyurusu verip vermediği Faz 2–4 envanterinde denetlenecektir.
