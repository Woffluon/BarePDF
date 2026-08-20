use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    System,
    English,
    Turkish,
}

impl Language {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Language::System => "system",
            Language::English => "en",
            Language::Turkish => "tr",
        }
    }

    #[must_use]
    pub fn display_name(&self) -> &'static str {
        match self {
            Language::System => "System Default",
            Language::English => "English",
            Language::Turkish => "Türkçe",
        }
    }

    #[must_use]
    pub fn resolve(&self) -> ResolvedLanguage {
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
    pub fn code(&self) -> &'static str {
        match self {
            ResolvedLanguage::English => "en",
            ResolvedLanguage::Turkish => "tr",
        }
    }
}

static ENGLISH_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static TURKISH_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();

fn init_english() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("app.title", "BarePDF");
    m.insert("open.file", "Open PDF");
    m.insert("open.file.tooltip", "Open PDF Document (Ctrl+O)");
    m.insert("sidebar.toggle", "Sidebar");
    m.insert("sidebar.thumbnails", "Thumbnails");
    m.insert("sidebar.outline", "Outline");
    m.insert("tab.new", "New tab");
    m.insert("view.mode", "View");
    m.insert("view.mode.continuous", "Continuous");
    m.insert("view.mode.single", "Single Page");
    m.insert("view.mode.two_page", "Two Pages");
    m.insert("view.mode.book", "Book View");
    m.insert("zoom.in", "Zoom In");
    m.insert("zoom.out", "Zoom Out");
    m.insert("zoom.fit_width", "Fit Width");
    m.insert("zoom.fit_page", "Fit Page");
    m.insert("zoom.actual_size", "Actual Size");
    m.insert("fullscreen", "Full Screen");
    m.insert("presentation", "Presentation");
    m.insert("settings", "Settings");
    m.insert("settings.title", "Preferences");
    m.insert("settings.language", "Language");
    m.insert("settings.theme", "Theme");
    m.insert("settings.theme.system", "System");
    m.insert("settings.theme.light", "Light");
    m.insert("settings.theme.dark", "Dark");
    m.insert("language.english", "English");
    m.insert("language.turkish", "Türkçe");
    m.insert("settings.view_mode", "Default View Mode");
    m.insert("settings.reading_dir", "Reading Direction");
    m.insert("settings.reading_dir.ltr", "Left to Right (LTR)");
    m.insert("settings.reading_dir.rtl", "Right to Left (RTL)");
    m.insert("settings.close", "Close");
    m.insert("updates", "Updates");
    m.insert("updates.enabled", "Enabled");
    m.insert("updates.disabled", "Disabled");
    m.insert("updates.check_now", "Check now");
    m.insert("updates.consent.title", "BarePDF updates");
    m.insert(
        "updates.consent.body",
        "Allow BarePDF to check GitHub for updates? If enabled, it checks at most once every 24 hours. You can change this later in Settings.",
    );
    m.insert("updates.status.ready", "Ready to check for updates.");
    m.insert("updates.status.checking", "Checking for updates...");
    m.insert("updates.status.current", "BarePDF is up to date.");
    m.insert(
        "updates.status.available",
        "A new BarePDF version is available.",
    );
    m.insert(
        "updates.status.downloading",
        "Downloading and verifying the update...",
    );
    m.insert("updates.status.installing", "Starting the installer...");
    m.insert(
        "updates.status.verified",
        "Update verified and ready to install.",
    );
    m.insert("updates.status.error", "The update could not be completed.");
    m.insert("updates.action.download", "Download update");
    m.insert("updates.action.install", "Install update");
    m.insert("updates.action.release", "View release");
    m.insert("password.title", "Password Required");
    m.insert(
        "password.desc",
        "This document is encrypted. Enter password to open:",
    );
    m.insert("password.placeholder", "Password");
    m.insert("password.unlock", "Unlock");
    m.insert("password.cancel", "Cancel");
    m.insert("password.error.too_long", "Password is too long.");
    m.insert("page.previous", "Previous page");
    m.insert("page.next", "Next page");
    m.insert("page.thumbnail", "Page");
    add_english_print_strings(&mut m);
    m.insert("context.copy", "Copy");
    m.insert("context.select_all", "Select All");
    m.insert("status.ready", "Ready");
    m.insert("status.opening", "Opening document...");
    m.insert("status.opened", "{name} ({pages} pages)");
    m.insert("status.error", "Error: {error}");
    m.insert("empty.title", "No Document Loaded");
    m.insert(
        "empty.desc",
        "Click 'Open PDF' or drag a PDF file here to begin reading.",
    );
    m.insert("outline.empty", "This document has no outline.");
    m.insert("recent.title", "Recent files");
    m.insert("action.retry", "Retry");
    m.insert("action.dismiss", "Dismiss");
    m.insert("status.loading", "Loading");
    m
}

fn init_turkish() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("app.title", "BarePDF");
    m.insert("open.file", "PDF Aç");
    m.insert("open.file.tooltip", "PDF Belgesi Aç (Ctrl+O)");
    m.insert("sidebar.toggle", "Kenar Çubuğu");
    m.insert("sidebar.thumbnails", "Küçük Resimler");
    m.insert("sidebar.outline", "İçindekiler");
    m.insert("tab.new", "Yeni sekme");
    m.insert("view.mode", "Görünüm");
    m.insert("view.mode.continuous", "Sürekli");
    m.insert("view.mode.single", "Tek Sayfa");
    m.insert("view.mode.two_page", "Çift Sayfa");
    m.insert("view.mode.book", "Kitap Görünümü");
    m.insert("zoom.in", "Yakınlaştır");
    m.insert("zoom.out", "Uzaklaştır");
    m.insert("zoom.fit_width", "Genişliğe Sığdır");
    m.insert("zoom.fit_page", "Sayfaya Sığdır");
    m.insert("zoom.actual_size", "Gerçek Boyut");
    m.insert("fullscreen", "Tam Ekran");
    m.insert("presentation", "Sunum");
    m.insert("settings", "Ayarlar");
    m.insert("settings.title", "Tercihler");
    m.insert("settings.language", "Dil");
    m.insert("settings.theme", "Tema");
    m.insert("settings.theme.system", "Sistem");
    m.insert("settings.theme.light", "Açık");
    m.insert("settings.theme.dark", "Koyu");
    m.insert("language.english", "İngilizce");
    m.insert("language.turkish", "Türkçe");
    m.insert("settings.view_mode", "Varsayılan Görünüm Mode");
    m.insert("settings.reading_dir", "Okuma Yönü");
    m.insert("settings.reading_dir.ltr", "Soldan Sağa (LTR)");
    m.insert("settings.reading_dir.rtl", "Sağdan Sola (RTL)");
    m.insert("settings.close", "Kapat");
    m.insert("updates", "Güncellemeler");
    m.insert("updates.enabled", "Açık");
    m.insert("updates.disabled", "Kapalı");
    m.insert("updates.check_now", "Şimdi denetle");
    m.insert("updates.consent.title", "BarePDF güncellemeleri");
    m.insert(
        "updates.consent.body",
        "BarePDF'in GitHub üzerinden güncellemeleri denetlemesine izin verilsin mi? Etkinleştirilirse en fazla 24 saatte bir denetler. Bunu daha sonra Ayarlar'dan değiştirebilirsiniz.",
    );
    m.insert("updates.status.ready", "Güncelleme denetimine hazır.");
    m.insert("updates.status.checking", "Güncellemeler denetleniyor...");
    m.insert("updates.status.current", "BarePDF güncel.");
    m.insert(
        "updates.status.available",
        "Yeni bir BarePDF sürümü mevcut.",
    );
    m.insert(
        "updates.status.downloading",
        "Güncelleme indiriliyor ve doğrulanıyor...",
    );
    m.insert("updates.status.installing", "Kurulum başlatılıyor...");
    m.insert(
        "updates.status.verified",
        "Güncelleme doğrulandı ve kuruluma hazır.",
    );
    m.insert("updates.status.error", "Güncelleme tamamlanamadı.");
    m.insert("updates.action.download", "Güncellemeyi indir");
    m.insert("updates.action.install", "Güncellemeyi kur");
    m.insert("updates.action.release", "Sürümü görüntüle");
    m.insert("password.title", "Parola Gerekli");
    m.insert(
        "password.desc",
        "Bu belge şifrelenmiş. Açmak için parolayı girin:",
    );
    m.insert("password.placeholder", "Parola");
    m.insert("password.unlock", "Kilidi Aç");
    m.insert("password.cancel", "İptal");
    m.insert("password.error.too_long", "Parola çok uzun.");
    m.insert("page.previous", "Önceki sayfa");
    m.insert("page.next", "Sonraki sayfa");
    m.insert("page.thumbnail", "Sayfa");
    add_turkish_print_strings(&mut m);
    m.insert("context.copy", "Kopyala");
    m.insert("context.select_all", "Tümünü Seç");
    m.insert("status.ready", "Hazır");
    m.insert("status.opening", "Belge açılıyor...");
    m.insert("status.opened", "{name} ({pages} sayfa)");
    m.insert("status.error", "Hata: {error}");
    m.insert("empty.title", "Yüklü Belge Yok");
    m.insert("empty.desc", "Okumaya başlamak için 'PDF Aç' seçeneğine tıklayın veya bir PDF dosyasını buraya sürükleyin.");
    m.insert("outline.empty", "Bu belgede içindekiler bulunmuyor.");
    m.insert("recent.title", "Son dosyalar");
    m.insert("action.retry", "Tekrar dene");
    m.insert("action.dismiss", "Kapat");
    m.insert("status.loading", "Yükleniyor");
    m
}

fn add_english_print_strings(m: &mut HashMap<&'static str, &'static str>) {
    m.extend([
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
        ("print.status.preparing", "Preparing print job…"),
        ("print.status.cancelling", "Cancelling print job…"),
        ("print.status.progress", "Printing page"),
        ("print.status.complete", "Printing complete."),
        ("print.status.cancelled", "Printing cancelled."),
        ("print.status.failed", "Printing failed."),
    ]);
}

fn add_turkish_print_strings(m: &mut HashMap<&'static str, &'static str>) {
    m.extend([
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
        ("print.status.preparing", "Yazdırma işi hazırlanıyor…"),
        ("print.status.cancelling", "Yazdırma işi iptal ediliyor…"),
        ("print.status.progress", "Sayfa yazdırılıyor"),
        ("print.status.complete", "Yazdırma tamamlandı."),
        ("print.status.cancelled", "Yazdırma iptal edildi."),
        ("print.status.failed", "Yazdırma başarısız oldu."),
    ]);
}

pub fn t(lang: ResolvedLanguage, key: &str) -> &'static str {
    let map = match lang {
        ResolvedLanguage::English => ENGLISH_MAP.get_or_init(init_english),
        ResolvedLanguage::Turkish => TURKISH_MAP.get_or_init(init_turkish),
    };

    if let Some(&val) = map.get(key) {
        val
    } else if let Some(&en_val) = ENGLISH_MAP.get_or_init(init_english).get(key) {
        en_val
    } else {
        ""
    }
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
        let en_map = ENGLISH_MAP.get_or_init(init_english);
        let tr_map = TURKISH_MAP.get_or_init(init_turkish);

        for key in en_map.keys() {
            assert!(
                tr_map.contains_key(key),
                "Missing Turkish translation for key: {key}"
            );
        }
    }

    #[test]
    fn test_translation_lookups() {
        assert_eq!(t(ResolvedLanguage::English, "open.file"), "Open PDF");
        assert_eq!(t(ResolvedLanguage::Turkish, "open.file"), "PDF Aç");
        assert_eq!(t(ResolvedLanguage::Turkish, "tab.new"), "Yeni sekme");
        assert_eq!(t(ResolvedLanguage::Turkish, "print.action"), "Yazdır");
        assert_eq!(t(ResolvedLanguage::Turkish, "page.thumbnail"), "Sayfa");
        assert_eq!(t(ResolvedLanguage::Turkish, "nonexistent"), "");
    }
}
