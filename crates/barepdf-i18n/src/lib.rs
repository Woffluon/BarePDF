use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    English,
    Turkish,
}

impl Language {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Language::System => "system",
            Language::English => "en",
            Language::Turkish => "tr",
        }
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Language::System => "System Default",
            Language::English => "English",
            Language::Turkish => "Türkçe",
        }
    }

    #[must_use]
    pub fn resolve(self) -> ResolvedLanguage {
        match self {
            Language::English => ResolvedLanguage::English,
            Language::Turkish => ResolvedLanguage::Turkish,
            Language::System => detect_system_language(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedLanguage {
    English,
    Turkish,
}

impl ResolvedLanguage {
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            ResolvedLanguage::English => "en",
            ResolvedLanguage::Turkish => "tr",
        }
    }
}

const ENGLISH_GENERAL: &[(&str, &str)] = &[
    ("app.title", "BarePDF"),
    ("open.file", "Open PDF"),
    ("open.file.tooltip", "Open PDF Document (Ctrl+O)"),
    ("sidebar.toggle", "Sidebar"),
    ("sidebar.thumbnails", "Thumbnails"),
    ("sidebar.outline", "Outline"),
    ("tab.new", "New tab"),
    ("view.mode", "View"),
    ("view.mode.continuous", "Continuous"),
    ("view.mode.single", "Single Page"),
    ("view.mode.two_page", "Two Pages"),
    ("view.mode.book", "Book View"),
    ("zoom.in", "Zoom In"),
    ("zoom.out", "Zoom Out"),
    ("zoom.label", "Zoom"),
    ("zoom.fit_width", "Fit Width"),
    ("zoom.fit_page", "Fit Page"),
    ("zoom.actual_size", "Actual Size"),
    ("fullscreen", "Full Screen"),
    ("presentation", "Presentation"),
    ("settings", "Settings"),
    ("settings.title", "Preferences"),
    ("settings.language", "Language"),
    ("settings.theme", "Theme"),
    ("settings.theme.system", "System"),
    ("settings.theme.light", "Light"),
    ("settings.theme.dark", "Dark"),
    ("settings.appearance", "Appearance"),
    ("settings.developer", "Developer"),
    ("settings.project_website", "Website"),
    ("settings.manifesto", "Manifesto"),
    ("settings.manifesto.button", "Read the Manifesto ↗"),
    ("settings.about", "About"),
    ("window.minimize", "Minimize"),
    ("window.maximize", "Maximize"),
    ("window.restore", "Restore"),
    ("window.close", "Close"),
    ("toolbar.fit", "Fit"),
    ("toolbar.search", "Find (Ctrl+F)"),
    ("toolbar.command_palette", "Command Palette (Ctrl+K)"),
    ("language.english", "English"),
    ("language.turkish", "Türkçe"),
    ("settings.view_mode", "Default View Mode"),
    ("settings.reading_dir", "Reading Direction"),
    ("settings.reading_dir.ltr", "Left to Right (LTR)"),
    ("settings.reading_dir.rtl", "Right to Left (RTL)"),
    ("settings.close", "Close"),
    (
        "command_palette_invert_colors_title",
        "Toggle Inverted Page Colors (Ctrl+I)",
    ),
    (
        "command_palette_invert_colors_desc",
        "High contrast inverted reading mode",
    ),
    ("settings_invert_colors", "Invert page colors"),
    ("updates", "Updates"),
    ("updates.enabled", "Enabled"),
    ("updates.disabled", "Disabled"),
    ("updates.check_now", "Check now"),
    ("updates.consent.title", "BarePDF updates"),
    (
        "updates.consent.body",
        "Allow BarePDF to check GitHub for updates? If enabled, it checks at most once every 24 hours. You can change this later in Settings.",
    ),
    ("updates.status.ready", "Ready to check for updates."),
    ("updates.status.checking", "Checking for updates..."),
    ("updates.status.current", "BarePDF is up to date."),
    (
        "updates.status.available",
        "A new BarePDF version is available.",
    ),
    (
        "updates.status.downloading",
        "Downloading and verifying the update...",
    ),
    ("updates.status.installing", "Starting the installer..."),
    (
        "updates.status.verified",
        "Update verified and ready to install.",
    ),
    ("updates.status.error", "The update could not be completed."),
    ("updates.action.download", "Download update"),
    ("updates.action.install", "Install update"),
    ("updates.action.release", "View release"),
    ("search.placeholder", "Find in document..."),
    ("search.no_matches", "No matches found"),
    ("search.counter", "{current} / {total}"),
    ("search.match_case", "Match Case"),
    ("search.whole_word", "Whole Word"),
    ("sidebar.bookmarks", "Bookmarks"),
    ("bookmark.added", "Bookmark added"),
    ("bookmark.removed", "Bookmark removed"),
    ("password.title", "Password Required"),
    (
        "password.desc",
        "This document is encrypted. Enter password to open:",
    ),
    ("password.placeholder", "Password"),
    ("password.unlock", "Unlock"),
    ("password.cancel", "Cancel"),
    ("password.error.too_long", "Password is too long."),
    ("page.previous", "Previous page"),
    ("page.next", "Next page"),
    ("page.thumbnail", "Page"),
    ("context.copy", "Copy"),
    ("context.select_all", "Select All"),
    ("status.ready", "Ready"),
    ("status.opening", "Opening document..."),
    ("status.opened", "{name} ({pages} pages)"),
    ("status.error", "Error: {error}"),
    ("empty.title", "No Document Loaded"),
    (
        "empty.desc",
        "Click 'Open PDF' or drag a PDF file here to begin reading.",
    ),
    ("outline.empty", "This document has no outline."),
    ("recent.title", "Recent files"),
    ("action.retry", "Retry"),
    ("action.dismiss", "Dismiss"),
    ("status.loading", "Loading"),
    ("status.reloaded", "Reloaded document"),
    ("document.reloaded", "Reloaded document"),
    ("toolbar.book", "Book View"),
];

const TURKISH_GENERAL: &[(&str, &str)] = &[
    ("app.title", "BarePDF"),
    ("open.file", "PDF Aç"),
    ("open.file.tooltip", "PDF Belgesi Aç (Ctrl+O)"),
    ("sidebar.toggle", "Kenar Çubuğu"),
    ("sidebar.thumbnails", "Sayfalar"),
    ("sidebar.outline", "İçindekiler"),
    ("tab.new", "Yeni sekme"),
    ("view.mode", "Görünüm"),
    ("view.mode.continuous", "Sürekli"),
    ("view.mode.single", "Tek Sayfa"),
    ("view.mode.two_page", "Çift Sayfa"),
    ("view.mode.book", "Kitap Görünümü"),
    ("zoom.in", "Yakınlaştır"),
    ("zoom.out", "Uzaklaştır"),
    ("zoom.label", "Yakınlaştırma"),
    ("zoom.fit_width", "Genişliğe Sığdır"),
    ("zoom.fit_page", "Sayfaya Sığdır"),
    ("zoom.actual_size", "Gerçek Boyut"),
    ("fullscreen", "Tam Ekran"),
    ("presentation", "Sunum"),
    ("settings", "Ayarlar"),
    ("settings.title", "Tercihler"),
    ("settings.language", "Dil"),
    ("settings.theme", "Tema"),
    ("settings.theme.system", "Sistem"),
    ("settings.theme.light", "Açık"),
    ("settings.theme.dark", "Koyu"),
    ("settings.appearance", "Görünüm"),
    ("settings.developer", "Geliştirici"),
    ("settings.project_website", "Web Sitesi"),
    ("settings.manifesto", "Bildiri"),
    ("settings.manifesto.button", "Manifestoyu oku ↗"),
    ("settings.about", "Hakkında"),
    ("window.minimize", "Simge durumuna küçült"),
    ("window.maximize", "Ekranı kapla"),
    ("window.restore", "Aşağı geri getir"),
    ("window.close", "Kapat"),
    ("toolbar.fit", "Sığdır"),
    ("toolbar.search", "Bul (Ctrl+F)"),
    ("toolbar.command_palette", "Komut Paleti (Ctrl+K)"),
    ("language.english", "İngilizce"),
    ("language.turkish", "Türkçe"),
    ("settings.view_mode", "Varsayılan Görünüm Mode"),
    ("settings.reading_dir", "Okuma Yönü"),
    ("settings.reading_dir.ltr", "Soldan Sağa (LTR)"),
    ("settings.reading_dir.rtl", "Sağdan Sola (RTL)"),
    ("settings.close", "Kapat"),
    (
        "command_palette_invert_colors_title",
        "Sayfa Renklerini Ters Çevir (Ctrl+I)",
    ),
    (
        "command_palette_invert_colors_desc",
        "Yüksek kontrastlı ters çevrilmiş okuma modu",
    ),
    ("settings_invert_colors", "Sayfa renklerini ters çevir"),
    ("updates", "Güncellemeler"),
    ("updates.enabled", "Açık"),
    ("updates.disabled", "Kapalı"),
    ("updates.check_now", "Şimdi denetle"),
    ("updates.consent.title", "BarePDF güncellemeleri"),
    (
        "updates.consent.body",
        "BarePDF'in GitHub üzerinden güncellemeleri denetlemesine izin verilsin mi? Etkinleştirilirse en fazla 24 saatte bir denetler. Bunu daha sonra Ayarlar'dan değiştirebilirsiniz.",
    ),
    ("updates.status.ready", "Güncelleme denetimine hazır."),
    ("updates.status.checking", "Güncellemeler denetleniyor..."),
    ("updates.status.current", "BarePDF güncel."),
    (
        "updates.status.available",
        "Yeni bir BarePDF sürümü mevcut.",
    ),
    (
        "updates.status.downloading",
        "Güncelleme indiriliyor ve doğrulanıyor...",
    ),
    ("updates.status.installing", "Kurulum başlatılıyor..."),
    (
        "updates.status.verified",
        "Güncelleme doğrulandı ve kuruluma hazır.",
    ),
    ("updates.status.error", "Güncelleme tamamlanamadı."),
    ("updates.action.download", "Güncellemeyi indir"),
    ("updates.action.install", "Güncellemeyi kur"),
    ("updates.action.release", "Sürümü görüntüle"),
    ("search.placeholder", "Belgede ara..."),
    ("search.no_matches", "Eşleşme bulunamadı"),
    ("search.counter", "{current} / {total}"),
    ("search.match_case", "Büyük/Küçük Harf Duyarlı"),
    ("search.whole_word", "Tam Sözcük"),
    ("sidebar.bookmarks", "Yer İmleri"),
    ("bookmark.added", "Yer imi eklendi"),
    ("bookmark.removed", "Yer imi kaldırıldı"),
    ("password.title", "Parola Gerekli"),
    (
        "password.desc",
        "Bu belge şifrelenmiş. Açmak için parolayı girin:",
    ),
    ("password.placeholder", "Parola"),
    ("password.unlock", "Kilidi Aç"),
    ("password.cancel", "İptal"),
    ("password.error.too_long", "Parola çok uzun."),
    ("page.previous", "Önceki sayfa"),
    ("page.next", "Sonraki sayfa"),
    ("page.thumbnail", "Sayfa"),
    ("context.copy", "Kopyala"),
    ("context.select_all", "Tümünü Seç"),
    ("status.ready", "Hazır"),
    ("status.opening", "Belge açılıyor..."),
    ("status.opened", "{name} ({pages} sayfa)"),
    ("status.error", "Hata: {error}"),
    ("empty.title", "Yüklü Belge Yok"),
    (
        "empty.desc",
        "Okumaya başlamak için 'PDF Aç' seçeneğine tıklayın veya bir PDF dosyasını buraya sürükleyin.",
    ),
    ("outline.empty", "Bu belgede içindekiler bulunmuyor."),
    ("recent.title", "Son dosyalar"),
    ("action.retry", "Tekrar dene"),
    ("action.dismiss", "Kapat"),
    ("status.loading", "Yükleniyor"),
    ("status.reloaded", "Belge güncellendi"),
    ("document.reloaded", "Belge güncellendi"),
    ("toolbar.book", "Kitap Görünümü"),
];

const ENGLISH_PRINT: &[(&str, &str)] = &[
    ("print.unavailable", "Printing is unavailable."),
    ("print.open_document", "Open a PDF before printing."),
    ("print.busy", "Another print job is already active."),
    ("print.start_failed", "Could not start printing."),
    (
        "print.dialog_unavailable",
        "The Windows print dialog is unavailable.",
    ),
    ("print.dialog_failed", "Could not open the print dialog."),
    ("print.queue_failed", "Could not queue the print job."),
    ("print.default_document", "BarePDF document"),
    ("print.action", "Print"),
    ("print.cancel", "Cancel"),
    ("toolbar.more", "More"),
    ("print.preview.title", "Print preview"),
    ("print.preview.cancel_tooltip", "Cancel print preview"),
    ("print.preview.empty", "Page preview"),
    ("print.preview.page", "Page"),
    ("print.preview.page_range", "Page range"),
    ("print.preview.range_placeholder", "All pages or 1-3"),
    ("print.preview.range_accessible", "Print page range"),
    ("print.preview.orientation", "Orientation"),
    ("print.preview.orientation.auto", "Auto"),
    ("print.preview.orientation.portrait", "Portrait"),
    ("print.preview.orientation.landscape", "Landscape"),
    ("print.preview.continue", "Continue"),
    ("print.status.preparing", "Preparing print job…"),
    ("print.status.cancelling", "Cancelling print job…"),
    ("print.status.progress", "Printing page"),
    ("print.status.complete", "Printing complete."),
    ("print.status.cancelled", "Printing cancelled."),
    ("print.status.failed", "Printing failed."),
];

const TURKISH_PRINT: &[(&str, &str)] = &[
    ("print.unavailable", "Yazdırma kullanılamıyor."),
    ("print.open_document", "Yazdırmadan önce bir PDF açın."),
    ("print.busy", "Başka bir yazdırma işi zaten etkin."),
    ("print.start_failed", "Yazdırma başlatılamadı."),
    (
        "print.dialog_unavailable",
        "Windows yazdırma iletişim kutusu kullanılamıyor.",
    ),
    ("print.dialog_failed", "Yazdırma iletişim kutusu açılamadı."),
    ("print.queue_failed", "Yazdırma işi kuyruğa eklenemedi."),
    ("print.default_document", "BarePDF belgesi"),
    ("print.action", "Yazdır"),
    ("print.cancel", "İptal"),
    ("toolbar.more", "Diğer"),
    ("print.preview.title", "Yazdırma önizlemesi"),
    (
        "print.preview.cancel_tooltip",
        "Yazdırma önizlemesini iptal et",
    ),
    ("print.preview.empty", "Sayfa önizlemesi"),
    ("print.preview.page", "Sayfa"),
    ("print.preview.page_range", "Sayfa aralığı"),
    ("print.preview.range_placeholder", "Tüm sayfalar veya 1-3"),
    (
        "print.preview.range_accessible",
        "Yazdırılacak sayfa aralığı",
    ),
    ("print.preview.orientation", "Yönlendirme"),
    ("print.preview.orientation.auto", "Otomatik"),
    ("print.preview.orientation.portrait", "Dikey"),
    ("print.preview.orientation.landscape", "Yatay"),
    ("print.preview.continue", "Devam Et"),
    ("print.status.preparing", "Yazdırma işi hazırlanıyor…"),
    ("print.status.cancelling", "Yazdırma işi iptal ediliyor…"),
    ("print.status.progress", "Sayfa yazdırılıyor"),
    ("print.status.complete", "Yazdırma tamamlandı."),
    ("print.status.cancelled", "Yazdırma iptal edildi."),
    ("print.status.failed", "Yazdırma başarısız oldu."),
];

const ENGLISH_TOOLS: &[(&str, &str)] = &[
    ("tools.title", "PDF Tools"),
    ("tools.tooltip", "PDF Operations and Tools"),
    ("tools.merge", "Merge PDFs"),
    (
        "tools.merge.desc",
        "Combine multiple PDF documents into a single file.",
    ),
    ("tools.split", "Split / Extract Pages"),
    (
        "tools.split.desc",
        "Extract specific page ranges or split every page into separate files.",
    ),
    ("tools.delete_pages", "Delete Pages"),
    (
        "tools.delete_pages.desc",
        "Remove selected pages and create a new PDF document.",
    ),
    ("tools.rotate", "Rotate Pages"),
    (
        "tools.rotate.desc",
        "Rotate pages by 90°, 180°, or 270° and save the result.",
    ),
    ("tools.crop", "Crop Pages"),
    (
        "tools.crop.desc",
        "Crop margins or custom areas from pages.",
    ),
    ("tools.organizer", "Visual Organizer"),
    (
        "tools.organizer.desc",
        "Reorder, rotate, or delete pages visually.",
    ),
    ("tools.reorder", "Reorder Pages"),
    ("tools.btn.add_files", "Add Files…"),
    ("tools.btn.move_up", "Move Up"),
    ("tools.btn.move_down", "Move Down"),
    ("tools.btn.remove", "Remove"),
    ("tools.btn.clear", "Clear All"),
    ("tools.btn.cancel", "Cancel"),
    ("tools.btn.save", "Save Result…"),
    ("tools.btn.execute", "Process"),
    ("tools.label.pages", "Pages (e.g. 1-3, 5, 8-10):"),
    ("tools.label.split_mode", "Split Mode:"),
    (
        "tools.split_mode.extract",
        "Extract page range to single file",
    ),
    (
        "tools.split_mode.separate",
        "Split every page into separate files",
    ),
    ("tools.label.rotation", "Rotation:"),
    ("tools.rotation.90", "90° Clockwise"),
    ("tools.rotation.180", "180°"),
    ("tools.rotation.270", "90° Counter-Clockwise"),
    ("tools.convert", "Convert PDF"),
    (
        "tools.convert.desc",
        "Export pages or text in a format that suits your workflow.",
    ),
    ("tools.drop.merge", "Drop PDFs to add to the merge"),
    ("tools.drop.split", "Drop a PDF to split"),
    ("tools.drop.delete", "Drop a PDF to delete pages"),
    ("tools.drop.rotate", "Drop a PDF to rotate pages"),
    ("tools.drop.convert", "Drop a PDF to convert"),
    ("tools.pages.unit", "pages"),
    ("tools.pages.empty_hint", "empty for all pages"),
    ("tools.pages.select_all", "Select All"),
    ("tools.pages.clear_selection", "Clear"),
    ("tools.convert.all_pages", "All pages"),
    ("tools.convert.custom_range", "Custom pages"),
    ("tools.convert.btn_all", "Convert All Pages"),
    ("tools.range.split_placeholder", "1-3, 5, 8-10"),
    ("tools.range.delete_placeholder", "2, 4-6, 10"),
    ("tools.range.rotate_placeholder", "All pages or e.g. 1, 3-5"),
    ("tools.format", "Format"),
    ("tools.resolution", "Resolution"),
    ("tools.jpeg_quality", "JPEG quality"),
    (
        "tools.merge.drag_hint",
        "First page • drag or ↑ / ↓ to reorder",
    ),
    ("tools.merge.dragged_hint", "Release on a card to move"),
    ("tools.status.working", "Processing PDF operation…"),
    ("tools.status.success", "Operation completed successfully."),
    ("tools.status.failed", "Operation failed."),
    ("tools.error.busy", "A PDF tool is already running."),
    ("tools.error.drop_pdf", "Drop one or more PDF files."),
    (
        "tools.error.single_source",
        "Choose one PDF source for this tool.",
    ),
    ("tools.error.source", "Choose a PDF source first."),
    ("tools.error.format", "Choose a conversion format."),
    ("tools.error.resolution", "Choose 150 or 300 DPI."),
    (
        "tools.password.incorrect",
        "The password is incorrect. Try again.",
    ),
    ("tools.password.required", "This PDF is password protected."),
    (
        "tools.error.no_files",
        "Please select at least one PDF file.",
    ),
    ("tools.error.no_pages", "Please specify pages to process."),
    ("tools.error.invalid_range", "Invalid page range entered."),
    ("tools.prompt.open_result", "Open in New Tab"),
];

const TURKISH_TOOLS: &[(&str, &str)] = &[
    ("tools.title", "PDF Araçları"),
    ("tools.tooltip", "PDF İşlemleri ve Araçları"),
    ("tools.merge", "PDF Birleştir"),
    (
        "tools.merge.desc",
        "Birden fazla PDF belgesini tek bir dosyada birleştirin.",
    ),
    ("tools.split", "Sayfa Ayıkla / Böl"),
    (
        "tools.split.desc",
        "Belirli sayfa aralıklarını ayıklayın veya tüm sayfaları ayrı dosyalara bölün.",
    ),
    ("tools.delete_pages", "Sayfa Sil"),
    (
        "tools.delete_pages.desc",
        "Seçilen sayfaları çıkarıp yeni bir PDF belgesi oluşturun.",
    ),
    ("tools.rotate", "Sayfa Döndür"),
    (
        "tools.rotate.desc",
        "Sayfaları 90°, 180° veya 270° döndürüp kaydedin.",
    ),
    ("tools.crop", "Sayfaları Kırp"),
    (
        "tools.crop.desc",
        "Sayfalardan kenar boşluklarını veya özel alanları kırpın.",
    ),
    ("tools.organizer", "Sayfa Düzenleyici"),
    (
        "tools.organizer.desc",
        "Sayfaları görsel olarak yeniden sıralayın, döndürün veya silin.",
    ),
    ("tools.reorder", "Sayfa Düzenleyici"),
    ("tools.btn.add_files", "Dosya Ekle…"),
    ("tools.btn.move_up", "Yukarı Taşı"),
    ("tools.btn.move_down", "Aşağı Taşı"),
    ("tools.btn.remove", "Kaldır"),
    ("tools.btn.clear", "Tümünü Temizle"),
    ("tools.btn.cancel", "İptal"),
    ("tools.btn.save", "Sonucu Kaydet…"),
    ("tools.btn.execute", "İşlemi Uygula"),
    ("tools.label.pages", "Sayfalar (örn. 1-3, 5, 8-10):"),
    ("tools.label.split_mode", "Bölme Modu:"),
    (
        "tools.split_mode.extract",
        "Sayfa aralığını tek dosyaya çıkart",
    ),
    (
        "tools.split_mode.separate",
        "Tüm sayfaları ayrı dosyalara böl",
    ),
    ("tools.label.rotation", "Döndürme Açısı:"),
    ("tools.rotation.90", "90° Saat Yönünde"),
    ("tools.rotation.180", "180°"),
    ("tools.rotation.270", "90° Saatin Tersi Yönünde"),
    ("tools.convert", "PDF Dönüştür"),
    (
        "tools.convert.desc",
        "Sayfaları veya metni iş akışınıza uygun bir biçimde dışa aktarın.",
    ),
    (
        "tools.drop.merge",
        "Birleştirmeye eklemek için PDF'leri bırakın",
    ),
    ("tools.drop.split", "Bölmek için bir PDF bırakın"),
    ("tools.drop.delete", "Sayfaları silmek için bir PDF bırakın"),
    (
        "tools.drop.rotate",
        "Sayfaları döndürmek için bir PDF bırakın",
    ),
    ("tools.drop.convert", "Dönüştürmek için bir PDF bırakın"),
    ("tools.pages.unit", "sayfa"),
    ("tools.pages.empty_hint", "tüm sayfalar için boş bırakın"),
    ("tools.pages.select_all", "Tümünü Seç"),
    ("tools.pages.clear_selection", "Temizle"),
    ("tools.convert.all_pages", "Tüm sayfalar"),
    ("tools.convert.custom_range", "Özel sayfalar"),
    ("tools.convert.btn_all", "Tüm Sayfaları Dönüştür"),
    ("tools.range.split_placeholder", "1-3, 5, 8-10"),
    ("tools.range.delete_placeholder", "2, 4-6, 10"),
    (
        "tools.range.rotate_placeholder",
        "Tüm sayfalar veya örn. 1, 3-5",
    ),
    ("tools.format", "Biçim"),
    ("tools.resolution", "Çözünürlük"),
    ("tools.jpeg_quality", "JPEG kalitesi"),
    (
        "tools.merge.drag_hint",
        "İlk sayfa • sürükleyip bırakarak yeniden sıralayın",
    ),
    (
        "tools.merge.dragged_hint",
        "Taşımak için bir kartın üzerine bırakın",
    ),
    ("tools.status.working", "PDF işlemi gerçekleştiriliyor…"),
    ("tools.status.success", "İşlem başarıyla tamamlandı."),
    ("tools.status.failed", "İşlem başarısız oldu."),
    ("tools.error.busy", "Bir PDF aracı zaten çalışıyor."),
    ("tools.error.drop_pdf", "Bir veya daha fazla PDF bırakın."),
    (
        "tools.error.single_source",
        "Bu araç için tek bir PDF kaynağı seçin.",
    ),
    ("tools.error.source", "Önce bir PDF kaynağı seçin."),
    ("tools.error.format", "Bir dönüştürme biçimi seçin."),
    ("tools.error.resolution", "150 veya 300 DPI seçin."),
    ("tools.password.incorrect", "Parola yanlış. Tekrar deneyin."),
    ("tools.password.required", "Bu PDF parola korumalı."),
    (
        "tools.error.no_files",
        "Lütfen en az bir PDF dosyası seçin.",
    ),
    (
        "tools.error.no_pages",
        "Lütfen işlenecek sayfaları belirtin.",
    ),
    (
        "tools.error.invalid_range",
        "Geçersiz sayfa aralığı girildi.",
    ),
    ("tools.prompt.open_result", "Yeni Sekmede Aç"),
];

const ENGLISH_ANNOTATIONS: &[(&str, &str)] = &[
    ("toolbar.rotate", "Rotate (Ctrl+R)"),
    ("toolbar.draw", "Draw"),
    ("toolbar.sign", "Sign"),
    ("toolbar.text", "Text"),
    ("toolbar.typewriter", "Typewriter / Text"),
    ("toolbar.crop", "Crop Pages"),
    ("toolbar.organizer", "Visual Organizer"),
    ("draw.text", "Typewriter / Text"),
    ("draw.typewriter", "Typewriter / Text"),
    ("annotation.typewriter", "Typewriter / Text"),
    ("context.find", "Find in Document"),
    ("context.highlight", "Highlight"),
    ("context.rotate_cw", "Rotate Clockwise"),
    ("context.fit_page", "Fit Page"),
    ("draw.pen", "Pen"),
    ("draw.eraser", "Eraser"),
    ("draw.undo", "Undo"),
    ("draw.clear", "Clear"),
    ("draw.save", "Save"),
    ("draw.save_as", "Save As…"),
    ("draw.discard", "Discard"),
    ("sign.title", "Add Signature"),
    ("sign.draw_tab", "Draw Signature"),
    ("sign.image_tab", "Upload Image"),
    ("sign.pick_image", "Choose Image (PNG/JPG)…"),
    ("sign.clear", "Clear"),
    ("sign.place", "Place on Page"),
    ("sign.apply", "Apply Signature"),
    ("sign.cancel", "Cancel"),
    ("status.saved", "Saved changes to {name}"),
];

const TURKISH_ANNOTATIONS: &[(&str, &str)] = &[
    ("toolbar.rotate", "Döndür (Ctrl+R)"),
    ("toolbar.draw", "Çizim"),
    ("toolbar.sign", "İmzala"),
    ("toolbar.text", "Metin Ekle"),
    ("toolbar.typewriter", "Metin Ekle"),
    ("toolbar.crop", "Sayfaları Kırp"),
    ("toolbar.organizer", "Sayfa Düzenleyici"),
    ("draw.text", "Metin Ekle"),
    ("draw.typewriter", "Metin Ekle"),
    ("annotation.typewriter", "Metin Ekle"),
    ("context.find", "Belgede Bul"),
    ("context.highlight", "Vurgula"),
    ("context.rotate_cw", "Saat Yönünde Döndür"),
    ("context.fit_page", "Sayfaya Sığdır"),
    ("draw.pen", "Kalem"),
    ("draw.eraser", "Silgi"),
    ("draw.undo", "Geri Al"),
    ("draw.clear", "Temizle"),
    ("draw.save", "Kaydet"),
    ("draw.save_as", "Farklı Kaydet…"),
    ("draw.discard", "Vazgeç"),
    ("sign.title", "İmza Ekle"),
    ("sign.draw_tab", "İmza Çiz"),
    ("sign.image_tab", "Görsel Yükle"),
    ("sign.pick_image", "Görsel Seç (PNG/JPG)…"),
    ("sign.clear", "Temizle"),
    ("sign.place", "Sayfaya Yerleştir"),
    ("sign.apply", "İmzayı Uygula"),
    ("sign.cancel", "İptal"),
    ("status.saved", "Değişiklikler kaydedildi: {name}"),
];

const ENGLISH_TABLES: &[&[(&str, &str)]] = &[
    ENGLISH_GENERAL,
    ENGLISH_PRINT,
    ENGLISH_TOOLS,
    ENGLISH_ANNOTATIONS,
];

const TURKISH_TABLES: &[&[(&str, &str)]] = &[
    TURKISH_GENERAL,
    TURKISH_PRINT,
    TURKISH_TOOLS,
    TURKISH_ANNOTATIONS,
];

fn lookup_tables(tables: &[&[(&'static str, &'static str)]], key: &str) -> Option<&'static str> {
    for table in tables {
        for &(k, v) in *table {
            if k == key {
                return Some(v);
            }
        }
    }
    None
}

#[must_use]
pub fn t(lang: ResolvedLanguage, key: &str) -> &'static str {
    let tables = match lang {
        ResolvedLanguage::English => ENGLISH_TABLES,
        ResolvedLanguage::Turkish => TURKISH_TABLES,
    };

    lookup_tables(tables, key)
        .or_else(|| lookup_tables(ENGLISH_TABLES, key))
        .unwrap_or("")
}

#[must_use]
pub fn detect_system_language() -> ResolvedLanguage {
    #[cfg(target_os = "windows")]
    {
        let lang_id = windows_ffi::user_default_ui_language();
        let primary_lang = lang_id & 0x3FF; // LANGID primary language bits
        if primary_lang == 0x1F {
            // LANG_TURKISH
            return ResolvedLanguage::Turkish;
        }
    }
    ResolvedLanguage::English
}

#[cfg(target_os = "windows")]
mod windows_ffi {
    use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;

    pub(super) fn user_default_ui_language() -> u16 {
        // SAFETY: GetUserDefaultUILanguage takes no pointers and returns a LANGID by value. Windows
        // imposes no thread-affinity or lifetime requirement, so no borrowed memory crosses FFI.
        unsafe { GetUserDefaultUILanguage() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_completeness() {
        for table in ENGLISH_TABLES {
            for &(key, _) in *table {
                assert!(
                    lookup_tables(TURKISH_TABLES, key).is_some(),
                    "Missing Turkish translation for key: {key}"
                );
            }
        }
        for table in TURKISH_TABLES {
            for &(key, _) in *table {
                assert!(
                    lookup_tables(ENGLISH_TABLES, key).is_some(),
                    "Missing English translation for key: {key}"
                );
            }
        }
    }

    #[test]
    fn test_language_metadata_and_resolution() {
        assert_eq!(Language::System.code(), "system");
        assert_eq!(Language::English.code(), "en");
        assert_eq!(Language::Turkish.code(), "tr");
        assert_eq!(Language::System.display_name(), "System Default");
        assert_eq!(Language::English.display_name(), "English");
        assert_eq!(Language::Turkish.display_name(), "Türkçe");
        assert_eq!(Language::English.resolve().code(), "en");
        assert_eq!(Language::Turkish.resolve().code(), "tr");
    }

    #[test]
    fn test_translation_lookups() {
        assert_eq!(t(ResolvedLanguage::English, "open.file"), "Open PDF");
        assert_eq!(t(ResolvedLanguage::Turkish, "open.file"), "PDF Aç");
        assert_eq!(
            t(ResolvedLanguage::English, "sidebar.thumbnails"),
            "Thumbnails"
        );
        assert_eq!(
            t(ResolvedLanguage::Turkish, "sidebar.thumbnails"),
            "Sayfalar"
        );
        assert_eq!(t(ResolvedLanguage::Turkish, "tab.new"), "Yeni sekme");
        assert_eq!(t(ResolvedLanguage::Turkish, "print.action"), "Yazdır");
        assert_eq!(t(ResolvedLanguage::Turkish, "page.thumbnail"), "Sayfa");
        assert_eq!(t(ResolvedLanguage::English, "tools.title"), "PDF Tools");
        assert_eq!(t(ResolvedLanguage::Turkish, "tools.title"), "PDF Araçları");
        assert_eq!(t(ResolvedLanguage::English, "tools.merge"), "Merge PDFs");
        assert_eq!(t(ResolvedLanguage::Turkish, "tools.merge"), "PDF Birleştir");
        assert_eq!(t(ResolvedLanguage::Turkish, "nonexistent"), "");
    }

    #[test]
    fn annotation_and_context_strings_are_localized() {
        let pairs = [
            ("toolbar.rotate", "Rotate (Ctrl+R)", "Döndür (Ctrl+R)"),
            ("toolbar.draw", "Draw", "Çizim"),
            ("toolbar.sign", "Sign", "İmzala"),
            ("context.find", "Find in Document", "Belgede Bul"),
            ("context.highlight", "Highlight", "Vurgula"),
            (
                "context.rotate_cw",
                "Rotate Clockwise",
                "Saat Yönünde Döndür",
            ),
            ("context.fit_page", "Fit Page", "Sayfaya Sığdır"),
            ("draw.pen", "Pen", "Kalem"),
            ("draw.eraser", "Eraser", "Silgi"),
            ("draw.undo", "Undo", "Geri Al"),
            ("draw.clear", "Clear", "Temizle"),
            ("draw.save", "Save", "Kaydet"),
            ("draw.save_as", "Save As…", "Farklı Kaydet…"),
            ("draw.discard", "Discard", "Vazgeç"),
            ("sign.title", "Add Signature", "İmza Ekle"),
            ("sign.draw_tab", "Draw Signature", "İmza Çiz"),
            ("sign.image_tab", "Upload Image", "Görsel Yükle"),
            (
                "sign.pick_image",
                "Choose Image (PNG/JPG)…",
                "Görsel Seç (PNG/JPG)…",
            ),
            ("sign.clear", "Clear", "Temizle"),
            ("sign.place", "Place on Page", "Sayfaya Yerleştir"),
            ("sign.apply", "Apply Signature", "İmzayı Uygula"),
            ("sign.cancel", "Cancel", "İptal"),
            (
                "status.saved",
                "Saved changes to {name}",
                "Değişiklikler kaydedildi: {name}",
            ),
        ];

        for (key, en, tr) in pairs {
            assert_eq!(t(ResolvedLanguage::English, key), en);
            assert_eq!(t(ResolvedLanguage::Turkish, key), tr);
        }
    }

    #[test]
    fn invert_colors_strings_are_localized() {
        assert_eq!(
            t(
                ResolvedLanguage::English,
                "command_palette_invert_colors_title"
            ),
            "Toggle Inverted Page Colors (Ctrl+I)"
        );
        assert_eq!(
            t(
                ResolvedLanguage::Turkish,
                "command_palette_invert_colors_title"
            ),
            "Sayfa Renklerini Ters Çevir (Ctrl+I)"
        );
        assert_eq!(
            t(
                ResolvedLanguage::English,
                "command_palette_invert_colors_desc"
            ),
            "High contrast inverted reading mode"
        );
        assert_eq!(
            t(
                ResolvedLanguage::Turkish,
                "command_palette_invert_colors_desc"
            ),
            "Yüksek kontrastlı ters çevrilmiş okuma modu"
        );
        assert_eq!(
            t(ResolvedLanguage::English, "settings_invert_colors"),
            "Invert page colors"
        );
        assert_eq!(
            t(ResolvedLanguage::Turkish, "settings_invert_colors"),
            "Sayfa renklerini ters çevir"
        );
    }

    #[test]
    fn print_preview_and_conversion_strings_are_localized() {
        let keys = [
            "toolbar.more",
            "print.preview.title",
            "print.preview.cancel_tooltip",
            "print.preview.empty",
            "print.preview.page",
            "print.preview.page_range",
            "print.preview.range_placeholder",
            "print.preview.range_accessible",
            "print.preview.orientation",
            "print.preview.orientation.auto",
            "print.preview.orientation.portrait",
            "print.preview.orientation.landscape",
            "print.preview.continue",
            "settings.developer",
            "settings.project_website",
            "settings.manifesto",
            "settings.manifesto.button",
            "settings.about",
            "tools.convert",
            "tools.convert.desc",
            "tools.drop.merge",
            "tools.drop.split",
            "tools.drop.delete",
            "tools.drop.rotate",
            "tools.drop.convert",
            "tools.pages.unit",
            "tools.pages.empty_hint",
            "tools.pages.select_all",
            "tools.pages.clear_selection",
            "tools.convert.all_pages",
            "tools.convert.custom_range",
            "tools.convert.btn_all",
            "tools.range.split_placeholder",
            "tools.range.delete_placeholder",
            "tools.range.rotate_placeholder",
            "tools.format",
            "tools.resolution",
            "tools.jpeg_quality",
            "tools.merge.drag_hint",
            "tools.merge.dragged_hint",
        ];

        for key in keys {
            assert!(
                !t(ResolvedLanguage::English, key).is_empty(),
                "Missing English translation for key: {key}"
            );
            assert!(
                !t(ResolvedLanguage::Turkish, key).is_empty(),
                "Missing Turkish translation for key: {key}"
            );
        }

        let spot_checks = [
            (
                "print.preview.range_placeholder",
                "All pages or 1-3",
                "Tüm sayfalar veya 1-3",
            ),
            ("settings.developer", "Developer", "Geliştirici"),
            ("settings.project_website", "Website", "Web Sitesi"),
            ("settings.about", "About", "Hakkında"),
            ("tools.pages.select_all", "Select All", "Tümünü Seç"),
            ("tools.pages.clear_selection", "Clear", "Temizle"),
            ("tools.convert.all_pages", "All pages", "Tüm sayfalar"),
            (
                "tools.convert.custom_range",
                "Custom pages",
                "Özel sayfalar",
            ),
            (
                "tools.convert.btn_all",
                "Convert All Pages",
                "Tüm Sayfaları Dönüştür",
            ),
        ];

        for (key, en, tr) in spot_checks {
            assert_eq!(t(ResolvedLanguage::English, key), en);
            assert_eq!(t(ResolvedLanguage::Turkish, key), tr);
        }
    }

    #[test]
    fn window_and_toolbar_strings_are_localized() {
        let pairs = [
            ("window.minimize", "Minimize", "Simge durumuna küçült"),
            ("window.maximize", "Maximize", "Ekranı kapla"),
            ("window.restore", "Restore", "Aşağı geri getir"),
            ("window.close", "Close", "Kapat"),
            ("toolbar.fit", "Fit", "Sığdır"),
            ("toolbar.search", "Find (Ctrl+F)", "Bul (Ctrl+F)"),
            (
                "toolbar.command_palette",
                "Command Palette (Ctrl+K)",
                "Komut Paleti (Ctrl+K)",
            ),
        ];

        for (key, en, tr) in pairs {
            assert_eq!(t(ResolvedLanguage::English, key), en);
            assert_eq!(t(ResolvedLanguage::Turkish, key), tr);
        }

        for removed_key in [
            "settings.effects",
            "settings.efficient",
            "settings.enhanced",
            "settings.effects.help",
        ] {
            assert_eq!(t(ResolvedLanguage::English, removed_key), "");
            assert_eq!(t(ResolvedLanguage::Turkish, removed_key), "");
        }
    }

    #[test]
    fn wave_0_strings_are_localized() {
        let cases = [
            ("view.mode.book", "Book View", "Kitap Görünümü"),
            ("toolbar.book", "Book View", "Kitap Görünümü"),
            ("tools.crop", "Crop Pages", "Sayfaları Kırp"),
            ("toolbar.crop", "Crop Pages", "Sayfaları Kırp"),
            ("tools.organizer", "Visual Organizer", "Sayfa Düzenleyici"),
            ("toolbar.organizer", "Visual Organizer", "Sayfa Düzenleyici"),
            ("tools.reorder", "Reorder Pages", "Sayfa Düzenleyici"),
            ("toolbar.typewriter", "Typewriter / Text", "Metin Ekle"),
            ("draw.text", "Typewriter / Text", "Metin Ekle"),
            ("status.reloaded", "Reloaded document", "Belge güncellendi"),
            (
                "document.reloaded",
                "Reloaded document",
                "Belge güncellendi",
            ),
        ];

        for (key, en, tr) in cases {
            assert_eq!(
                t(ResolvedLanguage::English, key),
                en,
                "English mismatch for {key}"
            );
            assert_eq!(
                t(ResolvedLanguage::Turkish, key),
                tr,
                "Turkish mismatch for {key}"
            );
        }
    }
}
